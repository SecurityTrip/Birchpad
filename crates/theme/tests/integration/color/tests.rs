use birchpad_theme::*;

#[test]
fn reads_and_writes_both_forms() {
    assert_eq!("#0000FF".parse(), Ok(Color(0x0000ffff)));
    assert_eq!("#3390ff40".parse(), Ok(Color(0x3390ff40)));
    assert_eq!(Color(0x0000ffff).to_string(), "#0000ff");
    assert_eq!(Color(0x3390ff40).to_string(), "#3390ff40");
    // Opaque written with its alpha reads as the short form.
    assert_eq!("#123456ff".parse::<Color>().unwrap().to_string(), "#123456");
}

#[test]
fn refuses_what_is_not_a_color() {
    for text in [
        "",
        "#",
        "0000ff",
        "#00f",
        "#0000f",
        "#0000fff",
        "#0000ffff0",
        "#00000g",
        "#+00000",
        "#-0000f",
        " #0000ff",
        "#０００",
    ] {
        assert!(text.parse::<Color>().is_err(), "{text:?}");
    }
}

#[test]
fn components_and_brightness() {
    let color = Color::rgb(0x123456);
    assert_eq!(color.to_rgb(), 0x123456);
    assert_eq!(color.alpha(), 0xff);
    assert_eq!(color.with_alpha(0x40), Color(0x12345640));
    assert_eq!(color.with_alpha(0).alpha(), 0);
    assert!(Color::rgb(0x000000).is_dark());
    assert!(Color::rgb(0x1e1e1e).is_dark());
    assert!(!Color::rgb(0xffffff).is_dark());
    assert!(!Color::rgb(0xfdf6e3).is_dark());
    // Middle grey: 0x80 is just over the line, 0x7f just under it.
    assert!(!Color::rgb(0x808080).is_dark());
    assert!(Color::rgb(0x7f7f7f).is_dark());
}

#[test]
fn a_tint_moves_towards_the_hue_and_keeps_the_brightness() {
    let grey = Color::rgb(0x404040);
    let red = grey.tinted(0., 0.5);
    let [r, g, b, a] = red.0.to_be_bytes();
    assert!(r > g && r > b, "{red}");
    assert_eq!(a, 0xff);
    let level = |c: Color| {
        let [r, g, b, _] = c.0.to_be_bytes();
        (u32::from(r) + u32::from(g) + u32::from(b)) / 3
    };
    assert!(level(red).abs_diff(level(grey)) <= 2);
    // None of it, or past all of it.
    assert_eq!(grey.tinted(120., 0.), grey);
    let blue = grey.tinted(220., 5.);
    assert_eq!(blue, grey.tinted(220., 1.));
    // The opacity stays.
    assert_eq!(Color(0x40404080).tinted(0., 0.5).alpha(), 0x80);
    // Black stays black.
    assert_eq!(Color::rgb(0).tinted(0., 1.), Color::rgb(0));
}

#[test]
fn tones_tint_the_interface_s_surfaces_only() {
    let ui = Theme::builtin(DARK).unwrap().ui;
    let toned = ui.toned(120.);
    assert_ne!(toned.background, ui.background);
    assert_ne!(toned.surface, ui.surface);
    assert_eq!(toned.text, ui.text);
    assert_eq!(toned.accent, ui.accent);
}

#[test]
fn a_customized_tone_sets_the_colors_given() {
    let ui = Theme::builtin(DARK).unwrap().ui;
    let custom = ui.customized([("background", "#102030"), ("text", "#f0e0d0")]);
    assert_eq!(custom.background, Color::rgb(0x102030));
    assert_eq!(custom.text, Color::rgb(0xf0e0d0));
    assert_eq!(custom.accent, ui.accent, "the others stay");
    // Nothing given, nothing changes.
    assert_eq!(ui.customized([]), ui);
}

#[test]
fn a_customized_tone_passes_over_what_is_no_color() {
    let ui = Theme::builtin(DARK).unwrap().ui;
    let custom = ui.customized([
        ("background", "blue"),
        ("text", "#12345"),
        ("", "#102030"),
        ("no-such-part", "#102030"),
        ("surface", "#102030"),
    ]);
    assert_eq!(custom.background, ui.background);
    assert_eq!(custom.text, ui.text);
    assert_eq!(custom.surface, Color::rgb(0x102030));
}
