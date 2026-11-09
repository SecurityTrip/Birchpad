# Privacy policy

English · [Русский](PRIVACY.ru.md)

Birchpad collects no personal data. It has no crash reporting and no accounts, and your
documents, sessions and backups stay on your computer, in Birchpad's data folder (or, in
portable mode, the `data` folder next to it). Its update checks send install statistics, which
say which copy of Birchpad asks and nothing about you; they are described
[below](#install-statistics), and one setting turns them off.

## Network requests

Birchpad connects to the network only for updates, and to install PlantUML, plugins and
dictionaries when you ask:

- **Checking for updates**: once a day while Birchpad runs, and when you pick Help > Check for
  Updates, it asks Birchpad's update service for the update manifest:
  `POST https://api.birchpad.ru/v1/updates/check`, with the
  [install statistics](#install-statistics) in the request's body, or `{}` with them off. The
  service answers with the manifest as it is published on GitHub, which Birchpad checks
  against the keys built into it, as any manifest. When the service does not answer, or its
  answer does not check out, Birchpad downloads the manifest from GitHub instead,
  `https://github.com/SecurityTrip/Birchpad/releases/download/updates/birchpad-updates.json`,
  which is sent nothing. With the `updates.url` setting (an administrator's copy of the
  manifest) Birchpad downloads it from there alone, and sends no statistics. Each of these
  requests also says which version of Birchpad asks and on which operating system (the
  `User-Agent` header, such as `Birchpad/0.8.0 (windows)`), and nothing about your files.
- **Downloading updates**: with `updates.mode = "auto"` (the default), a copy installed with the
  per-user Windows installer downloads the package of a newer version, from the address the
  manifest lists (GitHub's release downloads).
- **Installing PlantUML**: when you click Install PlantUML in the preview of a PlantUML
  document (or View > Preview > Install PlantUML), Birchpad downloads PlantUML and the Java that
  runs it from a release of its repository on GitHub (`plantuml-*`), once. With
  `network.online-features = false` it does not.
- **The online list of plugins and dictionaries**: when you click Refresh List in Plugins
  Admin, or open Plugins > Spell Checker > Download Languages..., Birchpad reads the list from
  `https://raw.githubusercontent.com/SecurityTrip/Birchpad/main/packaging/catalog/catalog.json`
  (or the address in `network.catalog`); when you install a plugin or download a dictionary
  from it, Birchpad downloads that plugin's or dictionary's files from the addresses the list
  names, and checks them against it. With `network.online-features = false` it does neither.

These go through the proxy of Help > Set Updater Proxy... (`updates.proxy`) when one is set, else
through the one of `HTTPS_PROXY` or `ALL_PROXY` or, on Windows, the system's.

The Markdown preview loads no images from the web, and a link in it opens in your browser
only when you click it. PlantUML diagrams are drawn by PlantUML on your computer (yours, or the
one Birchpad installed), not by a server. Language servers (Preferences > Language Servers) are
programs you configure, which Birchpad starts on your computer and sends the text of your
documents of their language; Birchpad sends nothing about them anywhere else. Git
(Preferences > Git) is your own `git` program, which Birchpad asks for the last commit of your
files and the blame of a line; it never fetches, pulls or pushes.

GitHub serves the other files and, like any web server, sees the requests: the address of your
computer or network and the time. Its
[privacy statement](https://docs.github.com/site-policy/privacy-policies/github-general-privacy-statement)
covers them.

## Install statistics

With `updates.statistics = true` (the default), an update check tells `api.birchpad.ru`:

| Field | What it is | Why |
|---|---|---|
| `installId` | A random id (a UUID, version 4) | To count copies of Birchpad, not checks |
| `version` | The version of Birchpad, such as `0.8.0` | To know which versions are in use, and when an old one can stop being tested |
| `channel` | `stable`, `beta` or `nightly` (`updates.channel`) | To know who gets which builds |
| `platform` | `windows-x64`, `windows-arm64`, `linux-x64`, `macos-arm64` or `macos-x64` | To know which builds are needed |
| `plantuml` | Which PlantUML the previews draw with: `component` (the one Birchpad installed), `custom` (another one) or `none` | To know whether PlantUML updates reach those who use it |
| `plantumlVersion` | The version of the PlantUML Birchpad installed, with `component` only | The same |

The `installId` is made by your copy of Birchpad at its first check with statistics on and kept
in `state.toml` in Birchpad's data folder. It is random: it is not derived from your computer,
your name, your account or anything else, and it names a copy of Birchpad, not a person (two
users of one computer, or a portable copy beside an installed one, have two ids). Nothing else
is sent: no file names, no settings, no documents.

The service keeps one row for each `installId`: the version it was first seen with and the
current version, the channel, the platform, the PlantUML and its version, and the times of the
first and the last check. It does not keep IP addresses, the `User-Agent` or a history of
checks.

Rows are kept for TODO(retention). To have the row of your copy deleted, write to
TODO(deletion-address) with its `installId` (`[updates] install-id` in `state.toml`).

With statistics off, the request's body is `{}`: the service then learns nothing a download of
the manifest from GitHub does not carry (the address and the `User-Agent`), and keeps nothing.
Turning them off also deletes the `installId` from `state.toml`; turning them on again makes a
new one, which nothing ties to the old.

## Turning it off

- `updates.statistics = false` (Preferences > Updates > Send install statistics) sends no
  statistics: update checks send `{}`.
- `updates.mode = "off"`, in Birchpad's settings, stops the requests for updates, and with them
  the statistics.
- `updates.mode = "notify"` checks and announces new versions, but downloads nothing.
- `network.online-features = false` stops installing PlantUML and reading the online list of
  plugins and dictionaries; with both, Birchpad makes no network requests at all.
- Administrators can set the same with the `UpdateStatistics` (0 turns the statistics off),
  `UpdateMode` and `OnlineFeatures` policies (the Windows registry), which users cannot
  override; with `UpdateUrl`, update checks go to the administrator's copy of the manifest and
  send no statistics.

A link you choose to open (Open Download Page in an update notice, or Birchpad's home page,
manual and forum in the Help menu) goes to your browser.
