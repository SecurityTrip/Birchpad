# ADR 0055: SQL's backslash escapes read by the parser

- Status: accepted
- Date: 2026-10-28

## Context

Notepad++'s Preferences > Language has "Treat backslash as escape character for SQL": with it,
`'it\'s'` is one string, as MySQL reads it; without it, as standard SQL reads it, the string ends
at the second quote and only a doubled quote (`'it''s'`) stays inside. Birchpad highlights SQL
with the tree-sitter-sequel grammar ([ADR 0011](0011-syntax-highlighting.md)), whose strings end
at any quote that is not doubled, with no switch for backslashes; 0.7.0 left the setting out
for that reason (roadmap 7.4f). Built from the user manual's description of the setting.

A grammar of Birchpad's own for SQL would be a fork of a large grammar to change one token. A
query cannot join what the grammar already split into a string and an error.

## Decision

- With `sql.backslash-escapes` on, the parser of an SQL tree reads a quote that follows an odd
  run of backslashes as a backslash (`birchpad_syntax::backslash_escaped`): `'it\'s'` reads
  `'it\\s'`, one string; `'a\\'` reads as it is, the backslash escaping itself. The text the
  parser reads has the same length as the document's, byte for byte, so every node lies where
  its text is, and highlighting, folds, brackets and selection work on the document as before.
- The parse callback does this chunk by chunk, counting the backslashes before the chunk from
  the document, and only for SQL, the document's own or SQL embedded in another language.
- `Syntax::set_backslash_escapes` drops the trees when the setting changes, and the buffer
  parses the whole text again when the settings are applied.

## Consequences

- Double-quoted text takes the same escapes (`"a\"b"`), as MySQL's does.
- A backslash before a quote outside a string (which SQL has no use for) is read the same way.
- The text a parse reads may now be a copy of the chunk where it changes a quote; other
  languages, and SQL with the setting off, read the document's chunks as they are.
