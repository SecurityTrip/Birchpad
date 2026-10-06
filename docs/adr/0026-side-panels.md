# ADR 0026: Side panels

- Status: accepted
- Date: 2026-10-06

## Context

Notepad++ has dockable panels beside the documents: Function List, Document Map, Document
List, Folder as Workspace, three Project Panels, Clipboard History and the Character Panel.
Each opens from a menu item with a check mark, docks on a side of the window, shares that side
with other panels as tabs, and is remembered for the next start. Its panels can also float and
dock anywhere by dragging.

## Decision

- **Two docks.** A dock on each side of the documents, with a splitter: Folder as Workspace, the
  project panels, Document List, Clipboard History and the Character Panel on the left;
  Function List and Document Map on the right, where Notepad++ puts them by default. The search
  results stay under the documents. Panels do not float and cannot be moved to another side
  yet: a dock with a fixed set is simpler and covers how they are used.
- **Tabs.** A dock with several open panels shows them as tabs; the dock's close button closes
  the panel shown, and the panel opened last on that side takes its place.
- **Commands.** Each panel has a command, `panel.*` (`panel.project` takes `{ "panel": 1..3 }`),
  in the menus where Notepad++ has it: View > Project, Folder as Workspace, Document Map,
  Document List, Function List; Edit > Character Panel, Clipboard History. The command opens
  the panel, or closes it if it is open, and the menu item is checked while it is open.
- **Remembered.** The open panels are kept in `state.toml` (`[panels] open`) in the order they
  were opened and open again on the next start; unknown names are ignored.
- **Panels read the workspace.** A panel holds a weak handle to the workspace and reads the
  documents when it draws, so it never holds a copy that can go stale; it acts through the
  workspace's own methods (activating a view, closing with confirmation).

### Document List

- The open documents, in the order of the tabs (the main view's, then the second view's),
  with their names and paths. Modified documents are marked `*`, documents of the second view
  `(2)` in split view, and the active one is highlighted.
- A click shows the document; a middle click closes it, asking about unsaved changes as File >
  Close does. A click on a column's heading sorts by it, without regard to case, in reverse on
  a second click and back to the order of the tabs on a third.

## Consequences

- The docks' widths last for the session only.
- Floating panels and docking on another side are left for later.
