//! The built-in simple types and the values each takes.

use birchpad_i18n::t;

use crate::xml_schema::dtd::is_token;

pub(super) const BUILTINS: &[&str] = &[
    "string",
    "normalizedString",
    "token",
    "language",
    "Name",
    "NCName",
    "QName",
    "ID",
    "IDREF",
    "IDREFS",
    "ENTITY",
    "ENTITIES",
    "NMTOKEN",
    "NMTOKENS",
    "anyURI",
    "boolean",
    "decimal",
    "integer",
    "int",
    "long",
    "short",
    "byte",
    "nonNegativeInteger",
    "positiveInteger",
    "nonPositiveInteger",
    "negativeInteger",
    "unsignedLong",
    "unsignedInt",
    "unsignedShort",
    "unsignedByte",
    "float",
    "double",
    "date",
    "dateTime",
    "time",
    "duration",
    "gYear",
    "gYearMonth",
    "gMonth",
    "gMonthDay",
    "gDay",
    "hexBinary",
    "base64Binary",
    "anySimpleType",
];

/// Why `value` is not of the built-in type `name`, if it is not.
pub(super) fn builtin_problem(name: &str, value: &str) -> Option<String> {
    let value = value.trim();
    let integer = |min: i128, max: i128| -> bool {
        let digits = value.strip_prefix(['+', '-']).unwrap_or(value);
        !digits.is_empty()
            && digits.bytes().all(|b| b.is_ascii_digit())
            && value
                .parse::<i128>()
                .is_ok_and(|number| (min..=max).contains(&number))
    };
    let date = |value: &str| -> bool {
        let value = value.strip_prefix('-').unwrap_or(value);
        let value = strip_timezone(value);
        let parts: Vec<&str> = value.split('-').collect();
        parts.len() == 3
            && parts[0].len() >= 4
            && parts
                .iter()
                .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
            && parts[1].len() == 2
            && parts[2].len() == 2
            && (1..=12).contains(&parts[1].parse::<u32>().unwrap_or(0))
            && (1..=31).contains(&parts[2].parse::<u32>().unwrap_or(0))
    };
    let time = |value: &str| -> bool {
        let value = strip_timezone(value);
        let parts: Vec<&str> = value.split(':').collect();
        parts.len() == 3
            && parts[0].len() == 2
            && parts[1].len() == 2
            && parts[0].parse::<u32>().is_ok_and(|hours| hours <= 24)
            && parts[1].parse::<u32>().is_ok_and(|minutes| minutes < 60)
            && parts[2]
                .parse::<f64>()
                .is_ok_and(|seconds| (0.0..61.0).contains(&seconds))
            && parts[2].starts_with(|c: char| c.is_ascii_digit())
    };
    let ok = match name {
        "boolean" => matches!(value, "true" | "false" | "1" | "0"),
        "decimal" => {
            let digits = value.strip_prefix(['+', '-']).unwrap_or(value);
            let (whole, fraction) = digits.split_once('.').unwrap_or((digits, ""));
            !(whole.is_empty() && fraction.is_empty())
                && whole.bytes().all(|b| b.is_ascii_digit())
                && fraction.bytes().all(|b| b.is_ascii_digit())
        }
        "integer" => integer(i128::MIN, i128::MAX),
        "long" => integer(i64::MIN.into(), i64::MAX.into()),
        "int" => integer(i32::MIN.into(), i32::MAX.into()),
        "short" => integer(i16::MIN.into(), i16::MAX.into()),
        "byte" => integer(i8::MIN.into(), i8::MAX.into()),
        "nonNegativeInteger" => integer(0, i128::MAX),
        "positiveInteger" => integer(1, i128::MAX),
        "nonPositiveInteger" => integer(i128::MIN, 0),
        "negativeInteger" => integer(i128::MIN, -1),
        "unsignedLong" => integer(0, u64::MAX.into()),
        "unsignedInt" => integer(0, u32::MAX.into()),
        "unsignedShort" => integer(0, u16::MAX.into()),
        "unsignedByte" => integer(0, u8::MAX.into()),
        "float" | "double" => {
            matches!(value, "INF" | "-INF" | "NaN")
                || (value.parse::<f64>().is_ok() && !value.contains(['i', 'I', 'n', 'N']))
        }
        "date" => date(value),
        "time" => time(value),
        "dateTime" => value
            .split_once('T')
            .is_some_and(|(day, hour)| date(day) && time(hour)),
        "NMTOKEN" => is_token(value),
        "NMTOKENS" => value.split_whitespace().all(is_token) && !value.is_empty(),
        "Name" | "NCName" | "ID" | "IDREF" | "QName" => {
            is_token(value)
                && !value.starts_with(|c: char| c.is_ascii_digit() || c == '-' || c == '.')
                && (name == "Name" || name == "QName" || !value.contains(':'))
        }
        "hexBinary" => {
            value.len().is_multiple_of(2) && value.bytes().all(|b| b.is_ascii_hexdigit())
        }
        _ => true,
    };
    (!ok).then(|| t!("tools.xml-schema.not-of-type", value = value, kind = name))
}

pub(super) fn strip_timezone(value: &str) -> &str {
    if let Some(value) = value.strip_suffix('Z') {
        return value;
    }
    // `+hh:mm` or `-hh:mm` at the end.
    if value.len() > 6 {
        let (head, zone) = value.split_at(value.len() - 6);
        if zone.starts_with(['+', '-']) && zone.as_bytes()[3] == b':' {
            return head;
        }
    }
    value
}

/// An XML Schema pattern as a regular expression of the whole value.
pub(super) fn pattern_regex(pattern: &str) -> Option<fancy_regex::Regex> {
    // XML Schema's name classes, as near as plain classes come.
    let translated = pattern
        .replace("\\i", "[A-Za-z_:]")
        .replace("\\I", "[^A-Za-z_:]")
        .replace("\\c", "[-.0-9A-Za-z_:]")
        .replace("\\C", "[^-.0-9A-Za-z_:]");
    fancy_regex::Regex::new(&format!("^(?:{translated})$")).ok()
}
