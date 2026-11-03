# ADR 0062: Includes followed through PlantUML's preprocessor

- Status: accepted
- Date: 2026-11-04
- Changes [ADR 0057](0057-plantuml-preview-redone.md): which files the preview finds a diagram
  includes, to follow them (roadmap 7.4p), to place mistakes in them (7.4u) and to open them
  with Ctrl+click (7.4r).

## Context

Birchpad read an `!include`'s path only when it was written out. In the architecture
repository the preview was tried on, every include goes through a variable or a function:
`!include $DIAGRAMS_ROOT/library/main.iuml`, with `!$DIAGRAMS_ROOT ?= "../.."` in the
document, and `!include FromRoot(/parts/billing/containers.puml)`, with
`!function FromRoot($p)` ... `!return $DIAGRAMS_ROOT + $p` in an included file. So the preview
followed none of the files those diagrams include, and Ctrl+click on those lines opened nothing.

Two ways were weighed. PlantUML itself could say which files it read: but it has no option that
lists them, and its picture names an included file only when a mistake is in it (and then by a
name of its own, `desc2`, at times). Or Birchpad could read the preprocessor's lines as PlantUML
does, as far as those includes need.

## Decision

- **Birchpad reads the preprocessor's variables and functions itself**, in the order PlantUML
  reads the text: the document's lines, and each included file's where its `!include` is.
  - Variables: `!$name = value`, `!$name ?= value` (kept if set already), `!global $name =`.
    Values are expressions of strings in double or single quotes, numbers, variables, calls
    and `+`.
  - Functions that return a value: `!function Name($a, $b = "default")` with its `!return`, on
    its own lines or on the definition's line; procedures and their lines set nothing.
  - Calls: the diagram's own functions, `%dirpath()` (the folder of the file being read) and
    `%getenv(NAME)`; an argument that is no expression is taken as written
    (`FromRoot(/billing/containers.puml)`), as PlantUML takes unquoted text. Calls nest at most 16
    deep.
  - An `!include`'s text has its variables and calls replaced by their values; the file is
    looked for next to the file that includes it, then in the include path (ADR 0057, roadmap
    7.4m), the document's folder first.
- **What cannot be worked out is left out**: a variable set nowhere before the line, a
  function Birchpad does not know (PlantUML's other built-in ones), `!if` branches (all are
  read, a later value replacing an earlier one). Such an include is not followed, and Redraw
  draws again by hand.
- **One reading for all three uses.** The walk returns the files, in PlantUML's order, and the
  `!include` lines that named them; the preview watches the files' folders, places mistakes
  in them, and Ctrl+click on a line of the document opens the file that line named.

## Consequences

- The includes of that repository are followed: a change to an included file draws its diagrams
  again, and Ctrl+click opens the files of `$DIAGRAMS_ROOT` and `FromRoot(...)` lines.
- Birchpad keeps a small part of PlantUML's preprocessor of its own; an include written with a
  construct it does not read is still drawn by PlantUML, but not followed or opened.
