use birchpad_commands::Invocation;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::json;

use super::*;
use crate::workspace::tests::open_workspace;

const PROBE: &[u8] = include_bytes!("../../../../plugin/tests/fixtures/probe.wasm");

fn setup(
    cx: &mut TestAppContext,
) -> (Entity<Workspace>, &mut VisualTestContext, tempfile::TempDir) {
    let (workspace, cx) = open_workspace(cx);
    let dir = tempfile::tempdir().unwrap();
    cx.update(|_, cx| {
        cx.global_mut::<AppState>().paths.user_settings =
            Some(dir.path().join("config").join("settings.toml"));
    });
    (workspace, cx, dir)
}

/// A plugin's folder to install from, outside the plugins folder.
fn download(dir: &tempfile::TempDir, id: &str) -> PathBuf {
    let folder = dir.path().join("downloads").join(id);
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(
        folder.join("plugin.toml"),
        format!(
            "id = \"{id}\"\nname = \"Probe\"\nversion = \"2.1\"\ndescription = \"Probes.\"\napi = \"0.1\"\n"
        ),
    )
    .unwrap();
    std::fs::write(folder.join("plugin.wasm"), PROBE).unwrap();
    folder.join("plugin.toml")
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

fn listed(cx: &mut VisualTestContext) -> Vec<(String, String, bool)> {
    cx.update(|_, cx| {
        rows(cx)
            .0
            .into_iter()
            .map(|row| (row.id, row.status, row.disabled))
            .collect()
    })
}

#[gpui_kit::test]
fn installs_turns_off_and_removes_a_plugin(cx: &mut TestAppContext) {
    let (workspace, cx, dir) = setup(cx);
    let manifest = download(&dir, "probe");
    run(
        &workspace,
        Invocation::with_args("plugin.install", json!({ "path": manifest })),
        cx,
    )
    .unwrap();
    assert!(
        dir.path()
            .join("config/plugins/probe/plugin.wasm")
            .is_file()
    );
    assert_eq!(listed(cx), [("probe".to_owned(), "On".to_owned(), false)]);
    let row = cx.update(|_, cx| rows(cx).0.remove(0));
    assert_eq!((row.name.as_str(), row.version.as_str()), ("Probe", "2.1"));
    assert_eq!(row.description, "Probes.");

    let off = json!({ "plugin": "probe", "enabled": false });
    run(&workspace, Invocation::with_args("plugin.enable", off), cx).unwrap();
    assert_eq!(listed(cx), [("probe".to_owned(), "Off".to_owned(), true)]);
    let on = json!({ "plugin": "probe", "enabled": true });
    run(&workspace, Invocation::with_args("plugin.enable", on), cx).unwrap();
    assert_eq!(listed(cx)[0].1, "On");

    run(
        &workspace,
        Invocation::with_args("plugin.uninstall", json!({ "plugin": "probe" })),
        cx,
    )
    .unwrap();
    assert!(listed(cx).is_empty());
    assert!(manifest.is_file(), "the download stays");
}

#[gpui_kit::test]
fn refuses_what_it_cannot_do(cx: &mut TestAppContext) {
    let (workspace, cx, dir) = setup(cx);
    let missing = dir.path().join("nowhere/plugin.toml");
    assert!(
        run(
            &workspace,
            Invocation::with_args("plugin.install", json!({ "path": missing })),
            cx
        )
        .is_err()
    );
    for (id, args) in [
        ("plugin.uninstall", json!({ "plugin": "none" })),
        (
            "plugin.enable",
            json!({ "plugin": "none", "enabled": false }),
        ),
        ("plugin.enable", json!({ "plugin": "none" })),
    ] {
        assert!(
            run(&workspace, Invocation::with_args(id, args), cx).is_err(),
            "{id}"
        );
    }

    // Installing against the policy.
    let manifest = download(&dir, "probe");
    cx.update(|_, cx| {
        cx.global_mut::<AppState>().settings.plugins.install = PluginInstall::Deny;
    });
    let error = run(
        &workspace,
        Invocation::with_args("plugin.install", json!({ "path": manifest })),
        cx,
    )
    .unwrap_err();
    assert!(error.to_string().contains("administrator"), "{error}");
    assert!(cx.update(|_, cx| policy_note(cx)).is_some());
    cx.update(|_, cx| {
        cx.global_mut::<AppState>().settings.plugins.install = PluginInstall::Allowlist;
    });
    assert!(cx.update(|_, cx| policy_note(cx)).is_some());
    cx.update(|_, cx| {
        cx.global_mut::<AppState>().settings.plugins.install = PluginInstall::Allow;
    });
    assert_eq!(cx.update(|_, cx| policy_note(cx)), None);
}

#[gpui_kit::test]
fn opens_and_draws_the_dialog(cx: &mut TestAppContext) {
    let (workspace, cx, dir) = setup(cx);
    let manifest = download(&dir, "probe");
    run(
        &workspace,
        Invocation::with_args("plugin.install", json!({ "path": manifest })),
        cx,
    )
    .unwrap();
    std::fs::create_dir_all(dir.path().join("config/plugins/broken")).unwrap();
    run(&workspace, Invocation::new("plugin.reload"), cx).unwrap();
    run(&workspace, Invocation::new("plugin.admin"), cx).unwrap();
    cx.run_until_parked();
    assert!(cx.debug_bounds("plugin-row-0").is_some());
    let (_, problems) = cx.update(|_, cx| rows(cx));
    assert_eq!(problems.len(), 1);
}

#[gpui_kit::test]
fn install_asks_for_the_plugins_manifest(cx: &mut TestAppContext) {
    let (workspace, cx, dir) = setup(cx);
    let ask = |cx: &mut VisualTestContext| {
        workspace.update_in(cx, |workspace, window, cx| {
            workspace.ask_plugin_to_install(window, cx);
        });
        cx.run_until_parked();
        assert!(cx.did_prompt_for_paths());
    };
    // Cancelled: nothing installed.
    ask(cx);
    cx.simulate_path_prompt_response(|_| None);
    cx.run_until_parked();
    assert!(listed(cx).is_empty());
    // A file that is no plugin's manifest: an error, nothing installed.
    let other = dir.path().join("notes.txt");
    std::fs::write(&other, "not a manifest").unwrap();
    ask(cx);
    cx.simulate_path_prompt_response(move |_| Some(vec![other]));
    cx.run_until_parked();
    assert!(listed(cx).is_empty());
    assert!(cx.update(|window, cx| !window.notifications(cx).is_empty()));
    // The manifest of a plugin: installed and on.
    let manifest = download(&dir, "probe");
    ask(cx);
    cx.simulate_path_prompt_response(move |_| Some(vec![manifest]));
    cx.run_until_parked();
    assert_eq!(listed(cx).len(), 1);
    assert_eq!(listed(cx)[0].0, "probe");
}
