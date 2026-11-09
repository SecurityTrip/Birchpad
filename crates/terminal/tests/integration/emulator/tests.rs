use birchpad_terminal::*;

const PALETTE: Palette = Palette::DARK;

fn screen_of(bytes: &[u8]) -> Screen {
    let mut emulator = Emulator::new(20, 5, 100);
    emulator.feed(bytes);
    emulator.screen(&PALETTE)
}

#[test]
fn text_lands_where_the_cursor_is() {
    let screen = screen_of(b"hello\r\nworld");
    assert_eq!(screen.text(), "hello\nworld");
    assert_eq!(screen.cursor, Some((1, 5)));
    assert_eq!((screen.columns, screen.lines), (20, 5));
    // A cursor moved by a sequence, and text over text.
    let screen = screen_of(b"abcdef\x1b[1;3HX\x1b[2;1Hy");
    assert_eq!(screen.text(), "abXdef\ny");
    // A line longer than the terminal wraps.
    let screen = screen_of(&[b'x'; 25]);
    assert_eq!(
        screen.text(),
        format!("{}\n{}", "x".repeat(20), "x".repeat(5))
    );
}

#[test]
fn colors_of_every_kind() {
    let cell = |bytes: &[u8]| screen_of(bytes).rows[0][0];
    // Defaults.
    let plain = cell(b"a");
    assert_eq!(
        (plain.foreground, plain.background),
        (PALETTE.foreground, PALETTE.background)
    );
    // The 16 of ANSI, bright ones, 256 and RGB, foreground and background.
    assert_eq!(cell(b"\x1b[31ma").foreground, PALETTE.ansi[1]);
    assert_eq!(cell(b"\x1b[92ma").foreground, PALETTE.ansi[10]);
    assert_eq!(cell(b"\x1b[44ma").background, PALETTE.ansi[4]);
    assert_eq!(cell(b"\x1b[38;5;196ma").foreground, 0xff0000);
    assert_eq!(cell(b"\x1b[48;5;232ma").background, 0x080808);
    assert_eq!(cell(b"\x1b[38;2;1;2;3ma").foreground, 0x010203);
    // Inverse swaps them; hidden text takes the background; dim moves toward it.
    let inverse = cell(b"\x1b[7ma");
    assert_eq!(
        (inverse.foreground, inverse.background),
        (PALETTE.background, PALETTE.foreground)
    );
    assert_eq!(cell(b"\x1b[8ma").foreground, PALETTE.background);
    let dim = cell(b"\x1b[2ma").foreground;
    assert_ne!(dim, PALETTE.foreground);
    assert_ne!(dim, PALETTE.background);
    // Reset.
    assert_eq!(cell(b"\x1b[31m\x1b[0ma").foreground, PALETTE.foreground);
    // A program's own default foreground (OSC 10).
    assert_eq!(cell(b"\x1b]10;rgb:ff/00/00\x07a").foreground, 0xff0000);
}

#[test]
fn styles_and_wide_characters() {
    let screen = screen_of("\x1b[1;3;4mb\x1b[0m日x".as_bytes());
    let row = &screen.rows[0];
    assert!(row[0].bold && row[0].italic && row[0].underline);
    assert!(!row[1].bold);
    assert_eq!(row[1].character, '日');
    assert!(row[1].wide);
    // The cell after a wide character is a space; the next character follows it.
    assert_eq!(row[2].character, ' ');
    assert_eq!(row[3].character, 'x');
    assert_eq!(screen.cursor, Some((0, 4)));
}

#[test]
fn modes_programs_ask_for() {
    let screen = screen_of(b"");
    assert!(!screen.bracketed_paste && !screen.app_cursor && !screen.alternate);
    assert!(screen.cursor.is_some());
    let screen = screen_of(b"\x1b[?2004h\x1b[?1h\x1b[?25l");
    assert!(screen.bracketed_paste && screen.app_cursor);
    assert_eq!(screen.cursor, None, "hidden");
    // A full-screen program's screen, and back.
    let screen = screen_of(b"shell\x1b[?1049hfull");
    assert!(screen.alternate);
    assert_eq!(screen.text(), "full");
    let screen = screen_of(b"shell\x1b[?1049hfull\x1b[?1049l");
    assert!(!screen.alternate);
    assert_eq!(screen.text(), "shell");
}

#[test]
fn scrolling_back_and_forth() {
    let mut emulator = Emulator::new(10, 3, 100);
    for line in 0..10 {
        emulator.feed(format!("line {line}\r\n").as_bytes());
    }
    let bottom = emulator.screen(&PALETTE);
    assert_eq!(bottom.text(), "line 8\nline 9");
    assert_eq!(bottom.scrolled, 0);
    emulator.scroll(3);
    let back = emulator.screen(&PALETTE);
    assert_eq!(back.scrolled, 3);
    assert_eq!(back.text(), "line 5\nline 6\nline 7");
    assert_eq!(back.cursor, None, "no cursor while scrolled back");
    // Past the top: as far as the scrollback goes.
    emulator.scroll(1000);
    assert_eq!(emulator.screen(&PALETTE).scrolled, 8);
    emulator.scroll(-2);
    assert_eq!(emulator.screen(&PALETTE).scrolled, 6);
    emulator.scroll_to_bottom();
    assert_eq!(emulator.screen(&PALETTE).scrolled, 0);
    // Without scrollback nothing is kept.
    let mut none = Emulator::new(10, 3, 0);
    none.feed(b"a\r\nb\r\nc\r\nd\r\n");
    none.scroll(5);
    assert_eq!(none.screen(&PALETTE).scrolled, 0);
}

#[test]
fn resizing_keeps_the_text() {
    let mut emulator = Emulator::new(10, 3, 100);
    emulator.feed(b"abc");
    emulator.resize(30, 6);
    let screen = emulator.screen(&PALETTE);
    assert_eq!((screen.columns, screen.lines), (30, 6));
    assert_eq!(screen.text(), "abc");
    // The smallest terminal is two columns by one line.
    emulator.resize(0, 0);
    let screen = emulator.screen(&PALETTE);
    assert_eq!((screen.columns, screen.lines), (2, 1));
    assert_eq!(clamp_size(0, 0), (2, 1));
    assert_eq!(clamp_size(1, 1), (2, 1));
    assert_eq!(clamp_size(u16::MAX, u16::MAX), (u16::MAX, u16::MAX));
    let tiny = Emulator::new(0, 0, 0).screen(&PALETTE);
    assert_eq!((tiny.columns, tiny.lines), (2, 1));
}

#[test]
fn bytes_that_are_no_sequence_do_not_break_it() {
    let mut emulator = Emulator::new(10, 3, 10);
    emulator.feed(b"\x1b[999999999999;99999999999H\x1b]\x07\xff\xfe\x1b[38;5;999mok");
    let screen = emulator.screen(&PALETTE);
    assert!(screen.text().contains("ok"), "{}", screen.text());
}
