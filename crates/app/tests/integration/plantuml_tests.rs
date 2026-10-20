//! PlantUML installed by Birchpad: offered when none is found, downloaded through the transport
//! of updates, and run with its own Java.

use std::io::Write as _;
use std::path::Path;
use std::sync::Arc;

use birchpad::app_state::AppState;
use birchpad::plantuml::*;
use birchpad::preview::plantuml_command;
use birchpad::updates::Updates;
use birchpad_update::Transport;
use birchpad_update::component::{ComponentPackage, PLANTUML};
use gpui_kit::{TestAppContext, VisualTestContext};
use sha2::{Digest as _, Sha256};

use crate::workspace::open_workspace;

/// Serves the same bytes for every URL.
struct Serve(Vec<u8>);

impl Transport for Serve {
    fn get(&self, _: &str) -> Result<Vec<u8>, String> {
        Ok(self.0.clone())
    }
}

/// A package with PlantUML's layout: `plantuml.jar` and `jre`.
fn package_body() -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    for (name, bytes) in [
        ("plantuml.jar", &b"jar"[..]),
        ("jre/bin/java", b"java"),
        ("README.txt", b"read me"),
    ] {
        zip.start_file(name, options).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

fn pin(body: &[u8]) {
    let sha256: &'static str =
        Box::leak(birchpad_update::hex(&Sha256::digest(body)).into_boxed_str());
    PACKAGE.with(|package| {
        *package.borrow_mut() = Some(ComponentPackage {
            platform: "test-x64",
            url: "https://example.org/plantuml.zip",
            size: body.len() as u64,
            sha256,
        });
    });
}

/// A workspace whose data folder is `data` and whose updates download from `transport`.
fn with_data(data: &Path, transport: Arc<dyn Transport>, cx: &mut VisualTestContext) {
    let data = data.to_owned();
    cx.update(|_, cx| {
        cx.global_mut::<AppState>().paths.user_data = Some(data);
        cx.global_mut::<AppState>().settings.network.online_features = true;
        Updates::install(transport, Vec::new(), None, cx);
    });
}

#[test]
fn installed_plantuml_runs_with_its_own_java() {
    let folder = Path::new("data")
        .join("components")
        .join("plantuml-1.2026.9-1");
    let (program, args) = plantuml_command("plantuml", Some(&folder));
    let java = folder
        .join("jre")
        .join("bin")
        .join(if cfg!(windows) { "java.exe" } else { "java" });
    assert_eq!(program, java.display().to_string());
    assert_eq!(args[0], "-Djava.awt.headless=true");
    assert_eq!(args[1], "-jar");
    assert_eq!(args[2], folder.join("plantuml.jar").display().to_string());
    assert!(args.contains(&"-tsvg".to_owned()));
    assert_eq!(
        args.last().map(String::as_str),
        Some("-Playout=smetana"),
        "no Graphviz needed"
    );
    assert_eq!(plantuml_command("", Some(&folder)).0, program);
    // A PlantUML of the user's own comes first.
    assert_eq!(
        plantuml_command("/opt/plantuml.jar", Some(&folder)).0,
        "java"
    );
    assert_eq!(
        plantuml_command("my-plantuml", Some(&folder)).0,
        "my-plantuml"
    );
    assert_eq!(plantuml_command("plantuml", None).0, "plantuml");
}

#[gpui_kit::test]
fn one_click_installs_plantuml(cx: &mut TestAppContext) {
    let (_, cx) = open_workspace(cx);
    let data = tempfile::tempdir().unwrap();
    let body = package_body();
    pin(&body);
    with_data(data.path(), Arc::new(Serve(body.clone())), cx);
    assert_eq!(cx.update(|_, cx| installed(cx)), None);
    assert_eq!(
        cx.update(|_, cx| can_install(cx).unwrap()),
        body.len() as u64
    );
    cx.update(|_, cx| install(cx).unwrap());
    assert!(cx.update(|_, cx| state(cx).unwrap().running));
    // Asked again while it runs: nothing more.
    cx.update(|_, cx| install(cx).unwrap());
    cx.run_until_parked();
    let (running, error, generation) = cx.update(|_, cx| {
        let state = state(cx).unwrap();
        (state.running, state.error.clone(), state.generation)
    });
    assert!(!running);
    assert_eq!(error, None);
    assert_eq!(generation, 1);
    let folder = cx.update(|_, cx| installed(cx)).expect("installed");
    assert_eq!(folder, PLANTUML.folder(data.path()));
    assert_eq!(std::fs::read(folder.join("plantuml.jar")).unwrap(), b"jar");
    PACKAGE.with(|package| *package.borrow_mut() = None);
}

#[gpui_kit::test]
fn a_failed_installation_says_why(cx: &mut TestAppContext) {
    let (_, cx) = open_workspace(cx);
    let data = tempfile::tempdir().unwrap();
    pin(&package_body());
    // The server sends something else.
    with_data(data.path(), Arc::new(Serve(b"other".to_vec())), cx);
    cx.update(|_, cx| install(cx).unwrap());
    cx.run_until_parked();
    let error = cx.update(|_, cx| state(cx).unwrap().error.clone());
    assert!(
        error.is_some_and(|error| error.contains("https://example.org/plantuml.zip")),
        "the package is not the one pinned"
    );
    assert_eq!(cx.update(|_, cx| installed(cx)), None);
    PACKAGE.with(|package| *package.borrow_mut() = None);
}

#[gpui_kit::test]
fn plantuml_is_not_downloaded_when_it_cannot_be(cx: &mut TestAppContext) {
    let (_, cx) = open_workspace(cx);
    let data = tempfile::tempdir().unwrap();
    pin(&package_body());
    with_data(data.path(), Arc::new(Serve(package_body())), cx);
    // Downloads turned off.
    cx.update(|_, cx| {
        cx.global_mut::<AppState>().settings.network.online_features = false;
    });
    let refused = cx.update(|_, cx| install(cx).unwrap_err().to_string());
    assert!(refused.contains("network.online-features"), "{refused}");
    // No data folder to install in.
    cx.update(|_, cx| {
        cx.global_mut::<AppState>().settings.network.online_features = true;
        cx.global_mut::<AppState>().paths.user_data = None;
    });
    assert!(cx.update(|_, cx| can_install(cx)).is_err());
    assert_eq!(cx.update(|_, cx| installed(cx)), None);
    PACKAGE.with(|package| *package.borrow_mut() = None);
}
