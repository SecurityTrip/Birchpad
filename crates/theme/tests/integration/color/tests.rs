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
