# ADR 0006: Command registry and keymaps as data

- Status: accepted
- Date: 2026-10-01

## Context

Notepad++ identifies every menu item and Scintilla operation by a numeric id; its shortcut
mapper, macros and plugins all build on that. Birchpad needs the same backbone from the start:
key bindings, menus, and later macros, the command palette, the shortcut mapper and WebAssembly
plugins must all trigger the same operations, and those operations must be storable.

GPUI's own model is one Rust type per action, bound to keys through its keymap. Using it
directly would spread command definitions over the UI code and make commands invisible to
anything that is not compiled into the binary.

## Decision

- **Commands are data.** A command has a stable string id (`file.open`, `edit.undo`,
  `view.zoom-in`, `encoding.convert-to`), a title and a scope (workspace or editor). An
  `Invocation` is an id plus JSON arguments (`{"encoding": "utf-8-bom"}`); it serializes as
  `{"command": "...", "args": {...}}`. The catalog, keymaps and the menu model live in the
  UI-independent crate `birchpad-commands`.
- **One GPUI action.** The application defines a single action, `RunCommand(Invocation)`. Every
  key binding and menu item dispatches it. The focused editor view handles editor-scope commands
  and propagates the rest; the workspace handles workspace commands and forwards editor commands
  to the active view when focus is elsewhere (a panel, the menu bar). Handlers are registered in
  a `CommandRegistry` by id, with typed, deserialized arguments. A test checks that every catalog
  command has a handler.
- **Keymaps are data.** A keymap is TOML: a list of `[[binding]]` entries with `keys`,
  `command`, optional `args`, `context` (`"Editor"`, or global when absent), `platforms` and
  `unbind`. Keys use GPUI's spelling (`ctrl-shift-s`); `secondary` is Ctrl on Windows and Linux
  and Cmd on macOS. The built-in keymap is such a file (modeled on Notepad++'s defaults); the
  user's `keymap.toml` next to `settings.toml` is layered on top.
- **Conflict rules**, implemented and tested without a window and handed to GPUI already
  resolved:
  - a later binding of the same keys in the same context replaces the earlier one; inside one
    layer that is reported as a duplicate, across layers it is an intended override;
  - `unbind = true` removes a binding;
  - a binding for a deeper context wins over a shallower or global one, as in GPUI;
  - a key sequence that is a prefix of another in the same context is reported;
  - unknown commands or platforms, bad keys and malformed files become diagnostics, and every
    valid binding still applies.
- **Menus are data too.** The main menu (File, Edit, Search, View, Encoding, Help) is described
  in `birchpad-commands` with placeholders for live content (recent files, character sets). On
  macOS it becomes the native menu bar through `cx.set_menus`, with About, Check for Updates and
  Quit moved to the application menu; on Windows and Linux gpui-component's `AppMenuBar` draws it
  under the system title bar. Shortcuts shown in menus come from the keymap, so a rebinding is
  reflected everywhere.

## Consequences

- Command ids are public API: they appear in user keymaps and, later, macros and plugins.
  Renaming a released id needs an alias.
- Arguments are validated when the command runs, not when the keymap loads; a binding with bad
  arguments shows an error notification when pressed.
- Commands that exist in the catalog but are not implemented yet are registered as "pending":
  they show up disabled in menus. None remain at the end of a phase.
- The command palette (phase 6) and shortcut mapper (phase 4) can be built from the catalog and
  the keymap without touching the code of individual commands.
- Text typed by the user does not go through the registry (it comes from the input handler), but
  `edit.insert-text` exists so macros can replay typing.
