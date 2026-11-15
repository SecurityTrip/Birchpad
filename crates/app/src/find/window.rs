//! The find panel in a window of its own (Preferences > Searching > The find panel floats in a
//! window of its own, ADR 0070).
//!
//! A field of GPUI's belongs to the window it was made in (its focus, its caret's blinking), so
//! the panel is made again in the window it goes to, with what it held. The window answers no
//! command itself: what its keys ask for (F3, Escape) goes on to the main window, whose
//! workspace answers the panel's events as it does for the docked one.

use std::collections::HashSet;

use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, Bounds, Context, Entity, FocusHandle, Focusable, Global,
    TitlebarOptions, WeakEntity, Window, WindowBounds, WindowKind, WindowOptions, div, point,
    prelude::*, px, size,
};

use crate::commands::RunCommand;
use crate::find::FindBar;
use crate::workspace::Workspace;

/// The size the window opens at: the panel with its buttons in a column at the right.
const SIZE: (f32, f32) = (810., 370.);

/// The root of the find panel's window.
pub struct FindWindow {
    pub bar: Entity<FindBar>,
    /// The window of the workspace whose panel this is.
    main: AnyWindowHandle,
    focus_handle: FocusHandle,
}

impl FindWindow {
    fn new(
        bar: Entity<FindBar>,
        main: AnyWindowHandle,
        workspace: WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        // The window's close button: the panel is closed, as its own Close button closes it.
        window.on_window_should_close(cx, move |_, cx| {
            workspace
                .update(cx, |workspace, cx| workspace.find_window_closed(cx))
                .ok();
            true
        });
        Self {
            bar,
            main,
            focus_handle: cx.focus_handle(),
        }
    }

    /// A command of a key pressed here, for the main window: its workspace runs the commands.
    fn forward(&mut self, command: &RunCommand, _: &mut Window, cx: &mut Context<Self>) {
        let (command, main) = (command.clone(), self.main);
        cx.defer(move |cx| {
            main.update(cx, |_, window, cx| {
                window.dispatch_action(Box::new(command), cx);
            })
            .ok();
        });
    }
}

impl Focusable for FindWindow {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for FindWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("find-window")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::forward))
            .size_full()
            .bg(crate::theme::paint(crate::theme::ui().surface))
            .text_color(crate::theme::paint(crate::theme::ui().text))
            .child(self.bar.clone())
    }
}

/// The find windows open, so that closing the last main window still quits.
#[derive(Default)]
struct FindWindows(HashSet<AnyWindowHandle>);

impl Global for FindWindows {}

/// Whether `window` is a find panel's window.
pub fn is_find_window(window: AnyWindowHandle, cx: &App) -> bool {
    cx.try_global::<FindWindows>()
        .is_some_and(|windows| windows.0.contains(&window))
}

/// Where the window opens: near the top right corner of the main window, over its documents.
pub fn placement(main: Bounds<gpui_kit::Pixels>) -> Bounds<gpui_kit::Pixels> {
    let (width, height) = (px(SIZE.0), px(SIZE.1));
    let x = (main.right() - width - px(32.)).max(main.left());
    let y = main.top() + px(96.);
    Bounds::new(point(x, y), size(width, height))
}

impl Workspace {
    /// The find panel's window, while it is open.
    pub fn find_panel_window(&self) -> Option<AnyWindowHandle> {
        self.find_window_handle
    }

    /// The panel's window, opened with what the docked panel held if it is not open; the panel
    /// is then the one in it.
    pub(crate) fn find_window(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyWindowHandle> {
        if let Some(open) = self.find_window_handle
            && cx.windows().contains(&open)
        {
            return Some(open);
        }
        let contents = self.find_bar.read(cx).contents(cx);
        let main = window.window_handle();
        let owner = native::handle(window);
        let workspace = cx.entity().downgrade();
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(placement(window.bounds()))),
            titlebar: Some(TitlebarOptions {
                title: Some(t!("find.find").into()),
                ..TitlebarOptions::default()
            }),
            kind: WindowKind::Floating,
            is_minimizable: false,
            app_id: Some(crate::APP_ID.to_owned()),
            ..WindowOptions::default()
        };
        let opened = gpui_kit::open_window(options, cx, |find, cx| {
            if let Some(owner) = owner {
                native::keep_above(find, owner);
            }
            let bar = cx.new(|cx| FindBar::with_contents(contents, find, cx));
            cx.new(|cx| FindWindow::new(bar, main, workspace, find, cx))
        });
        let (handle, find_window) = match opened {
            Ok(opened) => opened,
            Err(error) => {
                self.report(Some(format!("{error:#}")), true, cx);
                return None;
            }
        };
        cx.default_global::<FindWindows>().0.insert(handle);
        let bar = find_window.read(cx).bar.clone();
        self.set_find_bar(bar, window, cx);
        self.find_window_handle = Some(handle);
        Some(handle)
    }

    /// The panel the workspace answers from now on.
    fn set_find_bar(&mut self, bar: Entity<FindBar>, window: &mut Window, cx: &mut Context<Self>) {
        self.find_events = cx.subscribe_in(&bar, window, Self::on_find_bar_event);
        self.find_bar = bar;
    }

    /// Shows the panel's window in front, its find field with the keyboard.
    pub(crate) fn focus_find_window(&self, handle: AnyWindowHandle, cx: &mut App) {
        let bar = self.find_bar.clone();
        handle
            .update(cx, |_, window, cx| {
                window.activate_window();
                bar.update(cx, |bar, cx| bar.focus_find_field(window, cx));
            })
            .ok();
    }

    /// The panel's window was closed by its close button: the panel is closed.
    pub(crate) fn find_window_closed(&mut self, cx: &mut Context<Self>) {
        if let Some(handle) = self.find_window_handle.take() {
            cx.default_global::<FindWindows>().0.remove(&handle);
        }
        self.find_bar.update(cx, |bar, cx| {
            bar.visible = false;
            cx.notify();
        });
        cx.notify();
    }

    /// Closes the panel's window, if it is open.
    pub(crate) fn close_find_window(&mut self, cx: &mut Context<Self>) {
        let Some(handle) = self.find_window_handle else {
            return;
        };
        self.find_window_closed(cx);
        // After the event that asked for it: the window may be the one handling it.
        cx.defer(move |cx| {
            handle
                .update(cx, |_, window, _| window.remove_window())
                .ok();
        });
    }

    /// The panel's button: the docked panel goes into a window of its own, the panel in its
    /// window docks under the documents. Kept in the settings, as Preferences keeps it.
    pub fn toggle_find_floating(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let floats = !FindBar::floats(cx);
        let value = toml::Value::Boolean(floats);
        if let Err(error) = crate::preferences::set("searching.find-panel-floats", Some(value), cx)
        {
            self.report(Some(error.to_string()), true, cx);
            return;
        }
        if floats {
            if let Some(handle) = self.find_window(window, cx) {
                self.focus_find_window(handle, cx);
            }
        } else {
            let contents = self.find_bar.read(cx).contents(cx);
            self.close_find_window(cx);
            let bar = cx.new(|cx| FindBar::with_contents(contents, window, cx));
            bar.update(cx, |bar, cx| {
                bar.visible = true;
                bar.focus_find_field(window, cx);
            });
            self.set_find_bar(bar, window, cx);
            window.activate_window();
        }
        cx.notify();
    }
}

/// The panel's window kept above the main window, as a window it owns: Windows has owned
/// windows, which GPUI does not make (its `Floating` is a window like any other there).
mod native {
    use gpui_kit::Window;
    use raw_window_handle::RawWindowHandle;

    /// The main window's handle, for [`keep_above`]; none in the tests, which have no system
    /// windows.
    pub(super) fn handle(window: &Window) -> Option<RawWindowHandle> {
        if cfg!(feature = "test-support") {
            return None;
        }
        raw_window_handle::HasWindowHandle::window_handle(window)
            .ok()
            .map(|handle| handle.as_raw())
    }

    #[cfg(windows)]
    pub(super) fn keep_above(find: &Window, owner: RawWindowHandle) {
        use windows_sys::Win32::UI::WindowsAndMessaging::{GWLP_HWNDPARENT, SetWindowLongPtrW};
        let Ok(find) = raw_window_handle::HasWindowHandle::window_handle(find) else {
            return;
        };
        let (RawWindowHandle::Win32(find), RawWindowHandle::Win32(owner)) = (find.as_raw(), owner)
        else {
            return;
        };
        // SAFETY: both handles are live windows of this process, given by GPUI; setting the
        // owner only makes the find window stay above the main one and close with it.
        #[allow(unsafe_code)]
        unsafe {
            SetWindowLongPtrW(find.hwnd.get() as _, GWLP_HWNDPARENT, owner.hwnd.get() as _);
        }
    }

    /// macOS: GPUI's `Floating` is a floating window's level; X11 and Wayland: a window
    /// transient for, or a child of, the active one.
    #[cfg(not(windows))]
    pub(super) fn keep_above(_: &Window, _: RawWindowHandle) {}
}
