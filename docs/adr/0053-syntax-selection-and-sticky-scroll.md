# ADR 0053: Selection by the syntax tree and sticky scroll

- Status: accepted
- Date: 2026-10-25

## Context

Birchpad keeps a tree-sitter tree of each document ([ADR 0011](0011-syntax-highlighting.md)), used for
highlighting, folding and brackets. Editors beyond Notepad++ use it for two more things: a
selection that grows to the piece of syntax around it and shrinks back (Expand Selection), and
the first lines of the blocks around the top of the view kept there while scrolling (sticky
scroll). Built from how these behave for a user, not from another editor's code.

## Decision

- **Expand Selection** (`edit.expand-selection`, Ctrl+Alt+Up) grows each selection to the
  smallest named node of the tree that holds it and is larger (`birchpad_syntax::
  expand_selection`), in the document's tree or the tree of a language embedded there,
  whichever node is smaller. A node whose first and last children are brackets or quotes
  offers what is between them first: `"hi"` then `("hi", a)`. Without a tree, or once the
  tree has nothing larger, a selection grows to its word, its line and the whole text.
- **Shrink Selection** (`edit.shrink-selection`, Ctrl+Alt+Down) goes back the way Expand
  Selection grew: the view keeps the selections it grew from while the selection is still the
  one it made, as Multi-select keeps its order; once the selection changes otherwise, Shrink
  Selection does nothing. Alt+Shift+arrows, other editors' keys, select columns here.
- **Sticky scroll** (`editor.sticky-scroll`, off by default; `editor.sticky-scroll-lines`, 5)
  shows the header lines of the folds around the top of the view, outermost first, over the
  first rows, with their line numbers and their highlighting; folds come from the same fold
  points as the folding margin, so indentation folds work for languages without a tree. The
  headers cover rows, so the line that counts is the one under them (`birchpad_view::
  sticky_headers`). A click on one goes to it. Right-to-left views have none.

## Consequences

- Both follow the folds and nodes each grammar defines: a language whose grammar has no node
  for a construct selects past it.
- Sticky rows are shaped each frame like the visible ones, a few rows more.
