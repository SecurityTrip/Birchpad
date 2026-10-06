//! The workspace: the contents of a main window. It owns the two panes of split view, routes
//! commands and draws the menu bar and the status bar.

use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use birchpad_cli::CommandLine;
use birchpad_commands::Invocation;
use birchpad_config::{SplitOrientation, UserState};
use birchpad_core::Document;
use birchpad_menu_bar::MenuBar;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::Button;
use gpui_kit::component::dialog::DialogAction;
use gpui_kit::component::notification::Notification;
use gpui_kit::component::{h_resizable, resizable_panel, v_resizable};
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, EntityId, ExternalPaths, FocusHandle,
    Focusable, Subscription, Window, div, prelude::*, px, rgb,
};

use crate::app_state::AppState;
use crate::buffer::{Buffer, BufferEvent};
use crate::commands::{CommandRegistry, Handler, RunCommand};
use crate::disk::DiskState;
use crate::editor::{EditorEvent, EditorView, ViewSettings};
use crate::find::FindBar;
use crate::incremental::IncrementalBar;
use crate::menus::{self, MenuState};
use crate::navigation::{History, Place};
use crate::pane::{Pane, PaneEvent};
use crate::panels::Docks;
use crate::search_results::SearchResults;
use crate::session::SessionState;
use crate::status_bar::StatusInfo;

pub(crate) fn register_commands(registry: &mut CommandRegistry) {
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
    for &(id, read, write) in VIEW_SWITCHES {
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
        let on = !all_characters(&ViewSettings::read(cx));
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
            AppState::update_state(cx, |state, _| {
                state.zoom = if step == 0 {
                    0
                } else {
                    (state.zoom + step).clamp(*range.start(), *range.end())
                };
            });
            this.refresh_views(cx);
            Ok(())
        });
    }
    registry.workspace("language.set", |this, args: LanguageArgs, _, cx| {
        let language = match args.language.as_str() {
            "text" | "normal" => None,
            id => Some(
                birchpad_syntax::by_id(id)
                    .ok_or_else(|| anyhow::anyhow!("unknown language {id}"))?,
            ),
        };
        if let Some(view) = this.active_view(cx) {
            let buffer = view.read(cx).buffer.clone();
            buffer.update(cx, |buffer, cx| buffer.set_language(language, cx));
        }
        Ok(())
    });
    crate::encoding_ui::register_commands(registry);
    crate::file_ops::register_commands(registry);
    crate::find::register_commands(registry);
    crate::help::register_commands(registry);
    crate::incremental::register_commands(registry);
    crate::navigation::register_commands(registry);
    crate::panels::register_commands(registry);
    crate::session::register_commands(registry);
    crate::disk::register_commands(registry);
}

/// View menu switches kept in `state.toml`: command, current value, how to remember a new one.
type ViewSwitch = (
    &'static str,
    fn(&ViewSettings) -> bool,
    fn(&mut UserState, bool),
);

const VIEW_SWITCHES: &[ViewSwitch] = &[
    (
        "view.word-wrap",
        |view| view.word_wrap,
        |state, on| state.word_wrap = Some(on),
    ),
    (
        "view.show-whitespace",
        |view| view.show_whitespace,
        |state, on| state.show_whitespace = Some(on),
    ),
    (
        "view.show-eol",
        |view| view.show_eol,
        |state, on| state.show_eol = Some(on),
    ),
    (
        "view.indent-guides",
        |view| view.indent_guides,
        |state, on| state.indent_guides = Some(on),
    ),
    (
        "view.wrap-symbol",
        |view| view.wrap_symbol,
        |state, on| state.wrap_symbol = Some(on),
    ),
];

fn all_characters(view: &ViewSettings) -> bool {
    view.show_whitespace && view.show_eol
}

#[derive(serde::Deserialize)]
struct LanguageArgs {
    language: String,
}

/// Shows an error to the user without interrupting them.
pub(crate) fn report_error(error: &anyhow::Error, window: &mut Window, cx: &mut App) {
    window.push_notification(Notification::error(format!("{error:#}")), cx);
}

/// The confirming button of a dialog footer, at its natural width (`DialogAction` alone fills
/// the footer).
pub(crate) fn dialog_action(button: Button) -> impl IntoElement {
    div().child(DialogAction::new().child(button))
}

/// Shows a warning to the user without interrupting them.
pub(crate) fn report_warning(message: impl Into<String>, window: &mut Window, cx: &mut App) {
    window.push_notification(Notification::warning(message.into()), cx);
}

/// The line over the active view in split view.
const ACTIVE_VIEW: u32 = 0x0969da;

pub(crate) struct Workspace {
    focus_handle: FocusHandle,
    /// Notepad++'s main and second views. A pane without tabs is hidden; split view shows both.
    panes: [Entity<Pane>; 2],
    active_pane: usize,
    /// View > Synchronize Vertical / Horizontal Scrolling.
    sync_vertical: bool,
    sync_horizontal: bool,
    menu_bar: Option<Entity<MenuBar>>,
    pub(crate) find_bar: Entity<FindBar>,
    pub(crate) incremental: Entity<IncrementalBar>,
    pub(crate) search_results: Entity<SearchResults>,
    title: String,
    buffer_subscriptions: HashMap<EntityId, Subscription>,
    /// Focus and scroll events of each view, and its changes for the navigation history.
    view_subscriptions: HashMap<EntityId, [Subscription; 3]>,
    pub(crate) session_state: SessionState,
    pub(crate) disk_state: DiskState,
    /// Go Back and Go Forward.
    pub(crate) navigation: History<Place>,
    /// The side panels.
    pub(crate) docks: Docks,
    _subscriptions: Vec<Subscription>,
}

impl Workspace {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let panes = [cx.new(|_| Pane::new(0)), cx.new(|_| Pane::new(1))];
        for (pane, other) in [(0, 1), (1, 0)] {
            let other = panes[other].downgrade();
            panes[pane].update(cx, |pane, _| pane.set_other(other));
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
            title: String::new(),
            buffer_subscriptions: HashMap::new(),
            view_subscriptions: HashMap::new(),
            session_state: SessionState::default(),
            disk_state: DiskState::default(),
            navigation: History::default(),
            docks: Docks::default(),
            _subscriptions: subscriptions,
        };
        // Back in front: files may have changed meanwhile.
        let activation = cx.observe_window_activation(window, |this, window, cx| {
            if window.is_window_active() {
                this.check_files(window, cx);
            }
        });
        this._subscriptions.push(activation);
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
            this.menu_bar = Some(cx.new(|cx| MenuBar::new(window, cx)));
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
        }
    }

    /// Scrolls the other view along when synchronized scrolling is on.
    fn on_editor_event(
        &mut self,
        view: &Entity<EditorView>,
        event: &EditorEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let EditorEvent::Scrolled { rows, x } = *event;
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

    pub(crate) fn active_pane(&self) -> &Entity<Pane> {
        &self.panes[self.active_pane]
    }

    /// The main and the second view.
    pub(crate) fn panes(&self) -> &[Entity<Pane>; 2] {
        &self.panes
    }

    pub(crate) fn active_pane_index(&self) -> usize {
        self.active_pane
    }

    /// The pane showing `view`: 0 for the main view, 1 for the second.
    pub(crate) fn pane_of(&self, view: &Entity<EditorView>, cx: &App) -> Option<usize> {
        self.panes
            .iter()
            .position(|pane| pane.read(cx).index_of(view).is_some())
    }

    /// Whether both views are shown.
    pub(crate) fn is_split(&self, cx: &App) -> bool {
        self.panes
            .iter()
            .all(|pane| !pane.read(cx).items().is_empty())
    }

    /// Follows the focus into a view (a click in the other pane makes it the active one) and its
    /// scrolling.
    pub(crate) fn track_view(
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
        self.view_subscriptions
            .insert(view.entity_id(), [focused, events, changes]);
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

    pub(crate) fn active_view(&self, cx: &App) -> Option<Entity<EditorView>> {
        self.active_pane().read(cx).active_item().cloned()
    }

    pub(crate) fn all_views(&self, cx: &App) -> Vec<Entity<EditorView>> {
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

    pub(crate) fn new_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let number = self.next_untitled_number(cx);
        let buffer = cx.new(|_| Buffer::untitled(number));
        self.add_buffer(buffer, window, cx);
    }

    /// Opens a document that was not read from a file (e.g. generated text) as untitled.
    pub(crate) fn open_document(
        &mut self,
        doc: Document,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let number = self.next_untitled_number(cx);
        let buffer = cx.new(|_| Buffer::untitled_with(number, doc));
        self.add_buffer(buffer, window, cx);
    }

    /// Opens a file in a new tab, or switches to its tab if it is already open. The file is read
    /// in the background.
    pub(crate) fn open_path(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        self.open_path_with(path, false, window, cx);
    }

    /// Opens files and places carets as a command line asks (also from a second launch).
    pub(crate) fn open_command_line(
        &mut self,
        command_line: &CommandLine,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
            return;
        }
        let target = command_line.caret_target();
        // `-l<language>`, with Notepad++'s names; `-lnormal` is plain text.
        let language = command_line.language.as_deref().and_then(|id| match id {
            "normal" | "text" => Some(None),
            "javascript.js" => Some(birchpad_syntax::by_id("javascript")),
            "props" => Some(birchpad_syntax::by_id("properties")),
            id => match birchpad_syntax::by_id(id) {
                Some(language) => Some(Some(language)),
                None => {
                    report_warning(format!("Unknown language -l{id}"), window, cx);
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
        }
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
        let buffer = cx.new(|cx| Buffer::open(path, options, read_only, cx));
        let view = self.add_buffer(buffer, window, cx);
        if let Some(pristine) = pristine {
            self.close_view(&pristine, window, cx);
        }
        view
    }

    /// Tells the user about recovery copies left by saves that were interrupted (a crash or
    /// power loss while writing).
    pub(crate) fn report_pending_recoveries(&self, window: &mut Window, cx: &mut App) {
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

    pub(crate) fn view_for_path(&self, path: &Path, cx: &App) -> Option<Entity<EditorView>> {
        self.all_views(cx)
            .into_iter()
            .find(|view| view.read(cx).buffer.read(cx).path() == Some(path))
    }

    pub(crate) fn activate_view(
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
    pub(crate) fn watch_buffer(
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
                // Save As may have moved the file to another folder.
                self.sync_watches(cx);
                self.refresh_menus(cx);
                cx.notify();
            }
            BufferEvent::Edited { transaction, .. } => self.map_places(buffer, transaction),
            BufferEvent::MarksChanged | BufferEvent::SyntaxChanged | BufferEvent::FollowEnd => {}
        }
    }

    pub(crate) fn close_view(
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
        // Forget buffers no view shows any more.
        let live: Vec<EntityId> = self
            .all_views(cx)
            .iter()
            .map(|view| view.read(cx).buffer.entity_id())
            .collect();
        self.buffer_subscriptions.retain(|id, _| live.contains(id));
        self.view_subscriptions.remove(&view.entity_id());
        self.sync_watches(cx);
        if self.all_views(cx).is_empty() {
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
    pub(crate) fn dispatch(
        &mut self,
        invocation: &Invocation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let registry = cx.global::<CommandRegistry>();
        if registry.get(&invocation.command).is_some()
            && !registry.is_enabled(&invocation.command, cx)
        {
            anyhow::bail!("{} is turned off", invocation.command);
        }
        let handler = registry.get(&invocation.command);
        match handler {
            Some(Handler::Workspace(run)) => run(self, invocation, window, cx),
            Some(Handler::Editor(run)) => match self.active_view(cx) {
                Some(view) => view.update(cx, |view, cx| run(view, invocation, window, cx)),
                None => Ok(()),
            },
            None if cx
                .global::<CommandRegistry>()
                .is_pending(&invocation.command) =>
            {
                anyhow::bail!("{} is not available yet", invocation.command)
            }
            None => anyhow::bail!("unknown command {}", invocation.command),
        }
    }

    /// Redraws every editor after a view setting (zoom, word wrap, Show Symbol) changed.
    fn refresh_views(&mut self, cx: &mut Context<Self>) {
        for view in self.all_views(cx) {
            view.update(cx, |view, cx| view.settings_changed(cx));
        }
        self.refresh_menus(cx);
        cx.notify();
    }

    /// Rebuilds the menus from the current state.
    pub(crate) fn refresh_menus(&mut self, cx: &mut Context<Self>) {
        let format = self
            .active_view(cx)
            .map(|view| view.read(cx).buffer.read(cx).doc().format());
        let language = self.active_view(cx).map(|view| {
            view.read(cx)
                .buffer
                .read(cx)
                .language()
                .map_or("text", |l| l.id)
        });
        let ansi = AppState::global(cx).ansi;
        let view = ViewSettings::read(cx);
        let monitoring = self
            .active_view(cx)
            .is_some_and(|view| view.read(cx).buffer.read(cx).is_monitoring());
        let checked = |invocation: &Invocation| {
            if let Some((_, read, _)) = VIEW_SWITCHES
                .iter()
                .find(|(id, _, _)| *id == invocation.command)
            {
                return read(&view);
            }
            if invocation.command == "view.show-all-characters" {
                return all_characters(&view);
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
            if invocation.command == "language.set" {
                return invocation.args.get("language").and_then(|id| id.as_str()) == language;
            }
            if let Some(open) = crate::panels::is_checked(&self.docks, invocation) {
                return open;
            }
            let Some(format) = format else {
                return false;
            };
            crate::encoding_ui::is_current(invocation, format, ansi)
        };
        let recent_files = AppState::global(cx).state.recent_files.clone();
        let state = MenuState {
            recent_files: &recent_files,
            checked: &checked,
        };
        menus::install(&state, cx);
        if let Some(menu_bar) = &self.menu_bar {
            menu_bar.update(cx, |menu_bar, cx| menu_bar.reload(cx));
        }
    }

    fn update_title(&mut self, window: &mut Window, cx: &App) {
        let title = match self.active_view(cx) {
            Some(view) => {
                let buffer = view.read(cx).buffer.read(cx);
                let name = buffer
                    .path()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| buffer.display_name());
                let modified = if buffer.is_modified() { "*" } else { "" };
                format!("{modified}{name} - Birchpad")
            }
            None => "Birchpad".to_owned(),
        };
        if title != self.title {
            window.set_window_title(&title);
            // macOS shows a dot in the close button of a window with unsaved changes.
            window.set_window_edited(title.starts_with('*'));
            self.title = title;
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
        div()
            .id("workspace")
            .key_context("Workspace")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::run_command))
            .on_drop(cx.listener(|this, paths: &ExternalPaths, window, cx| {
                // A folder goes to Folder as Workspace, as in Notepad++.
                for path in paths.paths() {
                    if path.is_dir() {
                        this.add_workspace_folder(path, window, cx);
                    } else {
                        this.open_path(path, window, cx);
                    }
                }
            }))
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0xffffff))
            .text_color(rgb(0x1f2328))
            .when_some(self.menu_bar.clone(), birchpad_menu_bar::route_input)
            .when_some(self.menu_bar.clone(), |this, menu_bar| {
                this.child(
                    div()
                        .flex_none()
                        .h(px(30.))
                        .px_1()
                        .border_b_1()
                        .border_color(rgb(0xd0d7de))
                        .child(menu_bar),
                )
            })
            .child({
                let center = self.render_main_area(cx);
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .child(self.render_with_docks(center, cx))
            })
            .when(self.find_bar.read(cx).visible, |this| {
                this.child(self.find_bar.clone())
            })
            .when(self.incremental.read(cx).visible, |this| {
                this.child(self.incremental.clone())
            })
            .child(crate::status_bar::render(status, ansi, cx))
    }
}

impl Workspace {
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
                            .border_color(rgb(0xd0d7de))
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
                rgb(ACTIVE_VIEW)
            } else {
                rgb(0xffffff)
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

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use gpui_kit::{Modifiers, MouseButton, TestAppContext, VisualTestContext};

    /// `secondary-<key>` in the spelling GPUI's test harness expects on this platform.
    pub(crate) fn secondary(key: &str) -> String {
        let modifier = if cfg!(target_os = "macos") {
            "cmd"
        } else {
            "ctrl"
        };
        format!("{modifier}-{key}")
    }

    /// The keystroke that moves the caret to the start of the document on this platform:
    /// Ctrl+Home on Windows and Linux, Cmd+Up on macOS (Cmd+Home does nothing there).
    pub(crate) fn document_start() -> &'static str {
        if cfg!(target_os = "macos") {
            "cmd-up"
        } else {
            "ctrl-home"
        }
    }

    /// Toggle Bookmark: Ctrl+F2, or Cmd+F2 on macOS.
    pub(crate) fn toggle_bookmark() -> &'static str {
        if cfg!(target_os = "macos") {
            "cmd-f2"
        } else {
            "ctrl-f2"
        }
    }

    /// The keystroke that extends a rectangular selection towards `key`: Alt+Shift+<key> on
    /// Windows and Linux, Cmd+Alt+Shift+<key> on macOS.
    pub(crate) fn block(key: &str) -> String {
        if cfg!(target_os = "macos") {
            format!("cmd-alt-shift-{key}")
        } else {
            format!("alt-shift-{key}")
        }
    }

    /// Column-mode Begin/End Select: Alt+Shift+B on Windows and Linux, Cmd+Alt+Shift+B on
    /// macOS.
    pub(crate) fn begin_end_column() -> &'static str {
        if cfg!(target_os = "macos") {
            "cmd-alt-shift-b"
        } else {
            "alt-shift-b"
        }
    }

    /// The Column Editor: Alt+C on Windows and Linux, Cmd+Alt+C on macOS.
    pub(crate) fn column_editor() -> &'static str {
        if cfg!(target_os = "macos") {
            "cmd-alt-c"
        } else {
            "alt-c"
        }
    }

    pub(crate) fn open_workspace(
        cx: &mut TestAppContext,
    ) -> (Entity<Workspace>, &mut VisualTestContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            let settings = birchpad_config::resolve(birchpad_config::Sources::default());
            cx.set_global(AppState::new(
                settings,
                birchpad_config::ConfigPaths::default(),
            ));
            crate::commands::init(None, cx);
        });
        // The production entry point, so the window has gpui-component's Root (notifications,
        // dialogs).
        let (window, workspace) = cx.update(|cx| {
            let options = gpui_kit::WindowOptions {
                window_bounds: Some(gpui_kit::WindowBounds::Windowed(
                    gpui_kit::Bounds::maximized(None, cx),
                )),
                ..Default::default()
            };
            gpui_kit::open_window(options, cx, |window, cx| {
                cx.new(|cx| {
                    let mut workspace = Workspace::new(window, cx);
                    workspace.new_file(window, cx);
                    workspace
                })
            })
            .expect("open test window")
        });
        let cx = VisualTestContext::from_window(window, cx).into_mut();
        // Active like the window the user works in: focus events are only reported then.
        cx.update(|window, _| window.activate_window());
        cx.run_until_parked();
        (workspace, cx)
    }

    pub(crate) fn tab_names(
        workspace: &Entity<Workspace>,
        cx: &mut VisualTestContext,
    ) -> Vec<String> {
        workspace.read_with(cx, |workspace, cx| {
            workspace
                .all_views(cx)
                .iter()
                .map(|view| view.read(cx).buffer.read(cx).display_name())
                .collect()
        })
    }

    /// The tab names of the main and the second view, and which view is active.
    pub(crate) fn panes(
        workspace: &Entity<Workspace>,
        cx: &mut VisualTestContext,
    ) -> ([Vec<String>; 2], usize) {
        workspace.read_with(cx, |workspace, cx| {
            let names = workspace.panes.clone().map(|pane| {
                pane.read(cx)
                    .items()
                    .iter()
                    .map(|view| view.read(cx).buffer.read(cx).display_name())
                    .collect()
            });
            (names, workspace.active_pane)
        })
    }

    /// The views of a pane.
    pub(crate) fn pane_views(
        workspace: &Entity<Workspace>,
        pane: usize,
        cx: &mut VisualTestContext,
    ) -> Vec<Entity<EditorView>> {
        workspace.read_with(cx, |workspace, cx| {
            workspace.panes[pane].read(cx).items().to_vec()
        })
    }

    /// Sends a pane the event of a tab dropped on it.
    pub(crate) fn drop_tab(
        workspace: &Entity<Workspace>,
        pane: usize,
        view: Entity<EditorView>,
        index: usize,
        cx: &mut VisualTestContext,
    ) {
        let pane = workspace.read_with(cx, |workspace, _| workspace.panes[pane].clone());
        pane.update(cx, |_, cx| {
            cx.emit(PaneEvent::Dropped {
                view,
                index,
                clone: false,
            })
        });
        cx.run_until_parked();
    }

    pub(crate) fn active_text(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> String {
        workspace.read_with(cx, |workspace, cx| {
            let view = workspace.active_view(cx).unwrap();
            view.read(cx).text(cx).to_string()
        })
    }

    #[gpui_kit::test]
    fn key_bindings_dispatch_commands_through_the_registry(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        cx.simulate_keystrokes(&secondary("n"));
        cx.simulate_keystrokes(&secondary("n"));
        assert_eq!(tab_names(&workspace, cx), ["new 1", "new 2", "new 3"]);

        cx.simulate_input("abc");
        cx.simulate_keystrokes("left backspace");
        assert_eq!(active_text(&workspace, cx), "ac");
        cx.simulate_keystrokes(&secondary("z"));
        assert_eq!(active_text(&workspace, cx), "abc");

        // Ctrl+Tab wraps around to the first tab; closing it frees the number 1 again.
        cx.simulate_keystrokes("ctrl-tab");
        assert_eq!(active_text(&workspace, cx), "");
        cx.simulate_keystrokes(&secondary("w"));
        assert_eq!(tab_names(&workspace, cx), ["new 2", "new 3"]);
        cx.simulate_keystrokes(&secondary("n"));
        assert_eq!(tab_names(&workspace, cx), ["new 2", "new 1", "new 3"]);
    }

    #[gpui_kit::test]
    fn closing_the_last_tab_leaves_an_empty_document(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        cx.simulate_input("text");
        workspace.update_in(cx, |workspace, window, cx| {
            workspace
                .dispatch(&Invocation::new("file.close-all"), window, cx)
                .unwrap();
        });
        cx.run_until_parked();
        cx.simulate_prompt_answer("Don't Save");
        cx.run_until_parked();
        assert_eq!(tab_names(&workspace, cx), ["new 1"]);
        assert_eq!(active_text(&workspace, cx), "");
    }

    /// Clicks the element drawn with this debug selector, with `button`.
    fn click_on(selector: &'static str, button: MouseButton, cx: &mut VisualTestContext) {
        let center = cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("{selector} is not drawn"))
            .center();
        cx.simulate_mouse_down(center, button, Modifiers::none());
        cx.simulate_mouse_up(center, button, Modifiers::none());
        cx.run_until_parked();
    }

    #[gpui_kit::test]
    fn the_close_button_of_a_tab_asks_about_unsaved_changes(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        cx.simulate_input("changed");
        cx.simulate_keystrokes(&secondary("n"));

        // "new 1" has unsaved changes: it comes forward and is asked about.
        click_on("pane-0-close-tab-0", MouseButton::Left, cx);
        assert!(cx.has_pending_prompt());
        assert_eq!(active_text(&workspace, cx), "changed");
        cx.simulate_prompt_answer("Cancel");
        cx.run_until_parked();
        assert_eq!(tab_names(&workspace, cx), ["new 1", "new 2"]);
        click_on("pane-0-close-tab-0", MouseButton::Left, cx);
        cx.simulate_prompt_answer("Don't Save");
        cx.run_until_parked();
        assert_eq!(tab_names(&workspace, cx), ["new 2"]);

        // Without changes, it closes at once.
        cx.simulate_keystrokes(&secondary("n"));
        assert_eq!(tab_names(&workspace, cx), ["new 2", "new 1"]);
        click_on("pane-0-close-tab-1", MouseButton::Left, cx);
        assert!(!cx.has_pending_prompt());
        assert_eq!(tab_names(&workspace, cx), ["new 2"]);
    }

    #[gpui_kit::test]
    fn the_middle_mouse_button_closes_a_tab(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        cx.simulate_keystrokes(&secondary("n"));
        cx.simulate_input("changed");
        cx.simulate_keystrokes(&secondary("n"));
        assert_eq!(tab_names(&workspace, cx), ["new 1", "new 2", "new 3"]);

        // A tab that is not the active one closes; the active one stays active.
        click_on("pane-0-tab-0", MouseButton::Middle, cx);
        assert_eq!(tab_names(&workspace, cx), ["new 2", "new 3"]);
        assert_eq!(active_text(&workspace, cx), "");

        // Unsaved changes are asked about, as with the close button.
        click_on("pane-0-tab-0", MouseButton::Middle, cx);
        assert!(cx.has_pending_prompt());
        cx.simulate_prompt_answer("Don't Save");
        cx.run_until_parked();
        assert_eq!(tab_names(&workspace, cx), ["new 3"]);

        // A left click only activates.
        cx.simulate_keystrokes(&secondary("n"));
        click_on("pane-0-tab-0", MouseButton::Left, cx);
        assert_eq!(tab_names(&workspace, cx), ["new 3", "new 1"]);
        let active = workspace.read_with(cx, |workspace, cx| {
            let view = workspace.active_view(cx).unwrap();
            view.read(cx).buffer.read(cx).display_name()
        });
        assert_eq!(active, "new 3");
    }

    #[gpui_kit::test]
    fn command_line_opens_files_at_a_line_read_only(cx: &mut TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a b.txt"), "one\n\ttwo\nthree\n").unwrap();
        std::fs::write(dir.path().join("other.txt"), "other").unwrap();
        let (workspace, cx) = open_workspace(cx);
        let caret = |workspace: &Entity<Workspace>, cx: &mut VisualTestContext| {
            workspace.update(cx, |workspace, cx| {
                let view = workspace.active_view(cx).unwrap();
                let info = view.update(cx, |view, cx| crate::status_bar::StatusInfo::of(view, cx));
                (info.line, info.column)
            })
        };
        let open = |args: &[&str], cx: &mut VisualTestContext| {
            let args = args.iter().map(std::ffi::OsString::from);
            let command_line = CommandLine::parse(args, dir.path());
            workspace.update_in(cx, |workspace, window, cx| {
                workspace.open_command_line(&command_line, window, cx);
            });
            cx.run_until_parked();
        };

        // The caret is placed once the file has been read; -c counts tab stops like the status
        // bar's Col.
        open(&["-n2", "-c6", "-ro", "a b.txt", "other.txt"], cx);
        assert_eq!(tab_names(&workspace, cx), ["a b.txt", "other.txt"]);
        assert_eq!(caret(&workspace, cx), (1, 6));
        cx.simulate_input("x");
        assert_eq!(active_text(&workspace, cx), "other");
        cx.simulate_keystrokes("ctrl-shift-tab");
        assert_eq!(caret(&workspace, cx), (2, 6));

        // A later launch with an open file switches to its tab and moves the caret.
        open(&["-n3", "a b.txt"], cx);
        assert_eq!(caret(&workspace, cx), (3, 1));
        cx.simulate_input("x");
        assert_eq!(active_text(&workspace, cx), "one\n\ttwo\nthree\n");
        open(&["-p5", "other.txt"], cx);
        assert_eq!(caret(&workspace, cx), (1, 6));

        // Line numbers past the end go to the last line.
        open(&["-n99", "a b.txt"], cx);
        assert_eq!(caret(&workspace, cx), (4, 1));
    }

    #[gpui_kit::test]
    fn unknown_and_pending_commands_fail_cleanly(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        workspace.update_in(cx, |workspace, window, cx| {
            let unknown = workspace.dispatch(&Invocation::new("no.such"), window, cx);
            assert!(unknown.is_err());
            let bad_args = workspace.dispatch(
                &Invocation::with_args("edit.insert-text", serde_json::json!({ "txt": 1 })),
                window,
                cx,
            );
            assert!(bad_args.is_err());
        });
    }
}
