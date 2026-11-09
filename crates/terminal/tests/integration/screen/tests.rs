use birchpad_terminal::*;

#[test]
fn the_256_colors_of_xterm() {
    for palette in [Palette::LIGHT, Palette::DARK] {
        assert_eq!(palette.indexed(0), palette.ansi[0]);
        assert_eq!(palette.indexed(15), palette.ansi[15]);
        // The cube's corners and a red.
        assert_eq!(palette.indexed(16), 0x000000);
        assert_eq!(palette.indexed(231), 0xffffff);
        assert_eq!(palette.indexed(196), 0xff0000);
        assert_eq!(palette.indexed(21), 0x0000ff);
        assert_eq!(palette.indexed(17), 0x00005f);
        // The grays, from 8 to 238.
        assert_eq!(palette.indexed(232), 0x080808);
        assert_eq!(palette.indexed(255), 0xeeeeee);
    }
    assert_ne!(Palette::LIGHT.background, Palette::DARK.background);
}

#[test]
fn the_text_of_a_screen() {
    let mut emulator = Emulator::new(10, 4, 0);
    emulator.feed(b"ab  \r\n\r\ncd");
    let screen = emulator.screen(&Palette::DARK);
    // Trailing spaces and empty rows at the end go; empty rows between stay.
    assert_eq!(screen.text(), "ab\n\ncd");
    assert_eq!(screen.rows.len(), 4);
    assert!(screen.rows.iter().all(|row| row.len() == 10));
    let empty = Emulator::new(10, 4, 0).screen(&Palette::DARK);
    assert_eq!(empty.text(), "");
}
