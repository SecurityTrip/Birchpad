//! Plugins > Scripts (ADR 0037): the `.rhai` files of the `scripts` folder next to the
//! settings, run over the active document.

use std::path::PathBuf;

use anyhow::{Context as _, anyhow, bail};
use gpui_kit::App;
use serde::Deserialize;

use super::PanelOwner;
use crate::app_state::AppState;
use crate::commands::{CommandRegistry, Register as _};

/// The extension of scripts.
const EXTENSION: &str = "rhai";

pub fn scripts_dir(cx: &App) -> Option<PathBuf> {
    AppState::global(cx)
        .paths
        .user_config_dir()
        .map(|dir| dir.join("scripts"))
}

/// The scripts' names (their files without `.rhai`), sorted.
pub fn names(cx: &App) -> Vec<String> {
    let Some(dir) = scripts_dir(cx) else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            if !path.is_file() || path.extension()? != EXTENSION {
                return None;
            }
            Some(path.file_stem()?.to_str()?.to_owned())
        })
        .collect();
    names.sort_by_key(|name| name.to_lowercase());
    names
}

#[derive(Deserialize)]
struct RunArgs {
    name: String,
}

pub fn register_commands(registry: &mut CommandRegistry) {
    registry.workspace("script.run", |this, args: RunArgs, window, cx| {
        if !AppState::global(cx).settings.plugins.scripts {
            bail!(
                "{}",
                t!("extensions.scripts.scripts-are-turned-off-by-your-administrator")
            );
        }
        let name = args.name;
        if name.is_empty() || name.contains(['/', '\\']) || name.starts_with('.') {
            bail!(
                "{}",
                t!(
                    "extensions.scripts.is-not-a-script-s-name",
                    name = format!("{:?}", name)
                )
            );
        }
        let dir = scripts_dir(cx)
            .ok_or_else(|| anyhow!("{}", t!("extensions.scripts.there-is-no-settings-folder")))?;
        let path = dir.join(format!("{name}.{EXTENSION}"));
        let source = std::fs::read_to_string(&path)
            .with_context(|| format!("cannot read the script {}", path.display()))?;
        let (view, session) = super::session(this, cx);
        let outcome = super::nested(|| birchpad_plugin::script::run(&name, &source, session))?;
        this.apply_outcome(view.as_ref(), PanelOwner::Script(name), outcome, window, cx)
    });
    registry.workspace("script.open-folder", |this, (), _, cx| {
        let dir = scripts_dir(cx)
            .ok_or_else(|| anyhow!("{}", t!("extensions.scripts.there-is-no-settings-folder")))?;
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("cannot create {}", dir.display()))?;
        cx.open_with_system(&dir);
        this.refresh_menus(cx);
        Ok(())
    });
    registry.workspace("script.reload", |this, (), _, cx| {
        this.refresh_menus(cx);
        Ok(())
    });
}
