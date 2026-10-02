//! Property tests for the text model: random edits on random multilingual text, checked against
//! a naive `String`-based implementation.

use birchpad_core::motion::{line_count, line_of};
use birchpad_core::search::{Direction, Query, Searcher};
use birchpad_core::{
    Assoc, ChangeSet, Document, Edit, LineMarkers, Range, RangeSet, Rope, Selection, Transaction,
    UndoGrouping,
};
use proptest::prelude::*;

/// Text mixing one-, two-, three- and four-byte characters and every kind of line break.
fn text() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop::sample::select(vec![
            "a", "z", " ", "ж", "Ё", "€", "😀", "\n", "\r", "\r\n", "\t",
        ]),
        0..40,
    )
    .prop_map(|parts| parts.concat())
}

type RawEdit = (usize, usize, Option<String>);

fn raw_edits() -> impl Strategy<Value = Vec<RawEdit>> {
    prop::collection::vec(
        (any::<usize>(), any::<usize>(), prop::option::of(text())),
        0..6,
    )
}

/// Turns random numbers into valid edits for `doc`: char-boundary positions, sorted, disjoint.
fn edits_for(doc: &str, raw: Vec<RawEdit>) -> Vec<Edit> {
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
            Edit::replace(a.min(b)..a.max(b), text.unwrap_or_default())
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

fn apply_naive(doc: &str, edits: &[Edit]) -> String {
    let mut out = doc.to_owned();
    for edit in edits.iter().rev() {
        out.replace_range(edit.range.clone(), &edit.text);
    }
    out
}

fn apply(doc: &Rope, changes: &ChangeSet) -> Rope {
    let mut doc = doc.clone();
    changes.apply(&mut doc);
    doc
}

proptest! {
    #[test]
    fn apply_matches_naive(doc in text(), raw in raw_edits()) {
        let edits = edits_for(&doc, raw);
        let rope = Rope::from_str(&doc);
        let changes = ChangeSet::from_edits(&rope, edits.clone()).unwrap();
        let result = apply(&rope, &changes);
        prop_assert_eq!(result.to_string(), apply_naive(&doc, &edits));
        prop_assert_eq!(result.len(), changes.len_after());
    }

    #[test]
    fn invert_undoes(doc in text(), raw in raw_edits()) {
        let rope = Rope::from_str(&doc);
        let changes = ChangeSet::from_edits(&rope, edits_for(&doc, raw)).unwrap();
        let changed = apply(&rope, &changes);
        prop_assert_eq!(apply(&changed, &changes.invert(&rope)), rope);
    }

    #[test]
    fn compose_matches_sequential(doc in text(), raw_a in raw_edits(), raw_b in raw_edits()) {
        let rope = Rope::from_str(&doc);
        let a = ChangeSet::from_edits(&rope, edits_for(&doc, raw_a)).unwrap();
        let middle = apply(&rope, &a);
        let middle_text = middle.to_string();
        let b = ChangeSet::from_edits(&middle, edits_for(&middle_text, raw_b)).unwrap();
        let sequential = apply(&middle, &b);
        prop_assert_eq!(apply(&rope, &a.compose(b)), sequential);
    }

    #[test]
    fn map_pos_is_monotonic_and_in_bounds(doc in text(), raw in raw_edits(), assoc_after: bool) {
        let rope = Rope::from_str(&doc);
        let edits = edits_for(&doc, raw);
        let changes = ChangeSet::from_edits(&rope, edits.clone()).unwrap();
        let assoc = if assoc_after { Assoc::After } else { Assoc::Before };

        let mut previous = 0;
        for pos in 0..=doc.len() {
            let mapped = changes.map_pos(pos, assoc);
            prop_assert!(mapped >= previous && mapped <= changes.len_after());
            previous = mapped;
        }

        // Text before the first edit does not move.
        let first = edits.first().map_or(doc.len(), |edit| edit.range.start);
        for pos in 0..first {
            prop_assert_eq!(changes.map_pos(pos, assoc), pos);
        }
    }

    #[test]
    fn selection_stays_sorted_and_disjoint(
        points in prop::collection::vec((0usize..50, 0usize..50), 1..8),
        primary in any::<prop::sample::Index>(),
    ) {
        let ranges: Vec<Range> = points.iter().map(|&(a, h)| Range::new(a, h)).collect();
        let primary_range = ranges[primary.index(ranges.len())];
        let selection = Selection::new(ranges, primary.index(points.len()));

        for pair in selection.ranges().windows(2) {
            prop_assert!(pair[0].to() <= pair[1].from() && pair[0].from() < pair[1].from());
        }
        let primary = selection.primary();
        prop_assert!(primary.from() <= primary_range.from() && primary_range.to() <= primary.to());
    }

    #[test]
    fn undo_all_restores_and_redo_all_replays(
        doc in text(),
        steps in prop::collection::vec((raw_edits(), any::<bool>()), 1..8),
    ) {
        let mut document = Document::from_text(Rope::from_str(&doc));
        let mut versions = vec![document.text().clone()];
        for (raw, merge) in steps {
            let current = document.text().to_string();
            let transaction = Transaction::from_edits(document.text(), edits_for(&current, raw)).unwrap();
            let grouping = if merge { UndoGrouping::MergeWithPrevious } else { UndoGrouping::NewStep };
            document.apply(&transaction, &Selection::point(0), grouping);
            versions.push(document.text().clone());
        }
        let last = versions.last().unwrap().clone();

        while document.undo().is_some() {}
        prop_assert_eq!(document.text(), &versions[0]);
        prop_assert!(!document.is_modified());

        while document.redo().is_some() {}
        prop_assert_eq!(document.text(), &last);
    }

    #[test]
    fn search_matches_naive_search(
        hay in prop::collection::vec(prop::sample::select(vec!["ab", "a", "b", "Ж", "ж", "\n", "x"]), 0..300),
        needle in prop::collection::vec(prop::sample::select(vec!["a", "b", "ж", "Ж"]), 1..4),
        match_case: bool,
    ) {
        let hay = hay.concat();
        let needle = needle.concat();
        let rope = Rope::from_str(&hay);
        let searcher = Searcher::new(&Query {
            pattern: needle.clone(),
            match_case,
            ..Query::default()
        }).unwrap();
        // Naive: non-overlapping matches, comparing lowercased characters when ignoring case.
        let fold = |s: &str| -> String {
            if match_case { s.to_owned() } else { s.chars().map(|c| c.to_lowercase().next().unwrap()).collect() }
        };
        let (folded_hay, folded_needle) = (fold(&hay), fold(&needle));
        // Folding keeps byte lengths for these characters, so offsets carry over.
        let expected: Vec<std::ops::Range<usize>> = folded_hay
            .match_indices(&folded_needle)
            .map(|(at, m)| at..at + m.len())
            .collect();
        prop_assert_eq!(searcher.find_all(&rope), expected);
        // Searching backward finds the match that starts last, which may overlap others.
        let last = folded_hay
            .char_indices()
            .map(|(at, _)| at)
            .filter(|&at| folded_hay[at..].starts_with(&folded_needle))
            .max()
            .map(|at| at..at + folded_needle.len());
        prop_assert_eq!(searcher.find(&rope, rope.len(), Direction::Backward, false), last);
    }

    #[test]
    fn mapper_matches_map_pos(doc in text(), raw in raw_edits(), assoc_after: bool) {
        let rope = Rope::from_str(&doc);
        let changes = ChangeSet::from_edits(&rope, edits_for(&doc, raw)).unwrap();
        let assoc = if assoc_after { Assoc::After } else { Assoc::Before };
        let mut mapper = changes.mapper(assoc);
        for pos in 0..=doc.len() {
            prop_assert_eq!(mapper.map(pos), changes.map_pos(pos, assoc));
        }
    }

    #[test]
    fn range_sets_map_like_their_endpoints(
        doc in text(),
        raw in raw_edits(),
        cuts in prop::collection::vec(any::<usize>(), 0..10),
    ) {
        let rope = Rope::from_str(&doc);
        let boundaries: Vec<usize> =
            doc.char_indices().map(|(i, _)| i).chain([doc.len()]).collect();
        let mut points: Vec<usize> = cuts.iter().map(|c| boundaries[c % boundaries.len()]).collect();
        points.sort_unstable();
        points.dedup();
        let ranges: Vec<(std::ops::Range<usize>, usize)> = points
            .as_chunks::<2>()
            .0
            .iter()
            .enumerate()
            .map(|(i, &[start, end])| (start..end, i))
            .collect();
        let mut set = RangeSet::from_sorted(ranges.clone());
        let changes = ChangeSet::from_edits(&rope, edits_for(&doc, raw)).unwrap();
        set.map(&changes);

        let expected: Vec<(std::ops::Range<usize>, usize)> = ranges
            .into_iter()
            .map(|(range, value)| {
                let start = changes.map_pos(range.start, Assoc::After);
                let end = changes.map_pos(range.end, Assoc::Before).max(start);
                (start..end, value)
            })
            .filter(|(range, _)| !range.is_empty())
            .collect();
        let actual: Vec<_> = set.iter().map(|(r, v)| (r, *v)).collect();
        prop_assert_eq!(actual, expected);
    }

    #[test]
    fn line_markers_stay_one_per_line_and_in_bounds(
        doc in text(),
        raw in raw_edits(),
        marked in prop::collection::vec(any::<usize>(), 0..8),
    ) {
        let mut rope = Rope::from_str(&doc);
        let mut markers = LineMarkers::new();
        for line in &marked {
            markers.add(&rope, line % line_count(&rope));
        }
        let before: Vec<usize> = markers.lines(&rope);
        let positions: Vec<usize> = before.iter().map(|&l| rope.line_to_byte_idx(l, birchpad_core::LINE_TYPE)).collect();
        let changes = ChangeSet::from_edits(&rope, edits_for(&doc, raw)).unwrap();
        changes.apply(&mut rope);
        markers.map(&changes, &rope);

        let mut expected: Vec<usize> = positions
            .iter()
            .map(|&pos| line_of(&rope, changes.map_pos(pos, Assoc::After)))
            .collect();
        expected.dedup();
        prop_assert_eq!(markers.lines(&rope), expected);
    }

    #[test]
    fn nested_ranges_map_like_their_endpoints(
        doc in text(),
        raw in raw_edits(),
        pairs in prop::collection::vec((any::<usize>(), any::<usize>()), 0..8),
    ) {
        let rope = Rope::from_str(&doc);
        let boundaries: Vec<usize> =
            doc.char_indices().map(|(i, _)| i).chain([doc.len()]).collect();
        // Arbitrary, possibly nested or overlapping ranges, sorted by start.
        let mut ranges: Vec<std::ops::Range<usize>> = pairs
            .into_iter()
            .map(|(a, b)| {
                let a = boundaries[a % boundaries.len()];
                let b = boundaries[b % boundaries.len()];
                a.min(b)..a.max(b)
            })
            .filter(|range| !range.is_empty())
            .collect();
        ranges.sort_by_key(|range| range.start);
        let changes = ChangeSet::from_edits(&rope, edits_for(&doc, raw)).unwrap();
        let expected: Vec<std::ops::Range<usize>> = ranges
            .iter()
            .map(|range| {
                let start = changes.map_pos(range.start, Assoc::After);
                start..changes.map_pos(range.end, Assoc::Before).max(start)
            })
            .filter(|range| !range.is_empty())
            .collect();
        let mut mapped = ranges.clone();
        birchpad_core::map_ranges(&mut mapped, &changes);
        prop_assert_eq!(mapped, expected);
    }

    #[test]
    fn moving_lines_up_then_down_restores_the_text(
        lines in prop::collection::vec("[a-c ]{0,4}", 2..8),
        caret_line in any::<prop::sample::Index>(),
    ) {
        use birchpad_core::ops::move_lines;
        use birchpad_core::LineEnding;
        let doc = lines.join("\n");
        let rope = Rope::from_str(&doc);
        let line = 1 + caret_line.index(lines.len() - 1);
        let caret = rope.line_to_byte_idx(line, birchpad_core::LINE_TYPE);
        let selection = Selection::point(caret);
        let up = move_lines(&rope, &selection, true, LineEnding::Lf).unwrap();
        let mut moved = rope.clone();
        up.changes().apply(&mut moved);
        let after = up.selection().unwrap().clone();
        let down = move_lines(&moved, &after, false, LineEnding::Lf).unwrap();
        down.changes().apply(&mut moved);
        prop_assert_eq!(moved.to_string(), doc);
        prop_assert_eq!(down.selection().unwrap().primary(), Range::point(caret));
    }

    #[test]
    fn sorting_permutes_and_reversing_twice_restores(
        lines in prop::collection::vec("[a-cA-C]{0,3}", 1..10),
    ) {
        use birchpad_core::ops::{SortKey, reverse_lines, sort_lines};
        use birchpad_core::LineEnding;
        let doc = lines.join("\n");
        let rope = Rope::from_str(&doc);
        let selection = Selection::point(0);
        let apply = |rope: &Rope, transaction: Option<Transaction>| {
            let mut rope = rope.clone();
            if let Some(transaction) = transaction {
                transaction.changes().apply(&mut rope);
            }
            rope
        };
        let sorted = apply(&rope, sort_lines(&rope, &selection, SortKey::Lexicographic, false, LineEnding::Lf).unwrap());
        let mut expected = lines.clone();
        expected.sort();
        prop_assert_eq!(sorted.to_string(), expected.join("\n"));
        let reversed = apply(&rope, reverse_lines(&rope, &selection, LineEnding::Lf));
        let twice = apply(&reversed, reverse_lines(&reversed, &selection, LineEnding::Lf));
        prop_assert_eq!(twice.to_string(), doc);
    }

    #[test]
    fn selection_with_virtual_space_stays_sorted_and_disjoint(
        points in prop::collection::vec((0usize..30, 0usize..4, 0usize..30, 0usize..4), 1..8),
        primary in any::<prop::sample::Index>(),
    ) {
        let ranges: Vec<Range> = points
            .iter()
            .map(|&(a, av, h, hv)| Range::new(a, h).with_virtual(av, hv))
            .collect();
        let primary_range = ranges[primary.index(ranges.len())];
        let selection = Selection::new(ranges, primary.index(points.len()));

        // The byte invariants of ranges without virtual space still hold...
        for pair in selection.ranges().windows(2) {
            prop_assert!(pair[0].to() <= pair[1].from() && pair[0].from() < pair[1].from());
            // ...and virtual space never makes neighbours overlap.
            prop_assert!(
                (pair[0].to(), pair[0].to_virtual()) <= (pair[1].from(), pair[1].from_virtual())
            );
        }
        let primary = selection.primary();
        prop_assert!(
            (primary.from(), primary.from_virtual())
                <= (primary_range.from(), primary_range.from_virtual())
        );
        prop_assert!(
            (primary_range.to(), primary_range.to_virtual()) <= (primary.to(), primary.to_virtual())
        );
    }

    #[test]
    fn typing_into_virtual_space_matches_padding_by_hand(
        lines in prop::collection::vec("[a-c]{0,4}", 1..6),
        column in 0usize..7,
        typed in "[xy]{0,2}",
    ) {
        // A zero-width rectangle at `column` over every line: the caret is inside a line or
        // past its end in virtual space.
        let doc = lines.join("\n");
        let rope = Rope::from_str(&doc);
        let mut start = 0;
        let mut carets = Vec::new();
        for line in &lines {
            let caret = if column <= line.len() {
                Range::point(start + column)
            } else {
                Range::virtual_point(start + line.len(), column - line.len())
            };
            carets.push(caret);
            start += line.len() + 1;
        }
        let selection = Selection::new(carets, 0);
        let transaction = Transaction::replace_selections(&rope, &selection, &typed).unwrap();
        let result = apply(&rope, transaction.changes());

        let expected: Vec<String> = lines
            .iter()
            .map(|line| {
                if typed.is_empty() {
                    line.clone()
                } else if column <= line.len() {
                    format!("{}{typed}{}", &line[..column], &line[column..])
                } else {
                    format!("{line}{}{typed}", " ".repeat(column - line.len()))
                }
            })
            .collect();
        prop_assert_eq!(result.to_string(), expected.join("\n"));
        // Every caret ends up after the typed text, in virtual space only if nothing was typed.
        let after = transaction.selection().unwrap();
        let mut start = 0;
        for (range, line) in after.iter().zip(&expected) {
            let column_after = if typed.is_empty() {
                (range.head - start) + range.head_virtual
            } else {
                prop_assert_eq!(range.head_virtual, 0);
                range.head - start
            };
            prop_assert_eq!(column_after, column + typed.len());
            start += line.len() + 1;
        }
        // Mapping the old carets through the change and clipping leaves virtual space only
        // at line ends.
        for range in selection.map(transaction.changes()).clip_virtual(&result).iter() {
            if range.head_virtual > 0 {
                let line = birchpad_core::motion::line_of(&result, range.head);
                prop_assert_eq!(birchpad_core::motion::line_range(&result, line).end, range.head);
            }
        }
    }
}
