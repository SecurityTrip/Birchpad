//! The settings of Preferences > Git and Margins > Git changes (ADR 0066).

use birchpad_config::*;
use toml::Table;

fn table(toml: &str) -> Option<Table> {
    Some(toml::from_str(toml).unwrap())
}

#[test]
fn git_is_asked_by_default_through_git_on_the_path() {
    let settings = Settings::default();
    assert!(settings.git.enabled);
    assert_eq!(settings.git.program(), "git");
    assert!(settings.editor.git_changes);
    // A literal string in TOML: its backslashes stay.
    let settings: Settings = toml::from_str(
        r"[git]
program = 'C:\Program Files\Git\bin\git.exe'
",
    )
    .unwrap();
    assert_eq!(settings.git.program(), r"C:\Program Files\Git\bin\git.exe");
    assert!(settings.git.enabled, "the rest keeps its default");
}

#[test]
fn a_blank_program_is_git() {
    for blank in ["", "   ", "\t"] {
        let git = GitSettings {
            program: blank.into(),
            ..GitSettings::default()
        };
        assert_eq!(git.program(), "git", "{blank:?}");
    }
    let git = GitSettings {
        program: "  /opt/git/bin/git ".into(),
        ..GitSettings::default()
    };
    assert_eq!(git.program(), "/opt/git/bin/git");
}

#[test]
fn git_settings_are_checked() {
    check_setting("git.enabled", &toml::Value::Boolean(false)).unwrap();
    check_setting("git.program", &toml::Value::String("git".into())).unwrap();
    check_setting("editor.git-changes", &toml::Value::Boolean(false)).unwrap();
    for (key, value) in [
        ("git.enabled", toml::Value::String("no".into())),
        ("git.program", toml::Value::Integer(1)),
        ("editor.git-changes", toml::Value::Integer(0)),
    ] {
        assert!(check_setting(key, &value).is_err(), "{key} = {value}");
    }
    assert!(check_setting("git.path", &toml::Value::String("git".into())).is_err());
    let resolved = resolve(Sources {
        user: table("git.enabled = 'off'\ngit.program = 'mygit'\neditor.git-changes = false"),
        ..Sources::default()
    });
    assert!(resolved.settings.git.enabled, "a wrong value is left out");
    assert_eq!(resolved.settings.git.program(), "mygit");
    assert!(!resolved.settings.editor.git_changes);
    assert_eq!(resolved.diagnostics.len(), 1);
}
