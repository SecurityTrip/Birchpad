//! The install statistics of update checks (ADR 0064): the install id made, kept and deleted,
//! what is sent, the policy, and which PlantUML the previews draw with.

use std::path::Path;
use std::sync::{Arc, Mutex};

use birchpad::app_state::AppState;
use birchpad::help::VERSION;
use birchpad::plantuml::plantuml_in_use;
use birchpad::updates::*;
use birchpad::workspace::Workspace;
use birchpad_config::{UpdateChannel, UpdateMode};
use birchpad_update::manifest::{self, PRODUCT, SCHEMA};
use birchpad_update::statistics::{PlantUml, is_install_id};
use birchpad_update::{
    DEFAULT_MANIFEST, Manifest, ManifestRelease, SigningKey, Transport, UPDATE_API,
};
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use semver::Version;

use crate::workspace::open_workspace;

fn key() -> SigningKey {
    SigningKey::from_bytes(&[7; 32])
}

fn signed() -> Vec<u8> {
    let manifest = Manifest {
        schema: SCHEMA,
        product: PRODUCT.into(),
        timestamp: now(),
        expires: now() + 86_400,
        releases: vec![ManifestRelease {
            version: Version::new(99, 0, 0),
            page: "https://example.com/99.0.0".into(),
            packages: Vec::new(),
        }],
    };
    serde_json::to_vec(&manifest::sign(&manifest, &[key()])).unwrap()
}

/// The Update API and GitHub, both answering with the signed manifest; records what is asked.
#[derive(Default)]
struct Api {
    posts: Mutex<Vec<(String, Vec<u8>)>>,
    gets: Mutex<Vec<String>>,
}

impl Transport for Api {
    fn get(&self, url: &str) -> Result<Vec<u8>, String> {
        self.gets.lock().unwrap().push(url.to_owned());
        Ok(signed())
    }

    fn post_json(&self, url: &str, body: &[u8]) -> Result<Vec<u8>, String> {
        self.posts
            .lock()
            .unwrap()
            .push((url.to_owned(), body.to_vec()));
        Ok(signed())
    }
}

impl Api {
    /// The body of the last post, as JSON.
    fn last_body(&self) -> serde_json::Value {
        let posts = self.posts.lock().unwrap();
        let (url, body) = posts.last().expect("posted");
        assert_eq!(url, UPDATE_API);
        serde_json::from_slice(body).unwrap()
    }
}

/// Updates through `Api`, with Birchpad's data folder in `data` (`state.toml` is saved there).
fn install(data: &Path, cx: &mut VisualTestContext) -> Arc<Api> {
    let api = Arc::new(Api::default());
    let data = data.to_owned();
    cx.update(|_, cx| {
        Updates::install(api.clone(), vec![key().verifying_key()], None, cx);
        let state = cx.global_mut::<AppState>();
        state.paths.user_data = Some(data);
        state.settings.updates.mode = UpdateMode::Notify;
        state.settings.updates.channel = UpdateChannel::Beta;
        // PATH is the test machine's: no PlantUML looked for there.
        state.settings.preview.plantuml = "plantuml-that-is-nowhere".into();
    });
    api
}

fn check(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) {
    workspace.update_in(cx, |workspace, window, cx| {
        workspace
            .check_for_updates(Trigger::Background, window, cx)
            .unwrap();
    });
    cx.run_until_parked();
}

fn set_statistics(on: bool, cx: &mut VisualTestContext) {
    cx.update(|_, cx| cx.global_mut::<AppState>().settings.updates.statistics = on);
}

fn install_id(cx: &mut VisualTestContext) -> Option<String> {
    cx.update(|_, cx| AppState::global(cx).state.updates.install_id.clone())
}

fn saved_install_id(data: &Path) -> Option<String> {
    birchpad_config::UserState::load(&data.join("state.toml"))
        .updates
        .install_id
}

#[gpui_kit::test]
fn the_install_id_is_made_once_kept_deleted_when_off_and_new_when_on_again(
    cx: &mut TestAppContext,
) {
    let (workspace, cx) = open_workspace(cx);
    let data = tempfile::tempdir().unwrap();
    let api = install(data.path(), cx);
    assert_eq!(install_id(cx), None, "none before the first check");

    check(&workspace, cx);
    let first = install_id(cx).expect("made at the first check");
    assert!(is_install_id(&first), "{first}");
    assert_eq!(saved_install_id(data.path()).as_ref(), Some(&first));
    assert_eq!(api.last_body()["installId"], first.as_str());

    check(&workspace, cx);
    assert_eq!(install_id(cx).as_ref(), Some(&first), "kept");
    assert_eq!(api.last_body()["installId"], first.as_str());

    // Off: `{}`, and the id is gone from state.toml.
    set_statistics(false, cx);
    check(&workspace, cx);
    assert_eq!(api.last_body(), serde_json::json!({}));
    assert_eq!(install_id(cx), None);
    assert_eq!(saved_install_id(data.path()), None);

    // On again: another id.
    set_statistics(true, cx);
    check(&workspace, cx);
    let second = install_id(cx).expect("made again");
    assert!(is_install_id(&second));
    assert_ne!(second, first);
    assert_eq!(api.posts.lock().unwrap().len(), 4);
    assert!(api.gets.lock().unwrap().is_empty(), "GitHub not asked");
}

#[gpui_kit::test]
fn the_body_says_this_copy_s_version_channel_platform_and_plantuml(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let data = tempfile::tempdir().unwrap();
    let api = install(data.path(), cx);
    check(&workspace, cx);
    let body = api.last_body();
    assert_eq!(body["version"], VERSION);
    assert_eq!(body["channel"], "beta");
    match birchpad_update::component::platform() {
        Some(platform) => assert_eq!(body["platform"], platform),
        None => assert!(body.get("platform").is_none()),
    }
    assert_eq!(body["plantuml"], "none");
    assert!(body.get("plantumlVersion").is_none());
    let keys: Vec<&String> = body.as_object().unwrap().keys().collect();
    assert!(keys.len() <= 6, "{keys:?}");

    // PlantUML installed by Birchpad, drawn with as `preview.plantuml` comes.
    let folder = data.path().join("components").join("plantuml-1.2026.9-3");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join(".installed"), "").unwrap();
    cx.update(|_, cx| {
        cx.global_mut::<AppState>().settings.preview.plantuml = "plantuml".into();
    });
    check(&workspace, cx);
    let body = api.last_body();
    assert_eq!(body["plantuml"], "component");
    assert_eq!(body["plantumlVersion"], "1.2026.9-3");

    // Another one set: custom, the component notwithstanding.
    let jar = data.path().join("plantuml.jar");
    std::fs::write(&jar, "jar").unwrap();
    let setting = jar.display().to_string();
    cx.update(|_, cx| cx.global_mut::<AppState>().settings.preview.plantuml = setting);
    check(&workspace, cx);
    assert_eq!(api.last_body()["plantuml"], "custom");
    assert!(api.last_body().get("plantumlVersion").is_none());
}

#[gpui_kit::test]
fn a_mirror_is_sent_nothing_and_no_id_is_made_for_it(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let data = tempfile::tempdir().unwrap();
    let api = install(data.path(), cx);
    cx.update(|_, cx| {
        cx.global_mut::<AppState>().settings.updates.url =
            Some("https://mirror.local/m.json".into());
    });
    check(&workspace, cx);
    assert!(api.posts.lock().unwrap().is_empty());
    assert_eq!(*api.gets.lock().unwrap(), ["https://mirror.local/m.json"]);
    assert_eq!(install_id(cx), None);

    // An empty one is none: the API is asked.
    cx.update(|_, cx| cx.global_mut::<AppState>().settings.updates.url = Some(String::new()));
    check(&workspace, cx);
    assert_eq!(api.posts.lock().unwrap().len(), 1);
    assert!(install_id(cx).is_some());
}

#[gpui_kit::test]
fn off_sends_nothing_and_keeps_no_id_from_being_made(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let data = tempfile::tempdir().unwrap();
    let api = install(data.path(), cx);
    cx.update(|_, cx| cx.global_mut::<AppState>().settings.updates.mode = UpdateMode::Off);
    workspace.update_in(cx, |workspace, window, cx| {
        assert!(
            workspace
                .check_for_updates(Trigger::Request, window, cx)
                .is_err()
        );
    });
    cx.run_until_parked();
    assert!(api.posts.lock().unwrap().is_empty());
    assert!(api.gets.lock().unwrap().is_empty());
    assert_eq!(install_id(cx), None);
}

#[gpui_kit::test]
fn the_statistics_policy_wins_over_the_setting(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let data = tempfile::tempdir().unwrap();
    let api = install(data.path(), cx);
    check(&workspace, cx);
    assert!(install_id(cx).is_some());

    // The user turned them on; the administrator's UpdateStatistics = 0 turns them off.
    let resolved = birchpad_config::resolve(birchpad_config::Sources {
        user: Some(toml::from_str("updates.statistics = true").unwrap()),
        policy: Some(toml::from_str("updates.statistics = false").unwrap()),
        ..birchpad_config::Sources::default()
    });
    cx.update(|_, cx| {
        let state = cx.global_mut::<AppState>();
        state.set_settings(resolved);
        state.settings.updates.mode = UpdateMode::Notify;
        state.settings.preview.plantuml = "plantuml-that-is-nowhere".into();
        assert!(state.is_locked("updates.statistics"));
        forget_install_id_when_off(cx);
    });
    assert_eq!(install_id(cx), None, "deleted as the policy came");
    assert_eq!(saved_install_id(data.path()), None);
    check(&workspace, cx);
    assert_eq!(api.last_body(), serde_json::json!({}));
    assert_eq!(install_id(cx), None);
    // Preferences cannot turn them on again.
    let refused = cx.update(|_, cx| {
        birchpad::preferences::set("updates.statistics", Some(toml::Value::Boolean(true)), cx)
    });
    assert!(refused.is_err());
}

#[gpui_kit::test]
fn turning_statistics_off_in_preferences_deletes_the_id_at_once(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let data = tempfile::tempdir().unwrap();
    install(data.path(), cx);
    check(&workspace, cx);
    assert!(install_id(cx).is_some());
    let settings = data.path().join("settings.toml");
    std::fs::write(&settings, "").unwrap();
    cx.update(|_, cx| {
        cx.global_mut::<AppState>().paths.user_settings = Some(settings.clone());
        birchpad::preferences::set("updates.statistics", Some(toml::Value::Boolean(false)), cx)
            .unwrap();
    });
    assert!(
        std::fs::read_to_string(&settings)
            .unwrap()
            .contains("statistics = false")
    );
    assert_eq!(install_id(cx), None, "before any check");
    assert_eq!(saved_install_id(data.path()), None);
}

#[gpui_kit::test]
fn a_failing_api_is_only_logged_and_github_answers(cx: &mut TestAppContext) {
    /// The API down, GitHub up.
    struct Down;
    impl Transport for Down {
        fn get(&self, url: &str) -> Result<Vec<u8>, String> {
            assert_eq!(url, DEFAULT_MANIFEST);
            Ok(signed())
        }
        fn post_json(&self, _: &str, _: &[u8]) -> Result<Vec<u8>, String> {
            Err("503".into())
        }
    }
    let (workspace, cx) = open_workspace(cx);
    cx.update(|_, cx| {
        Updates::install(Arc::new(Down), vec![key().verifying_key()], None, cx);
        cx.global_mut::<AppState>().settings.updates.mode = UpdateMode::Notify;
    });
    check(&workspace, cx);
    let available = cx.update(|_, cx| cx.global::<Updates>().available.clone());
    assert_eq!(
        available.map(|release| release.version),
        Some(Version::new(99, 0, 0))
    );
    let window_has_dialog = cx.update(|window, cx| {
        use gpui_kit::component::WindowExt as _;
        window.has_active_dialog(cx)
    });
    assert!(!window_has_dialog, "nothing shown for the API");
}

#[test]
fn plantuml_is_the_component_when_the_previews_draw_with_it() {
    let data = tempfile::tempdir().unwrap();
    let folder = data.path().join("plantuml-1.2026.9-3");
    for setting in ["plantuml", "", "  plantuml  "] {
        assert_eq!(
            plantuml_in_use(setting, Some(&folder), None, None),
            PlantUml::Component {
                version: "1.2026.9-3".into()
            },
            "{setting:?}"
        );
    }
    // An earlier installation, still drawn with: its own version.
    let earlier = data.path().join("plantuml-1.2026.9-1");
    assert_eq!(
        plantuml_in_use("plantuml", Some(&earlier), None, None),
        PlantUml::Component {
            version: "1.2026.9-1".into()
        }
    );
    // A folder whose name says no version: this one's.
    let unnamed = data.path().join("plantuml-");
    assert_eq!(
        plantuml_in_use("plantuml", Some(&unnamed), None, None),
        PlantUml::Component {
            version: birchpad_update::component::PLANTUML.version.into()
        }
    );
}

#[test]
fn plantuml_is_custom_when_another_one_is_there_and_none_when_not() {
    let data = tempfile::tempdir().unwrap();
    let bin = data.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let program = if cfg!(windows) {
        "plantuml.exe"
    } else {
        "plantuml"
    };
    std::fs::write(bin.join(program), "").unwrap();
    let path = std::env::join_paths([data.path().join("empty"), bin.clone()]).unwrap();
    let component = data.path().join("plantuml-1.2026.9-3");

    // `plantuml` on PATH, nothing installed by Birchpad.
    assert_eq!(
        plantuml_in_use("plantuml", None, None, Some(&path)),
        PlantUml::Custom
    );
    // Not on PATH, or no PATH at all.
    let elsewhere = std::env::join_paths([data.path().join("empty")]).unwrap();
    assert_eq!(
        plantuml_in_use("plantuml", None, None, Some(&elsewhere)),
        PlantUml::None
    );
    assert_eq!(plantuml_in_use("", None, None, None), PlantUml::None);
    // An empty PATH entry is not the working folder.
    let empty = std::ffi::OsString::new();
    assert_eq!(
        plantuml_in_use("plantuml", None, None, Some(&empty)),
        PlantUml::None
    );

    // The setting names another one: custom when it is there, the component notwithstanding.
    let jar = data.path().join("tools").join("plantuml.jar");
    std::fs::create_dir_all(jar.parent().unwrap()).unwrap();
    std::fs::write(&jar, "jar").unwrap();
    let absolute = jar.display().to_string();
    assert_eq!(
        plantuml_in_use(&absolute, Some(&component), None, None),
        PlantUml::Custom
    );
    // Relative, from Birchpad's working folder, as the preview takes it.
    assert_eq!(
        plantuml_in_use("tools/plantuml.jar", None, Some(data.path()), None),
        PlantUml::Custom
    );
    assert_eq!(
        plantuml_in_use("tools/plantuml.jar", None, Some(&bin), None),
        PlantUml::None
    );
    // A setting that names nothing there: none, though Birchpad installed one.
    let missing = data.path().join("missing.jar").display().to_string();
    assert_eq!(
        plantuml_in_use(&missing, Some(&component), None, None),
        PlantUml::None
    );
    // A program by its name alone, on PATH.
    assert_eq!(
        plantuml_in_use(program, None, None, Some(&path)),
        PlantUml::Custom
    );
}
