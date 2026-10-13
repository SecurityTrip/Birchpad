//! The Shortcut Mapper driven as a user drives it: typed into, clicked, keys pressed, its tabs,
//! and the keymap file it cannot read or write.

use birchpad_commands::Invocation;
use gpui_kit::{Entity, MouseButton, TestAppContext, VisualTestContext};

use super::*;
use crate::workspace::tests::open_workspace;

/// A workspace with a settings folder, and the Shortcut Mapper open.
fn open_mapper(
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
    workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.dispatch(&Invocation::new("settings.shortcut-mapper"), window, cx)
        })
        .unwrap();
    cx.run_until_parked();
    let mapper = OPENED
        .with(|opened| opened.borrow().clone().and_then(|weak| weak.upgrade()))
        .expect("the mapper is open");
    (mapper, cx, dir)
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
}

fn click(selector: &'static str, cx: &mut VisualTestContext) {
    draw(cx);
    crate::workspace::tests::click_on(selector, MouseButton::Left, cx);
}

fn keys_now(invocation: &Invocation, cx: &mut VisualTestContext) -> Vec<String> {
    cx.update(|_, cx| shortcuts(&cx.global::<ActiveKeymap>().0, invocation))
}

#[gpui_kit::test]
fn a_shortcut_added_removed_and_reset_with_the_mouse(cx: &mut TestAppContext) {
    let (mapper, cx, _dir) = open_mapper(cx);
    // The filter has the focus.
    cx.simulate_input("file.close-others");
    draw(cx);
    let shown = mapper.read_with(cx, |this, _| this.shown.len());
    assert_eq!(shown, 1);
    let close = Invocation::new("file.close-others");
    let before = keys_now(&close, cx);

    click("shortcut-row-0", cx);
    assert_eq!(
        mapper.read_with(cx, |this, _| this
            .selected_row()
            .map(|row| row.invocation.clone())),
        Some(close.clone())
    );
    // Assign is there but off until keys are pressed.
    click("shortcut-add", cx);
    click("shortcut-assign", cx);
    assert_eq!(keys_now(&close, cx), before, "no keys, nothing assigned");
    cx.simulate_keystrokes("ctrl-alt-shift-q");
    click("shortcut-assign", cx);
    let now = keys_now(&close, cx);
    assert!(now.contains(&"ctrl-alt-shift-q".to_owned()), "{now:?}");

    let index = now
        .iter()
        .position(|keys| keys == "ctrl-alt-shift-q")
        .unwrap();
    let remove: &'static str = format!("shortcut-remove-{index}").leak();
    click(remove, cx);
    assert_eq!(keys_now(&close, cx), before);

    click("shortcut-only-here", cx);
    assert!(mapper.read_with(cx, |this, _| this.only_here));
    click("shortcut-reset", cx);
    assert_eq!(keys_now(&close, cx), before, "the defaults again");
}

#[gpui_kit::test]
fn keys_of_another_command_are_a_conflict(cx: &mut TestAppContext) {
    let (mapper, cx, _dir) = open_mapper(cx);
    cx.simulate_input("file.close-others");
    click("shortcut-row-0", cx);
    click("shortcut-add", cx);
    // Ctrl+S saves.
    cx.simulate_keystrokes(&crate::workspace::tests::secondary("s"));
    draw(cx);
    assert!(cx.debug_bounds("shortcut-conflict").is_some());
    // Escape gives up; nothing changed, and no conflict is shown.
    cx.simulate_keystrokes("escape");
    draw(cx);
    assert!(mapper.read_with(cx, |this, _| this.capture.is_none()));
    assert!(cx.debug_bounds("shortcut-conflict").is_none());
}

#[gpui_kit::test]
fn the_tabs_list_macros_and_command_lines(cx: &mut TestAppContext) {
    let (mapper, cx, _dir) = open_mapper(cx);
    cx.update(|_, cx| {
        crate::macros::save("Twice", vec![Invocation::new("edit.duplicate-line")], cx).unwrap();
        crate::run_menu::save("Shell", "cmd", cx).unwrap();
    });
    click("shortcut-tab-1", cx);
    assert_eq!(mapper.read_with(cx, |this, _| this.tab), MapperTab::Macros);
    click("shortcut-row-0", cx);
    click("shortcut-delete", cx);
    let saved = cx.update(|_, cx| cx.global::<crate::macros::Macros>().saved.len());
    assert_eq!(saved, 0);
    assert!(cx.debug_bounds("shortcut-row-0").is_none(), "no macro left");

    click("shortcut-tab-2", cx);
    assert_eq!(mapper.read_with(cx, |this, _| this.tab), MapperTab::Run);
    click("shortcut-row-0", cx);
    assert!(cx.debug_bounds("shortcut-delete").is_some());

    // The commands cannot be deleted: no button, and asking does nothing.
    click("shortcut-tab-0", cx);
    click("shortcut-row-0", cx);
    assert!(cx.debug_bounds("shortcut-delete").is_none());
    let rows = mapper.read_with(cx, |this, _| this.rows.len());
    mapper.update(cx, |this, cx| this.delete_selected(cx));
    assert_eq!(mapper.read_with(cx, |this, _| this.rows.len()), rows);
}

#[gpui_kit::test]
fn a_keymap_file_that_cannot_be_read_or_made(cx: &mut TestAppContext) {
    let (mapper, cx, dir) = open_mapper(cx);
    // keymap.toml is a folder.
    std::fs::create_dir(dir.path().join("keymap.toml")).unwrap();
    mapper.update(cx, |this, cx| {
        let new = this
            .rows
            .iter()
            .position(|row| row.invocation == Invocation::new("file.new"))
            .unwrap();
        this.select(new, cx);
        this.reset(cx);
        assert!(this.error.is_some());
    });
    draw(cx);
    // The settings folder is under a file.
    let file = dir.path().join("a-file");
    std::fs::write(&file, "").unwrap();
    cx.update(|_, cx| {
        cx.global_mut::<AppState>().paths.user_settings =
            Some(file.join("folder").join("settings.toml"));
    });
    mapper.update(cx, |this, cx| {
        this.error = None;
        this.reset(cx);
        assert!(this.error.is_some());
    });
}
