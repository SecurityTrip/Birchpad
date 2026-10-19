use birchpad_config::{DEFAULT_TOOLBAR, ToolbarSettings, ToolbarSize};
use gpui_kit::{Entity, MouseButton, TestAppContext, VisualTestContext};

use super::*;
use crate::app_state::AppState;
use crate::workspace::tests::{click_on, open_workspace, tab_names};

fn settings(buttons: &[&str]) -> ToolbarSettings {
    ToolbarSettings {
        buttons: buttons.iter().map(|&id| id.to_owned()).collect(),
        ..ToolbarSettings::default()
    }
}

fn button(id: &str) -> Item {
    Item::Button(id.to_owned())
}

#[test]
fn the_default_buttons_are_known_commands_with_icons() {
    let items = items(&ToolbarSettings::default());
    let buttons = DEFAULT_TOOLBAR.iter().filter(|id| **id != "|").count();
    assert_eq!(
        items
            .iter()
            .filter(|item| matches!(item, Item::Button(_)))
            .count(),
        buttons,
        "every default command is in the catalog"
    );
    for id in DEFAULT_TOOLBAR.iter().filter(|id| **id != "|") {
        assert!(icon(id).is_some(), "{id} has an icon");
    }
    for (id, svg) in ICONS {
        assert!(birchpad_commands::find(id).is_some(), "{id}");
        assert!(std::str::from_utf8(svg).unwrap().contains("<svg"), "{id}");
    }
}

#[test]
fn unknown_commands_and_stray_separators_are_left_out() {
    assert_eq!(
        items(&settings(&[
            "|",
            " file.new ",
            "no.such",
            "|",
            "|",
            "edit.undo",
            "|"
        ])),
        [button("file.new"), Item::Separator, button("edit.undo")]
    );
    assert!(items(&settings(&[])).is_empty());
    assert!(items(&settings(&["|", "|", "nothing"])).is_empty());
    // A command without an icon is still a button (it shows its title).
    assert_eq!(items(&settings(&["file.rename"])), [button("file.rename")]);
    assert!(icon("file.rename").is_none());
}

fn toolbar(
    workspace: &Entity<Workspace>,
    cx: &mut VisualTestContext,
    change: impl FnOnce(&mut ToolbarSettings),
) {
    cx.update(|_, cx| change(&mut cx.global_mut::<AppState>().settings.toolbar));
    workspace.update(cx, |workspace, cx| workspace.refresh_views(cx));
    cx.run_until_parked();
}

fn bounds(
    selector: &'static str,
    cx: &mut VisualTestContext,
) -> Option<gpui_kit::Bounds<gpui_kit::Pixels>> {
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    cx.debug_bounds(selector)
}

#[gpui_kit::test]
fn buttons_run_their_commands_and_show_switches(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    assert!(bounds("toolbar", cx).is_some());
    click_on("toolbar-file.new", MouseButton::Left, cx);
    assert_eq!(tab_names(&workspace, cx), ["new 1", "new 2"]);

    // A switch's button is pressed while it is on.
    let pressed = |cx: &mut VisualTestContext| {
        workspace.read_with(cx, |workspace, _| {
            workspace
                .toolbar_checked
                .contains(&"view.word-wrap".to_owned())
        })
    };
    assert!(!pressed(cx));
    click_on("toolbar-view.word-wrap", MouseButton::Left, cx);
    assert!(pressed(cx));
    click_on("toolbar-view.word-wrap", MouseButton::Left, cx);
    assert!(!pressed(cx));
}

#[gpui_kit::test]
fn the_toolbar_is_hidden_resized_or_rearranged(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let small = bounds("toolbar", cx).unwrap().size.height;
    toolbar(&workspace, cx, |toolbar| toolbar.size = ToolbarSize::Large);
    assert!(bounds("toolbar", cx).unwrap().size.height > small);

    // Only the buttons asked for, in their order.
    toolbar(&workspace, cx, |toolbar| {
        toolbar.buttons = vec!["edit.redo".into(), "|".into(), "file.new".into()];
    });
    let redo = bounds("toolbar-edit.redo", cx).unwrap();
    let new = bounds("toolbar-file.new", cx).unwrap();
    assert!(redo.left() < new.left());
    assert!(bounds("toolbar-file.open", cx).is_none());

    // Hidden, or without buttons: no toolbar.
    toolbar(&workspace, cx, |toolbar| toolbar.buttons.clear());
    assert!(bounds("toolbar", cx).is_none());
    toolbar(&workspace, cx, |toolbar| {
        *toolbar = ToolbarSettings {
            show: false,
            ..ToolbarSettings::default()
        };
    });
    assert!(bounds("toolbar", cx).is_none());

    // Post-It hides it too.
    toolbar(&workspace, cx, |toolbar| toolbar.show = true);
    assert!(bounds("toolbar", cx).is_some());
    cx.simulate_keystrokes("f12");
    assert!(bounds("toolbar", cx).is_none());
}
