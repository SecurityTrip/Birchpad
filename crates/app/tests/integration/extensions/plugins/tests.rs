use birchpad_commands::Invocation;
use birchpad_config::PluginInstall;
use gpui_kit::{TestAppContext, VisualTestContext};
use serde_json::json;

use crate::workspace::{active_text, open_workspace, take_info};
use anyhow::Result;
use birchpad::app_state::AppState;
use birchpad::extensions::PanelOwner;
use birchpad::extensions::plugins::*;
use birchpad::workspace::Workspace;
use gpui_kit::Entity;

/// The plugin of `birchpad-plugin`'s tests, which uses every function of the API.
const PROBE: &[u8] = include_bytes!("../../../../../plugin/tests/fixtures/probe.wasm");

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
            "id = \"{id}\"\nname = \"Probe {id}\"\nversion = \"1.0\"\napi = \"0.2\"\nevents = [{}]\n",
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

/// Installs the probe as `id` with more of `plugin.toml`: permissions, settings.
fn install_with(dir: &tempfile::TempDir, id: &str, events: &[&str], extra: &str) {
    install(dir, id, events, PROBE);
    let manifest = dir.path().join("plugins").join(id).join("plugin.toml");
    let text = std::fs::read_to_string(&manifest).unwrap();
    std::fs::write(&manifest, format!("{text}{extra}")).unwrap();
}

const SETTINGS: &str = "\n[[settings]]\nkey = \"greeting\"\ntitle = \"Greeting\"\nkind = \"text\"\ndefault = \"hey\"\n\n[[settings]]\nkey = \"loud\"\ntitle = \"Loud\"\nkind = \"bool\"\n";

fn set_setting(
    workspace: &Entity<Workspace>,
    plugin: &str,
    key: &str,
    value: &str,
    cx: &mut VisualTestContext,
) -> Result<()> {
    run(
        workspace,
        Invocation::with_args(
            "plugin.set-setting",
            json!({ "plugin": plugin, "key": key, "value": value }),
        ),
        cx,
    )
}

#[gpui_kit::test]
fn adds_its_commands_to_the_context_menu(cx: &mut TestAppContext) {
    let (workspace, cx, dir) = setup(cx);
    load(&workspace, cx);
    // No plugin, no items and no separator.
    assert!(
        cx.update(|_, cx| birchpad::context_menu::plugin_items(cx))
            .is_empty()
    );
    install(&dir, "probe", &[], PROBE);
    load(&workspace, cx);
    let items = cx.update(|_, cx| birchpad::context_menu::plugin_items(cx));
    assert_eq!(
        items,
        [
            birchpad::context_menu::Node::Separator,
            birchpad::context_menu::Node::Command {
                invocation: Invocation::with_args(
                    "plugin.run",
                    json!({ "plugin": "probe", "command": "upper" })
                ),
                label: Some("Upper Case".into()),
            },
        ]
    );
    assert_eq!(cx.update(|_, cx| context_menu_commands(cx)).len(), 1);
}

#[gpui_kit::test]
fn keeps_its_settings_and_shows_them_in_preferences(cx: &mut TestAppContext) {
    let (workspace, cx, dir) = setup(cx);
    install_with(&dir, "probe", &[], SETTINGS);
    load(&workspace, cx);
    take_info();
    run_plugin(&workspace, "probe", "settings", cx).unwrap();
    run_plugin(&workspace, "probe", "settings", cx).unwrap();
    assert_eq!(
        take_info(),
        ["greeting hey, runs 1", "greeting hey, runs 2"]
    );
    // Kept beside the plugin, so that Birchpad started again finds them.
    let kept = birchpad_plugin::manager::read_settings(&dir.path().join("plugins"), "probe");
    assert_eq!(kept["runs"], "2");

    set_setting(&workspace, "probe", "greeting", "yo", cx).unwrap();
    run_plugin(&workspace, "probe", "settings", cx).unwrap();
    assert_eq!(take_info(), ["greeting yo, runs 3"]);

    let shown = cx.update(|_, cx| birchpad::preferences::plugin_settings(cx));
    let shown: Vec<(&str, &str, &str)> = shown
        .iter()
        .map(|(plugin, spec, value)| (plugin.as_str(), spec.key.as_str(), value.as_str()))
        .collect();
    // Undeclared settings, as `runs`, are the plugin's own and not shown.
    assert_eq!(shown, [("probe", "greeting", "yo"), ("probe", "loud", "")]);
}

#[gpui_kit::test]
fn refuses_settings_it_cannot_keep(cx: &mut TestAppContext) {
    let (workspace, cx, dir) = setup(cx);
    install_with(&dir, "probe", &[], SETTINGS);
    load(&workspace, cx);
    let error = set_setting(&workspace, "probe", "loud", "maybe", cx).unwrap_err();
    assert!(error.to_string().contains("not a value"), "{error}");
    set_setting(&workspace, "probe", "loud", "true", cx).unwrap();
    assert!(set_setting(&workspace, "missing", "loud", "true", cx).is_err());
    assert!(set_setting(&workspace, "probe", "Bad Key", "x", cx).is_err());
    let kept = birchpad_plugin::manager::read_settings(&dir.path().join("plugins"), "probe");
    assert_eq!(kept.len(), 1);
    assert_eq!(kept["loud"], "true");
}

#[gpui_kit::test]
fn uses_files_once_the_user_allows_it(cx: &mut TestAppContext) {
    let (workspace, cx, dir) = setup(cx);
    install_with(&dir, "probe", &[], "permissions = [\"files\"]\n");
    install(&dir, "plain", &[], PROBE);
    load(&workspace, cx);
    let source = dir.path().join("source.txt");
    let target = dir.path().join("target.txt");
    std::fs::write(&source, "shout").unwrap();
    set_setting(&workspace, "probe", "source", source.to_str().unwrap(), cx).unwrap();
    set_setting(&workspace, "probe", "target", target.to_str().unwrap(), cx).unwrap();

    let error = run_plugin(&workspace, "probe", "files", cx).unwrap_err();
    assert!(error.to_string().contains("Plugins Admin"), "{error}");
    assert!(!target.exists());

    let allow = |plugin: &str, allowed: bool| {
        Invocation::with_args(
            "plugin.allow-files",
            json!({ "plugin": plugin, "allowed": allowed }),
        )
    };
    run(&workspace, allow("probe", true), cx).unwrap();
    run_plugin(&workspace, "probe", "files", cx).unwrap();
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "SHOUT");
    // The copy opened in a tab.
    assert_eq!(active_text(&workspace, cx), "SHOUT");

    // What the plugin does not ask for cannot be given; taking it back always works.
    assert!(run(&workspace, allow("plain", true), cx).is_err());
    run(&workspace, allow("plain", false), cx).unwrap();
    assert!(run(&workspace, allow("missing", true), cx).is_err());
    run(&workspace, allow("probe", false), cx).unwrap();
    assert!(run_plugin(&workspace, "probe", "files", cx).is_err());
}

#[gpui_kit::test]
fn shows_controls_and_hears_them(cx: &mut TestAppContext) {
    let (workspace, cx, dir) = setup(cx);
    install(&dir, "probe", &[], PROBE);
    load(&workspace, cx);
    run_plugin(&workspace, "probe", "controls", cx).unwrap();
    let (title, controls) = workspace.read_with(cx, |workspace, cx| {
        let panel = workspace
            .docks
            .view(birchpad::panels::PanelKind::PluginPanel)
            .unwrap()
            .clone()
            .downcast::<birchpad::panels::PluginPanel>()
            .unwrap();
        let panel = panel.read(cx);
        (panel.title.clone(), panel.controls.clone())
    });
    assert_eq!(title, "Probe controls");
    assert_eq!(controls.len(), 5);
    take_info();
    let owner = PanelOwner::Plugin("probe".into());
    for (id, value) in [("go", ""), ("name", "typed text"), ("loud", "false")] {
        workspace
            .update_in(cx, |workspace, window, cx| {
                workspace.plugin_control_used(&owner, id, value, window, cx)
            })
            .unwrap();
    }
    assert_eq!(
        take_info(),
        [
            "control go = ",
            "control name = typed text",
            "control loud = false"
        ]
    );
    // A script's panel has no controls to hear.
    workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.plugin_control_used(&PanelOwner::Script("s".into()), "go", "", window, cx)
        })
        .unwrap();
    assert!(take_info().is_empty());
}

#[gpui_kit::test]
fn hands_the_command_line_message_to_the_plugins_that_ask(cx: &mut TestAppContext) {
    let (workspace, cx, dir) = setup(cx);
    install(&dir, "hears", &["message"], PROBE);
    install(&dir, "deaf", &[], PROBE);
    // The message comes while the plugins are still loading: they get it once ready.
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.load_plugins(window, cx);
        workspace.deliver_plugin_message("open the log", window, cx);
    });
    cx.run_until_parked();
    assert_eq!(take_info(), ["message open the log"]);
    // Once ready, at once; an empty message too.
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.deliver_plugin_message("", window, cx);
    });
    assert_eq!(take_info(), ["message "]);
}

#[gpui_kit::test]
fn runs_tasks_in_the_background_and_hands_back_what_they_return(cx: &mut TestAppContext) {
    let (workspace, cx, dir) = setup(cx);
    install(&dir, "probe", &[], PROBE);
    load(&workspace, cx);
    run(
        &workspace,
        Invocation::with_args("edit.insert-text", json!({ "text": "sum fail" })),
        cx,
    )
    .unwrap();
    take_info();
    run_plugin(&workspace, "probe", "tasks", cx).unwrap();
    let heard = take_info();
    for expected in [
        "summing, document of 0 bytes",
        "task sum gave 9",
        "task fail failed: the task failed on purpose",
    ] {
        assert!(heard.contains(&expected.to_owned()), "{heard:?}");
    }
    assert_eq!(cx.update(|_, cx| cx.global::<Plugins>().running_tasks), 0);
    // A task the plugin does not know fails in it, and is told so.
    run(&workspace, Invocation::new("edit.select-all"), cx).unwrap();
    run(
        &workspace,
        Invocation::with_args("edit.insert-text", json!({ "text": "nothing" })),
        cx,
    )
    .unwrap();
    take_info();
    run_plugin(&workspace, "probe", "tasks", cx).unwrap();
    assert_eq!(take_info(), ["task nothing failed: unknown task nothing"]);
}

#[gpui_kit::test]
fn tells_plugins_about_typing_and_the_selection(cx: &mut TestAppContext) {
    let (workspace, cx, dir) = setup(cx);
    install(&dir, "hears", &["changed", "selection-changed"], PROBE);
    load(&workspace, cx);
    run(&workspace, Invocation::new("file.new"), cx).unwrap();
    take_info();
    run(
        &workspace,
        Invocation::with_args("edit.insert-text", json!({ "text": "typed" })),
        cx,
    )
    .unwrap();
    assert_eq!(take_info(), ["event Event::Changed"]);
    run(&workspace, Invocation::new("edit.select-all"), cx).unwrap();
    assert_eq!(take_info(), ["event Event::SelectionChanged"]);
    // Nothing changed, nothing told.
    run(&workspace, Invocation::new("edit.select-all"), cx).unwrap();
    assert!(take_info().is_empty());
    // The plugin's own edit is not told back to it.
    run_plugin(&workspace, "hears", "upper", cx).unwrap();
    assert_eq!(active_text(&workspace, cx), "TYPED");
    assert!(take_info().is_empty());
    // Undo is a change too.
    run(&workspace, Invocation::new("edit.undo"), cx).unwrap();
    assert_eq!(take_info(), ["event Event::Changed"]);
}

#[gpui_kit::test]
fn its_commands_are_in_the_shortcut_mappers_plugin_tab(cx: &mut TestAppContext) {
    let (workspace, cx, dir) = setup(cx);
    load(&workspace, cx);
    let rows = |cx: &mut VisualTestContext| {
        cx.update(|_, cx| {
            birchpad::shortcut_mapper::rows_of(
                birchpad::shortcut_mapper::MapperTab::Plugins,
                &cx.global::<birchpad::commands::ActiveKeymap>().0,
                cx,
            )
        })
    };
    assert!(rows(cx).is_empty());
    install(&dir, "probe", &[], PROBE);
    load(&workspace, cx);
    let rows = rows(cx);
    assert_eq!(rows.len(), 14);
    assert_eq!(rows[0].name, "Probe probe: Upper Case");
    assert_eq!(rows[0].category, "plugin");
    assert_eq!(
        rows[0].invocation,
        Invocation::with_args(
            "plugin.run",
            json!({ "plugin": "probe", "command": "upper" })
        )
    );
}
