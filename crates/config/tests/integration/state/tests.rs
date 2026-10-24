use birchpad_config::*;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

#[test]
fn recent_files_are_most_recent_first_and_limited() {
    let mut state = UserState::default();
    for name in ["a", "b", "c", "b"] {
        state.add_recent(Path::new(name), 3);
    }
    assert_eq!(
        state.recent_files,
        [Path::new("b"), Path::new("c"), Path::new("a")]
    );
    state.add_recent(Path::new("d"), 2);
    assert_eq!(state.recent_files, [Path::new("d"), Path::new("b")]);
    state.remove_recent(Path::new("d"));
    assert_eq!(state.recent_files, [Path::new("b")]);
    state.add_recent(Path::new("e"), 0);
    assert!(state.recent_files.is_empty());
}

#[test]
fn recent_commands_are_most_recent_first_and_limited() {
    let mut state = UserState::default();
    for key in ["a", "b", "c", "b"] {
        state.add_recent_command(key, 3);
    }
    assert_eq!(state.recent_commands, ["b", "c", "a"]);
    state.add_recent_command("d", 2);
    assert_eq!(state.recent_commands, ["d", "b"]);
    state.add_recent_command("e", 0);
    assert!(state.recent_commands.is_empty());
    for index in 0..=MAX_RECENT_COMMANDS {
        state.add_recent_command(&index.to_string(), usize::MAX);
    }
    assert_eq!(state.recent_commands.len(), MAX_RECENT_COMMANDS);
    assert_eq!(state.recent_commands[0], MAX_RECENT_COMMANDS.to_string());
}

#[test]
fn round_trips_and_survives_damage() {
    let dir = std::env::temp_dir().join(format!("birchpad-state-{}", std::process::id()));
    let path = dir.join("state.toml");
    let mut state = UserState {
        zoom: 2,
        updates: UpdateState {
            last_check: 1_790_000_000,
            manifest_timestamp: 1_789_000_000,
            announced: Some("0.2.0".into()),
        },
        ..UserState::default()
    };
    state.add_recent(Path::new("/tmp/x y.txt"), 10);
    state.add_recent_command(r#"{"command":"file.save"}"#, 8);
    state.save(&path).unwrap();
    assert_eq!(UserState::load(&path), state);

    fs::write(&path, "recent-files = 7").unwrap();
    assert_eq!(UserState::load(&path), UserState::default());
    assert_eq!(
        UserState::load(&dir.join("missing.toml")),
        UserState::default()
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn find_in_files_options_round_trip_and_default_when_missing() {
    let defaults: UserState = toml::from_str("zoom = 1").unwrap();
    assert_eq!(defaults.find_in_files, FindInFilesState::default());
    assert!(defaults.find_in_files.subfolders, "Notepad++'s default");
    // Some keys given: the others keep their defaults.
    let partial: UserState = toml::from_str("[find-in-files]\nhidden = true").unwrap();
    assert!(partial.find_in_files.hidden && partial.find_in_files.subfolders);
    let state = UserState {
        find_in_files: FindInFilesState {
            filters: r"*.rs !+\target".into(),
            directory: Some(PathBuf::from("/projects/birchpad")),
            subfolders: false,
            hidden: true,
            follow_current_document: true,
        },
        ..UserState::default()
    };
    let text = toml::to_string(&state).unwrap();
    assert_eq!(toml::from_str::<UserState>(&text).unwrap(), state);
    // A wrong type is damage: the whole state is the default, as for any other key.
    assert!(toml::from_str::<UserState>("[find-in-files]\nsubfolders = 3").is_err());
}
