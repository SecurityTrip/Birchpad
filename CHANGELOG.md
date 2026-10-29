# Changelog

Notable changes to Birchpad. The format follows [Keep a Changelog](https://keepachangelog.com),
and versions follow [Semantic Versioning](https://semver.org). The release workflow takes a
version's release notes from its section here.

## [Unreleased]

### Security

- GHSA-m665-v7pm-7grj (zip up to 9.0.4: an archive whose local headers disagree with its
  central directory can write files outside the folder it is extracted to). Birchpad itself,
  which unpacks plugins from the online catalog and the PlantUML components, requires zip
  9.0.5 or later. Velopack, the updater of per-user installs on Windows, is version 1.3.39, a
  pre-release and the only one with zip 9: its library in Birchpad uses zip 9.0.5 instead of
  8.6, so RUSTSEC-2026-0367 to 0369 no longer need exceptions either. Its `Update.exe`, which
  unpacks the update packages, is still built with zip 9.0.4; it only unpacks Birchpad's own
  packages, checked against the size and SHA-256 in the signed manifest first, and moves to a
  fixed zip with the next Velopack release that has one.

## [0.7.5] - 2026-10-29

### Changed

- The largest modules are split by what they do, each into a folder of smaller ones: the
  editor view (its commands, carets, editing, mouse and keyboard), the editor's layout and
  buffer, Preferences (a module per section), Find, the workspace and the pane, the settings
  of `birchpad-config` and the XML Schema validation of `birchpad-tools`. Nothing changes in
  how Birchpad works.
- What several parts of the window repeated is in one place: a command run from a button, a
  menu item or a panel shows what went wrong the same way everywhere (`Workspace::run`), and
  the bars across the window, the rows of lists and the text of errors take the theme's
  colors from `birchpad-editor`'s `widgets`.
- The parts of the window that need no workspace are crates of their own, each with its
  tests and only the dependencies it uses: the list to pick from of the command palette and Go
  to File (`birchpad-quick-pick`), the search results panel (`birchpad-search-results`),
  minimizing to the system tray (`birchpad-tray`) and printing through Windows' print dialog
  (`birchpad-print-dialog`) (ADR 0058).

## [0.7.3] - 2026-10-28

### Fixed

- The PlantUML preview scrolls across as well as down when a zoomed diagram is wider than it,
  with scrollbars both ways, Shift with the mouse wheel, and a wheel or touchpad that scrolls
  across ([#58](https://github.com/SecurityTrip/Birchpad/issues/58)).
- The messages above the text (a read-only file, a file opened with `-ro`, monitoring, a file
  that does not decode) are drawn in the theme's colors: on the dark theme they were light,
  with the theme's light text on them ([#57](https://github.com/SecurityTrip/Birchpad/issues/57)).

## [0.7.2] - 2026-10-28

### Added

- PlantUML diagrams are drawn on white in the preview, as they are made for paper, whatever
  the theme; the button above them (View > Preview > White Diagram Background) puts them on
  the theme's background instead, and the choice is kept for the diagrams shown later
  (`preview.plantuml-background`, Preferences > Preview)
  ([#53](https://github.com/SecurityTrip/Birchpad/issues/53)).
- Zoom in the PlantUML preview: Zoom Out, the zoom (a click sets 100%), Zoom In and Fit above
  the diagrams, Ctrl with the mouse wheel, and the same in View > Preview. From 25% to 400%;
  a zoomed diagram keeps its size, stays sharp, and the preview scrolls both ways. Fit, as
  before, shrinks a diagram wider than the preview to its width
  ([#52](https://github.com/SecurityTrip/Birchpad/issues/52)).
- Copy Diagram, Export Diagram as PNG... and Export Diagram as SVG... in a PlantUML diagram's
  context menu, and View > Preview for the first diagram. The SVG is PlantUML's; the PNG is
  drawn by Birchpad from it, twice its size by default (`preview.plantuml-png-scale`, 1 to
  8), on the diagram's background ([#51](https://github.com/SecurityTrip/Birchpad/issues/51)).
- Preferences > Preview: the PlantUML to run (`preview.plantuml`), the diagrams' background
  and the scale of PNG diagrams.

### Changed

- The PlantUML preview lists the mistakes PlantUML finds by the line of the document they are
  on ("Line 9: Syntax Error?"); a click puts the caret there. When the text changes while
  PlantUML is drawing, that run is stopped, so fast typing no longer leaves Java processes
  behind.

### Fixed

- Birchpad builds for Linux again: the release of 0.7.1 had no Linux package, as Minimize to
  system tray left the application without code for systems that have no tray.
- PlantUML diagrams that include other files by a relative path (`!include common.puml`)
  are drawn in the preview: PlantUML runs in the document's folder
  ([#50](https://github.com/SecurityTrip/Birchpad/issues/50)).

## [0.7.1] - 2026-10-28

### Added

- Preferences > MISC > Minimize to system tray, on Windows and macOS: the minimized window
  leaves the taskbar (the Dock) for an icon in the notification area (the menu bar), whose click
  brings it back. `-systemtray` now starts Birchpad there; on Linux it still starts minimized.
- Preferences > Editing > Non-printing characters: with View > Show Symbol > Show Non-Printing
  Characters on, each such character can show as its abbreviation (NBSP, ZWSP) or its code
  point (U+00A0) in a frame, taking the label's columns; framed and marked in its own cells,
  as before, by default.
- Preferences > Appearance > Dark Mode tone: Customized, with the dark interface's background,
  bars, borders, text, secondary and faint text, selected and hovered rows, accent and error
  colors each set in `#rrggbb` (`appearance.dark-tone-colors`); those left empty stay as they
  are.
- Preferences > Editing > Enable smooth font: text drawn with subpixel smoothing where the
  platform has it (Windows, Linux); off, as the platform smooths text by default.
- Preferences > Language > Treat backslash as escape character for SQL: `'it\'s'` is one
  string, as MySQL reads it; off, as standard SQL reads it, only a doubled quote stays inside.

## [0.7.0] - 2026-10-26

### Added

- View > Command Palette... (Ctrl+Shift+A) finds any command by a few letters of its name:
  the commands of the menus, the macros, the Run menu's command lines and the plugins' and
  scripts' commands, each with its shortcut. The commands you ran from it last come first;
  Preferences > Command Palette sets how many.
- Search > Go to File... (Ctrl+Shift+O) finds a file of the folders open as workspace, or a
  recent file, by parts of its path (`app work` finds `crates/app/src/workspace.rs`); Enter
  opens it. Preferences > Go to File chooses the files listed (by default not those in `.git`,
  `node_modules` and `target`), hidden ones too, and how many at most.
- Edit > Expand Selection (Ctrl+Alt+Up) grows the selection to the piece of syntax around it:
  a word, a string's text, the string, what is between brackets, the brackets, the statement,
  the function; Shrink Selection (Ctrl+Alt+Down) goes back. In plain text it grows to the
  word, the line, then everything.
- Edit > Auto-Completion lists the completion commands, with Word Completion (Ctrl+Enter, the
  document's words only) and Path Completion (Ctrl+Alt+Space, the names in the folder of the
  path typed before the caret).
- Edit > Line Operations > Sort Lines In Locale Order: letters first, then accents, then case.
- Edit > On Selection > Redact Selection turns the selected characters into █; Change Search
  Engine... opens Preferences > Search Engine. Edit > Column Mode... tells how to select
  columns.
- Edit > Read-Only Attribute of the File sets or clears the file's attribute, Set and Clear
  Read-Only for All Documents act on every tab; Set Read-Only is now Read-Only in Birchpad.
- Search > Copy Marked Text; the search results' Find in These Search Results... searches the
  lines a search found and lists what it finds as a search of its own.
- View > Show Symbol > Show Non-Printing Characters frames spaces that are not plain ones and
  marks characters without width; Show Control Characters & Unicode EOL (on by default) shows
  control characters and the line and paragraph separators as pictures, or as spaces.
- View > Zoom > Synchronize Across Views: turned off, each view zooms on its own.
- Tools > MD5 (SHA-1, SHA-256, SHA-512) > Generate... hashes a text you type, as a whole or
  line by line.
- Preferences > Appearance hides the menu bar (Alt or F10 shows it) and the status bar, and
  gives the dark interface a tone: red, green, blue, purple, cyan or olive.
- Preferences > Tab Bar: a maximum label length, alternate icons (a check mark, a pencil, a
  padlock), the inactive tabs' own color and the active tab's colored bar as options, pinning
  turned off or a pin button on every tab, and buttons on inactive tabs.
- Preferences > Editing: the caret's width (0 to 3 pixels, or a block) and blink rate, where
  wrapped rows start (aligned or indented), virtual space, scrolling beyond the last line,
  keeping the selection on a right click, copying and cutting the line with nothing selected,
  the selected text's own color, multi-editing and column selection to multi-editing,
  end-of-line labels without their box, and typing control characters prevented. The
  selected text can be dragged and dropped (Ctrl copies it); an option turns this off.
- Preferences > Margins: the fold margin's style (simple, arrow, circle tree, box tree, none),
  the border's width, a line number margin of constant width, and padding left, right and in
  Distraction Free mode.
- Preferences > New Document: files of ASCII alone take the new document's encoding, a new
  document opens at startup besides the others, and an untitled tab can be named after its
  first line. A folder dropped on the window can open its files instead of Folder as
  Workspace.
- Recent files: in a submenu, by their name only, cut to a maximum length, and kept when they
  are gone if you wish. The Language menu can be one list instead of letters, and leave out
  languages.
- Highlight Matching Tags in HTML and XML (with their attributes, and in comments, if asked);
  smart highlighting in the other view too.
- Preferences > Searching: when In selection is checked and the find field filled from the
  selection or the word at the caret, a monospaced find panel, the panel closing after Find
  All, Replace All in All Opened Documents confirmed, Replace staying on what it replaced,
  search results with a row for each match, Find in Files ignoring unsaved changes or always
  starting in the document's folder.
- Auto-Completion: which of Tab and Enter take an entry, a brief list, and Backspace
  unindenting. Delimiter: characters of your own that are part of words.
- Performance: large files can keep auto-completion, smart highlighting, brace matching and
  links, or turn word wrap off everywhere. Print can break pages at form feeds.
- Cloud & Link frames the link under the mouse; Multi-Instance remembers panels with
  `-nosession` if you wish.
- MISC: the document peeker in the Document Map, encoding detection turned off, Save All
  confirmed, symbolic links followed in Folder as Workspace, and extensions that open a file
  as a session or as a project workspace.
- The command line takes `-fullReadOnly` (every document read-only for the run) and
  `-fullReadOnlySavingForbidden` (nothing saved either).
- Sticky scroll, in Preferences > Editing: the first lines of the blocks around the top of the
  view stay there while you scroll, five at most by default; a click on one goes to it.

### Changed

- Edit > Set Read-Only is now Read-Only in Birchpad, beside Read-Only Attribute of the File.
- File > Recent Files leaves out files that are gone; Preferences > Files keeps them.
- Replace All in All Opened Documents asks first; a selection of 1024 characters or more
  checks In selection when the find panel opens; keys such as Ctrl+E no longer type control
  characters (Preferences > Searching, Editing).
- The ZIP library is version 9 (plugins, PlantUML and the online catalog's packages), with
  interprocess 2.4.6 and the Scala grammar 0.26.3.

## [0.6.0] - 2026-10-22

### Added

- Plugins can keep settings of their own, shown in Preferences > Plugins; read, write and open
  files once you allow them in Plugins Admin (Allow Files); fill their panel with buttons, text
  fields and check boxes; add items to the text's context menu; hear the text change and the
  selection move; receive the command line's `-pluginMessage`; and work in the background
  without holding up the window.
- Scripts can declare commands (`// command: <function> <title>`): Plugins > Scripts shows them
  in a submenu of the script, and Settings > Shortcut Mapper > Plugin Commands gives them keys,
  as it does the plugins' commands.
- Settings > Import > Import plugin(s)... installs several plugins at once.
- Plugins > Compare works as ComparePlus: blank rows keep the lines both documents have side
  by side, what differs within a changed line is marked too, the differences are marked along
  the scrollbar, and the documents are compared again a moment after either changes. Diff
  since Last Save, Compare to Clipboard and Diff to Git HEAD compare the document with its
  file, the clipboard or Git's last commit. Preferences > Compare turns each of these on or
  off.
- Plugins > JSON Viewer > Show JSON Viewer: the document's JSON as a tree beside it. A click
  selects a value in the text; a path (`$.store.book[0].title`) finds one; Copy Value, Copy Key
  and Copy Path copy it (with the panel closed, the value at the caret). Sort JSON Keys formats
  the JSON with every object's keys in order.
- Plugins > XML Tools: Evaluate XPath Expression... lists the nodes an expression selects;
  Validate Now checks the document against its DTD or XML Schema (the one it names, or one
  you choose) and goes to the first problem; Escape and Unescape Selected Text; Tag Auto-Close.
- Plugins > Hex Editor > View in HEX shows the document's bytes in place of its text and edits
  them there: two digits a byte, over the caret or before it (Insert), characters in the text
  column, the caret shared with the text. Find in HEX finds bytes or text in quotes.
  Preferences > Hex Editor sets the bytes in a row, the groups and capitals.
- Plugins > Spell Checker checks US English out of the box, with a dictionary made from SCOWL.
  Preferences > Spell Checker checks several languages together, chooses which words are left
  alone (with digits, in capitals, with a capital inside or at the start, with underscores,
  single letters, addresses) and what of code is checked (comments, strings, both, or all).
- Plugins Admin reads an online list of plugins (Refresh List): it installs those not installed
  yet and offers updates of those that are, each download checked against the list's SHA-256.
  Plugins > Spell Checker > Download Languages... downloads dictionaries from the same list.
  Neither goes online while online features are turned off (`OnlineFeatures`).
- Documentation in Russian: README, CONTRIBUTING, SECURITY, PRIVACY, the code of conduct,
  writing plugins and scripts, the release checklist and the roadmap as `*.ru.md` beside the
  English ones, each linking to the other.
- Notepad++ 8's command-line options `-noPlugin` (as `-nopluginsupport`), `-L<code>` (the
  interface in that language this time: `-Lde`, `-Lru`), `-notepadStyleCmdline` and `-z` (to
  stand in for Windows Notepad: Notepad's path is skipped and the rest is one file name with
  spaces), and `--help` (Help > Command Line Arguments... at start).

### Changed

- Plugins > Hex Editor > View in Hex is now Hex Dump in New Document.
- The plugin API is version 0.2 (`api = "0.2"`): plugins built for 0.1 must be rebuilt against
  the new `birchpad.wit`, where what 0.1 had is unchanged.

## [0.5.0] - 2026-10-21

### Added

- File > Print... shows a print preview of Birchpad's own: the pages one at a time, as they
  will be printed, with Page Setup... and Print.... A selection is printed alone, with its
  line numbers.
- Preferences > Print: paper and orientation, margins, the font's size, line numbers, the
  colors (WYSIWYG, Invert, Black on white, No background color), and a header and a footer
  of three parts each with Notepad++'s variables (`$(FULL_CURRENT_PATH)`, `$(FILE_NAME)`,
  `$(CURRENT_DIRECTORY)`, `$(CURRENT_PRINTING_PAGE)`, `$(SHORT_DATE)`, `$(LONG_DATE)`,
  `$(TIME)`).
- On Windows, printing goes through Windows' print dialog, and File > Print Now prints on the
  default printer at once. On macOS and Linux the same pages print from the browser.
- Tools > MD5, SHA-1, SHA-256, SHA-512 > Generate from Files... shows the hashes in a window,
  as Notepad++ does, with Copy and Open in New Document.
- The Help menu of Notepad++: Command Line Arguments..., Birchpad Home, Online Manual,
  Community (Forum), Set Updater Proxy... (`updates.proxy`, the proxy updates go through) and
  Debug Info... (what a bug report needs, with Copy).
- The command line options of Notepad++ that Birchpad ignored now work: `-notabbar`,
  `-alwaysOnTop`, `-systemtray` (starts minimized), `-x` and `-y`, `-titleAdd`,
  `-settingsDir`, `-loadingTime`, `-quickPrint`, `-monitor`, `-udl`, `-r` with wildcards (which
  now open every file they match), ghost typing with `-qt`, `-qf` and `-qSpeed`, and
  `-nopluginsupport`.
- PlantUML previews work without installing anything: when no PlantUML is found, the preview
  offers Install PlantUML, which downloads PlantUML and the Java that runs it (about 50 MB),
  checks them and keeps them in Birchpad's data folder. Diagrams are laid out without Graphviz.
  `preview.plantuml` still names a PlantUML of your own.

### Changed

- Links in the text open with Ctrl+click (Cmd+click on macOS); a double click selects a word
  of them, as anywhere in the text (#45). Preferences > Cloud & Link > Open them with a double
  click too (`links.double-click`) brings back Notepad++'s double click.
- Every crate is tested through its public interface: the tests in `tests/integration`, one
  test binary per crate, and `birchpad` is a library and a program that runs it (ADR 0043).

### Fixed

- In a very long line whose start has combining marks, a click or a move to a column landed
  after the marks rather than before them.

## [0.4.0] - 2026-10-19

### Added

- Markdown preview, as in IntelliJ IDEA: a Markdown document shows its text and the rendered
  page side by side. The buttons at the top right of the document and View > Preview show
  the text alone, both, or the preview alone; the choice is remembered. Links open in the
  browser or, for files, in Birchpad; images are read from files next to the document, never
  from the web.
- PlantUML preview in the same way for `.puml` files, drawn by the PlantUML installed on the
  computer: the `plantuml` command, or a `plantuml.jar` named in `preview.plantuml`.
- EBCDIC 875 (Greek) in Encoding > Character Sets > EBCDIC.
- EBCDIC files whose lines end with NEL, as on IBM mainframes, open with their lines broken
  and are saved with NEL again; the status bar shows "EBCDIC (NEL)".
- Auto-Completion closes HTML and XML tags as their `>` is typed (Preferences >
  Auto-Completion > Close HTML and XML tags, off by default as in Notepad++).
- Auto-Completion's options from Notepad++: Function completion and Word completion, each
  from its own number of characters, Ignore case and Ignore numbers.
- A call tip shows one overload at a time with "1 of 3"; Up and Down go through them.
- The API file of a language embedded in the document applies inside it: JavaScript's in an
  HTML `<script>`, CSS's in `<style>`.
- API files of Birchpad's own for 15 languages (Bash, C, C++, C#, CSS, Go, HTML, Java,
  JavaScript, PHP, PowerShell, Python, Rust, SQL, TypeScript): keywords and common functions
  with their parameters, in the completion list and call tips. A file of the same name in
  `autoCompletion` takes the place of Birchpad's.
- User Defined Languages: folding at the language's folding words and symbols, in code and
  in comments; numbers with prefixes, extra characters, suffixes and ranges; line comments
  that must start a line; delimiters that hold other delimiters, comments, keywords,
  operators or numbers, as their nesting says.
- User Defined Languages take the theme's colors where the language leaves them unset.
- Language > User Defined Language > Open User Defined Language Folder... and Notepad++
  User Defined Languages Collection.
- Language > User Defined Language > Define Your Language...: a dialog that writes a user
  defined language, new or one already there: extensions, keywords, comments, operators,
  delimiters, folding and numbers; the colors it does not edit stay as they were.
- Right-to-left text: a selection over mixed directions is drawn in pieces; in Text
  Direction RTL the margins are at the right; Ctrl+Alt+Shift+R and Ctrl+Alt+Shift+L switch
  the direction; sessions keep it per tab (Notepad++'s sessions too); very long rows are
  reordered as well.
- The interface in German.
- `appearance.native-lang` (Preferences > Appearance > Notepad++ translation file): the
  menu names and command titles of a Notepad++ translation you have, read from your own
  copy, come before Birchpad's.

### Changed

- Dependency updates: sha2 0.11.1.
- No tests in `src`: every crate's unit tests are in its `tests/unit` folder, which `src`
  includes for them.

## [0.3.0] - 2026-10-18

### Added

- Update from the window that announces an update: Update downloads it, checks it against the
  signed manifest, installs it and restarts with your documents, with a progress bar in place
  of the buttons; Help > Check for Updates keeps its window open with the same bar. A download
  no window shows (automatic updates) has a bar in the status bar, whose tooltip says how much
  has arrived or that it stalled.
- "Don't show again" in the window that announces an update turns update checks off.
- File > Export to HTML (`file.export-html`) takes a `path` argument, for macros, plugins and
  scripts: the page is written there without asking.

### Fixed

- Update downloads over a slow connection no longer fail after 30 seconds: a download fails
  only when nothing arrives for a minute, and says so.
- Compare marks a line removed at the start of a text against the right line of the other one;
  with lines added after it, the removal could be shown a few lines further down.

### Security

- RUSTSEC-2026-0367 to 0369 (zip 8.6, through Velopack, the updater of per-user installs on
  Windows) do not affect Birchpad: they concern Unix permission bits and symbolic links of
  attacker-controlled archives, and Velopack only extracts Birchpad's own update packages,
  checked against the signed manifest first. Velopack moves to zip 9 once it ships one in a
  release.

### Changed

- Tests run in about a quarter of the time (cargo-nextest, the test window without animations)
  and cover much more: the editor at the edges of the text, files and sessions when the file
  system refuses, updates and Plugins Admin against a local server, property tests, end-to-end
  scenarios counted in the coverage report; dictionaries, searches, plugin calls and scripts
  are fuzzed; timing checks and mutation testing run every night.
- Fewer dependencies: x11rb 0.13 and similar 2.7, the versions GPUI already builds, and
  yaml-rust2 without its encoding detection; 13 crates fewer in the build.

## [0.2.5] - 2026-10-15

### Changed

- Dependency updates: embed-resource 3.0.12, gpui-kit 0.7.3, interprocess 2.4.5, objc2 0.6.5,
  proptest 1.12, serde_json 1.0.154, smallvec 1.16.3, tree-sitter 0.27.1, tree-sitter-erlang
  0.21, tree-sitter-php 0.25, x11rb 0.14.

### Security

- wasmtime 49.0.3, with the fixes for RUSTSEC-2026-0353 to RUSTSEC-2026-0364: panics and
  unenforced limits in the component model that a plugin could cause in the plugin host.

### Fixed

- Dragging a selection past the left or right edge of the editor scrolls the text sideways, so
  the selection goes on past the visible part of a long line ([#34](https://github.com/SecurityTrip/Birchpad/issues/34)).
- In a low window the menus stay below the menu bar and scroll, instead of being pushed up over
  it, where the click that opened a menu chose its first item ([#33](https://github.com/SecurityTrip/Birchpad/issues/33)).
- Plugins > XML Tools > Pretty Print XML keeps a byte order mark right before the declaration;
  a line break between them made the result not well formed.

## [0.2.0] - 2026-10-14

### Added

- Pinned tabs (View > Tab > Pin Tab, or the pin on a tab): they stay at the start of the tab
  bar, and File > Close Multiple Documents > Close All but Pinned leaves them
  ([#26](https://github.com/SecurityTrip/Birchpad/issues/26)).
- Tab colors: View > Tab > Apply Color 1 to 5 and Remove Color. Themes get `tab-color-1` to
  `tab-color-5`; a Notepad++ theme's tab colors are imported with it.
- View > Tab: 1st to 9th Tab, Last Tab, Move Tab Forward (Ctrl+Shift+PageDown) and Backward
  (Ctrl+Shift+PageUp), Move Tab to Start and to End.
- File > Close Multiple Documents: Close All but This, Close All but Pinned, Close All to the
  Left, Close All to the Right, Close All Unchanged ([#25](https://github.com/SecurityTrip/Birchpad/issues/25)).
- The tab's context menu: close it or groups of tabs, save, print, pin, color, move or clone it
  to the other view ([#27](https://github.com/SecurityTrip/Birchpad/issues/27)).
- The document switcher: Ctrl+Tab and Ctrl+Shift+Tab go through the view's documents, most
  recently used first, in a list shown while Ctrl is held.
- The document peeker: hovering over a tab shows the start of its document.
- Preferences > Tab Bar (`[tabs]` in `settings.toml`): vertical tabs, tabs on several rows,
  smaller tabs, locked tabs, the close button on each tab, closing a tab with a double click,
  the document peeker, and the document switcher with or without most recently used first.
- A double click on the empty part of the tab bar opens a new document ([#28](https://github.com/SecurityTrip/Birchpad/issues/28)).
- Pinned tabs and tab colors are kept in sessions, and read from Notepad++'s `session.xml`.
- The Window menu: the documents of the active view, Sort By (name, path, type, content
  length or modified time, either way), and Windows..., a list of the view's documents with
  their path, type and size, sorted by any column, where the selected ones are activated,
  saved or closed and Sort Tabs gives the tabs the order shown.
- File > Open Containing Folder (in the file manager, a terminal or Folder as Workspace), Open
  in Default Viewer, Reload from Disk, Save a Copy As (in the document's encoding, leaving it
  as it is), Rename (the file in its folder, or the name of an untitled document, kept in
  sessions) and Move to Recycle Bin, also in the tab's context menu. Reload from Disk is on
  Ctrl+R (Cmd+R on macOS) ([#29](https://github.com/SecurityTrip/Birchpad/issues/29)).
- File > Recent Files > Restore Recent Closed File (Ctrl+Shift+T) and Open All Recent Files.
- Edit > Copy to Clipboard: the document's full path, file name and folder, also in the tab's
  context menu.
- View > Always on Top, Toggle Full Screen Mode (F11), Post-It (F12: the text alone, without
  menus, tabs, panels and status bar) and Distraction Free Mode (full screen, the text in a
  column in the middle); a button in the corner leaves the last two.
- View > Move/Clone Current Document > Move to New Instance and Open in New Instance: the file
  opens at the caret in another Birchpad of its own.
- The toolbar under the menus, with Notepad++'s buttons: they run their commands, show
  switches that are on pressed, and have small or large icons (Preferences > Toolbar). Which
  buttons it has, in what order, is `toolbar.buttons` in `settings.toml`; it can be hidden.
- Edit > Indent > Increase and Decrease Line Indent; Tab on a selection of several lines
  indents them and Shift+Tab takes one level off, by a tab or the tab width in spaces.
- Edit > Insert > Date Time (short, long, customized), in Notepad++'s format letters; the
  customized format and the order are in Preferences > Multi-Instance & Date.
- Edit > Set Read-Only and Clear Read-Only Flag.
- Edit > Paste Special: copy, cut and paste binary content (NUL characters kept), and paste
  the clipboard's HTML or RTF as text.
- Edit > On Selection: open the file the selection names, show its folder, or search for it
  on the Internet with the engine of Preferences > Search Engine (DuckDuckGo, Google, Bing,
  Yahoo!, Stack Overflow or an address of one's own).
- The text's context menu, on a right click: cut, copy, paste, styles of tokens, case,
  opening the selected file, searching the Internet, comments. Its items are
  `context-menu.toml` next to the settings, opened by Settings > Edit Popup ContextMenu, as
  Notepad++'s `contextMenu.xml`: any command with its arguments, own labels, submenus
  ([#30](https://github.com/SecurityTrip/Birchpad/issues/30)).
- Search > Find (Volatile) Next and Previous (Ctrl+Alt+F3, Ctrl+Alt+Shift+F3): as Select and
  Find, leaving the find panel's text as it is.
- Search > Find in Projects: a tab of the find panel that searches the files of the chosen
  project panels.
- Search > Find Characters in Range: the next non-ASCII or ASCII character, or one with a code
  in one's own range, up or down.
- Search > Change History: Go to Next and Previous Change, Clear All Changes.
- The search results' context menu: Open Selected Pathname, Open All, Wrap Long Lines and
  Purge for Every Search (both remembered); its texts are translated.
- View > Hide Lines (Alt+H): the selected lines are hidden, the line above them marked; a click
  in its margin, or View > Show Hidden Lines, shows them again, and so does a caret that lands
  in them.
- View > View Current File in the default browser, Firefox, Chrome, Edge or Safari.
- View > Summary: the document's path, times, characters without line endings, words, lines
  and length, and the selection's.
- Clickable links: addresses (http, https, ftp, ftps, file, mailto and schemes of one's own)
  are underlined and open with a double click or Ctrl+click (Preferences > Cloud & Link).
- Ctrl+double click selects the text between the delimiters around the click (Preferences >
  Delimiter: the characters, and whether they may be on other lines).
- Run: Notepad++'s own entries (Get PHP Help, Google Search, Wikipedia Search, Open File in
  Another Instance) are built in, before the saved command lines, which no longer start with
  copies of them.
- Macro > Trim Trailing Space and Save (Alt+Shift+S), Notepad++'s built-in macro.
- Macros record searches with their text and options (Find, Find Next, Replace, Replace All),
  so that they play back the same whatever the find panel holds then.
- Shortcut Mapper: shortcuts of several keys pressed one after the other (Ctrl+K Ctrl+C), for
  every platform or this one only (shown next to them); Macros and Run Commands tabs to give
  saved macros and command lines shortcuts, or delete them.
- Preferences > Appearance > Interface, Notepad++'s Dark Mode: menus, tabs, panels and dialogs
  light or dark whatever the editor's theme, or as the system is.
- Themes have a font (`[font] family`, `size`), set in Style Configurator and read from a
  Notepad++ theme's Default Style; the text is drawn in it.
- Style Configurator saves a Notepad++ theme back into its XML file, the rest of the file left
  as it was; more of Scintilla's style names find their highlight.
- Preferences > Language: each language's own tab size and spaces (`[languages.<id>]`).
- Preferences > New Document: the encoding, line ending and language of new documents.
- Preferences > Default Directory: Open and Save As start in the current document's folder,
  the folder last used, or one of one's own.
- Preferences > Backup: a copy of the file before each save, `.bak` next to it or one per save
  with the time in a folder; Remember inaccessible files keeps files that cannot be read now in
  the session.
- Preferences > Cloud & Link: settings in a folder of one's choice (`cloud.folder`), from the
  next start.
- Preferences: only the file name in the title bar; exit on closing the last tab.
- Preferences > Multi-Instance (`instances.mode`): every launch, or a launch opening a session,
  can start a Birchpad of its own instead of handing its files to the running one.

- "Edit with Birchpad" in the Windows Explorer's context menu of files: the per-user installer
  adds it for the user, also when an earlier version updates (unless the user took it away in
  Preferences), and the MSI for the machine (its `ExplorerMenu` feature, which administrators
  can leave out), and both take it away when uninstalled; Preferences > Files adds it for any
  copy, the portable one too, or takes it away
  ([#24](https://github.com/SecurityTrip/Birchpad/issues/24)).

- Birchpad as the default editor of text files (.txt, .json, .xml, .md, .log and others): at
  startup it offers to set itself up (Set Up, Not Now, Don't Ask Again;
  `files.offer-default-app`), and Settings > Make Birchpad the Default Editor does it at any
  time. Windows lets no program make itself the default, so Birchpad registers for those types
  and opens Default apps, where the user picks it; on Linux it becomes the default with
  `xdg-mime`; on macOS Finder lists it under Open With
  ([#32](https://github.com/SecurityTrip/Birchpad/issues/32)).

### Fixed

- Preferences: the sections at the bottom of the list could not be reached, and the rows of a
  long section were cut off; both lists scroll now. The Language section's fields no longer run
  past the dialog's edge, where clicking Set closed the dialog.
- A settings file holding only comments keeps them on top when Preferences adds a setting,
  instead of moving them to its end.

### Changed

- Ctrl+Tab goes to the most recently used document instead of the next tab, as in Notepad++;
  Ctrl+PageDown and Ctrl+PageUp still go to the next and previous tab, and turning the
  document switcher off in Preferences > Tab Bar gives Ctrl+Tab its old meaning.
- The localization is a crate of its own, `birchpad-i18n`, and the crates without a window make
  their messages in the interface's language, instead of the application translating their
  English afterwards; the rest of JSON validation's messages are translated now too.

## [0.1.5] - 2026-10-12

### Added

- The Plugins menu, with Birchpad's own versions of popular Notepad++ plugins.
- Plugins > JSON Viewer: Format JSON (one value per line, with the editor's indentation),
  Compress JSON and Validate JSON, for the selection or the whole document. Numbers and strings
  stay as written; an error puts the caret where it is and says what is wrong.
- Plugins > XML Tools: Pretty Print XML, Linearize XML and Check XML Syntax, likewise.
- Plugins > Hex Editor: View in Hex opens the document's bytes, in its encoding, as rows of 16
  in a new document; after editing them, Apply Hex to Document puts them back into the
  document as one undo step.
- Plugins > Compare: compares the active document with the other view's, or with the tab
  before it, which it puts side by side; lines added, removed and changed are colored across
  the line, Next and Previous Difference go through them, Clear Results takes the colors off.
  Spaces and case can be ignored. Themes get `compare-added`, `compare-removed`,
  `compare-changed` and `spell-error` colors.
- Plugins > Spell Checker, as DSpellCheck: Spell Check Document Automatically underlines
  misspelled words (in code, only in comments and strings); Find Next and Previous Misspelling;
  Suggestions in the completion list; Add Word to Dictionary; Dictionary Language. Dictionaries
  are Hunspell's `.aff` and `.dic`, from the `dictionaries` folder next to the settings (Open
  Dictionaries Folder) or the system's; none comes with Birchpad.
- Plugins > Scripts: scripts in Rhai, saved as `.rhai` files in the `scripts` folder next to
  the settings (Open Scripts Folder), run over the active document. A script reads and edits
  the text and the selections (its edits are one undo step), runs commands, shows messages and
  fills the Plugin Panel; it cannot touch files or the network, and stops if it runs too long.
  See [writing plugins and scripts](docs/plugins.md).
- Plugins as WebAssembly components ([ADR 0036](docs/adr/0036-webassembly-plugins.md)): the
  plugins of the `plugins` folder next to the settings load when Birchpad starts (or on
  Plugins > Reload Plugins), each with a submenu of its commands in the Plugins menu. A plugin
  works on the active document as a script does, hears the events it asks for (a document
  opened, saved, closed or activated) and fills the Plugin Panel, whose lines it hears clicked.
  It cannot touch files or the network; one that runs too long or takes too much memory is
  stopped. Plugins that the administrator's `PluginInstall` / `AllowedPlugins` policies do not
  allow are not loaded.
- Plugins > Plugins Admin...: the installed plugins with their version, description and state;
  Install... copies a plugin's folder in from its `plugin.toml` (installing it again updates
  it), Remove takes it out, Turn Off and Turn On keep it installed but stopped. The
  administrator's policy is shown, and installing against it is refused.
- The whole interface in Russian: command titles (also in the Shortcut Mapper), messages,
  errors, questions, the status bar, the find panel and the search results, with Russian plural
  forms. Interface languages are YAML files (`en.yaml`, `ru.yaml`) where every text has a key.
- The `UserScripts` policy (`plugins.scripts`): administrators can turn user scripts off, as
  `PluginInstall` and `AllowedPlugins` control plugins.

### Changed

- The Russian translation moved from `ru.toml`, keyed by the English text, to `ru.yaml`,
  keyed like `en.yaml` ([ADR 0033](docs/adr/0033-localization.md)).

## [0.1.4] - 2026-10-12

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
- The Tools menu: MD5, SHA-1, SHA-256 and SHA-512 of the selection, copied to the clipboard, or
  of files, listed in a new document as `sha256sum` does; Base64 Encode and Decode, URL Encode
  and Decode, ASCII to HEX and HEX to ASCII on every selection, as one undo step.
- The Run menu, as in Notepad++: Run... runs a command line with variables (`$(FULL_CURRENT_PATH)`,
  `$(CURRENT_DIRECTORY)`, `$(FILE_NAME)`, `$(NAME_PART)`, `$(EXT_PART)`, `$(CURRENT_WORD)`,
  `$(CURRENT_LINE)`, `$(CURRENT_COLUMN)`, `$(CURRENT_LINESTR)`, `$(NPP_DIRECTORY)`,
  `$(NPP_FULL_FILE_PATH)`, `$(SYS.NAME)`) through the system's shell, or opens a web address in
  the browser; Save keeps it by name in `run.toml`, listed in the menu.
- DOS (OEM) code pages in Encoding > Character Sets: OEM-US (437), 737, 775, 850, 852, 855,
  857, 858, 860, 861, 862, 863, 864, 865 and 869, and EBCDIC 037, 1140, 500 and 1026; `cp437`
  and `IBM437` name them in settings and on the command line.
- Hebrew and Arabic show in their order, by the Unicode Bidirectional Algorithm, with carets and
  clicks where they belong; Edit > Text Direction RTL makes a view run right to left, its rows
  ending at the right edge, and Text Direction LTR goes back.
- File > Print... (Ctrl+P) opens the document, in its colors with line numbers, in the browser's
  print dialog; File > Export to HTML... saves the same page.
- The interface in Russian: menus, dialogs, Preferences and the status bar follow the system's
  language, or `appearance.language` (`en`, `ru`, `auto`) in Preferences > Appearance.
- Notepad++'s User Defined Languages: UDL files in `userDefineLangs` next to the settings are
  listed in Language > User Defined Language (Import... copies them in), color their keywords,
  comments, operators, numbers and delimiters in the language's own colors, and apply to files
  with their extensions.
- Auto-completion, as in Notepad++: brackets and quotes get their closing partner as they are
  typed (typing it steps over it, Backspace deletes an empty pair); a list of the document's
  words and the language's keywords shows while typing (Ctrl+Space asks for it); a function's
  parameters show after its opening bracket (Ctrl+Shift+Space). Keywords and functions come from
  Notepad++'s API files, put in `autoCompletion` next to the settings. Settings in Preferences >
  Auto-Completion.
- The Macro menu, as in Notepad++: Start/Stop Recording (Ctrl+Shift+R) records the commands run
  and the text typed, Playback (Ctrl+Shift+P) plays them, Run a Macro Multiple Times... plays a
  number of times or until the end of the file, and saved macros (`macros.toml`) are listed in
  the menu.

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

[Unreleased]: https://github.com/SecurityTrip/Birchpad/compare/v0.7.5...HEAD
[0.7.5]: https://github.com/SecurityTrip/Birchpad/compare/v0.7.3...v0.7.5
[0.7.3]: https://github.com/SecurityTrip/Birchpad/compare/v0.7.2...v0.7.3
[0.7.2]: https://github.com/SecurityTrip/Birchpad/compare/v0.7.1...v0.7.2
[0.7.1]: https://github.com/SecurityTrip/Birchpad/compare/v0.7.0...v0.7.1
[0.7.0]: https://github.com/SecurityTrip/Birchpad/compare/v0.6.0...v0.7.0
[0.6.0]: https://github.com/SecurityTrip/Birchpad/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/SecurityTrip/Birchpad/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/SecurityTrip/Birchpad/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/SecurityTrip/Birchpad/compare/v0.2.5...v0.3.0
[0.2.5]: https://github.com/SecurityTrip/Birchpad/compare/v0.2.0...v0.2.5
[0.2.0]: https://github.com/SecurityTrip/Birchpad/compare/v0.1.5...v0.2.0
[0.1.5]: https://github.com/SecurityTrip/Birchpad/compare/v0.1.4...v0.1.5
[0.1.4]: https://github.com/SecurityTrip/Birchpad/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/SecurityTrip/Birchpad/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/SecurityTrip/Birchpad/compare/v0.1.0...v0.1.2
[0.1.0]: https://github.com/SecurityTrip/Birchpad/releases/tag/v0.1.0
