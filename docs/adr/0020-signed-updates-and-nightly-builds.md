# ADR 0020: Signed updates, nightly builds and the per-user installer

- Status: accepted
- Date: 2026-10-05

## Context

[ADR 0005](0005-distribution-and-updates.md) set the goal: a per-user installer with silent
updates, channels, and updates whose authenticity does not depend on the hosting or the
connection. Phase 1 ([ADR 0009](0009-releases-portable-mode-and-update-check.md)) only read
GitHub's release list when asked and opened a download page; nothing was downloaded, so nothing
was signed. Phase 2 adds the nightly channel, background checks and automatic updates.

## Decision

### The signed manifest

- **One file**, `birchpad-updates.json`, an asset of the `updates` release on GitHub:
  `{ "manifest": "<JSON text>", "signatures": [{ "key", "signature" }] }`. The signatures are
  ed25519 (`ed25519-dalek`, strict verification) over the exact bytes of the manifest text, so
  there is no canonical JSON to get wrong. Several signatures allow replacing the key: old and
  new versions each find one they trust.
- **Trust is compiled in**: the public keys of the `BIRCHPAD_TRUSTED_KEYS` repository variable,
  which the release workflows pass to the build (`crates/update/build.rs` checks them; a key that
  is not valid fails the build). A manifest without a valid signature by one of those keys is
  refused, whoever serves it; `updates.url` may point to any mirror. The secret key is a secret
  of the `update-signing` environment, used only to sign the manifest.
- **Keys stay out of the repository's files.** Neither half of the key is committed: the
  maintainer keeps both in the repository's settings, replaces a key without a commit, and forks
  and builds from source do not trust the project's key by accident (they trust none, or their
  own). The public keys are not secret, so they are a variable rather than a secret: each release
  build prints the keys it trusts in its public log, which takes the place of reading them in the
  source. Release and nightly builds stop when the variable is not set, since such a build could
  never update itself.
- **Content**: format version, product, signing time, expiry, and the newest releases (five per
  channel): version, release page, and for each platform with an installer its Velopack
  package's URL, size, SHA-256 and SHA-1. Signing the hashes signs the packages.
- **Against old manifests** (an update server or attacker replaying a validly signed but old
  manifest):
  - *rollback*: the signing time must not be older than the newest manifest seen before
    (remembered in `state.toml`) nor than the commit the running release was built from;
  - *freeze*: a manifest expires 30 days after signing. Workflows sign it again every week even
    when nothing is released;
  - *downgrade*: only versions newer than the running one are offered, and Velopack is told
    never to downgrade.
- **Errors are not silent on request**: Help > Check for Updates shows why a manifest was refused
  (not trusted, bad signature, expired, older than one seen, newer format). Background checks
  only log it.

### Installing: Velopack, for per-user installs on Windows

- Release workflows run `vpk pack` (the version matching the `velopack` crate) for Windows x64
  and ARM64: `birchpad-<v>-windows-<arch>-setup.exe` installs into `%LocalAppData%` without
  elevation, `…-full.nupkg` is the update package. Deltas are not produced yet.
- Birchpad runs Velopack's startup hooks first thing (installing, uninstalling, and applying an
  update downloaded during the previous run). It detects a Velopack installation; only then does
  it update itself. Portable copies, the MSI and development builds announce updates with their
  download page.
- Velopack does not download from its own feed: Birchpad gives it a feed of exactly the package
  the verified manifest lists, downloads it through Birchpad's HTTP client (the system's
  certificate store and proxy) with the size as the limit, and checks its size and SHA-256
  before Velopack checks them again and installs it.

### Checking

- `updates.mode`:
  - `off`: nothing, no network;
  - `notify`: a background check a day (30 seconds after the start, then at most every 20
    hours, while Birchpad runs); a newer version is announced once in a notification, with
    Update (an installed copy: download, then restart) or Download (its page);
  - `auto` (the default): an installed copy downloads the update in the background and
    announces that it is installed at the next start, with Restart Now; other copies announce
    it as with `notify`.
- Restarting saves the session as quitting does (asking about unsaved changes when they are not
  backed up), then hands over to Velopack's updater, which installs the update and starts
  Birchpad again, which reopens the documents.
- Development builds check only on request (`BIRCHPAD_BACKGROUND_UPDATES=1` enables background
  checks for trying them out).

### Nightly builds

- `nightly.yml` runs every night and builds `main` if it changed since the last nightly build:
  version `<workspace version>-nightly.<YYYYMMDD>.<run>`, a pre-release of the version being
  developed (so after releasing 0.2.0, `main` must move to 0.3.0; the workflow checks it). The
  version reaches the binary through `BIRCHPAD_VERSION`, because changing `Cargo.toml` would
  change `Cargo.lock`.
- It publishes a GitHub pre-release with the same artifacts as a release (except the MSI), adds
  it to the manifest, and keeps the ten newest nightly releases.
- `release.yml` and `nightly.yml` share `build.yml`. Stable and beta releases stay drafts until a
  maintainer publishes them; publishing adds them to the manifest (`update-manifest.yml`).
- `birchpad-release` (crate `birchpad-release-tool`) makes keys and adds, removes, signs again
  and verifies manifests; it refuses to extend a manifest whose signature does not check out.
  Setup and key replacement: [packaging/updates/README.md](../../packaging/updates/README.md).

## Consequences

- Until the maintainer creates the key, release and nightly builds stop with an error, and
  other builds trust no key: checks report that the manifest cannot be verified.
- The installer and the packages are not Authenticode-signed yet: SmartScreen warns about the
  installer. The manifest's signature protects updates regardless.
- `vpk` was not run locally (no .NET SDK); the packaging steps first run in CI. The update
  path inside Birchpad is tested with a fake installer, and checked in the running application
  against a local manifest signed with a throwaway key.
- Velopack brings its own HTTP stack (`ureq` with Mozilla's root certificates, licensed
  CDLA-Permissive-2.0), unused by Birchpad's own requests.
- macOS and Linux have no installer that updates itself yet; they announce updates with their
  download page.
- An expired manifest stops all updates until it is signed again: the weekly signing must keep
  working (the nightly workflow fails visibly otherwise).
