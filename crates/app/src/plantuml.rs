//! PlantUML that Birchpad installs (ADR 0045): when no PlantUML is found, the preview of a
//! PlantUML document offers to install it; one click downloads PlantUML and the Java that runs it
//! for this platform (`birchpad_update::component::PLANTUML`), through the transport of updates,
//! and every PlantUML preview draws with it from then on.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Result, anyhow};
use birchpad_update::component::{self, ComponentPackage, PLANTUML};
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

/// The folder of the PlantUML Birchpad installed, if it did.
pub fn installed(cx: &App) -> Option<PathBuf> {
    let data = AppState::global(cx).paths.user_data.clone()?;
    PLANTUML.installed(&data)
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
        cx.update(|cx| {
            let install = cx.global_mut::<PlantUmlInstall>();
            install.running = false;
            install.error = result.err();
            install.generation += 1;
            cx.refresh_windows();
        });
        drop(ticking);
    })
    .detach();
    Ok(())
}

pub fn register_commands(registry: &mut CommandRegistry) {
    registry.workspace("preview.install-plantuml", |_, (), _, cx| install(cx));
}
