use birchpad_core::Rope;
use birchpad_editor::buffer::*;

#[test]
fn mark_all_can_bookmark_the_lines_of_its_matches() {
    let text = Rope::from_str("a\nb b\nc\nb");
    let mut marks = DocumentMarks::default();
    marks.bookmarks.add(&text, 0);
    marks.bookmark_lines_of(&text, &[2..3, 4..5, 8..9]);
    assert_eq!(marks.bookmarks.lines(&text), [0, 1, 3]);
}
