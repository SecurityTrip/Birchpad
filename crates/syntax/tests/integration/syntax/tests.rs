use birchpad_core::ChangeSet;
use birchpad_core::Edit;
use birchpad_core::Rope;
use birchpad_syntax::*;
use birchpad_syntax::{LANGUAGES, by_id};
use std::ops::Range;
use std::sync::atomic::AtomicBool;

fn parse(language: &str, text: &Rope) -> Syntax {
    let config = config(by_id(language).unwrap()).unwrap();
    let mut syntax = Syntax::new(config);
    let parsed = syntax
        .parse_job(text.clone())
        .run(&AtomicBool::new(false))
        .unwrap();
    syntax.install(parsed);
    syntax
}

fn named(spans: &[(Range<usize>, Highlight)], text: &str) -> Vec<(String, &'static str)> {
    spans
        .iter()
        .map(|(range, highlight)| (text[range.clone()].to_owned(), highlight.name()))
        .collect()
}

/// Applies `edits` to the text and the syntax, and parses again.
fn edit_and_parse(syntax: &mut Syntax, text: &mut Rope, edits: Vec<Edit>) {
    let changes = ChangeSet::from_edits(text, edits).unwrap();
    let old = text.clone();
    changes.apply(text);
    syntax.edit(&old, &changes);
    let parsed = syntax
        .parse_job(text.clone())
        .run(&AtomicBool::new(false))
        .unwrap();
    syntax.install(parsed);
}

fn layer_summary(syntax: &Syntax, text: &Rope) -> Vec<(&'static str, String)> {
    syntax
        .layers()
        .iter()
        .map(|layer| {
            let content: String = layer
                .ranges()
                .iter()
                .map(|range| text.slice(range.start_byte..range.end_byte).to_string())
                .collect();
            (layer.language().id, content)
        })
        .collect()
}

#[test]
fn every_query_compiles() {
    for language in all_languages() {
        if let Err(error) = config(language) {
            panic!("{error}");
        }
    }
}

#[test]
fn highlights_rust() {
    let source = "fn main() {\n    // hi\n    let x = \"s\";\n}\n";
    let text = Rope::from_str(source);
    let syntax = parse("rust", &text);
    let spans = named(&syntax.highlights(&text, 0..text.len()), source);
    assert!(spans.contains(&("fn".into(), "keyword")), "{spans:?}");
    assert!(spans.contains(&("main".into(), "function")), "{spans:?}");
    assert!(spans.contains(&("// hi".into(), "comment")), "{spans:?}");
    assert!(spans.contains(&("\"s\"".into(), "string")), "{spans:?}");
    // Only the requested range.
    let tail = syntax.highlights(&text, 22..text.len());
    assert!(tail.iter().all(|(range, _)| range.start >= 22));
}

#[test]
fn specific_patterns_win_over_general_ones() {
    // C's query starts with `(identifier) @variable` and later says call targets are
    // functions.
    let source = "int main(void) { return puts(\"x\"); }";
    let text = Rope::from_str(source);
    let syntax = parse("c", &text);
    let spans = named(&syntax.highlights(&text, 0..text.len()), source);
    assert!(spans.contains(&("puts".into(), "function")), "{spans:?}");
}

#[test]
fn incremental_parse_matches_a_fresh_one() {
    let mut text = Rope::from_str("fn a() {}\n\nfn b() { let x = 1; }\n");
    let mut syntax = parse("rust", &text);
    let edits = [
        vec![Edit::insert(9, "\nstruct S { f: u8 }")],
        vec![Edit::replace(3..4, "c"), Edit::delete(20..22)],
        vec![
            Edit::insert(0, "// lead\n"),
            Edit::insert(text.len(), "\n// end"),
        ],
    ];
    for batch in edits {
        let len = text.len();
        let batch: Vec<Edit> = batch
            .into_iter()
            .map(|e| Edit::replace(e.range.start.min(len)..e.range.end.min(len), e.text))
            .collect();
        edit_and_parse(&mut syntax, &mut text, batch);
        let fresh = parse("rust", &text);
        assert_eq!(
            syntax.tree().unwrap().root_node().to_sexp(),
            fresh.tree().unwrap().root_node().to_sexp(),
            "{text}"
        );
    }
}

#[test]
fn a_cancelled_parse_returns_nothing() {
    let text = Rope::from_str(&"fn f() { 1 + 2; }\n".repeat(20_000));
    let config = config(by_id("rust").unwrap()).unwrap();
    let syntax = Syntax::new(config);
    assert!(syntax.parse_job(text).run(&AtomicBool::new(true)).is_none());
}

#[test]
fn scripts_and_styles_in_html_are_highlighted() {
    let source = "<p>if</p>\n<script>const x = 1;</script>\n<style>p { color: red; }</style>\n";
    let text = Rope::from_str(source);
    let syntax = parse("html", &text);
    assert_eq!(
        layer_summary(&syntax, &text),
        [
            ("javascript", "const x = 1;".to_owned()),
            ("css", "p { color: red; }".to_owned()),
        ]
    );
    let spans = named(&syntax.highlights(&text, 0..text.len()), source);
    assert!(spans.contains(&("const".into(), "keyword")), "{spans:?}");
    assert!(spans.contains(&("color".into(), "property")), "{spans:?}");
    // "if" in the text is not a keyword: only the script is JavaScript.
    assert!(!spans.contains(&("if".into(), "keyword")), "{spans:?}");
}

#[test]
fn markdown_highlights_code_blocks_and_inline_syntax() {
    let source = "# Title\n\nSome *emphasis* and `code`.\n\n```rust\nfn main() {}\n```\n\n```unknown\nfn x\n```\n";
    let text = Rope::from_str(source);
    let syntax = parse("markdown", &text);
    let languages: Vec<&str> = layer_summary(&syntax, &text)
        .into_iter()
        .map(|(language, _)| language)
        .collect();
    assert!(languages.contains(&"rust"), "{languages:?}");
    assert!(languages.contains(&"markdown_inline"), "{languages:?}");
    let spans = named(&syntax.highlights(&text, 0..text.len()), source);
    assert!(spans.contains(&("fn".into(), "keyword")), "{spans:?}");
    assert!(spans.contains(&("main".into(), "function")), "{spans:?}");
    assert!(
        spans
            .iter()
            .any(|(token, name)| token.contains("emphasis") && *name == "markup.italic"),
        "{spans:?}"
    );
    // A block in a language without a grammar stays as Markdown shows code.
    assert!(!spans.contains(&("x".into(), "variable")), "{spans:?}");
}

#[test]
fn php_is_html_around_its_code() {
    let source = "<div class=\"a\"><?php echo $x; ?></div>\n";
    let text = Rope::from_str(source);
    let syntax = parse("php", &text);
    assert_eq!(layer_summary(&syntax, &text)[0].0, "html");
    let spans = named(&syntax.highlights(&text, 0..text.len()), source);
    assert!(spans.contains(&("div".into(), "tag")), "{spans:?}");
    assert!(spans.contains(&("echo".into(), "keyword")), "{spans:?}");
}

#[test]
fn layers_follow_edits_and_keep_unchanged_trees() {
    let mut text =
        Rope::from_str("```js\nlet a = 1;\n```\n\ntext\n\n```python\ndef f(): pass\n```\n");
    let mut syntax = parse("markdown", &text);
    let python = |syntax: &Syntax| {
        syntax
            .layers()
            .iter()
            .find(|layer| layer.language().id == "python")
            .map(|layer| (layer.extent(), layer.reused()))
            .unwrap()
    };
    let (extent, _) = python(&syntax);

    // Typing in the JavaScript block shifts the Python block, whose tree stays.
    edit_and_parse(&mut syntax, &mut text, vec![Edit::insert(11, "bc")]);
    let (moved, reused) = python(&syntax);
    assert_eq!(moved, extent.start + 2..extent.end + 2);
    assert!(reused, "an unchanged layer keeps its tree");
    let source = text.to_string();
    let spans = named(&syntax.highlights(&text, 0..text.len()), &source);
    assert!(spans.contains(&("abc".into(), "variable")), "{spans:?}");
    assert!(spans.contains(&("def".into(), "keyword")), "{spans:?}");

    // Editing the Python block reparses it.
    let at = source.find("pass").unwrap();
    edit_and_parse(
        &mut syntax,
        &mut text,
        vec![Edit::replace(at..at + 4, "return 1")],
    );
    assert!(!python(&syntax).1, "a changed layer is parsed again");
    let source = text.to_string();
    let spans = named(&syntax.highlights(&text, 0..text.len()), &source);
    assert!(spans.contains(&("return".into(), "keyword")), "{spans:?}");

    // The same as parsing the new text afresh.
    let fresh = parse("markdown", &text);
    assert_eq!(layer_summary(&syntax, &text), layer_summary(&fresh, &text));
}

#[test]
fn typescript_scripts_in_svelte_win_over_javascript() {
    let source = "<script lang=\"ts\">let n: number = 1;</script>\n<p>{n + 1}</p>\n";
    let text = Rope::from_str(source);
    let syntax = parse("svelte", &text);
    let languages: Vec<&str> = layer_summary(&syntax, &text)
        .into_iter()
        .map(|(language, _)| language)
        .collect();
    assert_eq!(languages, ["typescript", "javascript"]);
}

#[test]
fn lua_patterns_become_regular_expressions() {
    assert_eq!(
        lua_pattern_to_regex("^[%u@][%u%d_]+$"),
        "^[A-Z@][A-Z\\\\d_]+$"
    );
    assert_eq!(lua_pattern_to_regex("^%a%w*$"), "^[A-Za-z][A-Za-z0-9]*$");
    assert_eq!(lua_pattern_to_regex("^%-%-.-$"), "^\\\\-\\\\-.*?$");
    assert_eq!(lua_pattern_to_regex("[^%s]"), "[^\\\\s]");
    let query = "((identifier) @constant (#lua-match? @constant \"^[%u_]+$\"))";
    assert_eq!(
        lua_matches_to_regex(query),
        "((identifier) @constant (#match? @constant \"^[A-Z_]+$\"))"
    );
}

#[test]
fn every_lua_class_inside_and_outside_sets() {
    for (lua, regex) in [
        ("%a", "[A-Za-z]"),
        ("[%a]", "[A-Za-z]"),
        ("%d", "\\\\d"),
        ("[%d]", "[\\\\d]"),
        ("%l", "[a-z]"),
        ("[%l]", "[a-z]"),
        ("%u", "[A-Z]"),
        ("[%u]", "[A-Z]"),
        ("%w", "[A-Za-z0-9]"),
        ("[%w]", "[A-Za-z0-9]"),
        ("%x", "[0-9A-Fa-f]"),
        ("[%x]", "[0-9A-Fa-f]"),
        ("%s", "\\\\s"),
        ("%p", "[[:punct:]]"),
        ("[%p]", "[[:punct:]]"),
    ] {
        assert_eq!(lua_pattern_to_regex(lua), regex, "{lua}");
    }
}

#[test]
fn lua_patterns_at_their_edges() {
    // An unknown class is the letter; `%%` a percent sign; a `%` at the very end nothing.
    assert_eq!(lua_pattern_to_regex("%z"), "z");
    assert_eq!(lua_pattern_to_regex("100%%"), "100%");
    assert_eq!(lua_pattern_to_regex("end%"), "end");
    assert_eq!(lua_pattern_to_regex(""), "");
    // A `]` outside a set, and a `^` that does not start a set.
    assert_eq!(lua_pattern_to_regex("a]^b"), "a]^b");
}

#[test]
fn lua_matches_in_queries_at_their_edges() {
    // Two in one query, an escaped quote inside the pattern, none at all.
    let two = r#"(#lua-match? @a "^%u") (#lua-match? @b "%d\"x")"#;
    assert_eq!(
        lua_matches_to_regex(two),
        r#"(#match? @a "^[A-Z]") (#match? @b "\\d\"x")"#
    );
    assert_eq!(lua_matches_to_regex("(#eq? @a \"x\")"), "(#eq? @a \"x\")");
    // A predicate without its string is left as it is (the query then fails to compile).
    assert_eq!(lua_matches_to_regex("(#lua-match? @a"), "(#match? @a");
    // An unterminated string runs to the end.
    assert_eq!(
        lua_matches_to_regex("(#lua-match? @a \"%d"),
        "(#match? @a \"\\\\d"
    );
}

#[test]
fn a_syntax_without_a_tree_has_no_highlights() {
    let config = config(by_id("rust").unwrap()).unwrap();
    assert!(format!("{config:?}").contains("rust"));
    let mut syntax = Syntax::new(config);
    let text = Rope::from_str("fn main() {}");
    assert!(syntax.highlights(&text, 0..text.len()).is_empty());
    // Following an edit before the first parse is no problem.
    let changes = ChangeSet::from_edits(&text, [Edit::insert(0, "x")]).unwrap();
    syntax.edit(&text, &changes);
    assert!(syntax.layers().is_empty());
}

#[test]
fn highlights_of_empty_and_overlong_ranges() {
    let text = Rope::from_str("fn main() {}");
    let syntax = parse("rust", &text);
    assert!(syntax.highlights(&text, 3..3).is_empty());
    assert!(syntax.highlights(&text, 50..60).is_empty(), "past the end");
    // A range running past the end is cut there.
    let whole = syntax.highlights(&text, 0..text.len());
    assert_eq!(syntax.highlights(&text, 0..1000), whole);
    // The same without the time budget.
    assert!(syntax.all_highlights(&text, 3..3).is_empty());
    assert!(syntax.all_highlights(&text, 50..60).is_empty());
    let all = syntax.all_highlights(&text, 0..text.len());
    assert_eq!(syntax.all_highlights(&text, 0..1000), all);
    assert!(!all.is_empty());
}

#[test]
fn all_highlights_reach_the_end_of_a_deep_tree() {
    // A file of thousands of nested parentheses, as a typo can make one: the screen's
    // query may stop at its budget, never with more than the query without one.
    let source = format!(
        "let x = {}1{};\nlet s = \"end\\n\";\n",
        "(".repeat(5000),
        ")".repeat(5000)
    );
    let text = Rope::from_str(&source);
    let syntax = parse("javascript", &text);
    let all = syntax.all_highlights(&text, 0..text.len());
    let budgeted = syntax.highlights(&text, 0..text.len());
    assert!(budgeted.len() <= all.len());
    let end = source.find("\"end").unwrap();
    let named = named(&all, &source);
    assert!(
        named
            .iter()
            .any(|(text, name)| text.starts_with("\"end") && *name == "string"),
        "no string at {end}: {:?}",
        &named[named.len().saturating_sub(5)..]
    );
}

#[test]
fn the_language_at_a_position_is_the_embedded_one() {
    let source = "<p>a</p><script>let x = 1;</script><style>p { }</style>";
    let text = Rope::from_str(source);
    let syntax = parse("html", &text);
    let at = |needle: &str| syntax.language_at(source.find(needle).unwrap()).id;
    assert_eq!(at("a</p>"), "html");
    assert_eq!(at("x = 1"), "javascript");
    assert_eq!(at("{ }"), "css");
    assert_eq!(syntax.language_at(0).id, "html", "the start");
    assert_eq!(syntax.language_at(source.len()).id, "html", "the end");
    assert_eq!(
        syntax.language_at(source.len() + 10).id,
        "html",
        "past the end"
    );
    let plain = parse("rust", &Rope::from_str("fn f() {}"));
    assert_eq!(plain.language_at(3).id, "rust", "nothing embedded");
}

#[test]
fn embedded_languages_stop_at_the_depth_limit() {
    // Markdown in a Markdown code block, eight deep, each fence longer than the inner one.
    let mut source = String::from("deepest *text*\n");
    for level in 0..8 {
        let fence = "`".repeat(3 + level);
        source = format!("{fence}markdown\n{source}{fence}\n");
    }
    let text = Rope::from_str(&source);
    let syntax = parse("markdown", &text);
    let depths: Vec<u8> = syntax.layers().iter().map(Layer::depth).collect();
    assert!(!depths.is_empty());
    assert!(depths.iter().all(|&depth| depth <= MAX_DEPTH), "{depths:?}");
    assert!(
        depths.contains(&MAX_DEPTH),
        "nesting goes down to the limit: {depths:?}"
    );
    // A first parse reuses nothing; highlighting still covers the whole text.
    assert!(syntax.layers().iter().all(|layer| !layer.reused()));
    assert!(!syntax.highlights(&text, 0..text.len()).is_empty());
}

#[test]
fn every_language_highlights_something() {
    // Catches a grammar whose query compiles but matches nothing (wrong node names).
    for language in LANGUAGES {
        let config = config(language).unwrap();
        assert!(
            config.highlight_pattern_count() > 0,
            "{} has no highlight patterns",
            language.id
        );
    }
}

fn parse_sql(source: &str, escapes: bool) -> Vec<(String, &'static str)> {
    let text = Rope::from_str(source);
    let mut syntax = Syntax::new(config(by_id("sql").unwrap()).unwrap());
    syntax.set_backslash_escapes(escapes);
    let parsed = syntax
        .parse_job(text.clone())
        .run(&AtomicBool::new(false))
        .unwrap();
    syntax.install(parsed);
    named(&syntax.highlights(&text, 0..text.len()), source)
}

#[test]
fn sql_strings_take_backslash_escapes_when_asked_to() {
    let source = r"SELECT 'it\'s', 2 FROM t WHERE a = 'b\';";
    let on = parse_sql(source, true);
    assert!(on.contains(&(r"'it\'s'".to_owned(), "string")), "{on:?}");
    assert!(on.contains(&("2".to_owned(), "number")), "{on:?}");
    // An escaped backslash does not escape the quote after it.
    assert!(on.contains(&(r"'b\'".to_owned(), "string")), "{on:?}");
    // Off, as standard SQL reads it: the string ends at the quote after the backslash.
    let off = parse_sql(source, false);
    assert!(!off.contains(&(r"'it\'s'".to_owned(), "string")), "{off:?}");
    // Doubled quotes are a quote either way.
    let doubled = r"SELECT 'it''s', 2;";
    for escapes in [false, true] {
        let spans = parse_sql(doubled, escapes);
        assert!(
            spans.contains(&("'it''s'".to_owned(), "string")),
            "{spans:?}"
        );
    }
}

#[test]
fn backslash_escapes_are_sql_s_alone() {
    // Another language reads its text as it is, the setting on or not: in Pascal, as in
    // standard SQL, a backslash escapes nothing.
    let text = Rope::from_str("begin s := 'a\\'; t := 'b' end.");
    let config = config(by_id("pascal").unwrap()).unwrap();
    let mut syntax = Syntax::new(config);
    assert!(syntax.set_backslash_escapes(true));
    assert!(!syntax.set_backslash_escapes(true), "no change");
    let parsed = syntax
        .parse_job(text.clone())
        .run(&AtomicBool::new(false))
        .unwrap();
    syntax.install(parsed);
    let plain = parse("pascal", &text);
    assert_eq!(
        syntax.tree().unwrap().root_node().to_sexp(),
        plain.tree().unwrap().root_node().to_sexp()
    );
}

#[test]
fn backslash_escaped_quotes_read_as_backslashes() {
    let read = |source: &str, from: usize| {
        let text = Rope::from_str(source);
        backslash_escaped(&text, from, &source.as_bytes()[from..]).into_owned()
    };
    assert_eq!(read(r"'a\'b'", 0), br"'a\b'");
    assert_eq!(read(r#""a\"b""#, 0), br#""a\b""#);
    // An even run of backslashes escapes itself, an odd one the quote.
    assert_eq!(read(r"'a\'", 0), br"'a\'");
    assert_eq!(read(r"'a\\'", 0), br"'a\\\\");
    // Other characters stay.
    assert_eq!(read(r"\n\t\x", 0), br"\n\t\x");
}

#[test]
fn backslash_escapes_read_across_chunks() {
    // The run of backslashes before the chunk counts.
    let text = Rope::from_str(r"x\\'y");
    assert_eq!(backslash_escaped(&text, 4, b"'y").as_ref(), br"\y");
    assert_eq!(backslash_escaped(&text, 3, br"\'y").as_ref(), br"\y");
    // At the very start, past the end, and an empty chunk.
    assert_eq!(backslash_escaped(&text, 0, b"'").as_ref(), b"'");
    assert_eq!(backslash_escaped(&text, 99, b"").as_ref(), b"");
    // Nothing to change: the chunk itself.
    let plain = Rope::from_str("abc");
    assert!(matches!(
        backslash_escaped(&plain, 0, b"abc"),
        std::borrow::Cow::Borrowed(_)
    ));
}
