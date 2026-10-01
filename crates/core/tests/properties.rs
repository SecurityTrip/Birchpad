//! Property tests for the text model: random edits on random multilingual text, checked against
//! a naive `String`-based implementation.

use birchpad_core::search::{Direction, Query, Searcher};
use birchpad_core::{
    Assoc, ChangeSet, Document, Edit, Range, Rope, Selection, Transaction, UndoGrouping,
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
}
