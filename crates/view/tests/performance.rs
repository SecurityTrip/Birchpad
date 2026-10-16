//! Timing checks for large documents. `cargo test -p birchpad-view --release -- --ignored --nocapture`

use std::time::{Duration, Instant};

use birchpad_core::timing::within;
use birchpad_core::{ChangeSet, Edit, Rope};
use birchpad_view::{DisplayMap, LayoutConfig};

fn big_text(megabytes: usize) -> Rope {
    let line =
        "The quick brown fox jumps over the lazy dog; съешь же ещё этих булок 0123456789\r\n";
    Rope::from_str(&line.repeat(megabytes * 1024 * 1024 / line.len()))
}

#[test]
#[ignore = "performance check; run in release"]
fn wrapping_a_large_document() {
    let text = big_text(100);
    let started = Instant::now();
    let mut map = DisplayMap::new(
        &text,
        LayoutConfig {
            tab_width: 4,
            wrap_width: Some(120),
        },
    );
    let rows = map.row_count(&text);
    within(
        &format!("wrap 100 MB at 120 columns ({rows} rows)"),
        started.elapsed(),
        Duration::from_secs(20),
    );

    let started = Instant::now();
    let changes = ChangeSet::from_edits(&text, [Edit::insert(text.len() / 2, "typed")]).unwrap();
    let mut edited = text.clone();
    changes.apply(&mut edited);
    map.edit(&edited, &changes);
    map.row_count(&edited);
    within(
        "one keystroke in 100 MB, wrapped",
        started.elapsed(),
        Duration::from_millis(500),
    );
}

#[test]
#[ignore = "performance check; run in release"]
fn ten_megabyte_line() {
    let text = Rope::from_str(&"0123456789abcdef\t".repeat(10 * 1024 * 1024 / 17));
    let mut map = DisplayMap::new(&text, LayoutConfig::default());
    let started = Instant::now();
    let column = map.column(&text, text.len() - 1);
    within(
        "first column lookup at the end of a 10 MB line (builds the index)",
        started.elapsed(),
        Duration::from_secs(3),
    );
    let started = Instant::now();
    for i in 0..1000 {
        map.column(&text, text.len() - 1 - i * 17);
    }
    let row = map.row(&text, 0);
    map.pos_at_column(&text, &row, column / 2);
    within(
        "1000 lookups with the index",
        started.elapsed(),
        Duration::from_millis(500),
    );

    let started = Instant::now();
    let mut wrapped = DisplayMap::new(
        &text,
        LayoutConfig {
            tab_width: 4,
            wrap_width: Some(100),
        },
    );
    let rows = wrapped.row_count(&text);
    let last = wrapped.row(&text, rows - 1);
    assert!(last.range.start > 0);
    within(
        &format!("wrap the 10 MB line ({rows} rows)"),
        started.elapsed(),
        Duration::from_secs(10),
    );
}

#[test]
#[ignore = "performance check; run in release"]
fn folds_of_a_large_document() {
    // 400 000 lines with a three-line fold every four lines, like a long source file.
    let unit = "fn item(x: u32) -> u32 {\n    let y = x * 2;\n    y\n}\n";
    let source = unit.repeat(100_000);
    let text = Rope::from_str(&source);
    let ranges: Vec<std::ops::Range<usize>> = (0..100_000)
        .map(|i| {
            let start = i * unit.len();
            start..start + unit.len() - 1
        })
        .collect();
    let started = Instant::now();
    let folds = birchpad_view::folds_from_ranges(&text, &ranges);
    within(
        &format!("{} folds in lines", folds.len()),
        started.elapsed(),
        Duration::from_secs(3),
    );
    let collapsed: Vec<usize> = folds.iter().map(|fold| fold.header).collect();
    let started = Instant::now();
    let hidden = birchpad_view::hidden_lines(&folds, &collapsed);
    within(
        &format!("hidden lines of {} collapsed folds", hidden.len()),
        started.elapsed(),
        Duration::from_secs(2),
    );
}
