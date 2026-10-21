use birchpad_config::*;
use std::env;
use std::fs;
use toml::Table;

#[test]
fn missing_files_are_not_errors_but_broken_ones_are() {
    let dir = env::temp_dir().join(format!("birchpad-config-test-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let broken = dir.join("settings.toml");
    fs::write(&broken, "editor = [unclosed").unwrap();

    let sources = load(&ConfigPaths {
        machine_defaults: Some(dir.join("missing.toml")),
        user_settings: Some(broken),
        policy_file: None,
        user_data: None,
        portable: false,
        local_settings: None,
    });
    fs::remove_dir_all(&dir).unwrap();

    assert!(sources.machine.is_none());
    assert!(sources.user.is_none());
    let user_problems: Vec<_> = sources
        .diagnostics
        .iter()
        .filter(|d| d.layer == Layer::User)
        .collect();
    assert_eq!(user_problems.len(), 1);
}

#[test]
fn unreadable_files_are_reported_and_empty_ones_are_empty_layers() {
    let dir = tempfile::tempdir().unwrap();
    // A folder where the policy file should be cannot be read; an empty file is a layer
    // with nothing in it; a good machine file is read.
    let policy = dir.path().join("policies.toml");
    fs::create_dir(&policy).unwrap();
    let user = dir.path().join("settings.toml");
    fs::write(&user, "").unwrap();
    let machine = dir.path().join("defaults.toml");
    fs::write(&machine, "editor.tab-width = 2").unwrap();

    let sources = load(&ConfigPaths {
        machine_defaults: Some(machine),
        user_settings: Some(user),
        policy_file: Some(policy),
        user_data: None,
        portable: false,
        local_settings: None,
    });
    assert_eq!(
        sources.machine.and_then(|t| t.get("editor").cloned()),
        Some(toml::Value::Table(toml::from_str("tab-width = 2").unwrap()))
    );
    assert_eq!(sources.user, Some(Table::new()));
    let policy_problems = sources
        .diagnostics
        .iter()
        .filter(|d| d.layer == Layer::Policy && d.key.is_none())
        .count();
    assert_eq!(policy_problems, 1, "{:?}", sources.diagnostics);
}

#[test]
fn no_paths_load_nothing() {
    let sources = load(&ConfigPaths {
        machine_defaults: None,
        user_settings: None,
        policy_file: None,
        user_data: None,
        portable: false,
        local_settings: None,
    });
    assert!(sources.machine.is_none() && sources.user.is_none());
    assert_eq!(ConfigPaths::default().user_config_dir(), None);
}

#[test]
fn a_marker_next_to_the_executable_makes_it_portable() {
    let dir = env::temp_dir().join(format!("birchpad-portable-test-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let exe = dir.join("birchpad.exe");
    let installed = ConfigPaths::for_executable(&exe);
    fs::write(dir.join(PORTABLE_MARKER), "").unwrap();
    let portable = ConfigPaths::for_executable(&exe);
    fs::remove_dir_all(&dir).unwrap();

    assert_eq!(installed, ConfigPaths::platform());
    assert!(portable.portable);
    assert_eq!(portable.user_data, Some(dir.join("data")));
    assert_eq!(portable.user_config_dir(), Some(dir.join("data").as_path()));
    // Policies and machine defaults are not the portable copy's to choose.
    assert_eq!(portable.policy_file, installed.policy_file);
    assert_eq!(portable.machine_defaults, installed.machine_defaults);
}

#[test]
fn settings_in_a_cloud_folder() {
    let dir = tempfile::tempdir().unwrap();
    let local = dir.path().join("settings.toml");
    let cloud = dir.path().join("cloud");
    fs::create_dir(&cloud).unwrap();
    fs::write(cloud.join("settings.toml"), "editor.tab-width = 3").unwrap();
    let paths = ConfigPaths {
        user_settings: Some(local.clone()),
        ..ConfigPaths::default()
    };
    // No local file, or no folder in it, or a folder that is not there: as they are.
    assert_eq!(paths.clone().following_cloud(), paths);
    fs::write(&local, "cloud.folder = \"\"").unwrap();
    assert_eq!(paths.clone().following_cloud(), paths);
    fs::write(&local, "cloud.folder = \"/no/such/folder\"").unwrap();
    assert_eq!(paths.clone().following_cloud(), paths);
    fs::write(&local, "[cloud\n").unwrap();
    assert_eq!(paths.clone().following_cloud(), paths);

    let text = format!("cloud.folder = '{}'\neditor.tab-width = 9", cloud.display());
    fs::write(&local, text).unwrap();
    let following = paths.following_cloud();
    assert_eq!(following.user_settings, Some(cloud.join("settings.toml")));
    assert_eq!(following.local_settings, Some(local.clone()));
    assert_eq!(
        following.settings_file_for("cloud.folder"),
        Some(local.as_path())
    );
    assert_eq!(
        following.settings_file_for("editor.tab-width"),
        Some(cloud.join("settings.toml").as_path())
    );
    let resolved = resolve(load(&following));
    assert_eq!(
        resolved.settings.editor.tab_width, 3,
        "the cloud's settings"
    );
    assert_eq!(resolved.settings.cloud.folder, cloud.display().to_string());
}

#[test]
fn without_user_settings_there_is_no_cloud_folder_to_follow() {
    let paths = ConfigPaths {
        user_settings: None,
        ..ConfigPaths::default()
    };
    assert_eq!(paths.clone().following_cloud(), paths);
}

#[test]
fn the_running_executable_is_not_portable_without_its_marker() {
    // The test executable has no marker next to it.
    let current = ConfigPaths::current();
    assert!(!current.portable);
    assert_eq!(current, ConfigPaths::platform());
    // Reading the platform's files never fails: missing or unreadable ones are reported.
    let settings = load_platform_settings();
    assert!(settings.diagnostics.iter().all(|d| !d.message.is_empty()));
}
