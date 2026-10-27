# ADR 0056: The system tray on Windows and macOS

- Status: accepted
- Date: 2026-10-28

## Context

Notepad++'s Preferences > MISC can minimize the window to the system tray: it leaves the taskbar
for an icon in the notification area, and a click on the icon brings it back; `-systemtray`
starts it there. Built from the user manual and the program's behavior, not its code.

GPUI (gpui-pre 0.3.10, gpui-kit 0.8.1) has no tray icon, no way to take a window off the taskbar,
and no event for a window being minimized. 0.7.0 waited for it (roadmap 7.4); it is still not
there, so Birchpad calls each platform itself, as it does for Always on Top
(`window_modes::native`).

## Decision

- `misc.minimize-to-tray` (Preferences > MISC > Minimize to system tray, off by default). The
  platforms' callbacks have no `App`, so `tray::apply` copies the setting into a flag they read,
  at startup and after every change in Preferences.
- **Windows**: the main window's procedure is subclassed (`SetWindowSubclass`), beside GPUI's.
  Minimized (`WM_SIZE` with `SIZE_MINIMIZED`, after GPUI has seen it), the window is hidden and
  an icon added with `Shell_NotifyIconW`: the executable's icon resource 1, "Birchpad" as its
  tooltip. A click on it removes the icon, restores the window and brings it to the front.
  When Explorer starts again (`TaskbarCreated`) the icon is added again; when the window is
  destroyed, it is removed with the subclass.
- **macOS**: the window's `NSWindowDidMiniaturizeNotification` hides the application, takes it
  out of the Dock (accessory activation policy) and puts its icon in the menu bar
  (`NSStatusItem`), whose click unhides it. `NSApplicationDidUnhideNotification`, from that
  click or from opening Birchpad again, takes the icon away, brings it back to the Dock and the
  window out of it.
- `-systemtray` starts the window in the tray on both, whatever the setting.
- **Linux** has none: a tray icon there is a D-Bus StatusNotifierItem that GNOME shows only with
  an extension, and Wayland lets no application hide its window and show it again. There
  `-systemtray` starts minimized as before, and the setting is disabled in Preferences.
- Tests fake the platforms' calls (`test-support`): they check the setting, how the window
  starts and that the window is watched; the icons themselves are checked by hand
  ([release checklist](../release-checklist.md)).

## Consequences

- `unsafe` code in `tray.rs`, each block with why it is sound, as for Always on Top.
- If GPUI gains a tray icon, `tray.rs`'s platform modules give way to it; the setting and the
  command line stay.
- A second window, if Birchpad ever opens one, would need watching too.
