//! An editor in a test window, as the application shows one but without the rest of it: a
//! window holding one editor view of an untitled document, with the default keymap bound and
//! the editor's commands registered. The window also runs the commands of the workspace the
//! editor's tests need: Language > (a language) and the View menu's switches.

use anyhow::{Result, anyhow, bail};
use birchpad_commands::{Invocation, Scope};
use birchpad_core::Document;
use birchpad_ui::{AppState, CommandRegistry, CommandTarget, RunCommand, report_error};
use gpui_kit::{
    App, AppContext as _, Context, Entity, FocusHandle, Focusable, IntoElement, Render,
    TestAppContext, VisualTestContext, Window, div, prelude::*,
};
use serde::Deserialize;

use crate::buffer::Buffer;
use crate::{EditorView, ViewSettings};

/// The window's root: one editor view.
pub(crate) struct Workspace {
    focus_handle: FocusHandle,
    view: Entity<EditorView>,
}

impl CommandTarget for Workspace {
    const SCOPE: Scope = Scope::Workspace;
}

#[derive(Deserialize)]
struct LanguageArgs {
    language: String,
}

/// The editor's commands and the workspace's few.
fn registry() -> CommandRegistry {
    let mut registry = CommandRegistry::default();
    crate::register_commands(&mut registry);
    registry.register(
        "language.set",
        |this: &mut Workspace, args: LanguageArgs, _, cx| {
            let language = match args.language.as_str() {
                "text" | "normal" => None,
                id => Some(
                    birchpad_syntax::by_id(id).ok_or_else(|| anyhow!("unknown language {id}"))?,
                ),
            };
            let buffer = this.view.read(cx).buffer.clone();
            buffer.update(cx, |buffer, cx| buffer.set_language(language, cx));
            this.view.update(cx, |view, cx| view.settings_changed(cx));
            Ok(())
        },
    );
    for &(id, read, write) in crate::VIEW_SWITCHES {
        registry.register(id, move |this: &mut Workspace, (), _, cx| {
            let on = !read(&ViewSettings::read(cx));
            AppState::update_state(cx, |state, _| write(state, on));
            this.view.update(cx, |view, cx| view.settings_changed(cx));
            Ok(())
        });
    }
    registry.register(
        "view.show-all-characters",
        |this: &mut Workspace, (), _, cx| {
            let on = !crate::all_characters(&ViewSettings::read(cx));
            AppState::update_state(cx, |state, _| {
                state.show_whitespace = Some(on);
                state.show_eol = Some(on);
            });
            this.view.update(cx, |view, cx| view.settings_changed(cx));
            Ok(())
        },
    );
    registry
}

impl Workspace {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let buffer = cx.new(|_| Buffer::untitled_with(1, Document::new()));
        let view = cx.new(|cx| EditorView::new(buffer, window, cx));
        window.focus(&view.focus_handle(cx), cx);
        Self {
            focus_handle: cx.focus_handle(),
            view,
        }
    }

    /// Shows `doc` instead of the document shown, as an untitled one.
    pub(crate) fn open_document(
        &mut self,
        doc: Document,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let buffer = cx.new(|_| Buffer::untitled_with(1, doc));
        self.view = cx.new(|cx| EditorView::new(buffer, window, cx));
        window.focus(&self.view.focus_handle(cx), cx);
        cx.notify();
    }

    pub(crate) fn active_view(&self, _: &App) -> Option<Entity<EditorView>> {
        Some(self.view.clone())
    }

    /// Runs a command: the workspace's, or the editor's in its view.
    pub(crate) fn dispatch(
        &mut self,
        invocation: &Invocation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let registry = cx.global::<CommandRegistry>();
        if let Some(run) = registry.handler::<Self>(&invocation.command) {
            return run(self, invocation, window, cx);
        }
        let Some(run) = registry.handler::<EditorView>(&invocation.command) else {
            bail!("unknown command {}", invocation.command);
        };
        self.view
            .update(cx, |view, cx| run(view, invocation, window, cx))
    }

    fn run_command(&mut self, action: &RunCommand, window: &mut Window, cx: &mut Context<Self>) {
        if let Err(error) = self.dispatch(&action.0, window, cx) {
            report_error(&error, window, cx);
        }
    }
}

impl Focusable for Workspace {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Workspace {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("workspace")
            .key_context("Workspace")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::run_command))
            .size_full()
            .flex()
            .flex_col()
            .child(self.view.clone())
    }
}

/// Opens the test window, with the settings the editor's tests expect.
pub(crate) fn open_workspace(
    cx: &mut TestAppContext,
) -> (Entity<Workspace>, &mut VisualTestContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        // Dialogs open in place instead of sliding in over a quarter of a second.
        cx.set_reduce_motion(true);
        let mut settings = birchpad_config::resolve(birchpad_config::Sources::default());
        // Tests type brackets and press Enter as before auto-completion; its own tests turn it
        // on (`completion::tests`).
        settings.settings.completion.auto_close.clear();
        settings.settings.completion.words = false;
        cx.set_global(AppState::new(
            settings,
            birchpad_config::ConfigPaths::default(),
        ));
        cx.set_global(registry());
        birchpad_ui::bind_keymap(None, cx);
    });
    // gpui-component's Root, for notifications and dialogs.
    let (window, workspace) = cx.update(|cx| {
        let options = gpui_kit::WindowOptions {
            window_bounds: Some(gpui_kit::WindowBounds::Windowed(
                gpui_kit::Bounds::maximized(None, cx),
            )),
            ..Default::default()
        };
        gpui_kit::open_window(options, cx, |window, cx| {
            cx.new(|cx| Workspace::new(window, cx))
        })
        .expect("open test window")
    });
    let cx = VisualTestContext::from_window(window, cx).into_mut();
    // Active like the window the user works in: focus events are only reported then.
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    (workspace, cx)
}

/// The text of the editor.
pub(crate) fn active_text(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> String {
    workspace.read_with(cx, |workspace, cx| {
        workspace.view.read(cx).text(cx).to_string()
    })
}

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

/// Column-mode Begin/End Select: Alt+Shift+B on Windows and Linux, Cmd+Alt+Shift+B on macOS.
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

/// Clicks the middle of what `selector` marks, once it no longer moves (the test window turns
/// motion off, so dialogs open in place without sliding in).
pub(crate) fn click(selector: &'static str, cx: &mut VisualTestContext) {
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    let Some(mut bounds) = cx.debug_bounds(selector) else {
        panic!("{selector} is not drawn");
    };
    for _ in 0..100 {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
        let Some(now) = cx.debug_bounds(selector) else {
            panic!("{selector} is no longer drawn");
        };
        if now == bounds {
            break;
        }
        bounds = now;
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let center = bounds.center();
    cx.simulate_mouse_down(
        center,
        gpui_kit::MouseButton::Left,
        gpui_kit::Modifiers::none(),
    );
    cx.simulate_mouse_up(
        center,
        gpui_kit::MouseButton::Left,
        gpui_kit::Modifiers::none(),
    );
    cx.run_until_parked();
}

/// Whether a dialog is open.
pub(crate) fn has_dialog(cx: &mut VisualTestContext) -> bool {
    use gpui_kit::component::WindowExt as _;
    cx.update(|window, cx| window.has_active_dialog(cx))
}
