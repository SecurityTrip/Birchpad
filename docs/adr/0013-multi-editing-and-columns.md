# ADR 0013: Multi-editing, rectangular selections and columns

- Status: accepted
- Date: 2026-10-02

## Context

Notepad++ edits in several places at once in two ways: a rectangular (column) selection,
made with Alt+drag or Alt+Shift+arrows, which reaches past the end of short lines into
*virtual space*; and multiple selections, made with Ctrl+click or Multi-select Next/All.
The Column Editor inserts text or a sequence of numbers into every row. All of this comes
from Scintilla, which places a rectangle by x coordinates.

Phase 1 left one question open: Birchpad's view and status bar count columns in cells (a
wide character takes two, a tab runs to the next stop, [ADR 0008](0008-editor-view-layout.md)),
while Notepad++'s status bar counts characters. Column editing needed an answer.

## Decision

- **Columns are cells everywhere.** Rectangles, the Column Editor, Alt+Shift+arrows and the
  status bar's `Col` all count cells. With a monospace font, cells are what Scintilla's x
  coordinates amount to, so a rectangle's edges stay straight across tabs and East Asian
  text, and "column" means one thing in the whole editor. The only visible difference from
  Notepad++ is that `Col` counts a wide character as two. A rectangle edge that falls inside
  a tab or a wide character rounds to the nearer side of it, as a click there does.
- **Virtual space in `Range`.** `birchpad_core::Range` gets `anchor_virtual` and
  `head_virtual`: cells past the end of the line, non-zero only for a position at a line
  end. The byte fields and every old invariant stay as they were: ranges sorted, starting at
  strictly increasing bytes, never overlapping, the primary range containing the one asked
  for. Ranges order by (byte, virtual cells); virtual space counts when deciding whether two
  ranges overlap. Text inserted at a virtual position uses up its virtual space; after each
  edit views drop virtual space that is no longer at a line end (`Selection::clip_virtual`).
  Typing into virtual space first fills it with spaces (never tabs, as Scintilla); deleting
  leaves the caret where it was, in virtual space. Property tests cover both the old
  invariants with virtual space and typing against padding done by hand.
- **Virtual space only in rectangles**, as in Notepad++'s defaults (its "Enable virtual
  space" option is not offered yet). Every other way of moving carets builds ranges without
  it.
- **A rectangle is a `birchpad_view::Block`**: an anchor corner and a caret corner, each a
  line and a column in cells. It becomes one range per line (backward when the caret corner
  is on the left), the caret's line primary. The view keeps the block together with the
  selection it made and treats it as active only while its selection is still that one, so
  anything else that changes the selection leaves column mode without having to say so.
  After typing or deleting, the rectangle becomes a caret on each line at the primary caret's
  column; the carets stay exactly where the edit left them, which matters next to tabs.
- **Folding.** A rectangle leaves out lines hidden by collapsed folds: it never edits text
  that cannot be seen, and the fold stays collapsed. Multi-select matches inside hidden lines
  expand the folds hiding them, as other caret movements do ([ADR 0012](0012-folding.md)).
- **Keys as in Notepad++.** Alt+drag; Alt+Shift with the arrows, Home, End, Page Up and Page
  Down (`select.block-*`). On macOS Option+Shift+Left/Right select by word, so the rectangle
  keys are Cmd+Option+Shift there. Esc does what Scintilla's `SCI_CANCEL` does: a rectangle
  becomes a caret at its caret corner, several selections become the primary one.
  Begin/End Select is Ctrl+Shift+B (Cmd+Shift+B), in column mode Alt+Shift+B
  (Cmd+Option+Shift+B), the Column Editor Alt+C (Cmd+Option+C): Option+letter types
  characters on macOS. Notepad++ has no default keys for the Multi-select commands, and
  neither has Birchpad; users bind them in `keymap.toml`.
- **Copying a rectangle** puts its rows on the clipboard, each followed by the document's
  line break, as Scintilla does, with GPUI clipboard metadata
  `{"rectangular": true, "rows": N}`. Other programs get plain text. Pasting checks that the
  text still has N rows (another program may have replaced the text but not the metadata),
  deletes the selection, and puts each row on the next line at the caret's column, filling
  short lines with spaces and adding lines at the end of the document when the rows run
  past it; deletion and insertion are composed into one transaction. GPUI keeps metadata
  next to a hash of the text on Windows and macOS; on Linux only while Birchpad owns the
  clipboard. Notepad++'s own `MSDEVColumnSelect` format cannot be written through GPUI, so a
  rectangle copied in Notepad++ pastes into Birchpad as text, and the other way round.
- **Multi-select Next, All, Undo the Latest Added and Skip Current** are pure functions in
  `birchpad_core::ops` (`select_next`, `select_all`, `skip_to_next`); match case and whole
  word are arguments of the commands, so the four variants of each Notepad++ menu item are
  one command each. With a caret, Next first selects the word at it. The view remembers in
  which order ranges were added, for Undo the Latest Added.
- **Input methods** compose at the primary caret only; the other carets follow the edit.
  Text committed without composition goes to every caret like typing.
- **Column Editor.** Number generation is `birchpad_core::ops::NumberSequence`: initial
  value, step, repeat, leading none/zeros/spaces, base dec/hex/oct/bin, hexadecimal in upper
  or lower case. Every number is padded to the width of the widest, sign included: with no
  leading characters it is left-aligned with spaces after it so that what follows stays
  aligned, zeros go after a minus sign. The number fields are read in the chosen base, as
  in Notepad++. Targets, top to bottom: the rows of the rectangle; otherwise every caret or
  selection; otherwise, with one caret, every line from the caret's to the last, at the
  caret's column. The dialog runs `edit.column-insert` with its fields as arguments, so the
  insertion is one undo step and can be replayed by macros.
- **Sorting with a rectangle** reorders whole lines by the text inside its columns
  (`sort_lines_by_columns`), stably; with a zero-width rectangle the key runs from the caret
  to the end of the line, as Notepad++ documents. The rectangle stays over the same lines
  and columns afterwards.

## Consequences

- Typing with 1000 carets, measured in release builds on Linux: 2.3 ms per keystroke
  including a frame's layout in GPUI's test harness (`typing_with_a_thousand_carets`, an
  ignored test that prints the timing); in the application under Xvfb with software
  rendering, ten keystrokes into a 1000-line rectangle took about 10 ms together, and a
  frame's layout 0.1–0.3 ms.
- Ctrl+click on an existing caret or selection does not remove it yet, and the primary
  selection is not drawn in a different color from the others, as Notepad++ does.
- A rectangle is mapped to lines and columns, not to rows: with word wrap, Alt+Shift+Up and
  Down move by lines, and a rectangle's columns count from the start of the line.
- Notepad++'s "Enable Column Selection to Multi-Editing" is how Birchpad always behaves:
  arrows on a rectangle move its carets like any multi-caret selection.
