//! Property tests: incremental layout equals a fresh layout; positions and columns agree.

use birchpad_core::{ChangeSet, Edit, Rope};
use birchpad_view::{DisplayMap, DisplayText, LayoutConfig, column_after, row_count, wrap_line};
use proptest::prelude::*;

fn text() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop::sample::select(vec![
            "a", "bb", " ", "  ", "\t", "\n", "\r\n", "\r", "日", "ж", "😀", "\0", "word ",
        ]),
        0..60,
    )
    .prop_map(|parts| parts.concat())
}

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
            Edit::replace(a.min(b)..a.max(b), text)
        })
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

proptest! {
    #[test]
    fn incremental_layout_matches_fresh(
        doc in text(),
        steps in prop::collection::vec(
            prop::collection::vec((any::<usize>(), any::<usize>(), text()), 0..4),
            1..6,
        ),
        width in 1usize..12,
    ) {
        let config = LayoutConfig { tab_width: 4, wrap_width: Some(width) };
        let mut rope = Rope::from_str(&doc);
        let mut map = DisplayMap::new(&rope, config);
        for raw in steps {
            let current = rope.to_string();
            let changes = ChangeSet::from_edits(&rope, edits(&current, raw)).unwrap();
            changes.apply(&mut rope);
            map.edit(&rope, &changes);
            let fresh = DisplayMap::new(&rope, config);
            prop_assert_eq!(map.rows_per_line(), fresh.rows_per_line());
        }
    }

    #[test]
    fn every_position_round_trips_through_rows(doc in text(), width in 1usize..12) {
        let rope = Rope::from_str(&doc);
        let mut map = DisplayMap::new(&rope, LayoutConfig { tab_width: 4, wrap_width: Some(width) });
        let rows = map.row_count(&rope);
        let mut previous_row = 0;
        // Caret positions: every character boundary except between the CR and LF of a CRLF.
        let positions = doc
            .char_indices()
            .map(|(i, _)| i)
            .chain([doc.len()])
            .filter(|&i| !(doc[..i].ends_with('\r') && doc[i..].starts_with('\n')));
        for pos in positions {
            let (index, row) = map.row_of(&rope, pos);
            prop_assert!(index < rows);
            prop_assert!(index >= previous_row, "rows are monotonic");
            prop_assert!(row.range.start <= pos && pos <= row.range.end);
            prop_assert_eq!(map.row(&rope, index), row.clone());
            previous_row = index;
            // Clicking at the column of a position finds it again.
            if pos < row.range.end || row.last_in_line {
                let column = map.column(&rope, pos);
                prop_assert_eq!(map.pos_at_column(&rope, &row, column), pos);
            }
        }
    }

    #[test]
    fn fast_row_count_agrees(doc in text(), width in 1usize..12) {
        let rope = Rope::from_str(&doc);
        prop_assert_eq!(
            row_count(&rope, 0..rope.len(), width, 4),
            wrap_line(&rope, 0..rope.len(), width, 4).len()
        );
    }

    #[test]
    fn display_offsets_round_trip(doc in text()) {
        let rope = Rope::from_str(&doc);
        let display = DisplayText::new(&rope, 0..rope.len(), 0, 4);
        for (pos, _) in doc.char_indices().chain([(doc.len(), ' ')]) {
            prop_assert_eq!(display.to_doc(display.to_display(pos)), pos);
        }
        // Expanded text is as wide as the columns say (on a single line).
        if !doc.contains(['\n', '\r']) {
            let cells: usize = display.text.chars().map(birchpad_view::char_cells).sum();
            prop_assert_eq!(cells, column_after(&rope, 0, 0, rope.len(), 4));
        }
    }
}
