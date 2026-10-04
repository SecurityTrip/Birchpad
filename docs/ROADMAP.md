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

- [x] Command registry: every action has a string id and serializable arguments; key bindings
      and menus are data ([ADR 0006](adr/0006-commands-and-keymap.md))
- [x] Tabs: new, open (several files), save, save as, save all, close, close all, close others;
      modified marker; Save / Don't Save / Cancel on close and exit; drag and drop files;
      recent files (configurable length); read-only files and `-ro`
- [x] Encodings: detection (BOM, UTF-8, UTF-16 without BOM, legacy code pages), UTF-8 with and
      without BOM, UTF-16 LE/BE, legacy code pages; "Encode in" (reinterpret) vs "Convert to"
      (transcode, undoable); lossless round trips and safe in-place saving
      ([ADR 0007](adr/0007-encodings-and-saving.md))
- [x] Line endings: detection and conversion as one undoable step
- [x] Editing: overwrite mode, word movement and deletion, smart Home, real tab stops,
      insert spaces, horizontal scrolling, double/triple click, blinking caret
- [x] Long lines: shape only the visible part of a row; word wrap through a display map ready
      for folding; documents over 4 MB rewrapped in the background
      ([ADR 0008](adr/0008-editor-view-layout.md))
- [x] Simple find/replace (match case, whole word, wrap around, direction, Replace All as one
      undo step), go to line or offset, zoom (remembered), word wrap
- [x] Status bar like Notepad++'s, with clickable line ending and encoding menus
- [x] Single instance with Notepad++-compatible command line (`-n`, `-c`, `-p`, `-ro`,
      `-multiInst`, `-nosession`; `-l` and other options accepted and ignored)
- [x] First installable builds: release workflow with portable ZIPs (Windows x64/ARM64, Linux
      x64, macOS universal), MSI x64, SBOM and provenance; portable mode; manual update check;
      About ([ADR 0009](adr/0009-releases-portable-mode-and-update-check.md)). The workflow has
      not run on a real tag yet, and the binaries are not signed yet.

What Phase 1 left for later is marked *(from Phase 1)* in the phases below.

## Phase 2: Notepad++-level editor

- [x] Decorations that follow edits (ranges like Scintilla's indicators, line markers like its
      markers); margins for line numbers, bookmarks and folding; clicking the symbol margin
      toggles a bookmark, the line number margin selects lines
      ([ADR 0010](adr/0010-decorations-markers-and-margins.md))
- [x] Syntax highlighting with tree-sitter for 27 languages, incremental and in the
      background; language detection by name, extension, `#!` line and first line; Language
      menu and `-l`; Large File Restriction (`files.large-file-limit-mb`); brace matching with
      Ctrl+B / Ctrl+Alt+B; basic and advanced auto-indent
      ([ADR 0011](adr/0011-syntax-highlighting.md))
- Language injections (scripts in HTML, code blocks and inline formatting in Markdown)
- [x] Folding from the syntax tree (indentation for plain text); Fold All, Unfold All,
      collapse levels 1–8, current level, fold boxes in the margin; carets step over or expand
      collapsed folds ([ADR 0012](adr/0012-folding.md))
- [x] Multi-editing and column selection: rectangles with Alt+drag and Alt+Shift+arrows that
      reach into virtual space; typing, deleting, copying and pasting a rectangle; Multi-select
      Next and All, Undo the Latest Added, Skip Current; Esc back to one caret; Begin/End
      Select in both modes; Column Editor with text or numbers (dec, hex, oct, bin; leading
      zeros or spaces) as one undo step ([ADR 0013](adr/0013-multi-editing-and-columns.md))
- [x] Smart highlighting of the selected word in the visible text (settings as in Notepad++,
      off over the large file limit); Style All Occurrences of Token and Style One Token with
      5 styles, Clear Style, Jump Up/Down, Copy Styled Text; bookmarks (Ctrl+F2, F2,
      Shift+F2) and Cut/Copy/Paste to/Remove bookmarked lines, Remove Unmarked Lines,
      Inverse Bookmark, each one undo step; shifted-digit key bindings on Linux
      ([ADR 0014](adr/0014-smart-highlighting-token-styles-and-bookmarks.md))
- [x] Line operations: duplicate, delete, move, insert blank line, join, split; sort
      (lexicographic, ignoring case, as integers, as decimals with comma or dot, by length;
      ascending and descending), reverse, randomize; remove duplicate, consecutive duplicate
      and empty lines; trim, EOL to space, tab/space conversion; case conversion (upper,
      lower, proper, sentence, invert, random); line and block comments by the language's
      tokens. All in `birchpad_core::ops`, one undo step each, with Notepad++'s keys
- [x] Sorting by the columns of a rectangular selection
- [x] View > Show Symbol: spaces and tabs, line endings (CR, LF, CRLF), Show All Characters,
      indentation guides, wrap symbol; the vertical edge (lines at several columns, or a
      background past one); the current line as a background or a frame; Split Lines at the
      edge or the view width ([ADR 0015](adr/0015-show-symbol-edge-and-current-line.md))
- [x] Split view: main and second views side by side or stacked (Rotate Split View), a
      draggable divider; Move and Clone to Other View, one buffer in two independent views;
      tabs dragged between and within views; Focus on Another View (F8); synchronized vertical
      and horizontal scrolling ([ADR 0016](adr/0016-split-view.md))
- [x] Sessions and periodic backup of unsaved changes, untitled documents included: quitting
      does not ask (quitting from the macOS Dock included *(from Phase 1)*), the next launch
      restores documents, views, carets, folds, bookmarks, languages and encodings, a crash
      loses at most the backup interval; `session.*` settings, the `BackupUnsaved` policy,
      `-nosession`; File > Save Session... and Load Session..., `-openSession`, Notepad++
      `session.xml` import ([ADR 0017](adr/0017-sessions-and-backup.md))
- [x] Files changed by other programs: watched with `notify` and checked when the window comes
      back; reload (silently or after asking) keeping carets, bookmarks and folds on their
      lines; unsaved changes and deleted files asked about; all, current or no files
      (`files.change-detection`); View > Monitoring (tail -f) reading only appended bytes
      ([ADR 0018](adr/0018-file-changes-on-disk.md))
- Nightly channel with automatic updates
- Incremental column index for multi-megabyte lines: typing into a 10 MB line takes about
  70 ms per keystroke because the line's index is rebuilt *(from Phase 1)*
- "Edit anyway" for files that do not decode cleanly; they open read-only with a Reopen with
  Encoding choice for now *(from Phase 1)*
- Keyboard access to the menu bar on Windows and Linux (Alt, mnemonics) *(from Phase 1)*
- [x] Columns count terminal-style cells (East Asian wide characters take two) where
      Notepad++ counts characters: kept for rectangles, the Column Editor and the status bar
      ([ADR 0013](adr/0013-multi-editing-and-columns.md)) *(from Phase 1)*
- Verify IME input (Windows TSF, macOS, IBus/Fcitx) on real systems; Phase 1 tested text
  input through GPUI's input handler but not with a real IME *(from Phase 1)*
- Tests that saving preserves Windows ACLs and alternate data streams *(from Phase 1)*

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
- Character sets that `encoding_rs` lacks: OEM 437/850 and other DOS code pages, EBCDIC
  *(from Phase 1)*
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
- SBOM and build provenance attestations with every release (the release workflow does both)
- Background update checks (`notify`) and automatic updates (`auto`) with the signed manifest;
  Phase 1 checks only on Help > Check for Updates *(from Phase 1)*
- MSI for pre-release versions and for ARM64 *(from Phase 1)*

See [ADR 0005](adr/0005-distribution-and-updates.md).
