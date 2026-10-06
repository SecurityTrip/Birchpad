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

/// The error and missing nodes under `node`, with their text.
fn errors(node: birchpad_syntax::Node, source: &str, out: &mut Vec<String>) {
    if node.is_error() || node.is_missing() {
        let range = node.byte_range();
        let what = if node.is_missing() {
            "missing"
        } else {
            "error"
        };
        out.push(format!(
            "{what} at line {}: {:?}",
            node.start_position().row + 1,
            &source[range.start..range.end.min(range.start + 60)]
        ));
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.has_error() {
            errors(child, source, out);
        }
    }
}

/// A sample with a syntax error would test the grammar's error recovery, not its highlights:
/// every sample parses cleanly, the languages embedded in it too.
#[test]
fn every_sample_parses_cleanly() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/samples");
    let mut broken = Vec::new();
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        let source = std::fs::read_to_string(&path).unwrap();
        let language = detect(Some(&path), source.lines().next().unwrap_or_default()).unwrap();
        let text = Rope::from_str(&source);
        let mut syntax = Syntax::new(config(language).unwrap());
        let parsed = syntax
            .parse_job(text.clone())
            .run(&AtomicBool::new(false))
            .unwrap();
        syntax.install(parsed);
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let mut found = Vec::new();
        errors(syntax.tree().unwrap().root_node(), &source, &mut found);
        for layer in syntax.layers() {
            errors(layer.tree().root_node(), &source, &mut found);
        }
        broken.extend(found.into_iter().map(|error| format!("{name}: {error}")));
    }
    assert!(
        broken.is_empty(),
        "samples with syntax errors:\n{}",
        broken.join("\n")
    );
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
