# ADR 0068: File operations in Folder as Workspace

- Status: accepted
- Date: 2026-11-12
- Builds on [ADR 0026](0026-side-panels.md) (Folder as Workspace among the side panels),
  [ADR 0018](0018-file-changes-on-disk.md) (the folders of open files watched) and
  [ADR 0041](0041-crate-layout.md).

## Context

Roadmap 7.7b asks Folder as Workspace to work with files as an IDE's project tree does: new file
and folder, rename, copy, cut and paste, duplicate, delete to the recycle bin, move by
dragging, show in the file manager; open documents follow their files, and the tree follows the
disk. Notepad++'s panel only opens files and copies their paths, so there is nothing of it to
match; the questions are where the operations live, what they refuse, and what happens to the
documents and the watchers when files move under them.

## Decision

### The operations without a window

- `birchpad-io` (`entries`) makes, renames, copies and moves files and folders: `create_file`,
  `create_folder`, `rename`, `copy_into`, `duplicate`, `move_into`, with `check_name` and
  `free_name`, and `relocated` for a path inside something that moved. Its tests run on the
  disk, without a window.
- Nothing overwrites. A name taken is refused (`EntryError::AlreadyExists`), with one
  exception: a copy takes a free name of its own (`a copy.txt`, `a copy 2.txt`, ...), so that
  Paste and Duplicate always succeed beside the original. A folder is never copied or moved into
  itself. A name that differs only in case renames the entry where names ignore case (Windows,
  macOS). A move between drives copies and then removes. Links to folders inside a copied folder
  are left out, so that a link cannot make a copy endless.
- Deleting is moving to the recycle bin (the `trash` crate), as File > Move to Recycle Bin does,
  asked first unless MISC > Folder as Workspace: confirm moving to the recycle bin
  (`misc.confirm-workspace-delete`) is off. Nothing is deleted for good from the tree.

### The panel

- The right-click menu has New File..., New Folder..., Rename..., Duplicate, Copy, Cut, Paste
  and Move to Recycle Bin; the keys are F2, Ctrl+C, Ctrl+X, Ctrl+V, Ctrl+D and Delete while the
  tree has the focus. The editor binds the same keys in its own context, so the panel handles
  them itself, as it handles the arrows. Names are asked in the dialog File > Rename uses.
- Copy and Cut set an entry aside in the panel, not on the system's clipboard: what the panel
  pastes is files, and the system's clipboard keeps the text the user copied.
- A row is dragged onto a folder, or onto a file for the file's folder, and moves there.
- Top folders are renamed (and remembered by the new name) but not moved, cut, duplicated or
  deleted from the tree: they are the workspace's, and Remove takes them out of it.

### Documents and watchers

- After a rename or a move, the documents of the entry and of everything in it take their new
  paths (`Buffer::renamed`), unsaved changes and all; the tree keeps what it knew of a moved
  folder (unfolded stays unfolded), and the selection and what was set aside follow it.
- After a move to the recycle bin, the tabs of its files close, except those with unsaved
  changes, which stay open with their text as a deleted file's tab does.
- Windows refuses to rename, move or delete a folder something watches. Before the tree changes
  a folder, the folders in it are no longer watched for changes of open files
  (`Workspace::unwatch_under`); once the documents have their new paths, the folders they are
  in are watched again.
- The tree reads the folders it changed again at once; for what other programs change, it keeps
  reading unfolded folders every two seconds.

## Consequences

- The file operations are tested without a window, positive, negative and boundary cases on a
  real disk; the panel's tests drive the menu's methods, the keys and a drag.
- Copy and Cut do not reach other programs, and files copied in Explorer or Finder cannot be
  pasted into the tree; dropping files from the file manager on the window still opens them.
- Undo of file operations, several entries selected at once, and watching the tree's folders
  instead of reading them again are left for later.
