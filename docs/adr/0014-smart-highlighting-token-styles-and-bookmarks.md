# ADR 0014: Smart highlighting, token styles and bookmarks

- Status: accepted
- Date: 2026-10-02

## Context

Notepad++ highlights the other occurrences of a selected word (smart highlighting), lets the
user mark tokens with five styles (Search > Style All Occurrences of Token and the menus
below it) and has a Bookmark menu with operations on bookmarked lines. ADR 0010 gave us the
data: `RangeSet` and `LineMarkers`, with the bookmarks and the five styles stored in the
buffer (`DocumentMarks`). This ADR decides how the features behave and where their work
happens.

## Decision

- **Smart highlighting belongs to the view.** Its result depends on the view's selection and
  visible text, so each view keeps its own `RangeSet<()>`, cached by the document revision,
  the query and the ranges searched.
  - It triggers on the primary selection when it is exactly one word (a double click), or,
    with `highlighting.smart.whole-word = false`, on any selection within one line
    (`birchpad_core::search::smart_highlight_token`). Settings, as in Notepad++'s
    Preferences > Highlighting: `highlighting.smart.enabled`, `match-case`, `whole-word`,
    `use-find-options` (take match case and whole word from the find panel). It is off for
    files over `files.large-file-limit-mb`.
  - Only the text shown in the frame is searched (the same merged ranges syntax highlighting
    queries; a long line contributes only its visible columns), extended by 16 KB on each
    side so that short scrolls reuse the result. Selections over 64 KB are not highlighted.
  - The search runs on the UI thread with a budget: `search::Scan` searches a range in steps
    of 64 KB and stops when the caller says so; the view gives it 2 ms per frame and asks for
    another frame while it is not done. A background task would need a copy of the rope and
    a round trip per selection change for work that normally takes well under a millisecond.
  - Drawn as Notepad++'s translucent green fill, under the token styles and the selection.
- **Token styles** (document decorations, ADR 0010).
  - The token is the selection, or the word at the caret when nothing is selected
    (`search::token_at`). Style All Occurrences of Token finds every occurrence in the
    document with `highlighting.token-style.match-case` and `whole-word`: Notepad++ has
    separate options for it in Preferences > Highlighting, with the same defaults as smart
    highlighting (whole words, ignoring case). Style One Token styles only the token itself.
    Many matches are added in one pass (`RangeSet::insert_all`).
  - Jump Down goes to the first occurrence that starts after the start of the selection,
    Jump Up to the last one that ends before it, so the occurrence at the caret is skipped,
    as in Notepad++. Both wrap around, select the occurrence and expand folds hiding it.
    Notepad++'s "Find Style" entry jumps to the marks of the Mark dialog (phase 3); we offer
    "Any Style" there instead, over all five styles.
  - Copy Styled Text copies the occurrences of one style, or of all, in document order, each
    followed by the document's line break; a range styled twice is copied once.
- **Bookmarks.** Toggle Bookmark works on the lines of all carets: if they are all
  bookmarked, it removes their bookmarks, otherwise it bookmarks them all. Next and Previous
  Bookmark move to the start of the line, wrap around and expand folds. The line operations
  are pure functions in `birchpad_core::ops::marks`, each one transaction:
  - Copy Bookmarked Lines copies each line followed by the document's line break; Cut also
    removes them.
  - Remove Bookmarked Lines deletes the lines with their line breaks; a block that reaches
    the last line takes the line break before it, like Delete Current Line. The bookmarks go
    with the lines, as in Notepad++ (they do not land on the next line).
  - Remove Unmarked Lines deletes the others; with no bookmarks at all that is every line,
    as in Notepad++, and Undo brings it back.
  - Paste to (Replace) Bookmarked Lines replaces each line's content (not its line break) with
    the clipboard text, verbatim; the bookmark stays on the first line of each pasted block.
  - Inverse Bookmark bookmarks exactly the lines that had none.
- **Not part of undo.** Styles and bookmarks follow edits, undo and redo, but are not
  recorded in the history, as in Scintilla: text deleted together with its style or bookmark
  comes back plain when the deletion is undone.
- **Mark dialog hook.** `DocumentMarks::bookmark_lines_of` bookmarks the lines of a list of
  matches: the "Bookmark line" option of the Mark dialog (phase 3) will call it with the
  results of Mark All.

## Keys

| | Windows, Linux | macOS |
|---|---|---|
| Style All Occurrences, style N | Ctrl+Alt+N | Cmd+Alt+N |
| Clear style N / all styles | Ctrl+Alt+Shift+N / Ctrl+Alt+Shift+0 | Cmd+Alt+Shift+N / Cmd+Alt+Shift+0 |
| Jump Down, style N | Ctrl+N | Cmd+N |
| Jump Up, style N | Ctrl+Shift+N | Cmd+Ctrl+N (Cmd+Shift+3/4/5 take screenshots) |
| Toggle Bookmark | Ctrl+F2 | Cmd+F2 (Ctrl+F2 focuses the menu bar) |
| Next / Previous Bookmark | F2 / Shift+F2 | F2 / Shift+F2 |

Style One Token, Jump to Any Style, Copy Styled Text and the bookmarked-line operations have
no keys, as in Notepad++. Notepad++'s Ctrl+0 / Ctrl+Shift+0 for "Find Style" are not bound:
Ctrl+0 stays Restore Default Zoom.

X11 and Wayland report Shift with a digit or punctuation key as the character it types,
without Shift (Ctrl+Alt+Shift+1 arrives as `ctrl-alt-!`), and GPUI has no keyboard mapper on
Linux, so such bindings never matched there; this also affected Alt+Shift+0..8 for folding
(ADR 0012). On Linux every such binding now gets a second binding spelled the way a US layout
reports it (`Keystroke::shifted_symbol`). Layouts that are not Latin report the US character
too; European layouts that type other characters with Shift and a digit (a German Shift+7 is
`/`) still need a binding of their own in `keymap.toml`.

## Measurements (release, Linux x64)

A 10 000-line Rust-like file, a word selected that occurs six times per line, the search
range being the visible rows (3.5 KB) plus the margins:

| | Time |
|---|---|
| Search after the selection or the text changed | 0.54 ms |
| Frame reusing the cached result | under 1 µs for smart highlighting |
| Whole frame, smart highlighting on / off | 1.33 ms / 1.15 ms |

The search is case-insensitive by default, which compares characters one by one; it is the
slower case.

## Consequences

- Highlight another view (Notepad++'s fifth smart highlighting option) waits for split view.
- The Mark dialog with its own style, "Bookmark line" and "Find Style" jumps comes with the
  Find dialog in phase 3.
- Bookmarks and styles are not saved; sessions (stage 9) will keep bookmarks.
