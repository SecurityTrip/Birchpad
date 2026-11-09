//! Keys as the bytes a terminal sends for them, as xterm does: control characters for Ctrl with
//! a letter, `ESC` before a key with Alt, CSI sequences for the other keys with the modifiers'
//! number, and SS3 for the arrows in application cursor mode.

/// A key pressed in the terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// A character key, as typed (with Shift already applied).
    Char(char),
    Enter,
    Tab,
    Backspace,
    Escape,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Insert,
    Delete,
    /// F1 to F12.
    F(u8),
}

/// The modifiers held.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub shift: bool,
    pub alt: bool,
    pub control: bool,
}

impl Modifiers {
    /// xterm's number for the modifiers in a CSI sequence: 1 plus 1 for Shift, 2 for Alt, 4 for
    /// Control; `None` without any.
    fn parameter(self) -> Option<u8> {
        let value = u8::from(self.shift) + 2 * u8::from(self.alt) + 4 * u8::from(self.control);
        (value > 0).then_some(value + 1)
    }
}

/// The control character of Ctrl with `c`: letters, `@` and space, `[` `\` `]` `^` `_`, `?`.
fn control(c: char) -> Option<u8> {
    match c {
        'a'..='z' => Some(c as u8 - b'a' + 1),
        'A'..='Z' => Some(c as u8 - b'A' + 1),
        '@' | ' ' | '2' => Some(0),
        '[' | '3' => Some(0x1b),
        '\\' | '4' => Some(0x1c),
        ']' | '5' => Some(0x1d),
        '^' | '6' => Some(0x1e),
        '_' | '-' | '7' => Some(0x1f),
        '?' | '8' => Some(0x7f),
        _ => None,
    }
}

/// The bytes of `key` with `modifiers`; `app_cursor` when the program asked for application
/// cursor keys (DECCKM). `None` for a key a terminal sends nothing for.
pub fn encode(key: Key, modifiers: Modifiers, app_cursor: bool) -> Option<Vec<u8>> {
    let alt = |mut bytes: Vec<u8>| {
        if modifiers.alt {
            bytes.insert(0, 0x1b);
        }
        bytes
    };
    let cursor = |letter: u8| match modifiers.parameter() {
        None if app_cursor => vec![0x1b, b'O', letter],
        None => vec![0x1b, b'[', letter],
        Some(parameter) => format!("\x1b[1;{parameter}{}", letter as char).into_bytes(),
    };
    let tilde = |number: u8| match modifiers.parameter() {
        None => format!("\x1b[{number}~").into_bytes(),
        Some(parameter) => format!("\x1b[{number};{parameter}~").into_bytes(),
    };
    Some(match key {
        Key::Char(c) => {
            if modifiers.control {
                alt(vec![control(c)?])
            } else {
                let mut bytes = [0; 4];
                alt(c.encode_utf8(&mut bytes).as_bytes().to_vec())
            }
        }
        Key::Enter => alt(vec![b'\r']),
        Key::Tab if modifiers.shift => b"\x1b[Z".to_vec(),
        Key::Tab => alt(vec![b'\t']),
        Key::Backspace if modifiers.control => alt(vec![0x08]),
        Key::Backspace => alt(vec![0x7f]),
        Key::Escape => alt(vec![0x1b]),
        Key::Up => cursor(b'A'),
        Key::Down => cursor(b'B'),
        Key::Right => cursor(b'C'),
        Key::Left => cursor(b'D'),
        Key::Home => cursor(b'H'),
        Key::End => cursor(b'F'),
        Key::Insert => tilde(2),
        Key::Delete => tilde(3),
        Key::PageUp => tilde(5),
        Key::PageDown => tilde(6),
        Key::F(number @ 1..=4) => {
            let letter = b"PQRS"[usize::from(number - 1)];
            match modifiers.parameter() {
                None => vec![0x1b, b'O', letter],
                Some(parameter) => format!("\x1b[1;{parameter}{}", letter as char).into_bytes(),
            }
        }
        Key::F(number @ 5..=12) => tilde([15, 17, 18, 19, 20, 21, 23, 24][usize::from(number - 5)]),
        Key::F(_) => return None,
    })
}

/// The bytes of pasting `text`: line breaks sent as Enter sends them, and, when the program
/// asked for bracketed paste, between its markers, without an `ESC` that could end them early.
pub fn paste(text: &str, bracketed: bool) -> Vec<u8> {
    let text = text.replace("\r\n", "\r").replace('\n', "\r");
    if !bracketed {
        return text.into_bytes();
    }
    let mut bytes = b"\x1b[200~".to_vec();
    bytes.extend(text.bytes().filter(|byte| *byte != 0x1b));
    bytes.extend_from_slice(b"\x1b[201~");
    bytes
}
