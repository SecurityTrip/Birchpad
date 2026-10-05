//! Rough timings of parsing and highlighting, for the performance budgets of the editor:
//! `cargo run --release -p birchpad-syntax --example bench [lines]`.

#![allow(clippy::print_stdout, reason = "a benchmark prints its timings")]

use std::sync::atomic::AtomicBool;
use std::time::Instant;

use birchpad_core::{ChangeSet, Edit, Rope};
use birchpad_syntax::{Syntax, by_id, config};

fn main() {
    let lines: usize = std::env::args()
        .nth(1)
        .and_then(|n| n.parse().ok())
        .unwrap_or(10_000);
    rust(lines);
    markdown(lines);
}

/// A Rust file of `lines` lines.
fn rust(lines: usize) {
    let unit = "fn item(x: u32) -> u32 {\n    let y = x * 2; // double\n    format!(\"{y}\").len() as u32\n}\n";
    let source = unit.repeat(lines / 4);
    let mut text = Rope::from_str(&source);
    println!("{} lines, {} KB", lines, source.len() / 1024);

    let mut syntax = Syntax::new(config(by_id("rust").unwrap()).unwrap());
    let started = Instant::now();
    let tree = syntax
        .parse_job(text.clone())
        .run(&AtomicBool::new(false))
        .unwrap();
    syntax.install(tree);
    println!("full parse:          {:?}", started.elapsed());

    // About one screen: 50 lines of ~25 bytes, in the middle of the file.
    let middle = unit.len() * (lines / 8);
    let visible = middle..middle + 50 * 25;
    let started = Instant::now();
    let spans = syntax.highlights(&text, visible.clone());
    println!(
        "highlight a screen:  {:?} ({} spans)",
        started.elapsed(),
        spans.len()
    );

    // Type a character into a comment, as when editing.
    let at = middle + unit.find("double").unwrap();
    let changes = ChangeSet::from_edits(&text, [Edit::insert(at, "x")]).unwrap();
    let old = text.clone();
    changes.apply(&mut text);
    let started = Instant::now();
    syntax.edit(&old, &changes);
    let edited = started.elapsed();
    let started = Instant::now();
    let tree = syntax
        .parse_job(text.clone())
        .run(&AtomicBool::new(false))
        .unwrap();
    syntax.install(tree);
    println!("edit tree:           {edited:?}");
    println!("incremental reparse: {:?}", started.elapsed());

    let started = Instant::now();
    let spans = syntax.highlights(&text, visible);
    println!(
        "highlight after it:  {:?} ({} spans)",
        started.elapsed(),
        spans.len()
    );

    // Break the syntax at the top level (`fn` becomes `xfn`), as while typing a new item.
    let changes = ChangeSet::from_edits(&text, [Edit::insert(middle, "x")]).unwrap();
    let old = text.clone();
    changes.apply(&mut text);
    syntax.edit(&old, &changes);
    let started = Instant::now();
    let tree = syntax
        .parse_job(text.clone())
        .run(&AtomicBool::new(false))
        .unwrap();
    syntax.install(tree);
    println!("reparse, broken:     {:?}", started.elapsed());
    let started = Instant::now();
    let spans = syntax.highlights(&text, middle..middle + 50 * 25);
    println!(
        "highlight, broken:   {:?} ({} spans)",
        started.elapsed(),
        spans.len()
    );
}

/// A Markdown document with a layer per paragraph and per code block: the worst case for
/// embedded languages.
fn markdown(lines: usize) {
    let unit = "## Section

Some *text* with `code` and a [link](https://example.org).

```rust
fn f(x: u32) -> u32 { x * 2 }
```

";
    let source = unit.repeat(lines / 8);
    let mut text = Rope::from_str(&source);
    println!(
        "
Markdown: {} lines, {} KB",
        lines,
        source.len() / 1024
    );

    let mut syntax = Syntax::new(config(by_id("markdown").unwrap()).unwrap());
    let started = Instant::now();
    let parsed = syntax
        .parse_job(text.clone())
        .run(&AtomicBool::new(false))
        .unwrap();
    syntax.install(parsed);
    println!(
        "full parse:          {:?} ({} layers)",
        started.elapsed(),
        syntax.layers().len()
    );

    let middle = unit.len() * (lines / 16);
    let visible = middle..middle + 50 * 25;
    let started = Instant::now();
    let spans = syntax.highlights(&text, visible.clone());
    println!(
        "highlight a screen:  {:?} ({} spans)",
        started.elapsed(),
        spans.len()
    );

    // Type into a paragraph in the middle.
    let at = middle + unit.find("text").unwrap();
    let changes = ChangeSet::from_edits(&text, [Edit::insert(at, "x")]).unwrap();
    let old = text.clone();
    changes.apply(&mut text);
    let started = Instant::now();
    syntax.edit(&old, &changes);
    let edited = started.elapsed();
    let started = Instant::now();
    let parsed = syntax
        .parse_job(text.clone())
        .run(&AtomicBool::new(false))
        .unwrap();
    syntax.install(parsed);
    let reparsed = syntax
        .layers()
        .iter()
        .filter(|layer| !layer.reused())
        .count();
    println!("edit trees:          {edited:?}");
    println!(
        "incremental reparse: {:?} ({reparsed} layers parsed again)",
        started.elapsed()
    );
    let started = Instant::now();
    let spans = syntax.highlights(&text, visible);
    println!(
        "highlight after it:  {:?} ({} spans)",
        started.elapsed(),
        spans.len()
    );
}
