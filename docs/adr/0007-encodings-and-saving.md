# ADR 0007: Encodings, reinterpretation and saving files

- Status: accepted
- Date: 2026-10-01

## Context

Notepad++ users open files in UTF-8 (with and without BOM), UTF-16 and legacy code pages, often
without knowing which. An editor that guesses wrong must let the user correct the guess without
damaging the file, and an editor that saves must never change bytes the user did not edit. The
crate `birchpad-io` implements both, without UI dependencies.

## Decision

### Detection

In order, first match wins:

1. Byte order mark: UTF-8, UTF-16 LE, UTF-16 BE.
2. Valid UTF-8 — unless the first 64 KB show the UTF-16 zero-byte pattern (at least 40% of byte
   pairs with a zero in one half, under 5% in the other). ASCII text in UTF-16 is technically
   valid UTF-8 full of NULs, so the pattern takes precedence; real UTF-8 text never has it.
3. UTF-16 LE/BE without BOM by that pattern.
4. chardetng's guess among legacy encodings, given up to 4 MB. The system ANSI code page serves
   as chardetng's locale hint (in place of the top-level domain it was designed for). The guess
   is used only with at least 16 non-ASCII bytes and if it decodes without errors; KOI8-U is
   reported as KOI8-R when none of the Ukrainian-only bytes occur.
5. The ANSI code page: `GetACP()` on Windows; elsewhere Windows-1252, or `files.ansi-encoding`
   from the settings.

### Lossless by construction

- Text is decoded without silent replacement. A decoding with malformed bytes, or one whose
  text would not encode back to the same bytes (multi-byte legacy encodings map some byte
  sequences to the same character; checked by re-encoding on open), is a `DecodeProblem`. Such a
  document opens **read-only** with a warning naming the first offending byte offset and offers
  to reopen it in another encoding.
- Encoding never substitutes characters. If the text contains characters the target encoding
  lacks, saving is refused with the characters and their positions; "Convert to" warns as soon
  as the document contains such characters.
- The encoding, the BOM and the line-ending mode are kept from the file. Mixed line breaks in
  the text stay as they are; the mode only applies to new lines. Property and fixture tests
  check that open → save is byte-identical for every encoding × BOM × line-ending combination,
  and that an edit changes only the edited bytes.

### "Encode in" versus "Convert to"

- **Convert to X** changes how the document will be saved: a transaction with no text change
  and a new format. It marks the document modified and is undone with Ctrl+Z like any edit; the
  document's format is part of the undo history (`birchpad-core`'s `Format`). EOL conversion
  works the same way, with the line-break edits and the new mode in one transaction.
- **Encode in X** reinterprets the file's bytes. We **re-read the file from disk** instead of
  keeping the original bytes in memory: keeping them would double memory for every open file
  (200 MB for a 100 MB file) to serve a rare command. Re-reading is what Notepad++ does too. The
  consequences:
  - it replaces the document and clears its undo history, like a reload;
  - if the document has unsaved changes, the user confirms that they will be lost;
  - if the file changed on disk, the current content is reinterpreted;
  - for an untitled document there are no bytes: "Encode in" acts like "Convert to".
  If the chosen BOM differs from the file's, the BOM change is applied as an undoable format
  change after the reload.

### Saving: in place, with a recovery copy

Options considered:

- **Temporary file and atomic rename** (`rename`, `ReplaceFileW`). Survives a crash mid-write,
  but replaces the file's identity: hard links break, ownership changes when someone else owns
  the file, ACLs, extended attributes and alternate data streams (including Windows'
  Zone.Identifier) are lost unless copied one by one, file watchers and single-file Docker bind
  mounts lose the file, and the directory must be writable.
- **In-place write.** Keeps everything above; Notepad++, VS Code and Sublime Text (by default)
  do this. Risk: a crash or power loss during the write leaves a partly written file.

Decision: **write in place, after securing the new content.**

1. Encode the whole document in memory first; unencodable text fails before the disk is touched.
2. Follow symbolic links to the real file, so the link stays a link.
3. Refuse read-only files (read-only attribute or no write permission) instead of changing
   their permissions; the user can use Save As.
4. For an existing file, write the new content to a recovery copy in the user data directory
   (`recovery/`) and flush it.
5. Grow the file to the new size first (on most file systems a full disk fails here, before any
   byte is overwritten), write from the start, truncate to the new length, flush (`fsync`).
6. Remove the recovery copy. If the write failed part way, the error names the recovery copy;
   copies left by a crash are listed on the next start.

New files are created directly; a failed write removes the incomplete file.

### Limits

Files are read whole into memory, in the background with a progress indicator; files over
2 GiB are refused with a message. A 100 MB UTF-8 file loads in about 0.35 s (release build,
read + detect + decode + rope). Memory-mapped huge files are a phase 6 item.

## Consequences

- The save path needs no platform-specific code and no `unsafe`; its behavior with symlinks,
  hard links and permissions is tested on Unix. ACL and alternate data stream preservation on
  Windows follows from never replacing the file, but is not covered by automated tests yet.
- Every save writes the content twice (recovery copy, then the file). For a 100 MB file that is
  an extra ~100 MB of I/O; acceptable for the safety it buys.
- encoding_rs implements the WHATWG encodings: OEM code pages other than 866 (437, 850, ...)
  and EBCDIC are not available yet.
- A document with a decoding problem cannot be edited, even to fix it. Reopening in the right
  encoding is the way out; an explicit "edit anyway, invalid bytes become U+FFFD" may come later.
