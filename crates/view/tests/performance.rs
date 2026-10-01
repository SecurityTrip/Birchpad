//! Timing checks for large documents. `cargo test -p birchpad-view --release -- --ignored --nocapture`

use std::time::Instant;

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
    eprintln!(
        "wrap 100 MB at 120 columns: {rows} rows in {:?}",
        started.elapsed()
    );

    let started = Instant::now();
    let changes = ChangeSet::from_edits(&text, [Edit::insert(text.len() / 2, "typed")]).unwrap();
    let mut edited = text.clone();
    changes.apply(&mut edited);
    map.edit(&edited, &changes);
    map.row_count(&edited);
    eprintln!("one keystroke: {:?}", started.elapsed());
}

#[test]
#[ignore = "performance check; run in release"]
fn ten_megabyte_line() {
    let text = Rope::from_str(&"0123456789abcdef\t".repeat(10 * 1024 * 1024 / 17));
    let mut map = DisplayMap::new(&text, LayoutConfig::default());
    let started = Instant::now();
    let column = map.column(&text, text.len() - 1);
    eprintln!(
        "first column lookup at the end (builds the index): {:?}",
        started.elapsed()
    );
    let started = Instant::now();
    for i in 0..1000 {
        map.column(&text, text.len() - 1 - i * 17);
    }
    let row = map.row(&text, 0);
    map.pos_at_column(&text, &row, column / 2);
    eprintln!("1000 lookups with the index: {:?}", started.elapsed());

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
    eprintln!(
        "wrap the 10 MB line: {rows} rows in {:?} (last starts at {})",
        started.elapsed(),
        last.range.start
    );
}
