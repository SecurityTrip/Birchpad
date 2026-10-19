use gpui_kit::{TestAppContext, VisualTestContext};
use serde_json::json;

use super::*;
use crate::workspace::tests::{active_text, document_start, open_workspace, secondary};

fn run(
    workspace: &Entity<Workspace>,
    invocation: Invocation,
    cx: &mut VisualTestContext,
) -> Result<()> {
    let result = workspace.update_in(cx, |workspace, window, cx| {
        workspace.dispatch(&invocation, window, cx)
    });
    cx.run_until_parked();
    result
}

fn recorded(cx: &mut VisualTestContext) -> Vec<Invocation> {
    cx.update(|_, cx| cx.global::<Macros>().current.clone())
}

fn setup(
    cx: &mut TestAppContext,
) -> (Entity<Workspace>, &mut VisualTestContext, tempfile::TempDir) {
    let (workspace, cx) = open_workspace(cx);
    let dir = tempfile::tempdir().unwrap();
    cx.update(|_, cx| {
        cx.global_mut::<AppState>().paths.user_settings = Some(dir.path().join("settings.toml"));
        init(cx);
    });
    (workspace, cx, dir)
}

#[gpui_kit::test]
fn records_typing_and_commands_then_plays_them(cx: &mut TestAppContext) {
    let (workspace, cx, _dir) = setup(cx);
    cx.simulate_keystrokes(&secondary("shift-r"));
    assert!(cx.update(|_, cx| cx.global::<Macros>().is_recording()));
    cx.simulate_input("ab");
    cx.simulate_keystrokes("enter");
    cx.simulate_input("c");
    cx.simulate_keystrokes(&secondary("shift-r"));
    assert!(!cx.update(|_, cx| cx.global::<Macros>().is_recording()));
    let steps = recorded(cx);
    assert_eq!(
        steps.first(),
        Some(&Invocation::with_args("edit.type", json!({ "text": "ab" })))
    );
    assert_eq!(
        steps.last(),
        Some(&Invocation::with_args("edit.type", json!({ "text": "c" })))
    );
    assert!(
        steps
            .iter()
            .all(|step| birchpad_commands::is_recordable(&step.command))
    );
    let once = active_text(&workspace, cx);
    cx.simulate_keystrokes(&secondary("shift-p"));
    let twice = active_text(&workspace, cx);
    assert_eq!(twice.len(), once.len() * 2, "{twice:?}");
    // Playing is not recorded into the macro it plays.
    assert_eq!(recorded(cx), steps);
}

#[gpui_kit::test]
fn runs_several_times_and_until_the_end(cx: &mut TestAppContext) {
    let (workspace, cx, _dir) = setup(cx);
    run(
        &workspace,
        Invocation::with_args("edit.insert-text", json!({ "text": "a\nb\nc\nd" })),
        cx,
    )
    .unwrap();
    run(&workspace, Invocation::new("cursor.document-start"), cx).unwrap();
    let steps = vec![
        Invocation::new("cursor.home"),
        Invocation::with_args("edit.type", json!({ "text": "-" })),
        Invocation::new("cursor.down"),
    ];
    workspace
        .update_in(cx, |workspace, window, cx| {
            play(workspace, &steps, 2, false, window, cx)
        })
        .unwrap();
    assert_eq!(active_text(&workspace, cx), "-a\n-b\nc\nd");
    workspace
        .update_in(cx, |workspace, window, cx| {
            play(workspace, &steps, 1, true, window, cx)
        })
        .unwrap();
    // Until the caret reaches the end of the text, which the step down to "d" does.
    assert_eq!(active_text(&workspace, cx), "-a\n-b\n-c\nd");
}

#[gpui_kit::test]
fn refuses_what_it_cannot_do(cx: &mut TestAppContext) {
    let (workspace, cx, _dir) = setup(cx);
    // Nothing recorded yet.
    assert!(run(&workspace, Invocation::new("macro.playback"), cx).is_err());
    assert!(
        run(
            &workspace,
            Invocation::with_args("macro.run", json!({ "name": "none" })),
            cx
        )
        .is_err()
    );
    let failing = vec![Invocation::new("no.such-command")];
    let error = workspace
        .update_in(cx, |workspace, window, cx| {
            play(workspace, &failing, 3, false, window, cx)
        })
        .unwrap_err();
    assert!(format!("{error:#}").contains("no.such-command"));
    assert!(!cx.update(|_, cx| cx.global::<Macros>().playing));
    // Running while recording.
    run(&workspace, Invocation::new("macro.start-recording"), cx).unwrap();
    assert!(
        run(
            &workspace,
            Invocation::with_args("macro.run", json!({})),
            cx
        )
        .is_err()
    );
    run(&workspace, Invocation::new("macro.stop-recording"), cx).unwrap();
    // An empty recording plays nothing.
    assert!(
        run(
            &workspace,
            Invocation::with_args("macro.run", json!({})),
            cx
        )
        .is_err()
    );
    cx.update(|_, cx| {
        assert!(save("x", vec![], cx).is_err());
        assert!(save("  ", vec![Invocation::new("edit.undo")], cx).is_err());
    });
}

#[gpui_kit::test]
fn saved_macros_are_kept_and_run_by_name(cx: &mut TestAppContext) {
    let (workspace, cx, dir) = setup(cx);
    let steps = vec![Invocation::with_args("edit.type", json!({ "text": "hi" }))];
    cx.update(|_, cx| {
        save("Greet", steps.clone(), cx).unwrap();
        save("Other", vec![Invocation::new("edit.undo")], cx).unwrap();
        // The same name replaces it.
        save("Greet", steps.clone(), cx).unwrap();
        assert_eq!(cx.global::<Macros>().saved.len(), 2);
    });
    let text = std::fs::read_to_string(dir.path().join("macros.toml")).unwrap();
    assert_eq!(birchpad_commands::parse_macros(&text).unwrap().len(), 2);
    // A new start reads them.
    cx.update(|_, cx| {
        cx.set_global(Macros::default());
        init(cx);
        assert_eq!(cx.global::<Macros>().saved[0].name, "Greet");
    });
    run(
        &workspace,
        Invocation::with_args("macro.run", json!({ "name": "Greet", "times": 3 })),
        cx,
    )
    .unwrap();
    assert_eq!(active_text(&workspace, cx), "hihihi");
    // A broken file is reported and leaves no macros.
    std::fs::write(dir.path().join("macros.toml"), "[[macro]").unwrap();
    cx.update(|_, cx| {
        init(cx);
        assert!(cx.global::<Macros>().saved.is_empty());
        cx.global_mut::<AppState>().paths.user_settings = None;
        assert!(save("Greet", vec![Invocation::new("edit.undo")], cx).is_err());
    });
}

#[gpui_kit::test]
fn the_run_multiple_dialog(cx: &mut TestAppContext) {
    let (workspace, cx, _dir) = setup(cx);
    cx.update(|_, cx| {
        cx.global_mut::<Macros>().current =
            vec![Invocation::with_args("edit.type", json!({ "text": "x" }))];
    });
    let weak = workspace.downgrade();
    let dialog = workspace.update_in(cx, |_, window, cx| {
        cx.new(|cx| RunMultiple::new(weak, window, cx))
    });
    dialog.update_in(cx, |this, window, cx| {
        this.times
            .update(cx, |input, cx| input.set_value("4", window, cx));
        assert!(this.run(window, cx));
    });
    assert_eq!(active_text(&workspace, cx), "xxxx");
    dialog.update_in(cx, |this, window, cx| {
        for bad in ["0", "", "many", "-1"] {
            this.times
                .update(cx, |input, cx| input.set_value(bad, window, cx));
            assert!(!this.run(window, cx), "{bad:?}");
            assert!(this.error.is_some());
        }
        this.choice = 5;
        this.until_end = true;
        assert!(!this.run(window, cx));
    });
    run(&workspace, Invocation::new("macro.run-multiple"), cx).unwrap();
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
}

#[gpui_kit::test]
fn trim_trailing_space_and_save(cx: &mut TestAppContext) {
    let (workspace, cx, dir) = setup(cx);
    let path = dir.path().join("a.txt");
    std::fs::write(&path, "a  \nb\t\n").unwrap();
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.open_path(&path, window, cx)
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("alt-shift-s");
    cx.run_until_parked();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "a\nb\n");
    // Untitled: saving asks for a path; cancelled, the text is trimmed all the same.
    cx.simulate_keystrokes(&secondary("n"));
    cx.simulate_input("x ");
    run(&workspace, Invocation::new("macro.trim-and-save"), cx).unwrap();
    cx.simulate_new_path_selection(|_| None);
    cx.run_until_parked();
    assert_eq!(active_text(&workspace, cx), "x");
}

fn find_bar_text(
    workspace: &Entity<Workspace>,
    text: &str,
    replace: Option<&str>,
    cx: &mut VisualTestContext,
) {
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.find_bar.update(cx, |bar, cx| {
            bar.show(
                crate::find::FindTab::Replace,
                Some(text.to_owned()),
                None,
                window,
                cx,
            );
            if let Some(replace) = replace {
                bar.replace_input_for_tests(replace, window, cx);
            }
        });
    });
    cx.run_until_parked();
}

fn bar_event(
    workspace: &Entity<Workspace>,
    event: crate::find::FindBarEvent,
    cx: &mut VisualTestContext,
) {
    workspace.update_in(cx, |workspace, window, cx| {
        let bar = workspace.find_bar.clone();
        workspace.on_find_bar_event(&bar, &event, window, cx);
    });
    cx.run_until_parked();
}

#[gpui_kit::test]
fn searches_are_recorded_with_their_text(cx: &mut TestAppContext) {
    use birchpad_core::search::Direction;
    let (workspace, cx, _dir) = setup(cx);
    cx.simulate_input("one foo two foo three foo");
    cx.simulate_keystrokes(document_start());
    run(&workspace, Invocation::new("macro.start-recording"), cx).unwrap();
    find_bar_text(&workspace, "foo", Some("bar"), cx);
    bar_event(
        &workspace,
        crate::find::FindBarEvent::Find(Direction::Forward),
        cx,
    );
    bar_event(&workspace, crate::find::FindBarEvent::Replace, cx);
    run(&workspace, Invocation::new("macro.stop-recording"), cx).unwrap();
    let steps = recorded(cx);
    let commands: Vec<&str> = steps.iter().map(|step| step.command.as_str()).collect();
    assert_eq!(
        commands,
        ["search.find-text", "search.replace-text"],
        "{steps:?}"
    );
    assert_eq!(steps[0].args["text"], "foo");
    assert_eq!(steps[1].args["with"], "bar");
    assert_eq!(active_text(&workspace, cx), "one bar two foo three foo");

    // Played back from the start with other text in the panel: the recorded search runs.
    find_bar_text(&workspace, "zzz", Some("qqq"), cx);
    workspace.update(cx, |workspace, cx| {
        let view = workspace.active_view(cx).unwrap();
        view.update(cx, |view, cx| view.select_range(0..0, cx));
    });
    run(&workspace, Invocation::new("macro.playback"), cx).unwrap();
    assert_eq!(active_text(&workspace, cx), "one bar two bar three foo");

    // A recorded search that names a mode Birchpad does not know is refused.
    let wrong = Invocation::with_args("search.find-text", json!({ "text": "x", "mode": "fuzzy" }));
    assert!(run(&workspace, wrong, cx).is_err());
    // Replace All, recorded as one step with `all`.
    run(&workspace, Invocation::new("macro.start-recording"), cx).unwrap();
    find_bar_text(&workspace, "foo", Some("X"), cx);
    bar_event(&workspace, crate::find::FindBarEvent::ReplaceAll, cx);
    run(&workspace, Invocation::new("macro.stop-recording"), cx).unwrap();
    let steps = recorded(cx);
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0].args["all"], true);
    assert_eq!(active_text(&workspace, cx), "one bar two bar three X");
}
