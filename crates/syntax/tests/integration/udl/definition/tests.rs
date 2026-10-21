use birchpad_syntax::*;
use birchpad_syntax::{UdlKind, parse_udl};

const FILE: &str = r#"<NotepadPlus>
    <UserLang name="Mini" ext="mini mn" udlVersion="2.1" darkModeTheme="no">
        <Settings>
            <Global caseIgnored="yes" allowFoldOfComments="yes" forcePureLC="1" />
            <Prefix Keywords1="no" Keywords2="yes" />
        </Settings>
        <KeywordLists>
            <Keywords name="Comments">00// 00# 01 02 03/* 04*/</Keywords>
            <Keywords name="Keywords1">if &quot;end if&quot;</Keywords>
            <Keywords name="Delimiters">00&quot; 01\ 02&quot; 03( 04 05)</Keywords>
        </KeywordLists>
        <Styles>
            <WordsStyle name="KEYWORDS1" fgColor="0000FF" fontStyle="1" />
        </Styles>
    </UserLang>
</NotepadPlus>"#;

#[test]
fn reads_what_the_dialog_edits() {
    let mini = UdlDefinition::parse_all(FILE).unwrap().remove(0);
    assert_eq!(mini.name, "Mini");
    assert_eq!(mini.extensions, "mini mn");
    assert!(mini.ignore_case);
    assert_eq!(mini.line_comment_at, LineCommentAt::LineStartOrIndent);
    assert_eq!(mini.prefix[..2], [false, true]);
    assert_eq!(mini.list("Keywords1"), "if \"end if\"");
    assert_eq!(mini.list("Keywords2"), "");
    assert_eq!(
        mini.comments(),
        ("// #".to_owned(), "/*".to_owned(), "*/".to_owned())
    );
    assert_eq!(
        mini.delimiter(0),
        ("\"".to_owned(), "\\".to_owned(), "\"".to_owned())
    );
    assert_eq!(
        mini.delimiter(1),
        ("(".to_owned(), String::new(), ")".to_owned())
    );
    assert_eq!(
        mini.delimiter(7),
        (String::new(), String::new(), String::new())
    );
}

#[test]
fn writes_what_reads_back_the_same() {
    let mini = UdlDefinition::parse_all(FILE).unwrap().remove(0);
    let written = udl_file(std::slice::from_ref(&mini));
    assert_eq!(
        UdlDefinition::parse_all(&written).unwrap(),
        std::slice::from_ref(&mini)
    );
    // The language written colors as the one read.
    let read = parse_udl(FILE).unwrap().remove(0);
    let again = parse_udl(&written).unwrap().remove(0);
    let text = "if x // c\n\"s\" (p)";
    assert_eq!(read.highlight(text, 0), again.highlight(text, 0));
    assert_eq!(
        again.style(UdlKind::Keyword(0)),
        read.style(UdlKind::Keyword(0))
    );
    assert!(written.contains("darkModeTheme=\"no\""), "kept as read");
    assert!(written.contains("allowFoldOfComments=\"yes\""));
}

#[test]
fn edits_reach_the_file() {
    let mut language = UdlDefinition::new("New <One>");
    language.extensions = "new".into();
    language.set_list("Keywords3", "  alpha beta  ");
    language.set_comments("--", "{-", "-}");
    language.set_delimiter(2, "'", "", "'");
    language.line_comment_at = LineCommentAt::LineStart;
    let written = udl_file(std::slice::from_ref(&language));
    let read = UdlDefinition::parse_all(&written).unwrap().remove(0);
    assert_eq!(read.name, "New <One>", "escaped and read back");
    assert_eq!(read.list("Keywords3"), "alpha beta");
    assert_eq!(read.comments(), ("--".into(), "{-".into(), "-}".into()));
    assert_eq!(read.delimiter(2), ("'".into(), String::new(), "'".into()));
    assert_eq!(read.line_comment_at, LineCommentAt::LineStart);
    // Line comments start in the first column only, as written.
    let colors = parse_udl(&written).unwrap().remove(0);
    assert_eq!(
        colors.highlight("alpha\n-- c", 0),
        [(0..5, UdlKind::Keyword(2)), (6..10, UdlKind::LineComment)]
    );
    assert_eq!(
        colors.highlight("alpha -- c", 0),
        [(0..5, UdlKind::Keyword(2))]
    );
}

#[test]
fn emptied_lists_are_left_out() {
    let mut language = UdlDefinition::parse_all(FILE).unwrap().remove(0);
    language.set_list("Keywords1", "   ");
    language.set_comments("", "", "");
    for index in 0..DELIMITERS {
        language.set_delimiter(index, "", "", "");
    }
    let written = language.to_xml();
    assert!(!written.contains(r#"name="Keywords1""#));
    assert!(!written.contains(r#"name="Comments""#));
    assert!(!written.contains(r#"name="Delimiters""#));
    // Out of range: nothing changes.
    let before = language.clone();
    language.set_delimiter(DELIMITERS, "x", "", "x");
    assert_eq!(language, before);
    assert!(UdlDefinition::parse_all("<NotepadPlus/>").is_err());
    assert!(UdlDefinition::parse_all("<broken").is_err());
}
