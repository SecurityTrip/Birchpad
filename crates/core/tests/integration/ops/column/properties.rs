use proptest::prelude::*;

use birchpad_core::ops::*;

proptest! {
    #[test]
    fn the_numbering_reads_back(
        initial in -100_000i64..100_000,
        step in -1_000i64..1_000,
        repeat in 0usize..5,
        rows in 0usize..60,
        leading in prop_oneof![Just(Leading::None), Just(Leading::Zeros), Just(Leading::Spaces)],
        base in prop_oneof![Just(Base::Dec), Just(Base::Hex), Just(Base::Oct), Just(Base::Bin)],
        uppercase: bool,
    ) {
        let sequence = NumberSequence { initial, step, repeat, leading, base, uppercase };
        let numbers = sequence.generate(rows);
        prop_assert_eq!(numbers.len(), rows);
        // All as wide as the widest.
        if let Some(first) = numbers.first() {
            prop_assert!(numbers.iter().all(|number| number.len() == first.len()));
        }
        for (row, number) in numbers.iter().enumerate() {
            let expected = initial + step * (row / repeat.max(1)) as i64;
            prop_assert_eq!(parse_number(number, base), Some(expected), "{:?}", number);
            let digits = number.trim();
            prop_assert_eq!(digits.chars().any(|ch| ch.is_ascii_lowercase()), base == Base::Hex && !uppercase && digits.chars().any(|ch| ch.is_ascii_alphabetic()));
        }
    }
}
