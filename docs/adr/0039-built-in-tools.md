# ADR 0039: Built-in versions of popular Notepad++ plugins

- Status: accepted
- Date: 2026-10-12

## Context

Most Notepad++ users install the same few plugins: Compare, JSON Viewer, XML Tools, Hex Editor
and DSpellCheck. Birchpad cannot load them (ADR 0001), and they are too central to wait for
third-party ports.

## Decision

- **They are compiled into Birchpad,** as pure functions in `birchpad-tools` (no window), with
  their commands and menus in the application, under the Plugins menu as in Notepad++.
- **JSON:** checking against RFC 8259 with the place of the first error; formatting one value
  or member per line with the editor's indentation and line ending; putting it on one line.
  Strings and numbers are copied as written, so formatting never changes a value; nesting is
  followed with a stack of its own, up to 10 000 levels.
- **XML:** checking that the document is well formed (`roxmltree`, DTDs allowed); pretty
  printing (an element holding only text stays on one line, its text untouched; whitespace-only
  text between tags goes; text in mixed content is trimmed onto its own line); linearizing
  (whitespace between tags taken out). Tags, comments, CDATA and DOCTYPE are copied as written.
- **Hex:** the document's bytes in its encoding as rows of 16 (offset, bytes, text) in a
  document of their own; edited rows read back into bytes, which replace the original text
  (decoded in its encoding) as one undo step.
- **Compare:** two documents line by line (Myers, line breaks ignored, optionally whitespace
  and case), shown side by side in the split view with added, removed and changed lines marked,
  and commands to go to the next and previous difference.
- **Spell check:** Hunspell dictionaries (`.aff` and `.dic`) read by `spellbook`, from the
  `dictionaries` folder next to the settings, then the system's folders (and Notepad++'s
  DSpellCheck folder on Windows). Words are runs of letters with inner apostrophes; words with
  digits or underscores, in capitals or with a capital inside, single letters and addresses are
  skipped, as they are mostly code and names. Words added by the user are kept in
  `dictionaries/user.txt`.

## Consequences

- No dictionary is bundled: on Windows, spell checking needs one put into the `dictionaries`
  folder (or DSpellCheck's), which Birchpad's message says.
- These tools follow Notepad++'s plugins in what they do, not in every option; more options
  are Phase 6.
