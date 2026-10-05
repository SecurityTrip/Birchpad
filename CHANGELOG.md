# Changelog

Notable changes to Birchpad. The format follows [Keep a Changelog](https://keepachangelog.com),
and versions follow [Semantic Versioning](https://semver.org). The release workflow takes a
version's release notes from its section here.

## [Unreleased]

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

[Unreleased]: https://github.com/SecurityTrip/Birchpad/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/SecurityTrip/Birchpad/releases/tag/v0.1.0
