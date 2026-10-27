# ADR 0054: Labels of non-printing characters in the column model

- Status: accepted
- Date: 2026-10-27

## Context

View > Show Symbol > Show Non-Printing Characters (roadmap 7.4a) frames spaces that are not
plain ones and marks characters without width, each in the cells it takes anyway: a no-break
space one cell, a zero-width space none. Notepad++'s Preferences > Editing 2 can draw them
instead by a label, their abbreviation (NBSP, ZWSP) or their code point (U+00A0), as wide as the
label. Built from the user manual's description of the setting, not from Notepad++'s code.

Birchpad lays text out on a monospace grid of cells in `birchpad-view` ([ADR 0041](0041-crate-layout.md)):
columns, word wrap, the caret and clicks all count a character's cells with one function. A
label several cells wide drawn over a character of one cell (or none) would put everything after
it out of step with the columns the caret and wrap compute.

## Decision

- The grid is a `Grid`: the tab width and the labels (`NonPrintingLabel`: off, abbreviation,
  code point). Every column function of `birchpad-view` (`cells_at`, `column_after`,
  `pos_at_column`, `DisplayText`, wrapping) takes it; a tab width alone converts to a grid
  without labels, so callers that never label keep passing the width. `LayoutConfig` carries
  the labels, and a change of them lays the document out again, as a change of the tab width
  does.
- While labels are on, a character that Show Non-Printing Characters shows takes the cells of
  its label, which `DisplayText` draws in its place. The label is all ASCII, so its length is
  its cells. Characters without an abbreviation (U+2061 to U+2064, U+1680) show their code
  point.
- The editor turns labels on only while Show Non-Printing Characters is on and
  `editor.non-printing-style` is `abbreviation` or `code-point`; with `marks`, the default,
  nothing changes from before. The label gets a frame, the color of the text around it.
- Fast paths that took a line no longer in bytes than the wrap width to fit in one row hold
  only without labels (`Grid::cells_within_bytes`): a two-byte no-break space labeled NBSP
  takes four cells.
- `DisplayText` records every character it replaces, even by text of the same length (ZWJ by
  "ZWJ"), so that an offset inside a label maps to an end of the character.

## Consequences

- Turning labels on or off, or Show Non-Printing Characters with labels chosen, lays the
  document out again, on a background thread for large wrapped documents as any layout change.
- Column positions (Go to's column, column selection, the status bar's column) count labels
  while they show, as they count tabs by their stops.
- The document map draws the text without labels: it shows the text's shape, not its symbols.
