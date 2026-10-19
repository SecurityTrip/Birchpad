use std::sync::atomic::AtomicBool;

use birchpad_core::{ChangeSet, Edit};
use proptest::prelude::*;

use super::*;
use crate::language::by_id;

fn parse(text: &Rope, old: Option<&Syntax>) -> Syntax {
    let config = config(by_id("rust").unwrap()).unwrap();
    let mut syntax = Syntax::new(config);
    let job = old.unwrap_or(&syntax).parse_job(text.clone());
    let parsed = job.run(&AtomicBool::new(false)).unwrap();
    syntax.install(parsed);
    syntax
}

fn piece() -> impl Strategy<Value = String> {
    proptest::sample::select(
        &[
            "fn f() {",
            "}",
            "let x = 1;",
            "\"s\"",
            "// c\n",
            "\n",
            " ",
            "(",
            ")",
            "struct S;",
            "/*",
            "*/",
            "'a'",
            "0x1f",
            "+",
            "x",
        ][..],
    )
    .prop_map(str::to_owned)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]
    #[test]
    fn a_tree_reparsed_after_an_edit_equals_a_fresh_parse(
        pieces in proptest::collection::vec(piece(), 0..30),
        edits in proptest::collection::vec((any::<prop::sample::Index>(), 0usize..12, piece()), 1..4),
    ) {
        let mut text = Rope::from_str(&pieces.concat());
        let mut syntax = parse(&text, None);
        for (at, deleted, inserted) in edits {
            // The pieces are ASCII: every byte is a character boundary.
            let start = at.index(text.len() + 1);
            let end = (start + deleted).min(text.len());
            let changes =
                ChangeSet::from_edits(&text, vec![Edit::replace(start..end, inserted)]).unwrap();
            let old = text.clone();
            changes.apply(&mut text);
            syntax.edit(&old, &changes);
            let parsed = syntax.parse_job(text.clone()).run(&AtomicBool::new(false)).unwrap();
            syntax.install(parsed);
            let fresh = parse(&text, None);
            let reparsed = syntax.tree().unwrap().root_node();
            let fresh_root = fresh.tree().unwrap().root_node();
            // Tree-sitter recovers from errors by the old tree too, so text with errors may
            // reparse into other error nodes than a fresh parse makes (`"s"/*fn f() {/'a'`):
            // there the tree only has to cover the whole text.
            if fresh_root.has_error() {
                prop_assert_eq!(reparsed.end_byte(), text.len(), "{}", text);
            } else {
                prop_assert_eq!(reparsed.to_sexp(), fresh_root.to_sexp(), "{}", text);
            }
        }
    }
}
