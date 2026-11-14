use birchpad_core::Encoding;
use birchpad_panels::character_panel::*;

const WESTERN: Encoding = Encoding::Legacy("windows-1252");
const CYRILLIC: Encoding = Encoding::Legacy("windows-1251");

#[test]
fn ascii_and_controls() {
    let rows = rows(WESTERN);
    assert_eq!(rows.len(), 256);
    assert_eq!((rows[0].shown.as_str(), rows[0].ch), ("NUL", Some('\0')));
    assert_eq!(rows[31].shown, "US");
    assert_eq!((rows[32].shown.as_str(), rows[32].name), (" ", None));
    assert_eq!(rows[38].html_name().as_deref(), Some("&amp;"));
    assert_eq!(rows[65].html_decimal().as_deref(), Some("&#65;"));
    assert_eq!(rows[65].html_hex().as_deref(), Some("&#x41;"));
    assert_eq!(
        (rows[127].shown.as_str(), rows[127].ch),
        ("DEL", Some('\u{7F}'))
    );
    assert_eq!(rows[127].html_name(), None);
}

#[test]
fn the_upper_half_follows_the_code_page() {
    let western = rows(WESTERN);
    assert_eq!(western[0x80].shown, "€");
    assert_eq!(western[0x80].html_name().as_deref(), Some("&euro;"));
    assert_eq!(western[0x80].html_hex().as_deref(), Some("&#x20AC;"));
    assert_eq!(western[0xA0].html_name().as_deref(), Some("&nbsp;"));
    assert_eq!(western[0xFF].html_name().as_deref(), Some("&yuml;"));
    let cyrillic = rows(CYRILLIC);
    assert_eq!(cyrillic[0xC0].shown, "А");
    assert_eq!(cyrillic[0xC0].html_name(), None, "HTML 4 names no Cyrillic");
    assert_eq!(cyrillic[0xC0].html_decimal().as_deref(), Some("&#1040;"));
    // A value the code page leaves undefined has nothing to insert.
    let greek = rows(Encoding::Legacy("windows-1253"));
    let undefined = &greek[0xAA];
    assert_eq!((undefined.ch, undefined.shown.as_str()), (None, ""));
    assert_eq!(undefined.html_decimal(), None);
}

#[test]
fn entity_names_cover_latin_1_and_windows_1252() {
    assert_eq!(entity_name('"'), Some("quot"));
    assert_eq!(entity_name('<'), Some("lt"));
    assert_eq!(entity_name('\u{A0}'), Some("nbsp"));
    assert_eq!(entity_name('\u{FF}'), Some("yuml"));
    assert_eq!(entity_name('\u{178}'), Some("Yuml"));
    for nameless in ['a', '\u{9F}', '\u{100}', 'Ж', '\''] {
        assert_eq!(entity_name(nameless), None, "{nameless:?}");
    }
}
