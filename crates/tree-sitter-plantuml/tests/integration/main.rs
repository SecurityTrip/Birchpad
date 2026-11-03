//! Tests of the PlantUML grammar: what it makes of diagrams, blocks and lines, of a text being
//! typed, and of an empty one.

use tree_sitter::{Node, Parser, Tree};

fn parse(text: &str) -> Tree {
    let mut parser = Parser::new();
    parser
        .set_language(&birchpad_tree_sitter_plantuml::LANGUAGE.into())
        .unwrap();
    parser.parse(text, None).unwrap()
}

/// The kinds of the named nodes under `node`, depth first, with their depth.
fn kinds(node: Node, depth: usize, out: &mut Vec<(usize, String)>) {
    if node.is_named() {
        out.push((depth, node.kind().to_owned()));
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        kinds(child, depth + 1, out);
    }
}

/// The kinds of the nodes holding a line or more: diagrams, blocks and lines.
fn structure(text: &str) -> Vec<(usize, String)> {
    let tree = parse(text);
    assert!(
        !tree.root_node().has_error(),
        "{}",
        tree.root_node().to_sexp()
    );
    let mut out = Vec::new();
    kinds(tree.root_node(), 0, &mut out);
    out.retain(|(_, kind)| {
        [
            "diagram",
            "block",
            "if_block",
            "procedure",
            "brace_block",
            "note",
            "statement",
            "participant_declaration",
            "section",
            "title",
            "include",
            "comment",
            "block_comment",
        ]
        .contains(&kind.as_str())
    });
    out
}

fn pairs(list: &[(usize, &str)]) -> Vec<(usize, String)> {
    list.iter()
        .map(|(depth, kind)| (*depth, (*kind).to_owned()))
        .collect()
}

#[test]
fn a_sequence_diagram_is_lines_in_blocks() {
    let text = "' first\n@startuml\n!include common.iuml\ntitle Orders\n\
                participant \"Web shop\" as Shop\n== Ordering ==\nUser -> Shop : order\n\
                alt paid\n  Shop --> User : ok\nelse\n  Shop ->x User\nend\n\
                box \"Inside\"\n  participant Store\nend box\n@enduml\n";
    assert_eq!(
        structure(text),
        pairs(&[
            (1, "comment"),
            (1, "diagram"),
            (2, "include"),
            (2, "title"),
            (2, "participant_declaration"),
            (2, "section"),
            (2, "statement"),
            (2, "block"),
            (3, "statement"),
            (3, "statement"),
            (3, "statement"),
            (2, "block"),
            (3, "participant_declaration"),
        ])
    );
}

#[test]
fn preprocessor_blocks_braces_and_notes_are_blocks() {
    let text = "@startuml\n!procedure $call($a)\n  $a -> B\n!endprocedure\n\
                !if %true()\n  A -> B\n!else\n  B -> A\n!endif\n\
                package Shop {\n  class Order {\n    +id : int\n  }\n}\n\
                note left of A\n  alt is text here\nend note\n/' a\ncomment '/\n@enduml";
    assert_eq!(
        structure(text),
        pairs(&[
            (1, "diagram"),
            (2, "procedure"),
            (3, "statement"),
            (2, "if_block"),
            (3, "statement"),
            (3, "statement"),
            (2, "brace_block"),
            (3, "brace_block"),
            (4, "statement"),
            (2, "note"),
            (2, "block_comment"),
        ])
    );
}

#[test]
fn a_diagram_being_typed_has_no_end_yet() {
    assert_eq!(
        structure("@startuml\nA -> B\n"),
        pairs(&[(1, "diagram"), (2, "statement")])
    );
    assert_eq!(
        structure("@startuml"),
        pairs(&[(1, "diagram")]),
        "the last line without its line break"
    );
    assert_eq!(
        structure("@startuml\nA -> B\n@enduml"),
        pairs(&[(1, "diagram"), (2, "statement")])
    );
    let tree = parse("@startuml\n");
    assert_eq!(tree.root_node().child(0).unwrap().kind(), "diagram");
}

#[test]
fn an_empty_text_is_an_empty_document() {
    let tree = parse("");
    assert_eq!(tree.root_node().kind(), "document");
    assert_eq!(tree.root_node().child_count(), 0);
    assert!(structure("\n\n").is_empty());
}

#[test]
fn an_end_without_its_block_is_a_mistake() {
    let tree = parse("@startuml\nA -> B\nend box\n!endif\n@enduml\n");
    assert!(tree.root_node().has_error());
    // An `end note` inside a block ends the note, not the block.
    let text = "alt x\nnote over A\nend\nend note\nend\n";
    assert_eq!(
        structure(text),
        pairs(&[(1, "block"), (2, "note")]),
        "`end` in a note is text"
    );
}

#[test]
fn the_node_types_are_listed() {
    let types = birchpad_tree_sitter_plantuml::NODE_TYPES;
    for kind in ["\"diagram\"", "\"include\"", "\"arrow\"", "\"label\""] {
        assert!(types.contains(kind), "{kind}");
    }
}
