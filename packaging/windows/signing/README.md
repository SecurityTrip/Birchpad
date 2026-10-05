# Signing the Windows files with SignPath

Release builds sign the Windows executable, the per-user installers and the MSI with an
Authenticode certificate of [SignPath Foundation](https://signpath.org), through
[SignPath.io](https://signpath.io)
([ADR 0021](../../../docs/adr/0021-authenticode-signing-with-signpath.md)). Until the steps below
are done, releases build exactly as before, unsigned, with a warning in the log.

## 1. Apply (once)

- Turn on two-factor authentication on GitHub (SignPath Foundation requires it of every
  maintainer).
- Apply at [signpath.org/apply](https://signpath.org/apply) with the repository
  (`https://github.com/SecurityTrip/Birchpad`), the download page (its releases) and the code
  signing policy (`https://github.com/SecurityTrip/Birchpad#code-signing-policy`). Worth
  mentioning: the installers are made by Velopack's `vpk` from our signed executable and carry
  Birchpad's metadata, and contain Velopack's `Update.exe` unsigned.

SignPath Foundation decides; it may take a while.

## 2. Set up SignPath (once approved)

In [app.signpath.io](https://app.signpath.io):

1. Turn on two-factor authentication.
2. The project, with the slug `birchpad`.
3. The trusted build system *GitHub.com*, linked to the project, and the SignPath GitHub App
   installed for the repository.
4. Two artifact configurations, pasted from this folder:
   - `executables`: [executables.xml](executables.xml);
   - `installers`: [installers.xml](installers.xml).
5. The signing policy `release-signing` (the one with SignPath Foundation's certificate), with
   yourself as approver.
6. A CI user with the *Submitter* role in that policy, and an API token for it.

The slugs are those of `.github/workflows/build.yml` (`SIGNPATH_PROJECT`, `SIGNPATH_POLICY`, and
the artifact configuration slugs); change them there if SignPath gave others.

## 3. Set up GitHub

In the repository's Settings:

1. Environments > New environment `code-signing`:
   - secret `SIGNPATH_API_TOKEN`: the CI user's API token;
   - *Deployment branches and tags*: the `v*` tags (release builds run on them).
2. Secrets and variables > Actions > Variables > New repository variable
   `SIGNPATH_ORGANIZATION_ID`: the organization ID from SignPath.

The variable switches signing on; deleting it switches it off.

## 4. Releasing

Pushing a `v*` tag starts the release workflow, which now waits twice for you in SignPath:

1. *executables*: `birchpad.exe` for x64 and ARM64, before they are packaged;
2. *installers*: the two `…-setup.exe` and, for stable versions, the MSI.

Approve each in SignPath (the workflow log links to the request); each waits up to two hours.
SignPath checks that the files come from this repository's workflow, on GitHub-hosted runners,
and that their metadata says Birchpad at the version being released.

Nightly builds are not signed: every signing request needs an approver.

## Checking a signature

```powershell
Get-AuthenticodeSignature .\birchpad-0.1.3-windows-x64-setup.exe | Format-List Status, SignerCertificate
```

SmartScreen still warns about a new certificate until enough people have downloaded files
signed with it; with every release signed by the same certificate, the warnings stop.
