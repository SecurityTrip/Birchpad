# ADR 0052: Command palette and fuzzy matching

- Status: accepted
- Date: 2026-10-24

## Context

Birchpad has several hundred commands, most of them in menus several levels deep; Notepad++
has no way to find one by its name, and the Shortcut Mapper's filter only finds words as
typed. Editors beyond Notepad++ (Sublime Text, Visual Studio Code, JetBrains' Find Action)
offer a palette: a field, and the commands whose names contain the letters typed, in order.
Built from how these behave for a user, not from their code. Go to File (roadmap 7.2) needs
the same list and the same matching for files.

## Decision

- **Fuzzy matching is text logic** and lives in `birchpad-core` (`fuzzy`): what is typed is
  split at spaces into terms, each matches on its own; a term matches when its characters
  appear in order, without regard to case. The best alignment of a term is found by dynamic
  programming over the text: each character matched is worth points, more at the start of a
  word or at a capital inside one and right after the character before it, less for each one
  skipped. Matches are ranked by score, then shorter texts, then the order given; with
  nothing typed the order given is kept. Texts so long that the alignment would cost too
  much take the first characters that match.
- **One list to pick from** (`quick_pick`), a dialog with a field above the items: Up, Down,
  Page Up and Page Down choose, Enter or a double-click picks, the characters matched are
  shown in bold. The recently used items come first while they match, in the order they were
  used (`fuzzy::rank_recent`); the others follow by score.
- **What the palette lists** (`view.command-palette`, Ctrl+Shift+A): the commands of the
  catalog whose handler runs without arguments (the registry notes, when a handler is
  registered, whether its arguments decode from none), the menu items that run a command with
  arguments, named after their submenu (Language: Python), the saved macros and command
  lines, and the plugins' and scripts' commands; each once, with the shortcut the menus would
  show. Commands turned off or not implemented yet are left out, as the menus disable them;
  the palette does not list itself.
- **Recently used** commands are those run from the palette, kept in `state.toml`
  (`recent-commands`, each an invocation as JSON), as many as Preferences > Command Palette
  says (`palette.recent-commands`, 8 by default, 50 at most; 0 keeps none).
- Ctrl+Shift+P, the palette of other editors, stays Notepad++'s macro playback; Ctrl+Shift+A
  is free in Notepad++'s keymap and is JetBrains' Find Action.

## Consequences

- A command added to the catalog or a menu shows in the palette without more work.
- Commands that need arguments (`edit.insert-text`) are found only through a menu item or a
  binding that gives them.
