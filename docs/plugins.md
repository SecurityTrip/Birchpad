# Writing plugins and scripts

Birchpad runs two kinds of extensions:

- **Plugins**: WebAssembly components, built from any language that targets the component
  model, installed through Plugins > Plugins Admin ([ADR 0036](adr/0036-webassembly-plugins.md),
  [ADR 0038](adr/0038-plugin-manager-and-allowlist.md)).
- **Scripts**: Rhai files in the `scripts` folder, run from Plugins > Scripts
  ([ADR 0037](adr/0037-rhai-scripts.md)).

Both see the same API: the active document, its selections, the commands of Birchpad,
messages and a panel. Neither can read files, use the network or run without end.

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
api = "0.1"                  # the plugin API it is built for
entry = "plugin.wasm"        # the default
events = ["saved"]           # opened, saved, closed, activated: what on-event hears
```

The component implements the `plugin` world of
[`crates/plugin/wit/birchpad.wit`](../crates/plugin/wit/birchpad.wit):

| Export | Called |
|---|---|
| `commands()` | once when the plugin loads: its menu items, `{ id, title }` |
| `run(command)` | when one of them is chosen (or bound to a key, or played in a macro) |
| `on-event(event)` | for the events `plugin.toml` lists, with the document concerned |
| `on-panel-click(line)` | when a line of the plugin's panel is clicked |

Through the `host` interface it reads and edits the document (positions are byte offsets into
UTF-8 text), sets the selections, runs commands of Birchpad by id with JSON arguments
(`edit.select-all`, `{}`), shows messages and fills its panel. Everything a call does to the
document is one undo step; commands, messages and the panel take effect once it returns. A call
that fails changes nothing.

A call may run for a few seconds and use 256 MiB; past that the plugin is stopped, with a
message.

### In Rust

[`crates/plugin/tests/fixtures/probe`](../crates/plugin/tests/fixtures/probe) is a plugin that
uses every function. With [`wit-bindgen`](https://crates.io/crates/wit-bindgen):

```rust
wit_bindgen::generate!({ path: "wit", world: "plugin" });

use birchpad::plugin::host;

struct WordCount;

impl Guest for WordCount {
    fn commands() -> Vec<Command> {
        vec![Command { id: "count".into(), title: "Count Words".into() }]
    }

    fn run(_command: String) -> Result<(), String> {
        let words = host::text().split_whitespace().count();
        host::message(&format!("{words} words"));
        Ok(())
    }

    fn on_event(_event: Event) {}

    fn on_panel_click(_line: u32) {}
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

Plugins > Plugins Admin... > Install... takes the plugin's `plugin.toml` and copies its folder
into `plugins/<id>` next to the settings. Installing the same id again updates it. An
administrator can allow only some plugins or none (`PluginInstall`, `AllowedPlugins`).

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

A script stops after 50 million operations. An administrator can turn scripts off
(`UserScripts`).
