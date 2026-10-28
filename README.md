# Birchpad

<img src="packaging/icons/birchpad.svg" alt="" width="96" align="right">

English · [Русский](README.ru.md)

A fast, extensible, cross-platform text editor in the spirit of Notepad++, written in Rust.

> **Status: alpha (0.7).** A Notepad++-like editor: tabs and split views, encodings, syntax
> highlighting for 50 languages with folding, multi-cursor and column editing, bookmarks,
> sessions that keep unsaved work, Find in Files, macros, themes, an interface in English,
> German and Russian, plugins as WebAssembly components and scripts in Rhai, with Compare, JSON and XML
> tools, a hex view, Markdown and PlantUML previews and spell checking built in. Full parity
> with Notepad++ comes next. Expect rough edges. See the [roadmap](docs/ROADMAP.md) and the
> [changelog](CHANGELOG.md).

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

On Windows, the installer and the MSI add "Edit with Birchpad" to the Explorer's context menu
of files (the MSI's `ExplorerMenu` feature can be left out: `ADDLOCAL=Main`); Preferences > Files
adds it for a portable copy, or takes it away. At its first start Birchpad offers to open text
files (Settings > Make Birchpad the Default Editor does it later): on Windows you then choose it
in Default apps, which programs may not do themselves; on macOS, use Finder's Get Info > Open
with > Change All.

To uninstall, use Installed apps in Windows Settings for the installer and the MSI, or delete the
folder of a portable copy.

Nightly builds of the next version's branch (`v0.7.5` or `v0.8.0` after 0.7.3) are published as
pre-releases; set `updates.channel = "nightly"` to get them as updates.

## Code signing policy

Windows releases are not signed yet. The release workflow is ready to sign the Windows
executable, the per-user installers and the MSI of each release (not nightly builds) through
SignPath, and stays switched off until Birchpad has a code signing certificate; see
[ADR 0021](docs/adr/0021-authenticode-signing-with-signpath.md). Until then, each release's
SHA-256 checksums and GitHub build provenance show that a download is what the release workflow
built from this repository's source on GitHub-hosted runners.

Privacy policy: Birchpad collects no personal data, and connects to the network only to check
for and download updates, which can be turned off, and, when you ask, to install PlantUML,
plugins and dictionaries; see [PRIVACY.md](PRIVACY.md).

## Building

Birchpad needs the Rust toolchain pinned in [`rust-toolchain.toml`](rust-toolchain.toml)
(rustup installs it automatically).

```bash
cargo run -p birchpad
```

```bash
cargo nextest run --workspace
```

or `cargo test --workspace`, more slowly; [CONTRIBUTING.md](CONTRIBUTING.md#development) has the
checks CI runs.

## Command line

Birchpad takes Notepad++'s options (Help > Command Line Arguments... lists them):

```text
birchpad [-n<line>] [-c<column>] [-p<position>] [-l<language>] [-udl="name"] [-ro]
         [-fullReadOnly] [-fullReadOnlySavingForbidden] [-monitor] [-r] [-multiInst] [-nosession] [-openSession] [-openFoldersAsWorkspace]
         [-notabbar] [-alwaysOnTop] [-systemtray] [-x<left>] [-y<top>] [-titleAdd="text"]
         [-settingsDir="folder"] [-loadingTime] [-quickPrint] [-qt="text"] [-qf="file"]
         [-qSpeed<1-3>] [-noPlugin] [-pluginMessage="text"] [-L<code>]
         [-notepadStyleCmdline] [-z <argument>] [--help] [FILE]...
```

A second launch opens its files as tabs in the running window and exits; `-multiInst` starts a
separate instance instead, as do every launch, or a launch with `-openSession`, when
Preferences > Multi-Instance says so (`instances.mode = "multi"` or `"session"`). `-l` takes
Notepad++'s language names (`-lcpp`, `-lpython`, `-lnormal`), `-openSession` opens the files as
sessions. File names with `*` and `?` open every file they match, in the folders below too with
`-r`. `-quickPrint` prints the files on the default printer and quits; `-qt` and `-qf` type a
text into a new document. The options of the window (`-notabbar`, `-alwaysOnTop`, `-x`, `-y`,
`-titleAdd`, `-settingsDir`, ...) apply when Birchpad starts. `-systemtray` starts in the
system tray on Windows and macOS, and minimized on Linux, which has no tray icon yet;
`-pluginMessage` hands its text to the plugins that ask for it;
`-qn` (Notepad++'s own easter eggs) is accepted and does nothing. `-L` (the one option whose case
counts) shows the interface in a language this time (`-Lde`, `-Lru`). To stand in for Windows
Notepad, `-notepadStyleCmdline -z` skips Notepad's path and reads the rest as one file name with
spaces.

## Portable mode and the network

Put a file named `birchpad-portable.txt` next to the executable (the Windows and Linux ZIPs come
with it) and Birchpad keeps its settings and data in a `data` folder next to it. Administrator
policies of the machine still apply.

On its own, Birchpad uses the network only to check for updates: once a day, and when you pick
Help > Check for Updates. It reads a manifest signed with the project's key and refuses any other; installed
copies (the Windows installer) update themselves when Birchpad restarts, other copies show where
to download the new version. With `updates.mode = "off"` (a setting or an administrator policy)
Birchpad makes no update requests; `"notify"` announces new versions without downloading them.
PlantUML, plugins from the online list and dictionaries are downloaded only when you ask, and
not with `network.online-features = false`. See [ADR 0020](docs/adr/0020-signed-updates-and-nightly-builds.md) and the
[privacy policy](PRIVACY.md).

## Repository layout

| Path | Contents |
|---|---|
| `crates/core` | Text model: rope buffer, selections, change sets, transactions, undo history, search. No UI. |
| `crates/config` | Layered settings: defaults, machine defaults, user settings, administrator policies; sessions. |
| `crates/i18n` | The interface languages: YAML locales, texts by key, plural forms, the language in use. No UI. |
| `crates/commands` | Command catalog, keymaps, the menu model, macros and Run commands, as data. No UI. |
| `crates/io` | Encoding detection and conversion, reading and safely saving files, searching them (Find in Files). No UI. |
| `crates/view` | Layout of a document view in cells: tab stops, word wrap, visual rows, right-to-left text. No UI. |
| `crates/syntax` | Languages and syntax trees: detection, tree-sitter parsing, highlighting, folding. No UI. |
| `crates/cli` | Notepad++-compatible command line and the single-instance hand-off. No UI. |
| `crates/tools` | Built-in tools: JSON and XML formatting, XPath and schemas, hex dumps, comparing texts, spell checking, date formats, HTML export. No UI. |
| `crates/plugin` | Plugins as WebAssembly components (`wit/birchpad.wit`), Rhai scripts, the plugin manager. No UI. |
| `crates/update` | Updates: the signed update manifest, channels, versions, installing (Velopack on Windows); the online catalog. No UI. |
| `crates/release-tool` | `birchpad-release`: signing keys and the update manifest, for the release workflows. |
| `crates/ui` | What the window's parts share: the command registry, the settings and state of the application, dialog buttons, messages to the user. |
| `crates/editor` | The editor: the buffer, the editor view drawn and edited in GPUI, the active theme. |
| `crates/menu-bar` | The menu bar drawn in the window on Windows and Linux, with Windows' keyboard access. |
| `crates/quick-pick` | The list to pick from by typing that the command palette and Go to File show. |
| `crates/search-results` | The search results panel of Find All and Find in Files. |
| `crates/tray` | Minimizing to the system tray, with each platform's own calls (Windows, macOS). |
| `crates/print-dialog` | Printing through Windows' print dialog and GDI. |
| `crates/app` | The desktop application: the workspace, panes, panels, dialogs and menus. See ADRs 0041 and 0058 for the layout. |
| `packaging/` | Release packaging: per-machine MSI, macOS app bundle, portable-mode marker, update signing setup, the online catalog. |
| `docs/` | Roadmap, architecture decision records, and [writing plugins and scripts](docs/plugins.md). |

## Contributing

Contributions are welcome. Please read [CONTRIBUTING.md](CONTRIBUTING.md) first.

## Relationship to Notepad++

Birchpad is an independent project, not affiliated with or endorsed by Notepad++ or its
authors. It is a clean-room reimplementation: it contains no code from Notepad++, Scintilla or
Notepad++'s plugins, and is written from Notepad++'s user manual, its behavior as a user sees it
and its file formats, never from its source ([the rules](CONTRIBUTING.md#clean-room)). Birchpad
reads Notepad++'s themes, sessions, user-defined languages and other files from the user's own
copy so that they carry over, but does not ship them. "Notepad++" is named only to describe
that compatibility.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion
in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above,
without any additional terms or conditions.

Birchpad is an independent project and is not affiliated with Notepad++.
