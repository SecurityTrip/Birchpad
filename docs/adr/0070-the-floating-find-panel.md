# ADR 0070: The floating find panel

- Status: accepted
- Date: 2026-11-15
- Issue: [#71](https://github.com/SecurityTrip/Birchpad/issues/71)

## Context

The find panel was docked under the documents, the width of the window. Notepad++'s Find
dialog floats over the text, where the user puts it, and #71 asks for the same, by default or
by a button.

GPUI can open a second window of the operating system, and Notepad++'s dialog is one. The find
panel's fields, though, are inputs of the main window: their focus, the keys they take (Enter,
Escape, the shortcuts of the find commands) and the redraw after a search belong to the window
whose workspace answers the panel's events.

## Decision

The panel floats over the documents' area of the main window, not in a window of its own.

- Preferences > Searching > The find panel floats over the documents
  (`searching.find-panel-floats`) chooses; it is on by default, as Notepad++'s dialog floats.
  A button beside the panel's Close turns it off and on again and writes the setting, as
  Preferences does.
- Floating, the panel has a title, by which it is moved: a drag (`DraggedFindPanel`) that the
  documents' area follows and hands to `FindBar::drag_to`. Where it is (`FloatingPlace`) is
  measured from the area's top right corner, so that it stays in that corner when the window is
  made wider, and is kept where 120 by 24 pixels of it, enough of the title to take it back,
  stay in the area. The window remembers it until it closes.
- Floating, the buttons are in a column at the panel's right, as in Notepad++'s dialog; docked,
  they stay beside the fields.
- Nothing else changes: the panel is the same `FindBar`, with the same events.

## Consequences

- The fields, the focus and the keys work as they did docked; no second window to keep in step
  with the first, on three platforms.
- The panel cannot leave the main window, as Notepad++'s dialog can. A window of its own is
  possible later, with the inputs in it and the events sent to the workspace's window.
- Where it was put is not kept across sessions.
