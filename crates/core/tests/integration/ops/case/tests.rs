use crate::ops_test_support::run;
use birchpad_core::ops::*;

#[test]
fn conversions() {
    let case = |marked: &str, case| run(marked, |t, s| convert_case(t, s, case));
    assert_eq!(case("[Hello Мир]!", Case::Upper), "[HELLO МИР]!");
    assert_eq!(case("[Hello Мир]!", Case::Lower), "[hello мир]!");
    assert_eq!(case("[hELLO wORLD-x]", Case::Proper), "[Hello World-X]");
    assert_eq!(case("[hELLO wORLD]", Case::ProperBlend), "[HELLO WORLD]");
    assert_eq!(
        case("[one. TWO! three]", Case::Sentence),
        "[One. Two! Three]"
    );
    assert_eq!(case("[one. TWO]", Case::SentenceBlend), "[One. TWO]");
    assert_eq!(case("[AbC]", Case::Invert), "[aBc]");
    assert_eq!(
        case("a|b", Case::Upper),
        "a|b",
        "a caret alone changes nothing"
    );
    // Lengths may change ("ß" upper-cases to "SS"); later selections shift.
    assert_eq!(case("[ß] [x]", Case::Upper), "[SS] [X]");
}

#[test]
fn random_case_keeps_the_letters() {
    let out = run("[abcdefgh]", |t, s| convert_case(t, s, Case::Random(42)));
    assert_eq!(out.to_lowercase(), "[abcdefgh]");
}

#[test]
fn redact_hides_every_character_but_line_breaks() {
    let hide = |marked: &str| run(marked, redact);
    assert_eq!(hide("a [pass word] b"), "a [█████████] b");
    assert_eq!(hide("[ab\r\ncd]"), "[██\r\n██]");
    assert_eq!(hide("[Мир] [x]"), "[███] [█]");
    // A caret alone, or nothing selected, changes nothing.
    assert_eq!(hide("a|b"), "a|b");
    assert_eq!(hide(""), "|");
}
