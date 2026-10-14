//! How the window shows, as Notepad++'s View menu: Always on Top, Full Screen, Post-It (the
//! text alone, without menu, tabs, panels and status bar) and Distraction Free (full screen,
//! the text alone in a column in the middle); and Move to New Instance / Open in New Instance,
//! which hand the document to a new Birchpad of its own.

use std::ffi::OsString;

use anyhow::{Result, anyhow, bail};
use gpui_kit::{Context, Window};

use crate::commands::{CommandRegistry, Register as _};
use crate::workspace::Workspace;

/// The window's modes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct WindowModes {
    pub(crate) always_on_top: bool,
    pub(crate) full_screen: bool,
    pub(crate) post_it: bool,
    pub(crate) distraction_free: bool,
}

impl WindowModes {
    /// Only the text shows: no menu bar, tabs, panels or status bar.
    pub(crate) fn text_only(self) -> bool {
        self.post_it || self.distraction_free
    }
}

pub(crate) fn register_commands(registry: &mut CommandRegistry) {
    registry.workspace("view.always-on-top", |this, (), window, cx| {
        let on = !this.modes.always_on_top;
        set_always_on_top(window, on)?;
        this.modes.always_on_top = on;
        this.refresh_menus(cx);
        Ok(())
    });
    registry.workspace("view.full-screen", |this, (), window, cx| {
        this.modes.full_screen = !this.modes.full_screen;
        // Distraction Free is full screen too; leaving it leaves that.
        this.modes.distraction_free = false;
        this.set_full_screen(this.modes.full_screen, window, cx);
        Ok(())
    });
    registry.workspace("view.post-it", |this, (), window, cx| {
        this.modes.post_it = !this.modes.post_it;
        this.modes.distraction_free = false;
        this.set_full_screen(this.modes.full_screen, window, cx);
        Ok(())
    });
    registry.workspace("view.distraction-free", |this, (), window, cx| {
        let on = !this.modes.distraction_free;
        this.modes.distraction_free = on;
        this.modes.post_it = false;
        this.set_full_screen(on || this.modes.full_screen, window, cx);
        Ok(())
    });
    for (id, keep) in [
        ("view.move-to-new-instance", false),
        ("view.open-in-new-instance", true),
    ] {
        registry.workspace(id, move |this, (), window, cx| {
            this.send_to_new_instance(keep, window, cx)
        });
    }
}

/// Whether View's switches are checked.
pub(crate) fn is_checked(command: &str, modes: WindowModes) -> Option<bool> {
    match command {
        "view.always-on-top" => Some(modes.always_on_top),
        "view.full-screen" => Some(modes.full_screen),
        "view.post-it" => Some(modes.post_it),
        "view.distraction-free" => Some(modes.distraction_free),
        _ => None,
    }
}

impl Workspace {
    /// Makes the window full screen or not, whatever it is now, and redraws without or with
    /// the menu bar, tabs, panels and status bar.
    fn set_full_screen(&mut self, on: bool, window: &mut Window, cx: &mut Context<Self>) {
        if window.is_fullscreen() != on {
            window.toggle_fullscreen();
        }
        let hidden = self.modes.text_only();
        for pane in self.panes().clone() {
            pane.update(cx, |pane, cx| pane.set_tabs_hidden(hidden, cx));
        }
        self.refresh_menus(cx);
        cx.notify();
    }

    /// Ends Post-It and Distraction Free (the button shown in them, and Escape).
    pub(crate) fn leave_text_only(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let full_screen = self.modes.full_screen;
        self.modes.post_it = false;
        self.modes.distraction_free = false;
        self.set_full_screen(full_screen, window, cx);
    }

    /// View > Move to New Instance (`keep` false) or Open in New Instance (`keep`): starts
    /// another Birchpad with the active file at the caret; a moved tab closes here.
    fn send_to_new_instance(
        &mut self,
        keep: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let view = self
            .active_view(cx)
            .ok_or_else(|| anyhow!("{}", t!("file-ops.no-document")))?;
        let (path, modified, name) = {
            let buffer = view.read(cx).buffer.read(cx);
            (
                buffer.path().map(std::path::Path::to_owned),
                buffer.is_modified(),
                buffer.display_name(),
            )
        };
        let Some(path) = path else {
            bail!("{}", t!("file-commands.has-no-file-yet", name = name));
        };
        if modified {
            bail!("{}", t!("window-modes.save-first", name = name));
        }
        let caret = view.update(cx, |view, cx| crate::status_bar::StatusInfo::of(view, cx));
        let (line, column) = (caret.line, caret.column);
        launch(instance_args(&path, line, column))?;
        if !keep {
            self.close_view(&view, window, cx);
        }
        Ok(())
    }
}

/// The command line of a new instance for `path` with the caret at `line` and `column`
/// (1-based): its own window, without the session.
pub(crate) fn instance_args(path: &std::path::Path, line: usize, column: usize) -> Vec<OsString> {
    vec![
        "-multiInst".into(),
        "-nosession".into(),
        format!("-n{line}").into(),
        format!("-c{column}").into(),
        "--".into(),
        path.as_os_str().to_owned(),
    ]
}

#[cfg(test)]
thread_local! {
    /// The command lines tests started instances with, instead of starting them.
    pub(crate) static LAUNCHED: std::cell::RefCell<Vec<Vec<OsString>>> =
        std::cell::RefCell::default();
    /// Always on Top as tests set it, instead of the window's.
    pub(crate) static ON_TOP: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Starts this program again with `args` (in tests, only notes them).
fn launch(args: Vec<OsString>) -> Result<()> {
    #[cfg(test)]
    {
        LAUNCHED.with(|launched| launched.borrow_mut().push(args));
        Ok(())
    }
    #[cfg(not(test))]
    {
        let program = std::env::current_exe()?;
        std::process::Command::new(program)
            .args(args)
            .spawn()
            .map(|_| ())
            .map_err(|error| anyhow!("{}", t!("window-modes.cannot-start", error = error)))
    }
}

/// Keeps the window above the others, or not.
fn set_always_on_top(window: &Window, on: bool) -> Result<()> {
    #[cfg(test)]
    {
        let _ = window;
        ON_TOP.with(|top| top.set(on));
        Ok(())
    }
    #[cfg(not(test))]
    {
        use raw_window_handle::HasWindowHandle;
        // The trait's handle, not GPUI's own `window_handle`.
        let handle = HasWindowHandle::window_handle(window)
            .map_err(|error| anyhow!("{error}"))?
            .as_raw();
        native::set_always_on_top(handle, on)
            .map_err(|error| anyhow!("{}", t!("window-modes.cannot-stay-on-top", error = error)))
    }
}

#[cfg(windows)]
mod native {
    use raw_window_handle::RawWindowHandle;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetWindowPos,
    };

    pub(super) fn set_always_on_top(handle: RawWindowHandle, on: bool) -> anyhow::Result<()> {
        let RawWindowHandle::Win32(handle) = handle else {
            anyhow::bail!("not a Win32 window");
        };
        let after = if on { HWND_TOPMOST } else { HWND_NOTOPMOST };
        // SAFETY: the handle is this process's live main window, given by GPUI for the window
        // being updated; SetWindowPos only changes its z-order (no move, size or activation).
        #[allow(unsafe_code)]
        let done = unsafe {
            SetWindowPos(
                handle.hwnd.get() as _,
                after,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            )
        };
        if done == 0 {
            anyhow::bail!("{}", std::io::Error::last_os_error());
        }
        Ok(())
    }
}

#[cfg(target_os = "macos")]
mod native {
    use objc2_app_kit::{NSFloatingWindowLevel, NSNormalWindowLevel, NSView};
    use raw_window_handle::RawWindowHandle;

    pub(super) fn set_always_on_top(handle: RawWindowHandle, on: bool) -> anyhow::Result<()> {
        let RawWindowHandle::AppKit(handle) = handle else {
            anyhow::bail!("not an AppKit window");
        };
        // SAFETY: `ns_view` is the content view of this process's live main window, given by
        // GPUI on the main thread for the window being updated.
        #[allow(unsafe_code)]
        let view: &NSView = unsafe { handle.ns_view.cast().as_ref() };
        let window = view
            .window()
            .ok_or_else(|| anyhow::anyhow!("the view has no window"))?;
        window.setLevel(if on {
            NSFloatingWindowLevel
        } else {
            NSNormalWindowLevel
        });
        Ok(())
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod native {
    use raw_window_handle::RawWindowHandle;
    use x11rb::connection::Connection as _;
    use x11rb::protocol::xproto::{ClientMessageEvent, ConnectionExt as _, EventMask};

    /// Asks the window manager through EWMH's `_NET_WM_STATE_ABOVE`; Wayland has no way for an
    /// application to do it.
    pub(super) fn set_always_on_top(handle: RawWindowHandle, on: bool) -> anyhow::Result<()> {
        let window = match handle {
            RawWindowHandle::Xlib(handle) => handle.window as u32,
            RawWindowHandle::Xcb(handle) => handle.window.get(),
            _ => anyhow::bail!("{}", t!("window-modes.not-on-wayland")),
        };
        let (connection, screen) = x11rb::connect(None)?;
        let root = connection.setup().roots[screen].root;
        let atom = |name: &[u8]| -> anyhow::Result<u32> {
            Ok(connection.intern_atom(false, name)?.reply()?.atom)
        };
        let (state, above) = (atom(b"_NET_WM_STATE")?, atom(b"_NET_WM_STATE_ABOVE")?);
        let event = ClientMessageEvent::new(32, window, state, [u32::from(on), above, 0, 1, 0]);
        connection.send_event(
            false,
            root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            event,
        )?;
        connection.flush()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
