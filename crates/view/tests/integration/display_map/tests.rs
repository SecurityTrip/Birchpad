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
