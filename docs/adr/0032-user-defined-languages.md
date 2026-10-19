# ADR 0032: User Defined Languages

- Status: accepted
- Date: 2026-10-12

## Context

Notepad++'s User Defined Languages (UDL 2.x) describe a language in XML: comments, keyword
lists, operators, delimiters and folding words, each with its colors. Many languages Notepad++
has no lexer for exist only as shared UDL files. Birchpad highlights with tree-sitter grammars,
which UDL files cannot become.

## Decision

- **UDL files are read as they are** from `userDefineLangs` next to the settings (Notepad++'s
  folder name), several languages to a file allowed; Language > User Defined Language >
  Import... checks files and copies them in. The languages are listed in that submenu and run
  `language.set` with `udl:<name>`, which sessions keep.
- **A small lexer colors the text** (`birchpad_syntax::UserLanguage::highlight`), as Notepad++'s
  UDL lexer does: block and line comments, delimiters with their escape character (to the end of
  the line when they have no closing part), operators (longest first), folding symbols, and
  words, which are numbers, keywords of lists 1 to 8 (ignoring case if the language says so, or
  as prefixes), folding words or word operators. It lexes from the start of the text, since a
  comment may begin far above, up to a little past what is shown, and keeps the result until the
  text changes.
- **Numbers** follow the "Numbers" lists of UDL 2.1: a number may start with a prefix of
  `prefix1` (`0x`) and then have the characters of `extras1` (`a`...`f`), likewise with
  `prefix2` and `extras2`; it may end with a suffix (`L`); a `range` separator (`..`) joins two
  numbers into one. With none of the lists set, numbers take the usual forms.
- **Line comments** start anywhere, after white space only, or in the first column only, as
  the language's `forcePureLC` says (0, 1, 2).
- **Nested delimiters:** a delimiter holds what the `nesting` bits of its style allow:
  other delimiters (bits 0 to 7), comments (0x100), line comments (0x200), keywords 1 to 8
  (0x400 and up), operators (0x40000, 0x80000) and numbers (0x100000), each in its own color,
  the delimiter's color between them. Delimiters nest at most eight deep.
- **Folding** follows the language's folders: "Folders in code" 1 (symbols) and 2 (words)
  outside comments and delimiters, "Folders in comment" inside comments; "middle" words
  (`else`) neither open nor close. A fold closed on the line it opened is none, one never
  closed runs to the end, and a line starts one fold at most. The document's indentation
  folds are not used for a user defined language.
- **The language's own colors** are used, as in Notepad++, not the theme's.
- **Detection:** a file whose extension no built-in language claims gets the user defined
  language that lists it.

## Consequences

- Folding points in the middle of a fold (`else`) do not split it, as they do in Notepad++.
- Lexing from the start costs a pass over the text up to the screen after each edit; UDL files
  are meant for small languages, and files over the large file limit get no user defined
  language.
