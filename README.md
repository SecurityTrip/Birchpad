# Birchpad

A fast, extensible, cross-platform text editor in the spirit of Notepad++, written in Rust.

> **Status: pre-alpha.** The text model and project infrastructure are being built.
> Nothing is usable yet. See the [roadmap](docs/ROADMAP.md).

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

## Repository layout

| Path | Contents |
|---|---|
| `crates/core` | Text model: rope buffer, selections, change sets, transactions, undo history. No UI. |
| `crates/config` | Layered settings: defaults, machine defaults, user settings, administrator policies. |
| `crates/app` | The desktop application. |
| `packaging/` | Installers (MSI for per-machine installs). |
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
