# ADR 0036: Plugins as WebAssembly components

- Status: accepted
- Date: 2026-10-12

## Context

Notepad++ plugins are DLLs that call Scintilla and Notepad++ through window messages: they run
with all the rights of the editor, crash it when they fail, and exist for one platform. ADR 0001
already gave up on loading them. Birchpad needs plugins that run on every platform it ships
for, cannot take the editor or the machine down, and that enterprises can approve.

## Decision

- **A plugin is a WebAssembly component** of the `plugin` world in
  `crates/plugin/wit/birchpad.wit` (package `birchpad:plugin@0.1.0`), run in Wasmtime
  (`birchpad-plugin`). Any language that builds components works; the tests use a plugin in
  Rust (`crates/plugin/tests/fixtures/probe`).
- **The API is small and data-only.** The `host` interface gives the document a call works on:
  its text, length, ranges by byte offset, replacing, typing over the selections, the
  selections, its path and language; running a command of the catalog by id with JSON
  arguments (ADR 0006); a message; the plugin's panel as lines. The plugin exports its commands
  (shown in its submenu of the Plugins menu, run as `plugin.run`), `run`, `on-event` for the
  events its manifest asks for (opened, saved, closed, activated) and `on-panel-click`.
- **A call works on a copy.** Before calling, Birchpad hands the plugin a session with a copy
  of the document (a rope clone); the plugin's edits compose into one transaction, applied as
  one undo step once the call returns; commands, messages and panels it asks for run after
  that, in order. A call that fails applies nothing.
- **Sandboxed and bounded.** Plugins get no WASI: no files, network, clock or environment. A
  call burns fuel (`DEFAULT_FUEL`, a few seconds of work) and the plugin has at most 256 MiB
  of memory; a plugin past either stops with an error. A plugin that traps is instantiated
  afresh on its next call. At most 200 commands and 1 000 effects a call.
- **Cranelift compiles plugins** off the main thread when Birchpad starts and on Reload
  Plugins; a plugin's commands appear in the Plugins menu once it is ready. On Windows on ARM,
  where Wasmtime's native code generation is not supported, plugins run in its Pulley
  interpreter.
- **Plugins and scripts may run each other** through commands, at most 8 deep, so that one
  that runs itself stops. Events are sent after what caused them, and Activated once per
  document made active.

## Consequences

- Plugins are portable files; one build runs on Windows, macOS and Linux.
- Changes to the API bump the WIT package version and the manifest's `api`; a plugin built for
  another API version is refused with a message rather than failing at run time.
- Plugins cannot read other files, draw their own UI or act while the user types; what
  Notepad++ plugins do beyond the API waits for later versions of it (Phase 6).
- Wasmtime adds a few megabytes to Birchpad and compile time to its build.
