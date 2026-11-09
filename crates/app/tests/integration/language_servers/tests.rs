//! Tools > Language Server (ADR 0065), against scripted servers in the same process.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use anyhow::Result;
use birchpad::app_state::AppState;
use birchpad::editor::{DiagnosticLevel, EditorView};
use birchpad::language_servers::*;
use birchpad::workspace::Workspace;
use birchpad_commands::Invocation;
use birchpad_lsp::fake::{FakeHandle, FakeServer};
use birchpad_lsp::message::ResponseError;
use birchpad_ui::{take_errors, take_info, take_warnings};
use gpui_kit::component::WindowExt as _;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::{Value, json};

use crate::workspace::{active_text, open_workspace};

/// What a scripted server can do.
fn capabilities() -> Value {
    json!({
        "textDocumentSync": { "openClose": true, "change": 1, "save": { "includeText": false } },
        "completionProvider": { "triggerCharacters": ["."] },
        "hoverProvider": true,
        "definitionProvider": true,
        "referencesProvider": true,
        "renameProvider": true,
        "documentFormattingProvider": true,
    })
}

/// The servers started, and the command lines and folders they were started with.
#[derive(Default)]
struct Started {
    handles: Vec<FakeHandle>,
    commands: Vec<(String, Vec<String>, PathBuf)>,
}

type Script = Rc<dyn Fn() -> FakeServer>;

struct Setup {
    workspace: Entity<Workspace>,
    dir: tempfile::TempDir,
    started: Rc<RefCell<Started>>,
}

/// A window whose Rust documents get the server `script` makes, in a folder with `.git`.
fn setup(
    script: impl Fn() -> FakeServer + 'static,
    cx: &mut TestAppContext,
) -> (Setup, &mut VisualTestContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.executor().allow_parking();
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join(".git")).unwrap();
    let started: Rc<RefCell<Started>> = Rc::default();
    let script: Script = Rc::new(script);
    {
        let started = started.clone();
        cx.update(|_, cx| {
            cx.global_mut::<AppState>()
                .settings
                .lsp
                .servers
                .insert("rust".into(), "fake-rust-server --stdio".into());
            cx.global_mut::<AppState>().paths.user_settings =
                Some(dir.path().join("settings.toml"));
            set_launcher(
                Rc::new(move |command, root| {
                    let (transport, handle) = script().start();
                    let mut started = started.borrow_mut();
                    started.handles.push(handle);
                    started.commands.push((
                        command.program.clone(),
                        command.args.clone(),
                        root.to_owned(),
                    ));
                    Ok(transport)
                }),
                cx,
            );
        });
    }
    (
        Setup {
            workspace,
            dir,
            started,
        },
        cx,
    )
}

fn server(cx: &mut TestAppContext) -> (Setup, &mut VisualTestContext) {
    setup(|| FakeServer::new(capabilities()), cx)
}

/// Lets time pass, the servers' threads answer and the window take it, until `done`.
fn wait_for(
    cx: &mut VisualTestContext,
    mut done: impl FnMut(&mut VisualTestContext) -> bool,
) -> bool {
    for _ in 0..300 {
        cx.run_until_parked();
        if done(cx) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(5));
        cx.executor().advance_clock(Duration::from_millis(50));
    }
    cx.run_until_parked();
    done(cx)
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

fn insert(workspace: &Entity<Workspace>, text: &str, cx: &mut VisualTestContext) {
    run(
        workspace,
        Invocation::with_args("edit.insert-text", json!({ "text": text })),
        cx,
    )
    .unwrap();
}

fn open(workspace: &Entity<Workspace>, path: &Path, cx: &mut VisualTestContext) {
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.open_path(path, window, cx)
    });
    cx.run_until_parked();
}

fn active_view(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Entity<EditorView> {
    workspace.read_with(cx, |workspace, cx| workspace.active_view(cx).unwrap())
}

fn handle(setup: &Setup, index: usize) -> FakeHandle {
    setup.started.borrow().handles[index].clone()
}

fn launches(setup: &Setup) -> usize {
    setup.started.borrow().handles.len()
}

/// A Rust file in the setup's folder, opened, its server told.
fn open_rust(setup: &Setup, name: &str, text: &str, cx: &mut VisualTestContext) -> PathBuf {
    let path = setup.dir.path().join(name);
    std::fs::write(&path, text).unwrap();
    let before = setup
        .started
        .borrow()
        .handles
        .iter()
        .map(|handle| handle.notifications("textDocument/didOpen").len())
        .sum::<usize>();
    open(&setup.workspace, &path, cx);
    assert!(
        wait_for(cx, |_| {
            setup
                .started
                .borrow()
                .handles
                .iter()
                .map(|handle| handle.notifications("textDocument/didOpen").len())
                .sum::<usize>()
                > before
        }),
        "{name} was opened in the server"
    );
    path
}

fn uri(path: &Path) -> String {
    birchpad_lsp::uri::file_uri_text(&std::fs::canonicalize(path).unwrap_or(path.to_owned()))
        .unwrap()
}

/// The URI the server was told of the document `path`.
fn told_uri(handle: &FakeHandle, path: &Path) -> String {
    let name = path.file_name().unwrap().to_str().unwrap();
    handle
        .notifications("textDocument/didOpen")
        .iter()
        .map(|params| params["textDocument"]["uri"].as_str().unwrap().to_owned())
        .find(|told| told.ends_with(name))
        .unwrap_or_else(|| uri(path))
}

fn range(a: (u32, u32), b: (u32, u32)) -> Value {
    json!({ "start": { "line": a.0, "character": a.1 }, "end": { "line": b.0, "character": b.1 } })
}

fn selected(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> String {
    let view = active_view(workspace, cx);
    view.read_with(cx, |view, cx| {
        let primary = view.selection.primary();
        view.text(cx)
            .slice(primary.from()..primary.to())
            .to_string()
    })
}

fn tip(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Option<String> {
    let view = active_view(workspace, cx);
    view.read_with(cx, |view, _| view.call_tip.as_ref().map(|tip| tip.text()))
}

fn result_rows(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Vec<String> {
    workspace.read_with(cx, |workspace, cx| {
        workspace.search_results.read(cx).rows_text()
    })
}

#[gpui_kit::test]
fn a_document_of_a_language_with_a_server_is_opened_in_it(cx: &mut TestAppContext) {
    let (setup, cx) = server(cx);
    let path = open_rust(&setup, "main.rs", "fn main() {}\n", cx);
    let handle = handle(&setup, 0);
    let opened = &handle.notifications("textDocument/didOpen")[0]["textDocument"];
    assert_eq!(opened["languageId"], "rust");
    assert_eq!(opened["version"], 1);
    assert_eq!(opened["text"], "fn main() {}\n");
    assert!(opened["uri"].as_str().unwrap().ends_with("/main.rs"));
    let (program, args, root) = setup.started.borrow().commands[0].clone();
    assert_eq!(program, "fake-rust-server");
    assert_eq!(args, ["--stdio"]);
    assert_eq!(
        std::fs::canonicalize(root).unwrap(),
        std::fs::canonicalize(setup.dir.path()).unwrap()
    );
    // Initialized with the root, before the document was opened.
    let initialize = &handle.requests("initialize")[0];
    assert!(
        initialize["rootUri"]
            .as_str()
            .unwrap()
            .starts_with("file:///")
    );
    assert_eq!(handle.notifications("initialized").len(), 1);
    let keys = cx.update(|_, cx| server_keys(cx));
    assert_eq!(keys.len(), 1);
    assert_eq!(keys[0].language, "rust");
    assert!(matches!(
        cx.update(|_, cx| server_state(&keys[0], cx)),
        Some(ServerState::Ready(_))
    ));
    // A second document of the same language and folder goes to the same server.
    open_rust(&setup, "lib.rs", "pub fn f() {}\n", cx);
    assert_eq!(launches(&setup), 1);
    assert_eq!(handle.notifications("textDocument/didOpen").len(), 2);
    let _ = path;
}

#[gpui_kit::test]
fn documents_without_a_server_get_none(cx: &mut TestAppContext) {
    let (setup, cx) = server(cx);
    // Untitled, plain text, and a language without a server.
    insert(&setup.workspace, "fn untitled() {}", cx);
    for (name, text) in [("notes.txt", "text"), ("script.py", "print(1)\n")] {
        let path = setup.dir.path().join(name);
        std::fs::write(&path, text).unwrap();
        open(&setup.workspace, &path, cx);
    }
    wait_for(cx, |_| false);
    assert_eq!(launches(&setup), 0);
    // Servers turned off: a Rust file gets none either.
    cx.update(|_, cx| cx.global_mut::<AppState>().settings.lsp.enabled = false);
    let path = setup.dir.path().join("main.rs");
    std::fs::write(&path, "fn main() {}").unwrap();
    open(&setup.workspace, &path, cx);
    wait_for(cx, |_| false);
    assert_eq!(launches(&setup), 0);
    // Asking it says why.
    let error = run(&setup.workspace, Invocation::new("lsp.hover"), cx).unwrap_err();
    assert!(error.to_string().contains("turned off"), "{error}");
    cx.update(|_, cx| cx.global_mut::<AppState>().settings.lsp.enabled = true);
    let error = run(&setup.workspace, Invocation::new("lsp.definition"), cx).unwrap_err();
    assert!(
        error.to_string().contains("no language server for Rust"),
        "{error}"
    );
}

#[gpui_kit::test]
fn edits_are_sent_after_a_pause_and_saves_are_told(cx: &mut TestAppContext) {
    let (setup, cx) = server(cx);
    let path = open_rust(&setup, "main.rs", "fn main() {}\n", cx);
    let handle = handle(&setup, 0);
    // Three edits in a row: one change, with the whole text.
    insert(&setup.workspace, "a", cx);
    insert(&setup.workspace, "b", cx);
    insert(&setup.workspace, "c", cx);
    assert!(
        handle.notifications("textDocument/didChange").is_empty(),
        "not yet"
    );
    assert!(wait_for(cx, |_| !handle
        .notifications("textDocument/didChange")
        .is_empty()));
    std::thread::sleep(Duration::from_millis(50));
    let changes = handle.notifications("textDocument/didChange");
    assert_eq!(changes.len(), 1, "{changes:?}");
    assert_eq!(changes[0]["textDocument"]["version"], 2);
    let text = active_text(&setup.workspace, cx);
    assert_eq!(changes[0]["contentChanges"][0]["text"], text.as_str());
    assert!(changes[0]["contentChanges"][0].get("range").is_none());
    // Saved: told, without the text, as the server asked.
    run(&setup.workspace, Invocation::new("file.save"), cx).unwrap();
    assert!(wait_for(cx, |_| !handle
        .notifications("textDocument/didSave")
        .is_empty()));
    let saved = &handle.notifications("textDocument/didSave")[0];
    assert!(saved.get("text").is_none());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
    // Saving again with nothing changed tells nothing more.
    run(&setup.workspace, Invocation::new("file.save"), cx).unwrap();
    wait_for(cx, |_| false);
    assert_eq!(handle.notifications("textDocument/didSave").len(), 1);
}

#[gpui_kit::test]
fn closing_the_last_document_stops_its_server(cx: &mut TestAppContext) {
    let (setup, cx) = server(cx);
    open_rust(&setup, "main.rs", "fn main() {}\n", cx);
    open_rust(&setup, "lib.rs", "\n", cx);
    let handle = handle(&setup, 0);
    run(&setup.workspace, Invocation::new("file.close"), cx).unwrap();
    assert!(wait_for(cx, |_| handle
        .notifications("textDocument/didClose")
        .len()
        == 1));
    assert_eq!(
        cx.update(|_, cx| server_keys(cx)).len(),
        1,
        "main.rs is still open"
    );
    run(&setup.workspace, Invocation::new("file.close"), cx).unwrap();
    assert!(wait_for(cx, |_| !handle.notifications("exit").is_empty()));
    assert_eq!(handle.requests("shutdown").len(), 1);
    assert_eq!(handle.notifications("textDocument/didClose").len(), 2);
    assert!(cx.update(|_, cx| server_keys(cx)).is_empty());
}

#[gpui_kit::test]
fn a_server_that_cannot_start_is_told_once(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.executor().allow_parking();
    let dir = tempfile::tempdir().unwrap();
    cx.update(|_, cx| {
        cx.global_mut::<AppState>()
            .settings
            .lsp
            .servers
            .insert("rust".into(), "no-such-server".into());
        set_launcher(Rc::new(|_, _| Err(std::io::Error::other("not found"))), cx);
    });
    take_warnings();
    for name in ["a.rs", "b.rs"] {
        let path = dir.path().join(name);
        std::fs::write(&path, "fn f() {}").unwrap();
        open(&workspace, &path, cx);
    }
    wait_for(cx, |_| false);
    let warnings = take_warnings();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(
        warnings[0].contains("cannot start no-such-server: not found"),
        "{warnings:?}"
    );
    let key = cx.update(|_, cx| server_keys(cx))[0].clone();
    assert!(matches!(
        cx.update(|_, cx| server_state(&key, cx)),
        Some(ServerState::Failed(_))
    ));
    let error = run(&workspace, Invocation::new("lsp.format"), cx).unwrap_err();
    assert!(error.to_string().contains("not running"), "{error}");
    // A command line that does not parse fails the same way.
    cx.update(|_, cx| {
        cx.global_mut::<AppState>()
            .settings
            .lsp
            .servers
            .insert("rust".into(), "\"unclosed".into());
    });
    workspace.update(cx, |workspace, cx| workspace.refresh_views(cx));
    wait_for(cx, |_| false);
    let warnings = take_warnings();
    assert!(
        warnings
            .iter()
            .any(|warning| warning.contains("not closed")),
        "{warnings:?}"
    );
}

#[gpui_kit::test]
fn a_server_that_exits_is_told_and_its_diagnostics_go(cx: &mut TestAppContext) {
    let (setup, cx) = server(cx);
    let path = open_rust(&setup, "main.rs", "fn main() {}\n", cx);
    let handle = handle(&setup, 0);
    handle.notify(
        "textDocument/publishDiagnostics",
        json!({ "uri": told_uri(&handle, &path), "diagnostics": [
            { "range": range((0, 3), (0, 7)), "message": "x" },
        ]}),
    );
    let view = active_view(&setup.workspace, cx);
    let buffer = view.read_with(cx, |view, _| view.buffer.entity_id());
    assert!(wait_for(cx, |cx| cx
        .update(|_, cx| !diagnostics(buffer, cx).is_empty())));
    take_warnings();
    handle.close();
    assert!(wait_for(cx, |cx| cx
        .update(|_, cx| diagnostics(buffer, cx).is_empty())));
    let warnings = take_warnings();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(
        warnings[0].contains("rust language server is not running"),
        "{warnings:?}"
    );
    // Restarting starts another.
    run(&setup.workspace, Invocation::new("lsp.restart"), cx).unwrap();
    assert!(wait_for(cx, |_| launches(&setup) == 2));
    let second = self::handle(&setup, 1);
    assert!(wait_for(cx, |_| !second
        .notifications("textDocument/didOpen")
        .is_empty()));
}

#[gpui_kit::test]
fn diagnostics_are_underlined_followed_and_listed(cx: &mut TestAppContext) {
    let (setup, cx) = server(cx);
    let path = open_rust(&setup, "main.rs", "let ё = 1;\nlet b = 2;\n", cx);
    let handle = handle(&setup, 0);
    let view = active_view(&setup.workspace, cx);
    let buffer = view.read_with(cx, |view, _| view.buffer.entity_id());
    // No severity is an error; an empty range takes the character after it; UTF-16 columns.
    handle.notify(
        "textDocument/publishDiagnostics",
        json!({ "uri": told_uri(&handle, &path), "diagnostics": [
            { "range": range((1, 4), (1, 5)), "severity": 2, "source": "rustc",
              "message": "unused variable `b`" },
            { "range": range((0, 4), (0, 5)), "message": "bad name" },
            { "range": range((1, 9), (1, 9)), "severity": 4, "message": "end" },
        ]}),
    );
    assert!(wait_for(cx, |cx| cx.update(|_, cx| diagnostics(
        buffer, cx
    )
    .len()
        == 3)));
    let found = cx.update(|_, cx| diagnostics(buffer, cx));
    let b = "let ё = 1;\n".len();
    assert_eq!(found[0].range, 4..6, "ё is two bytes");
    assert_eq!(found[0].level, DiagnosticLevel::Error);
    assert_eq!(found[1].range, b + 4..b + 5);
    assert_eq!(found[1].message, "rustc: unused variable `b`");
    assert_eq!(found[2].range, b + 9..b + 10);
    assert_eq!(found[2].level, DiagnosticLevel::Hint);
    let drawn = cx.update(|_, cx| underlined(buffer, cx));
    assert_eq!(drawn.len(), 3);

    // Next and Previous go around, with the message in the call tip.
    run(&setup.workspace, Invocation::new("lsp.next-diagnostic"), cx).unwrap();
    assert_eq!(selected(&setup.workspace, cx), "ё");
    assert_eq!(
        tip(&setup.workspace, cx).as_deref(),
        Some("Error: bad name")
    );
    run(&setup.workspace, Invocation::new("lsp.next-diagnostic"), cx).unwrap();
    assert_eq!(selected(&setup.workspace, cx), "b");
    run(&setup.workspace, Invocation::new("lsp.next-diagnostic"), cx).unwrap();
    run(&setup.workspace, Invocation::new("lsp.next-diagnostic"), cx).unwrap();
    assert_eq!(selected(&setup.workspace, cx), "ё", "around the end");
    run(
        &setup.workspace,
        Invocation::new("lsp.previous-diagnostic"),
        cx,
    )
    .unwrap();
    assert_eq!(selected(&setup.workspace, cx), ";", "around the start");

    // An edit before them moves them; one that deletes a diagnostic's text drops it.
    view.update(cx, |view, cx| view.go_to(0, cx));
    insert(&setup.workspace, "// x\n", cx);
    let found = cx.update(|_, cx| diagnostics(buffer, cx));
    assert_eq!(found[0].range, 9..11);
    run(&setup.workspace, Invocation::new("edit.select-all"), cx).unwrap();
    insert(&setup.workspace, "fresh", cx);
    assert!(cx.update(|_, cx| diagnostics(buffer, cx)).is_empty());
    let error = run(&setup.workspace, Invocation::new("lsp.next-diagnostic"), cx).unwrap_err();
    assert!(error.to_string().contains("reports nothing"), "{error}");
    let error = run(&setup.workspace, Invocation::new("lsp.diagnostics"), cx).unwrap_err();
    assert!(error.to_string().contains("reports nothing"), "{error}");

    // Listed in the search results.
    handle.notify(
        "textDocument/publishDiagnostics",
        json!({ "uri": told_uri(&handle, &path), "diagnostics": [
            { "range": range((0, 0), (0, 5)), "severity": 1, "message": "line one\nmore" },
        ]}),
    );
    assert!(wait_for(cx, |cx| cx.update(|_, cx| diagnostics(
        buffer, cx
    )
    .len()
        == 1)));
    run(&setup.workspace, Invocation::new("lsp.diagnostics"), cx).unwrap();
    let rows = result_rows(&setup.workspace, cx);
    assert!(rows[0].starts_with("Diagnostics of "), "{rows:?}");
    assert!(
        rows.iter().any(|row| row.contains("Error: line one")),
        "{rows:?}"
    );
    assert!(!rows.iter().any(|row| row.contains("more")), "{rows:?}");
    // Diagnostics of a file not open, or of another scheme, are left alone.
    handle.notify(
        "textDocument/publishDiagnostics",
        json!({ "uri": "untitled:x", "diagnostics": [{ "range": range((0, 0), (0, 1)), "message": "?" }] }),
    );
    wait_for(cx, |_| false);
    assert_eq!(cx.update(|_, cx| diagnostics(buffer, cx)).len(), 1);
}

#[gpui_kit::test]
fn hover_shows_the_servers_text_and_the_diagnostics_at_the_caret(cx: &mut TestAppContext) {
    let (setup, cx) = setup(
        || {
            FakeServer::new(capabilities()).on("textDocument/hover", |params| {
                Ok(if params["position"]["character"] == 0 {
                    Value::Null
                } else {
                    json!({ "contents": { "kind": "markdown", "value": "```rust\nfn main()\n```" } })
                })
            })
        },
        cx,
    );
    let path = open_rust(&setup, "main.rs", "fn main() {}\n", cx);
    let handle = handle(&setup, 0);
    let view = active_view(&setup.workspace, cx);
    view.update(cx, |view, cx| view.go_to(4, cx));
    run(&setup.workspace, Invocation::new("lsp.hover"), cx).unwrap();
    assert!(wait_for(cx, |cx| tip(&setup.workspace, cx).is_some()));
    assert_eq!(tip(&setup.workspace, cx).as_deref(), Some("fn main()"));
    let asked = &handle.requests("textDocument/hover")[0];
    assert_eq!(asked["position"], json!({ "line": 0, "character": 4 }));
    // With a diagnostic there, its message first.
    handle.notify(
        "textDocument/publishDiagnostics",
        json!({ "uri": told_uri(&handle, &path), "diagnostics": [
            { "range": range((0, 3), (0, 7)), "severity": 2, "message": "never used" },
        ]}),
    );
    let buffer = view.read_with(cx, |view, _| view.buffer.entity_id());
    assert!(wait_for(cx, |cx| cx
        .update(|_, cx| !diagnostics(buffer, cx).is_empty())));
    run(&setup.workspace, Invocation::new("lsp.hover"), cx).unwrap();
    assert!(wait_for(cx, |cx| tip(&setup.workspace, cx)
        .is_some_and(|tip| tip.contains("never"))));
    assert_eq!(
        tip(&setup.workspace, cx).as_deref(),
        Some("Warning: never used\n\nfn main()")
    );
    // Nothing at the start of the line: said so.
    view.update(cx, |view, cx| {
        view.call_tip = None;
        view.go_to(0, cx)
    });
    take_info();
    run(&setup.workspace, Invocation::new("lsp.hover"), cx).unwrap();
    assert!(wait_for(cx, |_| !take_info().is_empty()));
    assert_eq!(tip(&setup.workspace, cx), None);
}

#[gpui_kit::test]
fn a_request_unanswered_in_time_is_reported(cx: &mut TestAppContext) {
    let (setup, cx) = setup(
        || {
            FakeServer::new(capabilities()).on("textDocument/hover", |_| {
                std::thread::sleep(Duration::from_millis(300));
                Ok(Value::Null)
            })
        },
        cx,
    );
    open_rust(&setup, "main.rs", "fn main() {}\n", cx);
    take_errors();
    run(&setup.workspace, Invocation::new("lsp.hover"), cx).unwrap();
    cx.executor()
        .advance_clock(REQUEST_TIMEOUT + Duration::from_secs(1));
    cx.run_until_parked();
    let errors = take_errors();
    assert!(
        errors
            .iter()
            .any(|error| error.contains("did not answer within 10 s")),
        "{errors:?}"
    );
}

#[gpui_kit::test]
fn definitions_open_their_file_or_are_listed(cx: &mut TestAppContext) {
    let (setup, cx) = setup(
        || {
            FakeServer::new(capabilities()).on("textDocument/definition", |params| {
                let uri = params["textDocument"]["uri"].as_str().unwrap();
                let lib = uri.replace("main.rs", "lib.rs");
                Ok(match params["position"]["character"].as_u64() {
                    // One, in another file.
                    Some(4) => json!({ "uri": lib, "range": range((1, 7), (1, 12)) }),
                    // Two.
                    Some(5) => json!([
                        { "uri": lib, "range": range((0, 0), (0, 3)) },
                        { "uri": uri, "range": range((0, 0), (0, 2)) },
                    ]),
                    _ => Value::Null,
                })
            })
        },
        cx,
    );
    std::fs::write(setup.dir.path().join("lib.rs"), "//\npub fn thing() {}\n").unwrap();
    open_rust(&setup, "main.rs", "use thing;\n", cx);
    let view = active_view(&setup.workspace, cx);
    view.update(cx, |view, cx| view.go_to(4, cx));
    run(&setup.workspace, Invocation::new("lsp.definition"), cx).unwrap();
    assert!(wait_for(cx, |cx| selected(&setup.workspace, cx) == "thing"));
    assert!(active_view(&setup.workspace, cx).read_with(cx, |view, cx| {
        view.buffer.read(cx).path().unwrap().ends_with("lib.rs")
    }));
    // Several: listed.
    open(&setup.workspace, &setup.dir.path().join("main.rs"), cx);
    let view = active_view(&setup.workspace, cx);
    view.update(cx, |view, cx| view.go_to(5, cx));
    run(&setup.workspace, Invocation::new("lsp.definition"), cx).unwrap();
    assert!(wait_for(cx, |cx| !result_rows(&setup.workspace, cx).is_empty()));
    let rows = result_rows(&setup.workspace, cx);
    assert!(rows[0].starts_with("Definitions of "), "{rows:?}");
    // None: said so.
    view.update(cx, |view, cx| view.go_to(0, cx));
    take_info();
    run(&setup.workspace, Invocation::new("lsp.definition"), cx).unwrap();
    assert!(wait_for(cx, |_| take_info()
        .iter()
        .any(|info| info.contains("no definition"))));
}

#[gpui_kit::test]
fn references_are_listed_by_file_and_line(cx: &mut TestAppContext) {
    let (setup, cx) = setup(
        || {
            FakeServer::new(capabilities()).on("textDocument/references", |params| {
                assert_eq!(params["context"]["includeDeclaration"], true);
                let uri = params["textDocument"]["uri"].as_str().unwrap().to_owned();
                if params["position"]["character"] == 0 {
                    return Ok(json!([]));
                }
                let other = uri.replace("main.rs", "other.rs");
                Ok(json!([
                    { "uri": uri, "range": range((0, 3), (0, 7)) },
                    { "uri": uri, "range": range((1, 0), (1, 4)) },
                    { "uri": uri, "range": range((1, 6), (1, 10)) },
                    { "uri": other, "range": range((0, 0), (0, 4)) },
                ]))
            })
        },
        cx,
    );
    std::fs::write(setup.dir.path().join("other.rs"), "main();\n").unwrap();
    open_rust(&setup, "main.rs", "fn main() {}\nmain; main;\n", cx);
    let view = active_view(&setup.workspace, cx);
    view.update(cx, |view, cx| view.go_to(4, cx));
    run(&setup.workspace, Invocation::new("lsp.references"), cx).unwrap();
    assert!(wait_for(cx, |cx| !result_rows(&setup.workspace, cx).is_empty()));
    let rows = result_rows(&setup.workspace, cx);
    assert!(rows[0].starts_with("References to main ("), "{rows:?}");
    assert!(rows[0].contains('4'), "four hits: {rows:?}");
    // Two lines of main.rs, one of other.rs (not open: read from disk).
    assert!(
        rows.iter().any(|row| row.contains("main; main;")),
        "{rows:?}"
    );
    assert!(rows.iter().any(|row| row.contains("main();")), "{rows:?}");
    view.update(cx, |view, cx| view.go_to(0, cx));
    take_info();
    run(&setup.workspace, Invocation::new("lsp.references"), cx).unwrap();
    assert!(wait_for(cx, |_| take_info()
        .iter()
        .any(|info| info.contains("no references"))));
}

#[gpui_kit::test]
fn renaming_changes_every_file_without_saving(cx: &mut TestAppContext) {
    let (setup, cx) = setup(
        || {
            FakeServer::new(capabilities()).on("textDocument/rename", |params| {
                let uri = params["textDocument"]["uri"].as_str().unwrap().to_owned();
                let name = params["newName"].as_str().unwrap();
                if name == "files" {
                    return Ok(json!({ "documentChanges": [
                        { "kind": "create", "uri": uri.replace("main.rs", "new.rs") },
                    ]}));
                }
                if name == "bad" {
                    return Err(ResponseError::new(-32602, "not a valid name"));
                }
                let other = uri.replace("main.rs", "other.rs");
                Ok(json!({ "changes": {
                    uri: [
                        { "range": range((0, 3), (0, 7)), "newText": name },
                        { "range": range((1, 0), (1, 4)), "newText": name },
                    ],
                    other: [{ "range": range((0, 0), (0, 4)), "newText": name }],
                }}))
            })
        },
        cx,
    );
    let other = setup.dir.path().join("other.rs");
    std::fs::write(&other, "main();\n").unwrap();
    let main = open_rust(&setup, "main.rs", "fn main() {}\nmain();\n", cx);
    let view = active_view(&setup.workspace, cx);
    view.update(cx, |view, cx| view.go_to(4, cx));
    // Without a name, a dialog asks, with the word at the caret.
    run(&setup.workspace, Invocation::new("lsp.rename"), cx).unwrap();
    let dialog = birchpad::language_servers::commands::RENAME_OPENED
        .with(|opened| opened.borrow().as_ref().and_then(|dialog| dialog.upgrade()))
        .expect("the dialog opened");
    assert_eq!(
        dialog.read_with(cx, |dialog, cx| dialog.input.read(cx).value().to_string()),
        "main"
    );
    cx.update(|window, cx| window.close_dialog(cx));
    take_info();
    run(
        &setup.workspace,
        Invocation::with_args("lsp.rename", json!({ "name": "start" })),
        cx,
    )
    .unwrap();
    assert!(wait_for(cx, |_| !take_info().is_empty()));
    let main_text = |cx: &mut VisualTestContext| {
        setup.workspace.read_with(cx, |workspace, cx| {
            workspace
                .view_for_path(&main, cx)
                .map(|view| view.read(cx).text(cx).to_string())
        })
    };
    assert_eq!(main_text(cx).as_deref(), Some("fn start() {}\nstart();\n"));
    // The other file opened, changed once read, and nothing saved.
    assert!(wait_for(cx, |cx| setup.workspace.read_with(
        cx,
        |workspace, cx| {
            workspace
                .view_for_path(&other, cx)
                .is_some_and(|view| view.read(cx).text(cx).to_string() == "start();\n")
        }
    )));
    assert_eq!(std::fs::read_to_string(&other).unwrap(), "main();\n");
    assert_eq!(
        std::fs::read_to_string(&main).unwrap(),
        "fn main() {}\nmain();\n"
    );
    // One undo takes the whole rename back from main.rs.
    open(&setup.workspace, &main, cx);
    run(&setup.workspace, Invocation::new("edit.undo"), cx).unwrap();
    assert_eq!(active_text(&setup.workspace, cx), "fn main() {}\nmain();\n");

    // Refused: an edit that creates files, an error of the server, an empty name.
    take_errors();
    run(
        &setup.workspace,
        Invocation::with_args("lsp.rename", json!({ "name": "files" })),
        cx,
    )
    .unwrap();
    assert!(wait_for(cx, |_| !take_errors().is_empty()));
    assert_eq!(active_text(&setup.workspace, cx), "fn main() {}\nmain();\n");
    run(
        &setup.workspace,
        Invocation::with_args("lsp.rename", json!({ "name": "bad" })),
        cx,
    )
    .unwrap();
    assert!(wait_for(cx, |_| take_errors()
        .iter()
        .any(|error| error.contains("not a valid name"))));
    let error = run(
        &setup.workspace,
        Invocation::with_args("lsp.rename", json!({ "name": "  " })),
        cx,
    )
    .unwrap_err();
    assert!(error.to_string().contains("empty"), "{error}");
}

#[gpui_kit::test]
fn formatting_applies_the_servers_edits_in_one_step(cx: &mut TestAppContext) {
    let (setup, cx) = setup(
        || {
            FakeServer::new(capabilities()).on("textDocument/formatting", |params| {
                assert_eq!(params["options"]["insertSpaces"], false);
                Ok(json!([
                    { "range": range((0, 0), (0, 4)), "newText": "\t" },
                    { "range": range((1, 0), (1, 0)), "newText": "// end\n" },
                ]))
            })
        },
        cx,
    );
    open_rust(&setup, "main.rs", "    x\n", cx);
    run(&setup.workspace, Invocation::new("lsp.format"), cx).unwrap();
    assert!(wait_for(cx, |cx| active_text(&setup.workspace, cx)
        == "\tx\n// end\n"));
    run(&setup.workspace, Invocation::new("edit.undo"), cx).unwrap();
    assert_eq!(active_text(&setup.workspace, cx), "    x\n");
}

#[gpui_kit::test]
fn a_server_without_a_feature_says_so(cx: &mut TestAppContext) {
    let (setup, cx) = setup(|| FakeServer::new(json!({ "textDocumentSync": 1 })), cx);
    open_rust(&setup, "main.rs", "fn main() {}\n", cx);
    for command in [
        "lsp.format",
        "lsp.definition",
        "lsp.references",
        "lsp.hover",
    ] {
        let error = run(&setup.workspace, Invocation::new(command), cx).unwrap_err();
        assert!(
            error.to_string().contains("does not offer"),
            "{command}: {error}"
        );
    }
    let error = run(
        &setup.workspace,
        Invocation::with_args("lsp.rename", json!({ "name": "x" })),
        cx,
    )
    .unwrap_err();
    assert!(error.to_string().contains("does not offer"), "{error}");
}

#[gpui_kit::test]
fn completions_come_after_a_trigger_character(cx: &mut TestAppContext) {
    let (setup, cx) = setup(
        || {
            FakeServer::new(capabilities()).on("textDocument/completion", |params| {
                let trigger = params["context"]["triggerCharacter"].clone();
                Ok(json!({ "isIncomplete": false, "items": [
                    { "label": "len", "detail": "fn len(&self) -> usize", "sortText": "1" },
                    { "label": "is_empty", "insertText": "is_empty()", "sortText": "2" },
                    { "label": format!("trigger {trigger}"), "sortText": "3" },
                ]}))
            })
        },
        cx,
    );
    cx.update(|_, cx| cx.global_mut::<AppState>().settings.completion.words = true);
    open_rust(&setup, "main.rs", "", cx);
    let handle = handle(&setup, 0);
    cx.simulate_input("v.");
    let view = active_view(&setup.workspace, cx);
    let labels = |cx: &mut VisualTestContext| {
        view.read_with(cx, |view, _| {
            view.completion.as_ref().map(|popup| {
                popup
                    .items
                    .iter()
                    .map(|item| item.label.clone())
                    .collect::<Vec<_>>()
            })
        })
    };
    assert!(wait_for(cx, |cx| labels(cx).is_some()));
    // The server's first, then the API file's.
    assert_eq!(
        labels(cx).unwrap()[..3],
        ["len", "is_empty", "trigger \".\""]
    );
    let asked = handle.requests("textDocument/completion");
    assert_eq!(asked.last().unwrap()["context"]["triggerKind"], 2);
    // The text was sent before the request.
    let changes = handle.notifications("textDocument/didChange");
    assert_eq!(changes.last().unwrap()["contentChanges"][0]["text"], "v.");
    // Typing on filters them without asking again.
    let before = handle.requests("textDocument/completion").len();
    cx.simulate_input("i");
    let filtered = labels(cx).unwrap();
    assert_eq!(filtered[0], "is_empty");
    assert!(!filtered.contains(&"len".to_owned()), "{filtered:?}");
    wait_for(cx, |_| false);
    assert_eq!(handle.requests("textDocument/completion").len(), before);
    run(&setup.workspace, Invocation::new("completion.accept"), cx).unwrap();
    assert_eq!(active_text(&setup.workspace, cx), "v.is_empty()");
}

#[gpui_kit::test]
fn a_changed_command_line_restarts_the_server(cx: &mut TestAppContext) {
    let (setup, cx) = server(cx);
    open_rust(&setup, "main.rs", "fn main() {}\n", cx);
    let first = handle(&setup, 0);
    cx.update(|_, cx| {
        cx.global_mut::<AppState>()
            .settings
            .lsp
            .servers
            .insert("rust".into(), "other-server".into());
    });
    setup
        .workspace
        .update(cx, |workspace, cx| workspace.refresh_views(cx));
    assert!(wait_for(cx, |_| launches(&setup) == 2));
    assert!(wait_for(cx, |_| !first.notifications("exit").is_empty()));
    assert_eq!(setup.started.borrow().commands[1].0, "other-server");
    let second = handle(&setup, 1);
    assert!(wait_for(cx, |_| !second
        .notifications("textDocument/didOpen")
        .is_empty()));
    // No server for the language any more: stopped.
    cx.update(|_, cx| {
        cx.global_mut::<AppState>().settings.lsp.servers.clear();
    });
    setup
        .workspace
        .update(cx, |workspace, cx| workspace.refresh_views(cx));
    assert!(wait_for(cx, |_| !second.notifications("exit").is_empty()));
    assert!(cx.update(|_, cx| server_keys(cx)).is_empty());
}
