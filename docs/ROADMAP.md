# Birchpad roadmap

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
issues users opened on GitHub ([Release 0.2.0](#release-020)), then 0.3.0 (tests), 0.4.0
(languages and text), 0.5.0 (printing, tools and help) and 0.6.0 (plugins, documentation in
Russian, the last comparison with Notepad++), which closes the phase.

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
- [ ] 0.2.9 Release 0.2.0: the workspace version (0.2.0, so that nightly builds are its
      pre-releases), `CHANGELOG.md` and this roadmap updated; the
      [release checklist](release-checklist.md) gone through; the answered issues closed

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

- [x] 6.21 Dialogs and menus driven in a test window, every control: Preferences (62.8% of lines
      run), the Shortcut Mapper (76.9%), the column editor (64.1%), the path dialog, the Tools
      menu (62.4%), the banners, the menus and the window modes; the command catalog of
      `birchpad-commands` (63.2%)
- [ ] 6.22 The editor view (`editor/mod.rs`, 83.8%): smart highlighting (78.7%), the Edit menu's
      extras (81.2%), what clicks, drags and the keyboard do at the edges of the text, in
      wrapped and folded rows and with several carets
- [ ] 6.23 Files, folders and sessions when the file system refuses: a file changed or deleted
      outside (`disk.rs`, 71.6%), saving, renaming and moving that fail (`file_ops.rs`,
      `file_commands.rs`, about 84%), sessions with missing files and broken backups (84.0%),
      Folder as Workspace (78.1%), Find in Files over unreadable files; the configuration
      folders (`config/load.rs`, 86.1%) and the hand-off to a running instance
      (`cli/instance.rs`, 87.8%)
- [ ] 6.24 Updates and Plugins Admin against a local server: checks, downloads cut short, a
      wrong signature or checksum, an older version offered, installing and rolling back
      (`updates.rs` 76.2%, `update/install.rs` 87.6%, `extensions/admin.rs` 80.7%)
- [ ] 6.25 Property tests where there are none yet: the bidirectional row (logical and visual
      positions map one to one), comparing texts (the differences applied give the other
      text), search and replace against a plain reference implementation, keystrokes and key
      sequences that read back as written, syntax trees reparsed after an edit equal to a
      fresh parse, the column editor's numbering
- [ ] 6.26 Fuzzing what reads outside input and is not fuzzed yet: Hunspell dictionaries
      (`.aff`, `.dic`), search queries (regular expressions, the extended mode's escapes, Find
      in Files' filters), plugins' calls into the host with any arguments, Rhai scripts under
      their time and memory limits
- [ ] 6.27 End-to-end scenarios beyond the four of `tests/e2e.rs`: Find in Files and Replace in
      Files, recording and playing a macro, a second instance handing over its files,
      restoring unsaved text after the process is killed, a plugin installed and run, Export
      to HTML; the run counted in the coverage report, so that `main.rs` (0%) is measured
- [ ] 6.28 Performance in CI: the timing checks (`view/tests/performance.rs`, now ignored) and
      new ones for opening, searching and editing a 100 MB file and for a frame of the editor
      view run nightly in release, failing when a time grows past its limit
- [ ] 6.29 Mutation testing: `cargo-mutants` nightly over the crates without a window; the
      mutants that survive get tests, and the share caught gets a floor that only rises, as
      coverage does
- [ ] Release 0.3.0: the workspace version (0.3.0, set when its branch opens, so that nightly
      builds are its pre-releases), `CHANGELOG.md` and this roadmap updated; the
      [release checklist](release-checklist.md) gone through

### Release 0.4.0: languages and text

- [ ] 6.30 Code pages: EBCDIC 875 (Greek), whose standard table maps two bytes to one
      character; NEL as a line break in EBCDIC documents
- [ ] 6.31 Auto-completion: closing HTML and XML tags; API files for languages embedded in a
      document (scripts in HTML); cycling through a function's overloads in the call tip;
      Notepad++'s auto-completion options (ignore numbers, ignore case, completion from a number
      of characters per source); bundled API files for common languages, written for Birchpad
      (Notepad++'s are GPL)
- [ ] 6.32 User Defined Languages: the UDL editor dialog; folding (code and comment folders);
      number prefixes, suffixes, extras and ranges; comments that must start a line; nested
      delimiters; colors from the theme where the UDL leaves them unset; Open User Defined
      Language Folder, the UDL collection link
- [ ] 6.33 Right-to-left text: selections drawn piece by piece over mixed text; margins on the
      right in right-to-left mode; keys for Text Direction RTL and LTR; the direction
      remembered per document in sessions; very long rows reordered too
- [ ] 6.34 Localization: more interface languages, translated for Birchpad; Notepad++'s
      `nativeLang` XML files read from the user's own copy, not bundled
- [ ] Release 0.4.0: the workspace version (0.4.0, set when its branch opens, so that nightly
      builds are its pre-releases), `CHANGELOG.md` and this roadmap updated; the
      [release checklist](release-checklist.md) gone through

### Release 0.5.0: printing, tools and help

- [ ] 6.35 Printing: a print preview and page setup of Birchpad's own (margins, header and
      footer with Notepad++'s variables, colors or black on white, line numbers), printing the
      selection, Print Now; native printing where the platform offers it
- [ ] 6.36 Tools: hashes of files in a dialog with Copy, as Notepad++ shows them, besides the
      listing in a new document
- [ ] 6.37 Help menu: Command Line Arguments, home page, online manual and forum, Debug Info
      (copyable), Set Updater Proxy
- [ ] 6.38 The command line options Birchpad accepts but ignores: -notabbar, -systemtray,
      -alwaysOnTop, -loadingTime, -r (recursive wildcards), -quickPrint, -monitor,
      -openFoldersAsWorkspace, -x and -y (window position), -qn, -qt, -qf, -qSpeed (ghost
      typing), -settingsDir, -titleAdd, -udl, -pluginMessage
- [ ] Release 0.5.0: the workspace version (0.5.0, set when its branch opens, so that nightly
      builds are its pre-releases), `CHANGELOG.md` and this roadmap updated; the
      [release checklist](release-checklist.md) gone through

### Release 0.6.0: plugins, documentation, parity

Closing Phase 6.

- [ ] 6.39 A wider plugin API: settings of their own, files opened with the user's permission,
      a panel of their own controls, items in the text's context menu, events while typing and
      when the selection changes, work in the background; scripts' own commands and keys;
      Settings > Import plugin(s)
- [ ] 6.40 Compare as ComparePlus: lines padded to stay aligned side by side, differences
      within a line, comparing with the last save, the clipboard or Git; a navigation bar of the
      differences
- [ ] 6.41 JSON Viewer's tree panel (find a path, copy a value), sorting keys; XML Tools'
      XPath, XSD and DTD validation, escaping and unescaping, closing tags as typed
- [ ] 6.42 Hex Editor in place: the bytes edited in a hex view of the document itself
      (overwrite, insert, the caret shared with the text), columns and grouping, Find in hex
- [ ] 6.43 DSpellCheck's options (several languages at once, ignore rules, words in code by
      style); a dictionary bundled for English under a license compatible with Birchpad's
- [ ] 6.44 Online features under the `OnlineFeatures` policy: Plugins Admin's online list of
      plugins (available, updates, installed) with checksums; DSpellCheck's dictionary download

- [ ] 6.45 Documentation in Russian: the user's and the contributor's documents (README,
      CONTRIBUTING, SECURITY, PRIVACY, the code of conduct, writing plugins and scripts, the
      release checklist and this roadmap) as `*.ru.md` next to the English ones, linked from
      each other; from then on a change to one updates the other. ADRs and the changelog stay
      in English
- [ ] 6.46 A last comparison with Notepad++ 8's menus, Preferences and command line, with what
      is still missing moved to Phase 7 or written down
- [ ] Release 0.6.0: the workspace version (0.6.0, set when its branch opens, so that nightly
      builds are its pre-releases), `CHANGELOG.md` and this roadmap updated; the
      [release checklist](release-checklist.md) gone through

## Phase 7: beyond Notepad++

What Notepad++ does not have, in releases 0.7.0 to 0.9.0; closing the phase releases 1.0.0.
The steps follow the rules of Phase 6: clean-room where an editor is followed, one commit
each with its tests, docs and a `CHANGELOG.md` entry, its options in Preferences.

### Release 0.7.0: finding your way

- [ ] 7.1 Command palette: every command of the catalog by its name, with its shortcut, the
      recently used first
- [ ] 7.2 Fuzzy file finder: the files of the workspace's folders and of the recent files, by
      parts of their path
- [ ] 7.3 Syntax-aware selection (expand and shrink by the syntax tree) and sticky scroll (the
      enclosing scopes' first lines kept at the top)
- [ ] 7.4 Minimize to the system tray (Notepad++'s Preferences > MISC), once GPUI has a tray
      icon; until then, written down why not
- [ ] Release 0.7.0: the workspace version (0.7.0, set when its branch opens, so that nightly
      builds are its pre-releases), `CHANGELOG.md` and this roadmap updated; the
      [release checklist](release-checklist.md) gone through

### Release 0.8.0: development tools

- [ ] 7.5 LSP client: diagnostics, completion, hover, go to definition and references,
      rename, formatting; servers configured per language, none bundled
- [ ] 7.6 Git: changes in the gutter, blame of the line, the diff of a file against its last
      commit
- [ ] 7.7 Integrated terminal in a panel
- [ ] Release 0.8.0: the workspace version (0.8.0, set when its branch opens, so that nightly
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
