# Changelog

Notable changes to Birchpad. The format follows [Keep a Changelog](https://keepachangelog.com),
and versions follow [Semantic Versioning](https://semver.org). The release workflow takes a
version's release notes from its section here.

## [Unreleased]

### Added

- Color themes (Settings > Theme): the built-in Default and Dark, and theme files in the
  `themes` folder next to the settings. A dark theme darkens the whole window: tabs, panels,
  bars, menus and dialogs. Theme files are TOML and set only the colors they change; Notepad++
  themes (`.xml`) work as they are, with their colors matched to Birchpad's highlighting, and
  Settings > Import > Import Style Themes... copies them in. `appearance.theme` sets the theme
  Birchpad starts with.
- Settings > Style Configurator..., as in Notepad++: choose a theme and change its colors (the
  editor's global styles, the interface, and highlighting for all languages or one: foreground,
  background, bold, italic, underline), seen as they change. Save & Close keeps the theme in the
  `themes` folder; Cancel goes back.
- Settings > Preferences...: tab size, auto-indent, the current line, margins and the edge,
  smart highlighting, recent files, the ANSI encoding, the large file limit, change detection,
  backups and updates. Each change applies at once and is written to `settings.toml`, keeping
  its comments; settings an administrator's policy sets are shown but cannot be changed.
- Settings > Shortcut Mapper...: every command with its shortcuts, with a filter. Add Shortcut
  takes the keys pressed (without running them), Remove takes one off, Reset gives the
  defaults back; it warns when the keys run another command. Changes are written to
  `keymap.toml`, keeping its comments, and apply at once.

### Changed

- Nightly builds come from the branch of the next version (`v0.1.4` after 0.1.3) instead of
  `main`, so that the nightly channel gets what is being developed for the next release.

### Fixed

- A file of a Project Panel that is the workspace file's own folder was saved with an empty
  name and gone the next time the workspace was opened.

## [0.1.3] - 2026-10-07

### Added

- Search modes as in Notepad++: Extended (`\n`, `\t`, `\x41`, ...) and regular expressions
  with back-references and look-around; replacements with `$1`, `\1`, `${name}` and case
  conversion (`\U`, `\L`, `\u`, `\l`).
- The find panel has Find, Replace and Mark tabs: Count; Replace All in the selection or in all
  opened documents; Mark All with Bookmark line and Purge for each search, Clear All Marks and
  Copy Marked Text. Search > Mark... (Ctrl+M), Select and Find Next / Previous (Ctrl+F3,
  Ctrl+Shift+F3).
- The search results panel, as in Notepad++: Find All in Current Document and in All Opened
  Documents list the lines with matches; double-click or F4 and Shift+F4 go to them, F7 shows or
  hides the panel.
- Results can be removed from the search results panel one by one: Delete, or Remove in its
  right-click menu, takes out the selected line, document or search, and the heading counts
  what is left. The menu also copies the selected line or path, folds and unfolds everything,
  and clears the list.
- Incremental search, as in Notepad++ (Search > Incremental Search, Ctrl+Alt+I): a bar that
  finds the text while it is typed, Enter and Shift+Enter for the next and previous match,
  Match case, Whole word, and Highlight all, which marks and counts every match.
- Go Back and Go Forward (Search menu; Alt+Left and Alt+Right, Ctrl+- and Ctrl+Shift+- on
  macOS) return to the places the caret jumped from: other documents, search results, Go To.
  A closed file opens again.
- Side panels docked beside the documents, as in Notepad++: several on one side share it as
  tabs, the View and Edit menus open and close them, and the open ones come back on the next
  start. The first is Document List (View > Document List): the open documents with their
  paths; click to show one, middle-click to close it, click a heading to sort.
- Function List (View > Function List): the classes, functions, methods and sections of the
  document as a tree, from its syntax tree, for 28 languages including Markdown headings. It
  follows edits, highlights the definition the caret is in, goes to a definition on a click,
  and can be filtered, sorted by name and folded.
- Folder as Workspace (File > Open Folder as Workspace..., or drop a folder on the window): folders
  as trees beside the documents, kept current as files come and go; double-click to open a
  file. Its menu copies paths and names, runs Find in Files in a folder, and opens the file
  manager or a terminal there. A folder on the command line opens there too.
- Project Panels 1 to 3 (View > Project): projects of folders and files, kept in Notepad++'s
  `.workspace` files so that workspaces move between the two editors, built from each item's
  right-click menu and saved as soon as they change.
- Clipboard History (Edit > Clipboard History): the texts copied in Birchpad and other programs,
  newest first; double-click one to paste it.
- Character Panel (Edit > Character Panel): the 256 values of the document's code page with
  their characters and HTML entities; double-click to insert one.
- Document Map (View > Document Map): the whole document in miniature in its syntax colors,
  with the part on screen framed; click or drag on it to scroll there.
- Find in Files (Ctrl+Shift+F): Find All and Replace in Files in the files of a folder, with
  Notepad++'s filters (`*.rs *.toml !*.bak !\target !+\node_modules`), In all sub-folders, In
  hidden folders and Follow current doc. Open documents are searched with their unsaved changes
  and replaced in their tabs, where Undo works; other files keep their encoding, byte order mark
  and line endings. Binary files are skipped. Stop ends a long search with the results so far.
- A middle click on a tab closes it, as in Notepad++
  ([#9](https://github.com/SecurityTrip/Birchpad/issues/9)).
- Change history, as in Notepad++: a margin marks the lines changed since the document was
  opened, orange until they are saved, then green
  ([#11](https://github.com/SecurityTrip/Birchpad/issues/11)). `editor.change-history = "off"`
  turns it off.
- Split view by dragging a tab: dropped on the right or bottom third of the text, it opens in
  the other view beside or below ([#12](https://github.com/SecurityTrip/Birchpad/issues/12)).
  With Ctrl, or for the only tab, the document is cloned there; with Ctrl, a tab dropped in the
  other view is cloned too.

### Changed

- Ctrl+Alt+I opens Incremental Search, as in Notepad++; iNVERT cASE no longer has a shortcut
  (Notepad++ has none for it) and stays in Edit > Convert Case to.

### Fixed

- The close button of a tab closed a document with unsaved changes without asking, losing them;
  it asks now, as File > Close does.
- JSON keys are colored as keys, no longer like the strings
  ([#10](https://github.com/SecurityTrip/Birchpad/issues/10)), and TOML keys no longer take the
  color of table names.
- Dialogs that ask for a name or a path (the Project Panels' Add New Project, Rename and Add
  Folder, and the dialog used when the system's file dialog is not available) put the focus in
  their field with its suggestion selected; typing went nowhere until a click in the field.
- File > Load Session... and `-openSession` read a Notepad++ `session.xml` that an editor saved
  with a byte order mark; it was taken for Birchpad's format and refused as damaged.
- SQL numbers are colored as numbers; they never were, as the grammar's query looked for them
  with a pattern that matches no number.

## [0.1.2] - 2026-10-05

### Fixed

- Installed copies could not download any update: every package failed with "the response
  body is larger than request limit", because a package exactly as large as its manifest says
  was refused. Copies of 0.1.0 have the bug, so they need the 0.1.2 installer run once by hand;
  it updates them in place, keeping settings and sessions.

### Added

- The application icon: a strip of birch bark. On Windows the executable, its windows, the
  installer and the shortcuts show it; the macOS app and the Linux desktop file too.
- The MSI adds a Start menu shortcut, and Explorer shows Birchpad's name and version in the
  executable's details.

## [0.1.0] - 2026-10-05

The first release: a Notepad++-like editor for Windows, Linux and macOS.

### Editing

- Tabs with new, open, save, save as, save all and close commands; unsaved changes asked about
  on close; drag and drop; recent files; read-only files.
- Split view: two views side by side or stacked, the same document in both, tabs dragged
  between them, synchronized scrolling.
- Multiple carets and column (rectangular) selection with Alt+drag and Alt+Shift+arrows, into
  virtual space; Multi-select Next and All; the Column Editor with text or numbers.
- Line operations as in Notepad++: duplicate, move, join, split, sort (including by the columns
  of a rectangle), remove duplicates and empty lines, trim, case conversion, comments.
- Bookmarks, smart highlighting of the selected word, five token styles with Jump Up/Down.
- Find and replace in a panel, go to line or offset, zoom, word wrap, View > Show Symbol, the
  vertical edge and the current line.
- Long lines stay fast: typing into a 10 MB line takes under 2 ms per keystroke.

### Languages

- Syntax highlighting with tree-sitter for 50 languages, parsed incrementally in the
  background, with languages embedded in others (scripts and styles in HTML, code blocks and
  inline formatting in Markdown, HTML around PHP, regular expressions and JSDoc in
  JavaScript).
- Folding from the syntax tree, brace matching, auto-indent, and the Large File Restriction.

### Files and sessions

- Encodings: detection, UTF-8 with or without BOM, UTF-16, legacy code pages; reinterpreting
  and converting; Edit Anyway for files that do not decode exactly, after listing every byte
  that saving would change. Saving keeps permissions, alternate data streams and links.
- Sessions: quitting never asks about unsaved work, which is backed up and comes back on the
  next start; session files, and Notepad++'s `session.xml`.
- Files changed by other programs reload, keeping carets, bookmarks and folds; View >
  Monitoring follows a growing log.

### Windows, Linux and macOS

- A menu bar usable from the keyboard on Windows and Linux (Alt, F10, Alt+letter); the native
  menu bar on macOS.
- A Notepad++-compatible command line (`-n`, `-c`, `-p`, `-l`, `-ro`, `-multiInst`,
  `-nosession`, `-openSession`) and a single instance that takes the files of later launches.
- Layered settings with administrator policies (the Windows registry), and a portable mode.

### Installing and updating

- A per-user installer for Windows (x64 and ARM64) that updates itself, an MSI for
  administrators, portable ZIPs for Windows and Linux, and a universal macOS app.
- Updates are checked against an update manifest signed with ed25519, which protects against
  old or forged manifests whatever server or mirror serves them; checks can be turned off
  entirely (`updates.mode = "off"`).
- Nightly builds of `main` on the nightly channel.

[Unreleased]: https://github.com/SecurityTrip/Birchpad/compare/v0.1.3...HEAD
[0.1.3]: https://github.com/SecurityTrip/Birchpad/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/SecurityTrip/Birchpad/compare/v0.1.0...v0.1.2
[0.1.0]: https://github.com/SecurityTrip/Birchpad/releases/tag/v0.1.0
