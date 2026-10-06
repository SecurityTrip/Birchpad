# ADR 0021: Authenticode signing with SignPath Foundation

- Status: accepted, on hold
- Date: 2026-10-06

**On hold (2026-10-06).** SignPath Foundation did not take Birchpad for now. The workflow
stays as decided here, switched off while `SIGNPATH_ORGANIZATION_ID` is unset, so releases are
unsigned. Signing can come back through SignPath later, or with another certificate (such as
one for open source developers, or Azure Trusted Signing) in place of the two signing jobs.

## Context

Windows SmartScreen shows "Windows protected your PC" for downloaded programs without
reputation. Reputation accrues to a file, or to the certificate that signed it, as people
download and run it. An unsigned file starts from nothing, so every release warns again; files
signed with one certificate share its reputation, so the warnings fade over a few releases
(since 2024 not even an EV certificate skips this). Signed files also trip fewer false positives
of antivirus heuristics, and Smart App Control, when on, blocks unsigned programs outright.
[ADR 0005](0005-distribution-and-updates.md) planned Authenticode signatures for installers and
binaries; the update manifest has its own ed25519 signature
([ADR 0020](0020-signed-updates-and-nightly-builds.md)), which Authenticode does not replace.

The options:

- **SignPath Foundation**: free for open source projects. The certificate is issued to SignPath
  Foundation, which is the publisher Windows shows. SignPath signs only files that a project's
  CI built from its public source, verified through GitHub, and every signing request needs a
  maintainer's approval. The project must publish a code signing policy and a privacy policy.
- **Azure Trusted Signing**: about $10 a month, the publisher is the developer; identity
  validation is limited to individuals in the United States and Canada and organizations in a
  few more countries.
- **An OV certificate** in a CA's cloud HSM: $100–400 a year, the publisher is the developer,
  after the CA's identity validation.

## Decision

- **SignPath Foundation.** The project is open source and free to use; the Foundation's terms
  (open source license, no proprietary components, maintained, already released, two-factor
  authentication, roles, a code signing policy) fit it.
- **What is signed:** `birchpad.exe` of both Windows platforms, the per-user installers
  (`…-setup.exe`) and the MSI of stable releases. Velopack's `Update.exe` inside the installers
  is an upstream binary and stays unsigned, which the Foundation allows in signed packages.
  The `.nupkg` update packages are not Authenticode-signed: the signed manifest covers them, and
  they contain the signed executable.
- **Two signing requests per release**, because the installers contain the executable:
  `build.yml` builds both executables, signs them in one request (artifact configuration
  `executables`), makes the ZIPs, Velopack's installers and update packages and the MSI from the
  signed executables, then signs the installers and the MSI in a second request (`installers`).
  Each waits up to two hours for approval. The configurations
  (`packaging/windows/signing/*.xml`) restrict the files' metadata to Birchpad at the version
  being built (`crates/app/build.rs` writes it into the executable; `vpk` into its installer; the
  WiX source sets the MSI's Subject and Author).
- **Off until set up, never half-on.** Signing runs when the release workflow asks for it and
  the `SIGNPATH_ORGANIZATION_ID` variable is set; the API token is a secret of the
  `code-signing` environment, limited to release tags. Without the variable, builds are unsigned
  as before, with a warning. A signing request that fails or is not approved fails the release:
  it is never published half-signed.
- **Nightly builds are not signed**: each would need an approval every night.
- **Policies on the home page:** the README's *Code signing policy* (SignPath's wording, the
  roles) and [PRIVACY.md](../../PRIVACY.md); release notes link to the policy, as the Foundation
  asks of download pages. Birchpad collects no personal data; its only network use is the update
  check, which the privacy policy describes and `updates.mode = "off"` turns off.

## Consequences

- Windows names "SignPath Foundation" as the publisher, not the project.
- Releasing needs the approver at hand twice, within two hours each.
- SmartScreen keeps warning for a while after the first signed release, until the certificate
  has reputation.
- The packaging moved: Windows ZIPs, installers and the MSI are made in one job
  (`package-windows`, on x64) for both platforms, after the build; the MSI job left
  `release.yml`.
- macOS still needs Developer ID signing and notarization (an Apple Developer account).
- Setting up is the maintainer's:
  [packaging/windows/signing/README.md](../../packaging/windows/signing/README.md).
