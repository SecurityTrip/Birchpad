# ADR 0017: Sessions and periodic backup

- Status: accepted
- Date: 2026-10-05

## Context

Notepad++ remembers the open documents for the next launch. With "session snapshot and
periodic backup", the default since version 7.6, it also keeps unsaved changes, including
untitled "new N" tabs:
- Quitting does not ask about them.
- They come back on the next launch.
- A crash loses at most a few seconds of typing.

Birchpad asked about unsaved changes on quitting and forgot everything else. Quitting from the
macOS Dock bypassed the question and lost the changes (Phase 1).

## Decision

- **Settings** (Notepad++'s Preferences > Backup):

  | Key | Default | Meaning |
  |---|---|---|
  | `session.remember` | `true` | reopen the documents of the last run |
  | `session.backup-unsaved` | `true` | keep unsaved changes in backup copies; quitting does not ask |
  | `session.backup-interval-seconds` | `7` | how often they are backed up |

  `backup-unsaved` needs `remember`. The existing `BackupUnsaved` policy turns it off, for
  organizations that do not want document contents stored outside the files. `-nosession`
  neither restores nor saves the session, and backs nothing up. A portable copy keeps its
  session in its `data` folder.
- **What a session holds** (`birchpad_config::Session`), for each document:
  - the path, or the untitled number;
  - the backup copy with its unsaved text;
  - the encoding chosen in the Encoding menu, or, with a backup copy, the encoding and line
    ending to save in;
  - the language chosen in the Language menu;
  - `-ro`;
  - the bookmarks.

  For each tab of the two views: its document (clones share one), the selections, the scroll
  position and the collapsed folds. Also the active tab of each view and the active view.
  Token styles are not saved, as in Notepad++.
- **Format.** TOML with `version = 1`. A newer version is refused rather than misread.
  Unknown keys are ignored, so a newer Birchpad with the same version can add some.
- **Files.** Everything is in the user data folder:
  - `session.toml`, with the session it replaced kept as `session.toml.bak`;
  - `backup/<name>@<time>-<n>`: one copy of each document with unsaved text, as UTF-8.

  Every file is written to a temporary file, flushed to disk and renamed over the old one. On
  Unix it is created readable only by the user, the folder only for the user. On Windows they
  inherit the private permissions of the user's profile. A damaged or missing `session.toml`
  loads the `.bak` one, and a temporary file left by a crash is never read.
- **Snapshots.** Every interval, and on quitting:
  1. Each document with unsaved text gets a backup copy: a modified file, or an untitled
     document with any text. Only copies whose revision changed are written.
  2. Then the session is written, if it changed.
  3. Then backup copies used by neither this session nor the previous one are removed.

  The texts are rope snapshots, cheap to take, so the periodic writes run on a background
  thread and large documents do not stall typing. Quitting writes on the main thread.
- **Quitting:**
  - With backups, closing the window, File > Exit and quitting from the Dock or by logging
    out save a snapshot and quit without asking.
  - Without backups, closing the window and File > Exit ask about each modified document
    (Save, Don't Save, Cancel) without closing anything, then save the session of the files.
    Untitled documents are left out of it. Quitting from the Dock cannot ask: the session of
    the files is saved and unsaved changes are lost, as before.
  - With no session (`remember` off, `-nosession`), the open files become recent files as
    before.
- **Restoring.** At startup the last session is opened; an empty one gives "new 1":
  - Files without a backup copy are read from disk again, in the encoding chosen before.
    Carets, scroll position and folds apply once the file has been read, with positions past
    the end clamped. Files that no longer exist are dropped silently.
  - Documents with a backup copy show its text, modified. They are flagged as never saved, so
    undo cannot make them look unmodified. Collapsed folds wait for the first parse of their
    language before invalid ones are dropped.
  - Detecting that a file changed on disk since is stage 10.
- **Session files.** File > Save Session... writes the files of the window: positions,
  bookmarks, folds, languages, no unsaved text. File > Load Session... and `-openSession`
  open such a file next to what is open; an open document is not opened twice. They also
  read Notepad++'s `session.xml`:
  - files of both views, with carets, first line, horizontal offset, bookmarks, collapsed
    folds, language name and read-only flag;
  - unsaved text from Notepad++'s backup copies, in their own encoding.

  Encodings are detected again; tab colors and the document map have no counterpart.

## Consequences

- The split orientation stays in `state.toml`, as before. The divider position is not saved.
- A crash loses at most the last interval of typing. Saving to the real file stays explicit.
- Detecting that a restored file changed on disk, and asking about it, comes with stage 10.
- Session tests use a data folder of their own. The other tests have none, so they keep the
  old question on quitting.
