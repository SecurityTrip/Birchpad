use birchpad_core::Encoding;
use birchpad_io::*;

const ANSI: Encoding = Encoding::Legacy("windows-1252");

fn detect_bytes(bytes: &[u8]) -> Detection {
    detect(bytes, ANSI)
}

fn encode_legacy(text: &str, name: &str) -> Vec<u8> {
    let (bytes, _, errors) = encoding_rs::Encoding::for_label(name.as_bytes())
        .unwrap()
        .encode(text);
    assert!(!errors);
    bytes.into_owned()
}

const RUSSIAN: &str = "Съешь же ещё этих мягких французских булок, да выпей чаю. \
        В чащах юга жил бы цитрус? Да, но фальшивый экземпляр! \
        Широкая электрификация южных губерний даст мощный толчок подъёму сельского хозяйства.";

#[test]
fn boms_win() {
    assert_eq!(detect_bytes(b"\xEF\xBB\xBF").encoding, Encoding::Utf8);
    assert_eq!(detect_bytes(b"\xFF\xFEa\0").by, DetectedBy::Bom);
    assert_eq!(detect_bytes(b"\xFE\xFF\0a").encoding, Encoding::Utf16Be);
}

#[test]
fn plain_and_empty_text_is_utf8() {
    assert_eq!(detect_bytes(b"").encoding, Encoding::Utf8);
    assert_eq!(detect_bytes(b"hello\r\n").encoding, Encoding::Utf8);
    assert_eq!(
        detect_bytes(RUSSIAN.as_bytes()),
        Detection {
            encoding: Encoding::Utf8,
            by: DetectedBy::Utf8
        }
    );
    // NUL bytes alone do not make UTF-16.
    assert_eq!(detect_bytes(b"a\0\0\0b\0c").encoding, Encoding::Utf8);
}

#[test]
fn utf16_without_bom_by_zero_bytes() {
    let le: Vec<u8> = "Hello, world\r\n"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    let be: Vec<u8> = "Hello, world\r\n"
        .encode_utf16()
        .flat_map(u16::to_be_bytes)
        .collect();
    assert_eq!(
        detect_bytes(&le),
        Detection {
            encoding: Encoding::Utf16Le,
            by: DetectedBy::Utf16Pattern
        }
    );
    assert_eq!(detect_bytes(&be).encoding, Encoding::Utf16Be);
}

#[test]
fn legacy_cyrillic_is_guessed() {
    for name in ["windows-1251", "KOI8-R", "IBM866"] {
        let bytes = encode_legacy(RUSSIAN, name);
        let detection = detect_bytes(&bytes);
        assert_eq!(detection.encoding, Encoding::Legacy(name), "{name}");
        assert_eq!(detection.by, DetectedBy::Guess);
    }
}

#[test]
fn system_code_page_steers_the_guess() {
    // Too little text for chardetng alone; a Japanese system makes Shift_JIS the bet.
    let bytes = encode_legacy("いろはにほへと ちりぬるを わかよたれそ", "Shift_JIS");
    let japanese = Encoding::Legacy("Shift_JIS");
    assert_eq!(detect(&bytes, japanese).encoding, japanese);
}

#[test]
fn little_evidence_falls_back_to_ansi() {
    let bytes = encode_legacy("Café crème", "windows-1252");
    assert_eq!(
        detect_bytes(&bytes),
        Detection {
            encoding: ANSI,
            by: DetectedBy::Ansi
        }
    );
    // With a Cyrillic ANSI code page, short Cyrillic text stays Windows-1251.
    let bytes = encode_legacy("Привет", "windows-1251");
    let cyrillic = Encoding::Legacy("windows-1251");
    assert_eq!(detect(&bytes, cyrillic).encoding, cyrillic);
}
