# ADR 0011: Syntax highlighting with tree-sitter

- Status: accepted
- Date: 2026-10-02

## Context

Notepad++ highlights about 90 languages with Scintilla's hand-written lexers. We need
highlighting that is correct, incremental (typing must not re-lex the file), works for the
languages people edit most, and leaves room for folding, brace matching and, later, structural
features.

## Decision

- **tree-sitter** (0.27) with grammar crates from crates.io for 50 languages: Assembly, Bash,
  Batch, C, C#, C++, CMake, CSS, Dart, Diff, Dockerfile, Elixir, Erlang, Go, GraphQL, Groovy
  (and Gradle), Haskell, HCL (Terraform), HTML, INI, Java, JavaScript, JSON, Kotlin, Lua,
  Makefile, Markdown, nginx, Nix, Objective-C, Pascal, PHP, PowerShell, Properties, Protocol
  Buffers, Python, R, Ruby, Rust, Scala, Solidity, SQL, Svelte, Swift, TOML, TypeScript, TSX,
  XML, YAML, Zig. Three more grammars only appear inside others: Markdown's inline syntax,
  regular expressions and JSDoc. All of it lives in the UI-independent crate
  `birchpad-syntax`. A grammar costs about a sixth of its generated `parser.c` in the binary
  (Kotlin 3.4 MB, Swift 4.3 MB); grammars much larger than that for few users (OCaml, F#,
  Julia, Verilog, Fortran) wait for loadable grammars. Perl's crate depends on another
  tree-sitter version and is left out for now.
- **Language detection**: file name (`Makefile`, `Cargo.lock`), extension (Notepad++'s
  conventions, e.g. `.h` is C++), the interpreter of a `#!` line, or a first-line signature
  (`<?xml`, `<!DOCTYPE html`, `<?php`). Language ids follow Notepad++'s `-l` names, so
  `-lcpp file.h` works. The Language menu sets the language by hand; a chosen language survives
  Save As, a detected one is re-detected from the new name.
- **Queries**: each grammar's own highlight query; C++ and TypeScript prepend the C and
  JavaScript queries they build on, as the grammars' configuration does. Precedence: nested
  captures win over the nodes around them, and for the same node a later pattern wins, which
  is how current queries are written (general `(identifier) @variable` first, specific
  patterns after). A query of our own may be appended to fix a grammar's choices (Diff), or
  replace a missing one (Groovy, HCL, GraphQL). Queries that grammar crates ship without
  exporting are copied into `crates/syntax/queries` with their licenses (`THIRD-PARTY.md`).
  Many queries are written for Neovim: its `#lua-match?` patterns are rewritten as regular
  expressions, and patterns with predicates nothing here evaluates (`#has-ancestor?`) are
  dropped rather than matching too much.
- **Embedded languages (injections)**, as Neovim and Zed do them: a language's injection query
  finds the text written in another (`<script>` and `<style>` in HTML and Svelte, fenced code
  blocks, inline formatting and front matter in Markdown, the HTML around PHP code, tagged
  templates such as ``css`...` `` and regular expressions and JSDoc comments in JavaScript and
  TypeScript). Each embedded part becomes a *layer*: a tree of its language, parsed over just
  those ranges of the document (all the parts at once for PHP's HTML), with layers of its own
  (Markdown, then its inline syntax, then HTML in it, then a script). Layers are found and
  parsed after the main tree, in the same background parse. A layer whose text no edit
  touched keeps its tree and its own layers without running any query; one that changed is
  reparsed incrementally from its old tree. Edits shift layers along with the main tree until
  then. Highlights paint the host's captures first and each deeper layer's over them, clipped
  to the layer's ranges, so a code block keeps Markdown's code color where Rust's query has
  nothing to say. A code block's language is matched by id, alias (`js`, `c++`, `shell`), name
  or extension; blocks in a language without a grammar stay plain code. Folding and brace
  matching use the main tree only.
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

Markdown with embedded languages (`cargo run --release -p birchpad-syntax --example bench`):
10 000 lines with a heading, a paragraph and a Rust code block every eight lines, 3 750
layers.

| | 10 000 lines of Markdown (140 KB) |
|---|---|
| Full parse with all layers (background) | 200 ms |
| Following a keystroke: main tree and the layers after it (UI thread) | 1.7 ms |
| Reparse after a keystroke in a paragraph (background; one layer parsed again) | 95 ms |
| Highlight query for a screen, all layers (UI thread) | 0.5 ms |

The reparse is mostly the Markdown grammar itself (about 60 ms, as before layers) and the
injection query over the whole document (30 ms).

## Consequences

- Languages without a grammar crate stay normal text until User Defined Languages (phase 4).
- tree-sitter's root node rebuild makes incremental reparsing linear in the number of
  top-level items; it runs off the UI thread, so it costs CPU, not latency.
