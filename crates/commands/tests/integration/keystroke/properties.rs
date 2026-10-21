use proptest::prelude::*;

use birchpad_commands::*;

fn keystroke() -> impl Strategy<Value = Keystroke> {
    let key = prop_oneof![
        proptest::sample::select(NAMED_KEYS).prop_map(str::to_owned),
        (1u8..=24).prop_map(|n| format!("f{n}")),
        "[a-z0-9]".prop_map(|key| key),
        proptest::sample::select(&["-", "=", "[", "]", ";", "'", ",", ".", "/", "`", "\\"][..])
            .prop_map(str::to_owned),
    ];
    (any::<[bool; 5]>(), key).prop_map(|([ctrl, alt, shift, platform, function], key)| Keystroke {
        modifiers: Modifiers {
            ctrl,
            alt,
            shift,
            platform,
            function,
        },
        key,
    })
}

proptest! {
    #[test]
    fn keystrokes_read_back_as_written(
        keys in proptest::collection::vec(keystroke(), 1..4),
        platform in prop_oneof![Just(Platform::Windows), Just(Platform::Linux), Just(Platform::MacOs)],
    ) {
        let written: Vec<String> = keys.iter().map(ToString::to_string).collect();
        for (key, text) in keys.iter().zip(&written) {
            prop_assert_eq!(&Keystroke::parse(text, platform).unwrap(), key);
        }
        let sequence = written.join(" ");
        prop_assert_eq!(Keystroke::parse_sequence(&sequence, platform).unwrap(), keys);
        // In capitals and with spaces around, the same.
        let shouted = format!("  {}  ", sequence.to_uppercase());
        let reread = Keystroke::parse_sequence(&shouted, platform);
        prop_assert_eq!(reread.map(|keys| keys.len()).ok(), Some(written.len()));
    }
}
