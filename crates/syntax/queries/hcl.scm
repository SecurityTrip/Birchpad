; HCL and Terraform. The grammar crate ships no highlight query.

(comment) @comment

(identifier) @variable

; resource "aws_instance" "web" { ... }: the block type, then nested block types.
(body
  (block
    (identifier) @keyword))
(body
  (block
    (body
      (block
        (identifier) @type))))

(attribute
  (identifier) @property)

(object_elem
  key: (expression
    (variable_expr
      (identifier) @property)))

; var.region, local.name, data.x: the root and the attributes.
(expression
  (variable_expr
    (identifier) @variable.builtin)
  (get_attr
    (identifier) @property))

(get_attr
  (identifier) @property)

(function_call
  (identifier) @function)

[
  (quoted_template_start)
  (quoted_template_end)
  (template_literal)
  (heredoc_identifier)
  (heredoc_start)
] @string

[
  (template_interpolation_start)
  (template_interpolation_end)
  (template_directive_start)
  (template_directive_end)
  (strip_marker)
] @punctuation.special

(numeric_lit) @number
(bool_lit) @boolean
(null_lit) @constant.builtin

[
  "for"
  "endfor"
  "in"
  "if"
  "else"
  "endif"
] @keyword

[
  "!"
  "*"
  "/"
  "%"
  "+"
  "-"
  ">"
  ">="
  "<"
  "<="
  "=="
  "!="
  "&&"
  "||"
  "?"
  "=>"
  (ellipsis)
] @operator

[
  "{"
  "}"
  "["
  "]"
  "("
  ")"
] @punctuation.bracket

[
  "."
  ".*"
  ","
  "[*]"
] @punctuation.delimiter
