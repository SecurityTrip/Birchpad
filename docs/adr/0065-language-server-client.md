# ADR 0065: Language server client

- Status: accepted
- Date: 2026-11-09
- Builds on [ADR 0031](0031-auto-completion.md) (the completion list and the call tip, which
  the server's answers fill), [ADR 0041](0041-crate-layout.md) (logic without a window in a
  crate of its kind).

## Context

Phase 7 makes Birchpad useful for code beyond highlighting: diagnostics, completion, hover, go
to definition and references, rename and formatting (roadmap 7.5). Each language has its own
server speaking the Language Server Protocol (LSP 3.17): JSON-RPC 2.0 over the server's
standard input and output. Bundling servers would tie Birchpad's releases to dozens of
toolchains and their licenses; the user installs the ones they use, as they install compilers.

## Decision

### A crate without a window

- `birchpad-lsp` holds what needs no window: the base protocol (a `Content-Length` header, a
  blank line, the JSON), the JSON-RPC messages, a client that starts a server as a process and
  talks to it on two threads, the conversion between Birchpad's byte offsets and the protocol's
  positions, file paths as `file:` URIs, and the conversion of the server's answers (edits,
  locations, completion items, hover text) into plain values. The application only decides
  when to ask and shows the answers.
- The protocol's types come from `lsp-types` 0.97, the version GPUI already builds: no new
  crate is compiled for them.
- The client is generic over a reader and a writer, so that its tests and the application's
  run against a scripted server in the same process (`test-support`).

### Servers configured per language, none bundled

- `lsp.servers.<language id> = "command line"` (`rust = "rust-analyzer"`,
  `python = "pylsp"`) names the server of a language by Birchpad's language id; the command line
  is split as a shell would split it (quotes keep spaces). `lsp.enabled` (on by default) turns
  every server off. With no server configured nothing starts, and Birchpad behaves as before.
  Preferences > Language Servers shows both.
- One server runs per language and root folder. The root is the nearest folder above the file
  with `.git`, or else the file's own folder. Untitled documents, documents without a language
  and large files (Preferences > Performance) get no server.
- A server starts with the first document of its language and root, and stops (`shutdown`,
  then `exit`, then killed after two seconds) when the last one closes, when its command
  changes, when the window closes, or with Tools > Language Server > Restart.
- On Windows the process starts without a console window (`CREATE_NO_WINDOW`).

### Documents

- `didOpen` when a document with a server opens or takes its language; the full text in
  `didChange` after edits, at most every 150 ms and always before a request about the document
  (full text is valid whatever synchronization the server prefers, and keeps both sides in
  step without replaying edits); `didSave` after saving; `didClose` when it closes or changes
  language.
- Positions are UTF-8 when the server accepts them (`general.positionEncodings` offers
  `utf-8` first), UTF-16 otherwise, as the protocol's default. A position past the end of its
  line is the end of the line; a line past the end of the text is the end of the text.

### What the user gets

- **Diagnostics** (`publishDiagnostics`) are underlined with a wavy line, red for errors,
  yellow for warnings, blue for information and hints. They are byte ranges of the document
  from the moment they arrive and move with its edits until the server sends new ones.
  Next and Previous Diagnostic (`Alt+F8`, `Alt+Shift+F8`) select one and show its message in
  the call tip; List Diagnostics puts them in the search results panel.
- The shortcuts other editors give these (`F2`, `F12`) are Notepad++'s bookmarks and Post-It
  in Birchpad, and stay so; the ones here are free in its keymap.
- **Completion**: while auto-completion is on, typing a word character or one of the server's
  trigger characters asks the server, and its items come first in the auto-completion list,
  filtered by what has been typed since, before the document's words and the API's.
  Function Completion (`Ctrl+Space`) asks for them too. Snippets are not offered to servers; a
  snippet sent anyway is inserted as its plain text.
- **Show Hover** (`Ctrl+Alt+K`) shows the server's text, as plain text, and the messages of
  the diagnostics at the caret in the call tip.
- **Go to Definition** (`Alt+F12`) opens the file and selects the range; several definitions
  are listed in the search results panel. **Find References** (`Shift+F12`) lists them there.
- **Rename Symbol** (`Alt+F2`) asks for the new name and applies the server's edit: each
  document changes in one undoable step, files not open are opened and changed, and nothing is
  saved. An edit that creates, renames or deletes files is refused as a whole.
- **Format Document** (`Alt+Shift+F`) applies the server's edits in one undoable step, with
  the document's tab size and spaces.
- Requests the server makes are answered without asking the user: `workspace/configuration`
  with nothing set, registrations and progress accepted, `workspace/applyEdit` refused.
  `window/showMessage` errors go to the status bar, the rest to the log.

### Failures

- A server that cannot start, or exits, is reported once in the status bar; its documents work
  as without a server, and it is not restarted by itself. A request unanswered after ten
  seconds is reported and forgotten.
- Nothing about a server goes anywhere but to the process the user configured.

## Consequences

- Birchpad works with any LSP server without knowing it; the user installs and configures it.
- Full-text synchronization sends the whole document after edits: cheap for source files, and
  large files never get a server.
- Snippets, code actions, signature help, semantic tokens, inlay hints and workspace symbols
  are left for later; the client advertises only what Birchpad does.
