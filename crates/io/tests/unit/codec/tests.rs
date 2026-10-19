use super::*;

fn page(name: &str) -> Codec {
    Codec::Page(code_page(name).unwrap())
}

#[test]
fn known_characters() {
    let (text, errors) = page("IBM437").decode_without_bom_handling(&[b'A', 0x82, 0xC9, 0xDB]);
    assert_eq!(text, "Aé╔█");
    assert!(!errors);
    // EBCDIC: "Hi" is 0xC8 0x89.
    let (text, _) = page("IBM037").decode_without_bom_handling(&[0xC8, 0x89, 0x25]);
    assert_eq!(text, "Hi\n");
    assert_eq!(page("IBM037").encode("Hi\n").0.as_ref(), [0xC8, 0x89, 0x25]);
    assert_eq!(page("IBM1140").encode("€").0.as_ref(), [0x9F]);
}

#[test]
fn undefined_bytes_and_unmappable_characters() {
    let ibm857 = code_page("IBM857").unwrap();
    let undefined = (0..=255u8)
        .find(|&byte| char_of(ibm857, byte).is_none())
        .unwrap();
    let codec = Codec::Page(ibm857);
    let input = [b'a', undefined];
    let (text, errors) = codec.decode_without_bom_handling(&input);
    assert!(errors);
    assert_eq!(text, "a\u{FFFD}");
    assert_eq!(
        codec.decode_without_bom_handling_and_without_replacement(&[undefined]),
        None
    );
    let mut decoder = codec.new_decoder_without_bom_handling();
    let mut out = String::with_capacity(16);
    let (result, read) =
        decoder.decode_to_string_without_replacement(&[b'a', undefined, b'b'], &mut out, true);
    assert_eq!(
        (result, read, out.as_str()),
        (DecoderResult::Malformed(1, 0), 2, "a")
    );
    let (bytes, unmappable) = page("IBM437").encode("a€");
    assert!(unmappable);
    assert_eq!(bytes.as_ref(), b"a?");
    let mut encoder = page("IBM437").new_encoder();
    let mut out = Vec::with_capacity(8);
    let (result, read) = encoder.encode_from_utf8_to_vec_without_replacement("a€b", &mut out, true);
    assert_eq!(
        (result, read, out.as_slice()),
        (EncoderResult::Unmappable('€'), 4, b"a".as_slice())
    );
}

#[test]
fn full_buffers_stop_and_resume() {
    let codec = page("IBM850");
    let mut decoder = codec.new_decoder_without_bom_handling();
    let mut out = String::with_capacity(1);
    let capacity = out.capacity();
    let input = vec![0x82; capacity + 1];
    let (result, read) = decoder.decode_to_string_without_replacement(&input, &mut out, true);
    assert_eq!(result, DecoderResult::OutputFull);
    assert!(read < input.len());
    let mut encoder = codec.new_encoder();
    let mut bytes = Vec::new();
    let (result, read) = encoder.encode_from_utf8_to_vec_without_replacement("é", &mut bytes, true);
    assert_eq!((result, read), (EncoderResult::OutputFull, 0));
    assert_eq!(
        encoder.max_buffer_length_from_utf8_without_replacement(5),
        Some(5)
    );
    assert_eq!(
        decoder.max_utf8_buffer_length_without_replacement(5),
        Some(15)
    );
    assert_eq!(
        decoder.max_utf8_buffer_length_without_replacement(usize::MAX),
        None
    );
}

#[test]
fn every_page_round_trips_its_defined_bytes() {
    for page in CODE_PAGES {
        let codec = Codec::Page(page);
        assert!(codec.is_single_byte() && !codec.is_iso_2022_jp());
        let bytes: Vec<u8> = (0..=255u8)
            .filter(|&byte| char_of(page, byte).is_some())
            .collect();
        let text = codec
            .decode_without_bom_handling_and_without_replacement(&bytes)
            .unwrap();
        assert_eq!(
            codec.encode(&text),
            (Cow::Borrowed(bytes.as_slice()), false),
            "{}",
            page.name
        );
    }
}

#[test]
fn labels() {
    for label in ["IBM437", "ibm437", "cp437", "CP437", "ibm-437", " cp437 "] {
        assert_eq!(
            code_page_for_label(label).map(|page| page.name),
            Some("IBM437"),
            "{label}"
        );
    }
    for label in ["", "437", "cp", "cp99999", "ibm-x", "windows-437", "cp866"] {
        assert!(code_page_for_label(label).is_none(), "{label}");
    }
    assert!(code_page("ibm1140").is_some());
    assert!(code_page("IBM866").is_none(), "encoding_rs has it");
}
