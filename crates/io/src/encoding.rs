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
            ("IBM875", "EBCDIC 875 (Greek)"),
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
#[path = "../tests/unit/encoding/tests.rs"]
mod tests;
