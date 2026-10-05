//! Keystrokes in the textual form GPUI uses: modifiers and a key joined with `-`, e.g.
//! `ctrl-shift-s`, `secondary-w`, `alt-f4`, `ctrl--`.
//!
//! `secondary` is the platform's primary shortcut modifier: Ctrl on Windows and Linux, Cmd on
//! macOS. It is resolved when parsing, so two spellings of the same keystroke compare equal.

use std::fmt;

/// The platform a keymap is resolved for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Platform {
    Windows,
    Linux,
    MacOs,
}

impl Platform {
    pub const fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::MacOs
        } else if cfg!(windows) {
            Self::Windows
        } else {
            Self::Linux
        }
    }

    /// The name used in keymap files: `windows`, `linux` or `macos`.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Windows => "windows",
            Self::Linux => "linux",
            Self::MacOs => "macos",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Modifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    /// Cmd on macOS, the Windows/Super key elsewhere.
    pub platform: bool,
    pub function: bool,
}

/// One key press with modifiers. The key is lowercase: a named key (`enter`, `f3`, `pageup`) or
/// a single character (`a`, `+`, `-`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Keystroke {
    pub modifiers: Modifiers,
    pub key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum KeystrokeError {
    #[error("empty keystroke")]
    Empty,
    #[error("unknown modifier or key `{0}`")]
    UnknownKey(String),
    #[error("modifier `{0}` appears twice")]
    DuplicateModifier(String),
    #[error("keystroke `{0}` has no key, only modifiers")]
    MissingKey(String),
}

/// Keys with names, as GPUI reports them. Single printable characters are keys too.
const NAMED_KEYS: &[&str] = &[
    "enter",
    "tab",
    "space",
    "backspace",
    "delete",
    "insert",
    "escape",
    "left",
    "right",
    "up",
    "down",
    "home",
    "end",
    "pageup",
    "pagedown",
    "menu",
    "back",
    "forward",
    // Keypad keys on Linux (X11/Wayland report e.g. `KP_Add` as `add`).
    "add",
    "subtract",
    "multiply",
    "divide",
    "decimal",
];

impl Keystroke {
    /// Parses one keystroke, resolving `secondary` for `platform`.
    pub fn parse(source: &str, platform: Platform) -> Result<Self, KeystrokeError> {
        let source = source.trim();
        if source.is_empty() {
            return Err(KeystrokeError::Empty);
        }
        // `-` as the key itself: "-" or "ctrl--".
        let (modifier_part, key) = if source == "-" {
            ("", "-".to_owned())
        } else if let Some(rest) = source.strip_suffix("--") {
            (rest, "-".to_owned())
        } else {
            match source.rsplit_once('-') {
                Some((modifiers, key)) => (modifiers, key.to_ascii_lowercase()),
                None => ("", source.to_ascii_lowercase()),
            }
        };
        if key.is_empty() {
            return Err(KeystrokeError::MissingKey(source.to_owned()));
        }

        let mut modifiers = Modifiers::default();
        for part in modifier_part.split('-').filter(|part| !part.is_empty()) {
            let lower = part.to_ascii_lowercase();
            let flag = match lower.as_str() {
                "ctrl" | "control" => &mut modifiers.ctrl,
                "alt" | "option" => &mut modifiers.alt,
                "shift" => &mut modifiers.shift,
                "cmd" | "super" | "win" => &mut modifiers.platform,
                "fn" => &mut modifiers.function,
                "secondary" => match platform {
                    Platform::MacOs => &mut modifiers.platform,
                    Platform::Windows | Platform::Linux => &mut modifiers.ctrl,
                },
                _ => return Err(KeystrokeError::UnknownKey(part.to_owned())),
            };
            if *flag {
                return Err(KeystrokeError::DuplicateModifier(lower));
            }
            *flag = true;
        }

        let is_named = NAMED_KEYS.contains(&key.as_str()) || is_function_key(&key);
        let is_char = key.chars().count() == 1 && !key.chars().all(char::is_whitespace);
        if !is_named && !is_char {
            if matches!(
                key.as_str(),
                "ctrl" | "alt" | "shift" | "cmd" | "super" | "win" | "fn" | "secondary"
            ) {
                return Err(KeystrokeError::MissingKey(source.to_owned()));
            }
            return Err(KeystrokeError::UnknownKey(key));
        }
        Ok(Self { modifiers, key })
    }

    /// Parses a space-separated sequence of keystrokes (a chord such as `ctrl-k ctrl-c`).
    pub fn parse_sequence(source: &str, platform: Platform) -> Result<Vec<Self>, KeystrokeError> {
        let keys: Vec<Self> = source
            .split_whitespace()
            .map(|part| Self::parse(part, platform))
            .collect::<Result<_, _>>()?;
        if keys.is_empty() {
            return Err(KeystrokeError::Empty);
        }
        Ok(keys)
    }
}

/// What Shift with each digit and punctuation key types on a US layout.
const US_SHIFTED: [(char, char); 21] = [
    ('1', '!'),
    ('2', '@'),
    ('3', '#'),
    ('4', '$'),
    ('5', '%'),
    ('6', '^'),
    ('7', '&'),
    ('8', '*'),
    ('9', '('),
    ('0', ')'),
    ('-', '_'),
    ('=', '+'),
    ('[', '{'),
    (']', '}'),
    ('\\', '|'),
    (';', ':'),
    ('\'', '"'),
    (',', '<'),
    ('.', '>'),
    ('/', '?'),
    ('`', '~'),
];

impl Keystroke {
    /// The keystroke as X11 and Wayland report it when Shift is held with a digit or
    /// punctuation key: the shifted character without Shift (`ctrl-alt-shift-1` arrives as
    /// `ctrl-alt-!`). GPUI has no keyboard mapper on Linux, so bindings with such keys need
    /// this second spelling. The characters are a US layout's; layouts that are not Latin
    /// report the US character too, but European ones may type others (a German Shift+7 is
    /// `/`). `None` for keystrokes that need no second spelling.
    pub fn shifted_symbol(&self) -> Option<Self> {
        if !self.modifiers.shift {
            return None;
        }
        let mut chars = self.key.chars();
        let (Some(key), None) = (chars.next(), chars.next()) else {
            return None;
        };
        let (_, shifted) = US_SHIFTED.iter().find(|(plain, _)| *plain == key)?;
        Some(Self {
            modifiers: Modifiers {
                shift: false,
                ..self.modifiers
            },
            key: shifted.to_string(),
        })
    }
}

fn is_function_key(key: &str) -> bool {
    key.strip_prefix('f')
        .and_then(|n| n.parse::<u8>().ok())
        .is_some_and(|n| (1..=24).contains(&n))
}

/// The canonical spelling, accepted by GPUI's keystroke parser.
impl fmt::Display for Keystroke {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Modifiers {
            ctrl,
            alt,
            shift,
            platform,
            function,
        } = self.modifiers;
        for (on, name) in [
            (ctrl, "ctrl"),
            (alt, "alt"),
            (shift, "shift"),
            (platform, "cmd"),
            (function, "fn"),
        ] {
            if on {
                write!(f, "{name}-")?;
            }
        }
        f.write_str(&self.key)
    }
}

#[cfg(test)]
mod tests {
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
}
