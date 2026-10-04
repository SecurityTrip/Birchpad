# ADR 0016: Split view

- Status: accepted
- Date: 2026-10-05

## Context

Notepad++ has two views, main and second. A document can be moved to the other view or cloned
into it, and a clone is a second view of the same text. Each view has its own carets, scroll
position and folding, and edits in one move everything in the other. Since ADR 0008 an
`EditorView` is one view of a shared `Buffer`, and views already map their selections and
collapsed folds through each other's edits. This ADR decides how the workspace shows two views
and moves documents between them.

## Decision

- **Two panes, one hidden when empty.** The workspace always has two panes, Notepad++'s main
  and second views. A pane without tabs is not drawn. Split view is simply both panes having
  tabs. Closing or moving the last tab of the active pane hands focus to the other one. Closing
  every document leaves "new 1" in the main view, as before.
- **Move and clone.** View > Move/Clone Current Document:
  - Move to Other View moves the tab, the same `EditorView` with its state, after the active
    tab of the other pane.
  - Clone to Other View adds a new view of the same buffer there. It starts where the original
    is: same selection, scroll position and collapsed folds (`ViewState`; sessions will reuse
    it).
  - A pane never shows a buffer twice. If the other pane already shows the document, its view
    is activated instead, and a moved tab closes.
  - Closing a view whose buffer another view still shows never asks to save. Only the last view
    of a modified document asks (already so in `confirm_close`).
- **What views share.** Text, undo history, bookmarks and token styles belong to the buffer.
  Carets, scroll position, collapsed folds, smart highlighting and overwrite mode belong to
  each view. Edits made in one view map the other's carets, rectangle and collapsed folds
  (`BufferEvent::Edited` with the editing view as its origin).
- **Focus.** The workspace follows the focus: a click in the other pane, or View > Focus on
  Another View (F8, as in Notepad++), makes that pane active. Menus, the status bar, the window
  title, the find panel and every command act on the active pane's active tab. In split view a
  blue line over the active pane marks it, where Notepad++ colors the tab of the focused view.
- **Splitter.** gpui-component's resizable panel group draws the two panes, side by side
  (Notepad++'s default) or one above the other, with a draggable divider. View > Rotate Split
  View switches the orientation and is remembered in `state.toml` (`split`). Notepad++ offers
  rotation on the splitter's context menu instead.
- **Dragging tabs.** A tab dropped on another tab goes to that place. A tab dropped on the text
  of a pane goes last. In the other pane, this moves the tab there, with the same rule as Move
  to Other View when that pane already shows the document.
- **Synchronized scrolling.** View > Synchronize Vertical Scrolling and Synchronize Horizontal
  Scrolling, off at start, as in Notepad++:
  - After each frame, a view reports how many rows and pixels it scrolled
    (`EditorEvent::Scrolled`).
  - The other pane's active view then scrolls by as much. So the offset between the views when
    synchronization was turned on stays, whatever moved the first view (keys, wheel,
    scrollbar).
  - A scroll that came from the other view is not reported back.

## Consequences

- Move to New Instance and Open in New Instance are not implemented. Neither is the tab context
  menu (Close, Move/Clone to Other View...) of Notepad++.
- The tabs of both panes, the divider position and synchronization are not kept between runs.
  Sessions (stage 9) will save the panes. Only the orientation is remembered now.
- GPUI reports focus changes only for the active window. Tests activate their window, as the
  user's window is active when they type.
