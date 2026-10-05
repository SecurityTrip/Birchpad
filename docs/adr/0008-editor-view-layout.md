# ADR 0008: Editor view layout in cells

- Status: accepted
- Date: 2026-10-01

## Context

The Phase 0 spike rendered one GPUI `uniform_list` item per line and shaped each line whole. That
cannot do horizontal scrolling, word wrap or tab stops, and a 10 MB line would be shaped in full
every frame. Phase 2 adds folding, which must fit the same structure.

## Decision

- **A grid of cells.** Like Scintilla with a monospace font and like terminal-based editors,
  layout works in cells: a character takes 1 cell, East Asian wide characters and emoji 2,
  combining marks 0 (`unicode-width`); a tab advances to the next multiple of
  `editor.tab-width` counted from the line start; control characters are drawn as one-cell
  control pictures (␀, ␛). The UI-independent crate `birchpad-view` computes all of it and is
  tested without a window, including property tests.
- **Display map.** Rows are lines without word wrap. With word wrap, a line breaks into rows
  after whitespace or around wide characters (whitespace hangs at the end of a row, as in
  Scintilla); the map keeps a row count per line and prefix sums, so row ↔ line lookups are
  O(log n) and an edit only relays out the lines it touched. Folding will give folded lines
  zero rows. Documents over 4 MB are rewrapped on a background thread when the width changes;
  the previous layout stays on screen meanwhile.
- **Pixels from shaping, positions from cells.** Within a visible row, x positions come from
  GPUI's shaped text (exact for proportional fallback fonts such as CJK). Row breaks, vertical
  movement (the goal column), horizontal scrolling and long lines use cells. The display text
  of a row has tabs expanded and control characters replaced; a mapping converts between
  document and display offsets.
- **Long lines.** A row longer than 2 KB is shaped only for the columns in view (plus a
  margin), positioned by cells. Lines longer than 8 KB get a column index (a checkpoint every
  4 KB), so finding the column of a position deep in the line does not scan it from the start.
- **One custom element per view.** `EditorElement` lays out and paints the gutter, visible rows,
  selections, carets and scrollbars itself, replacing `uniform_list`. The view keeps the last
  frame's layout for mouse and IME hit testing.

## Consequences

- Measured in a release build (Linux, software rendering): a frame's layout takes 0.2–0.4 ms
  for a 100 MB file; a 10 MB single-line file opens in 0.09 s and scrolls to its end instantly.
- Long lines keep their layout through edits. An edit inside a long line rescans its column index
  from the checkpoint before the edit to the one after, then shifts later checkpoints, which a
  shift by a whole tab stop leaves valid (a tab after the edit makes the shift whole). With word
  wrap the line is rewrapped from the row before the edit until a row starts where one started
  before, at a column with the same tab stops; later rows are only moved. Starting at any row
  start gives the same rows as wrapping the line from its beginning, which a property test
  checks against fresh layouts, with small thresholds so that short texts have long lines.
  Typing into the middle of a 10 MB line, with a frame drawn (release build, Windows): 1.6 ms per
  keystroke, 6.7 ms with word wrap, down from 49 ms and 168 ms.
- Characters whose shaped width differs from their cell width (some fallback fonts) can make
  wrapped rows slightly narrower or wider than the view and vertical movement slightly off
  from the pixel column; Scintilla works in pixels instead. Acceptable for a monospace editor.
- Proportional fonts are not supported by this design; they are not a goal for the editor
  view.
