use birchpad_terminal::{Key, Modifiers};
use birchpad_terminal_panel::*;
use gpui_kit::{Bounds, Keystroke, point, px, size};

fn keystroke(text: &'static str, typed: Option<&str>) -> Keystroke {
    let mut keystroke = Keystroke::parse(text).unwrap();
    keystroke.key_char = typed.map(str::to_owned);
    keystroke
}

fn key(key: Key, shift: bool, alt: bool, control: bool) -> Option<Input> {
    Some(Input::Key(
        key,
        Modifiers {
            shift,
            alt,
            control,
        },
    ))
}

#[test]
fn keystrokes_become_what_the_shell_gets() {
    assert_eq!(
        input_of(&keystroke("a", Some("a"))),
        key(Key::Char('a'), false, false, false)
    );
    // Shift is in the character typed already.
    assert_eq!(
        input_of(&keystroke("shift-a", Some("A"))),
        key(Key::Char('A'), false, false, false)
    );
    // Another layout: the character typed, not the key.
    assert_eq!(
        input_of(&keystroke("f", Some("а"))),
        key(Key::Char('а'), false, false, false)
    );
    // Ctrl and Alt with a letter: the key.
    assert_eq!(
        input_of(&keystroke("ctrl-c", None)),
        key(Key::Char('c'), false, false, true)
    );
    assert_eq!(
        input_of(&keystroke("ctrl-r", Some("r"))),
        key(Key::Char('r'), false, false, true)
    );
    assert_eq!(
        input_of(&keystroke("alt-x", None)),
        key(Key::Char('x'), false, true, false)
    );
    for (text, named) in [
        ("enter", Key::Enter),
        ("tab", Key::Tab),
        ("backspace", Key::Backspace),
        ("escape", Key::Escape),
        ("up", Key::Up),
        ("pagedown", Key::PageDown),
        ("delete", Key::Delete),
        ("insert", Key::Insert),
        ("space", Key::Char(' ')),
        ("f1", Key::F(1)),
        ("f12", Key::F(12)),
    ] {
        assert_eq!(
            input_of(&keystroke(text, None)),
            key(named, false, false, false),
            "{text}"
        );
    }
    assert_eq!(
        input_of(&keystroke("shift-tab", None)),
        key(Key::Tab, true, false, false)
    );
    assert_eq!(
        input_of(&keystroke("ctrl-up", None)),
        key(Key::Up, false, false, true)
    );
    // Pasting.
    assert_eq!(
        input_of(&keystroke("ctrl-shift-v", None)),
        Some(Input::Paste)
    );
    assert_eq!(
        input_of(&keystroke("shift-insert", None)),
        Some(Input::Paste)
    );
}

#[test]
fn the_windows_own_keys_stay_its_own() {
    for text in [
        "ctrl-tab",
        "ctrl-shift-tab",
        "ctrl-pageup",
        "ctrl-pagedown",
        "ctrl-`",
    ] {
        assert_eq!(input_of(&keystroke(text, None)), None, "{text}");
    }
    let cmd = input_of(&keystroke("cmd-v", None));
    if cfg!(target_os = "macos") {
        assert_eq!(cmd, Some(Input::Paste));
    } else {
        assert_eq!(cmd, None);
    }
    assert_eq!(input_of(&keystroke("cmd-n", None)), None);
    // Keys a terminal has nothing for.
    assert_eq!(input_of(&keystroke("f13", None)), None);
    assert_eq!(input_of(&keystroke("f0", None)), None);
}

#[test]
fn the_grid_a_panel_has_room_for() {
    let bounds =
        |width: f32, height: f32| Bounds::new(point(px(10.), px(20.)), size(px(width), px(height)));
    assert_eq!(grid_of(bounds(800., 200.), px(8.), px(16.)), (100, 12));
    assert_eq!(grid_of(bounds(807.9, 207.9), px(8.), px(16.)), (100, 12));
    // Too small: the smallest terminal.
    assert_eq!(grid_of(bounds(1., 1.), px(8.), px(16.)), (2, 1));
    assert_eq!(grid_of(bounds(0., 0.), px(8.), px(16.)), (2, 1));
    // Huge: at most what a size can say.
    assert_eq!(
        grid_of(bounds(1e9, 1e9), px(1.), px(1.)),
        (u16::MAX, u16::MAX)
    );
}

#[test]
fn the_palette_follows_the_theme() {
    let palette = palette();
    let theme = birchpad_widgets::theme::current();
    assert_eq!(palette.background, theme.editor.background.to_rgb());
    assert_eq!(palette.foreground, theme.editor.text.to_rgb());
}
