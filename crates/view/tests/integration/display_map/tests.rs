#![allow(
    clippy::single_range_in_vec_init,
    reason = "expected ranges written out"
)]

use birchpad_core::ChangeSet;
use birchpad_core::Edit;
use birchpad_core::Rope;
use birchpad_core::motion::line_count;
use birchpad_core::motion::line_range;
use birchpad_core::motion::next_boundary;
use birchpad_view::*;
use proptest::prelude::*;

fn wrapped(width: usize) -> LayoutConfig {
    LayoutConfig {
        tab_width: 4,
        wrap_width: Some(width),
    }
}

/// Lines longer than 12 bytes are long, so that short texts have them.
const SHORT_LONG_LINES: LongLines = LongLines {
    length: 12,
    checkpoint: 5,
};

/// A map whose long lines are short ones.
fn map_of(text: &Rope, config: LayoutConfig) -> DisplayMap {
    let mut map = DisplayMap::new(text, config);
    map.set_long_lines(SHORT_LONG_LINES);
    map
}

/// Lays out every line, as frames showing all of them would.
fn show_all(map: &mut DisplayMap, text: &Rope) {
    for line in 0..line_count(text) {
        let range = line_range(text, line);
        map.column(text, range.end);
        map.row_of(text, range.start);
    }
}

/// Checks that the map kept through edits answers as a map made afresh for the text does: its
/// rows, and the column and row of every position.
fn check_kept(map: &mut DisplayMap, text: &Rope) -> Result<(), TestCaseError> {
    let mut fresh = map_of(text, map.config());
    prop_assert_eq!(map.row_count(text), fresh.row_count(text));
    for row in 0..fresh.row_count(text) {
        prop_assert_eq!(map.row(text, row), fresh.row(text, row), "row {}", row);
    }
    let mut pos = 0;
    loop {
        prop_assert_eq!(
            map.column(text, pos),
            fresh.column(text, pos),
            "column at {}",
            pos
        );
        prop_assert_eq!(
            map.row_of(text, pos),
            fresh.row_of(text, pos),
            "row at {}",
            pos
        );
        if pos == text.len() {
            break;
        }
        pos = next_boundary(text, pos);
    }
    for line in 0..line_count(text) {
        for column in [0, 1, 3, 7, 20] {
            prop_assert_eq!(
                map.pos_at_line_column(text, line, column),
                fresh.pos_at_line_column(text, line, column),
                "line {} column {}",
                line,
                column
            );
        }
    }
    Ok(())
}

fn some_text() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop::sample::select(vec![
            "a", "bb", " ", "\t", "\t", "日", "😀", "word ", "\n", "\r\n", "\u{301}",
        ]),
        0..80,
    )
    .prop_map(|parts| parts.concat())
}

/// Edits at character boundaries of `doc`, sorted and not overlapping.
fn edits(doc: &str, raw: Vec<(usize, usize, String)>) -> Vec<Edit> {
    let boundaries: Vec<usize> = doc
        .char_indices()
        .map(|(i, _)| i)
        .chain([doc.len()])
        .collect();
    let mut edits: Vec<Edit> = raw
        .into_iter()
        .map(|(a, b, text)| {
            let a = boundaries[a % boundaries.len()];
            let b = boundaries[b % boundaries.len()];
            Edit::replace(a.min(b)..a.max(b).min(a.min(b) + 24), text)
        })
        .filter(|edit| doc.is_char_boundary(edit.range.end))
        .collect();
    edits.sort_by_key(|edit| (edit.range.start, edit.range.end));
    let mut end = 0;
    edits.retain(|edit| {
        let keep = edit.range.start >= end;
        if keep {
            end = edit.range.end;
        }
        keep
    });
    edits
}

#[test]
fn deleting_a_long_line_with_its_neighbors() {
    for config in [LayoutConfig::default(), wrapped(5)] {
        let mut text = Rope::from_str(&format!("ab\n{}\ncd", "x y ".repeat(10)));
        let mut map = map_of(&text, config);
        show_all(&mut map, &text);
        let changes = ChangeSet::from_edits(&text, [Edit::delete(1..text.len() - 1)]).unwrap();
        changes.apply(&mut text);
        map.edit(&text, &changes);
        assert_eq!(map.row(&text, 0).range, 0..2);
        assert_eq!(map.row_count(&text), 1);
        assert_eq!(map.column(&text, 2), 2);
    }
}

proptest! {
    #[test]
    fn long_lines_keep_their_layout_through_edits(
        doc in some_text(),
        steps in prop::collection::vec(
            prop::collection::vec(
                (any::<usize>(), any::<usize>(), prop::sample::select(vec![
                    "", "x", "\t", "日", "ab ", "\t\t", "\n", "\u{301}",
                ]).prop_map(str::to_owned)),
                0..4,
            ),
            1..8,
        ),
        width in prop::option::of(1usize..12),
        tab_width in 1usize..5,
    ) {
        let config = LayoutConfig { tab_width, wrap_width: width };
        let mut text = Rope::from_str(&doc);
        let mut map = map_of(&text, config);
        show_all(&mut map, &text);
        for raw in steps {
            let current = text.to_string();
            let changes = ChangeSet::from_edits(&text, edits(&current, raw)).unwrap();
            changes.apply(&mut text);
            map.edit(&text, &changes);
            let fresh = map_of(&text, config);
            prop_assert_eq!(map.rows_per_line(), fresh.rows_per_line());
            check_kept(&mut map, &text)?;
            show_all(&mut map, &text);
        }
    }
}

#[test]
fn a_column_starts_before_the_marks_of_no_width_at_it() {
    // Three combining marks (two bytes each, no column) start a long line, so that a
    // checkpoint of its column index falls after them, still at column 0.
    let line = format!("{}abcdefghijklmnop", "\u{301}".repeat(3));
    let text = Rope::from_str(&line);
    for config in [LayoutConfig::default(), wrapped(4)] {
        let mut map = map_of(&text, config);
        assert_eq!(map.pos_at_line_column(&text, 0, 0), (0, 0));
        assert_eq!(map.pos_at_line_column(&text, 0, 1), (7, 0));
        let row = map.row(&text, 0);
        assert_eq!(map.pos_at_column(&text, &row, 0), 0);
    }
}

#[test]
fn unwrapped_rows_are_lines() {
    let text = Rope::from_str("one\ntwo\r\nthree");
    let mut map = DisplayMap::new(&text, LayoutConfig::default());
    assert_eq!(map.row_count(&text), 3);
    let row = map.row(&text, 1);
    assert_eq!((row.line, row.range.clone()), (1, 4..7));
    assert_eq!(map.row_of(&text, 9).0, 2);
    assert_eq!(map.row(&text, 99).line, 2, "clamped");
}

#[test]
fn wrapped_rows_map_both_ways() {
    let text = Rope::from_str("aaaa bbbb cccc\nshort\ndddd eeee");
    let mut map = DisplayMap::new(&text, wrapped(5));
    assert_eq!(map.rows_per_line(), [3, 1, 2]);
    assert_eq!(map.row_count(&text), 6);
    assert_eq!(map.first_row_of_line(2), 4);
    let row = map.row(&text, 1);
    assert_eq!(
        (row.line, row.index_in_line, row.range.clone()),
        (0, 1, 5..10)
    );
    assert_eq!(map.row(&text, 3).line, 1);
    let last = map.row(&text, 5);
    assert_eq!(
        (last.line, last.index_in_line, last.last_in_line),
        (2, 1, true)
    );

    // The break position belongs to the next row.
    assert_eq!(map.row_of(&text, 5).0, 1);
    assert_eq!(map.row_of(&text, 4).0, 0);
}

#[test]
fn vertical_movement_keeps_the_goal_column() {
    let text = Rope::from_str("aaaa bbbb cccc\nxy\ndddd eeee");
    let mut map = DisplayMap::new(&text, wrapped(5));
    // From column 3 of row 0 ("aaaa ") down to row 1 ("bbbb "), then row 2 ("cccc").
    let pos = map.move_by_rows(&text, 3, 1, 3);
    assert_eq!(pos, 8);
    let pos = map.move_by_rows(&text, pos, 1, 3);
    assert_eq!(pos, 13);
    // "xy" is shorter: the end of the line.
    assert_eq!(map.move_by_rows(&text, pos, 1, 3), 17);
    // Within a wrapped row the caret never lands on the next row's start.
    let row = map.row(&text, 0);
    assert_eq!(map.pos_at_column(&text, &row, 99), 4);
    assert_eq!(map.move_by_rows(&text, 0, -1, 3), 0);
    assert_eq!(map.move_by_rows(&text, 0, 99, 3), text.len());
}

#[test]
fn columns_in_long_lines_use_the_index() {
    let line = "a\tb".repeat(10_000);
    let text = Rope::from_str(&line);
    let mut map = DisplayMap::new(&text, LayoutConfig::default());
    // The first "a\tb" takes 5 columns, every later one 4 (its tab ends on a stop), so
    // unit k starts at column 4k + 1.
    assert_eq!(map.column(&text, 3 * 5_000), 4 * 5_000 + 1);
    let row = map.row(&text, 0);
    assert_eq!(map.pos_at_column(&text, &row, 4 * 7_000 + 2), 3 * 7_000 + 1);
    assert_eq!(map.column(&text, text.len()), 4 * 10_000 + 1);
}

#[test]
fn incremental_edits_match_a_fresh_layout() {
    let mut text = Rope::from_str("aaaa bbbb\ncc\r\ndddd eeee ffff\n");
    let mut map = DisplayMap::new(&text, wrapped(5));
    let edits = [
        vec![Edit::insert(2, "\nzz zz zz")],
        vec![Edit::delete(0..12)],
        vec![Edit::insert(3, "\r"), Edit::insert(text.len(), "end")],
        vec![Edit::replace(0..1, "\n\n\n")],
    ];
    for batch in edits {
        let len = text.len();
        let batch: Vec<Edit> = batch
            .into_iter()
            .map(|e| Edit::replace(e.range.start.min(len)..e.range.end.min(len), e.text))
            .collect();
        let changes = ChangeSet::from_edits(&text, batch).unwrap();
        changes.apply(&mut text);
        map.edit(&text, &changes);
        let fresh = DisplayMap::new(&text, wrapped(5));
        assert_eq!(map.rows_per_line(), fresh.rows_per_line(), "{text:?}");
    }
}

/// The rows of the map as (line, padding), top to bottom.
fn rows_of(map: &mut DisplayMap, text: &Rope) -> Vec<(usize, bool)> {
    (0..map.row_count(text))
        .map(|index| {
            let row = map.row(text, index);
            (row.line, row.padding)
        })
        .collect()
}

#[test]
fn padding_adds_blank_rows_above_lines() {
    let text = Rope::from_str("a\nb\nc");
    let mut map = DisplayMap::new(&text, LayoutConfig::default());
    map.set_padding(&text, [(1, 2), (0, 1)]);
    assert_eq!(map.padding(), [(0, 1), (1, 2)]);
    assert_eq!(map.row_count(&text), 6);
    assert_eq!(
        rows_of(&mut map, &text),
        [
            (0, true),
            (0, false),
            (1, true),
            (1, true),
            (1, false),
            (2, false)
        ]
    );
    // A padding row shows nothing, where its line starts, and is no line's first row.
    let blank = map.row(&text, 2);
    assert_eq!(blank.range, 2..2);
    assert!(!blank.last_in_line && blank.index_in_line != 0);
    // Rows of positions skip the padding.
    assert_eq!(map.first_row_of_line(1), 4);
    assert_eq!(map.row_of(&text, 2).0, 4);
    assert_eq!(map.row_of(&text, 0).0, 1);
    assert_eq!(map.row_of(&text, 5).0, 5);
}

#[test]
fn padding_below_the_last_line_and_none() {
    let text = Rope::from_str("a\nb");
    let mut map = DisplayMap::new(&text, LayoutConfig::default());
    // Past the text counts as below it; entries of a line add up; zero rows are none.
    map.set_padding(&text, [(2, 1), (9, 1), (0, 0)]);
    assert_eq!(map.padding(), [(2, 2)]);
    assert_eq!(map.row_count(&text), 4);
    let last = map.row(&text, 3);
    assert!(last.padding);
    assert_eq!((last.line, last.range.clone()), (1, 3..3));
    // Clamped to the last row, a padding one.
    assert!(map.row(&text, 99).padding);
    map.set_padding(&text, []);
    assert_eq!(map.row_count(&text), 2);
    assert_eq!(rows_of(&mut map, &text), [(0, false), (1, false)]);
}

#[test]
fn padding_of_an_empty_text() {
    let text = Rope::new();
    let mut map = DisplayMap::new(&text, LayoutConfig::default());
    map.set_padding(&text, [(0, 2)]);
    assert_eq!(rows_of(&mut map, &text), [(0, true), (0, true), (0, false)]);
    assert_eq!(map.row_of(&text, 0).0, 2);
}

#[test]
fn padding_while_wrapping() {
    let text = Rope::from_str("aaaa bbbb\nc\nd");
    let mut map = DisplayMap::new(&text, wrapped(5));
    map.set_padding(&text, [(1, 1), (3, 2)]);
    assert_eq!(map.row_count(&text), 2 + 1 + 1 + 1 + 2);
    assert_eq!(
        rows_of(&mut map, &text),
        [
            (0, false),
            (0, false),
            (1, true),
            (1, false),
            (2, false),
            (2, true),
            (2, true)
        ]
    );
    assert_eq!(map.first_row_of_line(1), 3);
    assert_eq!(map.row_of(&text, 12).0, 4);
    let wrapped_row = map.row(&text, 1);
    assert_eq!((wrapped_row.index_in_line, wrapped_row.padding), (1, false));
}

#[test]
fn hidden_lines_have_no_padding() {
    let text = Rope::from_str("a\nb\nc\nd");
    let mut map = DisplayMap::new(&text, LayoutConfig::default());
    map.set_padding(&text, [(2, 1), (3, 1)]);
    map.set_hidden(vec![1..3]);
    // Lines 1 and 2 are hidden with the padding above line 2; line 3's stays.
    assert_eq!(
        rows_of(&mut map, &text),
        [(0, false), (3, true), (3, false)]
    );
    assert_eq!(map.first_row_of_line(3), 2);
    map.set_hidden(Vec::new());
    assert_eq!(map.row_count(&text), 6);
}

#[test]
fn the_caret_moves_past_padding() {
    let text = Rope::from_str("ab\ncd\nef");
    for config in [LayoutConfig::default(), wrapped(10)] {
        let mut map = DisplayMap::new(&text, config);
        map.set_padding(&text, [(1, 2), (3, 1)]);
        // Down from line 0 lands on line 1, not in the blank rows between.
        assert_eq!(map.move_by_rows(&text, 1, 1, 1), 4);
        assert_eq!(map.move_by_rows(&text, 4, -1, 1), 1);
        // Down from the last line, past the padding below it: the end of the text.
        assert_eq!(map.move_by_rows(&text, 7, 1, 1), text.len());
        // Up from the first line: the start.
        assert_eq!(map.move_by_rows(&text, 1, -1, 1), 0);
        // Not moving stays.
        assert_eq!(map.move_by_rows(&text, 4, 0, 1), 4);
    }
}

#[test]
fn padding_follows_lines_through_edits() {
    let text = Rope::from_str("a\nb\nc\nd");
    let mut map = DisplayMap::new(&text, LayoutConfig::default());
    map.set_padding(&text, [(0, 1), (2, 1), (4, 1)]);
    // A line break typed in line 0: the padding below moves down a line.
    let changes = ChangeSet::from_edits(&text, [Edit::insert(1, "\nx")]).unwrap();
    let mut edited = text.clone();
    changes.apply(&mut edited);
    map.edit(&edited, &changes);
    assert_eq!(map.padding(), [(0, 1), (3, 1), (5, 1)]);
    // Lines x and b removed: the padding above c and below the text moves up with them.
    let removed = ChangeSet::from_edits(&edited, [Edit::replace(1..5, "")]).unwrap();
    let mut shorter = edited.clone();
    removed.apply(&mut shorter);
    map.edit(&shorter, &removed);
    assert_eq!(shorter.to_string(), "a\nc\nd");
    assert_eq!(map.padding(), [(0, 1), (1, 1), (3, 1)]);
    // Line c removed: its padding goes with it.
    let gone = ChangeSet::from_edits(&shorter, [Edit::replace(1..3, "")]).unwrap();
    let mut two = shorter.clone();
    gone.apply(&mut two);
    map.edit(&two, &gone);
    assert_eq!(two.to_string(), "a\nd");
    assert_eq!(map.padding(), [(0, 1), (2, 1)]);
    // An edit within a line moves nothing.
    let typed = ChangeSet::from_edits(&two, [Edit::insert(0, "z")]).unwrap();
    let mut longer = two.clone();
    typed.apply(&mut longer);
    map.edit(&longer, &typed);
    assert_eq!(map.padding(), [(0, 1), (2, 1)]);
}
