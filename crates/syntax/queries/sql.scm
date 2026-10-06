; Birchpad: tree-sitter-sequel's number patterns test `#match?` with Lua's `%d`, which a regular
; expression takes literally, so no number was ever colored. The same patterns with `\d`,
; appended after the grammar's query: later patterns win.

((literal) @number
  (#match? @number "^[-+]?\\d+$"))

((literal) @float
  (#match? @float "^[-+]?\\d*\\.\\d*$"))
