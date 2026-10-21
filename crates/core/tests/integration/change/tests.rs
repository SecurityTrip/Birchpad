use birchpad_core::*;

fn apply(text: &str, edits: Vec<Edit>) -> (ChangeSet, String) {
    let mut rope = Rope::from_str(text);
    let changes = ChangeSet::from_edits(&rope, edits).unwrap();
    changes.apply(&mut rope);
    (changes, rope.to_string())
}

#[test]
fn applies_multiple_edits() {
    let (_, result) = apply(
        "hello world",
        vec![Edit::replace(0..5, "goodbye"), Edit::insert(11, "!")],
    );
    assert_eq!(result, "goodbye world!");
}

#[test]
fn canonical_form_puts_insert_before_delete() {
    let (changes, _) = apply("abc", vec![Edit::replace(1..2, "XY")]);
    assert_eq!(
        changes.ops(),
        [
            Operation::Retain(1),
            Operation::Insert("XY".into()),
            Operation::Delete(1),
            Operation::Retain(1),
        ]
    );
}

#[test]
fn rejects_bad_edits() {
    let rope = Rope::from_str("жук");
    let err = |edits: Vec<Edit>| ChangeSet::from_edits(&rope, edits).unwrap_err();
    assert_eq!(
        err(vec![Edit::delete(1..2)]),
        InvalidEdit::NotCharBoundary { pos: 1 }
    );
    assert_eq!(
        err(vec![Edit::delete(0..99)]),
        InvalidEdit::OutOfBounds { end: 99, len: 6 }
    );
    assert_eq!(
        err(vec![Edit::delete(2..4), Edit::delete(0..2)]),
        InvalidEdit::Unordered {
            start: 0,
            prev_end: 4
        }
    );
    #[expect(
        clippy::reversed_empty_ranges,
        reason = "the reversed range is the test"
    )]
    let reversed = Edit::delete(4..2);
    assert_eq!(
        err(vec![reversed]),
        InvalidEdit::Reversed { start: 4, end: 2 }
    );
    // One past the end, and overlapping edits.
    assert_eq!(
        err(vec![Edit::insert(7, "x")]),
        InvalidEdit::OutOfBounds { end: 7, len: 6 }
    );
    assert_eq!(
        err(vec![Edit::delete(0..4), Edit::delete(2..6)]),
        InvalidEdit::Unordered {
            start: 2,
            prev_end: 4
        }
    );
    // The end of a multi-byte character, not its middle, is a boundary.
    assert_eq!(
        err(vec![Edit::insert(5, "x")]),
        InvalidEdit::NotCharBoundary { pos: 5 }
    );
}

#[test]
fn accepts_edits_at_the_boundaries() {
    let rope = Rope::from_str("жук");
    let ok = |edits: Vec<Edit>| ChangeSet::from_edits(&rope, edits).unwrap();
    // At the very start and the very end, and two edits that touch.
    let changes = ok(vec![Edit::insert(0, "<"), Edit::insert(6, ">")]);
    assert_eq!((changes.len_before(), changes.len_after()), (6, 8));
    let changes = ok(vec![Edit::delete(0..2), Edit::replace(2..4, "Ж")]);
    let mut text = rope.clone();
    changes.apply(&mut text);
    assert_eq!(text.to_string(), "Жк");
    // No edits at all, and an empty text.
    assert!(ok(Vec::new()).is_identity());
    let empty = ChangeSet::from_edits(&Rope::new(), [Edit::insert(0, "a")]).unwrap();
    assert_eq!((empty.len_before(), empty.len_after()), (0, 1));
}

#[test]
fn invert_restores_original() {
    // Cyrillic letters take two bytes each: "два" is bytes 9..15, the text ends at 22.
    let original = Rope::from_str("один два три");
    let changes = ChangeSet::from_edits(
        &original,
        vec![Edit::replace(9..15, "2"), Edit::insert(22, ".")],
    )
    .unwrap();
    let mut text = original.clone();
    changes.apply(&mut text);
    assert_eq!(text, "один 2 три.");
    changes.invert(&original).apply(&mut text);
    assert_eq!(text, original);
}

#[test]
fn compose_equals_sequential_application() {
    let mut text = Rope::from_str("abc");
    let first = ChangeSet::from_edits(&text, vec![Edit::insert(1, "123")]).unwrap();
    let mut intermediate = text.clone();
    first.apply(&mut intermediate);
    // Deletes part of the inserted text and part of the original.
    let second = ChangeSet::from_edits(&intermediate, vec![Edit::delete(2..5)]).unwrap();
    first.compose(second).apply(&mut text);
    assert_eq!(text, "a1c");
}

/// Past the number of operations above which a change set rebuilds the text in one pass (4096)
/// instead of editing it in place.
const MANY: usize = 4096 + 10;

#[test]
fn many_edits_rebuild_the_same_text() {
    let line = "ab\r\n";
    let original = Rope::from_str(&line.repeat(MANY));
    let edits = (0..MANY).map(|i| Edit::replace(i * 4 + 2..i * 4 + 4, "\n"));
    let changes = ChangeSet::from_edits(&original, edits).unwrap();
    assert!(changes.ops().len() > 4096);
    let mut text = original.clone();
    changes.apply(&mut text);
    assert_eq!(text, "ab\n".repeat(MANY).as_str());
    changes.invert(&original).apply(&mut text);
    assert_eq!(text, original);
}

#[test]
fn map_pos_respects_assoc() {
    let rope = Rope::from_str("abcdef");
    let changes =
        ChangeSet::from_edits(&rope, vec![Edit::insert(2, "XX"), Edit::delete(3..5)]).unwrap();
    assert_eq!(changes.map_pos(1, Assoc::Before), 1);
    assert_eq!(changes.map_pos(2, Assoc::Before), 2);
    assert_eq!(changes.map_pos(2, Assoc::After), 4);
    assert_eq!(changes.map_pos(4, Assoc::Before), 5, "inside deletion");
    assert_eq!(changes.map_pos(6, Assoc::Before), 6, "end of text");
}
