//! Minimize to the system tray (Notepad++'s Preferences > MISC), on Windows and macOS: the
//! window leaves the taskbar (on macOS, the Dock) for an icon in the notification area (the menu
//! bar), and a click on the icon brings it back (ADR 0056). GPUI has no tray icon, so this is
//! each platform's own calls beside GPUI's event loop. Linux has none (roadmap 7.4): there
//! `-systemtray` starts the window minimized and the setting is disabled.

use anyhow::Result;
use raw_window_handle::HasWindowHandle;

/// Whether this platform has a tray Birchpad can go to.
pub const SUPPORTED: bool = cfg!(any(windows, target_os = "macos"));

/// How the window starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Start {
    Normal,
    /// `-systemtray` where there is no tray: minimized.
    Minimized,
    /// `-systemtray`: in the tray.
    InTray,
}

/// How the window starts with `-systemtray` (`system_tray`), where the tray is `supported`.
pub fn start(system_tray: bool, supported: bool) -> Start {
    match (system_tray, supported) {
        (false, _) => Start::Normal,
        (true, true) => Start::InTray,
        (true, false) => Start::Minimized,
    }
}

/// Follows Preferences > MISC > Minimize to system tray: whether minimizing the window sends it
/// to the tray. The platform's callbacks read it, which have no `App`.
pub fn apply(minimize_to_tray: bool) {
    flag::set(SUPPORTED && minimize_to_tray);
}

/// Whether minimizing the window sends it to the tray now.
pub fn minimizes_to_tray() -> bool {
    flag::get()
}

/// Watches `window` for being minimized, to send it to the tray then if the settings say so.
pub fn watch(window: &impl HasWindowHandle) -> Result<()> {
    #[cfg(feature = "test-support")]
    {
        let _ = window;
        WATCHED.with(|watched| watched.set(true));
        Ok(())
    }
    #[cfg(not(feature = "test-support"))]
    {
        native::watch(raw(window)?)
    }
}

/// Sends `window` to the tray at once (`-systemtray`), whatever the settings.
pub fn send(window: &impl HasWindowHandle) -> Result<()> {
    #[cfg(feature = "test-support")]
    {
        let _ = window;
        IN_TRAY.with(|in_tray| in_tray.set(true));
        Ok(())
    }
    #[cfg(not(feature = "test-support"))]
    {
        native::send(raw(window)?)
    }
}

#[cfg(not(feature = "test-support"))]
fn raw(window: &impl HasWindowHandle) -> Result<raw_window_handle::RawWindowHandle> {
    // The trait's handle, not GPUI's own `window_handle`.
    Ok(HasWindowHandle::window_handle(window)
        .map_err(|error| anyhow::anyhow!("{error}"))?
        .as_raw())
}

#[cfg(feature = "test-support")]
thread_local! {
    /// Whether tests' windows are watched, and sent to the tray, instead of the system's.
    pub static WATCHED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    pub static IN_TRAY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// The setting as the platform's callbacks read it: a process-wide flag, one per test thread
/// in tests.
mod flag {
    #[cfg(not(feature = "test-support"))]
    static ON: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

    #[cfg(not(feature = "test-support"))]
    pub(super) fn set(on: bool) {
        ON.store(on, std::sync::atomic::Ordering::Relaxed);
    }

    #[cfg(not(feature = "test-support"))]
    pub(super) fn get() -> bool {
        ON.load(std::sync::atomic::Ordering::Relaxed)
    }

    #[cfg(feature = "test-support")]
    thread_local! {
        static ON: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }

    #[cfg(feature = "test-support")]
    pub(super) fn set(on: bool) {
        ON.with(|flag| flag.set(on));
    }

    #[cfg(feature = "test-support")]
    pub(super) fn get() -> bool {
        ON.with(std::cell::Cell::get)
    }
}

#[cfg(all(windows, not(feature = "test-support")))]
mod native {
    use std::sync::OnceLock;

    use anyhow::{Result, bail};
    use raw_window_handle::RawWindowHandle;
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::Shell::{
        DefSubclassProc, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
        RemoveWindowSubclass, SetWindowSubclass, Shell_NotifyIconW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        IDI_APPLICATION, LoadIconW, RegisterWindowMessageW, SIZE_MINIMIZED, SW_HIDE, SW_RESTORE,
        SetForegroundWindow, ShowWindow, WM_APP, WM_LBUTTONUP, WM_NCDESTROY, WM_RBUTTONUP, WM_SIZE,
    };

    /// The id of Birchpad's subclass of GPUI's window procedure, and of its icon.
    const ID: usize = 0xB12C;
    /// The message the icon sends the window when clicked.
    const TRAY_MESSAGE: u32 = WM_APP + 0x42;

    fn hwnd(handle: RawWindowHandle) -> Result<HWND> {
        let RawWindowHandle::Win32(handle) = handle else {
            bail!("not a Win32 window");
        };
        Ok(handle.hwnd.get() as HWND)
    }

    pub(super) fn watch(handle: RawWindowHandle) -> Result<()> {
        let hwnd = hwnd(handle)?;
        // SAFETY: the handle is this process's live main window, given by GPUI on the thread
        // that created it, as subclassing requires; `subclass` passes every message it does
        // not handle on to GPUI's procedure, and removes itself when the window is destroyed.
        #[allow(unsafe_code)]
        let done = unsafe { SetWindowSubclass(hwnd, Some(subclass), ID, 0) };
        if done == 0 {
            bail!("{}", std::io::Error::last_os_error());
        }
        Ok(())
    }

    pub(super) fn send(handle: RawWindowHandle) -> Result<()> {
        let hwnd = hwnd(handle)?;
        // The icon's clicks come to the subclass.
        watch(handle)?;
        to_tray(hwnd);
        Ok(())
    }

    /// The message Explorer sends when it starts again (after a crash): icons must be added
    /// again.
    fn taskbar_created() -> u32 {
        static MESSAGE: OnceLock<u32> = OnceLock::new();
        *MESSAGE.get_or_init(|| {
            let name: Vec<u16> = "TaskbarCreated\0".encode_utf16().collect();
            // SAFETY: `name` is a NUL-terminated UTF-16 string that outlives the call.
            #[allow(unsafe_code)]
            unsafe {
                RegisterWindowMessageW(name.as_ptr())
            }
        })
    }

    /// Birchpad's part of the window's procedure: minimized, the window goes to the tray if
    /// the settings say so; the icon clicked, it comes back.
    #[allow(unsafe_code)]
    unsafe extern "system" fn subclass(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _id: usize,
        _data: usize,
    ) -> LRESULT {
        // SAFETY (for the calls below): `hwnd` is the live window this procedure was set on,
        // and the messages come on its thread.
        match message {
            WM_SIZE if wparam as u32 == SIZE_MINIMIZED && super::minimizes_to_tray() => {
                // GPUI first, so that it knows the window is minimized.
                let result = unsafe { DefSubclassProc(hwnd, message, wparam, lparam) };
                to_tray(hwnd);
                result
            }
            TRAY_MESSAGE => {
                if matches!(lparam as u32, WM_LBUTTONUP | WM_RBUTTONUP) {
                    from_tray(hwnd);
                }
                0
            }
            WM_NCDESTROY => {
                remove_icon(hwnd);
                unsafe { RemoveWindowSubclass(hwnd, Some(subclass), ID) };
                unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
            }
            _ if message == taskbar_created() => {
                // Explorer started again: the icon, if the window is in the tray.
                // SAFETY: as above.
                if unsafe { windows_sys::Win32::UI::WindowsAndMessaging::IsWindowVisible(hwnd) }
                    == 0
                {
                    add_icon(hwnd);
                }
                unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
            }
            _ => unsafe { DefSubclassProc(hwnd, message, wparam, lparam) },
        }
    }

    /// The icon's data: the executable's icon (resource 1, as GPUI's windows have) and
    /// "Birchpad" as its tooltip.
    fn icon_data(hwnd: HWND) -> NOTIFYICONDATAW {
        // SAFETY: NOTIFYICONDATAW is plain data, valid zeroed; the module handle of this
        // executable lives as long as the process, and loading its icon resource 1, or the
        // system's, only reads.
        #[allow(unsafe_code)]
        let (mut data, icon) = unsafe {
            let module = GetModuleHandleW(std::ptr::null());
            let mut icon = LoadIconW(module, 1 as _);
            if icon.is_null() {
                icon = LoadIconW(std::ptr::null_mut(), IDI_APPLICATION);
            }
            (std::mem::zeroed::<NOTIFYICONDATAW>(), icon)
        };
        data.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
        data.hWnd = hwnd;
        data.uID = ID as u32;
        data.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
        data.uCallbackMessage = TRAY_MESSAGE;
        data.hIcon = icon;
        for (slot, unit) in data.szTip.iter_mut().zip("Birchpad".encode_utf16()) {
            *slot = unit;
        }
        data
    }

    fn add_icon(hwnd: HWND) {
        let data = icon_data(hwnd);
        // SAFETY: `data` is filled for this live window and outlives the call.
        #[allow(unsafe_code)]
        unsafe {
            Shell_NotifyIconW(NIM_ADD, &data)
        };
    }

    fn remove_icon(hwnd: HWND) {
        let data = icon_data(hwnd);
        // SAFETY: as in `add_icon`; removing an icon that is not there does nothing.
        #[allow(unsafe_code)]
        unsafe {
            Shell_NotifyIconW(NIM_DELETE, &data)
        };
    }

    fn to_tray(hwnd: HWND) {
        // SAFETY: `hwnd` is this process's live main window, on its own thread.
        #[allow(unsafe_code)]
        unsafe {
            ShowWindow(hwnd, SW_HIDE)
        };
        add_icon(hwnd);
    }

    fn from_tray(hwnd: HWND) {
        remove_icon(hwnd);
        // SAFETY: as in `to_tray`.
        #[allow(unsafe_code)]
        unsafe {
            ShowWindow(hwnd, SW_RESTORE);
            SetForegroundWindow(hwnd);
        }
    }
}

/// Elsewhere (Linux) there is no tray Birchpad can use: the callers check [`SUPPORTED`] first,
/// and these only keep the release build whole.
#[cfg(all(not(any(windows, target_os = "macos")), not(feature = "test-support")))]
mod native {
    use anyhow::{Result, bail};
    use raw_window_handle::RawWindowHandle;

    pub(super) fn watch(_: RawWindowHandle) -> Result<()> {
        bail!("this system has no tray Birchpad can use")
    }

    pub(super) fn send(_: RawWindowHandle) -> Result<()> {
        bail!("this system has no tray Birchpad can use")
    }
}

#[cfg(all(target_os = "macos", not(feature = "test-support")))]
mod native {
    use std::cell::RefCell;
    use std::ptr::NonNull;

    use anyhow::{Result, anyhow, bail};
    use block2::RcBlock;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyObject, ProtocolObject};
    use objc2::{MainThreadMarker, Message as _, sel};
    use objc2_app_kit::{
        NSApplication, NSApplicationActivationPolicy, NSApplicationDidUnhideNotification,
        NSStatusBar, NSStatusItem, NSVariableStatusItemLength, NSView, NSWindow,
        NSWindowDidMiniaturizeNotification,
    };
    use objc2_foundation::{
        NSCopying as _, NSNotification, NSNotificationCenter, NSObjectProtocol, NSOperationQueue,
        NSSize, NSString,
    };
    use raw_window_handle::RawWindowHandle;

    thread_local! {
        /// The menu bar's icon while Birchpad is in it.
        static ITEM: RefCell<Option<Retained<NSStatusItem>>> = const { RefCell::new(None) };
        /// The window that went there, to bring back out of the Dock.
        static WINDOW: RefCell<Option<Retained<NSWindow>>> = const { RefCell::new(None) };
        /// The observers of the notifications, kept for the life of the process.
        static OBSERVERS: RefCell<Vec<Retained<ProtocolObject<dyn NSObjectProtocol>>>> =
            const { RefCell::new(Vec::new()) };
    }

    fn window(handle: RawWindowHandle) -> Result<Retained<NSWindow>> {
        let RawWindowHandle::AppKit(handle) = handle else {
            bail!("not an AppKit window");
        };
        // SAFETY: `ns_view` is the content view of this process's live main window, given by
        // GPUI on the main thread for the window being updated.
        #[allow(unsafe_code)]
        let view: &NSView = unsafe { handle.ns_view.cast().as_ref() };
        view.window()
            .ok_or_else(|| anyhow!("the view has no window"))
    }

    fn main_thread() -> Result<MainThreadMarker> {
        MainThreadMarker::new().ok_or_else(|| anyhow!("not on the main thread"))
    }

    pub(super) fn watch(handle: RawWindowHandle) -> Result<()> {
        let window = window(handle)?;
        main_thread()?;
        let center = NSNotificationCenter::defaultCenter();
        let watched = window.clone();
        let minimized = RcBlock::new(move |_: NonNull<NSNotification>| {
            if super::minimizes_to_tray() {
                to_tray(&watched);
            }
        });
        let unhidden = RcBlock::new(|_: NonNull<NSNotification>| from_tray());
        let object: &AnyObject = (*window).as_ref();
        let queue = NSOperationQueue::mainQueue();
        // SAFETY: the window is the object whose notifications are observed, and both blocks
        // run on the main queue, on the main thread, where AppKit is called.
        #[allow(unsafe_code)]
        let observers = unsafe {
            [
                center.addObserverForName_object_queue_usingBlock(
                    Some(NSWindowDidMiniaturizeNotification),
                    Some(object),
                    Some(&queue),
                    &minimized,
                ),
                center.addObserverForName_object_queue_usingBlock(
                    Some(NSApplicationDidUnhideNotification),
                    None,
                    Some(&queue),
                    &unhidden,
                ),
            ]
        };
        OBSERVERS.with(|kept| kept.borrow_mut().extend(observers));
        Ok(())
    }

    pub(super) fn send(handle: RawWindowHandle) -> Result<()> {
        let window = window(handle)?;
        main_thread()?;
        to_tray(&window);
        Ok(())
    }

    /// Hides Birchpad, out of the Dock, with an icon in the menu bar whose click unhides it.
    fn to_tray(window: &NSWindow) {
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        if ITEM.with(|item| item.borrow().is_some()) {
            return;
        }
        let app = NSApplication::sharedApplication(mtm);
        let item = NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);
        if let Some(button) = item.button(mtm) {
            if let Some(icon) = app.applicationIconImage() {
                let icon = icon.copy();
                icon.setSize(NSSize::new(18., 18.));
                button.setImage(Some(&icon));
            }
            button.setToolTip(Some(&NSString::from_str("Birchpad")));
            let target: &AnyObject = (*app).as_ref();
            // SAFETY: the application lives as long as the process, and `unhide:` is one of
            // its actions, taking the sender.
            #[allow(unsafe_code)]
            unsafe {
                button.setTarget(Some(target));
                button.setAction(Some(sel!(unhide:)));
            }
        }
        ITEM.with(|kept| *kept.borrow_mut() = Some(item));
        WINDOW.with(|kept| *kept.borrow_mut() = Some(window.retain()));
        app.hide(None);
        app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    }

    /// Unhidden (the icon clicked, which activates Birchpad too, or the application opened
    /// again): back in the Dock, the window out of it, the icon gone.
    fn from_tray() {
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        let Some(item) = ITEM.with(|item| item.borrow_mut().take()) else {
            return;
        };
        NSStatusBar::systemStatusBar().removeStatusItem(&item);
        let app = NSApplication::sharedApplication(mtm);
        app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
        if let Some(window) = WINDOW.with(|window| window.borrow_mut().take()) {
            if window.isMiniaturized() {
                window.deminiaturize(None);
            }
            window.makeKeyAndOrderFront(None);
        }
    }
}
