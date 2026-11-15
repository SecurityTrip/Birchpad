# ADR 0069: The application and the editor split along their parts

- Status: accepted, extends [ADR 0058](0058-parts-of-the-window-in-crates-of-their-own.md)
- Date: 2026-11-14
- Builds on [ADR 0041](0041-crate-layout.md) (the layers) and
  [ADR 0043](0043-tests-through-the-public-interface.md) (a crate's tests in its `tests` folder).

## Context

After 0.8.0 the application crate `birchpad` had 38 559 lines and `birchpad-editor` 10 038. A
change anywhere in the application compiled all of it again, its tests were one binary that
needs the whole window, and parts that never draw (running PlantUML and reading its mistakes,
the list of Preferences' settings) could only be tested from that binary. The editor crate held
the document as well as its view, so the parts of the window that only read documents or the
theme (the search results, the list to pick from) waited for the whole editor to build.

Roadmap 7.7d asks for both crates to be split along their parts, without a window where that can
be, and for a record of where the lines run.

## Decision

### What makes a part a crate

A part leaves `birchpad` or `birchpad-editor` for a crate of its own when it does not need the
type it would leave behind: the `Workspace` for the application, the `EditorView` for the
editor. Rust keeps a type's own methods in the type's crate, so what is written as
`impl Workspace` or `impl EditorView` stays with the type; what only *asks* the window for a few
things takes them as arguments, as an event the window answers, or through a small trait the
window implements.

The application re-exports each crate where the module was (`birchpad::hex_editor`,
`birchpad::preview::run_plantuml`, `birchpad_editor::buffer`), as ADR 0058 did, so the code
and the tests that stay keep their paths.

### Out of the editor

- `birchpad-buffer`: the document (`Buffer`), its file, its syntax and its marks. An entity of
  GPUI's that draws nothing. It asked the editor's hooks for a file's user defined language;
  it now has that one hook itself (`UserLanguages`).
- `birchpad-widgets`: the active theme and what parts of the window draw alike (bars, the rows
  of lists, the text of errors, the monospaced font's name). It is a crate of its own, not part
  of `birchpad-ui`, because the theme's styles are by syntax highlight, and `birchpad-ui` does
  not wait for the grammars to build.

`birchpad-quick-pick` now needs `birchpad-widgets` alone, and `birchpad-search-results` the
buffer and the widgets: neither waits for the editor.

What is left in `birchpad-editor` (8 513 lines) is the view: `EditorView`, whose editing,
folding, completion, multi-selection and marks are its own methods, and the layout and the
element that draw it. The work on text those methods call was already in `birchpad-core`.

### Out of the application, without a window

- `birchpad-diagrams`: PlantUML run and stopped, PlantUML kept running, its output split into
  diagrams, its mistakes placed on lines, the files a diagram includes, zoom and sizes, SVG as
  pixels and PNG (and with it `resvg`, which the application no longer names). The preview that
  shows the diagrams, and its commands, stay.
- `birchpad-preferences`: every setting Preferences lists, by section, how each is edited, and a
  field's text read and shown. The dialog stays.

Their tests that need no window moved with them (66 and 4 tests), and both are in the Mutants
workflow.

### Out of the application, parts of the window

- `birchpad-hex-editor`: View in HEX's editor.
- `birchpad-spell-check`: the spelling of a document and the commands that run in an editor;
  the three commands of the window (automatic checking, the language, the dictionaries'
  folder) stay. The built-in dictionary's files stay under `crates/app/assets`: the online
  catalog names them there.
- `birchpad-language-servers`: the servers kept in step with the documents. A window tells it
  its documents (`sync_documents`); the commands that ask a server stay.
- `birchpad-terminal-panel`: the terminal's panel, with the tests of its keys, its grid and
  its colors (4 tests). Showing and hiding it, and the folder a shell starts in, stay.
- `birchpad-panels`: Document List, Function List, Document Map, the JSON viewer, Clipboard
  History and the Character Panel. What they need of the window is the trait `Host` (the active
  document, the documents open, showing one, closing one, putting text into the active one),
  which the application implements for its workspace. What they work out without a window is
  tested in the crate (14 tests).
- `birchpad-find-bar`: the find panel's tabs, fields and options. It searches nothing: what the
  user asks for is an event the window answers.
- `birchpad-ui` takes the dialogs that choose files (`path_dialog`), which every part with a
  file to open or save shares.

### What stays in the application, and why

`birchpad` (28 965 lines) keeps what is the window itself or changes the application's own
things:

- the `Workspace` and what is written as its methods: opening, saving and closing documents,
  the session, files changed on disk, find and replace, Find in Files, compare, navigation,
  macros, the Run menu, window modes, printing, and the commands' registration;
- the pane with its tabs, which holds the editors, the hex editors and the previews of its
  documents, and the preview's view;
- the docks, and the panels that work with the window's files and plugins: Folder as
  Workspace, the project panels, a plugin's panel;
- the dialogs that change what the application keeps: Preferences, the Style Configurator,
  user defined languages, the Shortcut Mapper, plugins and their administration, updates, Help;
- the menus, the toolbar and the status bar, which show the state of all of the above.

Splitting further means the `Workspace` in a crate under the features, each feature an
extension of it with its own state. That changes how every command reaches the window; it is
left for a release of its own.

## Consequences

- Ten crates more, and `birchpad-quick-pick` and `birchpad-search-results` no longer depend on
  the editor. Each new crate has a version of its own, a coverage floor (low until measured)
  and, without a window, a place in the Mutants workflow.
- The tests that need no window run without the application's test binary
  (`cargo nextest run -p birchpad-diagrams`); those that need the whole window stay with the
  application, as in ADR 0058.
- The application has a quarter fewer lines to compile after a change to it, and the new crates
  compile beside each other: see the measurements below.
- A part of the window that needs no `Workspace` goes to a crate of its own from the start; one
  that needs a few things of the window takes them as `birchpad-panels` does.

## Measurements

`cargo check -p birchpad` on the same machine (Windows 11, debug profile), the dependencies
built, after touching one file whose text did not change; the best of the runs (three with
incremental compilation, two without, `CARGO_INCREMENTAL=0`, where a touched crate and those
that depend on it are checked whole):

| Touched | Compilation | Before (0.8.0 and 7.7c) | After | |
| --- | --- | --- | --- | --- |
| `crates/app/src/lib.rs` | incremental | 2.94 s | 2.47 s | −16 % |
| `crates/app/src/lib.rs` | whole | 5.52 s | 4.53 s | −18 % |
| `crates/editor/src/lib.rs` | incremental | 3.76 s | 3.36 s | −11 % |
| `crates/editor/src/lib.rs` | whole | 7.17 s | 6.41 s | −11 % |

The gain is that of the lines that left: a check of the application is a sixth shorter, of the
editor and what depends on it a tenth. The time of the tests was not measured here; the
application's test binary still links the whole window.
