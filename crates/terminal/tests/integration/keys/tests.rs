use birchpad_terminal::keys::*;

fn plain(key: Key) -> Vec<u8> {
    encode(key, Modifiers::default(), false).unwrap()
}

fn with(key: Key, shift: bool, alt: bool, control: bool) -> Option<Vec<u8>> {
    encode(
        key,
        Modifiers {
            shift,
            alt,
            control,
        },
        false,
    )
}

#[test]
fn characters_and_control_characters() {
    assert_eq!(plain(Key::Char('a')), b"a");
    assert_eq!(plain(Key::Char('A')), b"A");
    assert_eq!(plain(Key::Char('ё')), "ё".as_bytes());
    assert_eq!(plain(Key::Char('😀')), "😀".as_bytes());
    // Ctrl with a letter, either case: its control character.
    assert_eq!(with(Key::Char('c'), false, false, true), Some(vec![3]));
    assert_eq!(with(Key::Char('C'), true, false, true), Some(vec![3]));
    assert_eq!(with(Key::Char('a'), false, false, true), Some(vec![1]));
    assert_eq!(with(Key::Char('z'), false, false, true), Some(vec![26]));
    for (c, byte) in [
        (' ', 0),
        ('@', 0),
        ('[', 0x1b),
        ('\\', 0x1c),
        (']', 0x1d),
        ('^', 0x1e),
        ('_', 0x1f),
        ('?', 0x7f),
    ] {
        assert_eq!(
            with(Key::Char(c), false, false, true),
            Some(vec![byte]),
            "{c}"
        );
    }
    // Ctrl with what has none: nothing.
    assert_eq!(with(Key::Char('ё'), false, false, true), None);
    assert_eq!(with(Key::Char('!'), false, false, true), None);
    // Alt: ESC first.
    assert_eq!(
        with(Key::Char('x'), false, true, false),
        Some(b"\x1bx".to_vec())
    );
    assert_eq!(
        with(Key::Char('x'), false, true, true),
        Some(vec![0x1b, 24])
    );
}

#[test]
fn editing_keys() {
    assert_eq!(plain(Key::Enter), b"\r");
    assert_eq!(
        with(Key::Enter, false, true, false),
        Some(b"\x1b\r".to_vec())
    );
    assert_eq!(plain(Key::Tab), b"\t");
    assert_eq!(with(Key::Tab, true, false, false), Some(b"\x1b[Z".to_vec()));
    assert_eq!(plain(Key::Backspace), [0x7f]);
    assert_eq!(with(Key::Backspace, false, false, true), Some(vec![0x08]));
    assert_eq!(
        with(Key::Backspace, false, true, false),
        Some(vec![0x1b, 0x7f])
    );
    assert_eq!(plain(Key::Escape), [0x1b]);
}

#[test]
fn cursor_keys_in_both_modes_and_with_modifiers() {
    assert_eq!(plain(Key::Up), b"\x1b[A");
    assert_eq!(plain(Key::Down), b"\x1b[B");
    assert_eq!(plain(Key::Right), b"\x1b[C");
    assert_eq!(plain(Key::Left), b"\x1b[D");
    assert_eq!(plain(Key::Home), b"\x1b[H");
    assert_eq!(plain(Key::End), b"\x1b[F");
    // Application cursor mode: SS3.
    let app = |key| encode(key, Modifiers::default(), true).unwrap();
    assert_eq!(app(Key::Up), b"\x1bOA");
    assert_eq!(app(Key::Home), b"\x1bOH");
    // Modifiers: 2 Shift, 3 Alt, 5 Ctrl, 8 all three; the mode does not count then.
    assert_eq!(
        with(Key::Up, true, false, false),
        Some(b"\x1b[1;2A".to_vec())
    );
    assert_eq!(
        with(Key::Left, false, true, false),
        Some(b"\x1b[1;3D".to_vec())
    );
    assert_eq!(
        with(Key::Right, false, false, true),
        Some(b"\x1b[1;5C".to_vec())
    );
    assert_eq!(
        with(Key::End, true, true, true),
        Some(b"\x1b[1;8F".to_vec())
    );
    let control = Modifiers {
        control: true,
        ..Modifiers::default()
    };
    assert_eq!(encode(Key::Up, control, true), Some(b"\x1b[1;5A".to_vec()));
}

#[test]
fn tilde_keys_and_function_keys() {
    assert_eq!(plain(Key::Insert), b"\x1b[2~");
    assert_eq!(plain(Key::Delete), b"\x1b[3~");
    assert_eq!(plain(Key::PageUp), b"\x1b[5~");
    assert_eq!(plain(Key::PageDown), b"\x1b[6~");
    assert_eq!(
        with(Key::Delete, false, false, true),
        Some(b"\x1b[3;5~".to_vec())
    );
    assert_eq!(plain(Key::F(1)), b"\x1bOP");
    assert_eq!(plain(Key::F(4)), b"\x1bOS");
    assert_eq!(
        with(Key::F(1), true, false, false),
        Some(b"\x1b[1;2P".to_vec())
    );
    assert_eq!(plain(Key::F(5)), b"\x1b[15~");
    assert_eq!(plain(Key::F(6)), b"\x1b[17~");
    assert_eq!(plain(Key::F(10)), b"\x1b[21~");
    assert_eq!(plain(Key::F(11)), b"\x1b[23~");
    assert_eq!(plain(Key::F(12)), b"\x1b[24~");
    assert_eq!(
        with(Key::F(12), false, false, true),
        Some(b"\x1b[24;5~".to_vec())
    );
    // Past the ends: nothing.
    assert_eq!(encode(Key::F(0), Modifiers::default(), false), None);
    assert_eq!(encode(Key::F(13), Modifiers::default(), false), None);
}

#[test]
fn pastes_send_line_breaks_as_enter() {
    assert_eq!(paste("a\nb\r\nc\rd", false), b"a\rb\rc\rd");
    assert_eq!(paste("", false), b"");
    assert_eq!(paste("ls\n", true), b"\x1b[200~ls\r\x1b[201~");
    // An ESC in the text cannot end the paste early.
    assert_eq!(paste("x\x1b[201~rm", true), b"\x1b[200~x[201~rm\x1b[201~");
    assert_eq!(paste("", true), b"\x1b[200~\x1b[201~");
    assert_eq!(paste("ё", false), "ё".as_bytes());
}
