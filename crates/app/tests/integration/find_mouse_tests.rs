//! The find panel and the Go To dialog with the mouse: tabs, buttons, check boxes and search
//! modes clicked where they are drawn, as a user clicks them.

use std::fs;
use std::time::Duration;

use gpui_kit::{
    Entity, Modifiers, MouseButton, Pixels, Point, TestAppContext, VisualTestContext, point, px,
};

use crate::workspace::{active_text, click_on, document_start, open_workspace, secondary};
use birchpad::app_state::AppState;
use birchpad::find::*;
use birchpad::workspace::Workspace;
use birchpad_core::search::SearchMode;
use gpui_kit::component::WindowExt as _;

#[track_caller]
fn click(selector: &'static str, cx: &mut VisualTestContext) {
    click_on(selector, MouseButton::Left, cx);
}

fn drawn(selector: &'static str, cx: &mut VisualTestContext) -> bool {
    cx.debug_bounds(selector).is_some()
}

/// Types `text`, goes back to its start and opens the find panel (Ctrl+F) on `pattern`.
fn find_in(text: &str, pattern: &str, cx: &mut VisualTestContext) {
    cx.simulate_input(text);
    cx.simulate_keystrokes(document_start());
    search_for(pattern, cx);
}

/// Ctrl+F, then `pattern` over whatever the find field had.
fn search_for(pattern: &str, cx: &mut VisualTestContext) {
    cx.simulate_keystrokes(&secondary("f"));
    if pattern.is_empty() {
        cx.simulate_keystrokes("backspace");
    } else {
        cx.simulate_input(pattern);
    }
}

fn bar<R>(
    workspace: &Entity<Workspace>,
    read: impl FnOnce(&FindBar) -> R,
    cx: &mut VisualTestContext,
) -> R {
    workspace.read_with(cx, |workspace, cx| read(workspace.find_bar.read(cx)))
}

fn status(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Option<String> {
    bar(workspace, FindBar::status_text, cx)
}

fn selection(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> (usize, usize) {
    workspace.read_with(cx, |workspace, cx| {
        let range = workspace
            .active_view(cx)
            .unwrap()
            .read(cx)
            .selection
            .primary();
        (range.anchor, range.head)
    })
}

fn result_rows(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Vec<String> {
    workspace.read_with(cx, |workspace, cx| {
        workspace.search_results.read(cx).rows_text()
    })
}

#[gpui_kit::test]
fn the_buttons_find_count_and_list_matches(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    find_in("cat Cat category\ncat", "cat", cx);
    assert!(bar(&workspace, |bar| bar.visible, cx));

    click("find-next", cx);
    assert_eq!(selection(&workspace, cx), (0, 3));
    click("find-next", cx);
    assert_eq!(selection(&workspace, cx), (4, 7));
    click("find-previous", cx);
    assert_eq!(selection(&workspace, cx), (0, 3));
    click("count", cx);
    assert_eq!(
        status(&workspace, cx).as_deref(),
        Some("Count: 4 matches in entire file")
    );
    click("find-all", cx);
    assert_eq!(
        result_rows(&workspace, cx).first().map(String::as_str),
        Some("Search \"cat\" (4 hits in 1 file of 1 searched)")
    );
    click("find-all-in-all", cx);
    assert_eq!(result_rows(&workspace, cx).len(), 2 * 4, "two searches");

    // Text that is not there: the selection stays, and the panel says so.
    search_for("dog", cx);
    click("find-next", cx);
    assert_eq!(selection(&workspace, cx), (0, 3));
    assert_eq!(
        status(&workspace, cx).as_deref(),
        Some("Can't find the text \"dog\"")
    );

    // No text at all finds nothing and changes nothing.
    search_for("", cx);
    click("find-next", cx);
    click("count", cx);
    assert_eq!(selection(&workspace, cx), (0, 3));
    assert_eq!(active_text(&workspace, cx), "cat Cat category\ncat");
}

#[gpui_kit::test]
fn the_check_boxes_and_modes_change_the_search(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    find_in("cat Cat category\ncat", "cat", cx);
    let options = |cx: &mut VisualTestContext| bar(&workspace, |bar| bar.options, cx);
    let count = |cx: &mut VisualTestContext| {
        click("count", cx);
        status(&workspace, cx).unwrap()
    };

    click("match-case", cx);
    assert!(options(cx).match_case);
    assert_eq!(count(cx), "Count: 3 matches in entire file");
    click("whole-word", cx);
    assert!(options(cx).whole_word);
    assert_eq!(count(cx), "Count: 2 matches in entire file");
    // A second click clears the box again.
    click("match-case", cx);
    assert!(!options(cx).match_case);
    assert_eq!(count(cx), "Count: 3 matches in entire file");
    click("whole-word", cx);
    assert_eq!(count(cx), "Count: 4 matches in entire file");

    // ". matches newline" is for regular expressions only: in Normal mode it ignores clicks.
    assert_eq!(options(cx).mode, SearchMode::Normal);
    click("dot-newline", cx);
    assert!(!options(cx).dot_matches_newline);
    click("mode-regex", cx);
    assert_eq!(options(cx).mode, SearchMode::Regex);
    click("dot-newline", cx);
    assert!(options(cx).dot_matches_newline);
    // Whole word does not apply to a regular expression: its box ignores clicks.
    click("whole-word", cx);
    assert!(!options(cx).whole_word);
    search_for("c.t", cx);
    assert_eq!(count(cx), "Count: 4 matches in entire file");

    click("mode-extended", cx);
    assert_eq!(options(cx).mode, SearchMode::Extended);
    search_for("\\ncat", cx);
    assert_eq!(count(cx), "Count: 1 match in entire file");
    click("mode-normal", cx);
    assert_eq!(options(cx).mode, SearchMode::Normal);
    assert_eq!(count(cx), "Count: 0 matches in entire file");
}

#[gpui_kit::test]
fn the_tabs_switch_and_the_close_button_hides_the_panel(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    find_in("a-b-c\nb", "-", cx);
    assert!(drawn("count", cx) && !drawn("replace-all", cx));

    click("tab-replace", cx);
    assert_eq!(bar(&workspace, |bar| bar.tab, cx), FindTab::Replace);
    assert!(drawn("replace-all", cx) && !drawn("count", cx));
    workspace.update_in(cx, |workspace, window, cx| {
        workspace
            .find_bar
            .update(cx, |bar, cx| bar.replace_input_for_tests("+", window, cx));
    });
    click("replace-all", cx);
    assert_eq!(active_text(&workspace, cx), "a+b+c\nb");
    assert_eq!(
        status(&workspace, cx).as_deref(),
        Some("Replace All: 2 occurrences replaced in entire file")
    );
    // Nothing left to replace: the text stays as it is.
    click("replace-all", cx);
    assert_eq!(active_text(&workspace, cx), "a+b+c\nb");

    search_for("b", cx);
    click("tab-mark", cx);
    assert_eq!(bar(&workspace, |bar| bar.tab, cx), FindTab::Mark);
    click("bookmark-line", cx);
    click("mark-all", cx);
    assert_eq!(
        status(&workspace, cx).as_deref(),
        Some("Mark: 2 matches in entire file")
    );
    let marks = |cx: &mut VisualTestContext| {
        workspace.read_with(cx, |workspace, cx| {
            let view = workspace.active_view(cx).unwrap();
            let buffer = view.read(cx).buffer.read(cx);
            (buffer.marks().found.len(), buffer.bookmark_lines())
        })
    };
    assert_eq!(marks(cx), (2, vec![0, 1]));
    click("clear-marks", cx);
    assert_eq!(marks(cx).0, 0);
    // The tab already shown stays.
    click("tab-mark", cx);
    assert_eq!(bar(&workspace, |bar| bar.tab, cx), FindTab::Mark);

    click("close-find", cx);
    assert!(!bar(&workspace, |bar| bar.visible, cx));
    assert!(!drawn("mark-all", cx) && !drawn("close-find", cx));
}

#[gpui_kit::test]
fn the_find_in_files_tab_searches_the_folder_its_boxes_choose(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.txt"), "needle").unwrap();
    fs::create_dir(dir.path().join("sub")).unwrap();
    fs::write(dir.path().join("sub").join("b.txt"), "needle needle").unwrap();
    let (workspace, cx) = open_workspace(cx);
    search_for("needle", cx);
    click("tab-find-in-files", cx);
    assert!(drawn("subfolders", cx) && drawn("hidden-folders", cx) && !drawn("count", cx));

    // Without a folder, nothing is searched.
    click("find-in-files", cx);
    assert_eq!(
        status(&workspace, cx).as_deref(),
        Some("Find in Files: choose a folder")
    );
    assert!(result_rows(&workspace, cx).is_empty());

    workspace.update_in(cx, |workspace, window, cx| {
        workspace
            .find_bar
            .update(cx, |bar, cx| bar.set_directory(dir.path(), window, cx));
    });
    click("find-in-files", cx);
    cx.run_until_parked();
    assert_eq!(
        status(&workspace, cx).as_deref(),
        Some("Find in Files: 3 hits in 2 files of 2 searched")
    );

    // In all sub-folders, cleared: only the folder itself.
    click("subfolders", cx);
    assert!(!bar(&workspace, |bar| bar.folder.subfolders, cx));
    click("find-in-files", cx);
    cx.run_until_parked();
    assert_eq!(
        status(&workspace, cx).as_deref(),
        Some("Find in Files: 1 hit in 1 file of 1 searched")
    );
    click("hidden-folders", cx);
    click("follow-current", cx);
    let folder = bar(&workspace, |bar| bar.folder, cx);
    assert!(folder.hidden && folder.follow_current_document && !folder.subfolders);
}

#[gpui_kit::test]
fn the_go_to_dialog_goes_on_a_click_and_cancel_leaves_the_caret(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("one\ntwo\nthree");
    cx.simulate_keystrokes(document_start());
    let open = |cx: &mut VisualTestContext| cx.update(|window, cx| window.has_active_dialog(cx));
    // A dialog closed by a click gives the focus back once it has faded out.
    let closed = |cx: &mut VisualTestContext| {
        assert!(!open(cx));
        cx.executor().advance_clock(Duration::from_secs(1));
        cx.run_until_parked();
        cx.update(|window, cx| {
            let view = workspace.read(cx).active_view(cx).unwrap();
            assert!(view.read(cx).focus_handle.is_focused(window));
        });
    };

    cx.simulate_keystrokes(&secondary("g"));
    assert!(open(cx));
    cx.simulate_input("2");
    click("dialog-action", cx);
    closed(cx);
    assert_eq!(selection(&workspace, cx), (4, 4));

    // Not a line number: the dialog stays.
    cx.simulate_keystrokes(&secondary("g"));
    cx.simulate_input("two");
    click("dialog-action", cx);
    assert!(open(cx));
    // Line 0 is before the first line.
    cx.simulate_keystrokes(&format!("{} backspace", secondary("a")));
    cx.simulate_input("0");
    click("dialog-action", cx);
    assert!(open(cx));
    click("dialog-close", cx);
    closed(cx);
    assert_eq!(selection(&workspace, cx), (4, 4));

    // Past the last line: the start of the last line.
    cx.simulate_keystrokes(&secondary("g"));
    cx.simulate_input("99");
    click("dialog-action", cx);
    closed(cx);
    assert_eq!(selection(&workspace, cx), (8, 8));
}

/// The settings written to a file of the test's own, so that the panel's button can change them.
fn with_settings_file(cx: &mut VisualTestContext) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.toml");
    fs::write(&path, "").unwrap();
    cx.update(|_, cx| cx.global_mut::<AppState>().paths.user_settings = Some(path));
    dir
}

fn place(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> FloatingPlace {
    bar(workspace, |bar| bar.place, cx)
}

/// Drags the floating panel's title by `by`, in two moves as a hand does.
fn drag_title(by: Point<Pixels>, cx: &mut VisualTestContext) {
    let from = cx.debug_bounds("find-panel-title").unwrap().center();
    let half = point(by.x / 2., by.y / 2.);
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(from + half, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(from + by, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(from + by, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
}

#[gpui_kit::test]
fn the_find_panel_floats_and_is_moved_by_its_title(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    find_in("cat Cat category\ncat", "cat", cx);

    // Floating, as in Notepad++: over the documents, in their top right corner, with a title.
    assert!(cx.update(|_, cx| FindBar::floats(cx)));
    assert!(drawn("find-panel-title", cx));
    assert_eq!(place(&workspace, cx), FloatingPlace::default());
    let panel = cx.debug_bounds("find-bar").unwrap();
    let documents = cx.debug_bounds("pane-content-0").unwrap();
    assert!(panel.top() < documents.bottom() && panel.right() <= documents.right());

    // Dragged by its title, it follows the mouse; its buttons still work where it went.
    let title = cx.debug_bounds("find-panel-title").unwrap();
    drag_title(point(px(-200.), px(80.)), cx);
    assert_eq!(place(&workspace, cx), {
        let start = FloatingPlace::default();
        FloatingPlace {
            right: start.right + px(200.),
            top: start.top + px(80.),
        }
    });
    let moved = cx.debug_bounds("find-panel-title").unwrap();
    assert_eq!(moved.origin, title.origin + point(px(-200.), px(80.)));
    click("find-next", cx);
    assert_eq!(selection(&workspace, cx), (0, 3));
    click("count", cx);
    assert_eq!(
        status(&workspace, cx).as_deref(),
        Some("Count: 4 matches in entire file")
    );

    // Dragged out of the window, it stays in the corner it went towards.
    drag_title(point(px(5000.), px(-5000.)), cx);
    let corner = place(&workspace, cx);
    assert_eq!((corner.right, corner.top), (px(0.), px(0.)));
    assert!(drawn("find-panel-title", cx));

    // Clicking the title without moving the mouse leaves the panel where it is.
    click("find-panel-title", cx);
    assert_eq!(place(&workspace, cx), corner);
    // Closed and opened again: where it was put.
    click("close-find", cx);
    assert!(!drawn("find-panel-title", cx));
    search_for("cat", cx);
    assert_eq!(place(&workspace, cx), corner);
}

#[gpui_kit::test]
fn the_find_panel_s_button_docks_and_floats_it(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let dir = with_settings_file(cx);
    find_in("cat Cat category\ncat", "cat", cx);
    let floats = |cx: &mut VisualTestContext| cx.update(|_, cx| FindBar::floats(cx));

    // Docked: under the documents, without a title, and kept in the settings.
    click("float-find", cx);
    assert!(!floats(cx));
    assert!(!drawn("find-panel-title", cx));
    let panel = cx.debug_bounds("find-bar").unwrap();
    let documents = cx.debug_bounds("pane-content-0").unwrap();
    assert!(panel.top() >= documents.bottom(), "{panel:?} {documents:?}");
    let settings = fs::read_to_string(dir.path().join("settings.toml")).unwrap();
    assert!(settings.contains("find-panel-floats = false"), "{settings}");
    click("find-next", cx);
    assert_eq!(selection(&workspace, cx), (0, 3));

    // And floating again, where it was.
    click("float-find", cx);
    assert!(floats(cx));
    assert!(drawn("find-panel-title", cx));
    assert_eq!(place(&workspace, cx), FloatingPlace::default());
    let settings = fs::read_to_string(dir.path().join("settings.toml")).unwrap();
    assert!(settings.contains("find-panel-floats = true"), "{settings}");
}

#[gpui_kit::test]
fn without_a_settings_file_the_find_panel_says_why_it_stays(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.update(|_, cx| {
        let paths = &mut cx.global_mut::<AppState>().paths;
        paths.user_settings = None;
    });
    find_in("cat", "cat", cx);
    click("float-find", cx);
    assert!(cx.update(|_, cx| FindBar::floats(cx)), "nothing changed");
    assert!(drawn("find-panel-title", cx));
    assert!(
        bar(
            &workspace,
            |bar| bar.status.as_ref().is_some_and(|(_, failed)| *failed),
            cx
        ),
        "{:?}",
        status(&workspace, cx)
    );
}
