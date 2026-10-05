# Signed updates: setting up and running

Birchpad trusts an update only if the update manifest is signed by a key whose public half is
compiled into it ([ADR 0020](../../docs/adr/0020-signed-updates-and-nightly-builds.md)). This is
how a maintainer sets that up once, and what the workflows then do on their own.

## 1. Create the signing key (once)

Needs Rust and the [GitHub CLI](https://cli.github.com/) signed in (`gh auth login`) with admin
rights on the repository. From the repository root:

```powershell
pwsh packaging/updates/new-signing-key.ps1        # Windows
```

```bash
packaging/updates/new-signing-key.sh              # Linux, macOS
```

The script:

1. builds `birchpad-release` and makes a new ed25519 key with the system's random generator;
2. adds the public key to `crates/update/trusted-keys.txt`;
3. shows the secret key once: **store it in a password manager or offline**. It is not written
   to disk. Losing it means no update can be signed for the versions that trust only this key:
   their users would have to reinstall by hand;
4. creates the `update-signing` environment and stores the secret as its `BIRCHPAD_UPDATE_KEY`
   secret.

Then commit `crates/update/trusted-keys.txt` and push it to `main`. Builds from that commit on
trust the key.

Recommended in Settings > Environments > `update-signing`: limit it to the `main` branch, and,
if you want every manifest change approved, add yourself as a required reviewer (the nightly
workflow then waits for approval every night it publishes).

Without the script, the same by hand:

```bash
cargo run --release -p birchpad-release-tool -- keygen | gh secret set BIRCHPAD_UPDATE_KEY --env update-signing
```

## 2. What the workflows do

| Workflow | When | What |
|---|---|---|
| `nightly.yml` | 02:17 UTC daily, or by hand | if `main` changed: builds `v<version>-nightly.<date>.<run>`, publishes it as a pre-release with portable ZIPs, the Windows installer (`…-setup.exe`) and update packages (`…-full.nupkg`), adds it to the manifest, deletes nightly releases beyond the ten newest; otherwise signs the manifest again when it is a week old |
| `release.yml` | a `v*` tag is pushed | builds the same for a stable or beta version, plus the MSI, as a **draft** release |
| `update-manifest.yml` | a maintainer publishes a draft release; called by the nightly workflow; or by hand | adds the release to `birchpad-updates.json`, signs it and uploads it to the `updates` release |

The manifest lives at
`https://github.com/SecurityTrip/Birchpad/releases/download/updates/birchpad-updates.json`. It
keeps the five newest releases of each channel and expires 30 days after it was signed.

The nightly version is a pre-release of the workspace version on `main`, so after releasing
`v0.2.0`, raise `main` to `0.3.0` (the nightly workflow stops with an error until then).

## 3. By hand

Run **Update manifest** from the Actions tab:

- with a tag (`v0.2.0`): add that release again, for example after fixing a failed run;
- with *remove* (`0.2.0`): withdraw a broken release. Installations that already have it keep
  it, since Birchpad never downgrades itself; the next release replaces it;
- with neither: sign the manifest again now.

Locally, to look at the current manifest:

```bash
gh release download updates --pattern birchpad-updates.json
cargo run -p birchpad-release-tool -- verify --manifest birchpad-updates.json
```

## 4. Replacing the key

1. Make a second key: run `birchpad-release keygen` (it appends the public key), commit, and
   publish a release so that installations learn the new key.
2. Set `BIRCHPAD_UPDATE_KEY` to both secrets, separated by a comma: manifests are signed with
   both, and old and new versions accept them.
3. Once few users run versions that trust only the old key, remove it from
   `trusted-keys.txt` and from the secret.

If a secret key leaks, remove its public key from `trusted-keys.txt` at once and publish a
release; the versions that trusted it will accept manifests signed with it until they update.
