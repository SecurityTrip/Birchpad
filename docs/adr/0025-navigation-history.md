# ADR 0025: Navigation history

- Status: accepted
- Date: 2026-10-06

## Context

Search results, Go To, bookmarks, Find in Files and switching tabs move the caret far from
where it was, often into another document. Notepad++ has no way back (a plugin, Location
Navigate, adds one); browsers and IDEs have Back and Forward. The Phase 3 roadmap lists
navigation history.

## Decision

- **Jumps, not moves.** The workspace observes the active view. A caret that lands in another
  document, or ten lines or more away from the current place, has jumped: the place it left
  goes on the back stack and the forward stack is dropped, as in a browser. Smaller moves only
  update the current place, so Go Back returns to where the caret last was before the jump.
  Any command that jumps is covered without telling the history about it.
- **Typing is no jump.** Places follow the edits of their documents; the current place moves
  past text inserted where it is, as the caret does, so typing or pasting many lines records
  nothing. A file that is still being read is not observed until its caret is placed.
- **Closed documents.** A place in a closed file opens the file again at that offset; a place in
  a closed untitled document is dropped.
- **Keys.** Search > Go Back and Go Forward: Alt+Left and Alt+Right on Windows and Linux, as in
  browsers. On macOS Option+arrows move by word, so Ctrl+- and Ctrl+Shift+- are used, as in
  Xcode and Visual Studio Code. Each stack keeps fifty places.

## Consequences

- The history is per window and is not saved with the session.
- A jump is judged by lines, so moving far along one very long line is not one.
