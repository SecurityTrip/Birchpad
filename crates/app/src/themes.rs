//! Choosing and importing color themes (ADR 0027): the built-in ones, and the files of the
//! `themes` folder next to the settings, Birchpad's (`.toml`) and Notepad++'s (`.xml`).

use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, anyhow};
use birchpad_theme::Theme;
use gpui_kit::{App, Context, Window};

use crate::app_state::AppState;
use crate::commands::CommandRegistry;
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
        .ok_or_else(|| anyhow!("there is no theme named {name}"))
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
    theme.with_context(|| format!("the theme {}", path.display()))
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

/// Draws everything with `theme` from now on.
pub(crate) fn apply(theme: Theme, cx: &mut App) {
    let dark = theme.dark;
    let ui = theme.ui;
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
            let name = theme_name(path)
                .ok_or_else(|| anyhow!("{} is not a theme file (.toml or .xml)", path.display()))?;
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
                &anyhow!("there is no settings folder to keep themes in"),
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
mod tests {
    use super::*;

    const DARKISH: &str = "[editor]\nbackground = '#101010'\n";

    #[test]
    fn names_list_builtins_then_files() {
        let dir = tempfile::tempdir().unwrap();
        for file in [
            "zeta.toml",
            "Alpha.xml",
            "dark.toml",
            "notes.txt",
            ".toml",
            "beta.TOML",
        ] {
            std::fs::write(dir.path().join(file), "").unwrap();
        }
        std::fs::create_dir(dir.path().join("folder.toml")).unwrap();
        assert_eq!(
            names(Some(dir.path())),
            ["Default", "Dark", "Alpha", "beta", "zeta"]
        );
        // No folder, or one that does not exist: the built-in themes.
        assert_eq!(names(None), ["Default", "Dark"]);
        assert_eq!(
            names(Some(&dir.path().join("missing"))),
            ["Default", "Dark"]
        );
    }

    #[test]
    fn load_prefers_files_then_builtins() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Mine.toml"), DARKISH).unwrap();
        std::fs::write(dir.path().join("Dark.toml"), "dark = false").unwrap();
        let mine = load(Some(dir.path()), "Mine").unwrap();
        assert!(mine.dark);
        assert_eq!(mine.name, "Mine");
        // A file named like a built-in theme replaces it.
        assert!(!load(Some(dir.path()), "Dark").unwrap().dark);
        assert!(load(None, "Dark").unwrap().dark);
        assert_eq!(load(None, "Default").unwrap(), *Theme::default_theme());
    }

    #[test]
    fn load_reports_what_is_wrong() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Broken.toml"), "[editor]\ncaret = 'red'\n").unwrap();
        std::fs::write(dir.path().join("Old.xml"), "<Other/>").unwrap();
        let broken = load(Some(dir.path()), "Broken").unwrap_err();
        assert!(format!("{broken:#}").contains("Broken.toml"), "{broken:#}");
        assert!(load(Some(dir.path()), "Old").is_err());
        assert!(load(Some(dir.path()), "Nothing").is_err());
        assert!(load(None, "").is_err());
    }

    #[test]
    fn notepad_themes_read_with_a_byte_order_mark() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Np.xml"),
            "\u{feff}<NotepadPlus><GlobalStyles><WidgetStyle name=\"Default Style\" fgColor=\"FFFFFF\" bgColor=\"000000\"/></GlobalStyles></NotepadPlus>",
        )
        .unwrap();
        let theme = load(Some(dir.path()), "Np").unwrap();
        assert!(theme.dark);
        assert_eq!(theme.name, "Np");
    }

    #[test]
    fn import_copies_themes_and_refuses_others() {
        let source = tempfile::tempdir().unwrap();
        let target = source.path().join("settings").join("themes");
        let good = source.path().join("Night.toml");
        std::fs::write(&good, DARKISH).unwrap();
        let bad = source.path().join("Bad.toml");
        std::fs::write(&bad, "[editor").unwrap();
        let other = source.path().join("readme.txt");
        std::fs::write(&other, "").unwrap();
        let missing = source.path().join("Missing.xml");
        let (imported, errors) = import(&[good, bad, other, missing], &target);
        assert_eq!(imported.len(), 1);
        assert_eq!(imported[0].name, "Night");
        assert_eq!(errors.len(), 3, "{errors:?}");
        assert_eq!(names(Some(&target)), ["Default", "Dark", "Night"]);
        // The same name in the other format replaces it.
        let xml = source.path().join("Night.xml");
        std::fs::write(&xml, "<NotepadPlus><GlobalStyles/></NotepadPlus>").unwrap();
        let (imported, errors) = import(&[xml], &target);
        assert!(errors.is_empty(), "{errors:?}");
        assert!(!imported[0].dark);
        assert!(!target.join("Night.toml").exists());
        assert!(!load(Some(&target), "Night").unwrap().dark);
        // Importing a file of the folder itself keeps it.
        let (imported, errors) = import(&[target.join("Night.xml")], &target);
        assert_eq!((imported.len(), errors.len()), (1, 0));
        assert!(target.join("Night.xml").is_file());
        // Nothing to import.
        assert_eq!(import(&[], &target), (vec![], vec![]));
    }

    #[gpui_kit::test]
    fn settings_theme_switches_and_remembers(cx: &mut gpui_kit::TestAppContext) {
        use birchpad_commands::Invocation;
        use serde_json::json;

        let (workspace, cx) = crate::workspace::tests::open_workspace(cx);
        let run = |name: &str, cx: &mut gpui_kit::VisualTestContext| {
            let invocation = Invocation::with_args("settings.theme", json!({ "name": name }));
            let result = workspace.update_in(cx, |workspace, window, cx| {
                workspace.dispatch(&invocation, window, cx)
            });
            cx.run_until_parked();
            result
        };
        run("dark", cx).unwrap();
        assert!(crate::theme::current().dark);
        assert_eq!(crate::theme::current().name, "Dark");
        cx.update(|_, cx| {
            assert_eq!(AppState::global(cx).state.theme.as_deref(), Some("Dark"));
            assert_eq!(chosen(cx), "Dark");
            assert!(gpui_kit::component::Theme::global(cx).mode.is_dark());
        });
        // An unknown theme changes nothing.
        assert!(run("No Such Theme", cx).is_err());
        assert!(run("", cx).is_err());
        assert_eq!(crate::theme::current().name, "Dark");
        run("Default", cx).unwrap();
        assert!(!crate::theme::current().dark);
        cx.update(|_, cx| assert!(!gpui_kit::component::Theme::global(cx).mode.is_dark()));
    }
}
