use birchpad_commands::Invocation;
use gpui_kit::{TestAppContext, VisualTestContext};
use serde_json::json;

use super::*;
use crate::workspace::tests::open_workspace;

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

#[test]
fn urls_and_shell_lines() {
    for line in ["https://example.com", "  HTTP://x", "https://"] {
        assert!(is_url(line), "{line}");
    }
    for line in ["", "notepad.exe", "ftp://x", "start https://x"] {
        assert!(!is_url(line), "{line}");
    }
    let command = shell_command("echo hi");
    let args: Vec<_> = command
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    if cfg!(windows) {
        assert_eq!(command.get_program(), "cmd");
        assert_eq!(args, ["/C", "echo hi"]);
    } else {
        assert_eq!(command.get_program(), "sh");
        assert_eq!(args, ["-c", "echo hi"]);
    }
}

#[gpui_kit::test]
fn variables_come_from_the_active_document(cx: &mut TestAppContext) {
    let (workspace, cx, _dir) = setup(cx);
    run(
        &workspace,
        Invocation::with_args("edit.insert-text", json!({ "text": "one\ntwo birch" })),
        cx,
    )
    .unwrap();
    let vars = workspace.read_with(cx, variables);
    assert_eq!(vars.current_line, 2);
    assert_eq!(vars.current_column, 10);
    assert_eq!(vars.current_line_text, "two birch");
    assert_eq!(vars.current_word, "birch");
    assert_eq!(vars.full_current_path, "");
    assert_eq!(vars.file_name, "new 1");
    assert!(!vars.executable.is_empty());
    // A selection on one line is the word; one over lines is none.
    run(&workspace, Invocation::new("edit.select-all"), cx).unwrap();
    let vars = workspace.read_with(cx, variables);
    assert_eq!(vars.current_word, "");
}

#[gpui_kit::test]
fn runs_and_saves_command_lines(cx: &mut TestAppContext) {
    let (workspace, cx, dir) = setup(cx);
    // The defaults, when there is no run.toml.
    let names: Vec<String> = cx.update(|_, cx| {
        cx.global::<RunMenu>()
            .saved
            .iter()
            .map(|e| e.name.clone())
            .collect()
    });
    assert!(
        names.is_empty(),
        "the built-in commands are not saved ones: {names:?}"
    );
    let marker = dir.path().join("ran.txt");
    let line = if cfg!(windows) {
        format!("echo line $(CURRENT_LINE) > \"{}\"", marker.display())
    } else {
        format!("echo line $(CURRENT_LINE) > '{}'", marker.display())
    };
    run(
        &workspace,
        Invocation::with_args("run.run", json!({ "line": line })),
        cx,
    )
    .unwrap();
    for _ in 0..200 {
        if std::fs::read(&marker).is_ok_and(|bytes| !bytes.is_empty()) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    let written = String::from_utf8_lossy(&std::fs::read(&marker).unwrap()).into_owned();
    assert_eq!(written.trim(), "line 1", "{written:?}");
    cx.update(|_, cx| {
        save("Mine", "echo mine", cx).unwrap();
        save("Mine", "echo again", cx).unwrap();
        assert!(save("", "echo", cx).is_err());
        assert!(save("Empty", "  ", cx).is_err());
    });
    let text = std::fs::read_to_string(dir.path().join("run.toml")).unwrap();
    let saved = birchpad_commands::parse_run_commands(&text).unwrap();
    assert_eq!(saved.last().unwrap().line, "echo again");
    assert_eq!(saved.len(), 3);
    // Unknown names and empty lines are refused.
    assert!(
        run(
            &workspace,
            Invocation::with_args("run.run", json!({ "name": "nope" })),
            cx
        )
        .is_err()
    );
    assert!(
        run(
            &workspace,
            Invocation::with_args("run.run", json!({ "line": " " })),
            cx
        )
        .is_err()
    );
    // A new start reads run.toml; a broken one leaves no commands.
    std::fs::write(dir.path().join("run.toml"), "[[command]").unwrap();
    cx.update(|_, cx| {
        init(cx);
        assert!(cx.global::<RunMenu>().saved.is_empty());
    });
}

#[gpui_kit::test]
fn the_dialog(cx: &mut TestAppContext) {
    let (workspace, cx, dir) = setup(cx);
    run(&workspace, Invocation::new("run.run"), cx).unwrap();
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    let weak = workspace.downgrade();
    let dialog = workspace.update_in(cx, |_, window, cx| {
        cx.new(|cx| RunDialog::new(weak, window, cx))
    });
    dialog.update_in(cx, |this, window, cx| {
        // Nothing to run or save.
        assert!(!this.run(cx));
        assert!(this.error.is_some());
        this.line
            .update(cx, |input, cx| input.set_value("echo x", window, cx));
        this.save(cx);
        assert!(this.error.is_some(), "no name");
        this.name
            .update(cx, |input, cx| input.set_value("Echo", window, cx));
        this.save(cx);
        assert!(this.error.is_none(), "{:?}", this.error);
    });
    assert!(
        std::fs::read_to_string(dir.path().join("run.toml"))
            .unwrap()
            .contains("Echo")
    );
}

#[gpui_kit::test]
fn notepad_plus_plus_built_in_commands(cx: &mut TestAppContext) {
    let (workspace, cx, _dir) = setup(cx);
    let builtin =
        |name: &str| Invocation::with_args("run.builtin", serde_json::json!({ "name": name }));
    cx.simulate_input("rust");
    run(&workspace, builtin("Wikipedia Search"), cx).unwrap();
    let error = run(&workspace, builtin("Launch in Netscape"), cx).unwrap_err();
    assert!(error.to_string().contains("Launch in Netscape"), "{error}");
    assert!(run(&workspace, Invocation::new("run.builtin"), cx).is_err());
    // They are in the Run menu, before the saved ones.
    let menu = birchpad_commands::main_menu()
        .into_iter()
        .find(|menu| menu.title == "Run")
        .unwrap();
    let builtins = menu
        .invocations()
        .into_iter()
        .filter(|invocation| invocation.command == "run.builtin")
        .count();
    assert_eq!(builtins, birchpad_commands::builtin_run_commands().len());
}
