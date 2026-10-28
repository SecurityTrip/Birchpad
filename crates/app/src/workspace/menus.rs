//! The menus rebuilt from the state of the window and of the active document.

use birchpad_commands::Invocation;
use gpui_kit::Context;

use crate::app_state::AppState;
use crate::editor::ViewSettings;
use crate::menus::{self, MenuState};

use super::Workspace;

impl Workspace {
    /// Rebuilds the menus from the current state.
    pub fn refresh_menus(&mut self, cx: &mut Context<Self>) {
        let format = self
            .active_view(cx)
            .map(|view| view.read(cx).buffer.read(cx).doc().format());
        let language = self.active_view(cx).map(|view| {
            let buffer = view.read(cx).buffer.read(cx);
            match (buffer.language(), buffer.user_language()) {
                (Some(language), _) => language.id.to_owned(),
                (None, Some(found)) => {
                    format!("{}{}", crate::user_languages::ID_PREFIX, found.name)
                }
                (None, None) => "text".to_owned(),
            }
        });
        let ansi = AppState::global(cx).ansi;
        let view = ViewSettings::read(cx);
        let monitoring = self
            .active_view(cx)
            .is_some_and(|view| view.read(cx).buffer.read(cx).is_monitoring());
        let theme_name = crate::theme::current().name.clone();
        let compare_options = crate::compare::options(cx);
        let spell_enabled = crate::spell_check::is_enabled(cx);
        let spell_language = crate::spell_check::language(cx);
        let tab_marks = self.active_view(cx).map(|view| view.read(cx).tab);
        let preview_mode = self.active_pane().read(cx).preview_mode(cx);
        let requested_read_only = self
            .active_view(cx)
            .is_some_and(|view| view.read(cx).buffer.read(cx).requested_read_only());
        let file_read_only = self.active_view(cx).is_some_and(|view| {
            view.read(cx).buffer.read(cx).read_only() == Some(crate::buffer::ReadOnly::File)
        });
        let zoom_sync = !AppState::global(cx).state.zoom_per_view;
        let close_tags = AppState::global(cx).settings.completion.close_tags;
        let hex_shown = self.active_hex_editor(cx).is_some();
        let white_diagrams =
            crate::preview::background(cx) == birchpad_config::DiagramBackground::White;
        let checked = |invocation: &Invocation| {
            if let Some(on) = crate::pane::is_checked(invocation, tab_marks) {
                return on;
            }
            if let Some(on) = crate::preview::is_checked(&invocation.command, preview_mode) {
                return on;
            }
            if let Some(on) = crate::window_modes::is_checked(&invocation.command, self.modes) {
                return on;
            }
            if invocation.command == "edit.toggle-read-only" {
                return requested_read_only;
            }
            if invocation.command == "edit.toggle-read-only-attribute" {
                return file_read_only;
            }
            if invocation.command == "view.zoom-sync" {
                return zoom_sync;
            }
            if invocation.command == "preview.white-background" {
                return white_diagrams;
            }
            if let Some((_, read, _)) = crate::editor::VIEW_SWITCHES
                .iter()
                .find(|(id, _, _)| *id == invocation.command)
            {
                return read(&view);
            }
            if invocation.command == "view.show-all-characters" {
                return crate::editor::all_characters(&view);
            }
            if invocation.command == "view.monitoring" {
                return monitoring;
            }
            if invocation.command == "view.sync-vertical-scroll" {
                return self.sync_vertical;
            }
            if invocation.command == "view.sync-horizontal-scroll" {
                return self.sync_horizontal;
            }
            if invocation.command == "settings.theme" {
                return invocation
                    .args
                    .get("name")
                    .and_then(|name| name.as_str())
                    .is_some_and(|name| name.eq_ignore_ascii_case(&theme_name));
            }
            if invocation.command == "language.set" {
                return invocation.args.get("language").and_then(|id| id.as_str())
                    == language.as_deref();
            }
            if let Some(on) = crate::compare::is_checked(invocation, compare_options) {
                return on;
            }
            if let Some(on) =
                crate::spell_check::is_checked(invocation, spell_enabled, &spell_language)
            {
                return on;
            }
            if let Some(open) = crate::panels::is_checked(&self.docks, invocation) {
                return open;
            }
            if invocation.command == "xml.tag-auto-close" {
                return close_tags;
            }
            if invocation.command == "hex.editor" {
                return hex_shown;
            }
            let Some(format) = format else {
                return false;
            };
            crate::encoding_ui::is_current(invocation, format, ansi)
        };
        let toolbar_checked: Vec<String> =
            crate::toolbar::items(&AppState::global(cx).settings.toolbar)
                .into_iter()
                .filter_map(|item| match item {
                    crate::toolbar::Item::Button(id) => {
                        checked(&Invocation::new(&id)).then_some(id)
                    }
                    crate::toolbar::Item::Separator => None,
                })
                .collect();
        let recent_files = AppState::global(cx).state.recent_files.clone();
        let themes = crate::themes::names(crate::themes::themes_dir(cx).as_deref());
        let macros: Vec<String> = cx
            .try_global::<crate::macros::Macros>()
            .map(|macros| {
                macros
                    .saved
                    .iter()
                    .map(|found| found.name.clone())
                    .collect()
            })
            .unwrap_or_default();
        let run_commands: Vec<String> = cx
            .try_global::<crate::run_menu::RunMenu>()
            .map(|menu| menu.saved.iter().map(|entry| entry.name.clone()).collect())
            .unwrap_or_default();
        let user_languages = crate::user_languages::names(cx);
        let spell_languages = crate::spell_check::languages(cx);
        let scripts = crate::extensions::scripts::scripts(cx);
        let plugin_menus = crate::extensions::plugins::menus(cx);
        let (documents, active_document) = crate::window_menu::menu_documents(self, cx);
        let state = MenuState {
            documents: &documents,
            active_document,
            user_languages: &user_languages,
            spell_languages: &spell_languages,
            scripts: &scripts,
            plugins: &plugin_menus,
            recent_files: &recent_files,
            themes: &themes,
            macros: &macros,
            run_commands: &run_commands,
            checked: &checked,
        };
        menus::install(&state, cx);
        self.toolbar_checked = toolbar_checked;
        if let Some(menu_bar) = &self.menu_bar {
            menu_bar.update(cx, |menu_bar, cx| menu_bar.reload(cx));
        }
    }
}
