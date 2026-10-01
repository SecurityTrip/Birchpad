# ADR 0003: Text model

- Status: accepted
- Date: 2026-10-01

## Decision

- **Buffer:** `ropey` 2 (currently `2.0.0-beta.1`, the same version `gpui-component` uses).
  Ropey 2 indexes by byte, which is what we want; it is wrapped by `birchpad-core`, so a change of
  rope implementation stays local.
- **Positions are byte offsets.** Characters, UTF-16 code units (IME, LSP) and visual columns
  are derived at the edges. Ropey's `metric_utf16` feature is enabled for O(log n) UTF-16
  conversion.
- **Line breaks:** CRLF, LF and CR (`LineType::LF_CR`), as Scintilla does. A document has a
  line-ending mode (detected from the first line break, like Notepad++) used for new lines;
  mixed line endings in the text are preserved.
- **Edits** are `ChangeSet`s (retain / delete / insert) in canonical form, supporting apply,
  invert, compose and position mapping.
- **Selections** hold one or more ranges, sorted and disjoint, with a primary range. Edits with
  many carets are one transaction and one undo step.
- **Document vs view:** a `Document` owns text and history; carets and scrolling belong to views.
  Several views may show one document; after each change they map their selections through the
  change set.
- **History is linear**, like Notepad++: a new edit after undo discards the redo branch. Each
  undo step has a fresh `RevisionId`, and the saved revision is remembered, so undoing back to the
  saved state clears the modified flag.

## Consequences

- Property tests compare the model with a naive `String` implementation on random multilingual
  text with mixed line endings.
- A tree history (undo branches) could be added later without changing `Document`'s API.
- Collaborative editing would need a CRDT-based model; that is deliberately out of scope until
  Phase 6.
