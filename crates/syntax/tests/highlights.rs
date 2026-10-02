//! Snapshot tests of syntax highlighting: one small sample per language, highlighted whole.
//! A grammar or query update that changes the colors shows up as a snapshot diff to review.

use std::fmt::Write as _;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use birchpad_core::Rope;
use birchpad_syntax::{LANGUAGES, Syntax, config, detect};

fn render(path: &Path) -> String {
    let source = std::fs::read_to_string(path).unwrap();
    let first_line = source.lines().next().unwrap_or_default();
    let language = detect(Some(path), first_line)
        .unwrap_or_else(|| panic!("no language for {}", path.display()));
    let text = Rope::from_str(&source);
    let mut syntax = Syntax::new(config(language).unwrap());
    let tree = syntax
        .parse_job(text.clone())
        .run(&AtomicBool::new(false))
        .unwrap();
    syntax.install(tree);
    let mut out = format!("language: {}\n", language.id);
    for (range, highlight) in syntax.highlights(&text, 0..text.len()) {
        let token = source[range].replace('\n', "\\n");
        writeln!(out, "{:<24} {token}", highlight.name()).unwrap();
    }
    out
}

#[test]
fn every_language_has_a_sample() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/samples");
    let mut covered = Vec::new();
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        let source = std::fs::read_to_string(&path).unwrap();
        let language = detect(Some(&path), source.lines().next().unwrap_or_default()).unwrap();
        covered.push(language.id);
    }
    for language in LANGUAGES {
        assert!(
            covered.contains(&language.id),
            "no sample for {}",
            language.id
        );
    }
}

#[test]
fn highlight_snapshots() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/samples");
    let mut paths: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    paths.sort();
    for path in paths {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        insta::assert_snapshot!(name, render(&path));
    }
}
