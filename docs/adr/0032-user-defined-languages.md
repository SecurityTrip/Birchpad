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
- **The language's own colors** are used, as in Notepad++, not the theme's.
- **Detection:** a file whose extension no built-in language claims gets the user defined
  language that lists it.

## Consequences

- UDL folding, the "Numbers" prefix and suffix lists, comments that must start a line, and
  nested delimiters are not supported yet; numbers are recognized in the usual forms.
- Lexing from the start costs a pass over the text up to the screen after each edit; UDL files
  are meant for small languages, and files over the large file limit get no user defined
  language.
