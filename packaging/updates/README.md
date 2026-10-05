# Signed updates: setting up and running

Birchpad trusts an update only if the update manifest is signed by a key whose public half is
compiled into it ([ADR 0020](../../docs/adr/0020-signed-updates-and-nightly-builds.md)). Both
halves live in the repository's settings, not in its files:

| Setting | Kind | Holds |
|---|---|---|
| `BIRCHPAD_UPDATE_KEY` | secret of the `update-signing` environment | the secret key(s), only for signing the manifest |
| `BIRCHPAD_TRUSTED_KEYS` | repository variable | the public key(s), compiled into release builds |

The public keys are not secret (every installed copy contains them, and each release build's log
shows them), so they are a variable, not a secret. This is how a maintainer sets them up once,
and what the workflows then do on their own.

## 1. Create the signing key (once)

Needs Rust and the [GitHub CLI](https://cli.github.com/) signed in (`gh auth login`) with admin
rights on the repository. From the repository root:

```powershell
powershell -ExecutionPolicy Bypass -File packaging\updates\new-signing-key.ps1   # Windows
```

```bash
packaging/updates/new-signing-key.sh              # Linux, macOS
```

The script:

1. stops if `BIRCHPAD_TRUSTED_KEYS` is set already (replacing a key is section 4);
2. builds `birchpad-release` and makes a new ed25519 key with the system's random generator;
3. shows the secret key once: **store it in a password manager or offline**. It is not written
   to disk. Losing it means no update can be signed for the versions that trust only this key:
   their users would have to reinstall by hand;
4. creates the `update-signing` environment and stores the secret as its `BIRCHPAD_UPDATE_KEY`
   secret;
5. sets the `BIRCHPAD_TRUSTED_KEYS` repository variable to the public key.

Builds from then on trust the key. Release and nightly builds stop with an error while the
variable is not set, since such a build could never update itself. Builds without it elsewhere
(development builds, builds from source) trust no key: Help > Check for Updates says so.

Recommended in Settings > Environments > `update-signing`: under *Deployment branches and
tags*, allow only the `main` branch and the `v*` tags (publishing a release runs the manifest
workflow on its tag), and, if you want every manifest change approved, add yourself as a
required reviewer (the nightly workflow then waits for approval every night it publishes).

Without the script, the same by hand (`keygen` prints the public key on standard error):

```bash
cargo run --release -p birchpad-release-tool -- keygen | gh secret set BIRCHPAD_UPDATE_KEY --env update-signing
gh variable set BIRCHPAD_TRUSTED_KEYS --body "<the public key>"
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
BIRCHPAD_TRUSTED_KEYS="$(gh variable get BIRCHPAD_TRUSTED_KEYS)" \
  cargo run -p birchpad-release-tool -- verify --manifest birchpad-updates.json
```

To try updates with a local build, build it with `BIRCHPAD_TRUSTED_KEYS` set to the public key of
a throwaway key and point `updates.url` to a manifest signed with it.

## 4. Replacing the key

1. Make a second key with `birchpad-release keygen` and store its secret. Add its public key
   to the variable, after the old one: `gh variable set BIRCHPAD_TRUSTED_KEYS --body "<old>,<new>"`.
   Publish a release, so that installations learn the new key.
2. Set the `BIRCHPAD_UPDATE_KEY` secret to both secrets, separated by a comma
   (`gh secret set BIRCHPAD_UPDATE_KEY --env update-signing`): manifests are signed with both,
   and old and new versions accept them. `birchpad-release public-key` prints the public keys of
   the secrets in `BIRCHPAD_UPDATE_KEY`, to check which is which.
3. Once few users run versions that trust only the old key, remove it from the variable and from
   the secret.

If a secret key leaks, remove its public key from `BIRCHPAD_TRUSTED_KEYS` at once and publish a
release; the versions that trusted it will accept manifests signed with it until they update.
