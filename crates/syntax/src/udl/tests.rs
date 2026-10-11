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
