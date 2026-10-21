# ADR 0049: Hex Editor in place

- Status: accepted
- Date: 2026-10-21
- Changes [ADR 0039](0039-built-in-tools.md): besides the hex dump in a new document, the
  document's bytes are shown and edited in place of its text.

## Context

ADR 0039 shows a document's bytes as a hex dump in another document, applied back with a
command. Notepad++'s Hex Editor plugin switches the document itself to its bytes (View in HEX):
the caret moves over bytes, typing changes them in place, and switching back shows the text at
the same place. Birchpad's document is text, decoded from its file in an encoding, not bytes;
built from the plugin's documentation and behavior, not its code.

## Decision

- **A view of the bytes, not a second model.** View in HEX (`hex.editor`) puts a hex view in
  place of the active tab's text in its pane; the view encodes the text in the document's
  encoding (as saving does) and reads it again when the text changes. Each edit of bytes is
  decoded back and applied to the text as one undo step, the smallest edit that makes the new
  text (`hex::replace_differing`). Text and bytes cannot drift apart.
- **The caret is shared.** The hex view's caret and selection are bytes; every move puts the
  text's caret on the character those bytes belong to, and View in HEX begins at the text's
  caret. For UTF-8 a text position is its byte offset (after a byte order mark); for other
  encodings the view keeps the byte offset of every 256th character and encodes from there.
- **Bytes that are not text wait.** A byte is typed as two hexadecimal digits, over the byte at
  the caret or before it (Insert switches). Bytes that are not text in the encoding (the lead
  byte of a UTF-8 character) are held back, shown where they will go, until the bytes that
  complete a character come; four that never make text are refused. Delete and Backspace
  remove whole characters, or the selection; the byte order mark cannot be edited. The text
  column types characters as their bytes.
- **Rows and groups** come from Preferences > Hex Editor: `hex.columns` (1 to 64, 16 by
  default), `hex.group` and `hex.uppercase`.
- **Find in HEX** finds hexadecimal bytes (`4A 6F`) or text in quotes, encoded in the
  document's encoding, going round past the end; the bytes found are selected.
- The hex dump in a new document stays, as Hex Dump in New Document.

## Consequences

- Documents up to 64 MiB of bytes are shown in hex.
- Bytes that are not text in the document's encoding cannot be made: a file of arbitrary binary
  data is opened as text in an encoding that maps every byte (a single-byte code page) to edit
  every byte.
