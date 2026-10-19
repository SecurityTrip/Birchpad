# ADR 0035: Right-to-left text

- Status: accepted
- Date: 2026-10-12

## Context

GPUI shapes text in the order it is stored: Hebrew and Arabic showed reversed. Notepad++ shows
them through Scintilla's bidirectional support and has Edit > Text Direction RTL and LTR, which
make the whole editor run right to left.

## Decision

- **Every row with right-to-left characters is reordered** for display with the Unicode
  Bidirectional Algorithm (`unicode-bidi`): the row is shaped in the order it shows, each
  character in the color of where it is stored. Rows without such characters, in left-to-right
  mode, are shaped as before at no cost.
- **Logical and visual positions map both ways** per row: a caret before a left-to-right
  character shows at its left, before a right-to-left one at its right; a click maps back the
  same way. A selection over mixed text is drawn piece by piece, one for each run of
  selected characters that show side by side (`BidiRow::visual_spans`), as Scintilla draws it;
  a selected line break shows on the side the paragraph runs to.
- **Edit > Text Direction RTL** (and LTR; Ctrl+Alt+Shift+R and Ctrl+Alt+Shift+L, since
  Ctrl+Alt+R is Random Case) sets a view's paragraph direction: rows are reordered with a
  right-to-left base, rows that fit end at the right edge of the view, and the margins move to
  the right of the text, mirrored as Scintilla mirrors them: line numbers outermost, folding
  next to the text. It belongs to the view, as Notepad++'s does, and sessions keep it per tab
  (`rtl`, Notepad++'s `RTL="yes"` read from its sessions).
- Rows too long to shape whole (over 2048 bytes) are shaped around what shows, and that part
  is reordered on its own.

## Consequences

- Moving the caret with the arrow keys follows the stored order, as Notepad++ does.
- In a very long row, a run of right-to-left text that crosses the edge of the shaped part is
  reordered in two pieces.
