# ADR 0028: Preferences

- Status: accepted
- Date: 2026-10-10

## Context

Settings live in `settings.toml`, layered over machine defaults and under administrator policies
(`birchpad-config`). Until now they were changed only by
editing the file. Notepad++ has a Preferences dialog whose changes apply at once.

## Decision

- **Preferences edits the user's `settings.toml` itself,** one key at a time, with `toml_edit`:
  the rest of the file, its comments and its layout stay as they are, and a comment next to a
  changed value is kept. A value is checked against the settings schema before the file is
  written; a file that is not valid TOML, or has a value where a table is needed, is left alone
  and the change refused. Writing goes through a temporary file, as sessions do.
- **Changes apply at once,** as in Notepad++: after each change the layers are read again and
  every window redraws. There is no OK or Apply; the dialog only closes.
- **Settings set by policy are shown but disabled,** and refused if changed some other way.
- **The dialog lists the settings by section** (Editing, Margins, Highlighting, Files, Backup,
  Updates) from a table of entries: a key, a label and how it is edited (a check box, a choice,
  a number with its range, a list of numbers, text). View > Word Wrap and Show Symbol stay in
  the View menu, which remembers them in `state.toml`, and the theme in Settings > Theme.
- **An empty text field removes the key,** so the setting goes back to its default.

## Consequences

- Some settings take effect only where they are read: the backup interval at the next backup,
  update settings at the next check.
- A setting the dialog does not list is still changed by editing the file.
