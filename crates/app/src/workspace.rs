//! The workspace: the contents of a main window. It owns the panes, routes commands and draws
//! the menu bar and the status bar.

use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use birchpad_cli::CommandLine;
use birchpad_commands::Invocation;
use birchpad_core::Document;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::menu::AppMenuBar;
use gpui_kit::component::notification::Notification;
use gpui_kit::{
    App, AppContext as _, Context, Entity, EntityId, ExternalPaths, FocusHandle, Focusable,
    PromptLevel, Subscription, Window, div, prelude::*, px, rgb,
};

use crate::app_state::AppState;
use crate::buffer::{Buffer, BufferEvent};
use crate::commands::{CommandRegistry, Handler, RunCommand};
use crate::editor::EditorView;
use crate::find::FindBar;
use crate::menus::{self, MenuState};
use crate::pane::{Pane, PaneEvent};
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
    registry.workspace("help.about", |_, (), window, cx| {
        let detail = format!("Version {}", env!("CARGO_PKG_VERSION"));
        // The answer does not matter; dropping the receiver leaves the dialog open.
        drop(window.prompt(PromptLevel::Info, "Birchpad", Some(&detail), &["OK"], cx));
        Ok(())
    });
    registry.pending("help.check-updates");
    registry.workspace("view.word-wrap", |this, (), _, cx| {
        let current = crate::editor::ViewSettings::read(cx).word_wrap;
        AppState::update_state(cx, |state, _| state.word_wrap = Some(!current));
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
    crate::encoding_ui::register_commands(registry);
    crate::file_ops::register_commands(registry);
    crate::find::register_commands(registry);
}

/// Shows an error to the user without interrupting them.
pub(crate) fn report_error(error: &anyhow::Error, window: &mut Window, cx: &mut App) {
    window.push_notification(Notification::error(format!("{error:#}")), cx);
}

/// Shows a warning to the user without interrupting them.
pub(crate) fn report_warning(message: impl Into<String>, window: &mut Window, cx: &mut App) {
    window.push_notification(Notification::warning(message.into()), cx);
}

pub(crate) struct Workspace {
    focus_handle: FocusHandle,
    panes: Vec<Entity<Pane>>,
    active_pane: usize,
    menu_bar: Option<Entity<AppMenuBar>>,
    pub(crate) find_bar: Entity<FindBar>,
    title: String,
    buffer_subscriptions: HashMap<EntityId, Subscription>,
    _subscriptions: Vec<Subscription>,
}

impl Workspace {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let pane = cx.new(|_| Pane::new());
        let subscription = cx.subscribe_in(&pane, window, Self::on_pane_event);
        let find_bar = cx.new(|cx| FindBar::new(window, cx));
        let find_events = cx.subscribe_in(&find_bar, window, Self::on_find_bar_event);
        let mut this = Self {
            focus_handle: cx.focus_handle(),
            panes: vec![pane],
            active_pane: 0,
            menu_bar: None,
            find_bar,
            title: String::new(),
            buffer_subscriptions: HashMap::new(),
            _subscriptions: vec![subscription, find_events],
        };
        this.refresh_menus(cx);
        // The window's close button: ask about unsaved changes first.
        let workspace = cx.entity().downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            workspace
                .update(cx, |workspace, cx| {
                    if workspace.has_unsaved_changes(cx) {
                        let views = workspace.all_views(cx);
                        let closing = workspace.close_with_confirmation(views, window, cx);
                        cx.spawn_in(window, async move |_, cx| {
                            if closing.await {
                                cx.update(|window, _| window.remove_window()).ok();
                            }
                        })
                        .detach();
                        false
                    } else {
                        workspace.remember_open_files(cx);
                        true
                    }
                })
                .unwrap_or(true)
        });
        if !cfg!(target_os = "macos") {
            this.menu_bar = Some(AppMenuBar::new(cx));
        }
        this
    }

    fn on_pane_event(
        &mut self,
        _: &Entity<Pane>,
        event: &PaneEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            PaneEvent::ActiveItemChanged => {
                self.refresh_menus(cx);
                cx.notify();
            }
            PaneEvent::CloseRequested(view) => self.close_view(view, window, cx),
        }
    }

    pub(crate) fn active_pane(&self) -> &Entity<Pane> {
        &self.panes[self.active_pane]
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
        let target = command_line.caret_target();
        for path in &command_line.files {
            let view = self.open_path_with(path, command_line.read_only, window, cx);
            if let Some(target) = target {
                view.update(cx, |view, cx| view.set_caret_target(target, cx));
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

    fn view_for_path(&self, path: &Path, cx: &App) -> Option<Entity<EditorView>> {
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
    }

    fn add_buffer(
        &mut self,
        buffer: Entity<Buffer>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<EditorView> {
        let subscription = cx.subscribe_in(&buffer, window, Self::on_buffer_event);
        self.buffer_subscriptions
            .insert(buffer.entity_id(), subscription);
        let view = cx.new(|cx| EditorView::new(buffer, window, cx));
        self.active_pane()
            .update(cx, |pane, cx| pane.add(view.clone(), window, cx));
        cx.notify();
        view
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
            BufferEvent::StateChanged | BufferEvent::Reloaded => {
                self.refresh_menus(cx);
                cx.notify();
            }
            BufferEvent::Edited { .. } => {}
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
        // Like Notepad++, closing the last document leaves an empty "new 1".
        if self.all_views(cx).is_empty() {
            self.new_file(window, cx);
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
        let handler = cx.global::<CommandRegistry>().get(&invocation.command);
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

    /// Redraws every editor after a view setting (zoom, word wrap) changed.
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
        let ansi = AppState::global(cx).ansi;
        let word_wrap = crate::editor::ViewSettings::read(cx).word_wrap;
        let checked = |invocation: &Invocation| {
            if invocation.command == "view.word-wrap" {
                return word_wrap;
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
                for path in paths.paths() {
                    this.open_path(path, window, cx);
                }
            }))
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0xffffff))
            .text_color(rgb(0x1f2328))
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
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .child(self.active_pane().clone()),
            )
            .when(self.find_bar.read(cx).visible, |this| {
                this.child(self.find_bar.clone())
            })
            .child(crate::status_bar::render(status, ansi, cx))
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
    use gpui_kit::{TestAppContext, VisualTestContext};

    /// `secondary-<key>` in the spelling GPUI's test harness expects on this platform.
    pub(crate) fn secondary(key: &str) -> String {
        let modifier = if cfg!(target_os = "macos") {
            "cmd"
        } else {
            "ctrl"
        };
        format!("{modifier}-{key}")
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
