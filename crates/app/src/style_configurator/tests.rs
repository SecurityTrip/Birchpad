use birchpad_commands::Invocation;
use gpui_kit::{TestAppContext, VisualTestContext};

use super::*;
use crate::app_state::AppState;
use crate::workspace::tests::open_workspace;

fn index_of(group: Group, item: &str) -> usize {
    items(group).iter().position(|name| *name == item).unwrap()
}

#[test]
fn groups_list_everything_once() {
    let groups = groups();
    assert_eq!(groups[0].1, Group::Editor);
    assert_eq!(groups[1].1, Group::Interface);
    assert_eq!(groups[2].1, Group::Syntax(None));
    assert_eq!(groups.len(), 3 + birchpad_syntax::LANGUAGES.len());
    assert!(groups.contains(&("Python".to_owned(), Group::Syntax(Some("python")))));
    assert!(items(Group::Editor).contains(&"caret"));
    assert!(items(Group::Interface).contains(&"accent"));
    assert!(items(Group::Syntax(None)).contains(&"keyword"));
}

#[test]
fn colors_read_and_write() {
    let mut theme = Theme::default_theme().clone();
    let fields = read_fields(&theme, Group::Editor, "caret");
    assert_eq!(fields.color, "#1f2328");
    assert_eq!(fields.background, "");
    // With or without `#`, opaque or translucent.
    for (text, expected) in [
        ("#ff0000", 0xff0000ff),
        ("00ff00", 0x00ff00ff),
        (" #0000ff80 ", 0x0000ff80),
    ] {
        let fields = Fields {
            color: text.into(),
            ..Fields::default()
        };
        write_fields(&mut theme, Group::Editor, "caret", &fields).unwrap();
        assert_eq!(theme.editor.caret, Color(expected), "{text}");
    }
    let fields = Fields {
        color: "#123456".into(),
        ..Fields::default()
    };
    write_fields(&mut theme, Group::Interface, "accent", &fields).unwrap();
    assert_eq!(theme.ui.accent, Color::rgb(0x123456));
    assert_eq!(
        read_fields(&theme, Group::Interface, "accent").color,
        "#123456"
    );
}

#[test]
fn colors_refuse_bad_values() {
    let mut theme = Theme::default_theme().clone();
    for text in ["", "  ", "red", "#12345", "#1234567", "#gggggg"] {
        let fields = Fields {
            color: text.into(),
            ..Fields::default()
        };
        assert!(
            write_fields(&mut theme, Group::Editor, "caret", &fields).is_err(),
            "{text:?}"
        );
    }
    let fields = Fields {
        color: "#ffffff".into(),
        ..Fields::default()
    };
    assert!(write_fields(&mut theme, Group::Editor, "nothing", &fields).is_err());
    assert!(write_fields(&mut theme, Group::Interface, "caret", &fields).is_err());
    assert!(write_fields(&mut theme, Group::Syntax(None), "nothing", &fields).is_err());
    assert_eq!(theme, *Theme::default_theme());
}

#[test]
fn styles_for_all_languages_and_for_one() {
    let mut theme = Theme::default_theme().clone();
    let keyword = read_fields(&theme, Group::Syntax(None), "keyword");
    assert_eq!(keyword.color, "#0000ff");
    assert!(keyword.bold);
    // A language without styles of its own shows empty fields.
    let python = Group::Syntax(Some("python"));
    assert_eq!(read_fields(&theme, python, "keyword"), Fields::default());
    let fields = Fields {
        color: "#ff00ff".into(),
        background: "#00000040".into(),
        italic: true,
        underline: true,
        ..Fields::default()
    };
    write_fields(&mut theme, python, "keyword", &fields).unwrap();
    assert_eq!(
        read_fields(&theme, python, "keyword"),
        Fields {
            background: "#00000040".into(),
            ..fields.clone()
        }
    );
    let style = theme.syntax_style(Some("python"), "keyword").unwrap();
    assert_eq!(style.color, Some(Color::rgb(0xff00ff)));
    assert!(style.italic && style.underline && !style.bold);
    // Clearing every field gives the style back to all languages.
    write_fields(&mut theme, python, "keyword", &Fields::default()).unwrap();
    assert!(!theme.languages.contains_key("python"));
    // For all languages, it falls back to the prefix.
    write_fields(
        &mut theme,
        Group::Syntax(None),
        "string.escape",
        &Fields::default(),
    )
    .unwrap();
    assert!(!theme.syntax.contains_key("string.escape"));
    assert_eq!(
        theme.syntax_style(None, "string.escape"),
        theme.syntax_style(None, "string")
    );
    // A style may be bold alone, without a color.
    let bold = Fields {
        bold: true,
        ..Fields::default()
    };
    write_fields(&mut theme, Group::Syntax(None), "variable", &bold).unwrap();
    assert!(theme.syntax["variable"].bold);
    assert_eq!(theme.syntax["variable"].color, None);
    // A bad background is refused without a change.
    let before = theme.clone();
    let bad = Fields {
        color: "#ffffff".into(),
        background: "white".into(),
        ..Fields::default()
    };
    assert!(write_fields(&mut theme, python, "keyword", &bad).is_err());
    assert_eq!(theme, before);
}

#[test]
fn save_writes_a_theme_file() {
    let dir = tempfile::tempdir().unwrap();
    let mut theme = Theme::builtin("Dark").unwrap().clone();
    theme.editor.caret = Color::rgb(0xabcdef);
    let themes = dir.path().join("themes");
    save(&theme, &themes).unwrap();
    assert_eq!(themes::load(Some(&themes), "Dark").unwrap(), theme);
    // A folder that cannot be made.
    let file = dir.path().join("file");
    std::fs::write(&file, "").unwrap();
    assert!(save(&theme, &file.join("themes")).is_err());
}

fn configurator(
    cx: &mut TestAppContext,
) -> (
    Entity<StyleConfigurator>,
    &mut VisualTestContext,
    tempfile::TempDir,
) {
    let (workspace, cx) = open_workspace(cx);
    let dir = tempfile::tempdir().unwrap();
    let settings = dir.path().join("settings.toml");
    cx.update(|_, cx| cx.global_mut::<AppState>().paths.user_settings = Some(settings));
    let weak = workspace.downgrade();
    let configurator = workspace.update_in(cx, |_, window, cx| {
        cx.new(|cx| StyleConfigurator::new(weak, window, cx))
    });
    (configurator, cx, dir)
}

fn type_color(configurator: &Entity<StyleConfigurator>, text: &str, cx: &mut VisualTestContext) {
    let text = text.to_owned();
    configurator.update_in(cx, |this, window, cx| {
        this.color
            .update(cx, |input, cx| input.set_value(text, window, cx));
        this.edit(cx);
    });
}

#[gpui_kit::test]
fn edits_show_at_once_and_cancel_reverts(cx: &mut TestAppContext) {
    let (configurator, cx, _dir) = configurator(cx);
    let caret = index_of(Group::Editor, "caret");
    configurator.update_in(cx, |this, window, cx| this.select_item(caret, window, cx));
    type_color(&configurator, "#ff0000", cx);
    assert_eq!(crate::theme::editor().caret, Color::rgb(0xff0000));
    // A bad color says so and changes nothing.
    type_color(&configurator, "#ff00", cx);
    assert!(configurator.read_with(cx, |this, _| this.error.is_some()));
    assert_eq!(crate::theme::editor().caret, Color::rgb(0xff0000));
    configurator.update(cx, |this, cx| this.revert(cx));
    assert_eq!(
        crate::theme::editor().caret,
        Theme::default_theme().editor.caret
    );
}

#[gpui_kit::test]
fn checkboxes_and_languages(cx: &mut TestAppContext) {
    let (configurator, cx, _dir) = configurator(cx);
    let groups = groups();
    let python = groups
        .iter()
        .position(|(_, group)| *group == Group::Syntax(Some("python")))
        .unwrap();
    let keyword = index_of(Group::Syntax(None), "keyword");
    configurator.update_in(cx, |this, window, cx| {
        this.select_group(python, window, cx);
        this.select_item(keyword, window, cx);
        this.toggle(|fields| fields.italic = !fields.italic, cx);
    });
    let theme = crate::theme::current();
    assert!(theme.languages["python"]["keyword"].italic);
    // Other languages keep their style.
    assert!(!theme.syntax_style(Some("rust"), "keyword").unwrap().italic);
    configurator.update(cx, |this, cx| this.revert(cx));
}

#[gpui_kit::test]
fn switching_and_saving_themes(cx: &mut TestAppContext) {
    let (configurator, cx, dir) = configurator(cx);
    let dark = configurator.read_with(cx, |this, _| {
        this.names.iter().position(|name| name == "Dark").unwrap()
    });
    configurator.update_in(cx, |this, window, cx| {
        this.select_theme(dark, window, cx);
        // The fields show the new theme's colors.
        assert_eq!(this.fields.color, "#d4d4d4");
    });
    assert!(crate::theme::current().dark);
    let caret = index_of(Group::Editor, "caret");
    configurator.update_in(cx, |this, window, cx| this.select_item(caret, window, cx));
    type_color(&configurator, "#00ff00", cx);
    assert!(configurator.update(cx, |this, cx| this.save(cx)));
    let saved = themes::load(Some(&dir.path().join("themes")), "Dark").unwrap();
    assert_eq!(saved.editor.caret, Color::rgb(0x00ff00));
    cx.update(|_, cx| assert_eq!(AppState::global(cx).state.theme.as_deref(), Some("Dark")));
    // A field with an error keeps the dialog open.
    type_color(&configurator, "nope", cx);
    assert!(!configurator.update(cx, |this, cx| this.save(cx)));
    crate::theme::set(Theme::default_theme().clone());
}

#[gpui_kit::test]
fn saving_without_a_settings_folder_fails(cx: &mut TestAppContext) {
    let (configurator, cx, _dir) = configurator(cx);
    cx.update(|_, cx| cx.global_mut::<AppState>().paths.user_settings = None);
    assert!(!configurator.update(cx, |this, cx| this.save(cx)));
    assert!(configurator.read_with(cx, |this, _| this.error.is_some()));
}

#[gpui_kit::test]
fn the_command_opens_the_dialog(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    workspace.update_in(cx, |workspace, window, cx| {
        workspace
            .dispatch(&Invocation::new("settings.style-configurator"), window, cx)
            .unwrap();
    });
    cx.run_until_parked();
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
}
