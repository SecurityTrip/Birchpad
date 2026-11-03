use birchpad_syntax::plantuml::*;

fn target(line: &str) -> Option<&str> {
    include_target(line).map(|range| &line[range])
}

#[test]
fn includes_are_read_by_their_directives() {
    let text = "@startuml\n!include common.puml\n  !include_many  \"with space.iuml\"  \n\
                !include_once once.iuml\n!includesub parts.iuml!PART\n!include numbered.puml!2\n\
                !import lib.zip\n!INCLUDE upper.puml\n!include\ttab.puml\n@enduml";
    assert_eq!(
        include_targets(text),
        [
            "common.puml",
            "with space.iuml",
            "once.iuml",
            "parts.iuml",
            "numbered.puml",
            "lib.zip",
            "upper.puml",
            "tab.puml"
        ]
    );
    // Where the target is in its line.
    assert_eq!(include_target("  !include a.puml"), Some(11..17));
    assert_eq!(include_target("!include \"b c.puml\""), Some(10..18));
}

#[test]
fn includes_that_are_no_files_are_left_out() {
    let text = "!include <C4/C4_Context>\n!include https://example.org/a.puml\n\
                !include $LIB/a.puml\n!includeurl http://x/y.puml\n' !include commented.puml\n\
                !include\n!include   \n!include \"\"\n!includedef x.puml\nnote: !include no.puml";
    assert!(
        include_targets(text).is_empty(),
        "{:?}",
        include_targets(text)
    );
    assert!(include_targets("").is_empty());
    assert_eq!(target("A -> B"), None);
}

#[test]
fn includes_at_the_edges_of_a_line() {
    assert_eq!(
        include_targets("!include last.puml"),
        ["last.puml"],
        "on the last line, without a line end"
    );
    assert_eq!(
        target("!include \"open.puml"),
        Some("open.puml"),
        "a quote not closed"
    );
    assert_eq!(target("!include a!"), Some("a"));
    assert_eq!(target("!include !PART"), None, "a part without its file");
    assert_eq!(target("!include ü.puml"), Some("ü.puml"), "wide characters");
    assert_eq!(target("!import"), None);
}
