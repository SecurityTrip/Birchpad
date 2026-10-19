use super::*;

#[test]
fn range_sets_count_and_clear_their_ranges() {
    let mut set = RangeSet::new();
    assert!(set.is_empty());
    assert_eq!(set.len(), 0);
    set.insert(0..2, 'a');
    set.insert(5..9, 'b');
    assert_eq!(set.len(), 2);
    assert!(!set.is_empty());
    // An empty range sets nothing, even inside another.
    set.insert(6..6, 'c');
    set.insert(20..20, 'c');
    assert_eq!(set.len(), 2);
    assert_eq!(set.at(6), Some(&'b'));
    set.clear();
    assert!(set.is_empty());
}

#[test]
fn line_markers_count_and_clear_their_lines() {
    let text = Rope::from_str("a\nb\nc");
    let mut markers = LineMarkers::new();
    assert!(markers.is_empty());
    markers.add(&text, 0);
    markers.add(&text, 2);
    assert_eq!(markers.len(), 2);
    // An empty range of lines has none, and lines past the end are ignored.
    assert!(markers.lines_in(&text, 1..1).is_empty());
    assert_eq!(markers.lines_in(&text, 2..99), [2]);
    markers.set_lines(&text, [1, 7]);
    assert_eq!(markers.lines(&text), [1]);
    markers.clear();
    assert!(markers.is_empty() && markers.lines(&text).is_empty());
}
