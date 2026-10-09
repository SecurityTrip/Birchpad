use super::*;

fn read(text: &str) -> Theme {
    Theme::from_toml("Mine", text).unwrap_or_else(|error| panic!("{error}"))
}

#[test]
fn builtins_are_complete_and_round_trip() {
    let names: Vec<_> = Theme::builtins()
        .iter()
        .map(|theme| theme.name.as_str())
        .collect();
    assert_eq!(names, [DEFAULT, DARK]);
    assert!(!Theme::default_theme().dark);
    assert!(Theme::builtin(DARK).unwrap().dark);
    for theme in Theme::builtins() {
        let again = Theme::from_toml(&theme.name, &theme.to_toml()).unwrap();
        assert_eq!(&again, theme, "{}", theme.to_toml());
        assert_eq!(theme.dark, theme.editor.background.is_dark());
    }
}

#[test]
fn the_default_theme_keeps_notepad_plus_plus_colors() {
    let theme = Theme::default_theme();
    assert_eq!(theme.editor.background, Color::rgb(0xffffff));
    assert_eq!(theme.editor.current_line, Color::rgb(0xe8e8ff));
    assert_eq!(theme.editor.whitespace, Color::rgb(0xffb56a));
    assert_eq!(theme.editor.change_modified, Color::rgb(0xff8000));
    assert_eq!(theme.editor.selection, Color(0x3390ff40));
    assert_eq!(
        theme.editor.mark_styles(),
        [
            Color(0x00ffff66),
            Color(0xff800066),
            Color(0xffff0099),
            Color(0x8000ff55),
            Color(0x00800066)
        ]
    );
    let keyword = theme.syntax_style(None, "keyword").unwrap();
    assert_eq!(keyword.color, Some(Color::rgb(0x0000ff)));
    assert!(keyword.bold);
    assert_eq!(
        theme.syntax_style(None, "comment").unwrap(),
        Style::color(Color::rgb(0x008000))
    );
}

#[test]
fn builtins_are_found_ignoring_case() {
    assert_eq!(Theme::builtin("dark").unwrap().name, DARK);
    assert_eq!(Theme::builtin("DEFAULT").unwrap().name, DEFAULT);
    assert!(Theme::builtin("").is_none());
    assert!(Theme::builtin("Darker").is_none());
}

#[test]
fn an_empty_file_is_the_default_theme_under_its_own_name() {
    let theme = read("");
    assert_eq!(theme.name, "Mine");
    assert_eq!(
        Theme {
            name: DEFAULT.into(),
            ..theme
        },
        *Theme::default_theme()
    );
}

#[test]
fn colors_not_set_come_from_the_theme_of_the_same_brightness() {
    let light = read("[editor]\nbackground = '#fdf6e3'\ncaret = '#ff0000'\n");
    assert!(!light.dark);
    assert_eq!(light.editor.background, Color::rgb(0xfdf6e3));
    assert_eq!(light.editor.caret, Color::rgb(0xff0000));
    assert_eq!(light.editor.text, Theme::default_theme().editor.text);
    assert_eq!(light.ui, Theme::default_theme().ui);

    let dark_theme = Theme::builtin(DARK).unwrap();
    let dark = read("[editor]\nbackground = '#002b36'\n");
    assert!(dark.dark);
    assert_eq!(dark.editor.text, dark_theme.editor.text);
    assert_eq!(dark.ui, dark_theme.ui);
    assert_eq!(dark.syntax, dark_theme.syntax);

    // `dark` decides when it is set.
    let forced = read("dark = true\n[editor]\nbackground = '#ffffff'\n");
    assert!(forced.dark);
    assert_eq!(forced.ui, dark_theme.ui);
    assert!(!read("dark = false\n[editor]\nbackground = '#000000'\n").dark);
    assert!(read("dark = true").dark);
}

#[test]
fn ui_colors_can_be_set_too() {
    let theme = read("[ui]\naccent = '#ff00ff'\nerror-background = '#11223344'\n");
    assert_eq!(theme.ui.accent, Color::rgb(0xff00ff));
    assert_eq!(theme.ui.error_background, Color(0x11223344));
    assert_eq!(theme.ui.border, Theme::default_theme().ui.border);
}

#[test]
fn a_style_replaces_the_more_specific_ones_it_covers() {
    let theme = read("[syntax]\nstring = '#123456'\n");
    let string = Some(Style::color(Color::rgb(0x123456)));
    assert_eq!(theme.syntax_style(None, "string"), string);
    // The default theme's `string.escape` and `string.regexp` are gone with it,
    assert_eq!(theme.syntax_style(None, "string.escape"), string);
    assert_eq!(theme.syntax_style(None, "string.regexp"), string);
    // but not `stringy` names that only start the same way, nor other styles.
    assert!(!theme.syntax.contains_key("string.escape"));
    assert_eq!(
        theme.syntax_style(None, "keyword"),
        Theme::default_theme().syntax_style(None, "keyword")
    );
    let theme = read("[syntax]\nstr = '#123456'\n");
    assert!(theme.syntax.contains_key("string.escape"));
}

#[test]
fn dotted_names_read_quoted_or_as_tables() {
    let expected = Some(Style::color(Color::rgb(0xabcdef)));
    for text in [
        "[syntax]\n\"function.method\" = '#abcdef'\n",
        "[syntax]\nfunction.method = '#abcdef'\n",
        "[syntax.function]\nmethod = '#abcdef'\n",
        "[syntax.function.method]\ncolor = '#abcdef'\n",
    ] {
        let theme = read(text);
        assert_eq!(
            theme.syntax_style(None, "function.method"),
            expected,
            "{text}"
        );
        // `function` itself is untouched when only `function.method` is set.
        assert_eq!(
            theme.syntax_style(None, "function"),
            Theme::default_theme().syntax_style(None, "function"),
            "{text}"
        );
    }
    // A table with style keys and longer names styles both.
    let theme = read("[syntax.function]\ncolor = '#111111'\nbold = true\nmethod = '#222222'\n");
    let function = theme.syntax_style(None, "function").unwrap();
    assert_eq!(function.color, Some(Color::rgb(0x111111)));
    assert!(function.bold);
    assert_eq!(
        theme.syntax_style(None, "function.method"),
        Some(Style::color(Color::rgb(0x222222)))
    );
}

#[test]
fn language_styles_come_first_at_each_step() {
    let theme = read(
        "[syntax]\nstring = '#000001'\n\"string.escape\" = '#000002'\n\
         [language.Python]\nstring = '#000003'\n",
    );
    let color = |language, name| {
        theme
            .syntax_style(language, name)
            .and_then(|style| style.color)
    };
    assert_eq!(color(Some("python"), "string"), Some(Color::rgb(3)));
    // The language's `string` does not hide the theme's more specific `string.escape`,
    assert_eq!(color(Some("python"), "string.escape"), Some(Color::rgb(2)));
    // but stands for the names under it that nothing styles more specifically.
    assert_eq!(color(Some("python"), "string.special"), Some(Color::rgb(3)));
    assert_eq!(color(Some("rust"), "string.special"), Some(Color::rgb(1)));
    assert_eq!(color(None, "string"), Some(Color::rgb(1)));
    // Language ids are lowercase.
    assert!(theme.languages.contains_key("python"));
    assert!(!theme.languages.contains_key("Python"));
}

#[test]
fn names_without_a_style() {
    let theme = Theme::default_theme();
    assert_eq!(theme.syntax_style(None, "variable"), None);
    assert_eq!(theme.syntax_style(None, ""), None);
    assert_eq!(theme.syntax_style(None, "."), None);
    assert_eq!(theme.syntax_style(Some(""), "nothing.at.all"), None);
    // A trailing dot falls back to the name before it.
    assert_eq!(
        theme.syntax_style(None, "keyword."),
        theme.syntax_style(None, "keyword")
    );
}

#[test]
fn keys_of_newer_versions_are_ignored() {
    let theme = read(
        "fonts = 'x'\n[editor]\nglow = '#ffffff'\ncaret = '#010203'\n[ui]\nshadow = 1\n\
         [syntax]\nkeyword = { color = '#010203', blink = true }\n",
    );
    assert_eq!(theme.editor.caret, Color::rgb(0x010203));
    assert_eq!(
        theme.syntax_style(None, "keyword"),
        Some(Style::color(Color::rgb(0x010203)))
    );
}

#[test]
fn refuses_files_that_are_not_themes() {
    for text in [
        "[editor",
        "dark = 'yes'",
        "editor = 3",
        "[editor]\ncaret = 'red'\n",
        "[editor]\ncaret = '#12345'\n",
        "[ui]\naccent = 255\n",
        "syntax = '#ffffff'",
        "[syntax]\nkeyword = 'blue'\n",
        "[syntax]\nkeyword = { bold = 'yes' }\n",
        "[syntax.keyword]\ncolor = 1\n",
        "language = 1",
        "[language]\npython = '#ffffff'\n",
        "[language.python]\nkeyword = 'blue'\n",
    ] {
        let error = Theme::from_toml("Mine", text).expect_err(text);
        assert!(matches!(error, ThemeError::Toml(_)), "{text}");
        assert!(!error.to_string().is_empty());
    }
}

#[test]
fn every_written_theme_reads_back_the_same() {
    let theme = read(
        "[editor]\nbackground = '#101010'\n[syntax]\nkeyword = { color = '#ff0000', italic = true, underline = true, background = '#00000080' }\n\
         [language.rust.string]\ncolor = '#00ff00'\nescape = '#0000ff'\n",
    );
    let text = theme.to_toml();
    assert_eq!(read(&text), theme, "{text}");
    assert!(text.contains("[language.rust]"), "{text}");
}

#[test]
fn a_style_without_colors_stays_one() {
    // Found by fuzzing: an empty style was written as `type = {}` and read back as the base
    // theme's style.
    let theme = read("[syntax]\ntype = {}\n");
    assert_eq!(theme.syntax_style(None, "type"), Some(Style::default()));
    assert_eq!(read(&theme.to_toml()), theme);
    let theme = read("[syntax.type]\n");
    assert_eq!(theme.syntax_style(None, "type"), Some(Style::default()));
}

#[test]
fn colors_by_key() {
    let mut editor = Theme::default_theme().editor;
    assert_eq!(EditorColors::KEYS.len(), 30);
    assert_eq!(EditorColors::KEYS[0], "text");
    assert!(EditorColors::KEYS.contains(&"current-line"));
    assert!(UiColors::KEYS.contains(&"error-background"));
    assert_eq!(editor.get("current-line"), Some(editor.current_line));
    assert!(editor.set("mark-5", Color(0x01020304)));
    assert_eq!(editor.mark_5, Color(0x01020304));
    // Keys as files write them, not as Rust does.
    assert_eq!(editor.get("current_line"), None);
    assert!(!editor.set("", Color::rgb(0)));
    assert!(!editor.set("nothing", Color::rgb(0)));
    let mut ui = Theme::default_theme().ui;
    assert!(ui.set("accent", Color::rgb(1)));
    assert_eq!(ui.get("accent"), Some(Color::rgb(1)));
    // Every key reads and writes the color the file format names it by.
    let file = toml::to_string(&PartialEditorColors::from(&editor)).unwrap();
    for key in EditorColors::KEYS {
        assert!(file.contains(&format!("{key} = ")), "{key}");
        assert!(editor.get(key).is_some());
    }
}
