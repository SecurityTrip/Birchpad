# ADR 0029: Shortcut Mapper

- Status: accepted
- Date: 2026-10-10

## Context

Key bindings are data (ADR 0006): the built-in keymap, with the user's `keymap.toml` over it.
Until now the file was edited by hand and read once, at start. Notepad++ has a Shortcut Mapper.

## Decision

- **The Shortcut Mapper edits `keymap.toml` itself,** with `toml_edit`, so the user's other
  bindings, comments and bindings for other platforms stay as they are:
  - *Add Shortcut* binds the keys pressed to the selected command, replacing the file's own
    binding of those keys; a binding of the defaults is overridden by the user layer.
  - *Remove* drops the file's binding of the keys, and unbinds a default one
    (`unbind = true`).
  - *Reset* drops the file's bindings of the command and the unbinding of its default keys.
- **Changes apply at once:** the keymap is built again and GPUI's bindings of commands are
  replaced, as on a keyboard layout change. The menus show the new keys.
- **Keys are taken, not run, while a shortcut is being recorded:** a keystroke interceptor stops
  them before any binding sees them, and Escape gives up. One keystroke makes a shortcut, as in
  Notepad++; sequences (`ctrl-k ctrl-c`) are still written by hand.
- **Editor commands are bound in the editor's context,** as the defaults are; the others
  globally. The mapper warns when the keys run another command there.
- **The list** has every command of the catalog, and the menu items and bindings that run a
  command with arguments (Convert Case to UPPERCASE), with a filter on names, ids and keys.

## Consequences

- A keymap that does not read is left alone, and the mapper says why.
- Bindings with a `platforms` list are never changed by the mapper; it adds bindings for the
  current platform only.
