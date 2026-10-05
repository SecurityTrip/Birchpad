# Privacy policy

Birchpad collects no personal data. It has no telemetry, no crash reporting and no accounts, and
your documents, sessions and backups stay on your computer, in Birchpad's data folder (or, in
portable mode, the `data` folder next to it).

## Network requests

Birchpad connects to the network only for updates:

- **Checking for updates**: once a day while Birchpad runs, and when you pick Help > Check for
  Updates, it downloads the update manifest from
  `https://github.com/SecurityTrip/Birchpad/releases/download/updates/birchpad-updates.json`, or
  from the address in the `updates.url` setting. The request says which version of Birchpad
  asks and on which operating system (the `User-Agent` header, such as
  `Birchpad/0.1.3 (windows)`), and nothing about you or your files.
- **Downloading updates**: with `updates.mode = "auto"` (the default), a copy installed with the
  per-user Windows installer downloads the package of a newer version, from the address the
  manifest lists (GitHub's release downloads).

GitHub serves these files and, like any web server, sees the requests: the address of your
computer or network and the time. Its
[privacy statement](https://docs.github.com/site-policy/privacy-policies/github-general-privacy-statement)
covers them.

## Turning it off

- `updates.mode = "off"`, in Birchpad's settings, stops all network requests.
- `updates.mode = "notify"` checks and announces new versions, but downloads nothing.
- Administrators can set the same with the `UpdateMode` policy (the Windows registry), which
  users cannot override.

A link you choose to open (Open Download Page in an update notice, for example) goes to your
browser.
