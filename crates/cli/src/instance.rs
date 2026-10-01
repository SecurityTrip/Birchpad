//! Single instance: a second launch hands its command line to the running instance and exits.
//!
//! The running instance listens on a local socket: a named pipe on Windows, a Unix domain socket
//! in a per-user directory elsewhere. The name includes a hash of the data directory, so a
//! portable copy and an installed copy do not hand files to each other. If anything about the
//! socket fails, Birchpad simply starts another instance: single instance is a convenience,
//! never a reason not to start.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::Duration;

use interprocess::local_socket::prelude::*;
use interprocess::local_socket::{
    GenericFilePath, GenericNamespaced, ListenerOptions, Name, Stream,
};

use crate::args::CommandLine;

/// How long a second launch waits for the running instance to accept its files.
const HAND_OFF_TIMEOUT: Duration = Duration::from_secs(3);
/// Upper bound for one message, so a broken client cannot exhaust memory.
const MAX_MESSAGE: u64 = 1024 * 1024;

/// Where instances of one installation meet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Address {
    /// A named pipe (Windows) or abstract socket name.
    Namespaced(String),
    /// A socket file (Unix).
    File(PathBuf),
}

impl Address {
    /// The address for the current user and the installation using `data_dir`.
    ///
    /// On Unix the socket goes into `$XDG_RUNTIME_DIR` (private to the user) when set, else
    /// into `data_dir`.
    pub fn for_installation(data_dir: &Path) -> Self {
        let scope = fnv1a(data_dir.to_string_lossy().as_bytes());
        if cfg!(windows) {
            let user = std::env::var("USERNAME").unwrap_or_default();
            Self::Namespaced(format!("birchpad-{user}-{scope:016x}"))
        } else {
            let dir = std::env::var_os("XDG_RUNTIME_DIR")
                .filter(|dir| !dir.is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(|| data_dir.to_owned());
            Self::File(dir.join(format!("birchpad-{scope:016x}.sock")))
        }
    }

    fn name(&self) -> io::Result<Name<'static>> {
        match self {
            Self::Namespaced(name) => name.clone().to_ns_name::<GenericNamespaced>(),
            Self::File(path) => path.clone().to_fs_name::<GenericFilePath>(),
        }
    }
}

/// The outcome of [`start`].
pub enum Instance {
    /// No other instance runs: this one listens for later launches.
    Primary(Server),
    /// The command line was handed to the running instance; this process should exit.
    Secondary,
    /// The socket is unavailable; run on our own.
    Standalone(io::Error),
}

/// Hands `command_line` to a running instance, or becomes the instance others hand off to.
pub fn start(address: &Address, command_line: &CommandLine) -> Instance {
    match hand_off(address, command_line) {
        Ok(()) => return Instance::Secondary,
        // Nobody listens: become the primary instance.
        Err(error) if is_unoccupied(&error) => {}
        // Somebody listens but does not answer (hung, or busy with a broken client): do not
        // take its socket away, just run separately.
        Err(error) => return Instance::Standalone(error),
    }
    let name = match address.name() {
        Ok(name) => name,
        Err(error) => return Instance::Standalone(error),
    };
    if let Address::File(path) = address
        && let Some(dir) = path.parent()
        && let Err(error) = std::fs::create_dir_all(dir)
    {
        return Instance::Standalone(error);
    }
    // Nobody answered, so a socket file left by a crash is stale and may be replaced.
    match ListenerOptions::new()
        .name(name)
        .try_overwrite(true)
        .create_sync()
    {
        Ok(listener) => Instance::Primary(Server { listener }),
        Err(error) => {
            // Another instance may have won a race to listen; try it once more.
            if hand_off(address, command_line).is_ok() {
                Instance::Secondary
            } else {
                Instance::Standalone(error)
            }
        }
    }
}

/// Whether a failed connection means that no instance listens at the address.
fn is_unoccupied(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
    )
}

/// Sends the command line and waits for the running instance to confirm it.
pub fn hand_off(address: &Address, command_line: &CommandLine) -> io::Result<()> {
    let name = address.name()?;
    let mut message = serde_json::to_vec(command_line).map_err(io::Error::other)?;
    message.push(b'\n');
    let (done, result) = mpsc::channel();
    // Blocking socket calls have no portable timeout: talk on a thread and stop waiting for it
    // if the running instance does not answer.
    thread::spawn(move || {
        let outcome = (|| {
            let mut stream = BufReader::new(Stream::connect(name)?);
            stream.get_mut().write_all(&message)?;
            let mut answer = String::new();
            stream.read_line(&mut answer)?;
            if answer.trim() == "ok" {
                Ok(())
            } else {
                Err(io::Error::other(format!("unexpected answer {answer:?}")))
            }
        })();
        let _ = done.send(outcome);
    });
    result
        .recv_timeout(HAND_OFF_TIMEOUT)
        .unwrap_or_else(|_| Err(io::Error::from(io::ErrorKind::TimedOut)))
}

/// Accepts command lines from later launches.
pub struct Server {
    listener: interprocess::local_socket::Listener,
}

impl Server {
    /// Serves on a background thread, calling `on_request` for every command line received.
    /// Each connection is read on its own thread, so a client that never finishes its message
    /// does not block later launches.
    pub fn serve(self, on_request: impl Fn(CommandLine) + Send + Sync + 'static) {
        let on_request = Arc::new(on_request);
        thread::spawn(move || {
            for connection in self.listener.incoming() {
                let Ok(connection) = connection else {
                    continue;
                };
                let on_request = on_request.clone();
                thread::spawn(move || {
                    if let Ok(request) = receive(connection) {
                        on_request(request);
                    }
                });
            }
        });
    }
}

fn receive(connection: Stream) -> io::Result<CommandLine> {
    let mut reader = BufReader::new(connection);
    let mut line = String::new();
    (&mut reader).take(MAX_MESSAGE).read_line(&mut line)?;
    let request: CommandLine = serde_json::from_str(&line).map_err(io::Error::other)?;
    reader.get_mut().write_all(b"ok\n")?;
    Ok(request)
}

/// 64-bit FNV-1a: a stable, dependency-free hash for socket names.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn address(dir: &Path) -> Address {
        if cfg!(windows) {
            Address::Namespaced(format!(
                "birchpad-test-{:016x}",
                fnv1a(dir.to_string_lossy().as_bytes())
            ))
        } else {
            Address::File(dir.join("test.sock"))
        }
    }

    #[test]
    fn second_launch_hands_off_to_the_first() {
        let dir = tempfile::tempdir().unwrap();
        let address = address(dir.path());
        let first = CommandLine::default();
        let Instance::Primary(server) = start(&address, &first) else {
            panic!("the first launch should become the primary instance");
        };
        let (sender, received) = mpsc::channel();
        server.serve(move |request| {
            sender.send(request).unwrap();
        });

        let second = CommandLine {
            files: vec![dir.path().join("file with spaces.txt")],
            line: Some(120),
            ..CommandLine::default()
        };
        assert!(matches!(start(&address, &second), Instance::Secondary));
        let request = received.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(request.files, second.files);
        assert_eq!(request.line, Some(120));
    }

    #[cfg(unix)]
    #[test]
    fn a_stale_socket_file_is_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let address = address(dir.path());
        // A socket file nobody listens on, as a crash leaves behind.
        std::fs::write(dir.path().join("test.sock"), b"").unwrap();
        assert!(matches!(
            start(&address, &CommandLine::default()),
            Instance::Primary(_)
        ));
    }

    #[test]
    fn a_silent_client_does_not_block_later_launches() {
        let dir = tempfile::tempdir().unwrap();
        let address = address(dir.path());
        let Instance::Primary(server) = start(&address, &CommandLine::default()) else {
            panic!("the first launch should become the primary instance");
        };
        let (sender, received) = mpsc::channel();
        server.serve(move |request| {
            sender.send(request).unwrap();
        });
        // Connected, but never sends a line.
        let _silent = Stream::connect(address.name().unwrap()).unwrap();
        let second = CommandLine {
            read_only: true,
            ..CommandLine::default()
        };
        assert!(matches!(start(&address, &second), Instance::Secondary));
        assert!(
            received
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
                .read_only
        );
    }

    #[test]
    fn a_hung_instance_keeps_its_socket() {
        let dir = tempfile::tempdir().unwrap();
        let address = address(dir.path());
        // Listens but never accepts, like an instance whose server thread is stuck.
        let Instance::Primary(_hung) = start(&address, &CommandLine::default()) else {
            panic!("the first launch should become the primary instance");
        };
        let Instance::Standalone(error) = start(&address, &CommandLine::default()) else {
            panic!("a launch that gets no answer should run on its own");
        };
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    }

    #[test]
    fn installations_get_different_addresses() {
        let a = Address::for_installation(Path::new("/home/u/.local/share/birchpad"));
        let b = Address::for_installation(Path::new("/opt/birchpad-portable/data"));
        assert_ne!(a, b);
        assert_eq!(
            a,
            Address::for_installation(Path::new("/home/u/.local/share/birchpad"))
        );
    }
}
