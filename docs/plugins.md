# Writing plugins and scripts

Birchpad runs two kinds of extensions:

- **Plugins**: WebAssembly components, built from any language that targets the component
  model, installed through Plugins > Plugins Admin ([ADR 0036](adr/0036-webassembly-plugins.md),
  [ADR 0038](adr/0038-plugin-manager-and-allowlist.md), [ADR 0046](adr/0046-plugin-api-0-2.md)).
- **Scripts**: Rhai files in the `scripts` folder, run from Plugins > Scripts
  ([ADR 0037](adr/0037-rhai-scripts.md)).

Both see the same API: the active document, its selections, the commands of Birchpad,
messages and a panel. Plugins also keep settings, show controls, hear the user type, work in
the background and, once the user allows it, use files. Neither can use the network or run
without end.

## Plugins

A plugin is a folder:

```text
word-count/
├── plugin.toml
└── plugin.wasm
```

`plugin.toml`:

```toml
id = "word-count"            # lower-case letters, digits and dashes; names the folder
name = "Word Count"          # the submenu of the Plugins menu
version = "1.0.0"
description = "Counts the words of the document."
author = "Jane Doe"
api = "0.2"                  # the plugin API it is built for
entry = "plugin.wasm"        # the default
events = ["saved"]           # what on-event (and on-message) hears, see below
permissions = ["files"]      # asks the user to let it use files

[[settings]]                 # shown in Preferences > Plugins
key = "count-numbers"        # lower-case letters, digits, dashes, dots, underscores
title = "Count numbers as words"
kind = "bool"                # bool, number or text
default = "true"
```

`events` takes `opened`, `saved`, `closed` and `activated`; `changed` (the text changed:
typing, a command, an undo) and `selection-changed` (the caret or the selection moved), each
sent once the change is done; and `message`, for the command line's `-pluginMessage`.

The component implements the `plugin` world of
[`crates/plugin/wit/birchpad.wit`](../crates/plugin/wit/birchpad.wit):

| Export | Called |
|---|---|
| `commands()` | once when the plugin loads: its menu items, `{ id, title, context-menu }`; those with `context-menu` are also in the text's context menu |
| `run(command)` | when one of them is chosen (or bound to a key, or played in a macro) |
| `on-event(event)` | for the events `plugin.toml` lists, with the document concerned |
| `on-panel-click(line)` | when a line of the plugin's panel is clicked |
| `on-control(id, value)` | when a control of its panel is used |
| `on-message(message)` | with `-pluginMessage`, if `events` lists `message` |
| `run-task(task, input)` | on another thread, for a task the plugin started; returns its result |
| `on-task-done(task, outcome)` | with what the task returned, on the active document |

Through the `host` interface it reads and edits the document (positions are byte offsets into
UTF-8 text), sets the selections, runs commands of Birchpad by id with JSON arguments
(`edit.select-all`, `{}`), shows messages and fills its panel with lines (`show-panel`) or
controls (`show-controls`: labels, buttons, text fields, check boxes, separators). Everything
a call does to the document is one undo step; commands, messages, the panel, files to open and
tasks take effect once it returns. A call that fails changes nothing.

- **Settings**: `setting(key)` and `set-setting(key, value)` keep text between runs of
  Birchpad, in `plugins/<id>.settings.toml`. The settings `plugin.toml` declares show in
  Preferences > Plugins, starting from their default; others are the plugin's own.
- **Files**: `read-file`, `write-file` and `open-file` take absolute paths. They work only for a
  plugin that lists `permissions = ["files"]` and that the user allowed in Plugins Admin
  (Allow Files); until then they are refused.
- **Background work**: `start-task(task, input)` runs `run-task` away from the window, without
  a document; its result comes back through `on-task-done`.

A call may run for a few seconds and use 256 MiB; past that the plugin is stopped, with a
message. A task may run twelve times longer.

### In Rust

[`crates/plugin/tests/fixtures/probe`](../crates/plugin/tests/fixtures/probe) is a plugin that
uses every function. With [`wit-bindgen`](https://crates.io/crates/wit-bindgen):

```rust
wit_bindgen::generate!({ path: "wit", world: "plugin" });

use birchpad::plugin::host;

struct WordCount;

impl Guest for WordCount {
    fn commands() -> Vec<Command> {
        vec![Command {
            id: "count".into(),
            title: "Count Words".into(),
            context_menu: true,
        }]
    }

    fn run(_command: String) -> Result<(), String> {
        let words = host::text().split_whitespace().count();
        host::message(&format!("{words} words"));
        Ok(())
    }

    fn on_event(_event: Event) {}
    fn on_panel_click(_line: u32) {}
    fn on_control(_id: String, _value: String) {}
    fn on_message(_message: String) {}
    fn run_task(_task: String, _input: String) -> Result<String, String> {
        Err("no tasks".into())
    }
    fn on_task_done(_task: String, _outcome: Result<String, String>) {}
}

export!(WordCount);
```

Build it for `wasm32-unknown-unknown` as a `cdylib`, and make the component with
[`wasm-tools`](https://crates.io/crates/wasm-tools):

```bash
cargo build --release --target wasm32-unknown-unknown
```

```bash
wasm-tools component new target/wasm32-unknown-unknown/release/word_count.wasm -o plugin.wasm
```

### Installing

Plugins Admin also lists the plugins of Birchpad's online catalog (Refresh List,
`packaging/catalog/catalog.json`), installs them and offers their updates; to have a plugin
listed, open a pull request that adds it with its ZIP's address, size and SHA-256.

Plugins > Plugins Admin... > Install... takes the plugin's `plugin.toml` and copies its folder
into `plugins/<id>` next to the settings; Settings > Import > Import plugin(s)... takes several
at once. Installing the same id again updates it and keeps its settings; removing it removes
them and the permission given. An administrator can allow only some plugins or none
(`PluginInstall`, `AllowedPlugins`).

## Scripts

Save a `.rhai` file into the `scripts` folder next to the settings (Plugins > Scripts > Open
Scripts Folder); it appears in Plugins > Scripts. For example, `title-case.rhai`:

```rhai
let words = selected_text().split(" ");
let out = [];
for word in words {
    if word.len() > 0 {
        out.push(word.sub_string(0, 1).to_upper() + word.sub_string(1).to_lower());
    }
}
insert(out.reduce(|sum, w| if sum == () { w } else { sum + " " + w }));
```

| Function | Does |
|---|---|
| `text()`, `set_text(s)` | the whole text |
| `length()` | its length in bytes |
| `text_range(start, end)`, `replace(start, end, s)` | a part of it, by byte offsets |
| `selected_text()`, `insert(s)` | the primary selection; typing over every selection |
| `selections()`, `set_selection(anchor, head)` | selections as `#{anchor, head}` maps |
| `path()`, `language()` | the document's file (or `()`) and its language |
| `run_command(id)`, `run_command(id, #{...})` | a command of Birchpad, once the script ends |
| `message(s)`, `print(s)` | a message |
| `show_panel(title, [lines])` | the scripts' panel |

A script may declare commands, one comment line each: `// command: <function> <title>`. It
then shows in Plugins > Scripts as a submenu: Run runs it from the top, as before, and each
command runs it and then calls its function. Settings > Shortcut Mapper > Plugin Commands gives
keys to scripts and their commands, and to the plugins' commands.

```rhai
// command: upper Upper Case
// command: lower Lower Case
fn upper() { insert(selected_text().to_upper()); }
fn lower() { insert(selected_text().to_lower()); }
```

A script stops after 50 million operations. An administrator can turn scripts off
(`UserScripts`).
