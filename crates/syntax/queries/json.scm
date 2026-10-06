; From tree-sitter-json 0.24.8 (https://github.com/tree-sitter/tree-sitter-json),
; queries/highlights.scm, with the key pattern moved after the string one: for the same node a
; later pattern wins in Birchpad, so keys stay keys instead of plain strings. MIT License: see
; THIRD-PARTY.md in this folder.

(string) @string

(pair
  key: (_) @string.special.key)

(number) @number

[
  (null)
  (true)
  (false)
] @constant.builtin

(escape_sequence) @escape

(comment) @comment
