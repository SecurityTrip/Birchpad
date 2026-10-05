; Groovy, and Gradle build scripts. The grammar crate ships no highlight query.

(identifier) @variable

; Names that look like classes or constants.
((identifier) @type
 (#match? @type "^[A-Z][a-z0-9]"))
((identifier) @constant
 (#match? @constant "^[A-Z][A-Z0-9_]+$"))

(type_identifier) @type
[
  (integral_type)
  (floating_point_type)
  (boolean_type)
  (void_type)
] @type.builtin

(class_declaration
  name: (identifier) @type)
(interface_declaration
  name: (identifier) @type)
(enum_declaration
  name: (identifier) @type)
(record_declaration
  name: (identifier) @type)
(enum_constant
  name: (identifier) @constant)

(method_declaration
  name: (identifier) @function.method)
(constructor_declaration
  name: (identifier) @constructor)
(function_definition
  name: (identifier) @function)
(method_invocation
  name: (identifier) @function.method)
; Gradle's `implementation 'group:artifact:1.0'` and `id "plugin"`.
(juxt_function_call
  name: (identifier) @function)

(formal_parameter
  name: (identifier) @variable.parameter)
(field_access
  field: (identifier) @property)
(map_item
  key: (identifier) @property)

(annotation
  name: (identifier) @attribute)
(marker_annotation
  name: (identifier) @attribute)
"@" @attribute

[
  (this)
  (super)
] @variable.builtin

[
  (line_comment)
  (block_comment)
] @comment
(shebang) @keyword.directive

(string_literal) @string
; Single-quoted strings: the grammar calls them characters, as in Java.
(character_literal) @string
(escape_sequence) @string.escape

[
  (decimal_integer_literal)
  (hex_integer_literal)
  (octal_integer_literal)
  (binary_integer_literal)
  (decimal_floating_point_literal)
  (hex_floating_point_literal)
] @number
[
  (true)
  (false)
] @boolean
(null_literal) @constant.builtin

[
  "abstract"
  "as"
  "assert"
  "break"
  "case"
  "catch"
  "class"
  "continue"
  "def"
  "default"
  "do"
  "else"
  "enum"
  "extends"
  "final"
  "finally"
  "for"
  "if"
  "implements"
  "in"
  "instanceof"
  "interface"
  "native"
  "new"
  "non-sealed"
  "permits"
  "private"
  "protected"
  "public"
  "record"
  "return"
  "sealed"
  "static"
  "strictfp"
  "switch"
  "synchronized"
  "throw"
  "throws"
  "transient"
  "try"
  "volatile"
  "when"
  "while"
  "yield"
] @keyword

[
  "import"
  "package"
] @keyword.directive

[
  "!"
  "!="
  "%"
  "%="
  "&"
  "&&"
  "&="
  "*"
  "**"
  "*="
  "+"
  "++"
  "+="
  "-"
  "--"
  "-="
  "->"
  ".."
  "/"
  "/="
  "<"
  "<<"
  "<<="
  "<="
  "="
  "=="
  ">"
  ">="
  ">>"
  ">>="
  ">>>"
  ">>>="
  "?"
  "^"
  "^="
  "|"
  "|="
  "||"
  "~"
] @operator

[
  "("
  ")"
  "["
  "]"
  "{"
  "}"
] @punctuation.bracket

[
  ","
  "."
  ";"
  ":"
  "::"
] @punctuation.delimiter
