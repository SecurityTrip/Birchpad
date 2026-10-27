//! The workspace: the contents of a main window. It owns the two panes of split view, routes
//! commands and draws the menu bar and the status bar.

use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use birchpad_cli::CommandLine;
use birchpad_commands::Invocation;
use birchpad_config::SplitOrientation;
use birchpad_core::Document;
use birchpad_menu_bar::MenuBar;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{IconName, Sizable as _};
use gpui_kit::component::{h_resizable, resizable_panel, v_resizable};
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, EntityId, ExternalPaths, FocusHandle,
    Focusable, ModifiersChangedEvent, Subscription, Window, div, prelude::*, px, relative,
};

use crate::app_state::AppState;
use crate::buffer::{Buffer, BufferEvent};
use crate::commands::{CommandRegistry, Register as _, RunCommand};
use crate::disk::DiskState;
pub use crate::editor::{EditorEvent, EditorView, ViewSettings};
use crate::find::FindBar;
use crate::incremental::IncrementalBar;
use crate::menus::{self, MenuState};
use crate::navigation::{History, Place};
use crate::pane::{Pane, PaneEvent};
use crate::panels::Docks;
use crate::search_results::SearchResults;
use crate::session::SessionState;
use crate::status_bar::StatusInfo;

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
struct LanguageArgs {
    language: String,
}

/// The format and language of a new document: Preferences > New Document. Names it does not
/// know give UTF-8, the platform's line ending and plain text.
pub fn new_document(
    cx: &App,
) -> (
    birchpad_core::Format,
    Option<&'static birchpad_syntax::Language>,
) {
    let app = AppState::global(cx);
    let settings = &app.settings.new_document;
    let mut format = birchpad_core::Format::new();
    if let Some((encoding, bom)) = birchpad_io::parse_encoding(&settings.encoding, app.ansi) {
        format.encoding = encoding;
        format.bom = bom && encoding.is_unicode();
    }
    format.line_ending = match settings.line_ending.trim().to_ascii_lowercase().as_str() {
        "crlf" | "windows" => birchpad_core::LineEnding::CrLf,
        "lf" | "unix" => birchpad_core::LineEnding::Lf,
        "cr" | "mac" => birchpad_core::LineEnding::Cr,
        _ => format.line_ending,
    };
    let language = birchpad_syntax::by_id(settings.language.trim());
    (format, language)
}

pub use birchpad_ui::{
    dialog_action, dialog_close, report_error, report_info, report_success, report_warning,
};

pub struct Workspace {
    focus_handle: FocusHandle,
    /// Notepad++'s main and second views. A pane without tabs is hidden; split view shows both.
    pub panes: [Entity<Pane>; 2],
    pub active_pane: usize,
    /// View > Synchronize Vertical / Horizontal Scrolling.
    sync_vertical: bool,
    sync_horizontal: bool,
    menu_bar: Option<Entity<MenuBar>>,
    pub find_bar: Entity<FindBar>,
    pub incremental: Entity<IncrementalBar>,
    pub search_results: Entity<SearchResults>,
    /// The inactive tab the mouse is over, which the Document Map shows (MISC > Document
    /// Peeker).
    pub peeked: Option<gpui_kit::WeakEntity<EditorView>>,
    title: String,
    /// `-titleAdd`: text after the window's title.
    pub title_add: Option<String>,
    /// `-quickPrint`: the printings of the files, for a launch that quits after them.
    pub quick_prints: Vec<gpui_kit::Task<()>>,
    buffer_subscriptions: HashMap<EntityId, Subscription>,
    /// Focus and scroll events of each view, and its changes for the navigation history.
    view_subscriptions: HashMap<EntityId, [Subscription; 4]>,
    pub session_state: SessionState,
    pub disk_state: DiskState,
    /// Go Back and Go Forward.
    pub navigation: History<Place>,
    /// The side panels.
    pub docks: Docks,
    /// Always on Top, Full Screen, Post-It, Distraction Free.
    pub modes: crate::window_modes::WindowModes,
    /// The toolbar's commands whose switch is on, shown pressed.
    pub toolbar_checked: Vec<String>,
    _subscriptions: Vec<Subscription>,
}

impl Workspace {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let panes = [cx.new(|_| Pane::new(0)), cx.new(|_| Pane::new(1))];
        for (pane, other) in [(0, 1), (1, 0)] {
            let other = panes[other].downgrade();
            panes[pane].update(cx, |pane, _| pane.set_other(other));
        }
        let this = cx.entity().downgrade();
        for pane in &panes {
            pane.update(cx, |pane, _| pane.set_workspace(this.clone()));
        }
        let mut subscriptions: Vec<Subscription> = panes
            .iter()
            .map(|pane| cx.subscribe_in(pane, window, Self::on_pane_event))
            .collect();
        let find_bar = cx.new(|cx| FindBar::new(window, cx));
        subscriptions.push(cx.subscribe_in(&find_bar, window, Self::on_find_bar_event));
        let incremental = cx.new(|cx| IncrementalBar::new(window, cx));
        subscriptions.push(cx.subscribe_in(&incremental, window, Self::on_incremental_event));
        let search_results = cx.new(SearchResults::new);
        subscriptions.push(cx.subscribe_in(&search_results, window, Self::on_search_results_event));
        let mut this = Self {
            focus_handle: cx.focus_handle(),
            panes,
            active_pane: 0,
            sync_vertical: false,
            sync_horizontal: false,
            menu_bar: None,
            find_bar,
            incremental,
            search_results,
            peeked: None,
            title: String::new(),
            title_add: None,
            quick_prints: Vec::new(),
            buffer_subscriptions: HashMap::new(),
            view_subscriptions: HashMap::new(),
            session_state: SessionState::default(),
            disk_state: DiskState::default(),
            navigation: History::default(),
            docks: Docks::default(),
            modes: Default::default(),
            toolbar_checked: Vec::new(),
            _subscriptions: subscriptions,
        };
        // Back in front: files may have changed meanwhile.
        let activation = cx.observe_window_activation(window, |this, window, cx| {
            if window.is_window_active() {
                this.check_files(window, cx);
            }
        });
        this._subscriptions.push(activation);
        // Preferences > Appearance > Interface: as the system follows it.
        let appearance = cx.observe_window_appearance(window, |_, _, cx| {
            if AppState::global(cx).settings.appearance.interface
                == birchpad_config::InterfaceMode::System
            {
                crate::themes::reapply(cx);
            }
        });
        this._subscriptions.push(appearance);
        this.refresh_menus(cx);
        // The window's close button: ask about unsaved changes first, unless they are backed
        // up, and save the session.
        let workspace = cx.entity().downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            workspace
                .update(cx, |workspace, cx| {
                    if workspace.can_quit_now(cx) {
                        workspace.save_session_for_quit(cx);
                        return true;
                    }
                    let ready = workspace.prepare_to_quit(window, cx);
                    cx.spawn_in(window, async move |_, cx| {
                        if ready.await {
                            cx.update(|window, _| window.remove_window()).ok();
                        }
                    })
                    .detach();
                    false
                })
                .unwrap_or(true)
        });
        if !cfg!(target_os = "macos") {
            let menu_bar = cx.new(|cx| MenuBar::new(window, cx));
            // A hidden menu bar shows while a menu is open.
            cx.observe(&menu_bar, |_, _, cx| cx.notify()).detach();
            this.menu_bar = Some(menu_bar);
        }
        this
    }

    fn on_pane_event(
        &mut self,
        pane: &Entity<Pane>,
        event: &PaneEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            PaneEvent::ActiveItemChanged => {
                self.note_place(cx);
                self.refresh_menus(cx);
                cx.notify();
                cx.defer_in(window, |this, window, cx| {
                    this.note_seen_by_plugins(cx);
                    if let Some(view) = this.active_view(cx) {
                        this.plugin_event(birchpad_plugin::EventKind::Activated, &view, window, cx);
                    }
                });
            }
            PaneEvent::CloseRequested(view) => {
                // As File > Close: unsaved changes are asked about.
                self.close_with_confirmation(vec![view.clone()], window, cx)
                    .detach();
            }
            PaneEvent::Dropped { view, index, clone } => {
                let target = usize::from(pane == &self.panes[1]);
                if self.pane_of(view, cx) == Some(target) {
                    self.active_pane = target;
                    pane.update(cx, |pane, cx| pane.move_item(view, *index, window, cx));
                } else {
                    self.send_to_pane(view, target, Some(*index), *clone, window, cx);
                }
            }
            PaneEvent::Split {
                view,
                orientation,
                clone,
            } => {
                let target = 1 - usize::from(pane == &self.panes[1]);
                AppState::update_state(cx, |state, _| state.split = *orientation);
                self.send_to_pane(view, target, None, *clone, window, cx);
            }
            PaneEvent::Command { view, invocation } => {
                self.run_tab_command(view.as_ref(), invocation, window, cx);
            }
            PaneEvent::Peek(view) => {
                self.peeked = view.as_ref().map(Entity::downgrade);
                window.refresh();
            }
        }
    }

    /// Scrolls the other view along when synchronized scrolling is on.
    fn on_editor_event(
        &mut self,
        view: &Entity<EditorView>,
        event: &EditorEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (rows, x) = match event {
            EditorEvent::Scrolled { rows, x } => (*rows, *x),
            EditorEvent::Command(invocation) => {
                let (view, invocation) = (view.clone(), invocation.clone());
                // After the menu that chose it has closed.
                cx.defer_in(window, move |this, window, cx| {
                    this.run_context_command(&view, &invocation, window, cx);
                });
                return;
            }
        };
        if !(self.sync_vertical || self.sync_horizontal) || !self.is_split(cx) {
            return;
        }
        let Some(index) = self.pane_of(view, cx) else {
            return;
        };
        if self.panes[index].read(cx).active_item() != Some(view) {
            return;
        }
        let Some(other) = self.panes[1 - index].read(cx).active_item().cloned() else {
            return;
        };
        let rows = if self.sync_vertical { rows } else { 0. };
        let x = if self.sync_horizontal { x } else { px(0.) };
        if rows != 0. || x != px(0.) {
            other.update(cx, |other, cx| other.scroll_by(rows, x, cx));
        }
    }

    pub fn active_pane(&self) -> &Entity<Pane> {
        &self.panes[self.active_pane]
    }

    /// The main and the second view.
    pub fn panes(&self) -> &[Entity<Pane>; 2] {
        &self.panes
    }

    pub fn active_pane_index(&self) -> usize {
        self.active_pane
    }

    /// The pane showing `view`: 0 for the main view, 1 for the second.
    pub fn pane_of(&self, view: &Entity<EditorView>, cx: &App) -> Option<usize> {
        self.panes
            .iter()
            .position(|pane| pane.read(cx).index_of(view).is_some())
    }

    /// Whether both views are shown.
    pub fn is_split(&self, cx: &App) -> bool {
        self.panes
            .iter()
            .all(|pane| !pane.read(cx).items().is_empty())
    }

    /// Follows the focus into a view (a click in the other pane makes it the active one) and its
    /// scrolling.
    pub fn track_view(
        &mut self,
        view: &Entity<EditorView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let focus = view.read(cx).focus_handle.clone();
        let focused_view = view.downgrade();
        let focused = cx.on_focus(&focus, window, move |this, _, cx| {
            let Some(view) = focused_view.upgrade() else {
                return;
            };
            if let Some(index) = this.pane_of(&view, cx)
                && index != this.active_pane
            {
                this.active_pane = index;
                this.refresh_menus(cx);
                cx.notify();
            }
        });
        let events = cx.subscribe_in(view, window, Self::on_editor_event);
        // Wherever its caret goes, for Go Back.
        let changes = cx.observe(view, |this, _, cx| this.note_place(cx));
        // Its typing and its selection, for the plugins that hear about them.
        let watched = cx.observe_in(view, window, |this, _, window, cx| {
            this.watch_for_plugins(window, cx);
        });
        self.view_subscriptions
            .insert(view.entity_id(), [focused, events, changes, watched]);
    }

    /// View > Move/Clone Current Document.
    fn send_to_other_view(&mut self, clone: bool, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(view) = self.active_view(cx) {
            let target = 1 - self.active_pane;
            self.send_to_pane(&view, target, None, clone, window, cx);
        }
    }

    /// Shows `view`'s document in pane `target`, at `index` or after its active tab: moves the
    /// tab there, or adds another view of the document at the same place. If that pane already
    /// shows the document, its view is activated instead (and a moved tab closes).
    fn send_to_pane(
        &mut self,
        view: &Entity<EditorView>,
        target: usize,
        index: Option<usize>,
        clone: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let buffer = view.read(cx).buffer.clone();
        let source = self.pane_of(view, cx);
        let existing = self.panes[target]
            .read(cx)
            .items()
            .iter()
            .find(|other| other.read(cx).buffer == buffer)
            .cloned();
        if let Some(existing) = existing {
            if !clone && source != Some(target) {
                self.close_view(view, window, cx);
            }
            self.activate_view(&existing, window, cx);
            return;
        }
        let item = if clone {
            let state = view.read(cx).view_state(cx);
            let copy = cx.new(|cx| {
                let mut copy = EditorView::new(buffer, window, cx);
                copy.restore(state, cx);
                copy
            });
            self.track_view(&copy, window, cx);
            copy
        } else {
            if let Some(source) = source {
                self.panes[source].update(cx, |pane, cx| pane.remove(view, window, cx));
            }
            view.clone()
        };
        self.active_pane = target;
        self.panes[target].update(cx, |pane, cx| match index {
            Some(index) => pane.insert(index, item, window, cx),
            None => pane.add(item, window, cx),
        });
        self.refresh_menus(cx);
        cx.notify();
    }

    pub fn active_view(&self, cx: &App) -> Option<Entity<EditorView>> {
        self.active_pane().read(cx).active_item().cloned()
    }

    pub fn all_views(&self, cx: &App) -> Vec<Entity<EditorView>> {
        self.panes
            .iter()
            .flat_map(|pane| pane.read(cx).items().to_vec())
            .collect()
    }

    /// The lowest "new N" number not used by an open untitled buffer, as in Notepad++.
    fn next_untitled_number(&self, cx: &App) -> usize {
        let used: Vec<usize> = self
            .all_views(cx)
            .iter()
            .filter_map(|view| view.read(cx).buffer.read(cx).untitled_number())
            .collect();
        (1..)
            .find(|n| !used.contains(n))
            .expect("some number is free")
    }

    pub fn new_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let number = self.next_untitled_number(cx);
        let (format, language) = new_document(cx);
        let buffer = cx.new(|cx| {
            let doc = Document::with_format(birchpad_core::Rope::new(), format);
            let mut buffer = Buffer::untitled_with(number, doc);
            if language.is_some() {
                buffer.set_language(language, cx);
            }
            buffer
        });
        self.add_buffer(buffer, window, cx);
    }

    /// Opens a document that was not read from a file (e.g. generated text) as untitled.
    pub fn open_document(&mut self, doc: Document, window: &mut Window, cx: &mut Context<Self>) {
        let number = self.next_untitled_number(cx);
        let buffer = cx.new(|_| Buffer::untitled_with(number, doc));
        self.add_buffer(buffer, window, cx);
    }

    /// Opens a file in a new tab, or switches to its tab if it is already open. The file is read
    /// in the background.
    pub fn open_path(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        if self.open_by_extension(path, window, cx) {
            return;
        }
        self.open_path_with(path, false, window, cx);
    }

    /// MISC > Session file ext. and Workspace file ext.: a file with one of them opens as a
    /// session, or in Project Panel 1, rather than as text. Returns whether it did.
    pub fn open_by_extension(
        &mut self,
        path: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let session = &AppState::global(cx).settings.session;
        let matches = |wanted: &str| {
            let wanted = wanted.trim().trim_start_matches('.');
            !wanted.is_empty()
                && path
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case(wanted))
        };
        if matches(&session.session_extension) {
            if let Err(error) = self.load_session_file(path, window, cx) {
                report_error(&error, window, cx);
            }
            return true;
        }
        if matches(&session.workspace_extension) {
            let kind = crate::panels::PanelKind::Project(1);
            self.open_panel(kind, window, cx);
            if let Some(panel) = self.docks.views.get(&kind).and_then(|view| {
                view.clone()
                    .downcast::<crate::panels::project::ProjectPanel>()
                    .ok()
            }) {
                let path = path.to_owned();
                panel.update(cx, |panel, cx| panel.open_workspace(path, cx));
            }
            return true;
        }
        false
    }

    /// Opens files and places carets as a command line asks (also from a second launch), and
    /// does what it asks of them: Monitoring, a user defined language, printing, saying how
    /// long they took; and ghost typing. Returns the views of the files opened.
    pub fn open_command_line(
        &mut self,
        command_line: &CommandLine,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<Entity<EditorView>> {
        let started = std::time::Instant::now();
        let views = self.open_command_line_files(command_line, window, cx);
        if command_line.loading_time && !views.is_empty() {
            let time = format!("{:.3}", started.elapsed().as_secs_f64());
            report_info(t!("workspace.loading-time", seconds = time), window, cx);
        }
        if let Some(ghost) = &command_line.ghost {
            self.ghost_type(ghost, window, cx);
        }
        if let Some(message) = &command_line.plugin_message {
            self.deliver_plugin_message(message, window, cx);
        }
        if command_line.help {
            crate::help::show_command_line(window, cx);
        }
        if command_line.quick_print {
            for view in &views {
                self.activate_view(view, window, cx);
                let source = match crate::print::PrintSource::active(self, cx) {
                    Ok(source) => source,
                    Err(error) => {
                        report_error(&error, window, cx);
                        continue;
                    }
                };
                let setup = crate::print::page_setup(cx);
                let pages = source.paginate(&setup);
                let printing =
                    crate::print::print_task(&source.title, &pages, &setup, false, window, cx);
                self.quick_prints
                    .push(cx.spawn_in(window, async move |_, cx| {
                        if let Err(error) = printing.await {
                            cx.update(|window, cx| report_error(&error, window, cx))
                                .ok();
                        }
                    }));
            }
        }
        views
    }

    fn open_command_line_files(
        &mut self,
        command_line: &CommandLine,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<Entity<EditorView>> {
        let mut views = Vec::new();
        if let Some(lines) = command_line.generate {
            let doc = Document::from_text(birchpad_core::Rope::from_str(&crate::generate(lines)));
            self.open_document(doc, window, cx);
        }
        if command_line.open_session {
            for path in &command_line.files {
                if let Err(error) = self.load_session_file(path, window, cx) {
                    report_error(&error, window, cx);
                }
            }
            return views;
        }
        // `-udl="name"`: a user defined language the user has.
        let user_language = command_line.udl.as_deref().and_then(|name| {
            let found = crate::user_languages::find(name, cx);
            if found.is_none() {
                report_warning(t!("workspace.unknown-udl-option", name = name), window, cx);
            }
            found
        });
        let target = command_line.caret_target();
        // `-l<language>`, with Notepad++'s names; `-lnormal` is plain text.
        let language = command_line.language.as_deref().and_then(|id| match id {
            "normal" | "text" => Some(None),
            "javascript.js" => Some(birchpad_syntax::by_id("javascript")),
            "props" => Some(birchpad_syntax::by_id("properties")),
            id => match birchpad_syntax::by_id(id) {
                Some(language) => Some(Some(language)),
                None => {
                    report_warning(t!("workspace.unknown-language-option", id = id), window, cx);
                    None
                }
            },
        });
        for path in &command_line.files {
            // A folder goes to Folder as Workspace, as with -openFoldersAsWorkspace.
            if path.is_dir() {
                self.add_workspace_folder(path, window, cx);
                continue;
            }
            let view = self.open_path_with(path, command_line.read_only, window, cx);
            if let Some(target) = target {
                view.update(cx, |view, cx| view.set_caret_target(target, cx));
            }
            if let Some(language) = language {
                let buffer = view.read(cx).buffer.clone();
                buffer.update(cx, |buffer, cx| buffer.set_language(language, cx));
            }
            if let Some(found) = &user_language {
                let buffer = view.read(cx).buffer.clone();
                let found = found.clone();
                buffer.update(cx, |buffer, cx| buffer.set_user_language(Some(found), cx));
            }
            if command_line.monitor {
                let monitoring = view.read(cx).buffer.read(cx).is_monitoring();
                self.activate_view(&view, window, cx);
                if !monitoring && let Err(error) = self.toggle_monitoring(window, cx) {
                    report_error(&error, window, cx);
                }
            }
            views.push(view);
        }
        views
    }

    /// Ghost typing (`-qt`, `-qf`): the text typed into a new document, a few characters at a
    /// time as `-qSpeed` says (3: at once). `-qn` names Notepad++'s easter eggs, which are its
    /// own: Birchpad says it has none.
    fn ghost_type(
        &mut self,
        ghost: &birchpad_cli::Ghost,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text = match (&ghost.text, &ghost.file, &ghost.name) {
            (Some(text), _, _) => text.clone(),
            (None, Some(file), _) => match std::fs::read_to_string(file) {
                Ok(text) => text,
                Err(error) => {
                    let error = anyhow::anyhow!(
                        "{}",
                        t!(
                            "workspace.cannot-read",
                            path = file.display(),
                            error = error
                        )
                    );
                    report_error(&error, window, cx);
                    return;
                }
            },
            (None, None, Some(name)) => {
                report_warning(t!("workspace.no-easter-egg", name = name), window, cx);
                return;
            }
            (None, None, None) => return,
        };
        self.new_file(window, cx);
        let Some(view) = self.active_view(cx) else {
            return;
        };
        let chars: Vec<char> = text.chars().collect();
        let (chunk, delay) = match ghost.speed {
            1 => (1, 60),
            3 => (chars.len().max(1), 0),
            _ => (2, 15),
        };
        let view = view.downgrade();
        cx.spawn_in(window, async move |_, cx| {
            for piece in chars.chunks(chunk) {
                let piece: String = piece.iter().collect();
                let typed = view.update(cx, |view, cx| {
                    view.insert(&piece, crate::editor::LastEdit::None, cx);
                });
                if typed.is_err() {
                    return;
                }
                if delay > 0 {
                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(delay))
                        .await;
                }
            }
        })
        .detach();
    }

    /// Opens `path` (read-only if asked) or switches to its tab; returns the view.
    fn open_path_with(
        &mut self,
        path: &Path,
        read_only: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<EditorView> {
        let path = std::path::absolute(path).unwrap_or_else(|_| path.to_owned());
        if let Some(view) = self.view_for_path(&path, cx) {
            self.activate_view(&view, window, cx);
            return view;
        }
        // Like Notepad++, an untouched "new 1" is replaced by the first file opened.
        let pristine = self.active_view(cx).filter(|view| {
            let buffer = view.read(cx).buffer.read(cx);
            buffer.path().is_none()
                && buffer.doc().text().len() == 0
                && !buffer.is_modified()
                && !buffer.doc().history().can_undo()
        });
        AppState::remove_recent(&path, cx);
        let options = AppState::global(cx).load_options(None);
        // -fullReadOnly: every file read-only.
        let read_only = read_only || AppState::global(cx).full_read_only;
        let buffer = cx.new(|cx| Buffer::open(path, options, read_only, cx));
        let view = self.add_buffer(buffer, window, cx);
        if let Some(pristine) = pristine {
            self.close_view(&pristine, window, cx);
        }
        view
    }

    /// Tells the user about recovery copies left by saves that were interrupted (a crash or
    /// power loss while writing).
    pub fn report_pending_recoveries(&self, window: &mut Window, cx: &mut App) {
        let Some(dir) = AppState::global(cx).recovery_dir() else {
            return;
        };
        for recovery in birchpad_io::pending_recoveries(&dir) {
            report_warning(
                format!(
                    "Saving {} was interrupted last time. The text that was being saved is in {}.",
                    recovery.original.display(),
                    recovery.data.display()
                ),
                window,
                cx,
            );
        }
    }

    pub fn view_for_path(&self, path: &Path, cx: &App) -> Option<Entity<EditorView>> {
        self.all_views(cx)
            .into_iter()
            .find(|view| view.read(cx).buffer.read(cx).path() == Some(path))
    }

    pub fn activate_view(
        &mut self,
        view: &Entity<EditorView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for (index, pane) in self.panes.clone().into_iter().enumerate() {
            if let Some(position) = pane.read(cx).index_of(view) {
                self.active_pane = index;
                pane.update(cx, |pane, cx| pane.activate(position, window, cx));
            }
        }
        self.refresh_menus(cx);
        cx.notify();
    }

    fn add_buffer(
        &mut self,
        buffer: Entity<Buffer>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<EditorView> {
        self.watch_buffer(&buffer, window, cx);
        let view = cx.new(|cx| EditorView::new(buffer, window, cx));
        self.track_view(&view, window, cx);
        self.active_pane()
            .update(cx, |pane, cx| pane.add(view.clone(), window, cx));
        cx.notify();
        view
    }

    /// Follows a buffer's events: loading failures, state shown in the menus.
    pub fn watch_buffer(
        &mut self,
        buffer: &Entity<Buffer>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let subscription = cx.subscribe_in(buffer, window, Self::on_buffer_event);
        self.buffer_subscriptions
            .insert(buffer.entity_id(), subscription);
        self.sync_watches(cx);
    }

    fn on_buffer_event(
        &mut self,
        buffer: &Entity<Buffer>,
        event: &BufferEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            BufferEvent::LoadFailed(error) => {
                report_error(&anyhow::anyhow!("{error}"), window, cx);
                for view in self.all_views(cx) {
                    if &view.read(cx).buffer == buffer {
                        self.close_view(&view, window, cx);
                    }
                }
            }
            BufferEvent::StateChanged | BufferEvent::Reloaded { .. } => {
                if let BufferEvent::Reloaded { previous: None } = event
                    && let Some(view) = self.view_of_buffer(buffer, cx)
                {
                    cx.defer_in(window, move |this, window, cx| {
                        this.plugin_event(birchpad_plugin::EventKind::Opened, &view, window, cx);
                    });
                }
                // Save As may have moved the file to another folder.
                self.sync_watches(cx);
                self.refresh_menus(cx);
                cx.notify();
            }
            BufferEvent::Edited { transaction, .. } => {
                self.map_places(buffer, transaction);
                self.compare_after_edit(buffer, window, cx);
            }
            BufferEvent::MarksChanged | BufferEvent::SyntaxChanged | BufferEvent::FollowEnd => {}
        }
    }

    pub fn close_view(
        &mut self,
        view: &Entity<EditorView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for pane in &self.panes {
            pane.update(cx, |pane, cx| pane.remove(view, window, cx));
        }
        // A file whose last view closes becomes a recent file, as in Notepad++.
        let buffer = view.read(cx).buffer.clone();
        let still_shown = self
            .all_views(cx)
            .iter()
            .any(|other| other.read(cx).buffer == buffer);
        if !still_shown && let Some(path) = buffer.read(cx).path().map(Path::to_owned) {
            AppState::add_recent(&path, cx);
            self.refresh_menus(cx);
        }
        if !still_shown {
            let closed = view.clone();
            cx.defer_in(window, move |this, window, cx| {
                this.plugin_event(birchpad_plugin::EventKind::Closed, &closed, window, cx);
            });
        }
        // Forget buffers no view shows any more.
        let live: Vec<EntityId> = self
            .all_views(cx)
            .iter()
            .map(|view| view.read(cx).buffer.entity_id())
            .collect();
        self.buffer_subscriptions.retain(|id, _| live.contains(id));
        self.view_subscriptions.remove(&view.entity_id());
        self.sync_watches(cx);
        if self.all_views(cx).is_empty() && AppState::global(cx).settings.tabs.exit_on_last_close {
            // Preferences > Tab Bar > Exit on close the last tab.
            cx.defer_in(window, |this, window, cx| this.exit(window, cx));
            self.active_pane = 0;
            self.new_file(window, cx);
        } else if self.all_views(cx).is_empty() {
            // Like Notepad++, closing the last document leaves an empty "new 1".
            self.active_pane = 0;
            self.new_file(window, cx);
        } else if self.active_pane().read(cx).items().is_empty() {
            // The pane is hidden now: the other one takes over.
            let other = 1 - self.active_pane;
            if let Some(view) = self.panes[other].read(cx).active_item().cloned() {
                self.activate_view(&view, window, cx);
            }
        }
        cx.notify();
    }

    fn run_command(&mut self, action: &RunCommand, window: &mut Window, cx: &mut Context<Self>) {
        if let Err(error) = self.dispatch(&action.0, window, cx) {
            report_error(&error, window, cx);
        }
    }

    /// Runs a command. Editor commands that reach the workspace (focus is in a panel or the menu
    /// bar) go to the active view.
    pub fn dispatch(
        &mut self,
        invocation: &Invocation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let registry = cx.global::<CommandRegistry>();
        if registry.contains(&invocation.command) && !registry.is_enabled(&invocation.command, cx) {
            anyhow::bail!(
                "{}",
                t!("workspace.is-turned-off", invocation = invocation.command)
            );
        }
        let for_workspace = registry.handler::<Self>(&invocation.command);
        let for_editor = registry.handler::<EditorView>(&invocation.command);
        let result = match (for_workspace, for_editor) {
            (Some(run), _) => run(self, invocation, window, cx),
            (None, Some(run)) => match self.active_view(cx) {
                Some(view) => view.update(cx, |view, cx| run(view, invocation, window, cx)),
                None => Ok(()),
            },
            (None, None)
                if cx
                    .global::<CommandRegistry>()
                    .is_pending(&invocation.command) =>
            {
                anyhow::bail!(
                    "{}",
                    t!(
                        "workspace.is-not-available-yet",
                        invocation = invocation.command
                    )
                )
            }
            (None, None) => anyhow::bail!(
                "{}",
                t!("workspace.unknown-command", invocation = invocation.command)
            ),
        };
        if result.is_ok() {
            crate::macros::record(invocation, cx);
        }
        result
    }

    /// Redraws every editor after a view setting (zoom, word wrap, Show Symbol) changed.
    pub fn refresh_views(&mut self, cx: &mut Context<Self>) {
        for view in self.all_views(cx) {
            view.update(cx, |view, cx| view.settings_changed(cx));
        }
        // The tab bar's settings.
        for pane in &self.panes {
            pane.update(cx, |_, cx| cx.notify());
        }
        self.refresh_menus(cx);
        cx.notify();
    }

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

    fn update_title(&mut self, window: &mut Window, cx: &App) {
        let title = self.window_title(cx);
        if title != self.title {
            window.set_window_title(&title);
            // macOS shows a dot in the close button of a window with unsaved changes.
            window.set_window_edited(title.starts_with('*'));
            self.title = title;
        }
    }

    /// `-fullReadOnly`: every document read-only, those the session brought back too.
    pub fn apply_full_read_only(&mut self, cx: &mut Context<Self>) {
        if !AppState::global(cx).full_read_only {
            return;
        }
        for view in self.all_views(cx) {
            let buffer = view.read(cx).buffer.clone();
            buffer.update(cx, |buffer, cx| buffer.set_requested_read_only(true, cx));
        }
        self.refresh_menus(cx);
    }

    /// Preferences > New Document > Always open a new document at startup: a new one besides
    /// the session's and the command line's, unless the only one is an empty untitled one.
    pub fn open_startup_document(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !AppState::global(cx).settings.new_document.at_startup {
            return;
        }
        let views = self.all_views(cx);
        let only_empty = matches!(views.as_slice(), [view] if {
            let buffer = view.read(cx).buffer.read(cx);
            buffer.path().is_none() && buffer.doc().text().len_chars() == 0
        });
        if !only_empty {
            self.new_file(window, cx);
        }
    }

    /// The window's title: `*` for unsaved changes, the active file's path (or only its name,
    /// Preferences > Appearance), and Birchpad.
    pub fn window_title(&self, cx: &App) -> String {
        let title = match self.active_view(cx) {
            Some(view) => {
                let buffer = view.read(cx).buffer.read(cx);
                let name_only = AppState::global(cx)
                    .settings
                    .appearance
                    .title_file_name_only;
                let name = buffer
                    .path()
                    .filter(|_| !name_only)
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| buffer.display_name());
                let modified = if buffer.is_modified() { "*" } else { "" };
                format!("{modified}{name} - Birchpad")
            }
            None => "Birchpad".to_owned(),
        };
        match &self.title_add {
            Some(text) if !text.is_empty() => format!("{title} - {text}"),
            _ => title,
        }
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.update_title(window, cx);
        let status = self
            .active_view(cx)
            .map(|view| view.update(cx, |view, cx| StatusInfo::of(view, cx)));
        let ansi = AppState::global(cx).ansi;
        let appearance = &AppState::global(cx).settings.appearance;
        let (show_menu_bar, show_status_bar) = (appearance.menu_bar, appearance.status_bar);
        // Hidden, the menu bar still shows while one of its menus is open (Alt, F10).
        let menu_bar_shown = show_menu_bar
            || self
                .menu_bar
                .as_ref()
                .is_some_and(|menu_bar| menu_bar.read(cx).is_active());
        div()
            .id("workspace")
            .key_context("Workspace")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::run_command))
            // Ctrl released: the document switcher keeps the document it shows.
            .on_modifiers_changed(cx.listener(|this, event: &ModifiersChangedEvent, _, cx| {
                if !event.modifiers.control {
                    for pane in this.panes.clone() {
                        pane.update(cx, |pane, cx| pane.end_switch(cx));
                    }
                }
            }))
            .on_drop(cx.listener(|this, paths: &ExternalPaths, window, cx| {
                // A folder goes to Folder as Workspace, as in Notepad++.
                let opens_files = AppState::global(cx).settings.files.drop_folder_opens_files;
                for path in paths.paths() {
                    if path.is_dir() && opens_files {
                        // Preferences > Default Directory: open the folder's files instead.
                        for file in folder_files(path) {
                            this.open_path(&file, window, cx);
                        }
                    } else if path.is_dir() {
                        this.add_workspace_folder(path, window, cx);
                    } else {
                        this.open_path(path, window, cx);
                    }
                }
            }))
            .size_full()
            .flex()
            .flex_col()
            .bg(crate::theme::paint(crate::theme::ui().background))
            .text_color(crate::theme::paint(crate::theme::ui().text))
            .when_some(self.menu_bar.clone(), birchpad_menu_bar::route_input)
            .when_some(
                self.menu_bar
                    .clone()
                    .filter(|_| !self.modes.text_only() && menu_bar_shown),
                |this, menu_bar| {
                    this.child(
                        div()
                            .flex_none()
                            .h(px(30.))
                            .px_1()
                            .border_b_1()
                            .border_color(crate::theme::paint(crate::theme::ui().border))
                            .child(menu_bar),
                    )
                },
            )
            .when(!self.modes.text_only(), |this| {
                this.children(self.render_toolbar(cx))
            })
            .child({
                let center = self.render_main_area(cx);
                let main = if self.modes.distraction_free {
                    // The text in a column in the middle, as wide as Preferences > Margins >
                    // Distraction Free padding leaves it.
                    let padding = AppState::global(cx)
                        .settings
                        .editor
                        .padding_distraction_free
                        .min(9);
                    let width = 1. - f32::from(padding) / 10.;
                    div()
                        .size_full()
                        .flex()
                        .justify_center()
                        .child(div().h_full().w(relative(width)).child(center))
                        .into_any_element()
                } else if self.modes.post_it {
                    center
                } else {
                    self.render_with_docks(center, cx)
                };
                div()
                    .relative()
                    .flex_1()
                    .min_h(px(0.))
                    .child(main)
                    .when(self.modes.text_only(), |this| {
                        this.child(self.render_leave_button(cx))
                    })
            })
            .when(self.find_bar.read(cx).visible, |this| {
                this.child(self.find_bar.clone())
            })
            .when(self.incremental.read(cx).visible, |this| {
                this.child(self.incremental.clone())
            })
            .when(!self.modes.text_only() && show_status_bar, |this| {
                this.child(crate::status_bar::render(status, ansi, cx))
            })
    }
}

impl Workspace {
    /// The way out of Post-It and Distraction Free, whose menus are hidden: a faint button in
    /// the top right corner.
    fn render_leave_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .absolute()
            .top_1()
            .right_4()
            .opacity(0.4)
            .hover(|style| style.opacity(1.))
            .child(
                Button::new("leave-text-only")
                    .ghost()
                    .xsmall()
                    .icon(IconName::Close)
                    .debug_selector(|| "leave-text-only".into())
                    .tooltip(t!("window-modes.leave"))
                    .on_click(cx.listener(|this, _, window, cx| this.leave_text_only(window, cx))),
            )
    }

    /// The documents, and the search results under them when they are shown.
    fn render_main_area(&self, cx: &App) -> AnyElement {
        if !self.search_results.read(cx).visible {
            return self.render_panes(cx);
        }
        v_resizable("results-split")
            .child(resizable_panel().child(self.render_panes(cx)))
            .child(
                resizable_panel()
                    .size(px(220.))
                    .size_range(px(60.)..px(2000.))
                    .child(
                        div()
                            .size_full()
                            .border_t_1()
                            .border_color(crate::theme::paint(crate::theme::ui().border))
                            .child(self.search_results.clone()),
                    ),
            )
            .into_any_element()
    }

    /// The pane with tabs, or both side by side or stacked, with a splitter between them.
    fn render_panes(&self, cx: &App) -> AnyElement {
        if !self.is_split(cx) {
            let shown = self
                .panes
                .iter()
                .find(|pane| !pane.read(cx).items().is_empty())
                .unwrap_or(&self.panes[0]);
            return shown.clone().into_any_element();
        }
        let group = match AppState::global(cx).state.split {
            SplitOrientation::SideBySide => h_resizable("split-side-by-side"),
            SplitOrientation::Stacked => v_resizable("split-stacked"),
        };
        // A line over the active view, as Notepad++ marks the tab of the focused view.
        let panel = |index: usize| {
            let line = if index == self.active_pane {
                crate::theme::paint(crate::theme::ui().accent)
            } else {
                crate::theme::paint(crate::theme::ui().background)
            };
            resizable_panel().child(
                div()
                    .size_full()
                    .border_t_2()
                    .border_color(line)
                    .child(self.panes[index].clone()),
            )
        };
        group.child(panel(0)).child(panel(1)).into_any_element()
    }
}

impl Focusable for Workspace {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

/// The files of a folder dropped on the window, its subfolders' too, without hidden ones, at
/// most a thousand.
pub fn folder_files(folder: &Path) -> Vec<std::path::PathBuf> {
    let options = birchpad_io::FolderOptions {
        subfolders: true,
        hidden: false,
    };
    let mut files = birchpad_io::files_in(
        folder,
        &birchpad_io::Filters::default(),
        options,
        &std::sync::atomic::AtomicBool::new(false),
    )
    .unwrap_or_default();
    files.truncate(1000);
    files
}
