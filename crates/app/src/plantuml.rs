//! PlantUML that Birchpad installs (ADR 0045): when no PlantUML is found, the preview of a
//! PlantUML document offers to install it; one click downloads PlantUML and the Java that runs it
//! for this platform (`birchpad_update::component::PLANTUML`), through the transport of updates,
//! and every PlantUML preview draws with it from then on.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Result, anyhow};
use birchpad_update::component::{self, ComponentPackage, PLANTUML};
use birchpad_update::statistics::PlantUml;
use gpui_kit::{App, AppContext as _, Global};

use crate::app_state::AppState;
use crate::commands::{CommandRegistry, Register as _};

/// The installation, while it runs and after it ended.
#[derive(Default)]
pub struct PlantUmlInstall {
    /// Bytes received of the package being downloaded.
    pub received: Arc<AtomicU64>,
    pub running: bool,
    /// Why the last installation failed.
    pub error: Option<String>,
    /// Bumped when an installation ends, so that previews draw again.
    pub generation: u64,
}

impl Global for PlantUmlInstall {}

#[cfg(feature = "test-support")]
thread_local! {
    /// The package tests install instead of the published one.
    pub static PACKAGE: std::cell::RefCell<Option<ComponentPackage>> =
        std::cell::RefCell::default();
}

/// The package for this platform.
fn package() -> Option<ComponentPackage> {
    #[cfg(feature = "test-support")]
    if let Some(package) = PACKAGE.with(|package| *package.borrow()) {
        return Some(package);
    }
    component::platform()
        .and_then(|platform| PLANTUML.package(platform))
        .copied()
}

/// The folder of the PlantUML Birchpad installed, if it did: this version's, or an earlier
/// one's until this one is installed.
pub fn installed(cx: &App) -> Option<PathBuf> {
    let data = AppState::global(cx).paths.user_data.clone()?;
    PLANTUML.installed_or_earlier(&data)
}

/// Which PlantUML the previews draw with, for the statistics of update checks (ADR 0064).
pub fn in_use(cx: &App) -> PlantUml {
    let setting = &AppState::global(cx).settings.preview.plantuml;
    let base = std::env::current_dir().ok();
    plantuml_in_use(
        setting,
        installed(cx).as_deref(),
        base.as_deref(),
        std::env::var_os("PATH").as_deref(),
    )
}

/// Which PlantUML a preview draws with, chosen as `preview::plantuml_command` chooses it: with
/// `setting` (`preview.plantuml`) as it comes (`plantuml`, or empty) and PlantUML installed by
/// Birchpad in `installed`, that one, of the version its folder names; else the program
/// `setting` names, a relative path taken from `base` (Birchpad's working folder) and a name
/// alone looked for on `path`, as running it looks: another PlantUML when it is there, none
/// when it is not.
pub fn plantuml_in_use(
    setting: &str,
    installed: Option<&Path>,
    base: Option<&Path>,
    path: Option<&OsStr>,
) -> PlantUml {
    let setting = match setting.trim() {
        "" => "plantuml",
        setting => setting,
    };
    if let Some(folder) = installed.filter(|_| setting == "plantuml") {
        return PlantUml::Component {
            version: installed_version(folder),
        };
    }
    let program = match base {
        Some(base) => crate::preview::setting_path(setting, base),
        None => setting.to_owned(),
    };
    if program_found(&program, path) {
        PlantUml::Custom
    } else {
        PlantUml::None
    }
}

/// The version of the PlantUML installed in `folder`, from its name (`plantuml-1.2026.9-3`, or
/// that of an earlier installation); this one's when the name says none.
fn installed_version(folder: &Path) -> String {
    let prefix = format!("{}-", PLANTUML.name);
    folder
        .file_name()
        .and_then(OsStr::to_str)
        .and_then(|name| name.strip_prefix(&prefix))
        .filter(|version| !version.is_empty())
        .unwrap_or(PLANTUML.version)
        .to_owned()
}

/// Whether `program` is there to run: a file at its path when it has a folder, else a file of
/// its name in one of the folders of `path` (on Windows `.exe` added to a name without an
/// extension, as starting it adds it).
fn program_found(program: &str, path: Option<&OsStr>) -> bool {
    let program = Path::new(program);
    let has_folder = program.is_absolute() || program.components().count() > 1;
    if has_folder {
        return program.is_file();
    }
    let name = if cfg!(windows) && program.extension().is_none() {
        program.with_extension("exe")
    } else {
        program.to_path_buf()
    };
    path.map(std::env::split_paths)
        .into_iter()
        .flatten()
        .filter(|folder| !folder.as_os_str().is_empty())
        .any(|folder| folder.join(&name).is_file())
}

/// Whether the PlantUML Birchpad installed is of an earlier version or revision than the one it
/// installs now (before ELK, ADR 0063): installing it again brings this one.
pub fn outdated(cx: &App) -> bool {
    let Some(data) = AppState::global(cx).paths.user_data.clone() else {
        return false;
    };
    PLANTUML.installed(&data).is_none() && PLANTUML.installed_or_earlier(&data).is_some()
}

/// Whether a preview offers the newer PlantUML (roadmap 7.4ac): the one it draws with is an
/// earlier revision of the one Birchpad installs (`preview.plantuml` as it comes), and this one
/// can be installed here. Nothing is downloaded until the offer is taken (ADR 0045).
pub fn offers_update(cx: &App) -> bool {
    let setting = AppState::global(cx)
        .settings
        .preview
        .plantuml
        .trim()
        .to_owned();
    matches!(setting.as_str(), "" | "plantuml") && outdated(cx) && can_install(cx).is_ok()
}

/// The installation's state.
pub fn state(cx: &App) -> Option<&PlantUmlInstall> {
    cx.try_global::<PlantUmlInstall>()
}

/// The size of the package to download, if PlantUML can be installed here; else why not.
pub fn can_install(cx: &App) -> Result<u64> {
    let package = package().ok_or_else(|| anyhow!("{}", t!("plantuml.no-package")))?;
    let state = AppState::global(cx);
    if !state.settings.network.online_features {
        anyhow::bail!("{}", t!("plantuml.online-features-off"));
    }
    if state.paths.user_data.is_none() {
        anyhow::bail!("{}", t!("plantuml.no-data-folder"));
    }
    Ok(package.size)
}

/// Downloads and installs PlantUML, unless it is being installed already.
pub fn install(cx: &mut App) -> Result<()> {
    can_install(cx)?;
    let package = package().expect("can_install found it");
    let data = AppState::global(cx)
        .paths
        .user_data
        .clone()
        .expect("can_install found it");
    let transport = crate::updates::Updates::transport(cx)
        .ok_or_else(|| anyhow!("{}", t!("plantuml.no-package")))?;
    if !cx.has_global::<PlantUmlInstall>() {
        cx.set_global(PlantUmlInstall::default());
    }
    let install = cx.global_mut::<PlantUmlInstall>();
    if install.running {
        return Ok(());
    }
    install.running = true;
    install.error = None;
    install.received.store(0, Ordering::Relaxed);
    let received = install.received.clone();
    let downloading = cx.background_spawn(async move {
        let progress = move |bytes| received.store(bytes, Ordering::Relaxed);
        component::install_package(&PLANTUML, &package, transport.as_ref(), &data, &progress)
    });
    // The progress shows while it downloads.
    let ticking = cx.spawn(async move |cx| {
        loop {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(250))
                .await;
            let running = cx.update(|cx| {
                cx.refresh_windows();
                state(cx).is_some_and(|install| install.running)
            });
            if !running {
                break;
            }
        }
    });
    cx.spawn(async move |cx| {
        let result = downloading.await;
        let installed = result.is_ok();
        cx.update(|cx| {
            let install = cx.global_mut::<PlantUmlInstall>();
            install.running = false;
            install.error = result.err();
            install.generation += 1;
            cx.refresh_windows();
            if installed {
                // The PlantUMLs kept running from an earlier folder hold its files (ADR 0060):
                // ended, so that the folder can go; the next drawings start the new one.
                crate::preview::server::stop_all();
                remove_earlier(cx);
            }
        });
        drop(ticking);
    })
    .detach();
    Ok(())
}

/// Deletes, in the background, the folders of earlier versions of the PlantUML Birchpad installs
/// that are still there once this one is installed: at start, and after an installation.
pub fn remove_earlier(cx: &mut App) {
    let Some(data) = AppState::global(cx).paths.user_data.clone() else {
        return;
    };
    cx.background_spawn(async move {
        for left in PLANTUML.remove_earlier(&data) {
            eprintln!("plantuml: cannot delete {} yet", left.display());
        }
    })
    .detach();
}

pub fn register_commands(registry: &mut CommandRegistry) {
    registry.workspace("preview.install-plantuml", |_, (), _, cx| install(cx));
}
