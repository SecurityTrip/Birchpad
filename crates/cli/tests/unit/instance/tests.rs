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
        let mut stream = BufReader::new(Stream::connect(address.name().unwrap()).unwrap());
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
        .name(address.name().unwrap())
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
    let error = hand_off(&address(dir.path()), &CommandLine::default()).unwrap_err();
    assert!(is_unoccupied(&address(dir.path()), &error), "{error}");
    // Other failures do not mean that nobody listens.
    let timed_out = io::Error::from(io::ErrorKind::TimedOut);
    assert!(!is_unoccupied(&address(dir.path()), &timed_out));
}

#[test]
fn the_hash_of_socket_names_is_fnv1a() {
    assert_eq!(fnv1a(b""), 0xcbf2_9ce4_8422_2325, "the offset basis");
    assert_eq!(fnv1a(b"a"), 0xaf63_dc4c_8601_ec8c);
    assert_ne!(fnv1a(b"ab"), fnv1a(b"ba"));
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
