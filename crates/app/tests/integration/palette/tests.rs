use birchpad::app_state::AppState;
use birchpad::palette::*;
use birchpad::quick_pick::{QuickPick, highlight_ranges};
use birchpad::workspace::Workspace;
use birchpad_commands::{Invocation, Keymap, Platform};
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::json;

use crate::workspace::{open_workspace, tab_names};

fn command(workspace: &Entity<Workspace>, id: &str, cx: &mut VisualTestContext) {
    workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.dispatch(&Invocation::new(id), window, cx)
        })
        .unwrap();
    cx.run_until_parked();
}

/// Opens the palette and returns its list.
fn open_palette(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Entity<QuickPick> {
    command(workspace, "view.command-palette", cx);
    birchpad::quick_pick::OPENED
        .with(|opened| opened.borrow().clone())
        .and_then(|pick| pick.upgrade())
        .expect("the palette is open")
}

fn type_query(pick: &Entity<QuickPick>, text: &str, cx: &mut VisualTestContext) {
    let input = pick.read_with(cx, |pick, _| pick.input.clone());
    input.update_in(cx, |input, window, cx| input.set_value(text, window, cx));
    // Setting the value tells nothing, as typing would.
    pick.update(cx, |pick, cx| pick.refilter(cx));
    cx.run_until_parked();
}

fn shown(pick: &Entity<QuickPick>, cx: &mut VisualTestContext) -> Vec<String> {
    pick.read_with(cx, |pick, _| {
        pick.shown
            .iter()
            .map(|(index, _)| pick.items()[*index].text.clone())
            .collect()
    })
}

fn pick_chosen(pick: &Entity<QuickPick>, cx: &mut VisualTestContext) {
    pick.update_in(cx, |pick, window, cx| pick.pick(window, cx));
    cx.run_until_parked();
}

fn dialog_open(cx: &mut VisualTestContext) -> bool {
    cx.update(|window, cx| gpui_kit::component::WindowExt::has_active_dialog(window, cx))
}

#[gpui_kit::test]
fn lists_the_catalog_s_commands_with_their_shortcuts(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let (entries, recent) = cx.update(|_, cx| entries(&[], cx));
    assert_eq!(recent, 0);
    assert_eq!(entries[0].invocation, Invocation::new("file.new"));
    let save = entries
        .iter()
        .find(|entry| entry.invocation == Invocation::new("file.save"))
        .expect("Save is listed");
    assert_eq!(save.title, "Save");
    assert!(save.shortcut.ends_with('S'), "{}", save.shortcut);
    // Menu items with arguments, by their submenu and their own name.
    assert!(
        entries
            .iter()
            .any(|entry| entry.invocation.command == "language.set"
                && entry.invocation.args == json!({ "language": "python" })),
        "Language > Python is listed"
    );
    // Neither the palette itself nor commands that need arguments.
    for id in ["view.command-palette", "edit.insert-text", "edit.type"] {
        assert!(
            !entries
                .iter()
                .any(|entry| entry.invocation == Invocation::new(id)),
            "{id} is not listed"
        );
    }
    // Each once.
    for (index, entry) in entries.iter().enumerate() {
        assert!(
            !entries[index + 1..]
                .iter()
                .any(|other| other.invocation == entry.invocation),
            "{:?} twice",
            entry.invocation
        );
    }
    drop(workspace);
}

#[gpui_kit::test]
fn typing_finds_a_command_and_enter_runs_it(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let pick = open_palette(&workspace, cx);
    assert!(dialog_open(cx));
    type_query(&pick, "new", cx);
    assert_eq!(shown(&pick, cx)[0], "New");
    pick_chosen(&pick, cx);
    assert!(!dialog_open(cx));
    assert_eq!(tab_names(&workspace, cx), ["new 1", "new 2"]);
    // Remembered as the command used last, and listed first next time.
    let recent = cx.update(|_, cx| AppState::global(cx).state.recent_commands.clone());
    assert_eq!(recent, [recent_key(&Invocation::new("file.new"))]);
    let pick = open_palette(&workspace, cx);
    type_query(&pick, "", cx);
    assert_eq!(shown(&pick, cx)[0], "New");
    type_query(&pick, "close all", cx);
    assert_eq!(shown(&pick, cx)[0], "Close All");
}

#[gpui_kit::test]
fn the_recently_used_come_first_however_well_others_match(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.update(|_, cx| {
        AppState::update_state(cx, |state, _| {
            state.add_recent_command(&recent_key(&Invocation::new("file.save-all")), 8);
            state.add_recent_command(&recent_key(&Invocation::new("file.close-all")), 8);
        });
    });
    let pick = open_palette(&workspace, cx);
    assert_eq!(shown(&pick, cx)[..2], ["Close All", "Save All"]);
    type_query(&pick, "save", cx);
    assert_eq!(shown(&pick, cx)[0], "Save All");
    assert_eq!(shown(&pick, cx)[1], "Save");
}

#[gpui_kit::test]
fn no_recent_commands_with_a_limit_of_zero(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.update(|_, cx| {
        AppState::update_state(cx, |state, _| {
            state.add_recent_command(&recent_key(&Invocation::new("file.close-all")), 8);
        });
        cx.global_mut::<AppState>().settings.palette.recent_commands = 0;
    });
    let pick = open_palette(&workspace, cx);
    assert_eq!(shown(&pick, cx)[0], "New");
    type_query(&pick, "new", cx);
    pick_chosen(&pick, cx);
    let recent = cx.update(|_, cx| AppState::global(cx).state.recent_commands.clone());
    assert!(recent.is_empty(), "{recent:?}");
}

#[gpui_kit::test]
fn nothing_matching_picks_nothing(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let pick = open_palette(&workspace, cx);
    type_query(&pick, "qqqqzzzz", cx);
    assert!(shown(&pick, cx).is_empty());
    pick.update(cx, |pick, cx| pick.move_by(1, cx));
    pick_chosen(&pick, cx);
    assert!(dialog_open(cx), "the palette stays open");
    assert_eq!(tab_names(&workspace, cx), ["new 1"]);
}

#[gpui_kit::test]
fn up_and_down_stop_at_the_first_and_the_last(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let pick = open_palette(&workspace, cx);
    type_query(&pick, "close all", cx);
    let count = shown(&pick, cx).len();
    assert!(count > 2);
    let selected = |pick: &Entity<QuickPick>, cx: &mut VisualTestContext| {
        pick.read_with(cx, |pick, _| pick.selected)
    };
    pick.update(cx, |pick, cx| pick.move_by(-1, cx));
    assert_eq!(selected(&pick, cx), 0);
    pick.update(cx, |pick, cx| pick.move_by(1, cx));
    assert_eq!(selected(&pick, cx), 1);
    pick.update(cx, |pick, cx| pick.move_by(1000, cx));
    assert_eq!(selected(&pick, cx), count - 1);
    // Typing chooses the first again.
    type_query(&pick, "close", cx);
    assert_eq!(selected(&pick, cx), 0);
    cx.simulate_keystrokes("down");
    assert_eq!(selected(&pick, cx), 1);
    cx.simulate_keystrokes("up");
    assert_eq!(selected(&pick, cx), 0);
}

#[gpui_kit::test]
fn recent_commands_it_does_not_know_are_skipped(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    // Saved under a name the palette does not know: skipped.
    cx.update(|_, cx| {
        AppState::update_state(cx, |state, _| {
            state.add_recent_command("not json", 8);
            state.add_recent_command(&recent_key(&Invocation::new("no.such.command")), 8);
        });
    });
    let pick = open_palette(&workspace, cx);
    assert_eq!(shown(&pick, cx)[0], "New");
}

#[test]
fn shortcuts_show_as_the_menus_do() {
    let keymap = Keymap::with_defaults(Platform::current());
    let save = shortcut(&keymap, &Invocation::new("file.save"));
    let expected = if cfg!(target_os = "macos") {
        "⌘S"
    } else {
        "Ctrl+S"
    };
    assert_eq!(save, expected);
    assert_eq!(shortcut(&keymap, &Invocation::new("file.rename")), "");
    assert_eq!(shortcut(&keymap, &Invocation::new("no.such.command")), "");
}

#[test]
fn highlights_cover_whole_characters() {
    assert_eq!(highlight_ranges("Ёж s", &[0, 2, 5]), [0..2, 2..4, 5..6]);
    // Past the end or inside a character: left out.
    assert_eq!(
        highlight_ranges("Ёж", &[1, 4, 100]),
        Vec::<std::ops::Range<usize>>::new()
    );
    assert!(highlight_ranges("", &[]).is_empty());
}
