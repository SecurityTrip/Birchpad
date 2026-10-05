# ADR 0009: Release builds, portable mode and the manual update check

- Status: accepted; the update check is superseded by
  [ADR 0020](0020-signed-updates-and-nightly-builds.md) (a signed manifest instead of GitHub's
  release list, background checks and automatic updates), and the build jobs moved to
  `build.yml`, shared with nightly builds
- Date: 2026-10-01

## Context

Phase 1 ships the first installable builds. [ADR 0005](0005-distribution-and-updates.md) sets
the long-term model: a per-user installer with silent updates, a per-machine MSI, a portable ZIP,
Authenticode signatures and an ed25519-signed update manifest. Phase 1 needs the parts that do
not depend on signing infrastructure: release builds, portable mode and a way to learn about new
versions, without breaking the zero-network guarantee.

## Decision

**Release workflow.** Pushing a tag `v<version>` runs `.github/workflows/release.yml`. The tag
must equal the workspace version. The channel comes from the version: no pre-release part is
stable, `-beta.N`/`-rc.N` is beta, any other pre-release is nightly. The workflow builds:

| Artifact | Notes |
|---|---|
| `birchpad-<v>-windows-x64.zip`, `-windows-arm64.zip` | portable (contain the portable marker) |
| `birchpad-<v>-linux-x64.zip` | portable; built on Ubuntu 22.04 so it runs on older glibc |
| `birchpad-<v>-macos.zip` | `Birchpad.app`, universal binary (arm64 + x86_64), ad-hoc signed |
| `birchpad-<v>-windows-x64.msi` | stable versions only: MSI versions are numeric (ADR 0005) |
| `birchpad-<v>-sbom.cdx.json` | CycloneDX SBOM of the whole dependency graph (`cargo-cyclonedx`) |
| `birchpad-<v>-SHA256SUMS.txt` | checksums of everything above |

Every file gets a GitHub build provenance attestation. The workflow creates a **draft** release;
a maintainer checks it and publishes it. Release jobs do not use build caches, so a cache written
by another workflow cannot end up in a release. Signing steps are placeholders that say the
binaries are unsigned; Authenticode (SignPath Foundation,
[ADR 0021](0021-authenticode-signing-with-signpath.md)) and Apple Developer ID signing with
notarization replace them. Windows on ARM is non-blocking, as in CI, until it is green.

**Build information.** The workflow sets `BIRCHPAD_CHANNEL` and `BIRCHPAD_COMMIT`; local builds
are "development" builds and get the commit from `git` in `build.rs`. Help > About shows the
version, build channel, commit, update channel and feed, the settings and data locations
(marking portable mode), and every setting fixed by an administrator policy with its value. A
Copy button puts the text on the clipboard for bug reports.

**Portable mode.** A file named `birchpad-portable.txt` next to the executable turns it on: the
user's settings, key bindings, recent files and recovery copies live in a `data` folder next to
the executable. The name ends in `.txt` so that creating it in Explorer with hidden extensions
works, and its content explains what it does. Machine defaults and administrator policies are
still read from the system: a portable copy cannot escape the policies of the machine it runs
on. The single-instance address includes a hash of the data directory, so a portable copy and
an installed one run side by side. The macOS ZIP contains a regular app bundle without the
marker, because data written inside a signed bundle breaks its signature.

**Update check.** Phase 1 checks only when the user picks Help > Check for Updates, whatever
`updates.mode` says; background checks for `notify` and automatic updates for `auto` come with
the signed update manifest (ADR 0005). With `updates.mode = "off"` the command is disabled, and
the check function in `birchpad-update` refuses before doing any I/O (tested with a fake
transport that counts requests). Nothing else in Birchpad uses the network.

- **Feed format.** GitHub's releases API: an array of releases or a single release, each with
  `tag_name`, `html_url`, `draft` and `prerelease`. The default feed is
  `https://api.github.com/repos/SecurityTrip/Birchpad/releases?per_page=30`; `updates.url`
  replaces it, for example with an internal mirror serving the same format.
- **Channels.** Stable takes versions without a pre-release part, beta also `beta*` and `rc*`,
  nightly everything. A release flagged as a prerelease without a pre-release version counts as
  a beta. Versions are compared as semver, so a beta of the next version is newer than every
  stable release before it.
- **Result.** If a newer release exists, a dialog offers to open its page in the browser; only
  `http` and `https` pages are accepted from the feed. Nothing is downloaded or installed, which
  is why the feed is not signed yet.
- **HTTP.** `ureq` with `rustls` (ring) and `rustls-platform-verifier`: certificates are checked
  against the operating system's store, so corporate root certificates work and no CA list is
  bundled. The proxy comes from `HTTPS_PROXY`/`ALL_PROXY` or, on Windows, the system proxy
  settings. Requests time out after 20 seconds and feeds are limited to 8 MB. The only
  identifying header is `User-Agent: Birchpad/<version> (<os>)`.

## Consequences

- Until signing is set up, Windows SmartScreen and macOS Gatekeeper warn about the downloads
  (on macOS: right-click > Open the first time).
- Pre-release tags get no MSI until there is a mapping of pre-release versions into MSI versions.
- Automatic proxy configuration (PAC/WPAD) is not supported; static proxy settings are.
- On Unix the single-instance socket must fit in 100 bytes; when neither `$XDG_RUNTIME_DIR`,
  `$TMPDIR` (macOS) nor the data directory gives a short enough path, Birchpad starts as a
  separate instance instead of handing files over.
