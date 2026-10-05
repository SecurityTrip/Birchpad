# ADR 0022: Search modes and the find panel

- Status: accepted
- Date: 2026-10-06

## Context

Phase 1 had a simple find panel: literal text, Find Next and Previous, Replace and Replace
All. Notepad++'s Find dialog has three search modes (Normal, Extended, Regular expression),
options (match case, whole word, wrap around, backward, in selection, `.` matches newline), and
tabs: Find, Replace, Find in Files and Mark. Phase 3 brings Birchpad to that.

## Decision

### The search engine (`birchpad_core::search`)

- **Normal** stays as it was: literal text matched on the rope's chunks, case folding, whole
  words.
- **Extended** turns Notepad++'s escapes into literal text before searching: `\n`, `\r`, `\t`,
  `\0`, `\\`, and characters by code (`\xHH`, `\uHHHH`, `\oNNN`, `\dNNN`, `\bNNNNNNNN`). The
  replacement reads the same escapes.
- **Regular expressions** use `fancy-regex`: Perl syntax with back-references and look-around,
  which the `regex` crate lacks; it hands the parts without them to `regex`, so plain patterns
  stay linear. As in Notepad++, `^` and `$` match at every line, with any line ending (CRLF
  mode), and `.` matches line breaks only with the option. Matching runs on one copy of the text
  per operation (a few milliseconds for 10 MB): look-behind and anchors then see the whole
  text, also when only a selection is searched. A backtracking limit stops runaway patterns,
  and the error is reported, not taken for "no match".
- **Empty matches** work as in Notepad++: replacing `^` with `// ` comments out every line,
  and Find Next steps over an empty match at the caret instead of finding it forever.
- **Replacements** of regular expressions expand `$1`, `\1`, `${name}`, `$&` and `$$`, and
  change case with `\U`, `\L`, `\E`, `\u` and `\l` (Boost's syntax, which Notepad++ uses).
  Replace All builds all its edits in one pass and applies them as one undo step.

### The find panel

- **A panel, not a dialog.** It stays at the bottom of the window, above the status bar, with
  the tabs Find, Replace and Mark (Find in Files joins with the search results panel). Notepad++'s
  dialog is modeless and floats over the text; a docked panel never covers it, and GPUI dialogs
  are modal.
- **Find:** Find Next and Previous (F3, Shift+F3), Count. **Replace:** Replace, Replace All,
  Replace All in All Opened Documents (one undo step per document; read-only documents are left
  and counted). **Mark:** Mark All, with Bookmark line and Purge for each search; Clear All
  Marks; Copy Marked Text (one marked text per line, like Copy Styled Text).
- **Options:** match whole word (not for regular expressions, as in Notepad++), match case,
  wrap around, backward, in selection (Count, Replace All and Mark All), the search mode, and
  `.` matches newline. Opening the panel with a selection on one line puts it in Find what;
  with a selection over several lines, In selection is turned on instead.
- **Marks** are a layer of the document's decorations, like token styles: every view of the
  document shows them, edits move them, and they are drawn in Notepad++'s "Find Mark Style",
  a translucent red.
- **Select and Find Next / Previous** (Ctrl+F3, Ctrl+Shift+F3) search for the selection or the
  word at the caret. **Mark...** is Ctrl+M on Windows and Linux (macOS uses Cmd+M to minimize).

### The search results panel

- **Find All in Current Document** (in the selection with In selection) and **in All Opened
  Documents** list their matches in a panel under the documents, with a splitter, as
  Notepad++'s Search results window does: `Search "foo" (5 hits in 2 files of 3 searched)`, each
  document with its count, and one row per line with its matches highlighted. The active
  document comes first.
- Searches stack, the newest on top, until Clear; a search or a document folds when clicked.
  The list is virtual (`uniform_list`), so tens of thousands of lines cost nothing to show; a
  line shows at most its first kilobyte.
- **Going to a result:** double-click or Enter, and F4 / Shift+F4 from anywhere, which walk the
  lines and wrap around. The result's tab comes forward and its first match on the line is
  selected. Results keep the line number and the offsets in the line, as Notepad++ does, so an
  edit above a match moves it away from its result. F7 shows or hides the panel.

## Consequences

- `fancy-regex` is a new dependency of `birchpad-core` (MIT; it brings no new crates besides).
- Regular expressions copy the text for each search; incremental search on very large files
  will want a cached copy.
- Not yet: Find in Files (whose results go to the same panel), incremental search, copying
  results, and remembering past searches.
