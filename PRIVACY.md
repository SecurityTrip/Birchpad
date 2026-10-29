# Privacy policy

English · [Русский](PRIVACY.ru.md)

Birchpad collects no personal data. It has no telemetry, no crash reporting and no accounts, and
your documents, sessions and backups stay on your computer, in Birchpad's data folder (or, in
portable mode, the `data` folder next to it).

## Network requests

Birchpad connects to the network only for updates, and to install PlantUML, plugins and
dictionaries when you ask:

- **Checking for updates**: once a day while Birchpad runs, and when you pick Help > Check for
  Updates, it downloads the update manifest from
  `https://github.com/SecurityTrip/Birchpad/releases/download/updates/birchpad-updates.json`, or
  from the address in the `updates.url` setting. The request says which version of Birchpad
  asks and on which operating system (the `User-Agent` header, such as
  `Birchpad/0.7.6 (windows)`), and nothing about you or your files.
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
one Birchpad installed), not by a server.

GitHub serves these files and, like any web server, sees the requests: the address of your
computer or network and the time. Its
[privacy statement](https://docs.github.com/site-policy/privacy-policies/github-general-privacy-statement)
covers them.

## Turning it off

- `updates.mode = "off"`, in Birchpad's settings, stops the requests for updates.
- `updates.mode = "notify"` checks and announces new versions, but downloads nothing.
- `network.online-features = false` stops installing PlantUML and reading the online list of
  plugins and dictionaries; with both, Birchpad makes no network requests at all.
- Administrators can set the same with the `UpdateMode` and `OnlineFeatures` policies (the
  Windows registry), which users cannot override.

A link you choose to open (Open Download Page in an update notice, or Birchpad's home page,
manual and forum in the Help menu) goes to your browser.
