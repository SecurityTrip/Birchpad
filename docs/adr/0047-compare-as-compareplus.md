# ADR 0047: Compare as ComparePlus

- Status: accepted
- Date: 2026-10-21
- Changes [ADR 0039](0039-built-in-tools.md): Compare aligns the documents, marks within lines
  and compares with sources other than an open document.

## Context

ADR 0039 built Compare as Notepad++'s first Compare plugin did: two documents side by side, the
changed lines marked. ComparePlus, which users of Notepad++ 8 have instead, keeps the lines
both documents have on the same row, marks what differs within a changed line, compares with
the last save, the clipboard or Git, and shows where the differences are in the whole document.
Built from ComparePlus's documentation and its behavior, not its code (CONTRIBUTING.md).

## Decision

- **Blank rows in the display map.** Alignment is a property of the view, not of the text:
  `DisplayMap` takes padding, blank rows above lines (or below the last), and counts them in its
  row conversions, with and without word wrap; lines hidden by folds have none. A padding row
  shows no text and maps to the start of the line below it; vertical movement passes over
  padding, so the caret never stops in a row it could not type in. Edits move the padding with
  the line breaks they add or remove. Compare gives each side, for each difference, as many rows
  as the other side has more lines there.
- **Marks within lines.** The lines of a change are paired in order and compared character by
  character (as the line comparison, Myers with a deadline, spaces and case ignored as asked);
  what differs is a mark of the document (`DocumentMarks::compare_inline`) drawn over the line's
  color. Lines with nothing in common, or longer than 10 000 characters, are marked whole.
- **Other sources.** Diff since Last Save reads the file as saved, Compare to Clipboard takes the
  clipboard's text, Diff to Git HEAD runs `git show HEAD:./<name>` in the file's folder (Git as
  installed; Birchpad does not link a Git library). The text opens in the other view as a
  read-only document, which Clear Results closes.
- **The navigation bar** is the vertical scrollbar: each difference is marked along its track
  at its rows' place among all rows, and a click on the track goes there, as dragging does.
- **Compared again after edits**, 300 ms after the last one (`compare.recompare`); a document
  closed ends the comparison.
- **Preferences > Compare**: `compare.align`, `compare.inline`, `compare.navigation-bar`,
  `compare.recompare`, all on by default. The theme has two more colors, `compare-inline` and
  `compare-padding`.

## Consequences

- Any feature can show blank rows between lines through the display map; only Compare does so
  far.
- With word wrap the two sides can wrap a line into different numbers of rows: the lines both
  have stay in step only where they wrap alike, as in ComparePlus.
- Comparing with Git needs Git on the `PATH`; without it, the command says so.
