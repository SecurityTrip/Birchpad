# ADR 0070: The find panel in a window of its own

- Status: accepted
- Date: 2026-11-15
- Issue: [#71](https://github.com/SecurityTrip/Birchpad/issues/71)

## Context

The find panel was docked under the documents, the width of the window. Notepad++'s Find
dialog is a window of its own, which floats over the text where the user puts it, on another
monitor if they like, and #71 asks for the same, by default or by a button.

A field of GPUI's belongs to the window it was made in: it follows that window's focus and
activation, and its caret blinks there. The find panel's fields cannot be drawn in another
window as they are. And the panel's events (Find Next, Replace All) are answered by the
workspace of the main window, which holds the documents.

## Decision

The panel floats in a window of its own, or is docked under the documents as before.

- Preferences > Searching > The find panel in a window of its own
  (`searching.find-panel-floats`) chooses; it is on by default, as Notepad++'s dialog floats.
  A button beside the panel's Close docks the window under the documents, and floats the
  docked panel; it writes the setting, as Preferences does.
- The window opens near the top right corner of the main window, over its documents. It is
  GPUI's floating window: on macOS at the floating windows' level, on X11 and Wayland a window
  transient for the main one. On Windows, where GPUI's floating window is a window like any
  other, Birchpad makes the main window its owner, so that it stays above it and closes and
  minimizes with it.
- The panel moves by being made again: `FindBarContents` carries the fields' texts, the tab,
  the options and the state of the last search into a `FindBar` made in the window it goes to,
  and the workspace answers the new panel's events from then on.
- The window answers no command itself. A key pressed there that runs a command (F3, Escape,
  Ctrl+S) is sent on to the main window, whose workspace runs it as if the key were pressed
  there.
- Escape, the panel's Close and the window's close button close the window and the panel; the
  next Ctrl+F opens it again with what it held. Closing the main window quits, though the
  find window is open.
- Floating, the panel's buttons are in a column at its right, as in Notepad++'s dialog; docked,
  they stay beside the fields.

## Consequences

- The panel can be put anywhere, outside the main window and on another monitor.
- The tests that type into the docked panel keep it docked; the panel's window has tests of its
  own, which drive both windows. The end-to-end runs keep it docked, so that what a script
  types after Ctrl+F stays in the main window.
- Where the window was put is not kept across sessions.
