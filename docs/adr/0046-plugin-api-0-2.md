# ADR 0046: Plugin API 0.2

- Status: accepted
- Date: 2026-10-21
- Changes [ADR 0036](0036-webassembly-plugins.md) and [ADR 0037](0037-rhai-scripts.md): plugins
  get settings, files, controls, more events and background work; scripts get commands.

## Context

ADR 0036 kept the first plugin API small: the active document, commands, messages and a panel
of lines. What Notepad++ plugins commonly do needs more: options of their own, reading and
writing files (a plugin that compares with a file, or keeps a log), a panel with buttons and
fields, items in the text's context menu, reacting while the user types or moves the caret,
the command line's `-pluginMessage`, and work that takes longer than a call may. Scripts could
only run from the top, so one script could not offer several commands with keys of their own.

## Decision

- **The WIT package becomes `birchpad:plugin@0.2.0`, the manifest's `api` "0.2".** A plugin
  built for 0.1 is refused with a message, as ADR 0036 planned; its author rebuilds it against
  the new WIT, where every 0.1 function is unchanged.
- **Settings of their own.** `plugin.toml` may declare `[[settings]]` (key, title, kind `bool`,
  `number` or `text`, a default), shown in Preferences > Plugins. The host's `setting` and
  `set-setting` read and keep any key, declared or not (the plugin's own state), as text in
  `plugins/<id>.settings.toml`, outside the plugin's folder so that updating it keeps them;
  removing the plugin removes them. At most 256 keys of 64 KiB each.
- **Files with the user's permission.** A plugin that lists `permissions = ["files"]` may call
  `read-file`, `write-file` and `open-file` (absolute paths, UTF-8, at most 64 MiB read) once
  the user allowed it in Plugins Admin (Allow Files, `plugin.allow-files`); the grant lives in
  `plugins/permissions.toml`. Until then, and for a plugin that does not ask, the calls are
  refused with a message that says where to allow them. Nothing is asked in the middle of a
  call: a call runs on the main thread and cannot wait for the user.
- **A panel of controls.** `show-controls` fills the plugin's panel with labels, buttons, text
  fields, check boxes and separators (at most 200, ids unique); using one calls `on-control`
  with its id and value (empty for a button, the text on Enter, `true` or `false`).
- **The context menu.** A command whose `context-menu` is true is also an item below the text's
  context menu, after `context-menu.toml`'s items and a separator.
- **Events while typing.** `changed` and `selection-changed` are sent once the update that
  caused them is done, so typing a few characters at once is one event; a plugin's own edits are
  not told back to it, so that it cannot loop.
- **The command line.** `-pluginMessage` reaches `on-message` of the plugins whose `events`
  list `message`; plugins still loading get it when they are ready.
- **Work in the background.** `start-task` runs the plugin's `run-task` on another thread, in an
  instance of its own with no document and twelve times the fuel of a call; its result comes
  back through `on-task-done` on the main thread. A call may start 16 tasks, and at most 8 run
  at once.
- **Scripts' commands.** A comment `// command: <function> <title>` declares a command: Plugins >
  Scripts shows the script as a submenu of them, `script.run` takes `command`, and the Shortcut
  Mapper's Plugin Commands tab gives them keys, as it does the plugins' commands.
- **Settings > Import > Import plugin(s)...** installs several plugins at once from their
  `plugin.toml`, as Plugins Admin's Install... does one.

## Consequences

- Plugins written for 0.1 must be rebuilt; the WIT is the same for what they used.
- A plugin allowed to use files can read and change any file the user can: the permission is
  per plugin, asked for in its manifest and given by the user, and removing the plugin takes it
  back.
- Background tasks cannot see or change a document: they hand their result to `on-task-done`,
  which can.
