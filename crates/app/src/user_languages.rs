//! Notepad++'s User Defined Languages (ADR 0032): the files of `userDefineLangs` next to the
//! settings, listed in Language > User Defined Language, chosen for files by their extension,
//! and imported with Language > User Defined Language > Import....

use std::path::{Path, PathBuf};
use std::rc::Rc;

use anyhow::{Context as _, anyhow};
use birchpad_syntax::{UdlKind, UdlStyle, UserLanguage};
use gpui_kit::{App, Context, Global, Window};

use crate::app_state::AppState;
use crate::commands::CommandRegistry;
use crate::editor::theme::TextStyle;
use crate::workspace::{Workspace, report_error};

/// The prefix of a user defined language's id in `language.set` and sessions: `udl:Name`.
pub(crate) const ID_PREFIX: &str = birchpad_config::USER_LANGUAGE_PREFIX;

/// The user defined languages read at start or imported since.
#[derive(Default)]
pub(crate) struct UserLanguages(pub(crate) Vec<Rc<UserLanguage>>);

impl Global for UserLanguages {}

/// The folder of UDL files.
pub(crate) fn folder(cx: &App) -> Option<PathBuf> {
    AppState::global(cx)
        .paths
        .user_config_dir()
        .map(|dir| dir.join("userDefineLangs"))
}

/// Every language of the `.xml` files of `dir`, by name; a later one of the same name is left
/// out.
pub(crate) fn read_folder(dir: &Path) -> Vec<Rc<UserLanguage>> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("xml"))
        })
        .collect();
    paths.sort();
    let mut languages: Vec<Rc<UserLanguage>> = Vec::new();
    for path in paths {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        match birchpad_syntax::parse_udl(&text) {
            Ok(found) => {
                for language in found {
                    if !languages.iter().any(|known| known.name == language.name) {
                        languages.push(Rc::new(language));
                    }
                }
            }
            Err(error) => eprintln!("{}: {error}", path.display()),
        }
    }
    languages.sort_by_key(|language| language.name.to_lowercase());
    languages
}

pub(crate) fn init(cx: &mut App) {
    let languages = folder(cx).map(|dir| read_folder(&dir)).unwrap_or_default();
    cx.set_global(UserLanguages(languages));
}

fn all(cx: &App) -> &[Rc<UserLanguage>] {
    cx.try_global::<UserLanguages>()
        .map_or(&[], |found| &found.0)
}

/// The user defined language named `name`.
pub(crate) fn find(name: &str, cx: &App) -> Option<Rc<UserLanguage>> {
    all(cx)
        .iter()
        .find(|language| language.name == name)
        .cloned()
}

/// The user defined language of a file named `name`, by its extension.
pub(crate) fn for_file(name: &str, cx: &App) -> Option<Rc<UserLanguage>> {
    all(cx)
        .iter()
        .find(|language| language.matches_file(name))
        .cloned()
}

/// The names the Language menu lists.
pub(crate) fn names(cx: &App) -> Vec<String> {
    all(cx)
        .iter()
        .map(|language| language.name.clone())
        .collect()
}

/// How text of `kind` looks, from the language's own colors.
pub(crate) fn text_style(style: UdlStyle) -> TextStyle {
    let rgba = |rgb: u32| (rgb << 8) | 0xff;
    TextStyle {
        color: style.color.map(rgba),
        background: style.background.map(rgba),
        bold: style.bold,
        italic: style.italic,
        underline: style.underline,
    }
}

/// The style of each piece `language` colors.
pub(crate) fn style_of(language: &UserLanguage, kind: UdlKind) -> Option<TextStyle> {
    language.style(kind).map(text_style)
}

/// Copies UDL files into `dir` after checking them; the languages they hold, and a message for
/// each file that is not one.
pub(crate) fn import(paths: &[PathBuf], dir: &Path) -> (usize, Vec<String>) {
    let mut imported = 0;
    let mut errors = Vec::new();
    for path in paths {
        let result = (|| {
            let text = std::fs::read_to_string(path)
                .with_context(|| format!("cannot read {}", path.display()))?;
            let found =
                birchpad_syntax::parse_udl(&text).with_context(|| path.display().to_string())?;
            std::fs::create_dir_all(dir)
                .with_context(|| format!("cannot create {}", dir.display()))?;
            let name = path.file_name().ok_or_else(|| {
                anyhow!(
                    "{}",
                    t!("user-languages.has-no-name", path = path.display())
                )
            })?;
            let target = dir.join(name);
            if *path != target {
                std::fs::copy(path, &target)
                    .with_context(|| format!("cannot copy into {}", target.display()))?;
            }
            Ok::<_, anyhow::Error>(found.len())
        })();
        match result {
            Ok(count) => imported += count,
            Err(error) => errors.push(format!("{error:#}")),
        }
    }
    (imported, errors)
}

pub(crate) fn register_commands(registry: &mut CommandRegistry) {
    registry.workspace("language.import-udl", |this, (), window, cx| {
        this.import_user_languages(window, cx);
        Ok(())
    });
}

impl Workspace {
    fn import_user_languages(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(dir) = folder(cx) else {
            report_error(
                &anyhow!(
                    "{}",
                    t!("user-languages.there-is-no-settings-folder-to-keep-languages-in")
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
                let (_, errors) = import(&paths, &dir);
                for error in errors {
                    report_error(&anyhow!(error), window, cx);
                }
                init(cx);
                this.refresh_menus(cx);
            })
            .ok();
        })
        .detach();
    }
}

#[cfg(test)]
mod tests;
