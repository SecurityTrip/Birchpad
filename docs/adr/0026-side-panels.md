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

### Function List

- **From the syntax tree, not regular expressions.** Notepad++ describes each language's
  functions with regular expressions in XML files. Birchpad already has a tree-sitter tree of
  every document, so the list comes from tags queries, which GitHub's code navigation uses
  too: `@definition.function`, `.class`, `.method` and their kin capture a definition, `@name`
  its name (`birchpad_syntax::outline`). Seventeen grammar crates ship one; Birchpad writes its
  own, a few patterns each, for Bash, Batch, Haskell, INI, Kotlin, Make, Markdown (headings),
  PowerShell, Scala and TOML (tables), and adds Rust's `impl` blocks to Rust's.
- **What is listed:** classes and their kin, interfaces, modules, functions, methods, macros,
  type aliases and sections. The queries' references, constants and variables are left out. A
  definition inside another one's range is its child: methods under their class or `impl`,
  sections under their heading. A definition two patterns match keeps the more telling kind.
- **Kept current:** the outline is read again whenever the document's tree changes (it is
  reparsed in the background after edits), with a time budget for huge files. The definition
  the caret is in is highlighted.
- **Using it:** a click selects the definition's name in the text; the arrows fold a level;
  the search field filters by name, keeping the parents of what matches and opening folded
  levels; the A-Z button orders each level by name instead of by position. Plain text, a
  language without a query, and files over the large file limit say why the list is empty.

### Document Map

- **Bars, not text.** Notepad++ draws a second Scintilla view zoomed out. Text a few pixels high
  is unreadable anyway, so the map paints each line as a strip three pixels high in which every
  run of characters that is not blank is a bar in its syntax color, a pixel and a half per
  column (tabs to their stops, wide characters two columns). Only the lines on the map are
  painted, with the highlights of just those lines.
- **The frame** marks the lines the view shows. A map longer than the panel scrolls with the
  view so that the frame sits as far down the map as the view sits in the document.
- **Using it:** a click or a drag centers the view on that line, the wheel scrolls the view.
- Panels that show the active view (Function List, Document Map) observe it, so a moved caret,
  a scroll or an edit redraws them.

## Consequences

- The docks' widths last for the session only.
- Floating panels and docking on another side are left for later.
