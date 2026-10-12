use birchpad_commands::Invocation;
use birchpad_config::PluginInstall;
use gpui_kit::{TestAppContext, VisualTestContext};
use serde_json::json;

use super::*;
use crate::extensions::PanelOwner;
use crate::workspace::tests::{active_text, open_workspace, take_info};

/// The plugin of `birchpad-plugin`'s tests, which uses every function of the API.
const PROBE: &[u8] = include_bytes!("../../../../plugin/tests/fixtures/probe.wasm");

fn setup(
    cx: &mut TestAppContext,
) -> (Entity<Workspace>, &mut VisualTestContext, tempfile::TempDir) {
    let (workspace, cx) = open_workspace(cx);
    let dir = tempfile::tempdir().unwrap();
    cx.update(|_, cx| {
        cx.global_mut::<AppState>().paths.user_settings = Some(dir.path().join("settings.toml"));
    });
    (workspace, cx, dir)
}

/// Installs the probe as `id`, hearing `events`.
fn install(dir: &tempfile::TempDir, id: &str, events: &[&str], wasm: &[u8]) {
    let folder = dir.path().join("plugins").join(id);
    std::fs::create_dir_all(&folder).unwrap();
    let events: Vec<String> = events.iter().map(|event| format!("{event:?}")).collect();
    std::fs::write(
        folder.join("plugin.toml"),
        format!(
            "id = \"{id}\"\nname = \"Probe {id}\"\nversion = \"1.0\"\napi = \"0.1\"\nevents = [{}]\n",
            events.join(", ")
        ),
    )
    .unwrap();
    std::fs::write(folder.join("plugin.wasm"), wasm).unwrap();
}

fn load(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) {
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.load_plugins(window, cx)
    });
    cx.run_until_parked();
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

fn run_plugin(
    workspace: &Entity<Workspace>,
    plugin: &str,
    command: &str,
    cx: &mut VisualTestContext,
) -> Result<()> {
    run(
        workspace,
        Invocation::with_args(
            "plugin.run",
            json!({ "plugin": plugin, "command": command }),
        ),
        cx,
    )
}

fn states(cx: &mut VisualTestContext) -> Vec<(String, String)> {
    cx.update(|_, cx| {
        cx.global::<Plugins>()
            .entries
            .iter()
            .map(|entry| {
                let state = match &entry.state {
                    State::Loading => "loading".to_owned(),
                    State::Ready(_) => "ready".to_owned(),
                    State::Disabled => "disabled".to_owned(),
                    State::Refused(why) => format!("refused: {why}"),
                    State::Failed(why) => format!("failed: {why}"),
                };
                (entry.id().to_owned(), state)
            })
            .collect()
    })
}

#[gpui_kit::test]
fn loads_plugins_and_runs_their_commands(cx: &mut TestAppContext) {
    let (workspace, cx, dir) = setup(cx);
    install(&dir, "probe", &[], PROBE);
    load(&workspace, cx);
    assert_eq!(states(cx), [("probe".to_owned(), "ready".to_owned())]);
    let menus = cx.update(|_, cx| menus(cx));
    assert_eq!(menus.len(), 1);
    assert_eq!(menus[0].1, "Probe probe");
    assert_eq!(menus[0].2[0].title, "Upper Case");

    run(
        &workspace,
        Invocation::with_args("edit.insert-text", json!({ "text": "one two" })),
        cx,
    )
    .unwrap();
    run(&workspace, Invocation::new("edit.select-all"), cx).unwrap();
    run_plugin(&workspace, "probe", "upper", cx).unwrap();
    assert_eq!(active_text(&workspace, cx), "ONE TWO");
    run(&workspace, Invocation::new("edit.undo"), cx).unwrap();
    assert_eq!(active_text(&workspace, cx), "one two");

    take_info();
    run_plugin(&workspace, "probe", "hello", cx).unwrap();
    assert_eq!(take_info(), ["said hello"]);
}

#[gpui_kit::test]
fn fills_its_panel_and_hears_clicks(cx: &mut TestAppContext) {
    let (workspace, cx, dir) = setup(cx);
    install(&dir, "probe", &[], PROBE);
    load(&workspace, cx);
    run_plugin(&workspace, "probe", "panel", cx).unwrap();
    take_info();
    workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.plugin_panel_clicked(&PanelOwner::Plugin("probe".into()), 1, window, cx)
        })
        .unwrap();
    assert_eq!(take_info(), ["clicked 1"]);
    // Commands it runs run after it.
    run_plugin(&workspace, "probe", "select-all", cx).unwrap();
}

#[gpui_kit::test]
fn tells_plugins_about_the_events_they_ask_for(cx: &mut TestAppContext) {
    let (workspace, cx, dir) = setup(cx);
    install(
        &dir,
        "hears",
        &["activated", "closed", "saved", "opened"],
        PROBE,
    );
    install(&dir, "deaf", &[], PROBE);
    load(&workspace, cx);
    take_info();
    run(&workspace, Invocation::new("file.new"), cx).unwrap();
    assert_eq!(take_info(), ["event Event::Activated"]);
    run(&workspace, Invocation::new("file.close"), cx).unwrap();
    let heard = take_info();
    assert!(
        heard.contains(&"event Event::Closed".to_owned()),
        "{heard:?}"
    );

    let file = dir.path().join("notes.txt");
    std::fs::write(&file, "text").unwrap();
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.open_path(&file, window, cx)
    });
    cx.run_until_parked();
    let heard = take_info();
    assert!(
        heard.contains(&"event Event::Opened".to_owned()),
        "{heard:?}"
    );
    run(
        &workspace,
        Invocation::with_args("edit.insert-text", json!({ "text": "more " })),
        cx,
    )
    .unwrap();
    run(&workspace, Invocation::new("file.save"), cx).unwrap();
    assert_eq!(take_info(), ["event Event::Saved"]);
    // Going back to a document activates it again, once.
    run(&workspace, Invocation::new("file.new"), cx).unwrap();
    run(&workspace, Invocation::new("view.previous-tab"), cx).unwrap();
    assert_eq!(
        take_info(),
        ["event Event::Activated", "event Event::Activated"]
    );
}

#[gpui_kit::test]
fn reports_what_fails(cx: &mut TestAppContext) {
    let (workspace, cx, dir) = setup(cx);
    install(&dir, "probe", &[], PROBE);
    install(&dir, "broken", &[], b"not wasm");
    load(&workspace, cx);
    let found = states(cx);
    assert!(found[0].1.starts_with("failed: "), "{found:?}");
    let error = run_plugin(&workspace, "broken", "upper", cx).unwrap_err();
    assert!(error.to_string().contains("broken"), "{error}");

    let error = run_plugin(&workspace, "probe", "fail", cx).unwrap_err();
    assert_eq!(error.to_string(), "it failed on purpose");
    assert!(run_plugin(&workspace, "probe", "trap", cx).is_err());
    assert!(run_plugin(&workspace, "probe", "nothing", cx).is_err());
    assert!(run_plugin(&workspace, "none", "upper", cx).is_err());
    // It works again after a trap.
    run_plugin(&workspace, "probe", "hello", cx).unwrap();
    assert!(
        run(
            &workspace,
            Invocation::with_args("plugin.run", json!({})),
            cx
        )
        .is_err()
    );
}

#[gpui_kit::test]
fn leaves_off_what_is_turned_off_or_not_allowed(cx: &mut TestAppContext) {
    let (workspace, cx, dir) = setup(cx);
    install(&dir, "off", &[], PROBE);
    install(&dir, "other", &[], PROBE);
    birchpad_plugin::manager::set_enabled(&dir.path().join("plugins"), "off", false).unwrap();
    cx.update(|_, cx| {
        let plugins = &mut cx.global_mut::<AppState>().settings.plugins;
        plugins.install = PluginInstall::Allowlist;
        plugins.allowed = vec!["off".into()];
    });
    load(&workspace, cx);
    let found = states(cx);
    assert_eq!(found[0], ("off".to_owned(), "disabled".to_owned()));
    assert!(found[1].1.contains("administrator"), "{found:?}");
    assert!(cx.update(|_, cx| menus(cx)).is_empty());
    let error = run_plugin(&workspace, "off", "hello", cx).unwrap_err();
    assert!(error.to_string().contains("turned off"), "{error}");
    let error = run_plugin(&workspace, "other", "hello", cx).unwrap_err();
    assert!(error.to_string().contains("administrator"), "{error}");

    cx.update(|_, cx| cx.global_mut::<AppState>().settings.plugins.install = PluginInstall::Deny);
    birchpad_plugin::manager::set_enabled(&dir.path().join("plugins"), "off", true).unwrap();
    run(&workspace, Invocation::new("plugin.reload"), cx).unwrap();
    assert!(
        states(cx)
            .iter()
            .all(|(_, state)| state.starts_with("refused"))
    );
}

#[gpui_kit::test]
fn reload_finds_new_plugins_and_no_folder_has_none(cx: &mut TestAppContext) {
    let (workspace, cx, dir) = setup(cx);
    load(&workspace, cx);
    assert!(states(cx).is_empty());
    install(&dir, "late", &[], PROBE);
    run(&workspace, Invocation::new("plugin.reload"), cx).unwrap();
    assert_eq!(states(cx), [("late".to_owned(), "ready".to_owned())]);
    // A folder without a readable plugin is a problem, not a plugin.
    std::fs::create_dir_all(dir.path().join("plugins/empty")).unwrap();
    load(&workspace, cx);
    assert_eq!(cx.update(|_, cx| cx.global::<Plugins>().problems.len()), 1);
}
