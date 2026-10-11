# ADR 0030: Macros

- Status: accepted
- Date: 2026-10-11

## Context

Every action is a command invocation (ADR 0006), so a macro can be a list of invocations, as
Notepad++'s are lists of Scintilla messages and menu commands.

## Decision

- **Recording notes every command that runs,** from keys, menus or panels, after it succeeds:
  editor commands where the editor runs them, the others in the workspace's dispatch. The macro
  commands, Settings, Help and Exit are not recorded. Typed text is recorded as `edit.type`
  (which overwrites in overwrite mode, as typing does), joined into one step while typing goes
  on; text an input method commits counts as typed.
- **Playback dispatches the steps** to the workspace, which hands editor commands to the active
  view. Nothing is recorded while a macro plays. The first step that fails stops it.
- **Run a Macro Multiple Times** runs a number of times, or until the end of the file: until the
  caret reaches the end of the text, or a run neither moves it nor changes the text (and at most
  100 000 runs).
- **Saved macros live in `macros.toml`** next to the settings, by name, and appear in the Macro
  menu, each running `macro.run { "name": ... }`, so they can get key bindings too. A file that
  does not read leaves no saved macros, with a message.
- **Keys as in Notepad++:** Ctrl+Shift+R starts recording and, pressed again, stops it;
  Ctrl+Shift+P plays the last recorded macro.

## Consequences

- A macro replays commands, not keystrokes: a macro recorded with a key bound to a command keeps
  running that command after the key is bound to something else.
- Dialogs a command opens are not part of the macro; their result is (Find Next runs with the
  find panel's current text).
