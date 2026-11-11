# Birchpad roadmap

English · [Русский](ROADMAP.ru.md)

Birchpad first reaches feature parity with Notepad++ (Phases 1 to 6), then goes beyond it
(Phase 7); closing Phase 7 releases Birchpad 1.0.0. On the way, each release from 0.2.0 on
closes a group of steps, named in this roadmap's headings (Release 0.3.0, ...). Phases are
ordered by dependency, not by calendar. Architecture decisions live in [`adr/`](adr).

## Phase 0: foundation

- [x] Workspace, licenses, contribution and security policies
- [x] CI: fmt, clippy, tests on Windows x64/ARM64, Linux (Ubuntu, Debian 12 and 13), macOS;
      `cargo-deny`
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
      About ([ADR 0009](adr/0009-releases-portable-mode-and-update-check.md)). First released
      as 0.1.0; the binaries are not signed yet.

What Phase 1 left for later is marked *(from Phase 1)* in the phases below.

## Phase 2: Notepad++-level editor

- [x] Decorations that follow edits (ranges like Scintilla's indicators, line markers like its
      markers); margins for line numbers, bookmarks and folding; clicking the symbol margin
      toggles a bookmark, the line number margin selects lines
      ([ADR 0010](adr/0010-decorations-markers-and-margins.md))
- [x] Syntax highlighting with tree-sitter for 50 languages, incremental and in the
      background; language detection by name, extension, `#!` line and first line; Language
      menu and `-l`; Large File Restriction (`files.large-file-limit-mb`); brace matching with
      Ctrl+B / Ctrl+Alt+B; basic and advanced auto-indent
      ([ADR 0011](adr/0011-syntax-highlighting.md))
- [x] Embedded languages: scripts and styles in HTML and Svelte, code blocks, inline
      formatting and front matter in Markdown, HTML around PHP, tagged templates, regular
      expressions and JSDoc in JavaScript and TypeScript, each a tree of its own, reparsed only
      when its text changes ([ADR 0011](adr/0011-syntax-highlighting.md))
- [x] 23 more languages: Kotlin, Swift, Dart, Scala, Groovy/Gradle, R, Objective-C, Haskell,
      Elixir, Erlang, Zig, Pascal, Assembly, Solidity, Dockerfile, CMake, Protocol Buffers,
      GraphQL, HCL/Terraform, Nix, Properties, nginx, Svelte
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
- [x] Nightly channel with automatic updates: nightly builds of `main` when it changed, an
      ed25519-signed update manifest with rollback, freeze and downgrade protection, a per-user
      installer for Windows (Velopack) that updates itself, background checks (`notify`) and
      automatic updates (`auto`) ([ADR 0020](adr/0020-signed-updates-and-nightly-builds.md))
- [x] Long lines keep their layout through edits: the column index and the rows of a wrapped
      line are updated around the edit; typing into a 10 MB line takes 1.6 ms per keystroke
      (6.7 ms with word wrap), down from 49 ms (168 ms) ([ADR 0008](adr/0008-editor-view-layout.md))
      *(from Phase 1)*
- [x] Edit Anyway for files that do not decode exactly, after a list of every place where saving
      changes the file's bytes ([ADR 0007](adr/0007-encodings-and-saving.md)) *(from Phase 1)*
- [x] The menu bar from the keyboard on Windows and Linux: Alt or F10, Alt+letter, mnemonics
      in menus, arrows into submenus ([ADR 0019](adr/0019-menu-bar-keyboard-access.md))
      *(from Phase 1)*
- [x] Columns count terminal-style cells (East Asian wide characters take two) where
      Notepad++ counts characters: kept for rectangles, the Column Editor and the status bar
      ([ADR 0013](adr/0013-multi-editing-and-columns.md)) *(from Phase 1)*
- Verify IME input (Windows TSF, macOS, IBus/Fcitx) on real systems; Phase 1 tested text
  input through GPUI's input handler but not with a real IME *(from Phase 1)*
- [x] Tests that saving preserves Windows ACLs, alternate data streams, the creation time and
      links *(from Phase 1)*

## Phase 3: search and navigation

- [x] Find / Replace / Mark: Normal, Extended (`\n`, `\t`, `\x..`), Regex (back-references,
      look-around); in selection, wrap around, backwards, all open documents; Count, Mark All
      with Bookmark line and Purge, Copy Marked Text
      ([ADR 0022](adr/0022-search-modes-and-the-find-panel.md))
- [x] Find in Files: Notepad++'s filters (`*.rs !\target !+\node_modules`), sub-folders and
      hidden folders, Follow current doc.; open documents searched and replaced in as they are,
      other files written back in their own encoding; Stop
      ([ADR 0024](adr/0024-find-in-files.md))
- [x] Search results panel: Find All in Current Document and in All Opened Documents, searches
      stacked and foldable, F4 / Shift+F4 through the results, F7 to show or hide it; Delete
      removes the selected result, and a right-click menu copies, folds and clears
      ([ADR 0022](adr/0022-search-modes-and-the-find-panel.md))
- [x] Incremental search (Ctrl+Alt+I): the match grows as the text is typed, Enter and
      Shift+Enter step through it, Highlight all marks and counts every match
      ([ADR 0022](adr/0022-search-modes-and-the-find-panel.md))
- [x] Side panels in docks beside the documents, with tabs, remembered for the next start
      ([ADR 0026](adr/0026-side-panels.md)):
  - [x] Document List
  - [x] Function List, from tree-sitter tags queries for 28 languages
  - [x] Document Map
  - [x] Folder as Workspace
  - [x] Project Panels 1 to 3, with Notepad++'s `.workspace` files
- [x] Clipboard History and the Character Panel
      ([ADR 0026](adr/0026-side-panels.md))
- [x] Navigation history: Go Back and Go Forward through the places the caret jumped from,
      across documents, reopening closed files ([ADR 0025](adr/0025-navigation-history.md))
- [x] Issues reported on 0.1.2: a middle click closes a tab
      ([#9](https://github.com/SecurityTrip/Birchpad/issues/9)), JSON keys colored as keys
      ([#10](https://github.com/SecurityTrip/Birchpad/issues/10)); found on the way, the close
      button of a tab closed unsaved documents without asking, and TOML keys had the color of
      table names
- [x] Change history in the margin, orange and green as in Notepad++
      ([#11](https://github.com/SecurityTrip/Birchpad/issues/11),
      [ADR 0023](adr/0023-change-history.md)); split view by dragging a tab to the side of the
      text ([#12](https://github.com/SecurityTrip/Birchpad/issues/12),
      [ADR 0016](adr/0016-split-view.md))
- [x] Tests for every scenario. Birchpad has no testers, so its tests are its quality assurance:
  - [x] A coverage report in CI (`cargo llvm-cov`) for each crate, with a floor that only rises
        (`.github/coverage-floors.toml`), and the untested code it shows covered first
  - [x] Every command of the registry run in every kind of document (empty, one long line,
        multiple carets, a rectangular selection, read-only, word wrap, both views, a large
        file), checking that it neither panics nor breaks the document
  - [x] The mouse in GUI tests, not only the keyboard: tabs (click, middle click, close
        button, drag), the menu bar, the status bar's menus, the find and search results
        panels, the side panels, dialogs
  - [x] Property tests (`proptest`) of the text model: edits, selections, undo and redo,
        decorations following edits, and search checked against a plain reference
  - [x] Fuzzing (`cargo fuzz`) of everything that reads outside input: decoders, settings, the
        state and session files, Notepad++'s `session.xml` and `.workspace` files, the keymap,
        the command line and its hand-off to a running instance, the update manifest; the same
        checks run over seeds and their mutants in `cargo test`
  - [x] Highlighting: a real-world file for each language, and a check that every capture of a
        query that the theme colors shows up in its sample, so that a capture hidden by another
        pattern (JSON's keys) or by a misplaced capture (TOML's keys) fails; every sample parses
        without errors ([ADR 0011](adr/0011-syntax-highlighting.md))
  - [x] End-to-end tests of the built application on Windows, Linux and macOS: start, open,
        edit, save, quit and restore the session, in a real window driven by a script
  - [x] Every bug fixed with a test that fails without the fix
        ([CONTRIBUTING.md](../CONTRIBUTING.md#tests))
  - [x] A short manual checklist before each release for what automation cannot see: input
        methods, screen readers, high DPI, the installers and updates
        ([release-checklist.md](release-checklist.md))

## Phase 4: customization

- [x] Color themes for the editor and the whole window: built-in Default and Dark, theme files
      in TOML, Notepad++ XML themes read as they are; Settings > Theme and Import Style
      Themes... ([ADR 0027](adr/0027-themes.md))
- [x] Style Configurator: a theme's editor, interface and highlighting colors, for all
      languages or one, shown as they change and saved as a theme file
      ([ADR 0027](adr/0027-themes.md))
- [x] Preferences: the settings by section, each change written to `settings.toml` keeping its
      comments and applied at once; settings set by policy shown but locked
      ([ADR 0028](adr/0028-preferences.md))
- [x] Shortcut Mapper: add, remove and reset the shortcuts of every command, written to
      `keymap.toml` and applied at once ([ADR 0029](adr/0029-shortcut-mapper.md))
- [x] Macros: record commands and typing, play back once, N times or until the end of the
      file, save by name in `macros.toml` ([ADR 0030](adr/0030-macros.md))
- [x] User Defined Languages: Notepad++ UDL files read as they are, colored by a small lexer,
      chosen by extension or in the Language menu, imported ([ADR 0032](adr/0032-user-defined-languages.md))
- [x] Auto-completion: brackets and quotes closed as typed, the document's words and Notepad++
      API files' keywords while typing, call tips with every overload
      ([ADR 0031](adr/0031-auto-completion.md))
- [x] Run menu: command lines with Notepad++'s variables (`$(FULL_CURRENT_PATH)`,
      `$(CURRENT_WORD)`, `$(SYS.NAME)`, ...), saved by name in `run.toml`; web addresses open in
      the browser
- [x] Tools: MD5, SHA-1, SHA-256 and SHA-512 of the selection (into the clipboard) or of files
      (into a new document); Base64 and URL encoding and decoding, ASCII to HEX and back, of
      every selection as one undo step
- [x] Character sets that `encoding_rs` lacks: OEM 437, 737, 775, 850, 852, 855, 857, 858,
      860–865, 869, and EBCDIC 037, 500, 1026, 1140, from tables of Birchpad's own behind the
      same decoder and encoder as encoding_rs's *(from Phase 1)*
- [x] Localization: English and Russian, the interface's language from the system or
      `appearance.language` ([ADR 0033](adr/0033-localization.md))
- [x] Printing: the document as an HTML page in its colors with line numbers, printed
      through the browser's print dialog, or exported ([ADR 0034](adr/0034-printing.md))
- [x] Right-to-left text: rows with Hebrew or Arabic reordered by the Unicode Bidirectional
      Algorithm, carets and clicks mapped both ways; Edit > Text Direction RTL and LTR
      ([ADR 0035](adr/0035-right-to-left-text.md))

## Phase 5: plugins

- [x] Plugin runtime: WebAssembly components of the `birchpad:plugin` WIT world in Wasmtime,
      working on a copy of the document (one undo step a call), without files or network, with
      fuel and memory limits ([ADR 0036](adr/0036-webassembly-plugins.md))
- [x] Plugins in the window: the Plugins menu with each plugin's commands, events, the plugin
      panel; compiled in the background at start and on Reload Plugins
- [x] User scripting in Rhai with the plugins' API and an operation limit
      ([ADR 0037](adr/0037-rhai-scripts.md))
- [x] Scripts in the Plugins menu, from the `scripts` folder; the plugin panel
- [x] Plugins folder: `plugin.toml`, installing, updating, removing and turning plugins off;
      the `PluginInstall` / `AllowedPlugins` allowlist and the `UserScripts` policy
      ([ADR 0038](adr/0038-plugin-manager-and-allowlist.md))
- [x] Plugins Admin: the installed plugins, install from a file, remove, turn on and off
- [x] Built-in tools without a window (`birchpad-tools`): JSON and XML checking, formatting
      and linearizing, hex dumps, comparing texts, Hunspell spell checking
      ([ADR 0039](adr/0039-built-in-tools.md))
- [x] The Plugins menu; JSON Viewer and XML Tools in it: format, compress or linearize, and
      check the selection or the document ([ADR 0039](adr/0039-built-in-tools.md))
- [x] Hex Editor: the document's bytes as a hex dump, edited and applied back
- [x] Compare: two documents side by side with their differences marked, next and previous
      difference, ignoring spaces or case
- [x] Spell checking as DSpellCheck: misspelled words underlined, next and previous, suggestions,
      the user's words, the dictionary's language
- [x] Fuzzing `plugin.toml`, JSON, XML and hex dumps
- [x] The whole interface translated: every text by key from YAML locales (`en.yaml`,
      `ru.yaml`) with named arguments and plural forms; command titles, messages, errors,
      questions, the status bar and the find panel in Russian; errors of the crates without a
      window translated from their English ([ADR 0033](adr/0033-localization.md))

## Phase 6: full parity with Notepad++

What Phases 1 to 4 built differently from Notepad++, or left out, is brought in line here; what
Phase 5 leaves for later is added once it is done. Birchpad goes beyond Notepad++ only after
this phase. It ships in releases: what is done by 2026-10-14 (6.1 to 6.21) as 0.2.0, with the
issues users opened on GitHub ([Release 0.2.0](#release-020)), the fixes of 0.2.5
([Release 0.2.5](#release-025)), then 0.3.0 (tests), 0.4.0 (languages and text), 0.5.0
(printing, tools and help) and 0.6.0 (plugins, documentation in Russian, the last comparison
with Notepad++), which closes the phase.

Every step follows the clean-room rules of [CONTRIBUTING.md](../CONTRIBUTING.md#clean-room).
The steps are ordered by dependency; each one lands as its own commit with its tests, docs and
a `CHANGELOG.md` entry. A step that adds a feature with options adds them to Preferences too;
step 6.11 brings in the sections no feature brings.

### Release 0.2.0

Steps 6.1 to 6.21, and every open issue, one step each: those steps 6.1 to 6.21 answer are
done and closed with the release.

- [x] 0.2.1 [#25](https://github.com/SecurityTrip/Birchpad/issues/25) More ways to close tabs at once: Close All to the Left and to the
      Right (6.1)
- [x] 0.2.2 [#26](https://github.com/SecurityTrip/Birchpad/issues/26) Pinned tabs, at the start of the tab bar (6.1)
- [x] 0.2.3 [#27](https://github.com/SecurityTrip/Birchpad/issues/27) The tab's context menu, with closing groups of tabs and pinning
      (6.1)
- [x] 0.2.4 [#28](https://github.com/SecurityTrip/Birchpad/issues/28) A new document on a double click on the tab bar's empty part (6.1)
- [x] 0.2.5 [#30](https://github.com/SecurityTrip/Birchpad/issues/30) The text's context menu with the main commands (6.7)
- [x] 0.2.6 [#29](https://github.com/SecurityTrip/Birchpad/issues/29) Reloading a tab from disk: File > Reload from Disk (6.3) on Ctrl+R by
      default (Cmd+R on macOS)
- [x] 0.2.7 [#24](https://github.com/SecurityTrip/Birchpad/issues/24) "Edit with Birchpad" in the Windows Explorer's context menu of files,
      as Notepad++'s: added by the installers (the per-user one for the user, at install and at
      every update unless the user took it away; the MSI for the machine) and removed with them; for the portable build, a switch in Preferences that adds
      it for the user or takes it away
- [x] 0.2.8 [#32](https://github.com/SecurityTrip/Birchpad/issues/32) Birchpad as the default
      editor of text files (.txt, .json, .xml, .md, .log and others), offered at startup until
      set up or declined, and in Settings: Windows registers it for the user and opens Default
      apps for the choice, which no program may make; Linux makes it the default with
      `xdg-mime`; on macOS the app declares its document types for Finder's Change All
- [x] 0.2.9 Release 0.2.0: the workspace version (0.2.0, so that nightly builds are its
      pre-releases), `CHANGELOG.md` and this roadmap updated; the
      [release checklist](release-checklist.md) gone through; the answered issues closed

### Release 0.2.5

The issues users opened after 0.2.0 and the dependency updates, one commit each.

- [x] [#34](https://github.com/SecurityTrip/Birchpad/issues/34) Dragging a selection past the
      left or right edge of the text scrolls it sideways
- [x] [#33](https://github.com/SecurityTrip/Birchpad/issues/33) In a low window the menus stay
      below the menu bar and scroll instead of covering it
- [x] Dependabot's updates (#35 to #38) and wasmtime 49.0.3 for RUSTSEC-2026-0353 to 0364
- [x] What CI found on the way: a byte order mark before the XML declaration (the xml fuzz
      target); the fuzz job runs its targets in parallel
- [x] Release 0.2.5: the workspace version, `CHANGELOG.md` and this roadmap updated; the
      answered issues closed

### Window, tabs and files

- [x] 6.1 Tabs: tab colors, pinned tabs, multi-line and vertical tabs; the tab bar's context
      menu and options (double-click to close, lock, reduce); the document peeker on a tab;
      the Ctrl+Tab document switcher; go to the 1st-9th tab, move the tab forward or backward;
      Close Multiple Documents (to the left, to the right, unchanged, all but pinned)
      ([ADR 0040](adr/0040-tab-bar.md))
- [x] 6.2 The Window menu and its Windows dialog (sort, activate, save, close)
- [x] 6.3 File menu: Open Containing Folder (in the file manager, a terminal, Folder as
      Workspace), Open in Default Viewer, Reload from Disk, Save a Copy As, Rename, Move to
      Recycle Bin (Notepad++'s way of deleting a file), Restore Recent Closed File, Open All
      Recent Files; Copy full path / file name / folder
- [x] 6.4 Instances and the window: Move/Clone to a new instance, Open in a new instance;
      multi-instance in Preferences; Always on Top, Full Screen, Post-It, Distraction Free mode
- [x] 6.5 The toolbar: small and large icons (Lucide's line icons stand for Notepad++'s
      Fluent ones, which are Microsoft's); customizable in `toolbar.buttons`

### Editing, search and view

- [x] 6.6 Edit menu: Insert date and time, Set Read-Only and Clear Read-Only Flag, Paste Special
      (binary content, HTML, RTF), On Selection (open the file named, its folder, search the
      Internet with a chosen engine), Begin/End Select in Column Mode, Remove Consecutive
      Duplicate Lines, Increase/Decrease Line Indent as menu commands
- [x] 6.7 The text's context menu, editable as Notepad++'s Edit Popup ContextMenu
- [x] 6.8 Search menu: Find (Volatile) Next/Previous, Select and Find Previous, Find characters
      in range, Change History (next and previous change, clear all changes), Find in Projects;
      the search results' context menu as Notepad++'s (copy lines or paths, open all, wrap long
      lines, purge for every search)
- [x] 6.9 View menu: View Current File in a browser, Hide Lines, Summary (counts of
      characters, words, lines), Collapse/Expand by level from the menu
- [x] 6.10 Clickable links: addresses underlined and opened with Ctrl+click or a double click
      (Preferences > Cloud & Link); Ctrl+double click selection between delimiters

### Customization

- [x] 6.11 Preferences: the remaining sections and options of Notepad++'s dialog (tab bar,
      delimiters, search engine, MISC and the rest; changes apply at once, as most of
      Notepad++'s do); Language: tab size and spaces per language; new document defaults
      (encoding, line ending, language); backup copies on save (simple or verbose); default
      folder; settings in a folder of one's choice (cloud); "remember inaccessible files" in
      sessions. Minimizing to the system tray waits for a tray in GPUI (Phase 7)
- [x] 6.12 Themes and Style Configurator: fonts (family, size, bold/italic per style) in
      themes, the Style Configurator and Notepad++ theme import; saving changes to a Notepad++
      theme back to its XML; a closer match from Scintilla's style names to highlights, with a
      per-language table where names differ. The theme's font is one family and size for the
      whole text, Notepad++'s Default Style: the editor keeps a grid of cells, so a style's own
      family or size is not used
- [x] 6.13 Notepad++'s separate Dark Mode for the interface, independent of the editor theme
- [x] 6.14 Shortcut Mapper: key sequences (`ctrl-k ctrl-c`) recorded in the dialog; bindings
      limited to platforms shown and editable; command titles translated; the Macros and Run
      tabs (Modify Shortcut / Delete Macro or Command)
- [x] 6.15 Macros: Notepad++'s built-in "Trim Trailing Space and Save"; recording what dialogs
      a command opens (Find, Replace) with their fields, as Notepad++ records search parameters
- [x] 6.16 Run: Notepad++'s own Run menu entries as built-in items, not only saved command lines

### Splitting the application crate

`birchpad` holds more than half of the workspace's code (about 48,000 of 83,000 lines): its
window and dialogs, but also logic that needs no window, and the editor view, which every change
to a panel or a dialog rebuilds. These steps move both out before the remaining features land;
behavior does not change, and the tests move with the code.

- [x] 6.17 `birchpad-i18n`: the localization without a window (the YAML locales and their
      parsing, named arguments, plural forms, the current language, `tr_error`) as a crate of
      its own, so that the crates without a window translate their messages where they make
      them, not the application from their English
- [x] 6.18 The logic without a window left in `birchpad` moved to the crates that hold its kind:
      Find in Files' reading of one file (binary files skipped), its search and replace and the
      result lines (`line_hits`), Go To's positions to `birchpad-core` and `birchpad-io`; the
      bidirectional row (`BidiRow`) to `birchpad-view`; the date and time formats, Export to
      HTML (in the theme's colors, not GPUI's) and the hex view's replacement to
      `birchpad-tools`; the session's names of line endings, languages and backup copies to
      `birchpad-config`; Run's command line and address check to `birchpad-commands`
- [x] 6.19 The command registry without the workspace: handlers registered for a kind of target
      rather than for `Workspace` and `EditorView` by name; the dialogs' buttons and error
      reporting (`dialog_close`, `dialog_action`, `report_error`) and the settings the editor
      reads from `AppState` in a small shared crate, so that the editor no longer reaches into
      the application
- [x] 6.20 `birchpad-editor`: the editor view (`editor/`), the buffer and the active theme as a
      GPUI crate that `birchpad` builds on, with its own tests (`cargo test -p
      birchpad-editor`); find options, spelling and user languages reach it through what it
      exposes, not the application's globals. An ADR records the crates' layout

### Release 0.3.0: tests

Step 6.21 shipped with 0.2.0. Measured on Windows on 2026-10-13, the tests run 93.2% of the lines:
92 to 99.5% in the crates without a window, 89.4% in `birchpad`, where Preferences, the column
editor and the Tools menu stay under 65%. The gaps are mostly the negative scenarios and limits
CONTRIBUTING.md asks for (failed reads and writes, refused input, interrupted downloads). Each step
covers the positive scenario, the negative ones and the boundary values, and raises the crates'
floors in `.github/coverage-floors.toml` with its tests.

- [x] 0.3.1 Faster tests, first, since the steps after it add many: `cargo-nextest` (each test
      in a process of its own, run side by side) locally and in CI; the slowest tests of the
      window found and made faster (waits for redraws, sleeps while bounds settle, dialogs'
      animations); what a full run takes locally and per CI job measured before and after.
      Nothing is run less: every job still runs on every push. The sweeps of every command
      (`command_sweep_tests.rs`, 100 to 210 s each) spent their time drawing frames of
      animations in debug code: the test window now turns motion off (`set_reduce_motion`),
      debug builds optimize the dependencies (`opt-level = 1`), and a click waits only while
      what it clicks still moves. A full run of the 1092 tests on Windows with 12 cores, built:
      `cargo test` 393 s, `cargo nextest` 235 s, both changes 83 to 101 s (without the
      optimized dependencies 268 s); the sweeps take 33 to 93 s. A flaky session test (a file
      a virus scanner still holds) retries its writes too. In CI (minutes per job, the last
      green run of the old workflow, then the first of the new ones, with every cache cold):
      tests on Windows 19, 26 (and Clippy beside them, 12), ARM64 14, 18, macOS 11, 16,
      Ubuntu 7, 16, Debian trixie 20, 15, bookworm 20, 19, coverage 9, 11, fuzzing 13, 7; the
      runs with warm caches are the measure
- [x] CI by platform: four workflows that start together, each job side by side: Linux
      (Clippy, tests and end-to-end tests on Ubuntu, tests in Debian trixie and bookworm),
      Windows (Clippy and tests on x64 and ARM64, end-to-end tests), macOS (the same three) and
      Checks (formatting, cargo-deny, coverage, fuzzing). Clippy has jobs of its own beside the
      tests; CI builds keep only line tables as debug information; the Debian jobs install
      their libraries with the same action as Ubuntu's (`.github/actions/linux-deps`) and run
      nextest. Nothing is run less: the ARM64 Clippy and every test run as before
- [x] 6.21 Dialogs and menus driven in a test window, every control: Preferences (62.8% of lines
      run), the Shortcut Mapper (76.9%), the column editor (64.1%), the path dialog, the Tools
      menu (62.4%), the banners, the menus and the window modes; the command catalog of
      `birchpad-commands` (63.2%)
- [x] 0.3.2 Fewer dependencies, before the steps that build and test more: `cargo tree
      --duplicates` and what each crate pulls in reviewed; duplicate versions unified, unused
      ones and unused default features dropped (`cargo-machete`, `cargo-udeps`), heavy crates
      used for little replaced by a few lines or by what the standard library has; the number
      of crates in `Cargo.lock` and a clean build's time, locally and per CI job, measured
      before and after. Nothing the application does changes. Almost all of the
      crates come with GPUI (523 of 976 only through `gpui-kit`) and wasmtime (46); what
      Birchpad pulls in on its own: insta (3 crates; the highlighting snapshots compare
      themselves, `BIRCHPAD_BLESS=1` accepts new ones), a third resvg for the icon example (5,
      now the version GPUI draws with), yaml-rust2's encoding detection (2), x11rb 0.14 beside
      GPUI's 0.13 (2) and similar 3 beside GPUI's 2.7 (1); an unused `tree-sitter-language`.
      `.github/dependabot.yml` holds resvg, similar and x11rb at GPUI's versions. base64 and
      roxmltree stay: both versions are built either way. `Cargo.lock` 1002 crates to 989;
      a clean debug build on Windows with 12 cores 608 s and 618 crates to 569 s and 607
- [x] 0.3.3 The window that announces an update: instead of Download, which opens the release
      page on GitHub, an Update button that does what Help > Update and Restart does (the
      application downloads the update, checks it against the signed manifest, installs it and
      starts again with the documents; once pressed, the bottom of the window, where the
      buttons were, becomes a progress bar of the download), and a "Don't show again" check
      box that turns update checks off (`updates.mode = "off"`, shown so in Preferences and
      Help > About), so that the window does not come back. A portable copy, which cannot
      update itself, keeps the link to the download page. No time limit on the download as a
      whole: today `timeout_recv_body` (30 s, `crates/update/src/lib.rs`) bounds the whole
      body, so over a slow connection a package fails with a timeout while it is still
      arriving. The download fails only when it stalls, nothing received for a minute, and
      says so; while bytes arrive, however slowly, it goes on and the bar shows it moving.
      The same bar wherever an update is downloaded on request: in the window that announces
      an update found by a background check, and in Help > Check for Updates, whose window
      stays open after Update and Restart with the bar in place of its buttons (today both
      show only a "Downloading" notification until the download ends or fails). A download
      that no window shows, the one `updates.mode = "auto"` starts by itself, has a bar in the
      status bar; hovering it says what is happening (the version being downloaded, how much
      of it has arrived, or that it is stalled), and it goes away once the update is ready.
      Tests: the button installs from a local server and the bar follows the download, in
      both windows and in the status bar with its tooltip; a server that sends slowly but
      steadily for longer than the old limit completes, one that stops sending fails as
      stalled, a failed download leaves the application as it was and says why, the check
      box with and without the button. Done: each read of a body waits at most a minute
      (`STALL`, a ureq connector over the default one) and none bounds the body as a whole; the
      installer reports the bytes received; the announcement of a background check stays open
      with the bar after Update, Help > Check for Updates' window too, and a window closed while
      its download goes on leaves the bar to the status bar. `birchpad-update` 0.3.0
- [x] 6.22 The editor view (`editor/mod.rs`, 83.8%): smart highlighting (78.7%), the Edit menu's
      extras (81.2%), what clicks, drags and the keyboard do at the edges of the text, in
      wrapped and folded rows and with several carets. Done (`birchpad-editor`'s
      `edge_tests.rs`): the wheel (sideways, Ctrl to zoom, at the top), the scroll bars,
      drags past the top and bottom, input methods composing over characters of two UTF-16
      units, pasting nothing or an image, clicks in wrapped rows, carets at the start and end
      of the text; the longest token smart highlighting takes, its search over several frames
      and reused while it covers the shown text; selected paths absolute and quoted
- [x] 6.23 Files, folders and sessions when the file system refuses: a file changed or deleted
      outside (`disk.rs`, 71.6%), saving, renaming and moving that fail (`file_ops.rs`,
      `file_commands.rs`, about 84%), sessions with missing files and broken backups (84.0%),
      Folder as Workspace (78.1%), Find in Files over unreadable files; the configuration
      folders (`config/load.rs`, 86.1%) and the hand-off to a running instance
      (`cli/instance.rs`, 87.8%). Done: the watcher on the folders of open files (an outside
      change noticed without a check, folders unwatched when their files close or detection
      is off) and monitored files checked every second without it; saving into a deleted
      folder or with a backup copy that cannot be made keeps the changes; renaming a file
      deleted meanwhile; a recent file that is gone; session files missing, malformed or of
      files that are gone, a lost backup falling back to the file, backups on their
      interval; the running executable's paths; a socket that cannot be made. Left: Folder
      as Workspace's context menu and Find in Files over unreadable files (permissions that
      Windows tests cannot take away)
- [x] 6.24 Updates and Plugins Admin against a local server: checks, downloads cut short, a
      wrong signature or checksum, an older version offered, installing and rolling back
      (`updates.rs` 76.2%, `update/install.rs` 87.6%, `extensions/admin.rs` 80.7%). Done:
      checks over HTTP from a local server (a newer release, an older one, a manifest signed
      with a key not trusted, nobody listening), packages cut short or tampered with that
      install nothing and leave the window saying why; Plugins Admin's Install... cancelled,
      given a file that is no manifest, and given a plugin's. Velopack's own install and
      rollback cannot run outside an installation; its feed is tested in `update/install.rs`
- [x] 6.25 Property tests where there are none yet: the bidirectional row (logical and visual
      positions map one to one), comparing texts (the differences applied give the other
      text), search and replace against a plain reference implementation, keystrokes and key
      sequences that read back as written, syntax trees reparsed after an edit equal to a
      fresh parse, the column editor's numbering. Done with proptest in `birchpad-view`
      (`bidi.rs`), `birchpad-tools` (`compare.rs`), `birchpad-core` (`search.rs`,
      `ops/column.rs`), `birchpad-commands` (`keystroke.rs`) and `birchpad-syntax`
      (`syntax.rs`, Rust sources built from pieces, 64 cases)
- [x] 6.26 Fuzzing what reads outside input and is not fuzzed yet: Hunspell dictionaries
      (`.aff`, `.dic`), search queries (regular expressions, the extended mode's escapes, Find
      in Files' filters), plugins' calls into the host with any arguments, Rhai scripts under
      their time and memory limits. Done: the targets `spell_dictionary`,
      `search_query`, `plugin_calls` and `rhai_script` (`crates/fuzz`, `fuzz/`), with seeds;
      scripts run there with a smaller limit on operations (`script::run_limited`)
- [x] 6.27 End-to-end scenarios beyond the four of `tests/e2e.rs`: Find in Files and Replace in
      Files, recording and playing a macro, a second instance handing over its files,
      restoring unsaved text after the process is killed, a plugin installed and run, Export
      to HTML; the run counted in the coverage report, so that `main.rs` (0%) is measured. Done: the six scenarios; scripts gained
      `wait` and `files` steps (Replace in Files there does not ask: its question is the
      system's dialog) and reports the search results; `file.export-html` takes a `path` to
      write without asking; the coverage job runs the end-to-end tests on a virtual display
- [x] 6.28 Performance in CI: the timing checks (`view/tests/performance.rs`, now ignored) and
      new ones for opening, searching and editing a 100 MB file and for a frame of the editor
      view run nightly in release, failing when a time grows past its limit. Done: every timing check states its
      limit (`birchpad_core::timing::within`); new ones search (a word, a regular expression,
      nothing, the replacements) and edit (a keystroke and its undo, every line) 100 MB
      (`core/tests/performance.rs`) and draw frames of a 100 MB document; the Performance
      workflow runs them every night in release, one at a time. The limits are generous
      until the first nightly runs measure them
- [x] 6.29 Mutation testing: `cargo-mutants` nightly over the crates without a window; the
      mutants that survive get tests, and the share caught gets a floor that only rises, as
      coverage does. Done: the Mutants workflow, a job per crate (`.cargo/mutants.toml`),
      against `.github/mutants-floors.toml` (`.github/mutants_floors.py`, tested beside the
      coverage check); the missed mutants in the run's summary
- [ ] 6.29a The first nightly mutants runs: the floors set from them, a point or two under, and
      tests for the mutants that survive
- [x] Versions: the workspace version, the application's, is 0.3.0; library crates have
      versions of their own, raised only in a release that changes them (CONTRIBUTING.md,
      Versions), checked by `birchpad-release crate-versions` and the release workflow. In
      0.3.0 so far: cli, commands, config, core, editor, i18n, io, plugin, syntax, tools,
      update and view at 0.3.0, the others at 0.2.5
- [x] Release 0.3.0: the workspace version (0.3.0, set when its branch opens, so that nightly
      builds are its pre-releases), `CHANGELOG.md` and this roadmap updated; the
      [release checklist](release-checklist.md) gone through on 2026-10-18; the checks
      a person makes on real systems (installers, input methods, screen readers, displays)
      are noted in the release

### Release 0.4.0: languages and text

- [x] 6.30 Code pages: EBCDIC 875 (Greek), whose standard table maps two bytes to one
      character; NEL as a line break in EBCDIC documents
- [x] 6.31 Auto-completion: closing HTML and XML tags; API files for languages embedded in a
      document (scripts in HTML); cycling through a function's overloads in the call tip;
      Notepad++'s auto-completion options (ignore numbers, ignore case, completion from a number
      of characters per source); bundled API files for common languages, written for Birchpad
      (Notepad++'s are GPL)
- [x] 6.32 User Defined Languages: the UDL editor dialog; folding (code and comment folders);
      number prefixes, suffixes, extras and ranges; comments that must start a line; nested
      delimiters; colors from the theme where the UDL leaves them unset; Open User Defined
      Language Folder, the UDL collection link
- [x] 6.33 Right-to-left text: selections drawn piece by piece over mixed text; margins on the
      right in right-to-left mode; keys for Text Direction RTL and LTR; the direction
      remembered per document in sessions; very long rows reordered too
- [x] 6.34 Localization: more interface languages, translated for Birchpad; Notepad++'s
      `nativeLang` XML files read from the user's own copy, not bundled
- [x] Markdown preview beside the text, as in IntelliJ IDEA: Editor Only, Editor and
      Preview, Preview Only ([ADR 0042](adr/0042-previews.md), issue #41)
- [x] PlantUML preview of `.puml` files in the same modes, drawn by the PlantUML installed
      ([ADR 0042](adr/0042-previews.md), issue #42)
- [x] 6.35 Tests out of `src`: every test of a crate in its `tests` folder, none in `src`;
      for now as unit tests that `src` includes from there (`#[path]`), with access to the
      crate's private items
- [ ] Release 0.4.0: the workspace version (0.4.0, set when its branch opens, so that nightly
      builds are its pre-releases), `CHANGELOG.md` and this roadmap updated; the
      [release checklist](release-checklist.md) gone through

### Release 0.5.0: printing, tools, help and PlantUML

- [x] 6.36 Integration tests: each crate's tests in its `tests` folder test it through its
      public interface, the application split into a library and the program so that they
      can; the unit tests 6.35 moved there are rewritten as such tests
      ([ADR 0043](adr/0043-tests-through-the-public-interface.md))
- [x] 6.37 Printing: a print preview and page setup of Birchpad's own (margins, header and
      footer with Notepad++'s variables, colors or black on white, line numbers), printing the
      selection, Print Now; native printing where the platform offers it: Windows' (GDI); on
      macOS and Linux the pages print through the browser, since printing there needs an
      `NSView` or GTK, which GPUI and Birchpad do not have
      ([ADR 0044](adr/0044-print-preview-and-native-printing.md))
- [x] 6.38 Tools: hashes of files in a dialog with Copy, as Notepad++ shows them, besides the
      listing in a new document
- [x] 6.39 Help menu: Command Line Arguments, home page, online manual and forum, Debug Info
      (copyable), Set Updater Proxy
- [x] 6.40 The command line options Birchpad accepts but ignores: -notabbar, -systemtray,
      -alwaysOnTop, -loadingTime, -r (recursive wildcards), -quickPrint, -monitor,
      -openFoldersAsWorkspace, -x and -y (window position), -qn, -qt, -qf, -qSpeed (ghost
      typing), -settingsDir, -titleAdd, -udl, -pluginMessage; -nopluginsupport too. Not as
      Notepad++ does: -systemtray starts minimized, since GPUI has no tray icon (7.4);
      -openFoldersAsWorkspace is what Birchpad does with every folder; -qn names Notepad++'s
      easter eggs, its own texts, which Birchpad does not bundle (it says so); -pluginMessage
      is accepted but reaches no plugin until the plugin API has an event for it (6.42)
- [x] 6.41 PlantUML without installing it: the preview of a PlantUML document without a
      PlantUML offers to install one, and one click downloads PlantUML's MIT build with a Java
      runtime made for it, built for each platform by Birchpad's release workflow and pinned
      by size and SHA-256 ([ADR 0045](adr/0045-plantuml-installed-on-request.md))
- [x] [#45](https://github.com/SecurityTrip/Birchpad/issues/45) Links open with Ctrl+click
      only; a double click selects a word of them as anywhere, and opens them only with
      Preferences > Cloud & Link > Open them with a double click too (`links.double-click`)
- [x] Release 0.5.0: the workspace version (0.5.0, set when its branch opens, so that nightly
      builds are its pre-releases), `CHANGELOG.md` and this roadmap updated; the
      [release checklist](release-checklist.md) gone through

### Release 0.6.0: plugins, documentation, parity

Closing Phase 6.

- [x] 6.42 A wider plugin API: settings of their own, files opened with the user's permission,
      a panel of their own controls, items in the text's context menu, events while typing and
      when the selection changes, the command line's -pluginMessage, work in the background;
      scripts' own commands and keys; Settings > Import plugin(s)
      ([ADR 0046](adr/0046-plugin-api-0-2.md))
- [x] 6.43 Compare as ComparePlus: lines padded to stay aligned side by side, differences
      within a line, comparing with the last save, the clipboard or Git; a navigation bar of the
      differences ([ADR 0047](adr/0047-compare-as-compareplus.md)): the navigation bar is the
      vertical scrollbar's track, marked where the differences are
- [x] 6.44 JSON Viewer's tree panel (find a path, copy a value), sorting keys; XML Tools'
      XPath, XSD and DTD validation, escaping and unescaping, closing tags as typed
      ([ADR 0048](adr/0048-json-viewer-and-xml-tools.md)): XPath and schemas as far as the ADR
      lists; Tag Auto-Close is the Auto-Completion setting that closes tags
- [x] 6.45 Hex Editor in place: the bytes edited in a hex view of the document itself
      (overwrite, insert, the caret shared with the text), columns and grouping, Find in hex
      ([ADR 0049](adr/0049-hex-editor-in-place.md)): bytes that are not text in the document's
      encoding wait for the rest of their character, since the document is text
- [x] 6.46 DSpellCheck's options (several languages at once, ignore rules, words in code by
      style); a dictionary bundled for English under a license compatible with Birchpad's
      ([ADR 0050](adr/0050-spell-checker-options-and-built-in-dictionary.md): SCOWL's en_US)
- [x] 6.47 Online features under the `OnlineFeatures` policy: Plugins Admin's online list of
      plugins (available, updates, installed) with checksums; DSpellCheck's dictionary download
      ([ADR 0051](adr/0051-online-catalog.md))

- [x] 6.48 Documentation in Russian: the user's and the contributor's documents (README,
      CONTRIBUTING, SECURITY, PRIVACY, the code of conduct, writing plugins and scripts, the
      release checklist and this roadmap) as `*.ru.md` next to the English ones, linked from
      each other; from then on a change to one updates the other. ADRs and the changelog stay
      in English
- [x] 6.49 A last comparison with Notepad++ 8's menus, Preferences and command line, with what
      is still missing moved to Phase 7 or written down. Done against the user manual
      (npp-user-manual.org) on 2026-10-21: the command line gained `-noPlugin`, `-L<code>`,
      `-notepadStyleCmdline`, `-z` and `--help`; what the menus and Preferences still lack is
      7.4a to 7.4d. Not planned, as they have no meaning for Birchpad: DirectWrite (GPUI draws
      the text), the colorization of Notepad++'s toolbar icons (Birchpad's are Lucide's
      outlines), Preferences > File Association (Birchpad registers as the default editor, 0.2.8)
      and the installer's own options (`/S`, `/D=`, `/noUpdater`, ...; Birchpad's installers
      take their own)
- [x] Release 0.6.0: the workspace version (0.6.0, set when its branch opens, so that nightly
      builds are its pre-releases), `CHANGELOG.md` and this roadmap updated; the
      [release checklist](release-checklist.md) gone through

## Phase 7: beyond Notepad++

What Notepad++ does not have, in releases 0.7.0 to 0.9.0; closing the phase releases 1.0.0.
The steps follow the rules of Phase 6: clean-room where an editor is followed, one commit
each with its tests, docs and a `CHANGELOG.md` entry, its options in Preferences.

### Release 0.7.0: finding your way

- [x] 7.1 Command palette: every command of the catalog by its name, with its shortcut, the
      recently used first ([ADR 0052](adr/0052-command-palette.md)): View > Command
      Palette... (Ctrl+Shift+A, as Ctrl+Shift+P is Notepad++'s macro playback)
- [x] 7.2 Fuzzy file finder: the files of the workspace's folders and of the recent files, by
      parts of their path ([ADR 0052](adr/0052-command-palette.md)): Search > Go to File...
      (Ctrl+Shift+O, as Ctrl+P is Print), with Preferences > Go to File
- [x] 7.3 Syntax-aware selection (expand and shrink by the syntax tree) and sticky scroll (the
      enclosing scopes' first lines kept at the top)
      ([ADR 0053](adr/0053-syntax-selection-and-sticky-scroll.md)): Edit > Expand Selection
      and Shrink Selection (Ctrl+Alt+Up and Down); sticky scroll in Preferences > Editing
- [x] 7.4a Menus left from Notepad++ 8 (6.49): Edit > Auto-Completion (the completion commands
      exist, the submenu does not), Line Operations' Sort in Locale Order, On Selection's Redact
      Selection and Change Search Engine..., Column Mode..., Read-Only in Notepad++ (on the
      current or every document) beside the file's read-only attribute; Search > Copy Marked
      Text (only the Mark tab has it) and the search results' Find in These Search Results...;
      View > Show Symbol's Non-Printing Characters and Control Characters & Unicode EOL, Zoom's
      Synchronize Across Views; Tools' hash Generate... of a typed text. Built from the user
      manual and the menus' behavior. Auto-Completion gains Word Completion (Ctrl+Enter) and
      Path Completion (Ctrl+Alt+Space); the locale order is Unicode's default order in short
      (letters, then accents, then case, Cyrillic й a letter of its own), not each language's
      own rules, as Windows' would be; Read-Only in Notepad++ is Read-Only in Birchpad, the
      attribute Read-Only Attribute of the File (write permission elsewhere than Windows)
- [x] 7.4b Preferences left from Notepad++ 8 for the window and the text (6.49): hide the menu
      bar and the status bar; the tab bar's maximum label length, alternate icons, inactive tab
      color, colored bar on the active tab, pinning turned off or only on its button, buttons
      on inactive tabs; Editing's caret width and blink rate, wrapped lines' indent, smooth
      font, virtual space everywhere, scrolling beyond the last line, keeping the selection on
      a right click, copying and cutting the line with nothing selected as an option, the
      selected text's own foreground, dragging the selected text turned off; multi-editing and
      column-to-multi-editing as options, how end of lines and non-printing characters are
      drawn, typing control characters prevented; Dark Mode's tones; Margins' fold style,
      border width, line numbers' width, padding left, right and in Distraction Free mode, the
      vertical edge as a background. Built from the user manual's Preferences page. The edge
      as a background was there already (Margins > Vertical edge > Background); selected text
      can now be dragged, which the option turns off; virtual space takes clicks past the end
      of a line, and the arrow keys still stop at it. Not done: smooth font, since GPUI draws
      and smooths the text itself on every platform; how non-printing characters are drawn
      (abbreviations or code points), since Birchpad gives every character the cells it takes
      and a label several cells wide would need the column model to change with the setting
      (they are framed or marked instead); Dark Mode's customized tone, beside the seven tones
      (the Style Configurator sets the interface's colors); these are 7.4e, in 0.7.1
- [x] 7.4c Preferences left from Notepad++ 8 for files, search and typing (6.49): New Document's
      encoding applied to opened ANSI files, a new document at startup, the first line as an
      untitled tab's name; opening a folder's files one by one; Recent Files' submenu, file name
      only or full path, maximum length, skipping the existence check; Language's compact menu,
      hidden languages, SQL's backslash; Backspace unindents; Highlighting's Mark All match case
      and whole word, smart highlighting in the other view, matching tags and their attributes,
      comment zones; Print's form feed as a page break; every option of Searching (In selection
      from a size, filling the Find field from the selection or the word, monospaced font, the
      dialog staying open, confirming Replace All in all documents, replacing without moving,
      one result per line, Find in Files on unsaved changes and from the document's folder);
      Auto-Completion's accept with Tab or Enter, brief list, each pair and three of one's own;
      Delimiter's word characters; Performance's large file restrictions one by one; Cloud &
      Link's full box on hover; MISC's peek on the document map, mute sounds, encoding
      detection, Save All confirmation, following symbolic links in Folder as Workspace, the
      session and workspace file extensions; Multi-Instance's panels with `-nosession`. Built
      from the user manual's Preferences page. As Birchpad reads ASCII as UTF-8, "Apply to
      opened ANSI files" gives files of ASCII alone the new document's UTF-8 (with or without
      its byte order mark) or ANSI; Highlight Matching Tags is new, in HTML and XML; the
      Auto-Completion pairs, three of one's own too, were there already (any pairs in
      `completion.auto-close`), and so were Find in Files from the document's folder (Follow
      current doc., now also a preference) and Style All Occurrences' match case and whole
      word. Not done: SQL's backslash as an escape, since the SQL grammar decides what a
      string is and has no such switch (7.4f, in 0.7.1); muting sounds, since Birchpad makes
      none
- [x] 7.4d Command line left from Notepad++ 8 (6.49): `-fullReadOnly` and
      `-fullReadOnlySavingForbidden`, every document read-only for the run, saving too: the
      documents of the session too, Read-Only in Birchpad cannot be turned off, and the second
      takes Save, Save As, Save a Copy As and Save All away
- [x] Release 0.7.0: the workspace version (0.7.0, set when its branch opens, so that nightly
      builds are its pre-releases), `CHANGELOG.md` and this roadmap updated; the
      [release checklist](release-checklist.md) gone through

### Release 0.7.1: what 0.7.0 left

What the steps of 0.7.0 could not do, each as soon as it can be done; until then, as before,
written down why not.

- [x] 7.4 Minimize to the system tray (Notepad++'s Preferences > MISC), once GPUI has a tray
      icon; until then, written down why not. Not in 0.7.0: GPUI (gpui-pre 0.3.9, which
      gpui-kit 0.7 and 0.8 both build on, checked on 2026-10-25) has no tray or status-bar
      icon on any platform, nor a way to hide a window from the taskbar and bring it back from
      outside it; a window minimized there could not come back. A tray icon of Birchpad's own
      would be three platforms' native code beside GPUI's event loop (a Windows message window,
      an NSStatusItem, a D-Bus StatusNotifierItem), which is what GPUI should provide.
      `-systemtray` keeps starting minimized (6.40). Not in 0.7.1 either: gpui-pre 0.3.10 and
      gpui-kit 0.8.1, checked on 2026-10-27, add no tray icon and no way to take a window off
      the taskbar; their only hiding is macOS's Hide of the whole application. Done in 0.7.1
      without GPUI, on Windows and macOS: Preferences > MISC > Minimize to system tray, and
      `-systemtray` starts there; on Windows a subclass of the window's procedure and
      `Shell_NotifyIconW`, on macOS an `NSStatusItem` and the application hidden out of the
      Dock ([ADR 0056](adr/0056-system-tray-on-windows-and-macos.md)). Not on Linux: GNOME shows
      a StatusNotifierItem only with an extension, and Wayland lets no application hide its
      window and show it again; there `-systemtray` still starts minimized
- [x] 7.4e What 7.4b left: how non-printing characters are drawn, as abbreviations (NBSP,
      ZWSP) or code points, which needs the column model to give such a character the cells
      of its label while the setting is on; Dark Mode's customized tone beside the seven
      tones; smooth font, once GPUI lets an application choose how text is smoothed. Built
      from the user manual's Preferences page. Preferences > Editing > Non-printing characters
      (framed and marked by default, as before; abbreviation; code point): the column model
      takes a grid of the tab width and the labels, a labeled character the cells of its label
      ([ADR 0054](adr/0054-non-printing-labels-in-the-column-model.md)); characters without an
      abbreviation show their code point. Dark Mode's tone Customized, with ten colors of the
      dark interface in Preferences > Appearance (`appearance.dark-tone-colors`). Enable smooth
      font: GPUI (gpui-pre 0.3.9) lets an application choose subpixel smoothing, which Windows
      and Linux draw; macOS draws text in grayscale only, so there it changes nothing
- [x] 7.4f What 7.4c left: SQL's backslash as an escape character, which needs SQL strings
      read by a grammar or a query of Birchpad's own rather than the SQL grammar's. Built from
      the user manual's Preferences page. Neither: with Preferences > Language > Treat
      backslash as escape character for SQL, the parser reads a quote after an odd run of
      backslashes as a backslash, the same length, so the grammar's strings go on past it
      ([ADR 0055](adr/0055-sql-backslash-escapes.md))
- [x] Release 0.7.1: the workspace version (0.7.1, set when its branch opens, so that nightly
      builds are its pre-releases), `CHANGELOG.md` and this roadmap updated; the
      [release checklist](release-checklist.md) gone through

### Release 0.7.2: PlantUML again

- [x] 7.4g PlantUML redone: the preview of a PlantUML document worked out anew (rendering,
      updates while typing, errors), with what its users asked for:
      [#50](https://github.com/SecurityTrip/Birchpad/issues/50) relative paths (`!include` and
      the like) from the document's folder;
      [#51](https://github.com/SecurityTrip/Birchpad/issues/51) exporting a diagram (PNG, SVG);
      [#52](https://github.com/SecurityTrip/Birchpad/issues/52) zooming and scrolling the
      diagram; [#53](https://github.com/SecurityTrip/Birchpad/issues/53) a default background
      for diagrams; an ADR for the new design, over
      [ADR 0045](adr/0045-plantuml-installed-on-request.md). Built from the issues and
      PlantUML's command line as its users run it
      ([ADR 0057](adr/0057-plantuml-preview-redone.md)): PlantUML runs in the document's
      folder; a run the text moved past is stopped; mistakes are listed by the document's line,
      as links to it; diagrams on white by default (`preview.plantuml-background`), switched
      from the bar above them; Zoom In, Zoom Out, Actual Size and Fit there and with Ctrl and
      the mouse wheel, from 25% to 400%, drawn sharp; Copy Diagram and Export Diagram as PNG or
      SVG from a diagram's context menu, the PNG drawn by Birchpad from PlantUML's SVG at
      `preview.plantuml-png-scale`
- [x] Release 0.7.2: the workspace version (0.7.2, set when its branch opens, so that nightly
      builds are its pre-releases), `CHANGELOG.md` and this roadmap updated; the
      [release checklist](release-checklist.md) gone through

### Release 0.7.3: fixes

- [x] 7.4h [#57](https://github.com/SecurityTrip/Birchpad/issues/57) The messages above the
      text (a read-only file, `-ro`, monitoring, a file that does not decode) drawn in the
      theme's colors, so that they read in the dark theme too: in the theme's text color, on
      its `surface` (read-only), `selected` (monitoring) or `match-background` (a file that
      does not decode), and with the Dark Mode tones over it
- [x] 7.4i [#58](https://github.com/SecurityTrip/Birchpad/issues/58) The PlantUML preview
      scrolled both ways when a zoomed diagram does not fit: scrollbars, and Shift with the
      mouse wheel for across. Zoomed, the diagrams are laid out as wide as the widest of them,
      so that there is something to scroll across to; both scrollbars stay shown while a
      diagram does not fit; Shift with the wheel scrolls across where the system does not
      turn it so itself, no further than the diagram's edges
- [x] Release 0.7.3: the workspace version (0.7.3, set when its branch opens, so that nightly
      builds are its pre-releases), `CHANGELOG.md` and this roadmap updated; the
      [release checklist](release-checklist.md) gone through

### Release 0.7.5: refactoring

The code refactored throughout before the development tools of 0.8.0, so that each crate
builds and tests faster and depends on less. Behavior stays as it is: the tests, the
end-to-end tests and the highlighting snapshots pass unchanged, and the public paths the
tests use stay where they are.

- [x] 7.4j Large modules split by what they do, each into a folder of smaller ones: the
      editor's `lib.rs` (the view, its commands, its mouse and keyboard), its layout and its
      buffer; the application's Preferences (a module per section), Find, the workspace and
      the pane; `birchpad-config`'s settings; `birchpad-tools`' XML Schema
- [x] 7.4k Duplicated code brought together: what several parts of the window repeat
      (running a command from a button or a menu item and reporting its error, bars and
      their colors, the theme's colors read for an element) in one place of the lowest layer
      that can hold it
- [x] 7.4l The application crate lighter: the parts of the window that need no workspace
      moved to crates of their own (the quick pick lists, the search results, the system
      tray, the system's print dialog), each with its own tests and only the dependencies it
      uses; an ADR for the new crate layout, over [ADR 0041](adr/0041-crate-layout.md)
      ([ADR 0058](adr/0058-parts-of-the-window-in-crates-of-their-own.md))
- [x] Release 0.7.5: the workspace version (0.7.5, set when its branch opens, so that nightly
      builds are its pre-releases), `CHANGELOG.md` and this roadmap updated; the
      [release checklist](release-checklist.md) gone through

### Release 0.7.7: PlantUML on a real repository

What opening a real architecture repository in Birchpad showed: 124 PlantUML diagrams (C4
context and container diagrams over thousands of lines of shared `!include` files, and
sequence diagrams), each drawn the way the preview draws it (the PlantUML Birchpad installs,
`-pipe`, smetana, the document's folder). 85 were drawn; of the 39 that were not, 19 are
Birchpad's fault (7.4m), 2 are the smetana layout's (7.4o) and 18 are mistakes of the
diagrams themselves, which the preview should show at the right place (7.4n). And a question
from a user, whether sequence diagrams can be written as code in Birchpad: they can, and
are drawn; what is missing is the editing of their text (7.4r).

- [x] 7.4m Files included by included files found: PlantUML gets the document's folder as its
      include path (`PLANTUML_INCLUDE_PATH`, which PlantUML reads as it reads
      `-Dplantuml.include.path`, and which reaches it whatever runs it) besides running in it,
      so that a file included from another folder (`shared/context.iuml`) that includes by
      a path relative to the document (`../../shared/billing/billing_context.puml`) finds it, as
      IDE plugins and Confluence's macro do. Without it, every context diagram of that repository failed with
      "cannot include"; with it, all 19 are drawn. Folders of the user's own after the
      document's (`preview.plantuml-include-path`, separated as in `PATH`, a relative one taken
      from the document's folder), for libraries kept elsewhere, then those of the variable
      Birchpad was started with
- [x] 7.4n Mistakes in included files placed where they are: PlantUML's error output gives the
      line in the file the mistake is in, and the preview takes it for a line of the document
      ("Line 40: Syntax Error?" on 12 container diagrams, a link to an `!include` line that has
      nothing to do with it; the mistake is an undefined procedure on line 40 of an included
      file). The picture of the mistake says where it is (`[From string (line 75)]` for the
      document, another name for an included file): a mistake in an included file says so and
      links to the document's `!include` line that leads to it, the last of the document's
      lines in the picture's trace, or to no line when none is found
- [x] 7.4o Diagrams smetana cannot lay out: on the two largest diagrams (about 3,400 lines once
      included) PlantUML's smetana layout crashes (`NullPointerException` in `SmetanaEdge`) and
      the preview lists Java's stack trace as the mistake. A crash shown as one line (PlantUML
      could not lay out the diagram, with the layout it used), its details folded; Graphviz
      used where it is installed: `preview.plantuml-layout` (automatic, smetana, Graphviz), the
      automatic one Graphviz's `dot` when it is found (`GRAPHVIZ_DOT`, `PATH`, Graphviz's folder
      on Windows) and smetana
      otherwise, so that diagrams are laid out as on Confluence and in IDEs, which use
      Graphviz. An ADR, over [ADR 0057](adr/0057-plantuml-preview-redone.md), decides whether
      Birchpad installs Graphviz on request as it installs PlantUML
      ([ADR 0045](adr/0045-plantuml-installed-on-request.md)): it does not
      ([ADR 0059](adr/0059-plantuml-layouts-and-graphviz.md))
- [x] 7.4p The preview follows the files a diagram includes: it is drawn again when a PlantUML
      file in the document's folder or one it includes from changes, saved in Birchpad or on
      disk, rather than only when the document's own text changes; Redraw in the preview's
      toolbar and View > Preview for the rest
- [x] 7.4q Faster drawing: each drawing starts Java anew (0.6 s on its own, 1.5 to 3.5 s for a
      diagram of that repository with its includes). PlantUML kept running for the window and
      given each drawing in turn, started again when a run the text moved past is stopped;
      measured before and after on the same diagrams, and kept only if it is faster. Kept
      ([ADR 0060](adr/0060-plantuml-kept-running.md)): 0.5 to 0.7 s a drawing before, 0.01 to
      0.02 s with a warm PlantUML, for Java (the installed PlantUML, a `plantuml.jar`); a
      diagram still being typed, and a `plantuml` script, start PlantUML for each drawing
- [x] 7.4r PlantUML as a language of its own: Birchpad shows a `.puml` as a normal text file. A
      PlantUML language (Language menu, status bar): highlighting of `@start`/`@end` lines,
      keywords and participants' kinds, arrows, `!` preprocessor lines, strings, colors and
      comments (`'`, `/' '/`, which Toggle Comment uses); folding by blocks (`box`, `alt`,
      `group`, `!if`, `!procedure` and their ends) rather than by indentation; the Function
      List with participants, `==` sections, titles and `!procedure` and `!function`; Ctrl+click
      on an `!include` with a path opens the file. A grammar of
      Birchpad's own, `birchpad-tree-sitter-plantuml`
      ([ADR 0061](adr/0061-plantuml-grammar-of-its-own.md)); notes, legends, headers and footers
      of several lines and blocks in braces fold too
- [x] 7.4s Moving about a diagram: dragging it with the mouse scrolls it, as a hand does; Fit
      Page beside Fit, the whole diagram in the preview, since a wide sequence diagram fitted to
      the width is too small to read
- [x] 7.4t Closing to the tray, asked once: the tray (7.4) is reached only by minimizing, and
      people do not find Preferences > MISC > Minimize to system tray. A new setting,
      `misc.close-to-tray`, with no value until the user is asked: closing the main window (its
      ×, Alt+F4) while it has none asks whether to minimize Birchpad to the system tray or to
      close it, and saves the answer, so that it is asked only once; with a value, closing does
      what it says without asking. The answer can be changed in Preferences > MISC (Close
      button: ask, minimize to the tray, close). File > Exit always closes. Only where there is
      a tray (Windows and macOS); on Linux nothing is asked and closing closes

The same repository again, with the development build of 7.4m to 7.4s: 105 of the 124
diagrams are drawn (85 before). Of the 19 that are not, 17 have mistakes of their own and 2 are
the ones smetana cannot lay out, with no Graphviz on that machine. What it still showed:

- [x] 7.4u Mistakes in included files when PlantUML does not name the file: on 16 of the 17
      diagrams with mistakes the picture's trace is a single `[From desc2 (line 40)]`, with no
      `string` line of the document, so 7.4n finds no line: the preview shows "desc2, line 40",
      a name that is no file, and the mistake links nowhere. The file found by what the picture
      shows: the line under the mistake (`$audit_logger()`) at that line number in one
      of the files the diagram includes (7.4v); the mistake then names that file, relative to
      the document, and opens it at the line. When none is found, it says the mistake is in an
      included file, without PlantUML's internal name
- [x] 7.4v Includes by variables and functions followed: Birchpad reads an `!include`'s path
      only when it is written out, and every include of that repository goes through a
      variable or a function (`!include $DIAGRAMS_ROOT/library/main.iuml`,
      `!include FromRoot(/parts/billing/containers.puml)`). So the preview does not
      follow the files those diagrams include (checked: an include added on disk to
      `library/services/all.iuml` of a copy, the preview kept the old mistake until Redraw),
      and Ctrl+click on such an `!include` opens nothing. Variables set in the document and in
      what it includes (`!$DIAGRAMS_ROOT ?= "../.."`, `!$X = "..."`) put into the path; for a
      path only PlantUML can work out (a function of the diagram's own), the files PlantUML
      reads taken from it, if it can tell them; an ADR, over
      [ADR 0057](adr/0057-plantuml-preview-redone.md), decides how: Birchpad reads the
      preprocessor's variables and functions itself
      ([ADR 0062](adr/0062-includes-through-the-preprocessor.md))
- [x] 7.4w A way to Graphviz from the crash: the message of a diagram smetana cannot lay out says
      that Graphviz lays it out, and stops there. Two links under it: Graphviz's download page
      and Preferences > Preview at the layout (`preview.plantuml-layout`), and the diagram drawn
      again by itself once a `dot` is found
- [x] 7.4x The first drawing at Fit: the first time a sequence diagram was shown, at Fit, its
      white card was as tall as the preview, the diagram at its top and white below it; after
      switching to Fit Page and back it was as tall as the diagram. Reproduced and fixed, or
      written down why it cannot be reproduced. The cause:
      at Fit a card took its size from its image's, which is not known before the image is
      decoded; a card now gets its size from the diagram's own (from its SVG) and the preview's
      width, and the preview draws again once it is laid out

Once more, with 7.4t to 7.4x built: the mistakes of all 124 diagrams placed as the preview
places them, and the steps tried in the window on a copy of the repository. A first drawing at
Fit, the links to Graphviz, Ctrl+click on `!include $DIAGRAMS_ROOT/...` and the redraw when such
an included file changes on disk work. Of the 16 diagrams with a mistake in an included file, 6
name the file and open it at the line; 10 still say only that it is in an included file:

- [x] 7.4y Includes through functions of the diagram's own: the 10 include the file with the
      mistake by `!include FromRoot(/parts/billing/containers.puml)`, and `FromRoot` is
      not worked out. It is an `!unquoted function` with a `!local` variable,
      `%get_variable_value("$BRANCH")` and an `!if`/`!else` around its two `!return`s
      (`fullpath.iuml`), which the preprocessor of 7.4v does not follow. So for
      `orders_containers.puml` and the other diagrams whose parts are included that way,
      the mistake links nowhere, Ctrl+click on such an `!include` opens nothing, and those files
      are not followed. Functions with `!unquoted`, `!local` variables, `!if`, `!elseif` and
      `!else` around `!return`, and the built-in functions such paths use (`%get_variable_value`,
      `%strpos`, `%string`), worked out far enough to give the path; what is still out of reach
      stays as it is now
- [x] 7.4z The included file named relative to the document: a mistake in one names it by its
      whole path (`C:\Users\...\repo\parts\billing\containers.puml, line 41`), longer
      than the message, because only the document's own folder is taken off the path and the
      file is above it. Named relative to the document
      (`../../parts/billing/containers.puml`). With it, the texts seen while checking:
      the link under a crash reads "в Настройки > Предпросмотр" in Russian, which should be "в
      Настройках > Предпросмотр"; the question when a file open in Birchpad changes on disk has
      the title "Warning" in the Russian interface. Done but
      for the title: GPUI's Windows dialog sets it itself, in English ("Info", "Warning",
      "Critical", `gpui-fast-windows`), with no way to give it one; the only way around is
      Birchpad drawing every question in its window instead of the system's dialog
      (`set_prompt_builder`), a change of its own

A third time, with 7.4y and 7.4z built: all 16 mistakes in included files name their file,
relative to the document, and open it at the line, those included through a function of the
diagram's own too; Ctrl+click on such an `!include` opens the file, and saving it in Birchpad
draws the diagram again. What is left:

- [x] 7.4aa The picture of a mistake folded: once the preview has listed a mistake above the
      diagrams, with its file, line and message, PlantUML's picture of it says the same again,
      as tall as the preview, and PlantUML adds to it at random its own banners ("Add your own
      dedication into PlantUML. For just $5 per month", Patreon) and joke lines, on white or on
      black. The picture of a mistake the preview has placed folded under Details, as the
      details of a crash are (7.4o); shown as now when the mistake has no place, so that what
      PlantUML says is not lost
- [x] 7.4ab Diagrams smetana cannot lay out drawn with ELK: on C4 container diagrams with
      boundaries inside boundaries and many relations between them (reproduced on 105 made-up
      lines: two `System_Boundary` with `Container_Boundary` in them, 43 `Rel`, `BiRel`,
      `Rel_Back` and `Rel_U/D/L/R` across them) smetana's routing of a spline fails
      ("triangulation failed" in `routesplines`), loses the edge and crashes with
      `NullPointerException` in `SmetanaEdge`; `skinparam linetype polyline` or `ortho` does not
      help. ELK, PlantUML's third layout, lays out all of them. The PlantUML Birchpad installs
      is its EPL build (`plantuml-epl`, which has ELK; the MIT build has none), with the
      `org.eclipse.xtext.xbase.lib` that the EPL build's ELK needs and lacks, as revision
      `1.2026.9-3` of the package; with the automatic layout a crash of smetana draws the
      diagram again with ELK, and says so; `preview.plantuml-layout` gets `elk`. A PlantUML of
      an earlier revision keeps working until it is installed again. An ADR, over
      [ADR 0045](adr/0045-plantuml-installed-on-request.md) and
      [ADR 0059](adr/0059-plantuml-layouts-and-graphviz.md)
- [x] 7.4ac The newer PlantUML offered, and drawn with once installed: who installed PlantUML
      before 7.4ab keeps revision 1, without ELK, and is told of revision 3 only under a crash
      of smetana; and a preview drawn again after an installation only when it had no PlantUML,
      so the diagram that crashed stays crashed until Redraw. A preview drawn with an earlier
      revision of the PlantUML Birchpad installs shows one quiet line above the diagrams, that
      a newer PlantUML lays out what smetana cannot, with Update (its size, then its progress);
      it goes once the newer one is installed. Nothing is downloaded without that click, as
      [ADR 0045](adr/0045-plantuml-installed-on-request.md) asks. Every PlantUML preview is drawn
      again when an installation of PlantUML ends, whatever it showed

Revision 3 checked: its Windows package matches the size and SHA-256 Birchpad pins, installs
from the line of 7.4ac, and draws the 124 diagrams of the architecture repository with smetana
as revision 1 did (105 drawn). With ELK it draws 67. What it showed:

- [x] 7.4ad ELK crashes on diagrams smetana draws: of the 124, ELK crashes on 40 that smetana
      draws, and on both that smetana crashes on, so the fallback of 7.4ab draws none of them
      and `preview.plantuml-layout = elk` loses 40 diagrams. Each is a
      `NullPointerException` ("node1" is null) in `ElkGraphUtil.createSimpleEdge`, reproduced in
      six lines: `remove @unlinked`, `rectangle R {`, `component A`, `}`, `component B`,
      `B --> R`, a relation to a group once unlinked elements are removed; `remove $tag` on an
      element with a relation crashes it too. C4 container diagrams relate to boundaries and
      remove what is unlinked. Reported to PlantUML with those lines; ADR 0063's "lays out
      every one of those diagrams" put as measured; when ELK crashes too, the preview says
      that neither smetana nor ELK could lay the diagram out and that Graphviz is left, rather
      than showing smetana's crash alone; the description of `elk` in Preferences says what it
      cannot lay out. The report to PlantUML is written; it is posted from the
      maintainer's account
- [x] 7.4ae The earlier PlantUML left behind: installing revision 3 while revision 1's
      PlantUML is kept running (ADR 0060) cannot delete revision 1's folder on Windows, where
      its Java holds the files. Its `.installed` goes and the rest, 63 MB, stays, and revision
      1's two Java processes run until Birchpad ends. The PlantUML processes kept for a
      component ended before its earlier folders are deleted, and a folder of an earlier
      version that is still there deleted when Birchpad starts
- [x] 7.4af An installation cut off goes on where it stopped: Birchpad closed at 36 of 58 MB
      left a `.zip.part`, and the next installation started from nothing (the package took 16
      minutes at the speed seen). The download goes on from the part when the server takes a
      range, checked by its size and SHA-256 as now, and starts again when it does not
- [x] 7.4ag What a crash of smetana offers, said once and clearly: with revision 1 the line of
      7.4ac (Update) and the crash's link Install PlantUML again, with ELK offer the same at
      once; one is left. With `preview.plantuml-layout = elk` and a PlantUML without ELK
      (revision 1, a `plantuml` of PlantUML's MIT build) the preview shows
      `ClassNotFoundException: org.eclipse.elk...` with nothing about why: it says that this
      PlantUML has no ELK, and offers the one that has. And under Details, which folds the
      crash's stack trace, PlantUML's picture of the crash shows the same stack trace unfolded;
      the picture folded with it, as 7.4aa folds the picture of a placed mistake
- [x] Release 0.7.7: the workspace version (0.7.7, set when its branch opens, so that nightly
      builds are its pre-releases), `CHANGELOG.md` and this roadmap updated; the
      [release checklist](release-checklist.md) gone through

### Release 0.8.0: development tools

- [x] 7.5 LSP client: diagnostics, completion, hover, go to definition and references,
      rename, formatting; servers configured per language, none bundled
      ([ADR 0065](adr/0065-language-server-client.md)): `lsp.servers.<language>` names a
      server's command line (Preferences > Language Servers); one server runs per language and
      root folder (the nearest with `.git`), with the full text after edits; diagnostics are
      underlined and listed, the server's completions come first in the auto-completion list,
      and Tools > Language Server shows hover, goes to definitions, lists references in the
      search results, renames in every file without saving and formats; `birchpad-lsp` holds
      the protocol without a window, fuzzed as `language_server`
- [x] 7.6 Git: changes in the gutter, blame of the line, the diff of a file against its last
      commit ([ADR 0066](adr/0066-git-through-its-command-line.md)): through the `git` program
      (Preferences > Git), not a library; a margin of its own marks the lines added, modified
      and deleted since HEAD (Preferences > Margins > Git changes), read again after edits and
      when the window comes back to the front; Tools > Git blames the caret's line with the
      document's text as it is, goes to the next and the previous change, and compares with the
      last commit through Compare to Git HEAD; `birchpad-git` holds it without a window, its
      blame fuzzed as `git_blame`
- [x] 7.7 Integrated terminal in a panel ([ADR 0067](adr/0067-integrated-terminal.md)):
      View > Terminal (``Ctrl+` ``) shows the shell (`terminal.shell`, the system's when empty)
      under the documents, started in the active document's folder, on `alacritty_terminal`'s
      pseudo-terminal and emulator; the theme's colors, the shell's keys before the window's
      shortcuts, pastes, scrollback (`terminal.scrollback`), and Enter to start a new shell
      once it exited; `birchpad-terminal` holds it without a window, what programs write fuzzed
      as `terminal_output`. Several terminals in tabs and copying with the mouse are left for
      later
- [x] 7.7a Update checks through Birchpad's Update API, with install statistics
      ([ADR 0064](adr/0064-update-api-and-install-statistics.md)): the signed manifest read from
      `api.birchpad.ru` and verified as the one from GitHub, which stays the way when the API
      fails; the request says, unless `updates.statistics = false` or the `UpdateStatistics`
      policy, a random id of the copy, its version, channel, platform and PlantUML, and the
      privacy policy says so
- [x] Release 0.8.0: the workspace version (0.8.0, set when its branch opens, so that nightly
      builds are its pre-releases), `CHANGELOG.md` and this roadmap updated; the
      [release checklist](release-checklist.md) gone through

### Release 0.8.1: updates, workspace and refactoring

- [x] Dependabot's updates: gpui-kit 0.8.1
      ([#64](https://github.com/SecurityTrip/Birchpad/pull/64)) and the cargo group of nine
      ([#70](https://github.com/SecurityTrip/Birchpad/pull/70)): trash, zip, encoding_rs,
      tree-sitter-cmake, tree-sitter-powershell, tree-sitter-scala, uuid, ureq and
      unicode-width
- [ ] 7.7b Folder as Workspace works with files the way an IDE's project tree does: new file
      and folder, rename (F2), copy, cut and paste, duplicate, delete to the trash, move by
      dragging, show in the system's file manager; open documents follow their files when they
      are renamed or moved, and the tree follows what changes on disk
- [ ] 7.7c The updated libraries put to use: what gpui-kit 0.8 and the cargo group's releases
      bring looked through, and the code written around what they lacked replaced with their
      own features where that is simpler or faster, measured where it is about speed
- [ ] 7.7d The overloaded crates refactored: `birchpad` (the application, about 38 thousand
      lines) and `birchpad-editor` (about 10 thousand) split along their parts into crates of
      their own, without a window where they can be, so that they build faster and are tested
      apart; an ADR says where the lines run
- [ ] Release 0.8.1: the workspace version (0.8.1, set when its branch opens, so that nightly
      builds are its pre-releases), `CHANGELOG.md` and this roadmap updated; the
      [release checklist](release-checklist.md) gone through

### Release 0.9.0: large and remote files, other ways to edit

- [ ] 7.8 Multi-gigabyte files through memory mapping, read and searched without loading them
      whole
- [ ] 7.9 CSV and JSON viewers: a table and a tree beside the text, which follow its edits
- [ ] 7.10 Remote editing over SSH and SFTP: files and folders opened and saved on a server
- [ ] 7.11 Vim mode
- [ ] Release 0.9.0: the workspace version (0.9.0, set when its branch opens, so that nightly
      builds are its pre-releases), `CHANGELOG.md` and this roadmap updated; the
      [release checklist](release-checklist.md) gone through

### Release 1.0.0

- [ ] 7.12 Collaborative editing of a document by several users
- [ ] 7.13 Verified on real systems: input methods (Windows TSF, macOS, IBus/Fcitx), screen
      readers *(from Phase 1)*
- [ ] Release 1.0.0: the workspace version, `CHANGELOG.md` and this roadmap updated; the
      [release checklist](release-checklist.md) gone through with the draft's artifacts

## After 1.0.0

Not planned into a release yet.

- [ ] AI assistance, strictly opt-in and never pushed: off by default, and until the user
      turns it on in Preferences nothing of it shows (no button, menu item, prompt, tip or
      reminder) and nothing is sent anywhere; Birchpad never offers it unasked. Turned on, it
      runs only when the user asks (a command with no default shortcut), with the provider the
      user chooses, a local model too, and shows what is sent before it goes. The
      `OnlineFeatures` policy removes it entirely; an ADR records these rules first

## Distribution (runs alongside the phases)

| Channel | Built from | Cadence |
|---|---|---|
| Nightly | the next version's branch `vX.Y.Z` | nightly, when there are changes |
| Beta | `release/x.y` | every 1–2 weeks |
| Stable | tags on the release branch | every 4–8 weeks |

- [x] Per-user installer for Windows (Velopack) with silent updates from the signed manifest
      ([ADR 0020](adr/0020-signed-updates-and-nightly-builds.md)); delta updates, and installers
      that update themselves on macOS and Linux, are still to come
- Per-machine MSI (x64, ARM64) for enterprises, updated by IT; portable ZIP; winget and Scoop
  manifests
- [x] ed25519-signed update manifest
- Authenticode signing of the Windows executable, installers and MSI: the release workflow can
  sign through SignPath ([ADR 0021](adr/0021-authenticode-signing-with-signpath.md)), but
  SignPath Foundation did not take the project for now, so signing waits for a certificate;
  Apple Developer ID signing and notarization still to come
- ADMX/ADML templates generated from the policy list in `birchpad-config`
- [x] SBOM and build provenance attestations with every release
- [x] Background update checks (`notify`) and automatic updates (`auto`) with the signed
      manifest *(from Phase 1)*
- MSI for pre-release versions and for ARM64 *(from Phase 1)*

See [ADR 0005](adr/0005-distribution-and-updates.md).
