use super::*;
use crate::{DARK, Theme};

/// A small theme in Notepad++'s format, dark, with lexers that agree and one that does not.
const SAMPLE: &str = r#"<?xml version="1.0" encoding="UTF-8" ?>
<NotepadPlus>
    <LexerStyles>
        <LexerType name="cpp" desc="C++" ext="">
            <WordsStyle name="DEFAULT" styleID="11" fgColor="DCDCCC" bgColor="3F3F3F" fontStyle="0" />
            <WordsStyle name="INSTRUCTION WORD" styleID="5" fgColor="DFC47D" bgColor="3F3F3F" fontStyle="1" />
            <WordsStyle name="TYPE WORD" styleID="16" fgColor="CEDF99" bgColor="3F3F3F" fontStyle="0" />
            <WordsStyle name="COMMENT" styleID="1" fgColor="7F9F7F" bgColor="3F3F3F" fontStyle="2" />
            <WordsStyle name="COMMENT LINE" styleID="2" fgColor="FF0000" bgColor="3F3F3F" fontStyle="0" />
            <WordsStyle name="COMMENT DOC" styleID="3" fgColor="7F9F7F" bgColor="3F3F3F" fontStyle="0" />
            <WordsStyle name="STRING" styleID="6" fgColor="CC9393" bgColor="3F3F3F" fontStyle="0" />
            <WordsStyle name="NUMBER" styleID="4" fgColor="8CD0D3" bgColor="3F3F3F" fontStyle="" />
            <WordsStyle name="PREPROCESSOR" styleID="9" fgColor="FFCFAF" bgColor="3F3F3F" fontStyle="0" />
        </LexerType>
        <LexerType name="java" desc="Java" ext="">
            <WordsStyle name="INSTRUCTION WORD" styleID="5" fgColor="DFC47D" bgColor="3F3F3F" fontStyle="1" />
            <WordsStyle name="COMMENT" styleID="1" fgColor="7F9F7F" bgColor="3F3F3F" fontStyle="2" />
            <WordsStyle name="STRING" styleID="6" fgColor="CC9393" bgColor="3F3F3F" fontStyle="0" />
        </LexerType>
        <LexerType name="python" desc="Python" ext="">
            <WordsStyle name="KEYWORDS" styleID="5" fgColor="FF00FF" bgColor="3F3F3F" fontStyle="5" />
            <WordsStyle name="COMMENTLINE" styleID="1" fgColor="7F9F7F" bgColor="3F3F3F" fontStyle="2" />
            <WordsStyle name="STRING" styleID="3" fgColor="CC9393" bgColor="000000" fontStyle="0" />
            <WordsStyle name="DEF NAME" styleID="9" fgColor="EFEF8F" bgColor="3F3F3F" fontStyle="0" />
        </LexerType>
        <LexerType name="javascript" desc="JavaScript (embedded)" ext="">
            <WordsStyle name="KEYWORD" styleID="47" fgColor="123456" bgColor="3F3F3F" fontStyle="0" />
        </LexerType>
        <LexerType name="javascript.js" desc="JavaScript" ext="">
            <WordsStyle name="KEYWORD" styleID="5" fgColor="DFC47D" bgColor="3F3F3F" fontStyle="1" />
            <WordsStyle name="REGEX" styleID="14" fgColor="C89191" bgColor="3F3F3F" fontStyle="0" />
        </LexerType>
        <LexerType name="searchResult" desc="Search result" ext="">
            <WordsStyle name="HIT WORD" styleID="4" fgColor="FF0000" bgColor="FFFF00" fontStyle="1" />
        </LexerType>
    </LexerStyles>
    <GlobalStyles>
        <WidgetStyle name="Global override" styleID="0" fgColor="FFFF80" bgColor="FF8000" fontName="Courier New" fontStyle="0" fontSize="10" />
        <WidgetStyle name="Default Style" styleID="32" fgColor="DCDCCC" bgColor="3F3F3F" fontName="Consolas" fontStyle="0" fontSize="10" />
        <WidgetStyle name="Indent guideline style" styleID="37" fgColor="5A5A5A" bgColor="3F3F3F" />
        <WidgetStyle name="Brace highlight style" styleID="34" fgColor="FFFF00" bgColor="3F3F3F" fontStyle="1" />
        <WidgetStyle name="Bad brace colour" styleID="35" fgColor="FF0000" bgColor="3F3F3F" />
        <WidgetStyle name="Current line background colour" styleID="0" bgColor="434343" />
        <WidgetStyle name="Selected text colour" styleID="0" bgColor="585858" />
        <WidgetStyle name="Caret colour" styleID="2069" fgColor="BFBFBF" />
        <WidgetStyle name="Edge colour" styleID="0" fgColor="808080" />
        <WidgetStyle name="Line number margin" styleID="33" fgColor="9FAFAF" bgColor="262626" />
        <WidgetStyle name="Fold" styleID="0" fgColor="808080" bgColor="333333" />
        <WidgetStyle name="White space symbol" styleID="0" fgColor="5A5A5A" />
        <WidgetStyle name="Smart Highlighting" styleID="29" bgColor="00FF00" />
        <WidgetStyle name="Find Mark Style" styleID="31" bgColor="FF0000" />
        <WidgetStyle name="Mark Style 1" styleID="25" bgColor="00FFFF" />
        <WidgetStyle name="Mark Style 2" styleID="24" bgColor="FF8000" />
        <WidgetStyle name="Mark Style 3" styleID="23" bgColor="FFFF00" />
        <WidgetStyle name="Mark Style 4" styleID="22" bgColor="8000FF" />
        <WidgetStyle name="Mark Style 5" styleID="21" bgColor="008000" />
        <WidgetStyle name="Incremental highlight all" styleID="28" bgColor="0080FF" />
        <WidgetStyle name="EOL custom color" styleID="0" fgColor="DADADA" />
        <WidgetStyle name="Change History modified" styleID="0" fgColor="FF8000" bgColor="FF8001" />
        <WidgetStyle name="Change History saved" styleID="0" fgColor="00A000" bgColor="00A001" />
    </GlobalStyles>
</NotepadPlus>
"#;

fn sample() -> Theme {
    Theme::from_notepad_xml("Zenburnish", SAMPLE).unwrap()
}

#[test]
fn global_styles_become_editor_colors() {
    let theme = sample();
    assert_eq!(theme.name, "Zenburnish");
    assert!(theme.dark);
    let editor = theme.editor;
    assert_eq!(editor.text, Color::rgb(0xdcdccc));
    assert_eq!(editor.background, Color::rgb(0x3f3f3f));
    assert_eq!(editor.selection, Color::rgb(0x585858));
    assert_eq!(editor.caret, Color::rgb(0xbfbfbf));
    assert_eq!(editor.current_line, Color::rgb(0x434343));
    assert_eq!(editor.gutter_text, Color::rgb(0x9fafaf));
    assert_eq!(editor.gutter_background, Color::rgb(0x262626));
    assert_eq!(editor.fold_mark, Color::rgb(0x808080));
    assert_eq!(editor.whitespace, Color::rgb(0x5a5a5a));
    assert_eq!(editor.indent_guide, Color::rgb(0x5a5a5a));
    assert_eq!(editor.edge, Color::rgb(0x808080));
    assert_eq!(editor.eol_text, Color::rgb(0xdadada));
    assert_eq!(editor.brace_match, Color::rgb(0xffff00));
    assert_eq!(editor.brace_bad, Color::rgb(0xff0000));
    assert_eq!(editor.change_modified, Color::rgb(0xff8001));
    assert_eq!(editor.change_saved, Color::rgb(0x00a001));
    // Highlights stay translucent, with Birchpad's opacity.
    assert_eq!(editor.smart_highlight, Color(0x00ff0064));
    assert_eq!(editor.find_mark, Color(0xff000055));
    assert_eq!(editor.incremental_highlight, Color(0x0080ff55));
    assert_eq!(editor.mark_3, Color(0xffff0099));
    // What Notepad++ themes do not have comes from the dark theme.
    let dark = Theme::builtin(DARK).unwrap();
    assert_eq!(editor.bookmark, dark.editor.bookmark);
    assert_eq!(editor.gutter_border, dark.editor.gutter_border);
    assert_eq!(theme.ui, dark.ui);
}

#[test]
fn the_style_most_lexers_use_becomes_the_theme_style() {
    let theme = sample();
    let keyword = theme.syntax["keyword"];
    assert_eq!(keyword.color, Some(Color::rgb(0xdfc47d)));
    assert!(keyword.bold && !keyword.italic);
    let comment = theme.syntax["comment"];
    assert_eq!(comment.color, Some(Color::rgb(0x7f9f7f)));
    assert!(comment.italic);
    assert_eq!(theme.syntax["string"], Style::color(Color::rgb(0xcc9393)));
    assert_eq!(theme.syntax["type"], Style::color(Color::rgb(0xcedf99)));
    assert_eq!(
        theme.syntax["keyword.directive"],
        Style::color(Color::rgb(0xffcfaf))
    );
    assert_eq!(
        theme.syntax["string.regexp"],
        Style::color(Color::rgb(0xc89191))
    );
    // An empty fontStyle is a plain style.
    assert_eq!(theme.syntax["number"], Style::color(Color::rgb(0x8cd0d3)));
    // The first comment style of a lexer counts, not COMMENT LINE.
    assert_ne!(comment.color, Some(Color::rgb(0xff0000)));
    // Styles of the dark theme that Notepad++'s lexers do not name are kept.
    assert_eq!(
        theme.syntax["module"],
        Theme::builtin(DARK).unwrap().syntax["module"]
    );
}

#[test]
fn a_lexer_that_differs_gets_styles_of_its_own() {
    let theme = sample();
    let python = &theme.languages["python"];
    let keyword = python["keyword"];
    assert_eq!(keyword.color, Some(Color::rgb(0xff00ff)));
    assert!(keyword.bold && keyword.underline && !keyword.italic);
    // A background other than the theme's is kept.
    assert_eq!(python["string"].background, Some(Color::rgb(0x000000)));
    // What only it styles is the theme's style, and what it shares with the others is not
    // repeated.
    assert_eq!(theme.syntax["function"], Style::color(Color::rgb(0xefef8f)));
    assert!(!python.contains_key("function"));
    assert!(!python.contains_key("comment"));
    assert!(!theme.languages.contains_key("java"));
    assert_eq!(
        theme.syntax_style(Some("python"), "keyword").unwrap().color,
        Some(Color::rgb(0xff00ff))
    );
    assert_eq!(
        theme.syntax_style(Some("java"), "keyword").unwrap().color,
        Some(Color::rgb(0xdfc47d))
    );
}

#[test]
fn javascript_and_panels() {
    let theme = sample();
    // `javascript.js` is JavaScript; `javascript` is then only the HTML-embedded one, skipped.
    assert!(!theme.languages.contains_key("javascript.js"));
    assert_ne!(
        theme
            .syntax_style(Some("javascript"), "keyword")
            .unwrap()
            .color,
        Some(Color::rgb(0x123456))
    );
    // Notepad++'s search results are not a language.
    assert!(!theme.languages.contains_key("searchresult"));
    // An older theme with `javascript` alone keeps it.
    let old = SAMPLE.replace(
        r#"<LexerType name="javascript.js" desc="JavaScript" ext="">"#,
        r#"<LexerType name="other" desc="" ext="">"#,
    );
    let theme = Theme::from_notepad_xml("Old", &old).unwrap();
    assert_eq!(
        theme
            .syntax_style(Some("javascript"), "keyword")
            .unwrap()
            .color,
        Some(Color::rgb(0x123456))
    );
}

#[test]
fn a_light_theme_is_light() {
    let xml = r#"<NotepadPlus><GlobalStyles>
        <WidgetStyle name="Default Style" fgColor="000000" bgColor="FFFFFF" />
    </GlobalStyles></NotepadPlus>"#;
    let theme = Theme::from_notepad_xml("Light", xml).unwrap();
    assert!(!theme.dark);
    assert_eq!(theme.syntax, Theme::default_theme().syntax);
    assert_eq!(theme.editor.text, Color::rgb(0));
}

#[test]
fn sections_alone_and_unusable_values() {
    // Lexers without global styles: the colors are the default theme's.
    let xml = r#"<NotepadPlus><LexerStyles><LexerType name="cpp">
        <WordsStyle name="NUMBER" fgColor="FF0000" />
        <WordsStyle name="STRING" fgColor="12345" fontStyle="bold" />
        <WordsStyle name="OPERATOR" fgColor="GGGGGG" />
        <WordsStyle fgColor="00FF00" />
        <WordsStyle name="NOTHING KNOWN" fgColor="00FF00" />
    </LexerType><LexerType desc="nameless"><WordsStyle name="NUMBER" fgColor="0000FF" /></LexerType>
    </LexerStyles></NotepadPlus>"#;
    let theme = Theme::from_notepad_xml("Bare", xml).unwrap();
    assert!(!theme.dark);
    assert_eq!(theme.editor, Theme::default_theme().editor);
    assert_eq!(theme.syntax["number"], Style::color(Color::rgb(0xff0000)));
    // A color that does not read leaves the style without a color.
    assert_eq!(theme.syntax["string"], Style::default());
    assert_eq!(theme.syntax["operator"], Style::default());
    // Empty sections are fine too.
    let empty =
        Theme::from_notepad_xml("Empty", "<NotepadPlus><GlobalStyles/></NotepadPlus>").unwrap();
    assert_eq!(empty.editor, Theme::default_theme().editor);
    assert_eq!(empty.syntax, Theme::default_theme().syntax);
}

#[test]
fn refuses_what_is_not_a_notepad_plus_plus_theme() {
    for xml in [
        "",
        "not xml",
        "<NotepadPlus>",
        "<Theme><GlobalStyles/></Theme>",
        "<NotepadPlus/>",
        "<NotepadPlus><Session/></NotepadPlus>",
        "<NotepadPlus><!DOCTYPE x></NotepadPlus>",
    ] {
        let error = Theme::from_notepad_xml("Bad", xml).expect_err(xml);
        assert!(matches!(error, ThemeError::Xml(_)), "{xml}");
    }
}

#[test]
fn notepad_colors() {
    assert_eq!(color(Some("FF8000")), Some(Color::rgb(0xff8000)));
    assert_eq!(color(Some(" ff8000 ")), Some(Color::rgb(0xff8000)));
    for value in [
        None,
        Some(""),
        Some("FF800"),
        Some("FF80000"),
        Some("#FF800"),
        Some("GG8000"),
        Some("+F8000"),
    ] {
        assert_eq!(color(value), None, "{value:?}");
    }
}

#[test]
fn style_names_become_highlights() {
    for (lexer, name, expected) in [
        ("cpp", "INSTRUCTION WORD", Some("keyword")),
        ("cpp", "TYPE WORD", Some("type")),
        ("cpp", "COMMENT LINE DOC", Some("comment.documentation")),
        ("cpp", "comment line", Some("comment")),
        ("cpp", "PREPROCESSOR", Some("keyword.directive")),
        ("cpp", "CHARACTER", Some("character")),
        ("cpp", "VERBATIM", Some("string")),
        ("cpp", "REGEX", Some("string.regexp")),
        ("cpp", "ESCAPE SEQUENCE", Some("string.escape")),
        ("cpp", "DEFAULT", None),
        ("cpp", "IDENTIFIER", None),
        ("cpp", "", None),
        ("rust", "LIFETIME", Some("label")),
        ("rust", "MACRO", Some("function.macro")),
        ("python", "DEF NAME", Some("function")),
        ("python", "CLASS NAME", Some("type")),
        ("python", "TRIPLE DOUBLE", Some("string")),
        ("python", "DECORATOR", Some("attribute")),
        ("json", "PROPERTYNAME", Some("string.special.key")),
        ("json", "KEYWORD", Some("constant.builtin")),
        ("yaml", "IDENTIFIER", Some("string.special.key")),
        ("yaml", "DEFAULT", None),
        ("html", "TAG", Some("tag")),
        ("html", "TAGEND", Some("tag")),
        ("html", "ATTRIBUTE", Some("tag.attribute")),
        ("html", "DOUBLESTRING", Some("string")),
        ("html", "ENTITY", Some("string.escape")),
        ("css", "CSS1 PROPERTIES", Some("property")),
        ("css", "CLASS", Some("attribute")),
        ("diff", "ADDED", Some("diff.plus")),
        ("diff", "DELETED", Some("diff.minus")),
        ("markdown", "HEADER 1", Some("markup.heading")),
        ("markdown", "STRONG", Some("markup.strong")),
        ("markdown", "UNORDERED LIST", Some("markup.list")),
        ("ini", "SECTION", Some("type")),
        ("batch", "LABEL", Some("label")),
        ("sql", "KEYWORD USER1", Some("keyword")),
        ("lua", "FUNC1", None),
    ] {
        assert_eq!(highlight_of(lexer, name), expected, "{lexer} {name}");
    }
}
