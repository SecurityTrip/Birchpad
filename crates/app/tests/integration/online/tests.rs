//! The online catalog in the window: Plugins Admin's list of plugins to install and update, and
//! dictionaries to download, under the OnlineFeatures policy.

use std::collections::HashMap;
use std::io::Write as _;
use std::sync::Arc;

use anyhow::Result;
use birchpad::app_state::AppState;
use birchpad::updates::Updates;
use birchpad_commands::Invocation;
use birchpad_update::{Transport, hex};
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::json;
use sha2::{Digest as _, Sha256};

use crate::workspace::open_workspace;
use birchpad::workspace::Workspace;

const PROBE: &[u8] = include_bytes!("../../../../plugin/tests/fixtures/probe.wasm");
const CATALOG: &str = "https://example.org/catalog.json";

/// Serves files by their address.
struct Serve(HashMap<String, Vec<u8>>);

impl Transport for Serve {
    fn get(&self, url: &str) -> Result<Vec<u8>, String> {
        self.0.get(url).cloned().ok_or_else(|| format!("no {url}"))
    }
}

fn zip_of(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    for (name, bytes) in files {
        zip.start_file(*name, options).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

fn pinned(url: &str, body: &[u8]) -> serde_json::Value {
    json!({ "url": url, "size": body.len(), "sha256": hex(&Sha256::digest(body)) })
}

/// A plugin package of the probe as `id`, version `version`.
fn package(id: &str, version: &str) -> Vec<u8> {
    let manifest =
        format!("id = \"{id}\"\nname = \"Probe {id}\"\nversion = \"{version}\"\napi = \"0.2\"\n");
    zip_of(&[("plugin.toml", manifest.as_bytes()), ("plugin.wasm", PROBE)])
}

/// A workspace with a settings folder, online features on, and a catalog of `plugins` (id,
/// version, package) and the dictionary `xx_TEST`.
fn setup<'a>(
    cx: &'a mut TestAppContext,
    plugins: &[(&str, &str, Vec<u8>)],
) -> (
    Entity<Workspace>,
    &'a mut VisualTestContext,
    tempfile::TempDir,
) {
    let (workspace, cx) = open_workspace(cx);
    let dir = tempfile::tempdir().unwrap();
    let mut files = HashMap::new();
    let mut listed = Vec::new();
    for (id, version, body) in plugins {
        let url = format!("https://example.org/{id}-{version}.zip");
        listed.push(json!({
            "id": id, "name": format!("Probe {id}"), "version": version,
            "description": "Probes.", "api": "0.2", "file": pinned(&url, body)
        }));
        files.insert(url, body.clone());
    }
    let (aff, dic) = (b"SET UTF-8\n".to_vec(), b"2\nhello\nworld\n".to_vec());
    files.insert("https://example.org/xx.aff".into(), aff.clone());
    files.insert("https://example.org/xx.dic".into(), dic.clone());
    let catalog = json!({
        "format": 1,
        "plugins": listed,
        "dictionaries": [{
            "language": "xx_TEST", "name": "Test",
            "aff": pinned("https://example.org/xx.aff", &aff),
            "dic": pinned("https://example.org/xx.dic", &dic)
        }]
    });
    files.insert(CATALOG.into(), serde_json::to_vec(&catalog).unwrap());
    let transport: Arc<dyn Transport> = Arc::new(Serve(files));
    let settings = dir.path().join("settings.toml");
    cx.update(|_, cx| {
        let state = cx.global_mut::<AppState>();
        state.paths.user_settings = Some(settings);
        state.settings.network.online_features = true;
        state.settings.network.catalog = CATALOG.into();
        Updates::install(transport, Vec::new(), None, cx);
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

fn installed(cx: &mut VisualTestContext) -> Vec<(String, String)> {
    cx.update(|_, cx| {
        birchpad::extensions::admin::rows(cx)
            .0
            .into_iter()
            .map(|row| (row.id, row.version))
            .collect()
    })
}

#[gpui_kit::test]
fn lists_installs_and_updates_plugins(cx: &mut TestAppContext) {
    let (workspace, cx, dir) = setup(cx, &[("probe", "2.0.0", package("probe", "2.0.0"))]);
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.load_plugins(window, cx)
    });
    cx.run_until_parked();
    // Nothing listed until the list is read.
    assert!(
        cx.update(|_, cx| birchpad::extensions::admin::available(cx))
            .is_empty()
    );
    run(&workspace, Invocation::new("catalog.refresh"), cx).unwrap();
    let available = cx.update(|_, cx| birchpad::extensions::admin::available(cx));
    assert_eq!(available.len(), 1);
    assert_eq!(
        (available[0].name.as_str(), available[0].version.as_str()),
        ("Probe probe", "2.0.0")
    );

    run(
        &workspace,
        Invocation::with_args("plugin.install-online", json!({ "plugin": "probe" })),
        cx,
    )
    .unwrap();
    assert_eq!(installed(cx), [("probe".to_owned(), "2.0.0".to_owned())]);
    assert!(
        cx.update(|_, cx| birchpad::extensions::admin::available(cx))
            .is_empty()
    );
    // Its download is cleared away.
    assert!(!dir.path().join("plugins/.probe.download").exists());
    // No newer version: no update offered.
    let update = cx.update(|_, cx| birchpad::extensions::admin::rows(cx).0[0].update.clone());
    assert_eq!(update, None);

    // An older one installed by hand: 2.0.0 is offered over it.
    let folder = dir.path().join("plugins/probe");
    std::fs::write(
        folder.join("plugin.toml"),
        "id = \"probe\"\nname = \"Probe probe\"\nversion = \"1.0.0\"\napi = \"0.2\"\n",
    )
    .unwrap();
    run(&workspace, Invocation::new("plugin.reload"), cx).unwrap();
    let update = cx.update(|_, cx| birchpad::extensions::admin::rows(cx).0[0].update.clone());
    assert_eq!(update.as_deref(), Some("2.0.0"));
    run(&workspace, Invocation::new("plugin.admin"), cx).unwrap();
    assert!(cx.debug_bounds("plugin-update-0").is_some());
}

#[gpui_kit::test]
fn refuses_what_the_list_or_the_policy_does_not_allow(cx: &mut TestAppContext) {
    let wrong = package("other", "1.0.0");
    let (workspace, cx, dir) = setup(
        cx,
        &[
            ("probe", "1.0.0", package("probe", "1.0.0")),
            ("liar", "1.0.0", wrong),
        ],
    );
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.load_plugins(window, cx)
    });
    cx.run_until_parked();
    let install =
        |id: &str| Invocation::with_args("plugin.install-online", json!({ "plugin": id }));
    // Not read yet: no such plugin.
    let error = run(&workspace, install("probe"), cx).unwrap_err();
    assert!(error.to_string().contains("refresh"), "{error}");
    run(&workspace, Invocation::new("catalog.refresh"), cx).unwrap();
    assert!(run(&workspace, install("missing"), cx).is_err());
    // A package that holds another plugin than listed is not kept.
    run(&workspace, install("liar"), cx).unwrap();
    assert!(installed(cx).is_empty());
    assert!(!dir.path().join("plugins/other").exists());
    // The administrator allows only some plugins.
    cx.update(|_, cx| {
        let plugins = &mut cx.global_mut::<AppState>().settings.plugins;
        plugins.install = birchpad_config::PluginInstall::Allowlist;
        plugins.allowed = vec!["someone-else".into()];
    });
    let error = run(&workspace, install("probe"), cx).unwrap_err();
    assert!(error.to_string().contains("administrator"), "{error}");
    // Online features off: nothing goes online.
    cx.update(|_, cx| cx.global_mut::<AppState>().settings.network.online_features = false);
    let error = run(&workspace, Invocation::new("catalog.refresh"), cx).unwrap_err();
    assert!(error.to_string().contains("OnlineFeatures"), "{error}");
    assert!(run(&workspace, install("probe"), cx).is_err());
}

#[gpui_kit::test]
fn a_list_that_cannot_be_read_says_why(cx: &mut TestAppContext) {
    let (workspace, cx, _dir) = setup(cx, &[]);
    cx.update(|_, cx| {
        cx.global_mut::<AppState>().settings.network.catalog = "https://example.org/none".into();
    });
    run(&workspace, Invocation::new("catalog.refresh"), cx).unwrap();
    let error = cx.update(|_, cx| birchpad::online::status(cx));
    assert!(error.is_some_and(|error| error.contains("none")));
    assert!(cx.update(|_, cx| birchpad::online::catalog(cx).is_none()));
    // Without a setting, Birchpad's own list.
    cx.update(|_, cx| cx.global_mut::<AppState>().settings.network.catalog = "  ".into());
    assert_eq!(
        cx.update(|_, cx| birchpad::online::catalog_url(cx)),
        birchpad_update::catalog::DEFAULT_CATALOG
    );
}

#[gpui_kit::test]
fn downloads_dictionaries(cx: &mut TestAppContext) {
    let (workspace, cx, dir) = setup(cx, &[]);
    run(&workspace, Invocation::new("spell.download-languages"), cx).unwrap();
    // The dialog read the list.
    assert!(cx.update(|_, cx| birchpad::online::catalog(cx).is_some()));
    assert!(cx.debug_bounds("dictionary-download-0").is_some());
    run(
        &workspace,
        Invocation::with_args("spell.download", json!({ "language": "xx_TEST" })),
        cx,
    )
    .unwrap();
    assert!(dir.path().join("dictionaries/xx_TEST.dic").is_file());
    assert!(
        cx.update(|_, cx| birchpad::spell_check::languages(cx))
            .contains(&"xx_TEST".to_owned())
    );
    run(
        &workspace,
        Invocation::with_args("spell.language", json!({ "language": "xx_TEST" })),
        cx,
    )
    .unwrap();
    let error = run(
        &workspace,
        Invocation::with_args("spell.download", json!({ "language": "zz_NONE" })),
        cx,
    )
    .unwrap_err();
    assert!(error.to_string().contains("zz_NONE"), "{error}");
}
