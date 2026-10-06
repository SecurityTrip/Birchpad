# ADR 0010: Decorations, markers and margins

- Status: accepted
- Date: 2026-10-02

## Context

Phase 2 adds many things drawn over or beside the text: bookmarks, the five token styles of
Search > Style All Occurrences of Token, smart highlighting, search matches, the matching
brace, syntax colors, fold markers. Scintilla models them as *markers* (per line) and
*indicators* (per range). They have to follow edits, undo and redo, in every view of a
document, without costing more than the visible part of the text per frame.

## Decision

- **UI-independent data in `birchpad-core`:**
  - `RangeSet<T>`: non-overlapping, non-empty ranges sorted by position, each with a value.
    Setting a value over a range replaces what was there, like filling a Scintilla indicator.
    Through an edit, text inserted inside a range extends it; text inserted at either edge does
    not; a range whose text is deleted disappears.
  - `LineMarkers`: at most one marker per line, stored as a position on the line, mapped with
    `Assoc::After`. Typing or pressing Enter at the start of a marked line takes the marker
    along with the text; deleting a marked line leaves the marker on the line that takes its
    place, as Scintilla merges markers of deleted lines.
  - `ChangeSet::mapper` maps positions in non-decreasing order in amortized O(1), so mapping
    every decoration through a Replace All with thousands of edits stays linear.
  - Property tests compare both structures with mapping each endpoint through `map_pos`.
- **Who owns what.** Decorations that belong to the document (bookmarks, token styles) live
  in the `Buffer` (`DocumentMarks`) and are shown by every view of it, as in Notepad++ where
  they live in the Scintilla document. Decorations that depend on a view (smart highlighting
  of its selection, the brace at its caret) are computed per view for its visible range.
  Syntax colors are not stored as ranges at all: they are queried from the syntax tree for the
  visible rows of each frame (stage 2).
- **Appearance is the application's.** A decoration is drawn as a translucent fill, an
  outline or an underline (`Paint`), in colors from Notepad++'s default style. Fills are drawn
  under the selection, and the selection under the text.
- **Margins** left of the text, in Notepad++'s order: line numbers, symbols (bookmarks),
  folding. Each can be turned off (`editor.line-numbers`, `editor.bookmark-margin`,
  `editor.fold-margin`). A click in the symbol margin toggles the bookmark of that line; a
  click or drag in the line number margin selects whole lines. The change history margin
  joined later, between symbols and folding ([ADR 0023](0023-change-history.md)).

## Consequences

- Every document-level decoration is mapped on every edit: O(n) in the number of decorations,
  which is fine for thousands of marks. A document with hundreds of thousands of marks would
  need a tree of offsets instead of a sorted vector; that can replace `RangeSet`'s inside
  without changing its interface.
- Sessions keep bookmarks ([ADR 0017](0017-sessions-and-backup.md)); token styles are not saved.
