use gpui_kit::{TestAppContext, VisualTestContext};
use serde_json::json;

use super::*;
use crate::workspace::tests::{active_text, open_workspace, secondary};

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
