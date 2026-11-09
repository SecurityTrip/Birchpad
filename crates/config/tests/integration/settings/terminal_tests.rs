//! The settings of Preferences > Terminal (ADR 0067).

use birchpad_config::*;
use toml::Table;

fn table(toml: &str) -> Option<Table> {
    Some(toml::from_str(toml).unwrap())
}

#[test]
fn the_systems_shell_and_ten_thousand_lines_by_default() {
    let settings = Settings::default();
    assert_eq!(settings.terminal.shell, "");
    assert_eq!(settings.terminal.scrollback, 10_000);
    assert_eq!(settings.terminal.scrollback_lines(), 10_000);
    let settings: Settings = toml::from_str("[terminal]\nshell = 'pwsh -NoLogo'\n").unwrap();
    assert_eq!(settings.terminal.shell, "pwsh -NoLogo");
    assert_eq!(
        settings.terminal.scrollback, 10_000,
        "the rest keeps its default"
    );
}

#[test]
fn the_scrollback_has_a_limit() {
    for (lines, kept) in [
        (0, 0),
        (1, 1),
        (MAX_SCROLLBACK, MAX_SCROLLBACK as usize),
        (MAX_SCROLLBACK + 1, MAX_SCROLLBACK as usize),
        (u32::MAX, MAX_SCROLLBACK as usize),
    ] {
        let settings = TerminalSettings {
            scrollback: lines,
            ..TerminalSettings::default()
        };
        assert_eq!(settings.scrollback_lines(), kept, "{lines}");
    }
}

#[test]
fn terminal_settings_are_checked() {
    check_setting("terminal.shell", &toml::Value::String("bash -l".into())).unwrap();
    check_setting("terminal.scrollback", &toml::Value::Integer(500)).unwrap();
    for (key, value) in [
        ("terminal.shell", toml::Value::Integer(1)),
        ("terminal.scrollback", toml::Value::Integer(-1)),
        ("terminal.scrollback", toml::Value::String("many".into())),
    ] {
        assert!(check_setting(key, &value).is_err(), "{key} = {value}");
    }
    let resolved = resolve(Sources {
        user: table("terminal.scrollback = -5\nterminal.shell = 'zsh'"),
        ..Sources::default()
    });
    assert_eq!(resolved.settings.terminal.scrollback, 10_000);
    assert_eq!(resolved.settings.terminal.shell, "zsh");
    assert_eq!(resolved.diagnostics.len(), 1);
}
