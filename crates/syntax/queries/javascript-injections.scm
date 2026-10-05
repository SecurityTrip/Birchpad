; Also used for TypeScript.

; Tagged templates name their language: html`...`, css`...`, sql`...`, gql`...`. The parts of
; all the templates with one tag are parsed together, without the ${} substitutions.
(call_expression
  function: [
    (identifier) @injection.language
    (member_expression
      property: (property_identifier) @injection.language)
  ]
  arguments: (template_string
    (string_fragment) @injection.content)
  (#set! injection.combined)
  (#set! injection.include-children))

((regex_pattern) @injection.content
 (#set! injection.language "regex"))

; Documentation comments: /** ... */.
((comment) @injection.content
 (#match? @injection.content "^/[*][*][^/]")
 (#set! injection.language "jsdoc"))
