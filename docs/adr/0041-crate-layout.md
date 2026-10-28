# ADR 0041: The crates' layout

- Status: accepted, changed by [ADR 0058](0058-parts-of-the-window-in-crates-of-their-own.md)
- Date: 2026-10-13

## Context

By the end of Phase 6's features, the application crate `birchpad` held more than half of the
workspace's code (about 48,000 of 83,000 lines): its window and dialogs, but also logic that
needs no window, the localization, and the editor view, which every change to a panel or a
dialog rebuilt and whose tests could only run in a whole workspace window. Crates without a
window spoke English and the application translated their sentences afterwards; the command
registry named the workspace and the editor; the editor read the application's globals (find
options, spelling, user defined languages, macro recording).

## Decision

The workspace is layered; a crate depends only on the layers under it.

1. **Without a window** (each testable on its own, no GPUI):
   - `birchpad-core`: the text model, motions, search, ops. Depends on nothing of Birchpad's.
   - `birchpad-i18n`: the locales, `t!`, plural forms, the language in use (ADR 0033).
   - `birchpad-io`, `birchpad-config`, `birchpad-commands`, `birchpad-view`,
     `birchpad-syntax`, `birchpad-theme`, `birchpad-tools`, `birchpad-plugin`,
     `birchpad-update`, `birchpad-cli`: files and encodings (Find in Files' reading of one
     file too), settings and sessions, the command catalog and keymaps (Run's command line
     too), layout in cells and right-to-left rows, languages, themes, tools (date formats,
     Export to HTML, the hex view's replacement), plugins, updates, the command line. They make
     their messages in the interface's language, with keys that start with their name.
2. **Shared by the window's parts:** `birchpad-ui`, the command registry (handlers registered
   for a `CommandTarget`, the view of a scope of the catalog), the keymap bound to GPUI,
   `AppState` (settings, user state, paths), dialog buttons and messages to the user.
   `birchpad-menu-bar` draws the menu bar.
3. **The editor:** `birchpad-editor`, the buffer, the editor view and the active theme, with
   its own tests in a window of one editor (`cargo test -p birchpad-editor`). It registers its
   commands for `EditorView` and asks the application for the rest through what it exposes:
   `EditorHooks` (macro recording, opening links, the right-click menu, user defined
   languages, spelling) and `ActiveFindOptions` (for smart highlighting). Without them it
   works alone.
4. **The application:** `birchpad`, the workspace, panes, panels, dialogs and menus; it
   installs the editor's hooks and registers the workspace's commands. The editor's tests that
   need the whole window (both views, files on disk, panels) stay here (`editor_tests`).

New code goes to the lowest layer it can live in: logic that needs no window to its kind's
crate, a part of the window the editor does not need to the application.

## Consequences

- A change to a dialog or a panel no longer rebuilds the editor, and the editor's tests run
  without the application's.
- Crates without a window can be fuzzed and measured on their own; each has its coverage
  floor (`.github/coverage-floors.toml`).
- The editor's API is wide (its view, layout and buffer are public to the application); the
  hooks are plain function pointers, so the application's implementations stay where they are.
- The tests' language is per thread (the `per-thread` feature of `birchpad-i18n`), and the
  messages tests read come from `birchpad-ui`'s `test-support` feature.
