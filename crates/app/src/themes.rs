//! Choosing and importing color themes (ADR 0027): the built-in ones, and the files of the
//! `themes` folder next to the settings, Birchpad's (`.toml`) and Notepad++'s (`.xml`).

use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, anyhow};
use birchpad_theme::Theme;
use gpui_kit::{App, Context, Window};

use crate::app_state::AppState;
use crate::commands::{CommandRegistry, Register as _};
use crate::workspace::{Workspace, report_error};

/// The folder of theme files, next to the settings.
pub(crate) fn themes_dir(cx: &App) -> Option<PathBuf> {
    AppState::global(cx)
        .paths
        .user_config_dir()
        .map(|dir| dir.join("themes"))
}

/// Whether `path` is a theme file, and the theme's name if so.
fn theme_name(path: &Path) -> Option<String> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    if extension != "toml" && extension != "xml" {
        return None;
    }
    let name = path.file_stem()?.to_str()?;
    (!name.is_empty()).then(|| name.to_owned())
}

/// The themes to choose from: the built-in ones, then those of `dir` by name. A file named like
/// a built-in theme replaces it.
pub(crate) fn names(dir: Option<&Path>) -> Vec<String> {
    let mut names: Vec<String> = Theme::builtins()
        .iter()
        .map(|theme| theme.name.clone())
        .collect();
    let mut files: Vec<String> = dir
        .and_then(|dir| std::fs::read_dir(dir).ok())
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let entry = entry.ok()?;
            entry.file_type().ok()?.is_file().then_some(())?;
            theme_name(&entry.path())
        })
        .filter(|name| !names.iter().any(|known| known.eq_ignore_ascii_case(name)))
        .collect();
    files.sort_by_key(|name| name.to_lowercase());
    files.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    names.extend(files);
    names
}

/// Reads the theme `name`: a file of `dir` (`.toml` before `.xml`), else a built-in theme.
pub(crate) fn load(dir: Option<&Path>, name: &str) -> Result<Theme> {
    if let Some(dir) = dir {
        for extension in ["toml", "xml"] {
            let path = dir.join(format!("{name}.{extension}"));
            if path.is_file() {
                return read(&path, name);
            }
        }
    }
    Theme::builtin(name)
        .cloned()
        .ok_or_else(|| anyhow!("{}", t!("themes.there-is-no-theme-named", name = name)))
}

/// Reads a theme file, in Notepad++'s format if it is XML.
fn read(path: &Path, name: &str) -> Result<Theme> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("cannot read the theme {}", path.display()))?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let is_xml = path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("xml"));
    let theme = if is_xml {
        Theme::from_notepad_xml(name, text)
    } else {
        Theme::from_toml(name, text)
    };
    theme.with_context(|| t!("themes.the-theme", path = path.display()))
}

/// The theme chosen last, or the one the settings name.
pub(crate) fn chosen(cx: &App) -> String {
    let state = AppState::global(cx);
    state
        .state
        .theme
        .clone()
        .unwrap_or_else(|| state.settings.appearance.theme.clone())
}

/// Starts with the chosen theme; one that does not read leaves the default theme, with a
/// message.
pub(crate) fn init(cx: &mut App) {
    let name = chosen(cx);
    match load(themes_dir(cx).as_deref(), &name) {
        Ok(theme) => apply(theme, cx),
        Err(error) => {
            eprintln!("{error:#}");
            apply(Theme::default_theme().clone(), cx);
        }
    }
}

/// Whether the interface is dark, and its colors when they are not the theme's, for the theme
/// `dark` and the system `system_dark` (Preferences > Appearance > Interface).
pub(crate) fn interface(
    mode: birchpad_config::InterfaceMode,
    dark: bool,
    system_dark: bool,
) -> (bool, Option<birchpad_theme::UiColors>) {
    use birchpad_config::InterfaceMode;
    let builtin = |dark: bool| {
        let name = if dark {
            birchpad_theme::DARK
        } else {
            birchpad_theme::DEFAULT
        };
        Theme::builtin(name).map(|theme| theme.ui)
    };
    match mode {
        InterfaceMode::Theme => (dark, None),
        InterfaceMode::Light => (false, builtin(false)),
        InterfaceMode::Dark => (true, builtin(true)),
        InterfaceMode::System => (system_dark, builtin(system_dark)),
    }
}

/// Whether the system's appearance is dark.
fn system_dark(cx: &App) -> bool {
    matches!(
        cx.window_appearance(),
        gpui_kit::WindowAppearance::Dark | gpui_kit::WindowAppearance::VibrantDark
    )
}

/// Draws everything with `theme` from now on, the interface as Preferences say.
pub(crate) fn apply(theme: Theme, cx: &mut App) {
    let mode = AppState::global(cx).settings.appearance.interface;
    let (dark, interface) = interface(mode, theme.dark, system_dark(cx));
    let ui = interface.unwrap_or(theme.ui);
    crate::theme::set_interface(interface);
    crate::theme::set(theme);
    let mode = if dark {
        gpui_kit::component::ThemeMode::Dark
    } else {
        gpui_kit::component::ThemeMode::Light
    };
    gpui_kit::component::Theme::change(mode, None, cx);
    birchpad_menu_bar::set_colors(birchpad_menu_bar::Colors {
        background: ui.background.to_rgb(),
        text: ui.text.to_rgb(),
        border: ui.border.to_rgb(),
        selected: ui.selected.to_rgb(),
        hovered: ui.hovered.to_rgb(),
        muted: ui.faint.to_rgb(),
    });
    cx.refresh_windows();
}

#[derive(serde::Deserialize)]
struct ThemeArgs {
    name: String,
}

/// The theme again, after the interface setting or the system's appearance changed.
pub(crate) fn reapply(cx: &mut App) {
    apply((*crate::theme::current()).clone(), cx);
}

pub(crate) fn register_commands(registry: &mut CommandRegistry) {
    registry.workspace("settings.theme", |this, args: ThemeArgs, _, cx| {
        let theme = load(themes_dir(cx).as_deref(), &args.name)?;
        choose(this, theme, cx);
        Ok(())
    });
    registry.workspace("settings.style-configurator", |_, (), window, cx| {
        crate::style_configurator::open(cx.entity().downgrade(), window, cx);
        Ok(())
    });
    registry.workspace("settings.import-theme", |this, (), window, cx| {
        this.import_themes(window, cx);
        Ok(())
    });
}

/// Uses `theme` and remembers it.
pub(crate) fn choose(workspace: &mut Workspace, theme: Theme, cx: &mut Context<Workspace>) {
    let name = theme.name.clone();
    apply(theme, cx);
    AppState::update_state(cx, |state, _| state.theme = Some(name));
    workspace.refresh_menus(cx);
}

/// Copies theme files into `dir`, each checked first. Returns the themes imported and a message
/// for each file that is not one.
pub(crate) fn import(paths: &[PathBuf], dir: &Path) -> (Vec<Theme>, Vec<String>) {
    let mut imported = Vec::new();
    let mut errors = Vec::new();
    for path in paths {
        let result = (|| {
            let name = theme_name(path).ok_or_else(|| {
                anyhow!(
                    "{}",
                    t!(
                        "themes.is-not-a-theme-file-toml-or-xml",
                        path = path.display()
                    )
                )
            })?;
            let theme = read(path, &name)?;
            std::fs::create_dir_all(dir)
                .with_context(|| format!("cannot create {}", dir.display()))?;
            let file_name = path.file_name().expect("a theme file has a name");
            let target = dir.join(file_name);
            // The other format of the same name would come first or hide this one.
            for extension in ["toml", "xml"] {
                let other = dir.join(format!("{name}.{extension}"));
                if other != target && other.is_file() {
                    std::fs::remove_file(&other)
                        .with_context(|| format!("cannot replace {}", other.display()))?;
                }
            }
            if *path != target {
                std::fs::copy(path, &target)
                    .with_context(|| format!("cannot copy into {}", target.display()))?;
            }
            Ok::<_, anyhow::Error>(theme)
        })();
        match result {
            Ok(theme) => imported.push(theme),
            Err(error) => errors.push(format!("{error:#}")),
        }
    }
    (imported, errors)
}

impl Workspace {
    /// Settings > Import > Import Style Themes...: copies theme files into the `themes` folder
    /// and switches to the last one.
    fn import_themes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(dir) = themes_dir(cx) else {
            report_error(
                &anyhow!(
                    "{}",
                    t!("themes.there-is-no-settings-folder-to-keep-themes-in")
                ),
                window,
                cx,
            );
            return;
        };
        let start = self.default_directory(cx);
        cx.spawn_in(window, async move |this, cx| {
            let Some(paths) = crate::path_dialog::ask_open(start, cx).await else {
                return;
            };
            this.update_in(cx, |this, window, cx| {
                let (imported, errors) = import(&paths, &dir);
                for error in errors {
                    report_error(&anyhow!(error), window, cx);
                }
                if let Some(theme) = imported.into_iter().last() {
                    choose(this, theme, cx);
                }
            })
            .ok();
        })
        .detach();
    }
}

#[cfg(test)]
#[path = "../tests/unit/themes/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/unit/themes/interface_tests.rs"]
mod interface_tests;
