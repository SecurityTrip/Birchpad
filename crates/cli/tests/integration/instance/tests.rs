use birchpad_cli::*;
use interprocess::local_socket::Stream;
use interprocess::local_socket::prelude::*;
use interprocess::local_socket::{GenericFilePath, GenericNamespaced, ListenerOptions, Name};
use std::io;
use std::io::{BufRead as _, BufReader, Write as _};
use std::path::Path;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

/// An address of the test's own: named after its temporary folder, which is unique.
fn address(dir: &Path) -> Address {
    if cfg!(windows) {
        let unique = dir.file_name().unwrap().to_string_lossy();
        Address::Namespaced(format!("birchpad-test-{unique}"))
    } else {
        Address::File(dir.join("test.sock"))
    }
}

/// The name a client connects to at `address`.
fn name(address: &Address) -> Name<'static> {
    match address {
        Address::Namespaced(name) => name.clone().to_ns_name::<GenericNamespaced>().unwrap(),
        Address::File(path) => path.clone().to_fs_name::<GenericFilePath>().unwrap(),
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
    // A socket nobody listens on any more, as a crash leaves behind: the listener is gone,
    // the socket file stays.
    drop(std::os::unix::net::UnixListener::bind(dir.path().join("test.sock")).unwrap());
    assert!(dir.path().join("test.sock").exists());
    assert!(matches!(
        start(&address, &CommandLine::default()),
        Instance::Primary(_)
    ));
}

#[cfg(unix)]
#[test]
fn a_file_in_place_of_the_socket_is_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let address = address(dir.path());
    // Not a socket at all (macOS reports connecting to it differently from Linux).
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
    let _silent = Stream::connect(name(&address)).unwrap();
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
fn garbage_is_refused_and_later_launches_still_get_through() {
    let dir = tempfile::tempdir().unwrap();
    let address = address(dir.path());
    let Instance::Primary(server) = start(&address, &CommandLine::default()) else {
        panic!("the first launch should become the primary instance");
    };
    let (sender, received) = mpsc::channel();
    server.serve(move |request| {
        sender.send(request).unwrap();
    });
    // Not JSON, and a message past the size limit: no answer, nothing handed over.
    for message in [
        b"not json\n".to_vec(),
        vec![b'['; MAX_MESSAGE as usize + 10],
    ] {
        let mut stream = BufReader::new(Stream::connect(name(&address)).unwrap());
        let _ = stream.get_mut().write_all(&message);
        let _ = stream.get_mut().write_all(b"\n");
        let mut answer = String::new();
        let _ = stream.read_line(&mut answer);
        assert_eq!(answer, "", "no ok for garbage");
    }
    assert!(received.recv_timeout(Duration::from_millis(200)).is_err());
    assert!(matches!(
        start(&address, &CommandLine::default()),
        Instance::Secondary
    ));
    assert!(received.recv_timeout(Duration::from_secs(5)).is_ok());
}

#[test]
fn an_answer_other_than_ok_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let address = address(dir.path());
    let listener = ListenerOptions::new()
        .name(name(&address))
        .create_sync()
        .unwrap();
    thread::spawn(move || {
        let mut stream = BufReader::new(listener.incoming().next().unwrap().unwrap());
        let mut line = String::new();
        stream.read_line(&mut line).unwrap();
        stream.get_mut().write_all(b"busy\n").unwrap();
    });
    let error = hand_off(&address, &CommandLine::default()).unwrap_err();
    assert!(error.to_string().contains("unexpected answer"), "{error}");
}

#[test]
fn nothing_listens_at_a_new_address() {
    let dir = tempfile::tempdir().unwrap();
    let address = address(dir.path());
    assert!(hand_off(&address, &CommandLine::default()).is_err());
    // Nobody listens there: the first launch listens.
    assert!(matches!(
        start(&address, &CommandLine::default()),
        Instance::Primary(_)
    ));
}

/// A file that is not a socket where the socket goes: nobody can listen behind it, so it is
/// replaced.
#[cfg(unix)]
#[test]
fn a_file_in_the_way_of_the_socket_is_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let address = address(dir.path());
    let Address::File(path) = &address else {
        unreachable!("a socket file on Unix")
    };
    std::fs::write(path, "not a socket").unwrap();
    assert!(matches!(
        start(&address, &CommandLine::default()),
        Instance::Primary(_)
    ));
}

#[test]
fn socket_names_hash_the_data_folder_with_fnv1a() {
    // FNV-1a of nothing is its offset basis, of "a" a known value.
    let scope = |address: Address| match address {
        Address::Namespaced(name) => name.rsplit('-').next().unwrap().to_owned(),
        Address::File(path) => path
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .trim_start_matches("birchpad-")
            .to_owned(),
    };
    assert_eq!(
        scope(Address::for_installation(Path::new(""))),
        "cbf29ce484222325"
    );
    assert_eq!(
        scope(Address::for_installation(Path::new("a"))),
        "af63dc4c8601ec8c"
    );
}

#[test]
fn socket_paths_respect_the_length_limit() {
    let deep = PathBuf::from(format!("/home/u/{}", "nested/".repeat(20)));
    let dirs = [deep.clone(), PathBuf::from("/home/u/.local/share/birchpad")];
    assert_eq!(
        socket_path("b.sock", &dirs),
        PathBuf::from("/home/u/.local/share/birchpad/b.sock")
    );
    assert_eq!(socket_path("b.sock", &dirs[..1]), deep.join("b.sock"));
    // Exactly at the limit fits, one byte more does not. Without folders there is nowhere
    // to listen: an empty path, which fails and leaves Birchpad standalone.
    let name = "b.sock";
    let fits = PathBuf::from("/".repeat(MAX_SOCKET_PATH - name.len()));
    assert_eq!(fits.join(name).as_os_str().len(), MAX_SOCKET_PATH);
    let longer = PathBuf::from("/".repeat(MAX_SOCKET_PATH - name.len() + 1));
    let fallback = PathBuf::from("/tmp");
    assert_eq!(
        socket_path(name, &[fits.clone(), fallback.clone()]),
        fits.join(name)
    );
    assert_eq!(
        socket_path(name, &[longer, fallback.clone()]),
        fallback.join(name)
    );
    assert_eq!(socket_path(name, &[]), PathBuf::new());
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

#[test]
fn a_socket_that_cannot_be_made_means_running_alone() {
    let dir = tempfile::tempdir().unwrap();
    // The socket's folder would be inside a file.
    let file = dir.path().join("file");
    std::fs::write(&file, "").unwrap();
    let address = Address::File(file.join("sub").join("test.sock"));
    assert!(matches!(
        start(&address, &CommandLine::default()),
        Instance::Standalone(_)
    ));
}
