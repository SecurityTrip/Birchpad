//! Every color a language's highlight query can give shows up in that language's sample
//! (`crates/syntax/tests/samples`).
//!
//! A capture can be hidden: another pattern for the same node wins (JSON's keys were colored
//! as strings), or the capture sits on the wrong node (TOML's keys took the color of table
//! names). Snapshots only show what a sample has; this checks the other way round, from the
//! query: each themed color the query can produce must appear in the sample, highlighted
//! whole. Colors are compared, not names, since several names share one color.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::atomic::AtomicBool;

use birchpad_core::Rope;
use birchpad_syntax::{Syntax, config, detect};

use super::theme;

/// Captures no sample shows, and why. Each is checked to be a capture of that language's query,
/// so the list cannot go stale.
const EXEMPT: &[(&str, &str, &str)] = &[
    (
        "diff",
        "attribute",
        "Birchpad's diff.scm colors hunk headers as diff.delta instead",
    ),
    (
        "diff",
        "string",
        "Birchpad's diff.scm colors added lines as diff.plus instead",
    ),
    (
        "makefile",
        "error",
        "marks a malformed automatic variable; samples parse cleanly",
    ),
    ("r", "error", "marks syntax errors; samples parse cleanly"),
    ("xml", "error", "marks syntax errors; samples parse cleanly"),
    (
        "nginx",
        "attribute",
        "the grammar's own later @keyword pattern takes directive names",
    ),
    (
        "asm",
        "constant",
        "`const` lines are in the grammar, not in NASM or GAS",
    ),
    (
        "scala",
        "tag.attribute",
        "XML literals are Scala 2 only; the sample is Scala 3",
    ),
];

/// The colors `source`, a file named `path`, shows; and the colors its language's query can
/// give, each with the captures behind it.
fn sample_colors(
    path: &Path,
    source: &str,
) -> (String, BTreeSet<u32>, BTreeMap<u32, BTreeSet<String>>) {
    let language = detect(Some(path), source.lines().next().unwrap_or_default()).unwrap();
    let config = config(language).unwrap();
    let text = Rope::from_str(source);
    let mut syntax = Syntax::new(config.clone());
    let parsed = syntax
        .parse_job(text.clone())
        .run(&AtomicBool::new(false))
        .unwrap();
    syntax.install(parsed);
    let shown: BTreeSet<u32> = syntax
        .all_highlights(&text, 0..text.len())
        .into_iter()
        .filter_map(|(_, highlight)| theme::syntax_style(Some(language.id), highlight)?.color)
        .map(|color| color >> 8)
        .collect();
    let mut possible: BTreeMap<u32, BTreeSet<String>> = BTreeMap::new();
    for (capture, highlight) in config.capture_highlights() {
        let exempt = EXEMPT
            .iter()
            .any(|(id, name, _)| *id == language.id && *name == capture);
        if exempt {
            continue;
        }
        if let Some(color) =
            theme::syntax_style(Some(language.id), highlight).and_then(|style| style.color)
        {
            possible.entry(color >> 8).or_default().insert(capture);
        }
    }
    (language.id.to_owned(), shown, possible)
}

/// The colors `source`'s query can give that it does not show, as `#rrggbb from captures`.
fn missing_colors(path: &Path, source: &str) -> Vec<String> {
    let (_, shown, possible) = sample_colors(path, source);
    possible
        .into_iter()
        .filter(|(color, _)| !shown.contains(color))
        .map(|(color, captures)| {
            let captures: Vec<_> = captures.into_iter().collect();
            format!("#{color:06x} from {}", captures.join(", "))
        })
        .collect()
}

#[test]
fn exemptions_name_captures_their_queries_have() {
    for (id, capture, why) in EXEMPT {
        let language = birchpad_syntax::by_id(id).unwrap_or_else(|| panic!("no language {id}"));
        let config = config(language).unwrap();
        assert!(
            config
                .capture_highlights()
                .iter()
                .any(|(name, _)| name == capture),
            "{id} has no @{capture} any more ({why}): drop the exemption"
        );
    }
}

#[test]
fn every_color_of_every_query_shows_in_its_sample() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../syntax/tests/samples");
    let mut paths: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    paths.sort();
    assert!(paths.len() >= 50, "only {} samples", paths.len());
    let mut missing = Vec::new();
    for path in paths {
        let source = std::fs::read_to_string(&path).unwrap();
        let name = path.file_name().unwrap().to_string_lossy();
        missing.extend(
            missing_colors(&path, &source)
                .into_iter()
                .map(|color| format!("{name}: {color}")),
        );
    }
    assert!(
        missing.is_empty(),
        "colors a query can give that its sample does not show:\n{}",
        missing.join("\n")
    );
}

#[test]
fn a_sample_without_a_construct_misses_its_color() {
    let json = Path::new("sample.json");
    // Everything JSON's query colors, on one line.
    let full = r#"{"key": "s\n", "n": 1, "b": [true, null]} // comment"#;
    assert_eq!(missing_colors(json, full), Vec::<String>::new());
    // Without the comment, the comment's color is missing, and only it.
    let missing = missing_colors(json, r#"{"key": "s\n", "n": 1, "b": [true, null]}"#);
    assert_eq!(missing.len(), 1, "{missing:?}");
    assert!(missing[0].ends_with("from comment"), "{missing:?}");
    // A key alone shows the key color but not a string's.
    let missing = missing_colors(json, r#"{"key": 1}"#);
    assert!(
        missing.iter().any(|color| color.ends_with("from string")),
        "{missing:?}"
    );
    assert!(
        !missing.iter().any(|color| color.contains("key")),
        "{missing:?}"
    );
}

#[test]
fn an_empty_sample_misses_every_color() {
    let json = Path::new("sample.json");
    let (_, shown, possible) = sample_colors(json, "");
    assert!(shown.is_empty());
    assert_eq!(missing_colors(json, "").len(), possible.len());
    assert!(possible.len() >= 5, "{possible:?}");
}
