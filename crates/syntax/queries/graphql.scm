; GraphQL schemas and operations. The grammar crate ships no highlight query.

(comment) @comment
(description) @comment.documentation

(name) @variable

; Types: in definitions, extensions and references.
(named_type
  (name) @type)
(object_type_definition
  (name) @type)
(interface_type_definition
  (name) @type)
(union_type_definition
  (name) @type)
(enum_type_definition
  (name) @type)
(input_object_type_definition
  (name) @type)
(scalar_type_definition
  (name) @type)
(object_type_extension
  (name) @type)
(interface_type_extension
  (name) @type)
(union_type_extension
  (name) @type)
(enum_type_extension
  (name) @type)
(input_object_type_extension
  (name) @type)
(scalar_type_extension
  (name) @type)

; Fields and arguments.
(field_definition
  (name) @property)
(field
  (name) @property)
(field
  (alias
    (name) @property))
(input_value_definition
  (name) @variable.parameter)
(argument
  (name) @variable.parameter)
(object_field
  (name) @property)

(operation_definition
  (name) @function)
(fragment_definition
  (fragment_name
    (name) @function))
(fragment_spread
  (fragment_name
    (name) @function))

(directive
  (name) @attribute)
(directive_definition
  (name) @attribute)
"@" @attribute

(variable) @variable.parameter
(enum_value
  (name) @constant)
(directive_location) @constant

(string_value) @string
(int_value) @number
(float_value) @number
(boolean_value) @boolean
(null_value) @constant.builtin

(operation_type) @keyword
[
  "directive"
  "enum"
  "extend"
  "fragment"
  "implements"
  "input"
  "interface"
  "on"
  "repeatable"
  "scalar"
  "schema"
  "type"
  "union"
] @keyword

[
  "="
  "|"
  "&"
  "!"
  "..."
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
  ":"
  "$"
  (comma)
] @punctuation.delimiter
