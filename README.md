# Birchpad

A fast, extensible, cross-platform text editor in the spirit of Notepad++, written in Rust.

> **Status: pre-alpha.** Phase 1 is done: a basic notepad with tabs, encodings, find/replace
> and a Notepad++-compatible command line. Expect rough edges. See the [roadmap](docs/ROADMAP.md).

## Goals

- **Feature parity with Notepad++ first**: tabs and split views, multi-cursor and column
  editing, syntax highlighting and folding, powerful find/replace and find-in-files,
  macros, sessions that never lose unsaved work, user-defined languages and themes.
- **Then go further**: LSP, command palette, Git integration, built-in terminal, huge-file mode,
  plugins in any language via WebAssembly.
- **Native and fast**: GPU-rendered UI, no browser engine, instant startup.
- **Cross-platform**: Windows, Linux and macOS from day one.
- **Enterprise-ready**: per-machine MSI, Group Policy / Intune templates, update channels that
  administrators control, and a guaranteed zero-network mode.

## Why "Birchpad"?

Before paper became common in medieval Novgorod, people wrote everyday notes, letters and
school exercises on strips of birch bark. Those birch-bark documents were the notepads of
their time, and Birchpad is named after them.

## Building

Birchpad needs the Rust toolchain pinned in [`rust-toolchain.toml`](rust-toolchain.toml)
(rustup installs it automatically).

```bash
cargo run -p birchpad
```

```bash
cargo test --workspace
```

## Command line

Birchpad accepts the Notepad++ options that make sense for it:

```text
birchpad [-n<line>] [-c<column>] [-p<position>] [-ro] [-multiInst] [-nosession] [FILE]...
```

A second launch opens its files as tabs in the running window and exits; `-multiInst` starts a
separate instance instead. Other Notepad++ options (`-l<language>`, `-x`, `-y`, `-notabbar`, ...)
are accepted and ignored.

## Portable mode and the network

Put a file named `birchpad-portable.txt` next to the executable (the Windows and Linux ZIPs come
with it) and Birchpad keeps its settings and data in a `data` folder next to it. Administrator
policies of the machine still apply.

Birchpad uses the network only to check for updates: once a day, and when you pick Help > Check
for Updates. It reads a manifest signed with the project's key and refuses any other; installed
copies (the Windows installer) update themselves when Birchpad restarts, other copies show where
to download the new version. With `updates.mode = "off"` (a setting or an administrator policy)
Birchpad makes no network requests at all; `"notify"` announces new versions without
downloading them. See [ADR 0020](docs/adr/0020-signed-updates-and-nightly-builds.md).

## Repository layout

| Path | Contents |
|---|---|
| `crates/core` | Text model: rope buffer, selections, change sets, transactions, undo history. No UI. |
| `crates/config` | Layered settings: defaults, machine defaults, user settings, administrator policies. |
| `crates/commands` | Command catalog, keymaps and the menu model, as data. No UI. |
| `crates/io` | Encoding detection and conversion, reading and safely saving files. No UI. |
| `crates/view` | Layout of a document view in cells: tab stops, word wrap, visual rows. No UI. |
| `crates/syntax` | Languages and syntax trees: detection, tree-sitter parsing, highlighting, folding. No UI. |
| `crates/cli` | Notepad++-compatible command line and the single-instance hand-off. No UI. |
| `crates/update` | Checking for updates: the signed update manifest, channels, versions. No UI. |
| `crates/release-tool` | `birchpad-release`: signing keys and the update manifest, for the release workflows. |
| `crates/menu-bar` | The menu bar drawn in the window on Windows and Linux, with Windows' keyboard access. |
| `crates/app` | The desktop application. |
| `packaging/` | Release packaging: per-machine MSI, macOS app bundle, portable-mode marker, update signing setup. |
| `docs/` | Roadmap and architecture decision records. |

## Contributing

Contributions are welcome. Please read [CONTRIBUTING.md](CONTRIBUTING.md) first.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion
in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above,
without any additional terms or conditions.

Birchpad is an independent project and is not affiliated with Notepad++.
