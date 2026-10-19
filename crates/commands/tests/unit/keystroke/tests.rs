use super::*;

fn parse(source: &str) -> Keystroke {
    Keystroke::parse(source, Platform::Windows).unwrap()
}

#[test]
fn parses_modifiers_and_keys() {
    let keystroke = parse("Ctrl-Shift-S");
    assert!(keystroke.modifiers.ctrl && keystroke.modifiers.shift);
    assert_eq!(keystroke.key, "s");
    assert_eq!(parse("ctrl--").key, "-");
    assert_eq!(parse("-").key, "-");
    assert_eq!(parse("alt-f4").to_string(), "alt-f4");
    assert_eq!(parse("shift-ctrl-tab").to_string(), "ctrl-shift-tab");
    assert_eq!(parse("ctrl-+").key, "+");
}

#[test]
fn secondary_depends_on_platform() {
    let windows = Keystroke::parse("secondary-s", Platform::Windows).unwrap();
    let mac = Keystroke::parse("secondary-s", Platform::MacOs).unwrap();
    assert_eq!(windows, parse("ctrl-s"));
    assert_eq!(mac.to_string(), "cmd-s");
    assert_eq!(
        Keystroke::parse("secondary-s", Platform::Linux).unwrap(),
        windows
    );
}

#[test]
fn the_current_platform_is_the_one_built_for() {
    let expected = if cfg!(target_os = "macos") {
        Platform::MacOs
    } else if cfg!(windows) {
        Platform::Windows
    } else {
        Platform::Linux
    };
    assert_eq!(Platform::current(), expected);
}

#[test]
fn rejects_invalid_keystrokes() {
    let err = |s| Keystroke::parse(s, Platform::Linux).unwrap_err();
    assert_eq!(err(""), KeystrokeError::Empty);
    assert_eq!(err("hyper-a"), KeystrokeError::UnknownKey("hyper".into()));
    assert_eq!(
        err("ctrl-ctrl-a"),
        KeystrokeError::DuplicateModifier("ctrl".into())
    );
    assert_eq!(
        err("ctrl-shift"),
        KeystrokeError::MissingKey("ctrl-shift".into())
    );
    assert_eq!(err("ctrl-f99"), KeystrokeError::UnknownKey("f99".into()));
    // A trailing separator leaves no key; the error says which keystroke.
    assert_eq!(err("ctrl-"), KeystrokeError::MissingKey("ctrl-".into()));
    assert_eq!(
        err("ctrl-").to_string(),
        "keystroke `ctrl-` has no key, only modifiers"
    );
    assert_eq!(err("").to_string(), "empty keystroke");
    assert_eq!(err("ctrl-pgup"), KeystrokeError::UnknownKey("pgup".into()));
}

#[test]
fn shifted_digits_get_the_spelling_linux_reports() {
    let shifted = |s| parse(s).shifted_symbol().map(|k| k.to_string());
    assert_eq!(shifted("ctrl-alt-shift-1").as_deref(), Some("ctrl-alt-!"));
    assert_eq!(shifted("alt-shift-0").as_deref(), Some("alt-)"));
    assert_eq!(shifted("ctrl-shift-=").as_deref(), Some("ctrl-+"));
    assert_eq!(shifted("ctrl-alt-1"), None, "no shift");
    assert_eq!(shifted("ctrl-shift-l"), None, "letters keep shift");
    assert_eq!(shifted("shift-f2"), None);
}

#[test]
fn parses_chords() {
    let keys = Keystroke::parse_sequence("ctrl-k  ctrl-c", Platform::Linux).unwrap();
    assert_eq!(keys.len(), 2);
    assert!(Keystroke::parse_sequence("  ", Platform::Linux).is_err());
}
