; Birchpad's own: headings. A section holds its heading and the sections under it, so
; nesting follows the heading levels.
(section
  (atx_heading
    heading_content: (_) @name)) @definition.section

(section
  (setext_heading
    heading_content: (_) @name)) @definition.section
