# ADR 0058: Parts of the window in crates of their own

- Status: accepted, changes [ADR 0041](0041-crate-layout.md)
- Date: 2026-10-29

## Context

ADR 0041 split the workspace into layers, but left every part of the window in the application
crate `birchpad`. Some of those parts need nothing of the workspace: the list to pick from by
typing (the command palette's and Go to File's), the search results panel, minimizing to the
system tray, and printing through Windows' own print dialog. In the application crate a change to
any of them rebuilt all of it, their tests ran only in a whole workspace window, and the
application carried the platform crates they alone call (the tray's Shell and AppKit calls,
GDI and the print dialog).

The same parts of the window were drawn again and again in the application: bars across a part
of the window, the rows of lists in the selection and hover colors, the text of an error, and a
command run from a button or a menu item with its error shown.

## Decision

The layers of ADR 0041 stay. Between the editor (layer 3) and the application (layer 4), the
parts of the window that need no workspace are crates of their own, each with its tests and only
the dependencies it uses:

- `birchpad-quick-pick`: the list to pick from by typing. It knows the active theme
  (`birchpad-editor`) and the fuzzy matching (`birchpad-core`), not what picking does: the
  palette and Go to File pass it their items and what picking one does.
- `birchpad-search-results`: the search results panel. It knows the buffers it points at
  (`birchpad-editor`) and the state it remembers (`birchpad-ui`); opening a result is an event
  the workspace handles.
- `birchpad-tray`: minimizing to the tray, on Windows and macOS. It needs no GPUI: it takes any
  window with a raw handle (`raw-window-handle`), and the application passes the setting in.
- `birchpad-print-dialog`: printing through Windows' print dialog and GDI; empty on other
  systems, and the application depends on it on Windows only.

The application re-exports them where the modules were (`birchpad::quick_pick`,
`birchpad::search_results`, `birchpad::tray`), so its own code and its tests keep their paths.

What several parts of the window draw alike is in the lowest layer that can hold it: the bars,
the rows of lists and the text of errors, which read the active theme, in `birchpad-editor`'s
`widgets` module (the messages above the text are its bars too); running a command chosen in
the window and showing its error is `Workspace::run`, as only the workspace dispatches commands.

New parts of the window that need no workspace go to a crate of their own in the same way, and
what they share with others to `widgets`.

## Consequences

- A change to one of these parts rebuilds it and the application, not the application's other
  parts; their tests run in a small window of their own (`cargo test -p birchpad-quick-pick`).
- The application no longer depends on the platform crates only the tray and printing call;
  the tray builds and is checked for every system without GPUI.
- Each crate has a version of its own (`birchpad-release crate-versions`) and a coverage floor.
- The tests that need the whole window (the palette's commands, Find All filling the results,
  the tray setting in Preferences) stay in the application's tests.
