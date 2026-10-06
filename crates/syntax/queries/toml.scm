; From tree-sitter-toml-ng 0.7.0 (https://github.com/tree-sitter-grammars/tree-sitter-toml),
; queries/highlights.scm, with the capture of a pair's key moved onto the key: the original
; captures the whole pair, so keys kept the table-name color. MIT License: see THIRD-PARTY.md
; in this folder.

; Properties
;-----------

(bare_key) @type

(quoted_key) @string

(pair
  (bare_key) @property)

(pair
  (dotted_key
    (bare_key) @property))

; Literals
;---------

(boolean) @boolean

(comment) @comment

(string) @string

[
  (integer)
  (float)
] @number

[
  (offset_date_time)
  (local_date_time)
  (local_date)
  (local_time)
] @string.special

; Punctuation
;------------

[
  "."
  ","
] @punctuation.delimiter

"=" @operator

[
  "["
  "]"
  "[["
  "]]"
  "{"
  "}"
] @punctuation.bracket
