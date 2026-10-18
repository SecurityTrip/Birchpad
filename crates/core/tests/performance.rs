//! Timing checks for a 100 MB document: searching it and editing it.
//! `cargo test -p birchpad-core --release -- --ignored --nocapture`

use std::time::{Duration, Instant};

use birchpad_core::search::{Query, SearchMode, Searcher};
use birchpad_core::timing::within;
use birchpad_core::{ChangeSet, Edit, Rope};

const LINE: &str =
    "The quick brown fox jumps over the lazy dog; съешь же ещё этих булок 0123456789\r\n";

fn hundred_megabytes() -> Rope {
    Rope::from_str(&LINE.repeat(100 * 1024 * 1024 / LINE.len()))
}

fn searcher(pattern: &str, mode: SearchMode) -> Searcher {
    Searcher::new(&Query {
        pattern: pattern.to_owned(),
        match_case: false,
        mode,
        ..Query::default()
    })
    .unwrap()
}

#[test]
#[ignore = "performance check; run in release"]
fn searching_a_hundred_megabytes() {
    let text = hundred_megabytes();
    let lines = text.len() / LINE.len();

    let started = Instant::now();
    let found = searcher("lazy", SearchMode::Normal).find_all(&text);
    within(
        "find all of a word in 100 MB",
        started.elapsed(),
        Duration::from_secs(5),
    );
    assert_eq!(found.len(), lines);

    let started = Instant::now();
    let found = searcher(r"\d{4}", SearchMode::Regex).find_all(&text);
    within(
        "find all of a regular expression in 100 MB",
        started.elapsed(),
        Duration::from_secs(20),
    );
    assert_eq!(found.len(), lines * 2);

    // Nothing to find: the whole text read once.
    let started = Instant::now();
    assert!(
        searcher("absent", SearchMode::Normal)
            .find_all(&text)
            .is_empty()
    );
    within(
        "search 100 MB for nothing",
        started.elapsed(),
        Duration::from_secs(5),
    );

    let started = Instant::now();
    let replacements =
        searcher("fox", SearchMode::Normal).replacements(&text, 0..text.len(), "cat");
    within(
        "the replacements of a word in 100 MB",
        started.elapsed(),
        Duration::from_secs(5),
    );
    assert_eq!(replacements.len(), lines);
}

#[test]
#[ignore = "performance check; run in release"]
fn editing_a_hundred_megabytes() {
    let mut text = hundred_megabytes();
    let len = text.len();

    // A keystroke in the middle, and its undo.
    let started = Instant::now();
    let changes = ChangeSet::from_edits(&text, [Edit::insert(len / 2, "x")]).unwrap();
    let undo = changes.invert(&text);
    changes.apply(&mut text);
    undo.apply(&mut text);
    within(
        "a keystroke in 100 MB and its undo",
        started.elapsed(),
        Duration::from_millis(50),
    );
    assert_eq!(text.len(), len);

    // Replace All: an edit on every one of the 1.3 million lines.
    let edits: Vec<Edit> = (0..len / LINE.len())
        .map(|line| Edit::replace(line * LINE.len() + 4..line * LINE.len() + 9, "QUICK"))
        .collect();
    let started = Instant::now();
    let changes = ChangeSet::from_edits(&text, edits).unwrap();
    changes.apply(&mut text);
    within(
        "an edit on every line of 100 MB",
        started.elapsed(),
        Duration::from_secs(20),
    );
    assert_eq!(text.len(), len);
}
