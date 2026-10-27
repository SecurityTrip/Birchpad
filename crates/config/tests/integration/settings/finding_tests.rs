use birchpad_config::*;

#[test]
fn the_palette_lists_eight_recent_commands_by_default() {
    let palette = PaletteSettings::default();
    assert_eq!(palette.recent_commands, 8);
    assert_eq!(Settings::default().palette, palette);
    assert_eq!(palette.recent_limit(), 8);
}

#[test]
fn the_palette_s_recent_commands_stay_within_their_limit() {
    for (set, limit) in [(0, 0), (1, 1), (50, 50), (51, 50), (255, 50)] {
        let palette = PaletteSettings {
            recent_commands: set,
        };
        assert_eq!(palette.recent_limit(), limit, "{set}");
    }
    let settings: Settings = toml::from_str("[palette]\nrecent-commands = 3\n").unwrap();
    assert_eq!(settings.palette.recent_commands, 3);
    check_setting("palette.recent-commands", &toml::Value::Integer(0)).unwrap();
    assert!(check_setting("palette.recent-commands", &toml::Value::Integer(-1)).is_err());
    assert!(check_setting("palette.recent-commands", &toml::Value::Integer(256)).is_err());
    assert!(check_setting("palette.recent-commands", &toml::Value::Boolean(true)).is_err());
}

#[test]
fn go_to_file_leaves_out_version_control_and_build_folders_by_default() {
    let finder = FinderSettings::default();
    assert_eq!(finder.filters, r"!+\.git !+\node_modules !+\target");
    assert!(!finder.hidden);
    assert_eq!(finder.file_limit(), 50_000);
    assert_eq!(Settings::default().finder, finder);
}

#[test]
fn go_to_file_s_limit_stays_within_its_bounds() {
    for (set, limit) in [
        (0, 1000),
        (999, 1000),
        (1000, 1000),
        (1_000_000, 1_000_000),
        (1_000_001, 1_000_000),
        (u32::MAX, 1_000_000),
    ] {
        let finder = FinderSettings {
            max_files: set,
            ..FinderSettings::default()
        };
        assert_eq!(finder.file_limit(), limit, "{set}");
    }
    let settings: Settings =
        toml::from_str("[finder]\nfilters = \"*.rs\"\nhidden = true\n").unwrap();
    assert_eq!(settings.finder.filters, "*.rs");
    assert!(settings.finder.hidden);
    assert_eq!(settings.finder.max_files, 50_000);
    check_setting("finder.hidden", &toml::Value::Boolean(true)).unwrap();
    check_setting("finder.filters", &toml::Value::String(String::new())).unwrap();
    assert!(check_setting("finder.max-files", &toml::Value::Integer(-1)).is_err());
    assert!(check_setting("finder.hidden", &toml::Value::Integer(1)).is_err());
}

#[test]
fn sticky_scroll_is_off_and_keeps_five_lines_by_default() {
    let editor = EditorSettings::default();
    assert!(!editor.sticky_scroll);
    assert_eq!(editor.sticky_lines(), 5);
    for (set, lines) in [(0, 1), (1, 1), (10, 10), (11, 10), (255, 10)] {
        let editor = EditorSettings {
            sticky_scroll_lines: set,
            ..EditorSettings::default()
        };
        assert_eq!(editor.sticky_lines(), lines, "{set}");
    }
    let settings: Settings =
        toml::from_str("[editor]\nsticky-scroll = true\nsticky-scroll-lines = 3\n").unwrap();
    assert!(settings.editor.sticky_scroll);
    assert_eq!(settings.editor.sticky_lines(), 3);
    assert!(check_setting("editor.sticky-scroll", &toml::Value::Integer(1)).is_err());
    assert!(check_setting("editor.sticky-scroll-lines", &toml::Value::Integer(-1)).is_err());
}

#[test]
fn window_and_text_options_default_to_what_birchpad_did() {
    let settings = Settings::default();
    let appearance = &settings.appearance;
    assert!(appearance.menu_bar && appearance.status_bar);
    assert_eq!(appearance.dark_tone, DarkTone::Black);
    let tabs = &settings.tabs;
    assert_eq!(tabs.max_label_length, 0);
    assert!(!tabs.alternate_icons);
    assert!(tabs.inactive_color && tabs.active_bar && tabs.pinning);
    assert!(tabs.only_pinned_button && tabs.inactive_buttons);
    let editor = &settings.editor;
    assert_eq!(editor.caret_width, CaretWidth::Two);
    assert_eq!(editor.caret_blink_ms, 530);
    assert_eq!(editor.wrap_indent, WrapIndent::Default);
    assert!(!editor.virtual_space && !editor.scroll_beyond_last_line);
    assert!(!editor.keep_selection_on_right_click && !editor.copy_line_without_selection);
    assert!(!editor.selection_foreground && editor.drag_text && editor.multi_editing);
    assert!(!editor.column_to_multi_editing && editor.prevent_control_typing);
    assert_eq!(editor.eol_style, EolStyle::Box);
    assert_eq!(editor.fold_style, FoldStyle::BoxTree);
    assert_eq!(editor.border_width, 1);
    assert_eq!(editor.line_number_width, LineNumberWidth::Dynamic);
    assert_eq!((editor.padding_left, editor.padding_right), (0, 0));
    assert_eq!(editor.padding_distraction_free, 4);
}

#[test]
fn window_and_text_options_read_their_names() {
    let settings: Settings = toml::from_str(
        "[editor]\ncaret-width = \"block-after\"\nwrap-indent = \"indent\"\nfold-style = \"circle-tree\"\neol-style = \"plain-text\"\nline-number-width = \"constant\"\n[appearance]\ndark-tone = \"olive\"\n",
    )
    .unwrap();
    assert_eq!(settings.editor.caret_width, CaretWidth::BlockAfter);
    assert_eq!(settings.editor.wrap_indent, WrapIndent::Indent);
    assert_eq!(settings.editor.fold_style, FoldStyle::CircleTree);
    assert_eq!(settings.editor.eol_style, EolStyle::PlainText);
    assert_eq!(settings.editor.line_number_width, LineNumberWidth::Constant);
    assert_eq!(settings.appearance.dark_tone, DarkTone::Olive);
    check_setting("editor.caret-width", &toml::Value::String("0".into())).unwrap();
    assert!(check_setting("editor.caret-width", &toml::Value::String("4".into())).is_err());
    assert!(check_setting("editor.fold-style", &toml::Value::String("tree".into())).is_err());
    assert!(check_setting("appearance.dark-tone", &toml::Value::String("pink".into())).is_err());
}

#[test]
fn dark_tones_have_hues_but_black() {
    assert_eq!(DarkTone::Black.hue(), None);
    for tone in [
        DarkTone::Red,
        DarkTone::Green,
        DarkTone::Blue,
        DarkTone::Purple,
        DarkTone::Cyan,
        DarkTone::Olive,
    ] {
        let hue = tone.hue().unwrap();
        assert!((0. ..360.).contains(&hue), "{tone:?}");
    }
}

#[test]
fn tab_labels_are_cut_to_their_length() {
    let tabs = |max| TabSettings {
        max_label_length: max,
        ..TabSettings::default()
    };
    assert_eq!(tabs(0).label("a long name.txt"), "a long name.txt");
    assert_eq!(tabs(6).label("a long name.txt"), "a lon…");
    assert_eq!(tabs(15).label("a long name.txt"), "a long name.txt");
    assert_eq!(tabs(14).label("a long name.txt"), "a long name.t…");
    assert_eq!(tabs(1).label("ab"), "a…");
    assert_eq!(tabs(3).label("файлы"), "фа…");
    assert_eq!(tabs(3).label(""), "");
}

#[test]
fn files_search_and_typing_options_default_to_what_birchpad_did() {
    let settings = Settings::default();
    let new = &settings.new_document;
    assert!(!new.apply_to_ansi && !new.at_startup && !new.first_line_name);
    let files = &settings.files;
    assert!(!files.recent_submenu && !files.recent_file_name_only);
    assert_eq!(files.recent_max_length, 0);
    assert!(files.recent_check_existence && !files.drop_folder_opens_files);
    assert!(!files.large_file_no_wrap && !files.large_file_completion);
    assert!(!files.large_file_smart_highlighting);
    assert!(files.large_file_brace_match && files.large_file_links);
    let searching = &settings.searching;
    assert_eq!(
        (searching.in_selection_from, searching.fill_from_selection),
        (1024, 1024)
    );
    assert!(searching.fill_from_word && searching.stay_open);
    assert!(searching.confirm_replace_all_documents && searching.one_entry_per_line);
    assert!(!searching.replace_stays && !searching.ignore_unsaved_changes);
    assert!(settings.misc.detect_encoding && !settings.misc.follow_symlinks);
    assert!(settings.language_menu.compact);
    assert!(settings.highlighting.matching_tags);
    let completion = &settings.completion;
    assert!(completion.accept_with_tab && completion.accept_with_enter && !completion.brief);
    assert!(!settings.editor.backspace_unindents);
    assert!(settings.delimiters.word_chars.is_empty());
}

#[test]
fn hidden_languages_are_ids_separated_by_spaces() {
    let menu = LanguageMenuSettings {
        hidden: "cobol, Fortran;ada".into(),
        ..LanguageMenuSettings::default()
    };
    for id in ["cobol", "fortran", "ada", "COBOL"] {
        assert!(!menu.shows(id), "{id}");
    }
    assert!(menu.shows("rust"));
    assert!(LanguageMenuSettings::default().shows("cobol"));
    assert!(LanguageMenuSettings::default().shows(""));
}

#[test]
fn what_7_4b_left_defaults_to_what_birchpad_did() {
    let settings = Settings::default();
    assert_eq!(settings.editor.non_printing_style, NonPrintingStyle::Marks);
    assert!(!settings.editor.smooth_font);
    assert_eq!(
        settings.appearance.dark_tone_colors,
        DarkToneColors::default()
    );
    assert_eq!(settings.appearance.dark_tone_colors.set().count(), 0);
    assert_eq!(DarkTone::Custom.hue(), None);
}

#[test]
fn what_7_4b_left_reads_its_names() {
    let settings: Settings = toml::from_str(
        "[editor]\nnon-printing-style = \"code-point\"\nsmooth-font = true\n[appearance]\ndark-tone = \"custom\"\n[appearance.dark-tone-colors]\nbackground = \"#102030\"\ntext = \"#ABCdef\"\naccent = \"\"\n",
    )
    .unwrap();
    assert_eq!(
        settings.editor.non_printing_style,
        NonPrintingStyle::CodePoint
    );
    assert!(settings.editor.smooth_font);
    assert_eq!(settings.appearance.dark_tone, DarkTone::Custom);
    let colors: Vec<_> = settings.appearance.dark_tone_colors.set().collect();
    assert_eq!(colors, [("background", "#102030"), ("text", "#ABCdef")]);
    check_setting(
        "editor.non-printing-style",
        &toml::Value::String("abbreviation".into()),
    )
    .unwrap();
    check_setting(
        "appearance.dark-tone-colors.error",
        &toml::Value::String("#ff0000".into()),
    )
    .unwrap();
    // Empty: the dark interface's own color.
    check_setting(
        "appearance.dark-tone-colors.error",
        &toml::Value::String(String::new()),
    )
    .unwrap();
}

#[test]
fn what_7_4b_left_refuses_what_is_not_one_of_its_values() {
    let refused =
        |key: &str, value: &str| check_setting(key, &toml::Value::String(value.into())).is_err();
    assert!(refused("editor.non-printing-style", "boxes"));
    for color in ["red", "102030", "#12345", "#1234567", "#12345g", "#"] {
        assert!(
            refused("appearance.dark-tone-colors.text", color),
            "{color}"
        );
    }
    // A part the tone does not color.
    assert!(refused(
        "appearance.dark-tone-colors.tab-color-1",
        "#102030"
    ));
    assert!(check_setting("editor.smooth-font", &toml::Value::Integer(1)).is_err());
}

#[test]
fn sql_backslash_escapes_are_off_by_default() {
    assert!(!Settings::default().sql.backslash_escapes);
    let settings: Settings = toml::from_str("[sql]\nbackslash-escapes = true\n").unwrap();
    assert!(settings.sql.backslash_escapes);
    check_setting("sql.backslash-escapes", &toml::Value::Boolean(true)).unwrap();
    assert!(check_setting("sql.backslash-escapes", &toml::Value::String("yes".into())).is_err());
    assert!(check_setting("sql.escapes", &toml::Value::Boolean(true)).is_err());
}
