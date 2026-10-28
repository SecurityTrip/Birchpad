//! The online catalog in the window (ADR 0051): Plugins Admin's list of plugins to install and
//! update, and the dictionaries Plugins > Spell Checker > Download Languages... offers. Nothing
//! is read until the user asks, and nothing while the `OnlineFeatures` policy forbids it;
//! downloads go through the transport of updates (its proxy too) and are checked against the
//! catalog's sizes and SHA-256 before they are kept.

use std::path::PathBuf;

use anyhow::{Result, anyhow, bail};
use birchpad_update::catalog::{self, Catalog};
use gpui_kit::component::Sizable as _;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::{App, AppContext as _, Context, Global, WeakEntity, Window, div, prelude::*, px};
use serde::Deserialize;

use crate::app_state::AppState;
use crate::commands::{CommandRegistry, Register as _};
use crate::workspace::{Workspace, report_error};

/// The catalog read last, and what is being done with it.
#[derive(Default)]
pub struct OnlineCatalog {
    pub catalog: Option<Catalog>,
    /// Being read.
    pub loading: bool,
    /// Why it could not be read, or what failed last.
    pub error: Option<String>,
    /// The plugins and dictionaries being downloaded.
    pub busy: Vec<String>,
}

impl Global for OnlineCatalog {}

fn state(cx: &mut App) -> &mut OnlineCatalog {
    if !cx.has_global::<OnlineCatalog>() {
        cx.set_global(OnlineCatalog::default());
    }
    cx.global_mut::<OnlineCatalog>()
}

/// The catalog read last, if any.
pub fn catalog(cx: &App) -> Option<&Catalog> {
    cx.try_global::<OnlineCatalog>()?.catalog.as_ref()
}

/// The catalog's address: `network.catalog`, or Birchpad's own.
pub fn catalog_url(cx: &App) -> String {
    let own = AppState::global(cx)
        .settings
        .network
        .catalog
        .trim()
        .to_owned();
    if own.is_empty() {
        catalog::DEFAULT_CATALOG.to_owned()
    } else {
        own
    }
}

/// Whether the administrator's policy lets Birchpad go online for the catalog.
fn allowed(cx: &App) -> Result<()> {
    if !AppState::global(cx).settings.network.online_features {
        bail!("{}", t!("online.turned-off"));
    }
    Ok(())
}

fn transport(cx: &App) -> Result<std::sync::Arc<dyn birchpad_update::Transport>> {
    crate::updates::Updates::transport(cx).ok_or_else(|| anyhow!("{}", t!("online.no-network")))
}

#[derive(Deserialize)]
struct PluginArgs {
    plugin: String,
}

#[derive(Deserialize)]
struct LanguageArgs {
    language: String,
}

pub fn register_commands(registry: &mut CommandRegistry) {
    registry.workspace("catalog.refresh", |_, (), window, cx| refresh(window, cx));
    registry.workspace(
        "plugin.install-online",
        |_, args: PluginArgs, window, cx| install_plugin(&args.plugin, window, cx),
    );
    registry.workspace("spell.download", |_, args: LanguageArgs, window, cx| {
        download_dictionary(&args.language, window, cx)
    });
    registry.workspace("spell.download-languages", |_, (), window, cx| {
        open_dictionaries(cx.entity().downgrade(), window, cx);
        if catalog(cx).is_none() {
            // The list fills once it has been read; why not, if it cannot be.
            if let Err(error) = refresh(window, cx) {
                report_error(&error, window, cx);
            }
        }
        Ok(())
    });
}

/// Reads the catalog again, in the background.
pub fn refresh(window: &mut Window, cx: &mut Context<Workspace>) -> Result<()> {
    allowed(cx)?;
    let transport = transport(cx)?;
    let url = catalog_url(cx);
    let online = state(cx);
    if online.loading {
        return Ok(());
    }
    online.loading = true;
    online.error = None;
    let fetching = cx.background_spawn(async move { catalog::fetch(transport.as_ref(), &url) });
    cx.spawn_in(window, async move |_, cx| {
        let result = fetching.await;
        cx.update(|window, cx| {
            let online = state(cx);
            online.loading = false;
            match result {
                Ok(catalog) => online.catalog = Some(catalog),
                Err(error) => {
                    online.error = Some(error.clone());
                    report_error(&anyhow!(error), window, cx);
                }
            }
            cx.refresh_windows();
        })
        .ok();
    })
    .detach();
    cx.refresh_windows();
    Ok(())
}

/// Installs or updates the plugin `id` from the catalog, in the background.
fn install_plugin(id: &str, window: &mut Window, cx: &mut Context<Workspace>) -> Result<()> {
    allowed(cx)?;
    let entry = catalog(cx)
        .and_then(|catalog| catalog.plugins.iter().find(|plugin| plugin.id == id))
        .cloned()
        .ok_or_else(|| anyhow!("{}", t!("online.no-such-plugin", id = id)))?;
    if entry.api != birchpad_plugin::API_VERSION {
        bail!(
            "{}",
            t!(
                "online.other-api",
                id = id,
                api = entry.api,
                known = birchpad_plugin::API_VERSION
            )
        );
    }
    let settings = AppState::global(cx).settings.plugins.clone();
    birchpad_plugin::manager::permitted(id, &settings).map_err(|why| anyhow!(why))?;
    let dir = crate::extensions::plugins::plugins_dir(cx)
        .ok_or_else(|| anyhow!("{}", t!("extensions.admin.there-is-no-settings-folder")))?;
    let transport = transport(cx)?;
    let busy = format!("plugin:{id}");
    if state(cx).busy.contains(&busy) {
        return Ok(());
    }
    state(cx).busy.push(busy.clone());
    let work = dir.join(format!(".{id}.download"));
    let fetching = {
        let work = work.clone();
        cx.background_spawn(async move {
            catalog::fetch_plugin(transport.as_ref(), &entry, &work, &|_| {})
        })
    };
    let id = id.to_owned();
    cx.spawn_in(window, async move |this, cx| {
        let fetched = fetching.await;
        this.update_in(cx, |this, window, cx| {
            state(cx).busy.retain(|known| *known != busy);
            let result = fetched
                .map_err(|error| anyhow!(error))
                .and_then(|manifest| {
                    let installed = birchpad_plugin::manager::install(&manifest, &dir, &settings)?;
                    if installed.manifest.id != id {
                        birchpad_plugin::manager::uninstall(&dir, &installed.manifest.id).ok();
                        bail!("{}", t!("online.not-the-plugin-listed", id = id));
                    }
                    Ok(installed)
                });
            std::fs::remove_dir_all(&work).ok();
            this.load_plugins(window, cx);
            match result {
                Ok(installed) => crate::workspace::report_success(
                    format!(
                        "{} {} {}",
                        t!("extensions.admin.installed"),
                        installed.manifest.name,
                        installed.manifest.version
                    ),
                    window,
                    cx,
                ),
                Err(error) => {
                    state(cx).error = Some(format!("{error:#}"));
                    report_error(&error, window, cx);
                }
            }
            cx.refresh_windows();
        })
        .ok();
    })
    .detach();
    Ok(())
}

/// Downloads the dictionary of `language` from the catalog into the dictionaries folder, in
/// the background; it is kept only if it reads.
fn download_dictionary(
    language: &str,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> Result<()> {
    allowed(cx)?;
    let entry = catalog(cx)
        .and_then(|catalog| {
            catalog
                .dictionaries
                .iter()
                .find(|dictionary| dictionary.language == language)
        })
        .cloned()
        .ok_or_else(|| anyhow!("{}", t!("online.no-such-dictionary", language = language)))?;
    let folder: PathBuf = crate::spell_check::dictionaries_dir(cx)
        .ok_or_else(|| anyhow!("{}", t!("spell-check.there-is-no-settings-folder")))?;
    let transport = transport(cx)?;
    let busy = format!("dictionary:{language}");
    if state(cx).busy.contains(&busy) {
        return Ok(());
    }
    state(cx).busy.push(busy.clone());
    let fetching = cx.background_spawn(async move {
        let (aff, dic) = catalog::fetch_dictionary(transport.as_ref(), &entry, &folder, &|_| {})?;
        // A dictionary that does not read is not kept.
        if let Err(error) = birchpad_tools::spell::Speller::open(&aff, &dic) {
            std::fs::remove_file(&aff).ok();
            std::fs::remove_file(&dic).ok();
            return Err(error.to_string());
        }
        Ok(entry.name)
    });
    cx.spawn_in(window, async move |this, cx| {
        let fetched = fetching.await;
        this.update_in(cx, |this, window, cx| {
            state(cx).busy.retain(|known| *known != busy);
            match fetched {
                Ok(name) => {
                    crate::spell_check::forget(cx);
                    this.refresh_menus(cx);
                    crate::workspace::report_success(
                        t!("online.dictionary-ready", name = name),
                        window,
                        cx,
                    );
                }
                Err(error) => {
                    state(cx).error = Some(error.clone());
                    report_error(&anyhow!(error), window, cx);
                }
            }
            cx.refresh_windows();
        })
        .ok();
    })
    .detach();
    Ok(())
}

/// A line that says how the catalog stands: being read, why it could not be, or nothing.
pub fn status(cx: &App) -> Option<String> {
    let online = cx.try_global::<OnlineCatalog>()?;
    if online.loading {
        return Some(t!("online.loading"));
    }
    if !online.busy.is_empty() {
        return Some(t!("online.downloading"));
    }
    online.error.clone()
}

/// Download Languages...: the catalog's dictionaries, each with Download (or Download Again).
pub struct Dictionaries {
    workspace: WeakEntity<Workspace>,
}

fn open_dictionaries(workspace: WeakEntity<Workspace>, window: &mut Window, cx: &mut App) {
    let content = cx.new(|_| Dictionaries {
        workspace: workspace.clone(),
    });
    window.open_dialog(cx, move |dialog, _, _| {
        dialog
            .title(t!("online.download-languages"))
            .w(px(520.))
            .child(content.clone())
            .footer(DialogFooter::new().child(crate::workspace::dialog_close("common.close")))
    });
}

impl Render for Dictionaries {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = crate::theme::ui();
        let have = crate::spell_check::languages(cx);
        let rows: Vec<_> = catalog(cx)
            .map(|catalog| catalog.dictionaries.clone())
            .unwrap_or_default()
            .into_iter()
            .enumerate()
            .map(|(index, dictionary)| {
                let installed = have.contains(&dictionary.language);
                let language = dictionary.language.clone();
                let workspace = self.workspace.clone();
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .py_1()
                    .child(
                        div()
                            .flex_1()
                            .child(format!("{} ({})", dictionary.name, dictionary.language)),
                    )
                    .child(
                        Button::new(("dictionary-download", index))
                            .debug_selector(move || format!("dictionary-download-{index}"))
                            .small()
                            .when(installed, |button| button.ghost())
                            .label(if installed {
                                t!("online.download-again")
                            } else {
                                t!("online.download")
                            })
                            .on_click(move |_, window, cx| {
                                let invocation = birchpad_commands::Invocation::with_args(
                                    "spell.download",
                                    serde_json::json!({ "language": language }),
                                );
                                workspace
                                    .update(cx, |workspace, cx| {
                                        workspace.run(&invocation, window, cx)
                                    })
                                    .ok();
                            }),
                    )
            })
            .collect();
        let empty = rows.is_empty();
        div()
            .flex()
            .flex_col()
            .gap_1()
            .max_h(px(420.))
            .when_some(status(cx), |list, status| {
                list.child(
                    div()
                        .text_color(crate::theme::paint(colors.muted))
                        .child(status),
                )
            })
            .when(empty, |list| {
                list.child(
                    div()
                        .text_color(crate::theme::paint(colors.muted))
                        .child(t!("online.nothing-listed")),
                )
            })
            .child(
                div()
                    .id("dictionary-rows")
                    .flex()
                    .flex_col()
                    .overflow_y_scroll()
                    .children(rows),
            )
    }
}
