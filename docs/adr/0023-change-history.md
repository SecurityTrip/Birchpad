# ADR 0023: Change history

- Status: accepted
- Date: 2026-10-06

## Context

Notepad++ 8.4.6 added Change History, from Scintilla: a margin marks the lines changed since
the document was opened, orange while they differ from the file on disk and green once saved.
Undoing shows two more colors ("reverted to origin", "reverted to modified"), and the changes
can also be marked in the text. Users asked for the margin
([#11](https://github.com/SecurityTrip/Birchpad/issues/11)).

Scintilla tracks the history through its undo history, per character. Birchpad's undo history
is in `birchpad-core`, and its decorations already follow edits (ADR 0010).

## Decision

- **Compare lines, not edits.** The change history keeps two texts: the document as opened
  (or last read from the file) and as last saved. Ropes share their text, so keeping them costs
  little. A line of the current text is *modified* when it is not in the saved text, and a
  *saved change* when it is but differs from the text as opened. Lines are compared with their
  line breaks, so converting line endings changes every line, as in Notepad++.
- **Diff only what changed.** `birchpad_core::diff_lines` finds the bytes both texts start and
  end with by comparing the ropes' chunks (memory comparisons), then compares only the lines
  between with Myers' diff (`similar`, already in the lock file through `insta`) on line hashes,
  with a time limit for very different texts. One edit in a 10 MB file takes under 2 ms, edits
  at both ends of it about 60 ms.
- **In the background.** After each edit, save or text appended to the file, the buffer
  compares in the background. Until the result arrives, the changed lines are line markers
  that move with the text like bookmarks. Edits made during a comparison are composed and
  applied to its result, and another comparison follows.
- **A deletion** marks the line that took the deleted lines' place, as Scintilla does.
- **The margin** sits between the symbol (bookmark) margin and the folding margin, as in
  Notepad++, with its colors: orange `#FF8000` and green `#00A000`. Every row of a wrapped line
  gets the bar. A click there selects the line, as in the line number margin.
- **The history starts over** when the file is read again (reloaded after a change on disk, or
  in another encoding), as Notepad++ starts a new undo history then. A document restored from a
  session starts with no changes. Text that View > Monitoring appends is part of the file, not a
  change.
- **Setting:** `editor.change-history = "margin"` (the default, as in Notepad++) or `"off"`,
  which also stops the comparisons.

## Consequences

- Undoing back to the saved text clears the orange lines instead of showing Scintilla's
  "reverted" colors: the margin says how the text differs from the file, not what was done to
  it.
- Changes are not marked in the text (Notepad++'s "Show in text"), and there is no
  "go to next change" yet.
- A comparison keeps a second copy of the text alive while it runs, and the original and saved
  texts stay alive as long as the document is open: memory grows with what was changed, not
  with the file's size.
