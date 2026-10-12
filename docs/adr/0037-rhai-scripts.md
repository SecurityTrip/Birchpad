# ADR 0037: User scripts in Rhai

- Status: accepted
- Date: 2026-10-12

## Context

Notepad++ users automate with PythonScript or NppExec. Building a WebAssembly component is too
much for a five-line change to the document; Birchpad needs a script language that runs from a
text file. The roadmap left the choice between Lua and Rhai.

## Decision

- **Rhai.** It is pure Rust (no C toolchain for every platform, no `unsafe` to review at the
  boundary), MIT or Apache-2.0, sandboxed by default, and has operation limits built in. Lua is
  better known, but its bindings compile C and need care to sandbox.
- **Scripts are `.rhai` files in the `scripts` folder** next to the settings, listed in
  Plugins > Scripts and run over the active document as `script.run { "name": ... }`, so they
  can get keys and go into macros.
- **The same API as plugins** (ADR 0036), as functions: `text`, `set_text`, `length`,
  `text_range`, `replace`, `selected_text`, `insert`, `selections`, `set_selection`, `path`,
  `language`, `run_command`, `message` (and `print`), `show_panel`. A script works on a copy
  of the document; its edits are one undo step, applied when it ends without an error.
- **Bounded:** no modules, no files, at most 50 million operations, limits on string, array
  and map sizes and on nesting.
- **Administrators can turn scripts off** with the `UserScripts` policy (`plugins.scripts`).

## Consequences

- Positions are byte offsets, as in plugins; scripts that only use the selection or the whole
  text never see them.
- A script runs on the main thread: the limits keep a runaway one from hanging Birchpad for
  more than a few seconds.
