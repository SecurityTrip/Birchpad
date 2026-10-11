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
  same way. Selections over mixed text are drawn from the leftmost to the rightmost of their
  ends.
- **Edit > Text Direction RTL** (and LTR) sets a view's paragraph direction: rows are reordered
  with a right-to-left base, and rows that fit end at the right edge of the view. It belongs to
  the view, as Notepad++'s does, and is not remembered.
- Rows too long to shape whole (over 2048 bytes) keep the stored order.

## Consequences

- A selection over a line that mixes directions covers one span, where Scintilla shows several
  pieces; moving the caret with the arrow keys follows the stored order, as Notepad++ does.
- The margins stay on the left in right-to-left mode.
