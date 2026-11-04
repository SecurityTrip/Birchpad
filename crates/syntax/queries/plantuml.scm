; Birchpad's own: PlantUML (birchpad-tree-sitter-plantuml).

(comment) @comment
(block_comment) @comment

(start) @keyword.directive
(end) @keyword.directive
(directive_name) @keyword.directive
(include
  path: (path) @string.special)

(keyword) @keyword

(arrow) @operator
(string) @string
(label) @string
(note
  (text) @string)
(color) @constant
(stereotype) @type
(number) @number
(variable) @variable

(participant_declaration
  name: (_) @type)
(procedure
  name: (name) @function)
(section) @markup.heading
(title
  name: (name) @markup.heading)

(punctuation) @punctuation
[
  "{"
  "}"
] @punctuation.bracket
