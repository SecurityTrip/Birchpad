; <script> is JavaScript, or TypeScript with lang="ts" (the later pattern wins).
((script_element
  (raw_text) @injection.content)
 (#set! injection.language "javascript"))

((script_element
  (start_tag
    (attribute
      (attribute_name) @_attr
      (quoted_attribute_value
        (attribute_value) @_lang)))
  (raw_text) @injection.content)
 (#eq? @_attr "lang")
 (#any-of? @_lang "ts" "typescript")
 (#set! injection.language "typescript"))

((style_element
  (raw_text) @injection.content)
 (#set! injection.language "css"))

; {expressions} in the markup and in {#if}, {#each} and the other blocks.
((svelte_raw_text) @injection.content
 (#set! injection.language "javascript"))
