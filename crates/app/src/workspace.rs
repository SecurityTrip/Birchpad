//! The workspace: the contents of a main window. It owns the panes, routes commands and draws
//! the menu bar and the status bar.

use anyhow::Result;
use birchpad_commands::Invocation;
use birchpad_core::motion::{line_count, line_of, line_range};
use birchpad_core::{Document, LineEnding, Range};
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::menu::AppMenuBar;
use gpui_kit::component::notification::Notification;
use gpui_kit::{
    App, AppContext as _, Context, Entity, FocusHandle, Focusable, PromptLevel, Subscription,
    Window, div, prelude::*, px, rgb,
};

use crate::buffer::Buffer;
use crate::commands::{CommandRegistry, Handler, RunCommand};
use crate::editor::EditorView;
use crate::menus::{self, MenuState};
use crate::pane::{Pane, PaneEvent};

pub(crate) fn register_commands(registry: &mut CommandRegistry) {
    registry.workspace("file.new", |this, (), window, cx| {
        this.new_file(window, cx);
        Ok(())
    });
    registry.workspace("file.close", |this, (), window, cx| {
        if let Some(view) = this.active_view(cx) {
            this.close_view(&view, window, cx);
        }
        Ok(())
    });
    registry.workspace("file.close-all", |this, (), window, cx| {
        for view in this.all_views(cx) {
            this.close_view(&view, window, cx);
        }
        Ok(())
    });
    registry.workspace("file.close-others", |this, (), window, cx| {
        let active = this.active_view(cx);
        for view in this.all_views(cx) {
            if Some(&view) != active.as_ref() {
                this.close_view(&view, window, cx);
            }
        }
        Ok(())
    });
    registry.workspace("file.exit", |_, (), _, cx| {
        cx.quit();
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
    for id in [
        "file.open",
        "file.open-recent",
        "file.clear-recent",
        "file.save",
        "file.save-as",
        "file.save-all",
        "search.find",
        "search.replace",
        "search.find-next",
        "search.find-previous",
        "search.go-to",
        "view.word-wrap",
        "view.zoom-in",
        "view.zoom-out",
        "view.zoom-reset",
        "encoding.encode-in",
        "encoding.convert-to",
        "help.check-updates",
    ] {
        registry.pending(id);
    }
}

/// Shows an error to the user without interrupting them.
pub(crate) fn report_error(error: &anyhow::Error, window: &mut Window, cx: &mut App) {
    window.push_notification(Notification::error(format!("{error:#}")), cx);
}

pub(crate) struct Workspace {
    focus_handle: FocusHandle,
    panes: Vec<Entity<Pane>>,
    active_pane: usize,
    menu_bar: Option<Entity<AppMenuBar>>,
    title: String,
    _subscriptions: Vec<Subscription>,
}

impl Workspace {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let pane = cx.new(|_| Pane::new());
        let subscription = cx.subscribe_in(&pane, window, Self::on_pane_event);
        let mut this = Self {
            focus_handle: cx.focus_handle(),
            panes: vec![pane],
            active_pane: 0,
            menu_bar: None,
            title: String::new(),
            _subscriptions: vec![subscription],
        };
        this.refresh_menus(cx);
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
            PaneEvent::ActiveItemChanged => cx.notify(),
            PaneEvent::CloseRequested(view) => self.close_view(view, window, cx),
        }
    }

    pub(crate) fn active_pane(&self) -> &Entity<Pane> {
        &self.panes[self.active_pane]
    }

    pub(crate) fn active_view(&self, cx: &App) -> Option<Entity<EditorView>> {
        self.active_pane().read(cx).active_item().cloned()
    }

    fn all_views(&self, cx: &App) -> Vec<Entity<EditorView>> {
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

    /// Opens a document that has been read from `path`.
    pub(crate) fn open_document(
        &mut self,
        doc: Document,
        path: std::path::PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let buffer = cx.new(|_| Buffer::from_file(doc, path));
        self.add_buffer(buffer, window, cx);
    }

    fn add_buffer(&mut self, buffer: Entity<Buffer>, window: &mut Window, cx: &mut Context<Self>) {
        let view = cx.new(|cx| EditorView::new(buffer, cx));
        self.active_pane()
            .update(cx, |pane, cx| pane.add(view, window, cx));
        cx.notify();
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

    /// Rebuilds the menus from the current state.
    pub(crate) fn refresh_menus(&mut self, cx: &mut Context<Self>) {
        let checked = |_: &Invocation| false;
        let state = MenuState {
            recent_files: &[],
            character_sets: &[],
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
            self.title = title;
        }
    }

    fn status_bar(&self, cx: &App) -> impl IntoElement {
        let mut items = Vec::new();
        if let Some(view) = self.active_view(cx) {
            let view = view.read(cx);
            let doc = view.buffer.read(cx).doc();
            let text = doc.text();
            let primary = view.selection.primary();
            let line = line_of(text, primary.head);
            let column = text
                .slice(line_range(text, line).start..primary.head)
                .chars()
                .count();
            let selected: usize = view.selection.iter().map(Range::len).sum();
            items.push(format!(
                "Length : {}    Lines : {}",
                text.len(),
                line_count(text)
            ));
            items.push(format!(
                "Ln : {}    Col : {}    Sel : {selected}",
                line + 1,
                column + 1
            ));
            items.push(
                match doc.line_ending() {
                    LineEnding::CrLf => "Windows (CR LF)",
                    LineEnding::Lf => "Unix (LF)",
                    LineEnding::Cr => "Macintosh (CR)",
                }
                .to_owned(),
            );
            items.push("UTF-8".to_owned());
        }
        div()
            .flex()
            .flex_row()
            .gap_6()
            .px_3()
            .h(px(24.))
            .flex_none()
            .items_center()
            .border_t_1()
            .border_color(rgb(0xd0d7de))
            .bg(rgb(0xf6f8fa))
            .text_size(px(12.))
            .children(items)
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.update_title(window, cx);
        div()
            .id("workspace")
            .key_context("Workspace")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::run_command))
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
            .child(self.status_bar(cx))
    }
}

impl Focusable for Workspace {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

#[cfg(test)]
mod tests {
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
            crate::commands::init(None, cx);
        });
        cx.add_window_view(|window, cx| {
            let mut workspace = Workspace::new(window, cx);
            workspace.new_file(window, cx);
            workspace
        })
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

    fn active_text(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> String {
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
        assert_eq!(tab_names(&workspace, cx), ["new 1"]);
        assert_eq!(active_text(&workspace, cx), "");
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
