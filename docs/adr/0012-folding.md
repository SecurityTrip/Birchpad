# ADR 0012: Folding

- Status: accepted
- Date: 2026-10-02

## Decision

- **Fold points come from the syntax tree** without per-language fold queries: every named
  node spanning several lines can fold, and when several start on one line (a function and its
  body block) the outermost one is the fold. Bodies that begin with their first statement or
  entry rather than a token of their own (Python and Lua `block`, Ruby `body_statement`, YAML
  `block_node`/`block_mapping`/`block_sequence`) are skipped, so a body's first line never
  folds its siblings; the statements in it fold on their own. Plain text up to 1 MB folds by
  indentation (a line folds the more indented lines after it). Files over the large file limit
  do not fold.
- **Points belong to the buffer, collapsed state to the view**, as in Scintilla: a clone in the
  other pane can show the same document unfolded. Points are byte ranges computed with the
  syntax tree on the background parse thread and mapped through every edit
  (`birchpad_core::map_ranges`) until the next parse replaces them. Collapsed headers are line
  markers (`LineMarkers`), so a collapsed fold moves with its text; after a reparse, headers
  that no longer fold are forgotten.
- **Hidden lines live in the display map.** Collapsed folds give sorted, disjoint hidden line
  ranges. Without word wrap, rows map to lines through prefix sums over the hidden ranges
  (O(log n)); with it, hidden lines count zero rows in the existing prefix sums. Vertical
  movement, paging and scrolling skip hidden lines on their own.
- **Carets never stay hidden.** Folding moves carets inside the collapsed lines to the end of
  the fold's header. Left/Right steps over a collapsed fold. Anything else that puts a caret in
  hidden lines (edits, Go To, search results) expands the folds hiding it.
- **Commands as in Notepad++:** Fold All (Alt+0), Unfold All (Alt+Shift+0), Collapse /
  Uncollapse Level 1–8 (Alt+N, Alt+Shift+N), Collapse / Uncollapse Current Level (Ctrl+Alt+F,
  Ctrl+Alt+Shift+F), and a click on a fold box in the folding margin. The margin draws boxes
  with a minus or a plus, lines along expanded folds, and a line under collapsed headers.
- **Cost per keystroke.** Converting every fold of a 400 000-line file to lines takes about
  15 ms, too much per keystroke. Views shift their folds in lines instead (`shift_folds`): folds
  before the edited lines stay, folds after them move by the change in line count, folds around
  them grow or shrink; only an edit on a fold's first or last line recomputes them.

## Key bindings and keyboard layouts

Windows reports Shift with a digit or punctuation key as the character it types on the
current layout (Alt+Shift+0 arrives as `alt-)` on a US or Russian layout), so a binding
written `alt-shift-0` never matched. Key bindings are now built with the platform's keyboard
mapper, which turns them into what the current layout produces. Shifted digits differ between
layouts (Shift+2 is `@` on a US layout and `"` on a Russian one), so the bindings are rebuilt
when the layout changes: Birchpad's own `RunCommand` bindings are replaced where they were, and
gpui-component's bindings stay as they are.

## Consequences

- Sessions keep each view's collapsed folds ([ADR 0017](0017-sessions-and-backup.md)).
- Consecutive line comments do not fold (they are separate nodes); Notepad++ does not fold
  them either.
