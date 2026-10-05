; Everything outside <?php ... ?> is HTML, parsed as one document.
((text) @injection.content
 (#set! injection.language "html")
 (#set! injection.combined))

; Heredocs and nowdocs named after a language: <<<SQL ... SQL.
(heredoc
  (heredoc_body) @injection.content
  (heredoc_end) @injection.language
  (#set! injection.include-children))

(nowdoc
  (nowdoc_body) @injection.content
  (heredoc_end) @injection.language
  (#set! injection.include-children))
