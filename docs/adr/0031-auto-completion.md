# ADR 0031: Auto-completion

- Status: accepted
- Date: 2026-10-11

## Context

Notepad++'s Auto-Completion inserts closing brackets and quotes, lists the document's words and
the language's keywords while typing, and shows a function's parameters (a call tip) after its
opening bracket. Keywords and functions come from API files (`autoCompletion/<language>.xml`)
that users share.

## Decision

- **Pairs** are a setting (`completion.auto-close = "()[]{}\"\"''"`): typing an opening
  character before white space, a closing character or the end of the text inserts its partner;
  a quote does so only where a string can start (not after a word, where it is an apostrophe).
  Typing a closing character over its partner steps over it; Backspace between an empty pair
  deletes both. Every caret at once, as one undo step (`birchpad_core::ops::type_with_pairs`).
  Overwrite mode and selections type as usual.
- **The list** shows from `completion.min-chars` characters of a word (1, as in Notepad++),
  or on Ctrl+Space: the API file's keywords first, with their signatures, then the document's
  words that start with what is typed (ignoring case), each once. It follows typing and
  Backspace, and closes on any other command, a click, or a character that ends a word.
- **Keys go to the list through a key context:** while it shows, the editor's context is
  `Editor Completion`, and bindings for that context (Up, Down, Enter, Tab, Escape) come after
  the editor's own, so they win. A call tip makes it `Editor CallTip`, where Escape closes it.
  The keys stay in the keymap, so the Shortcut Mapper can change them.
- **API files are Notepad++'s, read as they are,** from `autoCompletion/<language id>.xml` next
  to the settings, once per run; Birchpad does not ship any. Their `Environment` gives the
  characters that open and close a call. A call tip shows every overload with its description;
  Ctrl+Shift+Space shows it again inside a call.

## Consequences

- Words come from the whole document on each keystroke while the list shows; that takes a few
  milliseconds per megabyte, and the list shows at most 100 words.
- HTML and XML tags are not closed yet, and an API file applies to its document's language
  only, not to a language embedded in it.
