use proptest::prelude::*;

use birchpad_tools::compare::*;

/// Lines from a small alphabet, so that equal lines are common.
fn text() -> impl Strategy<Value = String> {
    proptest::collection::vec("[abc]{0,2}", 0..20)
        .prop_map(|lines| lines.into_iter().map(|line| format!("{line}\n")).collect())
}

#[test]
fn a_line_removed_at_the_start_before_lines_added_is_where_it_is() {
    // similar's own diff of these says the removal is at new line 3.
    let old = "
b
b

";
    let new = "b
a
a
b
b








";
    let differences = compare(old, new, Options::default());
    assert_eq!(differences[0].old, 0..1);
    assert_eq!(differences[0].new, 0..0);
}

proptest! {
    #[test]
    fn the_differences_turn_one_text_into_the_other(old in text(), new in text()) {
        let differences = compare(&old, &new, Options::default());
        let (old_lines, new_lines) = (lines(&old), lines(&new));
        let mut rebuilt: Vec<&str> = Vec::new();
        let mut at = 0;
        for difference in &differences {
            prop_assert!(difference.old.start >= at, "sorted, apart");
            rebuilt.extend(&old_lines[at..difference.old.start]);
            rebuilt.extend(&new_lines[difference.new.clone()]);
            at = difference.old.end;
            let expected = match (difference.old.is_empty(), difference.new.is_empty()) {
                (true, false) => Change::Added,
                (false, true) => Change::Removed,
                _ => Change::Changed,
            };
            prop_assert_eq!(difference.change, expected);
        }
        rebuilt.extend(&old_lines[at..]);
        prop_assert_eq!(rebuilt, new_lines);
        // A text has no differences with itself.
        prop_assert!(compare(&old, &old, Options::default()).is_empty());
    }
}
