use super::*;

const UDL: &str = r#"<?xml version="1.0" encoding="UTF-8" ?>
<NotepadPlus>
    <UserLang name="Mini" ext="mini .MN" udlVersion="2.1">
        <Settings>
            <Global caseIgnored="yes" />
            <Prefix Keywords1="no" Keywords2="yes" />
        </Settings>
        <KeywordLists>
            <Keywords name="Comments">00// 00# 01 02 03/* 04*/</Keywords>
            <Keywords name="Operators1">+ - = &lt;= &lt;</Keywords>
            <Keywords name="Operators2">and or</Keywords>
            <Keywords name="Folders in code1, open">{</Keywords>
            <Keywords name="Folders in code1, close">}</Keywords>
            <Keywords name="Folders in code2, open">begin</Keywords>
            <Keywords name="Keywords1">if else &quot;end if&quot;</Keywords>
            <Keywords name="Keywords2">$</Keywords>
            <Keywords name="Delimiters">00&quot; 01\ 02&quot; 03&apos; 04 05</Keywords>
        </KeywordLists>
        <Styles>
            <WordsStyle name="DEFAULT" fgColor="000000" bgColor="FFFFFF" fontStyle="0" />
            <WordsStyle name="COMMENTS" fgColor="008000" fontStyle="2" />
            <WordsStyle name="KEYWORDS1" fgColor="0000FF" bgColor="FFFF00" colorStyle="1" fontStyle="1" />
            <WordsStyle name="DELIMITERS1" fgColor="808080" fontStyle="5" />
            <WordsStyle name="KEYWORDS9" fgColor="808080" />
            <WordsStyle name="UNKNOWN" fgColor="808080" />
        </Styles>
    </UserLang>
</NotepadPlus>"#;

fn mini() -> UserLanguage {
    parse_udl(UDL).unwrap().remove(0)
}

fn kinds(text: &str) -> Vec<(&str, UdlKind)> {
    mini()
        .highlight(text, 0)
        .into_iter()
        .map(|(range, kind)| (&text[range], kind))
        .collect()
}

#[test]
fn reads_the_language() {
    let mini = mini();
    assert_eq!(mini.name, "Mini");
    assert_eq!(mini.extensions, ["mini", "mn"]);
    assert!(mini.matches_file("a.MINI") && mini.matches_file("x.mn"));
    assert!(!mini.matches_file("mini") && !mini.matches_file("a.min"));
    let keyword = mini.style(UdlKind::Keyword(0)).unwrap();
    assert_eq!(keyword.color, Some(0x0000ff));
    // colorStyle 1: the foreground only.
    assert_eq!(keyword.background, None);
    assert!(keyword.bold);
    let delimiter = mini.style(UdlKind::Delimiter(0)).unwrap();
    assert!(delimiter.bold && delimiter.underline && !delimiter.italic);
    assert_eq!(mini.default_style.background, Some(0xffffff));
    assert!(mini.style(UdlKind::Number).is_none());
}

#[test]
fn colors_the_text() {
    assert_eq!(
        kinds("IF x <= 10 { y = 'a' } // end\nelse \"q\\\"z\" and $var"),
        [
            ("IF", UdlKind::Keyword(0)),
            ("<=", UdlKind::Operator),
            ("10", UdlKind::Number),
            ("{", UdlKind::FolderCode1),
            ("=", UdlKind::Operator),
            ("'a' } // end", UdlKind::Delimiter(1)),
            ("else", UdlKind::Keyword(0)),
            ("\"q\\\"z\"", UdlKind::Delimiter(0)),
            ("and", UdlKind::Operator),
            ("$var", UdlKind::Keyword(1)),
        ]
    );
}

#[test]
fn comments_and_words() {
    assert_eq!(
        kinds("a/* x\ny */b # c\nbegin 0x1F x+1"),
        [
            ("/* x\ny */", UdlKind::Comment),
            ("# c", UdlKind::LineComment),
            ("begin", UdlKind::FolderCode2),
            ("0x1F", UdlKind::Number),
            ("+", UdlKind::Operator),
            ("1", UdlKind::Number),
        ]
    );
    // Unclosed comments and delimiters run to the end.
    assert_eq!(kinds("/* open"), [("/* open", UdlKind::Comment)]);
    assert_eq!(kinds("\"open\\"), [("\"open\\", UdlKind::Delimiter(0))]);
    assert!(kinds("").is_empty());
    assert!(kinds("plain words only").is_empty());
}

#[test]
fn pieces_before_from_are_left_out() {
    let text = "if a\nelse b";
    let found = mini().highlight(text, 5);
    assert_eq!(found, [(5..9, UdlKind::Keyword(0))]);
}

#[test]
fn lists_and_codes() {
    assert_eq!(words("a \"b c\" d"), ["a", "b c", "d"]);
    assert_eq!(words("\"\" x"), ["\"\"", "x"]);
    assert!(words("  ").is_empty());
    assert_eq!(
        coded("00// 01 03/* x 4"),
        [(0, "//".into()), (1, String::new()), (3, "/*".into())]
    );
}

#[test]
fn refuses_other_files() {
    for xml in [
        "",
        "<x",
        "<NotepadPlus/>",
        "<NotepadPlus><UserLang/></NotepadPlus>",
        "<NotepadPlus><UserLang name=\" \"/></NotepadPlus>",
    ] {
        assert!(parse_udl(xml).is_err(), "{xml}");
    }
    let two = "\u{feff}<NotepadPlus><UserLang name=\"A\"/><UserLang name=\"B\"/></NotepadPlus>";
    assert_eq!(parse_udl(two).unwrap().len(), 2);
}

/// A language with the UDL 2.1 settings: numbers, line comments at the line start, nested
/// delimiters and folding.
const FULL: &str = r#"<NotepadPlus>
    <UserLang name="Full" ext="full">
        <Settings>
            <Global caseIgnored="no" forcePureLC="1" />
        </Settings>
        <KeywordLists>
            <Keywords name="Comments">00# 03/* 04*/</Keywords>
            <Keywords name="Numbers, prefix1">0x</Keywords>
            <Keywords name="Numbers, extras1">a b c d e f</Keywords>
            <Keywords name="Numbers, prefix2">0b</Keywords>
            <Keywords name="Numbers, suffix1">L</Keywords>
            <Keywords name="Numbers, range">..</Keywords>
            <Keywords name="Operators1">+</Keywords>
            <Keywords name="Folders in code1, open">{</Keywords>
            <Keywords name="Folders in code1, close">}</Keywords>
            <Keywords name="Folders in code2, open">begin</Keywords>
            <Keywords name="Folders in code2, middle">else</Keywords>
            <Keywords name="Folders in code2, close">end</Keywords>
            <Keywords name="Folders in comment, open">region</Keywords>
            <Keywords name="Folders in comment, close">endregion</Keywords>
            <Keywords name="Keywords1">let</Keywords>
            <Keywords name="Delimiters">00&quot; 01\ 02&quot; 03${ 04 05}</Keywords>
        </KeywordLists>
        <Styles>
            <WordsStyle name="DELIMITERS1" fgColor="808080" nesting="1026" />
            <WordsStyle name="DELIMITERS2" fgColor="808080" nesting="0" />
        </Styles>
    </UserLang>
</NotepadPlus>"#;

fn full_kinds(text: &str) -> Vec<(&str, UdlKind)> {
    parse_udl(FULL).unwrap()[0]
        .highlight(text, 0)
        .into_iter()
        .map(|(range, kind)| (&text[range], kind))
        .collect()
}

#[test]
fn numbers_follow_their_lists() {
    fn numbers(text: &str) -> Vec<&str> {
        full_kinds(text)
            .into_iter()
            .filter(|(_, kind)| *kind == UdlKind::Number)
            .map(|(piece, _)| piece)
            .collect()
    }
    assert_eq!(
        numbers("12 1.5 0xff 0b101 10L 1..10 0x"),
        ["12", "1.5", "0xff", "0b101", "10L", "1..10"],
        "a prefix alone is no number"
    );
    assert!(numbers("0xfg 1.2.3 L x1 0b").is_empty());
    assert!(numbers("").is_empty());
    assert_eq!(numbers("0XFF"), Vec::<&str>::new(), "case counts");
}

#[test]
fn line_comments_start_lines_when_the_language_says_so() {
    let text = "# yes\n  # indented\nlet x # no";
    let comments: Vec<&str> = full_kinds(text)
        .into_iter()
        .filter(|(_, kind)| *kind == UdlKind::LineComment)
        .map(|(piece, _)| piece)
        .collect();
    assert_eq!(comments, ["# yes", "# indented"]);
    let at_start = r#"<NotepadPlus><UserLang name="S"><Settings><Global forcePureLC="2" /></Settings>
        <KeywordLists><Keywords name="Comments">00#</Keywords></KeywordLists></UserLang></NotepadPlus>"#;
    let language = parse_udl(at_start).unwrap().remove(0);
    let pieces = language.highlight("# a\n # b", 0);
    assert_eq!(
        pieces,
        [(0..3, UdlKind::LineComment)],
        "the first column only"
    );
}

#[test]
fn delimiters_hold_what_their_nesting_allows() {
    // Delimiter 1 (a string) holds delimiter 2 (`${...}`, bit 1) and keywords 1 (bit 10).
    assert_eq!(
        full_kinds(r#""a ${b} let c""#),
        [
            ("\"a ", UdlKind::Delimiter(0)),
            ("${b}", UdlKind::Delimiter(1)),
            (" ", UdlKind::Delimiter(0)),
            ("let", UdlKind::Keyword(0)),
            (" c\"", UdlKind::Delimiter(0)),
        ]
    );
    // Delimiter 2 holds nothing: a string inside it is its text.
    assert_eq!(
        full_kinds(r#"${"x"}"#),
        [(r#"${"x"}"#, UdlKind::Delimiter(1))]
    );
    // A keyword inside a word is not one; an escaped quote does not close.
    assert_eq!(
        full_kinds(r#""xlet\"""#),
        [(r#""xlet\"""#, UdlKind::Delimiter(0))]
    );
    // Never closed: to the end.
    assert_eq!(
        full_kinds("\"${"),
        [("\"", UdlKind::Delimiter(0)), ("${", UdlKind::Delimiter(1))]
    );
}

#[test]
fn folds_open_and_close_at_their_words() {
    let language = parse_udl(FULL).unwrap().remove(0);
    let text = "begin\n  x { y\n  }\nelse\nend\n# region a\n# endregion\n{ }";
    let folds: Vec<&str> = language
        .folds(text)
        .into_iter()
        .map(|range| &text[range])
        .collect();
    assert_eq!(
        folds,
        [
            "begin\n  x { y\n  }\nelse\nend",
            "{ y\n  }",
            "region a\n# endregion",
        ],
        "a fold on one line is none"
    );
    // Never closed: to the end; a close without an open is ignored.
    assert_eq!(language.folds("}\nbegin\nx"), [2..9]);
    assert!(language.folds("").is_empty());
    // A fold word inside a string does not fold.
    assert!(language.folds("\"begin\"\nend").is_empty());
}
