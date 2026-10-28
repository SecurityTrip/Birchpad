//! The workspace's commands, and the registration of every part of the window's.

use crate::app_state::AppState;
use crate::commands::{CommandRegistry, Register as _};
use crate::editor::ViewSettings;

pub fn register_commands(registry: &mut CommandRegistry) {
    registry.workspace("file.new", |this, (), window, cx| {
        this.new_file(window, cx);
        Ok(())
    });
    registry.workspace("view.next-tab", |this, (), window, cx| {
        this.active_pane()
            .update(cx, |pane, cx| pane.cycle(1, window, cx));
        Ok(())
    });
    registry.workspace("view.previous-tab", |this, (), window, cx| {
        this.active_pane()
            .update(cx, |pane, cx| pane.cycle(-1, window, cx));
        Ok(())
    });
    for &(id, read, write) in crate::editor::VIEW_SWITCHES {
        registry.workspace(id, move |this, (), _, cx| {
            let on = !read(&ViewSettings::read(cx));
            AppState::update_state(cx, |state, _| write(state, on));
            this.refresh_views(cx);
            Ok(())
        });
    }
    registry.workspace("view.move-to-other-view", |this, (), window, cx| {
        this.send_to_other_view(false, window, cx);
        Ok(())
    });
    registry.workspace("view.clone-to-other-view", |this, (), window, cx| {
        this.send_to_other_view(true, window, cx);
        Ok(())
    });
    registry.workspace("view.focus-other-view", |this, (), window, cx| {
        let other = 1 - this.active_pane;
        if let Some(view) = this.panes[other].read(cx).active_item().cloned() {
            this.activate_view(&view, window, cx);
        }
        Ok(())
    });
    registry.workspace("view.rotate-split", |_, (), _, cx| {
        AppState::update_state(cx, |state, _| state.split = state.split.rotated());
        cx.notify();
        Ok(())
    });
    registry.workspace("view.sync-vertical-scroll", |this, (), _, cx| {
        this.sync_vertical = !this.sync_vertical;
        this.refresh_menus(cx);
        Ok(())
    });
    registry.workspace("view.sync-horizontal-scroll", |this, (), _, cx| {
        this.sync_horizontal = !this.sync_horizontal;
        this.refresh_menus(cx);
        Ok(())
    });
    // Checked when both are shown; turns both on, or both off if they are.
    registry.workspace("view.show-all-characters", |this, (), _, cx| {
        let on = !crate::editor::all_characters(&ViewSettings::read(cx));
        AppState::update_state(cx, |state, _| {
            state.show_whitespace = Some(on);
            state.show_eol = Some(on);
        });
        this.refresh_views(cx);
        Ok(())
    });
    for (id, step) in [
        ("view.zoom-in", 1),
        ("view.zoom-out", -1),
        ("view.zoom-reset", 0),
    ] {
        registry.workspace(id, move |this, (), _, cx| {
            let range = crate::editor::ZOOM_RANGE;
            let zoomed = |zoom: i32| {
                if step == 0 {
                    0
                } else {
                    (zoom + step).clamp(*range.start(), *range.end())
                }
            };
            let state = &AppState::global(cx).state;
            if state.zoom_per_view {
                let shared = state.zoom;
                if let Some(view) = this.active_view(cx) {
                    view.update(cx, |view, cx| {
                        view.zoom = Some(zoomed(view.zoom.unwrap_or(shared)));
                        view.settings_changed(cx);
                    });
                }
                return Ok(());
            }
            AppState::update_state(cx, |state, _| state.zoom = zoomed(state.zoom));
            this.refresh_views(cx);
            Ok(())
        });
    }
    // Back on, every view takes the shared zoom again.
    registry.workspace("view.zoom-sync", |this, (), _, cx| {
        let per_view = !AppState::global(cx).state.zoom_per_view;
        AppState::update_state(cx, |state, _| state.zoom_per_view = per_view);
        if !per_view {
            for view in this.all_views(cx) {
                view.update(cx, |view, _| view.zoom = None);
            }
        }
        this.refresh_views(cx);
        this.refresh_menus(cx);
        Ok(())
    });
    registry.workspace("language.set", |this, args: LanguageArgs, _, cx| {
        if let Some(name) = args.language.strip_prefix(crate::user_languages::ID_PREFIX) {
            let found = crate::user_languages::find(name, cx).ok_or_else(|| {
                anyhow::anyhow!(
                    "{}",
                    t!("workspace.there-is-no-user-defined-language", name = name)
                )
            })?;
            if let Some(view) = this.active_view(cx) {
                let buffer = view.read(cx).buffer.clone();
                buffer.update(cx, |buffer, cx| buffer.set_user_language(Some(found), cx));
            }
            return Ok(());
        }
        let language =
            match args.language.as_str() {
                "text" | "normal" => None,
                id => Some(birchpad_syntax::by_id(id).ok_or_else(|| {
                    anyhow::anyhow!("{}", t!("workspace.unknown-language", id = id))
                })?),
            };
        if let Some(view) = this.active_view(cx) {
            let buffer = view.read(cx).buffer.clone();
            buffer.update(cx, |buffer, cx| buffer.set_language(language, cx));
        }
        // Its tab settings may differ.
        this.refresh_views(cx);
        Ok(())
    });
    crate::encoding_ui::register_commands(registry);
    crate::file_ops::register_commands(registry);
    crate::find::register_commands(registry);
    crate::finder::register_commands(registry);
    crate::compare::register_commands(registry);
    crate::help::register_commands(registry);
    crate::hex_view::register_commands(registry);
    crate::incremental::register_commands(registry);
    crate::json_xml::register_commands(registry);
    crate::online::register_commands(registry);
    crate::navigation::register_commands(registry);
    crate::panels::register_commands(registry);
    crate::session::register_commands(registry);
    crate::extensions::scripts::register_commands(registry);
    crate::extensions::plugins::register_commands(registry);
    crate::extensions::admin::register_commands(registry);
    crate::spell_check::register_commands(registry);
    crate::disk::register_commands(registry);
    crate::themes::register_commands(registry);
    crate::preferences::register_commands(registry);
    crate::shortcut_mapper::register_commands(registry);
    crate::macros::register_commands(registry);
    crate::tools::register_commands(registry);
    crate::run_menu::register_commands(registry);
    crate::user_languages::register_commands(registry);
    crate::print::register_commands(registry);
    crate::pane::register_commands(registry);
    crate::palette::register_commands(registry);
    crate::preview::register_commands(registry);
    crate::plantuml::register_commands(registry);
    crate::window_menu::register_commands(registry);
    crate::file_commands::register_commands(registry);
    crate::window_modes::register_commands(registry);
    crate::date_time::register_commands(registry);
    crate::edit_extras::register_commands(registry);
    crate::context_menu::register_commands(registry);
    crate::search_extras::register_commands(registry);
    crate::view_extras::register_commands(registry);
    crate::default_app::register_commands(registry);
}

#[derive(serde::Deserialize)]
pub(super) struct LanguageArgs {
    language: String,
}
