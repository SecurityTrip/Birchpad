# ADR 0011: Syntax highlighting with tree-sitter

- Status: accepted
- Date: 2026-10-02

## Context

Notepad++ highlights about 90 languages with Scintilla's hand-written lexers. We need
highlighting that is correct, incremental (typing must not re-lex the file), works for the
languages people edit most, and leaves room for folding, brace matching and, later, structural
features.

## Decision

- **tree-sitter** (0.27) with the official or tree-sitter-grammars crates for 27 languages:
  Bash, Batch, C, C#, C++, CSS, Diff, Go, HTML, INI, Java, JavaScript, JSON, Lua, Makefile,
  Markdown, PHP, PowerShell, Python, Ruby, Rust, SQL, TOML, TypeScript, TSX, XML, YAML. All
  of it lives in the UI-independent crate `birchpad-syntax`.
- **Language detection**: file name (`Makefile`, `Cargo.lock`), extension (Notepad++'s
  conventions, e.g. `.h` is C++), the interpreter of a `#!` line, or a first-line signature
  (`<?xml`, `<!DOCTYPE html`, `<?php`). Language ids follow Notepad++'s `-l` names, so
  `-lcpp file.h` works. The Language menu sets the language by hand; a chosen language survives
  Save As, a detected one is re-detected from the new name.
- **Queries**: each grammar's own highlight query; C++ and TypeScript prepend the C and
  JavaScript queries they build on, as the grammars' configuration does. Precedence: nested
  captures win over the nodes around them, and for the same node a later pattern wins, which
  is how current queries are written (general `(identifier) @variable` first, specific
  patterns after). A query of our own may be appended to fix a grammar's choices (Diff).
- **Highlight names** resolve once, at query compilation, to a fixed vocabulary
  (`keyword`, `function.method`, `markup.heading`, ...) by longest dot-separated prefix plus a
  few synonyms. Themes style that vocabulary with prefix fallback. The default colors follow
  Notepad++'s default style.
- **Incremental parsing in the background.** On every edit the buffer applies `InputEdit`s to
  its tree (microseconds), so highlights of the old tree stay aligned with the text; then a
  background task reparses with the edited tree. Edits made while a parse runs are composed
  into one change set and replayed on its result, which is installed and reparsed again. A
  parse can be cancelled (language change, closing).
- **Highlights are not stored.** Each frame queries the tree for the text shown (merged into a
  few byte ranges; a long line contributes only its visible columns) and caches the result by
  tree version and ranges. A query normally takes about 0.4 ms for a screen, independent of
  file size; it has an 8 ms budget for trees that error recovery made pathologically deep (one
  typo can turn the rest of a file into a 50 000-level expression), and shows what it found by
  then.
- **Large File Restriction**, as in Notepad++: files over `files.large-file-limit-mb`
  (default 20) open as normal text unless a language is chosen by hand.
- **Brace matching** uses the tree: only brackets that are tokens of the language match,
  partners come from the bracket's parent node; plain text counts brackets within 1 MB.

## Measurements (release, Windows x64)

| | 10 000 lines (214 KB) | 400 000 lines (8.6 MB) |
|---|---|---|
| Full parse (background) | 51 ms | 2.1 s |
| Incremental reparse after a keystroke (background) | 3.4 ms | 250 ms |
| Highlight query for a screen (UI thread) | 0.4 ms | 0.4 ms |

## Consequences

- Injections (JavaScript in HTML, code blocks in Markdown, Markdown inline formatting) are not
  highlighted yet.
- Languages without a grammar crate stay normal text until User Defined Languages (phase 4).
- tree-sitter's root node rebuild makes incremental reparsing linear in the number of
  top-level items; it runs off the UI thread, so it costs CPU, not latency.
