//! The Preferences dialog driven as a user drives it: every section drawn, its controls
//! clicked and typed into, locked settings, refused values and the language table.

use birchpad_commands::Invocation;
use gpui_kit::{Entity, MouseButton, TestAppContext, VisualTestContext};

use super::*;
use crate::workspace::Workspace;
use crate::workspace::tests::{click_on, open_workspace, secondary};

/// A workspace with a settings file to write to, and Preferences open.
fn open_preferences(
    cx: &mut TestAppContext,
) -> (Entity<Workspace>, &mut VisualTestContext, tempfile::TempDir) {
    let (workspace, cx) = open_workspace(cx);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.toml");
    std::fs::write(&path, "# mine\n").unwrap();
    cx.update(|_, cx| cx.global_mut::<AppState>().paths.user_settings = Some(path));
    workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.dispatch(&Invocation::new("settings.preferences"), window, cx)
        })
        .unwrap();
    cx.run_until_parked();
    (workspace, cx, dir)
}

/// The dialog opened last.
fn opened() -> Entity<Preferences> {
    OPENED
        .with(|opened| opened.borrow().clone().and_then(|weak| weak.upgrade()))
        .expect("Preferences is open")
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
}

fn show_section(name: &str, cx: &mut VisualTestContext) {
    let index = sections().iter().position(|known| *known == name).unwrap();
    opened().update(cx, |preferences, cx| preferences.select_section(index, cx));
    draw(cx);
}

/// Clicks what `selector` marks near its right end (a radio group's last choice).
fn click_right_end(selector: &'static str, cx: &mut VisualTestContext) {
    draw(cx);
    let bounds = cx.debug_bounds(selector).expect(selector);
    let at = gpui_kit::point(
        bounds.origin.x + bounds.size.width - gpui_kit::px(12.),
        bounds.center().y,
    );
    cx.simulate_mouse_down(at, MouseButton::Left, gpui_kit::Modifiers::none());
    cx.simulate_mouse_up(at, MouseButton::Left, gpui_kit::Modifiers::none());
    cx.run_until_parked();
}

/// Clicks a field and types `text` over what it holds.
fn type_over(selector: &'static str, text: &str, cx: &mut VisualTestContext) {
    draw(cx);
    click_on(selector, MouseButton::Left, cx);
    cx.simulate_keystrokes(&secondary("a"));
    cx.simulate_keystrokes("backspace");
    cx.simulate_input(text);
    cx.run_until_parked();
}

fn settings(cx: &mut VisualTestContext) -> Settings {
    cx.update(|_, cx| AppState::global(cx).settings.clone())
}

#[test]
fn entries_made_at_run_time() {
    let made = entry("Toolbar", "toolbar.show", "Show the toolbar", Kind::Bool);
    assert_eq!(
        (made.section, made.key, made.label),
        ("Toolbar", "toolbar.show", "Show the toolbar")
    );
    assert!(ENTRIES.iter().any(|known| known.key == made.key));
}

#[gpui_kit::test]
fn sections_are_chosen_from_the_list(cx: &mut TestAppContext) {
    let (_, cx, _dir) = open_preferences(cx);
    for index in [1, 0, 2] {
        let selector: &'static str = format!("preferences-section-{index}").leak();
        draw(cx);
        click_on(selector, MouseButton::Left, cx);
        assert_eq!(
            opened().read_with(cx, |preferences, _| preferences.section),
            index
        );
    }
    // Past the last section, the last one shows.
    opened().update(cx, |preferences, cx| {
        preferences.select_section(usize::MAX, cx)
    });
    draw(cx);
    let last = sections().len() - 1;
    let first = ENTRIES
        .iter()
        .find(|entry| entry.section == sections()[last])
        .unwrap();
    let selector: &'static str = format!("preferences-{}", first.key).leak();
    assert!(cx.debug_bounds(selector).is_some(), "{selector}");
}

#[gpui_kit::test]
fn every_section_draws(cx: &mut TestAppContext) {
    let (_, cx, _dir) = open_preferences(cx);
    for name in sections() {
        show_section(name, cx);
        // The first control of every section is drawn.
        let first = ENTRIES.iter().find(|entry| entry.section == name).unwrap();
        let selector: &'static str = match first.kind {
            Kind::LanguageTabs => "language-set",
            _ => format!("preferences-{}", first.key).leak(),
        };
        assert!(cx.debug_bounds(selector).is_some(), "{name}: {selector}");
    }
}

#[gpui_kit::test]
fn switches_choices_and_fields_change_settings(cx: &mut TestAppContext) {
    let (_, cx, dir) = open_preferences(cx);
    show_section("Toolbar", cx);
    let shown = settings(cx).toolbar.show;
    click_on("preferences-toolbar.show", MouseButton::Left, cx);
    assert_eq!(settings(cx).toolbar.show, !shown);
    let size = settings(cx).toolbar.size;
    click_right_end("preferences-toolbar.size", cx);
    assert_ne!(settings(cx).toolbar.size, size, "another size was chosen");

    show_section("Editing", cx);
    type_over("preferences-editor.tab-width", "6", cx);
    assert_eq!(settings(cx).editor.tab_width, 6);
    // Past the limits, and not a number: refused, the setting stays.
    for refused in ["0", "six", "-1"] {
        type_over("preferences-editor.tab-width", refused, cx);
        assert_eq!(settings(cx).editor.tab_width, 6, "{refused:?}");
    }
    // Each keystroke applies what reads: of "17", the "1".
    type_over("preferences-editor.tab-width", "17", cx);
    assert_eq!(settings(cx).editor.tab_width, 1);
    // The edges are taken.
    type_over("preferences-editor.tab-width", "16", cx);
    assert_eq!(settings(cx).editor.tab_width, 16);
    type_over("preferences-editor.tab-width", "1", cx);
    assert_eq!(settings(cx).editor.tab_width, 1);

    show_section("Margins", cx);
    type_over("preferences-editor.edge-columns", "80, 120", cx);
    assert_eq!(settings(cx).editor.edge_columns, [80, 120]);
    let written = std::fs::read_to_string(dir.path().join("settings.toml")).unwrap();
    assert!(written.starts_with("# mine\n"), "{written}");
    assert!(written.contains("tab-width = 1"), "{written}");
}

#[gpui_kit::test]
fn locked_settings_cannot_be_changed(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.update(|_, cx| {
        cx.global_mut::<AppState>()
            .policies
            .push(("toolbar.show".into(), "true".into()));
    });
    workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.dispatch(&Invocation::new("settings.preferences"), window, cx)
        })
        .unwrap();
    cx.run_until_parked();
    show_section("Toolbar", cx);
    let shown = settings(cx).toolbar.show;
    click_on("preferences-toolbar.show", MouseButton::Left, cx);
    assert_eq!(settings(cx).toolbar.show, shown, "disabled");
    // The Language table is locked as a whole.
    cx.update(|_, cx| {
        cx.global_mut::<AppState>()
            .policies
            .push(("languages".into(), "{}".into()));
    });
    show_section("Language", cx);
    type_over("language-id", "python", cx);
    click_on("language-set", MouseButton::Left, cx);
    assert!(settings(cx).languages.is_empty(), "disabled");
}

#[gpui_kit::test]
fn languages_get_their_own_tabs_and_lose_them(cx: &mut TestAppContext) {
    let (_, cx, _dir) = open_preferences(cx);
    show_section("Language", cx);
    type_over("language-id", "Python", cx);
    type_over("language-width", "16", cx);
    click_on("language-spaces", MouseButton::Left, cx);
    click_on("language-set", MouseButton::Left, cx);
    let python = settings(cx).languages.get("python").cloned().unwrap();
    assert_eq!(
        (python.tab_width, python.insert_spaces),
        (Some(16), Some(true))
    );

    // An unknown language and widths past the limits change nothing; the dialog stays.
    for (id, width) in [("klingon", "4"), ("rust", "0"), ("rust", "17"), ("", "4")] {
        type_over("language-id", id, cx);
        type_over("language-width", width, cx);
        click_on("language-set", MouseButton::Left, cx);
        assert!(!settings(cx).languages.contains_key("rust"), "{id} {width}");
        assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    }
    assert_eq!(settings(cx).languages.len(), 1);

    click_on("language-remove-0", MouseButton::Left, cx);
    let python = settings(cx)
        .languages
        .get("python")
        .cloned()
        .unwrap_or_default();
    assert_eq!((python.tab_width, python.insert_spaces), (None, None));
    assert!(
        cx.debug_bounds("language-remove-0").is_none(),
        "no row left"
    );
}

#[gpui_kit::test]
fn a_settings_file_with_problems_still_takes_a_change(cx: &mut TestAppContext) {
    let (_, cx, dir) = open_preferences(cx);
    let path = dir.path().join("settings.toml");
    std::fs::write(&path, "[editor]\nno-such-setting = 1\n").unwrap();
    cx.update(|_, cx| {
        set("editor.tab-width", Some(Value::Integer(3)), cx).unwrap();
        assert_eq!(AppState::global(cx).settings.editor.tab_width, 3);
    });
    let written = std::fs::read_to_string(&path).unwrap();
    assert!(written.contains("no-such-setting = 1"), "kept: {written}");
}

/// Preferences > Files > Edit with Birchpad: the registry's, so on Windows the test puts back
/// what the user had.
#[gpui_kit::test]
fn the_explorer_menu_switch(cx: &mut TestAppContext) {
    let (_, cx, _dir) = open_preferences(cx);
    show_section("Files", cx);
    let had = birchpad_cli::explorer::is_added();
    let had_removed = birchpad_cli::explorer::is_removed_by_user();
    click_on("preferences-explorer-menu", MouseButton::Left, cx);
    if cfg!(windows) {
        assert_eq!(birchpad_cli::explorer::is_added(), !had);
        // Taken away by the user, it is remembered for updates; added, forgotten.
        assert_eq!(birchpad_cli::explorer::is_removed_by_user(), had);
        click_on("preferences-explorer-menu", MouseButton::Left, cx);
        assert_eq!(birchpad_cli::explorer::is_added(), had, "back as it was");
        assert_eq!(birchpad_cli::explorer::is_removed_by_user(), !had);
        if !had && !had_removed {
            // The user had chosen nothing: nothing is left remembered either.
            birchpad_cli::explorer::remove_for_uninstall().unwrap();
            assert!(!birchpad_cli::explorer::is_removed_by_user());
        }
    } else {
        assert!(!birchpad_cli::explorer::is_added(), "disabled elsewhere");
    }
    // Asked directly where there is no Explorer, it says why.
    opened().update(cx, |preferences, cx| {
        preferences.errors.clear();
        if !cfg!(windows) {
            preferences.switch_explorer_menu(true, cx);
            assert!(preferences.errors.contains_key(EXPLORER_MENU));
        }
    });
}
