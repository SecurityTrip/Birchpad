use birchpad_config::*;

fn resolved(toml: &str) -> ResolvedSettings {
    resolve(Sources {
        user: Some(toml::from_str(toml).unwrap()),
        ..Sources::default()
    })
}

#[test]
fn printing_starts_with_line_numbers_on_a4() {
    let print = PrintSettings::default();
    assert!(print.line_numbers && !print.landscape);
    assert_eq!((print.paper, print.colors), (Paper::A4, PrintColors::Wysiwyg));
    assert_eq!(
        (
            print.margin_top,
            print.margin_bottom,
            print.margin_left,
            print.margin_right
        ),
        (15, 15, 15, 15)
    );
    assert_eq!(print.font_size, 10);
    assert!(print.header_left.is_empty() && print.footer_right.is_empty());
}

#[test]
fn print_settings_read_from_the_file() {
    let settings = resolved(
        "[print]\nline-numbers = false\ncolors = \"black-on-white\"\npaper = \"letter\"\n\
         landscape = true\nmargin-left = 0\nfont-size = 72\n\
         footer-center = \"$(CURRENT_PRINTING_PAGE)\"",
    );
    assert!(settings.diagnostics.is_empty(), "{:?}", settings.diagnostics);
    let print = settings.settings.print;
    assert!(!print.line_numbers && print.landscape);
    assert_eq!(print.colors, PrintColors::BlackOnWhite);
    assert_eq!(print.paper, Paper::Letter);
    assert_eq!((print.margin_left, print.font_size), (0, 72));
    assert_eq!(print.footer_center, "$(CURRENT_PRINTING_PAGE)");
    for (key, value) in [
        ("colors", "\"no-background\""),
        ("colors", "\"invert\""),
        ("paper", "\"a3\""),
        ("paper", "\"a5\""),
        ("paper", "\"legal\""),
    ] {
        let settings = resolved(&format!("[print]\n{key} = {value}"));
        assert!(settings.diagnostics.is_empty(), "{key} = {value}");
    }
}

#[test]
fn wrong_print_settings_are_reported_and_left_at_their_defaults() {
    for (key, value) in [
        ("colors", "\"rainbow\""),
        ("paper", "\"a0\""),
        ("margin-top", "-1"),
        ("font-size", "\"big\""),
        ("line-numbers", "1"),
    ] {
        let settings = resolved(&format!("[print]\n{key} = {value}"));
        assert_eq!(settings.diagnostics.len(), 1, "{key} = {value}");
        assert_eq!(settings.settings.print, PrintSettings::default());
    }
}
