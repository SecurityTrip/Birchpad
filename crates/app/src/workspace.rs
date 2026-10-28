//! The workspace: the contents of a main window. It owns the two panes of split view, routes
//! commands and draws the menu bar and the status bar.

mod commands;
mod documents;
mod menus;
mod panes;
mod render;

pub use commands::register_commands;
pub use documents::new_document;

use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use birchpad_commands::Invocation;
use birchpad_menu_bar::MenuBar;
use gpui_kit::{
    App, AppContext as _, Context, Entity, EntityId, FocusHandle, Focusable, Subscription, Window,
    px,
};

use crate::app_state::AppState;
use crate::commands::{CommandRegistry, RunCommand};
use crate::disk::DiskState;
pub use crate::editor::{EditorEvent, EditorView, ViewSettings};
use crate::find::FindBar;
use crate::incremental::IncrementalBar;
use crate::navigation::{History, Place};
use crate::pane::{Pane, PaneEvent};
use crate::panels::Docks;
use crate::search_results::SearchResults;
use crate::session::SessionState;

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
