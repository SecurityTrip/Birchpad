use birchpad_core::Rope;
use birchpad_git::changes::*;

fn changes(head: &str, text: &str) -> LineChanges {
    line_changes(&Rope::from_str(head), &Rope::from_str(text))
}

use GitChange::{Added, Deleted, Modified};

#[test]
fn the_same_text_has_no_changes() {
    assert!(changes("", "").is_empty());
    assert!(changes("a\nb\n", "a\nb\n").is_empty());
    // Line breaks do not count: Git's LF, the document's CRLF or CR.
    assert!(changes("a\nb\n", "a\r\nb\r\n").is_empty());
    assert!(changes("a\r\nb", "a\rb").is_empty());
}

#[test]
fn added_modified_and_deleted_lines() {
    assert_eq!(changes("a\nc\n", "a\nb\nc\n"), [(1..2, Added)]);
    assert_eq!(changes("a\nb\nc\n", "a\nB\nc\n"), [(1..2, Modified)]);
    assert_eq!(changes("a\nb\nc\n", "a\nc\n"), [(1..2, Deleted)]);
    // At the start and in several places at once.
    assert_eq!(
        changes("a\nb\nc\nd\ne\n", "x\na\nB\nc\ne\n"),
        [(0..1, Added), (2..3, Modified), (4..5, Deleted)]
    );
    // Several lines in a row.
    assert_eq!(changes("a\nz\n", "a\nb\nc\nd\nz\n"), [(1..4, Added)]);
}

#[test]
fn changes_at_the_ends_of_the_text() {
    // Lines deleted at the end mark the last line.
    assert_eq!(changes("a\nb\nc\n", "a\n"), [(1..2, Deleted)]);
    assert_eq!(changes("a\nb", "a"), [(0..1, Modified)]);
    // Everything deleted: marked on the one, empty line left.
    assert_eq!(changes("a\nb\n", ""), [(0..1, Deleted)]);
    // Everything new.
    assert_eq!(changes("", "a\nb"), [(0..2, Modified)]);
    // A line added at the end without a final line break.
    assert_eq!(changes("a\n", "a\nb"), [(1..2, Modified)]);
    // A final line break added: the last line changed, and no line was deleted.
    let found = changes("a", "a\n");
    assert!(
        found.iter().all(|(_, change)| *change != Deleted),
        "{found:?}"
    );
}

#[test]
fn the_next_and_the_previous_change_go_around() {
    let found: LineChanges = vec![(2..4, Added), (6..7, Modified), (9..10, Deleted)];
    assert_eq!(next_change(&found, 0, true), Some(2));
    assert_eq!(next_change(&found, 2, true), Some(6));
    assert_eq!(
        next_change(&found, 3, true),
        Some(6),
        "inside a block: the next"
    );
    assert_eq!(next_change(&found, 9, true), Some(2), "around the end");
    assert_eq!(next_change(&found, 20, true), Some(2));
    assert_eq!(next_change(&found, 7, false), Some(6));
    assert_eq!(next_change(&found, 6, false), Some(2));
    assert_eq!(
        next_change(&found, 3, false),
        Some(9),
        "its own block, then around"
    );
    assert_eq!(next_change(&found, 0, false), Some(9));
    assert_eq!(next_change(&Vec::new(), 5, true), None);
    assert_eq!(next_change(&Vec::new(), 5, false), None);
    let one: LineChanges = vec![(0..1, Added)];
    assert_eq!(next_change(&one, 0, true), Some(0));
    assert_eq!(next_change(&one, 0, false), Some(0));
}
