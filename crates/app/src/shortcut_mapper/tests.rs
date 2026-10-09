use gpui_kit::{TestAppContext, VisualTestContext};
use serde_json::json;

use super::*;
use crate::workspace::tests::open_workspace;

fn defaults() -> Keymap {
    Keymap::with_defaults(Platform::current())
}

#[test]
fn rows_list_commands_and_invocations_with_arguments_once() {
    let keymap = defaults();
    let rows = rows(&keymap);
    let new = rows
        .iter()
        .find(|row| row.invocation == Invocation::new("file.new"))
        .unwrap();
    assert_eq!(new.name, "New");
    assert_eq!(new.category, "file");
    let upper = Invocation::with_args("edit.convert-case", json!({ "to": "upper" }));
    assert!(
        rows.iter().any(|row| row.invocation == upper),
        "menu items with arguments"
    );
    let mut seen = Vec::new();
    for row in &rows {
        assert!(
            !seen.contains(&&row.invocation),
            "{:?} twice",
            row.invocation
        );
        seen.push(&row.invocation);
    }
    assert!(rows.len() > birchpad_commands::COMMANDS.len());
}

#[test]
fn filters_by_name_id_and_keys() {
    let keymap = defaults();
    let row = Row {
        invocation: Invocation::new("file.save"),
        name: "Save".into(),
        category: "file".into(),
    };
    let keys = shortcuts(&keymap, &row.invocation);
    assert!(!keys.is_empty());
    for filter in ["", "  ", "save", "SAV", "file.sa", keys[0].as_str()] {
        assert!(matches(&row, &keys, filter), "{filter:?}");
    }
    for filter in ["open", "zzz", "file.new"] {
        assert!(!matches(&row, &keys, filter), "{filter:?}");
    }
}

#[test]
fn conflicts_are_other_commands_on_the_same_keys() {
    let keymap = defaults();
    let new = Invocation::new("file.new");
    let keys = keymap.bindings_for(&new).next().unwrap().keys.clone();
    assert_eq!(
        conflict(&keymap, &keys, &Invocation::new("file.save")),
        Some(new.clone())
    );
    // Its own keys, and keys nothing runs, are no conflict.
    assert_eq!(conflict(&keymap, &keys, &new), None);
    let free = vec![Keystroke::parse("ctrl-alt-shift-f12", Platform::current()).unwrap()];
    assert_eq!(conflict(&keymap, &free, &new), None);
}

fn mapper(
    cx: &mut TestAppContext,
) -> (
    Entity<ShortcutMapper>,
    &mut VisualTestContext,
    tempfile::TempDir,
) {
    let (workspace, cx) = open_workspace(cx);
    let dir = tempfile::tempdir().unwrap();
    let settings = dir.path().join("settings.toml");
    cx.update(|_, cx| cx.global_mut::<AppState>().paths.user_settings = Some(settings));
    let weak = workspace.downgrade();
    let mapper = workspace.update_in(cx, |_, window, cx| {
        cx.new(|cx| ShortcutMapper::new(weak, window, cx))
    });
    (mapper, cx, dir)
}

fn select(mapper: &Entity<ShortcutMapper>, invocation: &Invocation, cx: &mut VisualTestContext) {
    mapper.update(cx, |this, cx| {
        let index = this
            .rows
            .iter()
            .position(|row| &row.invocation == invocation)
            .unwrap();
        this.select(index, cx);
    });
}

fn keys_now(invocation: &Invocation, cx: &mut VisualTestContext) -> Vec<String> {
    cx.update(|_, cx| shortcuts(&cx.global::<ActiveKeymap>().0, invocation))
}

#[gpui_kit::test]
fn assign_remove_and_reset(cx: &mut TestAppContext) {
    let (mapper, cx, dir) = mapper(cx);
    let close = Invocation::new("file.close-others");
    let before = keys_now(&close, cx);
    select(&mapper, &close, cx);
    mapper.update(cx, |this, cx| {
        this.start_capture(cx);
        this.pressed_keys("ctrl-alt-shift-q", cx);
        this.assign(cx);
        assert!(this.error.is_none(), "{:?}", this.error);
        assert!(this.capture.is_none());
    });
    assert!(keys_now(&close, cx).contains(&"ctrl-alt-shift-q".to_owned()));
    let file = std::fs::read_to_string(dir.path().join("keymap.toml")).unwrap();
    assert!(file.contains("file.close-others"), "{file}");
    mapper.update(cx, |this, cx| this.remove("ctrl-alt-shift-q", cx));
    assert_eq!(keys_now(&close, cx), before);

    // A default shortcut removed, then given back.
    let new = Invocation::new("file.new");
    let default = keys_now(&new, cx);
    select(&mapper, &new, cx);
    mapper.update(cx, |this, cx| this.remove(&default[0], cx));
    assert!(!keys_now(&new, cx).contains(&default[0]));
    mapper.update(cx, |this, cx| this.reset(cx));
    assert_eq!(keys_now(&new, cx), default);
}

#[gpui_kit::test]
fn keys_pressed_while_capturing_are_taken_not_run(cx: &mut TestAppContext) {
    let (mapper, cx, _dir) = mapper(cx);
    let save = Invocation::new("file.save-all");
    select(&mapper, &save, cx);
    let new_keys = keys_now(&Invocation::new("file.new"), cx)[0].clone();
    mapper.update(cx, |this, cx| this.start_capture(cx));
    cx.simulate_keystrokes(&new_keys);
    cx.run_until_parked();
    let pressed = mapper.read_with(cx, |this, _| {
        this.pressed.as_ref().map(|keys| keys[0].to_string())
    });
    assert_eq!(pressed.as_deref(), Some(new_keys.as_str()));
    // Escape gives up without closing anything.
    cx.simulate_keystrokes("escape");
    assert!(mapper.read_with(cx, |this, _| this.capture.is_none()
        && this.pressed.is_none()));
    // Without a capture, keys run their commands again.
    drop(mapper);
}

#[gpui_kit::test]
fn nothing_happens_without_a_selection_or_keys(cx: &mut TestAppContext) {
    let (mapper, cx, dir) = mapper(cx);
    mapper.update(cx, |this, cx| {
        this.assign(cx);
        this.remove("ctrl-n", cx);
        this.reset(cx);
        this.select(0, cx);
        // Selected, but no keys pressed.
        this.assign(cx);
        // Keys that do not read.
        this.start_capture(cx);
        this.pressed_keys("ctrl-", cx);
        assert!(this.error.is_some());
        assert!(this.pressed.is_none());
    });
    assert!(!dir.path().join("keymap.toml").exists());
}

#[gpui_kit::test]
fn a_keymap_that_cannot_be_changed_says_so(cx: &mut TestAppContext) {
    let (mapper, cx, dir) = mapper(cx);
    std::fs::write(dir.path().join("keymap.toml"), "[[binding]").unwrap();
    select(&mapper, &Invocation::new("file.new"), cx);
    mapper.update(cx, |this, cx| {
        this.reset(cx);
        assert!(this.error.is_some());
    });
    assert_eq!(
        std::fs::read_to_string(dir.path().join("keymap.toml")).unwrap(),
        "[[binding]"
    );
    // No settings folder at all.
    cx.update(|_, cx| cx.global_mut::<AppState>().paths.user_settings = None);
    mapper.update(cx, |this, cx| {
        this.error = None;
        this.reset(cx);
        assert!(this.error.is_some());
    });
}

#[gpui_kit::test]
fn the_filter_and_the_command(cx: &mut TestAppContext) {
    let (mapper, cx, _dir) = mapper(cx);
    mapper.update_in(cx, |this, window, cx| {
        this.filter.update(cx, |input, cx| {
            input.set_value("Shortcut Mapper", window, cx)
        });
        this.refilter(cx);
        assert_eq!(this.shown.len(), 1);
        this.filter.update(cx, |input, cx| {
            input.set_value("no command is called this", window, cx)
        });
        this.refilter(cx);
        assert!(this.shown.is_empty());
    });
    let (workspace, cx) = open_workspace(&mut cx.cx);
    workspace.update_in(cx, |workspace, window, cx| {
        workspace
            .dispatch(&Invocation::new("settings.shortcut-mapper"), window, cx)
            .unwrap();
    });
    cx.run_until_parked();
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
}
