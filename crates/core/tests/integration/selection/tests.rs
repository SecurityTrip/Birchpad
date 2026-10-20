use birchpad_core::Edit;
use birchpad_core::*;
use ropey::Rope;

#[test]
fn sorts_and_merges_overlapping_ranges() {
    let selection = Selection::new(
        [
            Range::new(10, 12),
            Range::point(3),
            Range::new(1, 5),
            Range::point(12),
        ],
        1,
    );
    assert_eq!(
        selection.ranges(),
        [Range::new(1, 5), Range::new(10, 12), Range::point(12)]
    );
    assert_eq!(
        selection.primary(),
        Range::new(1, 5),
        "primary follows the merge"
    );
}

#[test]
fn merged_range_keeps_direction_of_primary() {
    let selection = Selection::new([Range::new(0, 4), Range::new(6, 2)], 1);
    assert_eq!(selection.ranges(), [Range::new(6, 0)]);
}

#[test]
fn duplicate_carets_collapse() {
    let selection = Selection::point(5).push(Range::point(5));
    assert_eq!(selection.ranges(), [Range::point(5)]);
}

#[test]
fn push_makes_range_primary() {
    let selection = Selection::point(10).push(Range::point(2));
    assert_eq!(selection.ranges(), [Range::point(2), Range::point(10)]);
    assert_eq!(selection.primary(), Range::point(2));
}

#[test]
fn virtual_space_orders_after_the_line_end() {
    let range = Range::new(5, 5).with_virtual(4, 1);
    assert!(range.is_backward(), "the head is left of the anchor");
    assert!(!range.is_empty(), "selects virtual space");
    assert_eq!((range.from(), range.to()), (5, 5));
    assert_eq!((range.from_virtual(), range.to_virtual()), (1, 4));
    assert!(Range::virtual_point(5, 2).is_empty());
    let mixed = Range::new(2, 5).with_virtual(0, 3);
    assert_eq!((mixed.from_virtual(), mixed.to_virtual()), (0, 3));
}

#[test]
fn merging_keeps_the_widest_virtual_span() {
    let selection = Selection::new(
        [
            Range::new(3, 5).with_virtual(0, 2),
            Range::new(5, 5).with_virtual(1, 6),
        ],
        0,
    );
    assert_eq!(
        selection.ranges(),
        [Range::new(3, 5).with_virtual(0, 6)],
        "same byte start or overlap merges"
    );
}

#[test]
fn inserting_at_a_virtual_position_uses_up_the_virtual_space() {
    let rope = Rope::from_str("ab\ncd");
    let caret = Range::virtual_point(2, 3);
    let elsewhere = ChangeSet::from_edits(&rope, [Edit::insert(4, "x")]).unwrap();
    assert_eq!(caret.map(&elsewhere), caret);
    let here = ChangeSet::from_edits(&rope, [Edit::insert(2, "   ")]).unwrap();
    assert_eq!(caret.map(&here), Range::point(2));
    let selection = Selection::new([caret, Range::virtual_point(1, 2)], 0);
    assert_eq!(
        selection.clip_virtual(&rope).ranges(),
        [Range::point(1), caret],
        "only line ends keep virtual space"
    );
}

#[test]
fn removing_a_range_moves_the_primary_back() {
    let selection = Selection::new([Range::point(1), Range::point(5), Range::point(9)], 2);
    let fewer = selection.remove(2).unwrap();
    assert_eq!(fewer.ranges(), [Range::point(1), Range::point(5)]);
    assert_eq!(fewer.primary(), Range::point(5));
    let fewer = selection.remove(0).unwrap();
    assert_eq!(fewer.primary(), Range::point(9));
    assert!(Selection::point(3).remove(0).is_none());

    // The primary range itself: the one before it takes over; at index 0, the next one.
    let middle = Selection::new([Range::point(1), Range::point(5), Range::point(9)], 1);
    assert_eq!(middle.remove(1).unwrap().primary(), Range::point(1));
    let first = Selection::new([Range::point(1), Range::point(5)], 0);
    assert_eq!(first.remove(0).unwrap().primary(), Range::point(5));
    // No such range.
    assert!(middle.remove(3).is_none());
}

#[test]
fn without_virtual_drops_virtual_space_only() {
    let range = Range::new(2, 4).with_virtual(3, 5);
    assert!(range.has_virtual());
    let real = range.without_virtual();
    assert!(!real.has_virtual());
    assert_eq!((real.anchor, real.head), (2, 4));
    assert_eq!(Range::point(0).without_virtual(), Range::point(0));
}

#[test]
fn maps_through_changes() {
    let rope = Rope::from_str("hello world");
    let changes = ChangeSet::from_edits(&rope, [Edit::insert(0, ">> ")]).unwrap();
    let selection = Selection::new([Range::point(0), Range::new(6, 11)], 1);
    assert_eq!(
        selection.map(&changes).ranges(),
        [Range::point(0), Range::new(9, 14)]
    );
}
