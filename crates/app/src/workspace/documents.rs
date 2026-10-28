//! Documents: new ones, opening files and the command line, following buffers, closing.

use std::path::Path;

use birchpad_cli::CommandLine;
use birchpad_core::Document;
use gpui_kit::{App, AppContext as _, Context, Entity, EntityId, Window};

use crate::app_state::AppState;
use crate::buffer::{Buffer, BufferEvent};
use crate::editor::EditorView;
use birchpad_ui::{report_error, report_info, report_warning};

use super::Workspace;

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

impl Workspace {
    /// The lowest "new N" number not used by an open untitled buffer, as in Notepad++.
    pub(super) fn next_untitled_number(&self, cx: &App) -> usize {
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

    pub(super) fn open_command_line_files(
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
    pub(super) fn ghost_type(
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
    pub(super) fn open_path_with(
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

    pub(super) fn add_buffer(
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

    pub(super) fn on_buffer_event(
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
}
