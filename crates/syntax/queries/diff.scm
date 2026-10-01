; Birchpad: name diff lines by meaning (the grammar's own query borrows `@string` and
; `@keyword` for their colors in `tree-sitter highlight`). Appended after the grammar's query,
; so these patterns win.
[(addition) (new_file)] @diff.plus
[(deletion) (old_file)] @diff.minus
(location) @diff.delta
