; Scripts are JavaScript, unless their type says they hold data (JSON, import maps) or a
; template.
((script_element
  (start_tag) @_start
  (raw_text) @injection.content)
 (#not-match? @_start "type=.?(application/(ld\+)?json|importmap|text/(template|x-template|html))")
 (#set! injection.language "javascript"))

((script_element
  (start_tag) @_start
  (raw_text) @injection.content)
 (#match? @_start "type=.?(application/(ld\+)?json|importmap)")
 (#set! injection.language "json"))

((style_element
  (raw_text) @injection.content)
 (#set! injection.language "css"))
