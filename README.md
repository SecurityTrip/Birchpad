# Birchpad

<img src="packaging/icons/birchpad.svg" alt="" width="96" align="right">

A fast, extensible, cross-platform text editor in the spirit of Notepad++, written in Rust.

> **Status: alpha (0.1).** A Notepad++-like editor: tabs and split views, encodings, syntax
> highlighting for 50 languages with folding, multi-cursor and column editing, bookmarks,
> sessions that keep unsaved work, and a Notepad++-compatible command line. Find in Files,
> macros, themes and plugins come next. Expect rough edges. See the [roadmap](docs/ROADMAP.md)
> and the [changelog](CHANGELOG.md).

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

## Installing

Downloads are on the [releases page](https://github.com/SecurityTrip/Birchpad/releases):

| File | For |
|---|---|
| `birchpad-<version>-windows-x64-setup.exe`, `…-windows-arm64-setup.exe` | Windows, installed for the current user without administrator rights; updates itself |
| `birchpad-<version>-windows-x64.msi` | Windows, installed per machine by administrators, who also update it |
| `birchpad-<version>-windows-x64.zip`, `…-windows-arm64.zip` | Windows, portable: unzip and run |
| `birchpad-<version>-macos.zip` | macOS 10.15.7 or later, Apple silicon and Intel (`Birchpad.app`) |
| `birchpad-<version>-linux-x64.zip` | Linux x64, portable |

On Linux, to see Birchpad in the applications menu, put `birchpad` on your `PATH` and copy
`io.github.securitytrip.birchpad.desktop` to `~/.local/share/applications/` and
`io.github.securitytrip.birchpad.png` to `~/.local/share/icons/hicolor/512x512/apps/`.

Windows releases are not signed yet (see the [code signing policy](#code-signing-policy)), so
Windows SmartScreen asks before the first start ("More info", "Run anyway"). macOS
needs Birchpad.app opened once with Control-click > Open until the app is notarized. Each release
lists SHA-256 checksums and carries GitHub build provenance (`gh attestation verify <file>
--repo SecurityTrip/Birchpad`).

To uninstall, use Installed apps in Windows Settings for the installer and the MSI, or delete the
folder of a portable copy.

Nightly builds of the next version's branch (`v0.1.4` after 0.1.3) are published as
pre-releases; set `updates.channel = "nightly"` to get them as updates.

## Code signing policy

Windows releases are not signed yet. The release workflow is ready to sign the Windows
executable, the per-user installers and the MSI of each release (not nightly builds) through
SignPath, and stays switched off until Birchpad has a code signing certificate; see
[ADR 0021](docs/adr/0021-authenticode-signing-with-signpath.md). Until then, each release's
SHA-256 checksums and GitHub build provenance show that a download is what the release workflow
built from this repository's source on GitHub-hosted runners.

Privacy policy: Birchpad collects no personal data, and connects to the network only to check
for and download updates, which can be turned off; see [PRIVACY.md](PRIVACY.md).

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
birchpad [-n<line>] [-c<column>] [-p<position>] [-l<language>] [-ro] [-multiInst]
         [-nosession] [-openSession] [FILE]...
```

A second launch opens its files as tabs in the running window and exits; `-multiInst` starts a
separate instance instead. `-l` takes Notepad++'s language names (`-lcpp`, `-lpython`,
`-lnormal`), `-openSession` opens the files as sessions. Other Notepad++ options (`-x`, `-y`,
`-notabbar`, ...) are accepted and ignored.

## Portable mode and the network

Put a file named `birchpad-portable.txt` next to the executable (the Windows and Linux ZIPs come
with it) and Birchpad keeps its settings and data in a `data` folder next to it. Administrator
policies of the machine still apply.

Birchpad uses the network only to check for updates: once a day, and when you pick Help > Check
for Updates. It reads a manifest signed with the project's key and refuses any other; installed
copies (the Windows installer) update themselves when Birchpad restarts, other copies show where
to download the new version. With `updates.mode = "off"` (a setting or an administrator policy)
Birchpad makes no network requests at all; `"notify"` announces new versions without
downloading them. See [ADR 0020](docs/adr/0020-signed-updates-and-nightly-builds.md) and the
[privacy policy](PRIVACY.md).

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
| `crates/update` | Updates: the signed update manifest, channels, versions, installing (Velopack on Windows). No UI. |
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
