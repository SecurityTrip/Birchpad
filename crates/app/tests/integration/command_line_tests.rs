//! What a command line asks besides files and carets: Notepad++'s options of the window, of the
//! files and ghost typing.

use birchpad::workspace::Workspace;
use birchpad_cli::CommandLine;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};

use crate::workspace::{active_text, open_workspace, tab_names};

fn open(
    workspace: &Entity<Workspace>,
    dir: &std::path::Path,
    args: &[&str],
    cx: &mut VisualTestContext,
) -> usize {
    let args = args.iter().map(std::ffi::OsString::from);
    let command_line = CommandLine::parse(args, dir);
    assert!(
        command_line.warnings.is_empty(),
        "{:?}",
        command_line.warnings
    );
    let views = workspace.update_in(cx, |workspace, window, cx| {
        workspace.open_command_line(&command_line, window, cx).len()
    });
    cx.run_until_parked();
    views
}

#[gpui_kit::test]
fn wildcards_open_every_match(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "a").unwrap();
    std::fs::write(dir.path().join("b.txt"), "b").unwrap();
    std::fs::write(dir.path().join("c.md"), "c").unwrap();
    std::fs::create_dir(dir.path().join("sub")).unwrap();
    std::fs::write(dir.path().join("sub").join("d.txt"), "d").unwrap();
    let (workspace, cx) = open_workspace(cx);
    assert_eq!(open(&workspace, dir.path(), &["*.txt"], cx), 2);
    assert_eq!(tab_names(&workspace, cx), ["a.txt", "b.txt"]);
    // -r: the folders below too.
    assert_eq!(open(&workspace, dir.path(), &["-r", "*.txt"], cx), 3);
    assert!(tab_names(&workspace, cx).contains(&"d.txt".to_owned()));
}

#[gpui_kit::test]
fn files_open_in_a_user_defined_language_and_monitored(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("log.txt");
    std::fs::write(&file, "start\n").unwrap();
    let (workspace, cx) = open_workspace(cx);
    open(&workspace, dir.path(), &["-monitor", "log.txt"], cx);
    let monitoring = workspace.read_with(cx, |workspace, cx| {
        let view = workspace.active_view(cx).unwrap();
        view.read(cx).buffer.read(cx).is_monitoring()
    });
    assert!(monitoring);
    // Opened again with -monitor: it stays on.
    open(&workspace, dir.path(), &["-monitor", "log.txt"], cx);
    let still = workspace.read_with(cx, |workspace, cx| {
        let view = workspace.active_view(cx).unwrap();
        view.read(cx).buffer.read(cx).is_monitoring()
    });
    assert!(still);
    // A user defined language the user does not have: said, and the file opens as it is.
    birchpad_ui::take_warnings();
    open(
        &workspace,
        dir.path(),
        &["-udl=Nothing Such", "log.txt"],
        cx,
    );
    let warnings = birchpad_ui::take_warnings();
    assert!(
        warnings
            .iter()
            .any(|warning| warning.contains("Nothing Such")),
        "{warnings:?}"
    );
}

#[gpui_kit::test]
fn loading_time_is_said_when_asked(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "a").unwrap();
    let (workspace, cx) = open_workspace(cx);
    birchpad_ui::take_info();
    open(&workspace, dir.path(), &["a.txt"], cx);
    assert!(birchpad_ui::take_info().is_empty());
    open(&workspace, dir.path(), &["-loadingTime", "a.txt"], cx);
    let info = birchpad_ui::take_info();
    assert!(
        info.iter()
            .any(|message| message.starts_with("The files opened in ")),
        "{info:?}"
    );
    // Nothing opened: nothing to say.
    open(&workspace, dir.path(), &["-loadingTime"], cx);
    assert!(birchpad_ui::take_info().is_empty());
}

#[gpui_kit::test]
fn ghost_typing_types_the_text_or_the_file(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("quote.txt"), "from a file").unwrap();
    let (workspace, cx) = open_workspace(cx);
    let tabs = tab_names(&workspace, cx).len();
    open(
        &workspace,
        dir.path(),
        &["-qt=Hello, Birchpad", "-qSpeed3"],
        cx,
    );
    assert_eq!(tab_names(&workspace, cx).len(), tabs + 1);
    assert_eq!(active_text(&workspace, cx), "Hello, Birchpad");
    open(&workspace, dir.path(), &["-qf=quote.txt", "-qSpeed3"], cx);
    assert_eq!(active_text(&workspace, cx), "from a file");
    // A slow speed types it all the same, in time.
    open(&workspace, dir.path(), &["-qt=ab", "-qSpeed1"], cx);
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(500));
    cx.run_until_parked();
    assert_eq!(active_text(&workspace, cx), "ab");
    // Notepad++'s easter eggs are its own: said, and nothing typed.
    let before = tab_names(&workspace, cx).len();
    birchpad_ui::take_warnings();
    open(&workspace, dir.path(), &["-qn=Joel Spolsky"], cx);
    assert_eq!(tab_names(&workspace, cx).len(), before);
    assert_eq!(birchpad_ui::take_warnings().len(), 1);
    // A file that cannot be read: nothing typed either.
    open(&workspace, dir.path(), &["-qf=missing.txt"], cx);
    assert_eq!(tab_names(&workspace, cx).len(), before);
}

#[gpui_kit::test]
fn quick_print_prints_each_file_without_asking(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "a").unwrap();
    std::fs::write(dir.path().join("b.txt"), "b").unwrap();
    let (workspace, cx) = open_workspace(cx);
    birchpad::print::PRINTED.with(|printed| printed.borrow_mut().clear());
    open(
        &workspace,
        dir.path(),
        &["-quickPrint", "a.txt", "b.txt"],
        cx,
    );
    let printed = birchpad::print::PRINTED.with(|printed| printed.borrow().clone());
    assert_eq!(
        printed,
        [
            ("a.txt".to_owned(), 1, false),
            ("b.txt".to_owned(), 1, false)
        ]
    );
    let pending = workspace.update(cx, |workspace, _| {
        std::mem::take(&mut workspace.quick_prints).len()
    });
    assert_eq!(pending, 2, "a launch for it waits for them before quitting");
}

#[gpui_kit::test]
fn the_title_and_the_tab_bar(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    workspace.update(cx, |workspace, cx| {
        workspace.title_add = Some("Work".into());
        assert_eq!(workspace.window_title(cx), "new 1 - Birchpad - Work");
        workspace.title_add = Some(String::new());
        assert_eq!(workspace.window_title(cx), "new 1 - Birchpad");
        workspace.modes.no_tab_bar = true;
        workspace.show_tabs(cx);
    });
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("pane-0-tab-bar").is_none(),
        "-notabbar hides the tabs"
    );
    workspace.update(cx, |workspace, cx| {
        workspace.modes.no_tab_bar = false;
        workspace.show_tabs(cx);
    });
}

#[gpui_kit::test]
fn a_message_for_plugins_is_accepted(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (workspace, cx) = open_workspace(cx);
    assert_eq!(
        open(&workspace, dir.path(), &["-pluginMessage=hello"], cx),
        0
    );
}
