//! Supported encodings: names, labels for the UI, the Encoding menu's character sets, and the
//! system's ANSI code page.

use birchpad_core::Encoding;

use crate::code_pages::CODE_PAGES;
use crate::codec::{Codec, code_page, code_page_for_label};

/// Legacy encodings offered in Encoding > Character Sets, grouped like Notepad++'s menu.
/// Each entry is `(name, label)`: a WHATWG name, or one of [`crate::code_pages`] (`IBM437`).
pub const CHARACTER_SETS: &[(&str, &[(&str, &str)])] = &[
    (
        "Arabic",
        &[
            ("ISO-8859-6", "ISO 8859-6"),
            ("windows-1256", "Windows-1256"),
            ("IBM864", "OEM 864"),
        ],
    ),
    (
        "Baltic",
        &[
            ("ISO-8859-4", "ISO 8859-4"),
            ("ISO-8859-13", "ISO 8859-13"),
            ("windows-1257", "Windows-1257"),
            ("IBM775", "OEM 775"),
        ],
    ),
    ("Celtic", &[("ISO-8859-14", "ISO 8859-14")]),
    (
        "Central European",
        &[
            ("ISO-8859-2", "ISO 8859-2"),
            ("windows-1250", "Windows-1250"),
            ("IBM852", "OEM 852"),
        ],
    ),
    (
        "Chinese",
        &[
            ("Big5", "Big5 (Traditional)"),
            ("GBK", "GBK (Simplified)"),
            ("gb18030", "GB18030"),
        ],
    ),
    (
        "Cyrillic",
        &[
            ("ISO-8859-5", "ISO 8859-5"),
            ("KOI8-R", "KOI8-R"),
            ("KOI8-U", "KOI8-U"),
            ("windows-1251", "Windows-1251"),
            ("IBM855", "OEM 855"),
            ("IBM866", "OEM 866"),
            ("x-mac-cyrillic", "Mac Cyrillic"),
        ],
    ),
    ("Eastern European", &[("ISO-8859-16", "ISO 8859-16")]),
    (
        "EBCDIC",
        &[
            ("IBM037", "EBCDIC 037 (US, Canada)"),
            ("IBM1140", "EBCDIC 1140 (US, Canada, Euro)"),
            ("IBM500", "EBCDIC 500 (International)"),
            ("IBM1026", "EBCDIC 1026 (Turkish)"),
        ],
    ),
    (
        "Greek",
        &[
            ("ISO-8859-7", "ISO 8859-7"),
            ("windows-1253", "Windows-1253"),
            ("IBM737", "OEM 737"),
            ("IBM869", "OEM 869"),
        ],
    ),
    (
        "Hebrew",
        &[
            ("ISO-8859-8", "ISO 8859-8"),
            ("ISO-8859-8-I", "ISO 8859-8-I"),
            ("windows-1255", "Windows-1255"),
            ("IBM862", "OEM 862"),
        ],
    ),
    (
        "Japanese",
        &[
            ("Shift_JIS", "Shift-JIS"),
            ("EUC-JP", "EUC-JP"),
            ("ISO-2022-JP", "ISO-2022-JP"),
        ],
    ),
    ("Korean", &[("EUC-KR", "EUC-KR (Windows-949)")]),
    (
        "North European",
        &[
            ("ISO-8859-10", "ISO 8859-10"),
            ("IBM861", "OEM 861 (Icelandic)"),
            ("IBM865", "OEM 865 (Nordic)"),
        ],
    ),
    ("Thai", &[("windows-874", "Windows-874")]),
    (
        "Turkish",
        &[
            ("ISO-8859-3", "ISO 8859-3"),
            ("windows-1254", "Windows-1254"),
            ("IBM857", "OEM 857"),
        ],
    ),
    (
        "Western European",
        &[
            ("windows-1252", "Windows-1252"),
            ("ISO-8859-15", "ISO 8859-15"),
            ("macintosh", "Macintosh"),
            ("IBM437", "OEM-US (437)"),
            ("IBM850", "OEM 850"),
            ("IBM858", "OEM 858"),
            ("IBM860", "OEM 860 (Portuguese)"),
            ("IBM863", "OEM 863 (French Canadian)"),
        ],
    ),
    ("Vietnamese", &[("windows-1258", "Windows-1258")]),
];

/// The decoder and encoder of an [`Encoding`].
///
/// # Panics
///
/// Panics if a legacy name is neither one encoding_rs knows nor one of [`crate::code_pages`];
/// [`Encoding::Legacy`] values are only created from those names.
pub(crate) fn to_codec(encoding: Encoding) -> Codec {
    match encoding {
        Encoding::Utf8 => Codec::Rs(encoding_rs::UTF_8),
        Encoding::Utf16Le => Codec::Rs(encoding_rs::UTF_16LE),
        Encoding::Utf16Be => Codec::Rs(encoding_rs::UTF_16BE),
        Encoding::Legacy(name) => match code_page(name) {
            Some(page) => Codec::Page(page),
            None => Codec::Rs(
                encoding_rs::Encoding::for_label(name.as_bytes())
                    .unwrap_or_else(|| panic!("unknown legacy encoding {name}")),
            ),
        },
    }
}

/// The [`Encoding`] for an encoding_rs encoding, or `None` for ones Birchpad cannot save
/// (`replacement`, `x-user-defined`).
pub(crate) fn from_encoding_rs(encoding: &'static encoding_rs::Encoding) -> Option<Encoding> {
    if encoding == encoding_rs::UTF_8 {
        Some(Encoding::Utf8)
    } else if encoding == encoding_rs::UTF_16LE {
        Some(Encoding::Utf16Le)
    } else if encoding == encoding_rs::UTF_16BE {
        Some(Encoding::Utf16Be)
    } else if encoding == encoding_rs::REPLACEMENT || encoding == encoding_rs::X_USER_DEFINED {
        None
    } else {
        Some(Encoding::Legacy(encoding.name()))
    }
}

/// Parses an encoding as used in commands and settings: `utf-8`, `utf-8-bom`, `utf-16le`,
/// `utf-16le-bom`, `utf-16be`, `utf-16be-bom`, `ansi`, or any WHATWG label of a legacy encoding
/// (`windows-1251`, `koi8-r`, `cp866`, `shift_jis`, ...). Returns the encoding and whether to
/// write a byte order mark.
pub fn parse_encoding(name: &str, ansi: Encoding) -> Option<(Encoding, bool)> {
    let lower = name.trim().to_ascii_lowercase();
    let parsed = match lower.as_str() {
        "utf-8" | "utf8" => (Encoding::Utf8, false),
        "utf-8-bom" | "utf8-bom" => (Encoding::Utf8, true),
        "utf-16le" | "utf-16" => (Encoding::Utf16Le, false),
        "utf-16le-bom" | "utf-16-bom" => (Encoding::Utf16Le, true),
        "utf-16be" => (Encoding::Utf16Be, false),
        "utf-16be-bom" => (Encoding::Utf16Be, true),
        "ansi" => (ansi, false),
        label => match code_page_for_label(label) {
            Some(page) => (Encoding::Legacy(page.name), false),
            None => {
                let encoding = encoding_rs::Encoding::for_label(label.as_bytes())?;
                (from_encoding_rs(encoding)?, false)
            }
        },
    };
    Some(parsed)
}

/// The canonical name of an encoding with BOM, accepted by [`parse_encoding`].
pub fn encoding_name(encoding: Encoding, bom: bool) -> String {
    let base = match encoding {
        Encoding::Utf8 => "utf-8",
        Encoding::Utf16Le => "utf-16le",
        Encoding::Utf16Be => "utf-16be",
        Encoding::Legacy(name) => return name.to_owned(),
    };
    if bom {
        format!("{base}-bom")
    } else {
        base.to_owned()
    }
}

/// The name shown in the status bar, as in Notepad++: `UTF-8`, `UTF-8-BOM`, `UTF-16 LE BOM`,
/// `Windows-1251`, `OEM 866`.
pub fn display_name(encoding: Encoding, bom: bool) -> String {
    match (encoding, bom) {
        (Encoding::Utf8, false) => "UTF-8".into(),
        (Encoding::Utf8, true) => "UTF-8-BOM".into(),
        (Encoding::Utf16Le, false) => "UTF-16 LE".into(),
        (Encoding::Utf16Le, true) => "UTF-16 LE BOM".into(),
        (Encoding::Utf16Be, false) => "UTF-16 BE".into(),
        (Encoding::Utf16Be, true) => "UTF-16 BE BOM".into(),
        (Encoding::Legacy(name), _) => CHARACTER_SETS
            .iter()
            .flat_map(|(_, sets)| sets.iter())
            .find(|(candidate, _)| candidate.eq_ignore_ascii_case(name))
            .map_or_else(|| name.to_owned(), |(_, label)| (*label).to_owned()),
    }
}

/// The legacy encoding of a Windows code page number, e.g. 1251 → `windows-1251`.
pub fn from_code_page(code_page: u32) -> Option<Encoding> {
    if let Some(page) = CODE_PAGES.iter().find(|page| page.number == code_page) {
        return Some(Encoding::Legacy(page.name));
    }
    let name = match code_page {
        866 => "IBM866",
        874 => "windows-874",
        932 => "Shift_JIS",
        936 => "GBK",
        949 => "EUC-KR",
        950 => "Big5",
        1250..=1258 => return Some(Encoding::Legacy(windows_125x(code_page))),
        10000 => "macintosh",
        10007 => "x-mac-cyrillic",
        20866 => "KOI8-R",
        21866 => "KOI8-U",
        28592..=28606 => return iso_8859(code_page - 28590),
        54936 => "gb18030",
        _ => return None,
    };
    to_known(name)
}

fn windows_125x(code_page: u32) -> &'static str {
    [
        "windows-1250",
        "windows-1251",
        "windows-1252",
        "windows-1253",
        "windows-1254",
        "windows-1255",
        "windows-1256",
        "windows-1257",
        "windows-1258",
    ][(code_page - 1250) as usize]
}

fn iso_8859(part: u32) -> Option<Encoding> {
    let label = format!("iso-8859-{part}");
    let encoding = encoding_rs::Encoding::for_label(label.as_bytes())?;
    from_encoding_rs(encoding).filter(|encoding| !encoding.is_unicode())
}

fn to_known(name: &str) -> Option<Encoding> {
    encoding_rs::Encoding::for_label(name.as_bytes()).and_then(from_encoding_rs)
}

/// The encoding used for "ANSI" files: the active code page on Windows, Windows-1252 elsewhere
/// (unless the user's settings name another one).
pub fn system_ansi() -> Encoding {
    platform_ansi().unwrap_or(Encoding::Legacy("windows-1252"))
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn platform_ansi() -> Option<Encoding> {
    // SAFETY: GetACP takes no arguments, has no preconditions and only returns a number.
    let code_page = unsafe { windows_sys::Win32::Globalization::GetACP() };
    // 65001 means the system runs with UTF-8 as its "ANSI" code page; legacy files then still
    // need a legacy fallback.
    from_code_page(code_page)
}

#[cfg(not(windows))]
fn platform_ansi() -> Option<Encoding> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_character_set_is_known() {
        for page in CODE_PAGES {
            let listed = CHARACTER_SETS
                .iter()
                .flat_map(|(_, sets)| sets.iter())
                .any(|(name, _)| *name == page.name);
            assert!(listed, "{} is in no group", page.name);
            assert_eq!(
                from_code_page(page.number),
                Some(Encoding::Legacy(page.name))
            );
            assert_eq!(
                parse_encoding(&format!("cp{}", page.number), Encoding::Utf8),
                Some((Encoding::Legacy(page.name), false))
            );
            assert!(to_codec(Encoding::Legacy(page.name)).is_single_byte());
        }
        for (_, sets) in CHARACTER_SETS {
            for (name, _) in *sets {
                if code_page(name).is_some() {
                    continue;
                }
                let encoding = encoding_rs::Encoding::for_label(name.as_bytes())
                    .unwrap_or_else(|| panic!("{name}"));
                assert_eq!(encoding.name(), *name, "use the canonical name");
                let ours = from_encoding_rs(encoding).unwrap();
                assert!(!ours.is_unicode());
                assert!(matches!(to_codec(ours), Codec::Rs(found) if found == encoding));
            }
        }
    }

    #[test]
    fn parses_command_names() {
        let ansi = Encoding::Legacy("windows-1251");
        assert_eq!(
            parse_encoding("utf-8-bom", ansi),
            Some((Encoding::Utf8, true))
        );
        assert_eq!(
            parse_encoding("UTF-16LE-BOM", ansi),
            Some((Encoding::Utf16Le, true))
        );
        assert_eq!(parse_encoding("ansi", ansi), Some((ansi, false)));
        assert_eq!(
            parse_encoding("cp866", ansi),
            Some((Encoding::Legacy("IBM866"), false))
        );
        assert_eq!(
            parse_encoding("koi8-r", ansi),
            Some((Encoding::Legacy("KOI8-R"), false))
        );
        assert_eq!(parse_encoding("replacement", ansi), None);
        assert_eq!(parse_encoding("klingon", ansi), None);
        for (encoding, bom) in [
            (Encoding::Utf8, false),
            (Encoding::Utf8, true),
            (Encoding::Utf16Be, true),
            (Encoding::Legacy("Shift_JIS"), false),
        ] {
            assert_eq!(
                parse_encoding(&encoding_name(encoding, bom), ansi),
                Some((encoding, bom))
            );
        }
    }

    #[test]
    fn display_names_follow_notepad_plus_plus() {
        assert_eq!(display_name(Encoding::Utf8, true), "UTF-8-BOM");
        assert_eq!(display_name(Encoding::Utf16Le, true), "UTF-16 LE BOM");
        assert_eq!(display_name(Encoding::Legacy("IBM866"), false), "OEM 866");
        assert_eq!(
            display_name(Encoding::Legacy("windows-1251"), false),
            "Windows-1251"
        );
    }

    #[test]
    fn maps_windows_code_pages() {
        assert_eq!(from_code_page(1251), Some(Encoding::Legacy("windows-1251")));
        assert_eq!(from_code_page(866), Some(Encoding::Legacy("IBM866")));
        assert_eq!(from_code_page(932), Some(Encoding::Legacy("Shift_JIS")));
        assert_eq!(from_code_page(28595), Some(Encoding::Legacy("ISO-8859-5")));
        assert_eq!(from_code_page(65001), None);
        assert!(!system_ansi().is_unicode());
    }

    #[test]
    fn code_pages_at_the_ends_of_their_ranges() {
        // Windows-1250 to 1258, and nothing either side.
        assert_eq!(from_code_page(1250), Some(Encoding::Legacy("windows-1250")));
        assert_eq!(from_code_page(1258), Some(Encoding::Legacy("windows-1258")));
        assert_eq!(from_code_page(1249), None);
        assert_eq!(from_code_page(1259), None);
        // ISO 8859-2 to 16; part 12 was never published, and part 1 is Windows-1252 in WHATWG.
        assert_eq!(from_code_page(28592), Some(Encoding::Legacy("ISO-8859-2")));
        assert_eq!(from_code_page(28606), Some(Encoding::Legacy("ISO-8859-16")));
        assert_eq!(from_code_page(28602), None);
        assert_eq!(from_code_page(28591), None);
        assert_eq!(from_code_page(28607), None);
        // The others one by one, and numbers that are no code page.
        for (code_page, name) in [
            (874, "windows-874"),
            (936, "GBK"),
            (949, "EUC-KR"),
            (950, "Big5"),
            (10000, "macintosh"),
            (10007, "x-mac-cyrillic"),
            (20866, "KOI8-R"),
            (21866, "KOI8-U"),
            (54936, "gb18030"),
            // DOS and EBCDIC code pages (Birchpad's own tables).
            (437, "IBM437"),
            (37, "IBM037"),
            (1140, "IBM1140"),
        ] {
            assert_eq!(
                from_code_page(code_page),
                Some(Encoding::Legacy(name)),
                "{code_page}"
            );
        }
        for nothing in [0, 1, 436, 65000, u32::MAX] {
            assert_eq!(from_code_page(nothing), None, "{nothing}");
        }
    }

    #[test]
    fn command_names_have_aliases_and_ignore_case_and_spaces() {
        let ansi = Encoding::Legacy("windows-1252");
        for (name, expected) in [
            ("utf8", (Encoding::Utf8, false)),
            ("utf8-bom", (Encoding::Utf8, true)),
            (" UTF-8 ", (Encoding::Utf8, false)),
            ("utf-16", (Encoding::Utf16Le, false)),
            ("utf-16-bom", (Encoding::Utf16Le, true)),
            ("utf-16le", (Encoding::Utf16Le, false)),
            ("utf-16be", (Encoding::Utf16Be, false)),
            ("ANSI", (ansi, false)),
            ("Windows-1251", (Encoding::Legacy("windows-1251"), false)),
        ] {
            assert_eq!(parse_encoding(name, ansi), Some(expected), "{name:?}");
        }
        // Nothing, a BOM on what has none, and encodings Birchpad cannot save.
        for name in ["", "  ", "ansi-bom", "windows-1251-bom", "x-user-defined"] {
            assert_eq!(parse_encoding(name, ansi), None, "{name:?}");
        }
    }

    #[test]
    fn every_unicode_form_has_its_display_name() {
        let names = [
            ((Encoding::Utf8, false), "UTF-8"),
            ((Encoding::Utf8, true), "UTF-8-BOM"),
            ((Encoding::Utf16Le, false), "UTF-16 LE"),
            ((Encoding::Utf16Le, true), "UTF-16 LE BOM"),
            ((Encoding::Utf16Be, false), "UTF-16 BE"),
            ((Encoding::Utf16Be, true), "UTF-16 BE BOM"),
        ];
        for ((encoding, bom), name) in names {
            assert_eq!(display_name(encoding, bom), name);
            assert_eq!(
                parse_encoding(&encoding_name(encoding, bom), encoding),
                Some((encoding, bom))
            );
        }
        // A legacy encoding has no BOM to show, and one outside the menus keeps its name.
        assert_eq!(
            display_name(Encoding::Legacy("windows-1251"), true),
            "Windows-1251"
        );
        assert_eq!(display_name(Encoding::Legacy("no-such"), false), "no-such");
    }

    #[test]
    fn encoding_rs_round_trip() {
        for encoding in [
            Encoding::Utf8,
            Encoding::Utf16Le,
            Encoding::Utf16Be,
            Encoding::Legacy("windows-1251"),
        ] {
            let Codec::Rs(found) = to_codec(encoding) else {
                panic!("{encoding:?} is encoding_rs's");
            };
            assert_eq!(from_encoding_rs(found), Some(encoding));
        }
        assert_eq!(from_encoding_rs(encoding_rs::REPLACEMENT), None);
        assert_eq!(from_encoding_rs(encoding_rs::X_USER_DEFINED), None);
    }

    #[test]
    #[should_panic(expected = "unknown legacy encoding klingon")]
    fn an_unknown_legacy_encoding_is_a_bug() {
        to_codec(Encoding::Legacy("klingon"));
    }
}
