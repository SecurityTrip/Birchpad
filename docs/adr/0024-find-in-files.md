# ADR 0024: Find in Files

- Status: accepted
- Date: 2026-10-06

## Context

Notepad++'s Find dialog has a Find in Files tab: it searches, or replaces in, every file of a
folder whose name matches a filter, and lists the matches in the Search results window. ADR
0022 brought the other tabs and the results panel; this is the last tab.

## Decision

- **The same panel.** Find in Files is a tab of the find panel (Search > Find in Files...,
  Ctrl+Shift+F), between Replace and Mark as in Notepad++, with Find what, Replace with,
  Filters, Directory with a "..." button for the folder dialog, and the options Follow current
  doc., In all sub-folders, In hidden folders, Match whole word, Match case and the search
  mode. Wrap around, backward and in selection mean nothing for files and are not shown.
- **Notepad++'s filters** (`birchpad_io::Filters`): wildcard patterns separated by spaces or
  semicolons, without regard to case. No pattern, `*` and `*.*` take every file; `!*.bak` leaves
  files out; `!\target` leaves out a folder directly in the searched one, `!+\target` one
  anywhere. An exclusion that names nothing is an error, not "every file".
- **Listing** (`birchpad_io::files_in`): each folder's files in name order, then its
  subfolders. Links to folders are not followed, so a link back up cannot loop. Hidden means
  the hidden attribute on Windows, as for Notepad++, and a leading dot elsewhere; In hidden
  folders takes hidden files too.
- **Open documents count as they are.** A file that is open is searched as it is in its tab,
  unsaved changes included, as Notepad++ does, and its results go to the tab. Replace in Files
  changes it there as one undo step and does not save it.
- **Other files** are read with the encoding detection of File > Open and written back in
  their own encoding, byte order mark and line endings, with the safe save of File > Save (ADR
  0007). A file is not changed when it would not save back exactly: a decoding problem, or a
  replacement with characters its encoding lacks. Read-only files are left alone, and a file
  with nothing to replace is never written.
- **Binary files are skipped**: a zero byte in the first 64 KB of the decoded text. Notepad++
  searches them, but their lines are noise in the results and replacing in them corrupts them.
- **In the background, in batches.** The folder is listed, then searched 32 files at a time on
  the background executor. Between batches the panel counts the files searched and Stop takes
  effect; the results found until then are listed. Searches and replaces that cannot start (no
  folder, a missing folder, a file instead of a folder, a filter error, an invalid regular
  expression) say so in the panel before anything is asked.
- **Replace in Files asks first**, naming the folder, as Notepad++ does.
- **Results** go to the search results panel like Find All's, with the full path of each file.
  A result in a file that is not open opens it and selects the match once it has been read; a
  result past the end of a file that changed since goes to its end.
- **Remembered:** the filters, the folder and the options are kept in `state.toml` between
  runs. Follow current doc. takes the folder of the active document whenever the tab opens, and
  an empty Directory field is filled in that way too.

## Consequences

- Searching reads every file in full; a folder of very large files takes as long as opening
  them. Stop is there for that.
- Results keep line numbers and offsets, as for the other searches (ADR 0022): editing a file
  above a match moves the match away from its result.
- Not yet: Find in Projects (with the project panels), and a search history for the fields.
