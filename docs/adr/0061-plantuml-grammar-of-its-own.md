# ADR 0061: A PlantUML grammar of Birchpad's own

- Status: accepted
- Date: 2026-11-04

## Context

Every language Birchpad highlights has a tree-sitter grammar, from a crate of its own on
crates.io; folding comes from the grammar's multi-line nodes, the Function List from its tags
query. PlantUML has no such crate, so a `.puml` was shown as plain text, folded by its
indentation, with nothing in the Function List, although Birchpad draws its diagrams. Notepad++'s
user defined languages, which Birchpad reads, can color keywords, but not fold `alt` ... `end`
apart from `end note`, nor list participants.

PlantUML is a family of languages (sequence, class, activity, C4 diagrams and more) with a
preprocessor, defined by what PlantUML accepts rather than by a grammar.

## Decision

- **A grammar crate in the workspace.** `birchpad-tree-sitter-plantuml` holds `grammar.js`,
  written for Birchpad, and the parser the tree-sitter CLI generates from it (`src/parser.c`,
  with the CLI of the version the workspace's `tree-sitter` has), compiled by its `build.rs`
  like the grammar crates of crates.io. The generated files are committed, so that building
  Birchpad needs no Node.js; they are regenerated when `grammar.js` changes, in the same commit.
- **A grammar of lines, not of diagrams.** A diagram runs from its `@start` line to its `@end`
  line (or to the end of the text while it is typed); the blocks that fold are nodes of their
  own (`alt`, `opt`, `loop`, `par`, `break`, `critical`, `group` and `box` up to `end` or
  `end box` alone on a line; `!if` to `!endif`; `!procedure` and `!function` to their ends; a
  line ending in `{` to its `}`; a note, legend, header or footer of several lines, whose lines
  are text); `!include` lines, participants' declarations, `== sections ==` and titles are
  lines of their kinds; any other line is a row of tokens (keywords, arrows, strings, colors,
  stereotypes, numbers, variables, the label after `:`). What PlantUML would refuse may still
  parse: the grammar serves highlighting, folding and the Function List, and PlantUML says what
  is wrong in the preview.
- **The end of the text ends a line.** A small external scanner gives the end of the text as
  the end of the last line, so that `@enduml` without a line break after it is a line like the
  others.
- **Language.** PlantUML is a language of the Language menu (`plantuml`; `.puml`, `.plantuml`,
  `.pu`, `.iuml`, `.wsd`, or a first line `@startuml`), with `'` and `/' '/` for Toggle
  Comment. The preview of a PlantUML document follows its language, and its extension as before.
  Ctrl+click on an `!include` line opens the file it names, looked for as the preview looks for
  it (the document's folder, then the include path).

## Consequences

- PlantUML is highlighted, folds by its blocks and lists its participants, sections, titles,
  procedures and functions, in documents and in Markdown's `plantuml` code blocks.
- The grammar is Birchpad's to keep: a construct of PlantUML it does not know is a row of
  tokens, colored by its words, until the grammar learns it.
- The workspace has a crate of generated C (about 2 MB of `parser.c`), compiled with the C
  compiler the other grammars need already.
