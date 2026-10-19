use super::*;
use proptest::prelude::*;

#[test]
fn base64() {
    assert_eq!(
        encode_text("Hello, мир!", Conversion::Base64Encode).unwrap(),
        "SGVsbG8sINC80LjRgCE="
    );
    assert_eq!(
        encode_text("SGVsbG8sINC80LjRgCE=", Conversion::Base64Decode).unwrap(),
        "Hello, мир!"
    );
    // Line breaks inside are fine, as mail wraps Base64.
    assert_eq!(
        encode_text("SGVs\r\nbG8=", Conversion::Base64Decode).unwrap(),
        "Hello"
    );
    assert_eq!(encode_text("", Conversion::Base64Encode).unwrap(), "");
    assert_eq!(encode_text("", Conversion::Base64Decode).unwrap(), "");
    for bad in ["SGVsbG8", "S", "****", "SGVsbG8=="] {
        assert_eq!(
            encode_text(bad, Conversion::Base64Decode),
            Err(EncodeError::Base64),
            "{bad}"
        );
    }
    // Bytes that are not UTF-8.
    assert_eq!(
        encode_text("/w==", Conversion::Base64Decode),
        Err(EncodeError::NotText)
    );
}

#[test]
fn url() {
    assert_eq!(
        encode_text("a b&c=д/~", Conversion::UrlEncode).unwrap(),
        "a%20b%26c%3D%D0%B4%2F~"
    );
    assert_eq!(
        encode_text("a%20b%26c%3d%D0%B4+", Conversion::UrlDecode).unwrap(),
        "a b&c=д+"
    );
    assert_eq!(encode_text("", Conversion::UrlDecode).unwrap(), "");
    for bad in ["%", "%2", "%zz", "a%2g", "%д"] {
        assert!(
            matches!(
                encode_text(bad, Conversion::UrlDecode),
                Err(EncodeError::Url(_))
            ),
            "{bad}"
        );
    }
    assert_eq!(
        encode_text("%FF", Conversion::UrlDecode),
        Err(EncodeError::NotText)
    );
}

#[test]
fn hex() {
    assert_eq!(
        encode_text("Hi\n", Conversion::AsciiToHex).unwrap(),
        "48690A"
    );
    assert_eq!(encode_text("д", Conversion::AsciiToHex).unwrap(), "D0B4");
    assert_eq!(
        encode_text("48 69\n0a", Conversion::HexToAscii).unwrap(),
        "Hi\n"
    );
    assert_eq!(encode_text("", Conversion::HexToAscii).unwrap(), "");
    assert!(matches!(
        encode_text("486", Conversion::HexToAscii),
        Err(EncodeError::Hex(_))
    ));
    assert!(matches!(
        encode_text("4G", Conversion::HexToAscii),
        Err(EncodeError::Hex(_))
    ));
    assert!(matches!(
        encode_text("0x48", Conversion::HexToAscii),
        Err(EncodeError::Hex(_))
    ));
    assert_eq!(
        encode_text("FF", Conversion::HexToAscii),
        Err(EncodeError::NotText)
    );
}

#[test]
fn selections_convert_in_one_step() {
    let text = Rope::from_str("ab cd ef");
    let selection = Selection::new(vec![Span::new(0, 2), Span::point(3), Span::new(8, 6)], 0);
    let transaction = encode_selections(&text, &selection, Conversion::AsciiToHex)
        .unwrap()
        .unwrap();
    let mut result = text.clone();
    transaction.changes().apply(&mut result);
    assert_eq!(result.to_string(), "6162 cd 6566");
    let after = transaction.selection().unwrap();
    let spans: Vec<_> = after
        .iter()
        .map(|range| (range.anchor, range.head))
        .collect();
    assert_eq!(spans, [(0, 4), (5, 5), (12, 8)]);
    // Nothing selected, and a range that does not convert.
    assert_eq!(
        encode_selections(&text, &Selection::point(3), Conversion::AsciiToHex),
        Ok(None)
    );
    let bad = Selection::new(vec![Span::new(0, 2), Span::new(3, 5)], 0);
    assert!(encode_selections(&text, &bad, Conversion::HexToAscii).is_err());
}

proptest! {
    #[test]
    fn encodings_round_trip(text in "\\PC*") {
        for (to, back) in [
            (Conversion::Base64Encode, Conversion::Base64Decode),
            (Conversion::UrlEncode, Conversion::UrlDecode),
            (Conversion::AsciiToHex, Conversion::HexToAscii),
        ] {
            let encoded = encode_text(&text, to).unwrap();
            prop_assert!(encoded.is_ascii());
            prop_assert_eq!(encode_text(&encoded, back).unwrap(), text.clone());
        }
    }
}
