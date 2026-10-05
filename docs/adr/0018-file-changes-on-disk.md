# ADR 0018: Files changed by other programs

- Status: accepted
- Date: 2026-10-05

## Context

Another program may change, replace or delete a file that is open: a build, a version control
checkout, a log writer. Notepad++ notices it (Preferences > MISC > File Status
Auto-Detection):
- it offers to reload the file;
- it warns before throwing away unsaved changes;
- it asks whether to keep a deleted file;
- with View > Monitoring (tail -f), it follows a growing log.

Birchpad noticed nothing: saving overwrote changes made meanwhile.

## Decision

- **What a file looked like.** A buffer records the file's size and modification time when it
  reads or writes it (`DiskStamp`), plus a fingerprint of its first 4 KB (`Head`). Another
  size or time means another program changed the file. While a save runs, the file's changes
  are the save's own: the stamp is taken from the file just written.
- **When files are checked:**
  - The folders of open files are watched with `notify`, non-recursively, one watch per
    folder. Events within 150 ms are checked once.
  - Open files are also checked when the window comes back to the front.
  - Monitored files are checked every second too, because watchers miss changes on some
    network drives.

  A check only reads the files' metadata, on a background thread.
- **Settings** (`files.*`, Notepad++'s defaults):
  - `change-detection`: `"all"`, `"current"` (the active tab, checked again when it becomes
    active) or `"off"`;
  - `reload-silently = false`;
  - `reload-scrolls-to-end = false`.
- **What happens:**
  - A changed file without unsaved changes reloads, or asks "Reload" / "Don't Reload".
    Declining marks the document modified: it no longer matches the file.
  - A changed file with unsaved changes always asks "Reload" (losing them) / "Keep My
    Changes", even with silent reloads.
  - A deleted file asks "Keep in Editor" (the document becomes modified; saving creates the
    file again) / "Close".
  - An answered change is not asked about again until the file changes once more.
  - Questions bring the file's tab to the front. While the window is in the background they
    wait: the check runs again when the window comes back. Silent reloads and monitoring do
    not wait.
- **Reloading keeps the places.** A reload replaces the text. Carets and selections stay on the
  same line and column, as far as the new text has them, and so do bookmarks, collapsed folds
  and the scroll position. Undo history starts over, as in Notepad++. Reinterpreting a file in
  another encoding now keeps the places the same way.
- **Monitoring** (View > Monitoring (tail -f), for a saved document):
  - The document is read-only, with a banner and an eye on its tab.
  - When the file grows and its first 4 KB are unchanged, only the new bytes are read and
    added at the end, up to the last complete character (UTF-8, UTF-16, single-byte
    encodings). Otherwise the file is read again in full: rewritten or truncated, or in a
    multi-byte legacy encoding.
  - Added text is not an edit: there is nothing to undo and the document stays unmodified.
  - Views follow the end.
- **Sessions** record the file's stamp along with the backup copy of its unsaved changes. A
  file changed while Birchpad was closed is then asked about after the launch, as Notepad++
  does.

## Consequences

- Changes within the file system's time resolution that keep the size are not noticed: two
  seconds on FAT, 100 ns on NTFS.
- Watching uses one watch per folder of an open file. Watchers on network drives may be silent;
  then the check on returning to the window catches up, and monitoring polls every second.
- Monitoring a log in a multi-byte legacy encoding (Shift_JIS, GB18030) reads the whole file on
  each change.
