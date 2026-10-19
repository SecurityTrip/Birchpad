use super::*;
use crate::change::Edit;

fn edit(text: &mut Rope, edits: Vec<Edit>) -> ChangeSet {
    let changes = ChangeSet::from_edits(text, edits).unwrap();
    changes.apply(text);
    changes
}

fn ranges(set: &RangeSet<u8>) -> Vec<(Range<usize>, u8)> {
    set.iter().map(|(r, v)| (r, *v)).collect()
}

#[test]
fn insert_replaces_and_splits() {
    let mut set = RangeSet::new();
    set.insert(0..10, 1);
    set.insert(3..5, 2);
    assert_eq!(ranges(&set), [(0..3, 1), (3..5, 2), (5..10, 1)]);
    set.insert(4..8, 3);
    assert_eq!(ranges(&set), [(0..3, 1), (3..4, 2), (4..8, 3), (8..10, 1)]);
    set.remove(2..9);
    assert_eq!(ranges(&set), [(0..2, 1), (9..10, 1)]);
    set.insert(2..9, 4);
    assert_eq!(ranges(&set), [(0..2, 1), (2..9, 4), (9..10, 1)]);
}

#[test]
fn insert_all_is_like_inserting_one_by_one() {
    let mut set = RangeSet::from_sorted([(0..4, 1), (6..8, 1), (9..20, 1)]);
    let mut expected = set.clone();
    let new = [1..2, 3..7, 8..9, 10..12, 15..25, 30..31];
    for range in new.clone() {
        expected.insert(range, 2);
    }
    set.insert_all(new, 2);
    assert_eq!(set, expected);
    assert_eq!(
        ranges(&set),
        [
            (0..1, 1),
            (1..2, 2),
            (2..3, 1),
            (3..7, 2),
            (7..8, 1),
            (8..9, 2),
            (9..10, 1),
            (10..12, 2),
            (12..15, 1),
            (15..25, 2),
            (30..31, 2)
        ]
    );
}

#[test]
fn queries() {
    let set = RangeSet::from_sorted([(2..4, 1), (6..9, 2), (9..10, 3)]);
    assert_eq!(set.at(3), Some(&1));
    assert_eq!(set.at(4), None);
    assert_eq!(set.at(9), Some(&3));
    let found: Vec<_> = set.overlapping(3..7).map(|(r, _)| r).collect();
    assert_eq!(found, [2..4, 6..9]);
    assert_eq!(set.next_from(5).map(|(r, _)| r), Some(6..9));
    assert_eq!(set.next_from(10), None);
    assert_eq!(set.previous_to(6).map(|(r, _)| r), Some(2..4));
    assert_eq!(set.previous_to(2), None);
}

#[test]
fn jumps_skip_the_range_at_the_caret_and_wrap() {
    let set = RangeSet::from_sorted([(2..4, ()), (6..9, ()), (9..10, ())]);
    assert_eq!(set.next_after(0), Some(2..4));
    assert_eq!(set.next_after(2), Some(6..9), "at the start of a range");
    assert_eq!(set.next_after(3), Some(6..9), "inside a range");
    assert_eq!(set.next_after(9), Some(2..4), "wraps to the first");
    assert_eq!(set.previous_before(6), Some(2..4));
    assert_eq!(set.previous_before(7), Some(2..4), "inside a range");
    assert_eq!(set.previous_before(10), Some(9..10));
    assert_eq!(set.previous_before(3), Some(9..10), "wraps to the last");
    assert_eq!(RangeSet::<()>::new().next_after(0), None);

    let other = RangeSet::from_sorted([(0..1, ()), (4..5, ())]);
    let sets = [&set, &other];
    assert_eq!(next_in_any(&sets, 2), Some(4..5));
    assert_eq!(next_in_any(&sets, 4), Some(6..9));
    assert_eq!(next_in_any(&sets, 9), Some(0..1));
    assert_eq!(previous_in_any(&sets, 6), Some(4..5));
    assert_eq!(previous_in_any(&sets, 0), Some(9..10));
}

#[test]
fn ranges_follow_edits_like_indicators() {
    let mut text = Rope::from_str("one two three");
    let mut set = RangeSet::from_sorted([(4..7, 1), (8..13, 2)]);
    // Inserting at the edges does not extend a range; inside it does.
    let changes = edit(&mut text, vec![Edit::insert(4, "<"), Edit::insert(7, ">")]);
    set.map(&changes);
    assert_eq!(ranges(&set), [(5..8, 1), (10..15, 2)]);
    let changes = edit(&mut text, vec![Edit::insert(6, "w")]);
    set.map(&changes);
    assert_eq!(ranges(&set), [(5..9, 1), (11..16, 2)]);
    // Deleting a range's text removes it.
    let changes = edit(&mut text, vec![Edit::delete(4..11)]);
    set.map(&changes);
    assert_eq!(ranges(&set), [(4..9, 2)]);
    assert_eq!(text, "one three");
}

#[test]
fn markers_follow_their_lines() {
    let mut text = Rope::from_str("a\nb\nc\nd");
    let mut markers = LineMarkers::new();
    assert!(markers.add(&text, 1));
    assert!(markers.add(&text, 3));
    assert!(!markers.add(&text, 3));
    assert_eq!(markers.lines(&text), [1, 3]);

    // Enter at the start of a marked line moves the marker down with the text.
    let changes = edit(&mut text, vec![Edit::insert(2, "\n")]);
    markers.map(&changes, &text);
    assert_eq!(markers.lines(&text), [2, 4]);

    // Deleting a marked line leaves the marker on the line that takes its place, and
    // markers landing on one line merge.
    let changes = edit(&mut text, vec![Edit::delete(3..7)]);
    markers.map(&changes, &text);
    assert_eq!(text, "a\n\nd");
    assert_eq!(markers.lines(&text), [2]);

    assert!(!markers.toggle(&text, 2));
    assert!(markers.is_empty());
}

#[test]
fn marker_navigation_wraps() {
    let text = Rope::from_str("0\n1\n2\n3\n4");
    let mut markers = LineMarkers::new();
    markers.add(&text, 1);
    markers.add(&text, 3);
    assert_eq!(markers.next_after(&text, 1), Some(3));
    assert_eq!(markers.next_after(&text, 3), Some(1));
    assert_eq!(markers.previous_before(&text, 1), Some(3));
    assert_eq!(markers.previous_before(&text, 2), Some(1));
    assert_eq!(markers.lines_in(&text, 2..5), [3]);
    assert_eq!(markers.lines_in(&text, 0..2), [1]);
    markers.set_lines(&text, [4, 0, 4, 9]);
    assert_eq!(markers.lines(&text), [0, 4]);
}
