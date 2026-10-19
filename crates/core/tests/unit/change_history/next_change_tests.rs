#[test]
fn going_from_change_to_change() {
    use super::next_change;
    let changed = [2, 3, 4, 8, 12, 13];
    assert_eq!(next_change(&changed, 0, true), Some(2));
    assert_eq!(
        next_change(&changed, 3, true),
        Some(8),
        "past its own block"
    );
    assert_eq!(next_change(&changed, 12, true), Some(2), "around");
    assert_eq!(next_change(&changed, 20, true), Some(2));
    assert_eq!(next_change(&changed, 13, false), Some(8));
    assert_eq!(next_change(&changed, 10, false), Some(8));
    assert_eq!(next_change(&changed, 3, false), Some(12), "around");
    assert_eq!(next_change(&changed, 0, false), Some(12));
    // One block: from inside, nowhere else to go; from outside, to it.
    assert_eq!(next_change(&[5, 6], 6, true), None);
    assert_eq!(next_change(&[5, 6], 5, false), None);
    assert_eq!(next_change(&[5, 6], 0, true), Some(5));
    assert_eq!(next_change(&[5, 6], 9, false), Some(5));
    assert_eq!(next_change(&[], 0, true), None);
    assert_eq!(next_change(&[], 0, false), None);
}
