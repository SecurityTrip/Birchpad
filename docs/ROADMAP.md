# Birchpad roadmap

Birchpad first reaches feature parity with Notepad++, then goes beyond it. Phases are ordered by
dependency, not by calendar. Architecture decisions live in [`adr/`](adr).

## Phase 0: foundation

- [x] Workspace, licenses, contribution and security policies
- [x] CI: fmt, clippy, tests on Windows x64/ARM64, Linux, macOS; `cargo-deny`
- [x] Text model (`birchpad-core`): rope, change sets, selections with multiple carets,
      transactions, linear undo history with save-point tracking; unit and property tests
- [x] Layered settings with administrator policies (`birchpad-config`), Windows registry policies
- [x] GPUI spike: virtualized rendering, typing, IME plumbing, multilingual text
      (see [ADR 0002](adr/0002-gpui-ui-framework.md))
- [x] MSI skeleton with a fixed `UpgradeCode`

## Phase 1: MVP notepad

- Tabs: new, open, save, save as, close, close others; modified marker; confirm on close;
  drag and drop files
- Status bar: line/column/selection, length, line ending, encoding, INS/OVR
- Editing: overwrite mode, word movement, smart Home, real tab stops, horizontal scrolling
- Long lines: shape and wrap only the visible part of a line
- Encodings: UTF-8 with and without BOM, UTF-16 LE/BE, legacy code pages (Windows-1251 and
  friends) with detection; "Encode in" (reinterpret) vs "Convert to" (transcode)
- Line endings: detection and conversion
- Simple find/replace, go to line, recent files, zoom, word wrap
- Single instance with command-line compatibility (`-n<line>`, `-multiInst`, `-nosession`, ...)
- Command registry: every action has a string id and serializable arguments
- First installable builds: portable ZIP and installer, manual update check

## Phase 2: Notepad++-level editor

- Syntax highlighting with tree-sitter for ~20 popular languages, auto-indent, brace matching
- Folding; margins for line numbers, bookmarks and folding
- Multi-editing, column selection (Alt+drag, Alt+Shift+arrows), Column Editor
- Smart highlighting, Mark with 5 styles, bookmarks and bookmarked-line operations
- Line operations: duplicate, move, sort, remove duplicates/empty lines, join/split, trim,
  case conversion, comment toggling, tab/space conversion
- Show whitespace and line endings, indentation guides, edge column
- Split view, clone document to the other view, synchronized scrolling
- Sessions and periodic backup of unsaved changes
- External change detection and reload; tail -f monitoring
- Nightly channel with automatic updates

## Phase 3: search and navigation

- Find / Replace / Find in Files / Mark dialog: Normal, Extended (`\n`, `\t`, `\x..`), Regex
  (back-references, look-around); in selection, wrap around, backwards, all open documents
- Search results panel; incremental search
- Function List, Document Map, Document List, Folder as Workspace, Project panels
- Clipboard history, character panel, navigation history

## Phase 4: customization

- Preferences, Style Configurator, themes (importing Notepad++ XML themes)
- Shortcut mapper; macros (record, play N times, save) on top of the command registry
- User Defined Languages (importing Notepad++ UDL files)
- Word and API-file auto-completion, call tips, auto-closing brackets
- Run menu with `$(FULL_CURRENT_PATH)`-style variables
- Hashes, Base64/URL encoding, HEX converter
- Localization (English, Russian first), printing, right-to-left text

## Phase 5: plugins

- Plugin API as WebAssembly components (WIT): commands, documents, events, panels, menus
- User scripting (Lua or Rhai)
- Plugin manager; enterprise allowlist
- Built-in versions of popular Notepad++ plugins: Compare, JSON/XML tools, hex view, spell check

## Phase 6: beyond Notepad++

- LSP client; command palette and fuzzy file finder
- Git: gutter changes, blame, diff
- Integrated terminal
- Syntax-aware selection, sticky scroll
- Multi-gigabyte files via memory mapping
- Remote editing over SSH/SFTP; CSV and JSON viewers
- AI assistance (disabled by the `OnlineFeatures` policy); vim mode; collaborative editing

## Distribution (runs alongside the phases)

| Channel | Built from | Cadence |
|---|---|---|
| Nightly | `main` | nightly, when there are changes |
| Beta | `release/x.y` | every 1–2 weeks |
| Stable | tags on the release branch | every 4–8 weeks |

- Per-user installer with silent delta updates; per-machine MSI (x64, ARM64) for enterprises,
  updated by IT; portable ZIP; winget and Scoop manifests
- Authenticode signing (SignPath Foundation for OSS) plus an ed25519-signed update manifest
- ADMX/ADML templates generated from the policy list in `birchpad-config`
- SBOM and build provenance attestations with every release

See [ADR 0005](adr/0005-distribution-and-updates.md).
