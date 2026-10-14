# ADR 0040: The tab bar

- Status: accepted
- Date: 2026-10-12

## Context

Notepad++'s tab bar does more than switch documents: tabs can be pinned and colored, it can
stand vertically or wrap onto several rows, be reduced or locked against dragging, close a tab
on a double click, show a preview of a document on hover, and its context menu acts on the tab
under the mouse. Ctrl+Tab opens the document switcher, a list of the view's documents ordered
by recent use, which Notepad++ keeps open while Ctrl is held. View > Tab goes to the first nine
tabs and moves the active one; File > Close Multiple Documents closes groups of tabs.

Until now a pane drew gpui-component's `TabBar`, which has one scrolling row and no hooks for
colors, wrapping or a column.

## Decision

- **Birchpad draws its own tab bar** (`pane.rs`): one row that scrolls, several rows
  (`tabs.multi-line`) or a column on the left of the view (`tabs.vertical`), with smaller tabs
  for `tabs.reduce`. Each tab keeps the debug selectors the tests use (`pane-N-tab-M`,
  `pane-N-close-tab-M`). The empty part of the bar takes dropped tabs and opens a new document
  on a double click, as in Notepad++.
- **A tab carries its marks.** Pinned and the color (1 to 5) belong to the tab, not the
  document: they live in the `EditorView` (`TabMarks`), follow it to the other view, and are
  saved per tab in sessions (`pinned`, `color` in `[[main-view.tabs]]`); Notepad++'s
  `tabPinned` and `tabColourId` are read from its `session.xml`. A clone starts without them.
- **Pinned tabs come first.** Every insertion, drop and move keeps the pinned tabs before the
  others: a pinned tab cannot be dropped among the others or the reverse, and Move Tab
  Forward/Backward/to Start/to End stay within the group. Pinning moves a tab to the end of the
  pinned ones, unpinning to the start of the others. A pinned tab shows a pin, which unpins it,
  instead of the close button.
- **Tab colors are theme colors.** `tab-color-1` to `tab-color-5` in a theme's `[ui]`; a
  Notepad++ theme's `Tab color N` global styles become them, its `Tab color dark mode N` for a
  dark theme.
- **The context menu runs commands.** Its items are the catalog's commands (close, close
  groups, save, print, pin, colors, move or clone to the other view); choosing one activates
  the tab, then dispatches the command, so it behaves as the menu item and the shortcut do.
- **The switcher previews.** Each Ctrl+Tab shows the next document of the list at once (the
  list is fixed when it opens), and releasing Ctrl keeps it and makes it the most recent. The
  order is the view's tabs by recent use (`tabs.switcher-mru`) or in tab order; with
  `tabs.switcher` off, Ctrl+Tab is Next Tab. The switcher stays within the active view, as
  Ctrl+Tab did before. Ctrl+PageDown and Ctrl+PageUp stay Next and Previous Tab.
- **The document peeker is a tooltip** over a tab that is not active: up to twelve lines of
  its text, from the first line its view showed, tabs as spaces, cut at 72 characters
  (`tabs.peeker`). Notepad++ draws a scaled-down picture of the text; lines of text are
  readable and need no second renderer.
- **Settings.** The tab bar's options are a `[tabs]` section of `settings.toml`, in Preferences
  > Tab Bar: `vertical`, `multi-line`, `reduce`, `lock`, `close-button`,
  `double-click-close`, `peeker`, `switcher`, `switcher-mru`. Defaults follow Notepad++.

## Consequences

- The tab bar's look is Birchpad's to keep in line with the theme; it no longer follows
  gpui-component's tab styles.
- Ctrl+Tab no longer goes to the next tab by default but to the most recently used one;
  `tabs.switcher = false` brings the old behavior back.
- Session files gain two optional keys per tab; older versions ignore them.
