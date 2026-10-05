# ADR 0005: Distribution, updates and enterprise deployment

- Status: accepted
- Date: 2026-10-01

## Decision

**Two installation models, one binary.** The application detects how it was installed.

| | Per-user | Per-machine |
|---|---|---|
| Installer | Velopack | MSI (WiX), x64 and ARM64 |
| Location | `%LocalAppData%` | `Program Files` |
| Elevation | none | administrator |
| Updates | silent, delta, channels | managed by IT; built-in updater is off, notify-only, or downloads a signed MSI and asks for elevation |

Plus a portable ZIP (settings next to the executable) and winget/Scoop manifests.

**Channels:** nightly (`main`), beta (`release/x.y`), stable (tags). Experimental features are
behind flags rather than long-lived branches.

**Update security** does not rely on the transport or hosting: installers and binaries carry an
Authenticode signature, and the update manifest and packages are signed with our own ed25519 key
whose public half is compiled into Birchpad. In 2025 attackers hijacked Notepad++'s update traffic
through its hosting provider; independent signatures make that kind of attack ineffective.

**Configuration layers** (`birchpad-config`), lowest to highest priority: built-in defaults,
machine defaults (`%ProgramData%\Birchpad\defaults.toml`), user settings, administrator policies
(`HKLM`/`HKCU\Software\Policies\Birchpad` on Windows, a root-owned TOML file elsewhere). Policy keys
are locked in the UI. ADMX/ADML templates are generated from the single policy list in code.

**Zero-network guarantee:** with updates and online features disabled by policy, Birchpad makes no
network requests at all. This is documented and tested.

Implemented for nightly builds, the signed manifest and the Windows per-user installer in
[ADR 0020](0020-signed-updates-and-nightly-builds.md).

## Consequences

- The MSI `UpgradeCode` is fixed forever: `A1F35DF8-07F9-400E-92E7-E761EC9BAE37`.
- MSI versions are limited to `major.minor.build` with `major, minor < 256` and `build < 65536`;
  nightly builds need a mapping into that space.
- Settings files carry a schema version and are migrated with a backup, so switching from beta
  back to stable never breaks them; unknown keys are ignored.
- Releases ship an SBOM (`cargo-cyclonedx`) and GitHub build provenance attestations.
