//! `birchpad-release`: the update manifest tooling of the release workflows (ADR 0020).
//!
//! ```text
//! birchpad-release keygen
//! birchpad-release public-key
//! birchpad-release add --manifest FILE --out FILE --version V --page URL --download-base URL
//!                      [--package PLATFORM=PATH]... [--keep N] [--expires-days N] [--new]
//! birchpad-release remove --manifest FILE --out FILE --version V [--expires-days N]
//! birchpad-release refresh --manifest FILE --out FILE [--expires-days N] [--if-older-than-days N]
//! birchpad-release verify --manifest FILE
//! ```
//!
//! Signing commands (and `public-key`) read the secret keys from `BIRCHPAD_UPDATE_KEY` (base64,
//! several separated by commas or newlines while a key is being replaced). An existing manifest
//! is extended only if its signature checks out against the trusted public keys: those of
//! `BIRCHPAD_TRUSTED_KEYS`, the repository variable that release builds compile in, or those of
//! `--keys-file`.

#![allow(
    clippy::print_stdout,
    reason = "a command-line tool prints its results"
)]

use std::collections::HashMap;
use std::fs;
use std::io::Read as _;
use std::path::Path;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context as _, Result, bail, ensure};
use birchpad_update::manifest::{self, PRODUCT, SCHEMA};
use birchpad_update::{Manifest, ManifestRelease, Package, hex};
use ed25519_dalek::{SigningKey, VerifyingKey};
use semver::Version;
use sha1::Sha1;
use sha2::{Digest as _, Sha256};

const DAY: u64 = 86_400;
const DEFAULT_EXPIRY_DAYS: u64 = 30;
const DEFAULT_KEEP: usize = 5;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args, &Env::from_process()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("birchpad-release: {error:#}");
            ExitCode::FAILURE
        }
    }
}

/// What the tool takes from its environment, read once so that tests can give their own.
struct Env {
    /// `BIRCHPAD_UPDATE_KEY`: the secret signing keys.
    update_key: Option<String>,
    /// `BIRCHPAD_TRUSTED_KEYS`: the public keys release builds trust.
    trusted_keys: Option<String>,
    /// Seconds since 1970.
    now: u64,
}

impl Env {
    fn from_process() -> Self {
        Self {
            update_key: std::env::var("BIRCHPAD_UPDATE_KEY").ok(),
            trusted_keys: std::env::var("BIRCHPAD_TRUSTED_KEYS").ok(),
            now: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("the clock is after 1970")
                .as_secs(),
        }
    }
}

fn run(args: &[String], env: &Env) -> Result<()> {
    let Some((command, rest)) = args.split_first() else {
        bail!(
            "usage: birchpad-release keygen|public-key|add|remove|refresh|verify ... (see the source)"
        );
    };
    let options = Options::parse(rest)?;
    match command.as_str() {
        "keygen" => keygen(),
        "public-key" => public_key(env),
        "add" => add(&options, env),
        "remove" => remove(&options, env),
        "refresh" => refresh(&options, env),
        "verify" => verify(&options, env),
        other => bail!("unknown command {other:?}"),
    }
}

/// `--name value` pairs; `--package` may repeat, `--new` takes no value.
struct Options {
    values: HashMap<String, String>,
    packages: Vec<(String, String)>,
    new: bool,
}

impl Options {
    fn parse(args: &[String]) -> Result<Self> {
        let mut options = Self {
            values: HashMap::new(),
            packages: Vec::new(),
            new: false,
        };
        let mut args = args.iter();
        while let Some(arg) = args.next() {
            let Some(name) = arg.strip_prefix("--") else {
                bail!("unexpected argument {arg:?}");
            };
            if name == "new" {
                options.new = true;
                continue;
            }
            let value = args
                .next()
                .with_context(|| format!("--{name} needs a value"))?;
            if name == "package" {
                let (platform, path) = value
                    .split_once('=')
                    .context("--package takes PLATFORM=PATH")?;
                options
                    .packages
                    .push((platform.to_owned(), path.to_owned()));
            } else {
                options.values.insert(name.to_owned(), value.clone());
            }
        }
        Ok(options)
    }

    fn get(&self, name: &str) -> Option<&str> {
        self.values.get(name).map(String::as_str)
    }

    fn required(&self, name: &str) -> Result<&str> {
        self.get(name)
            .with_context(|| format!("--{name} is required"))
    }

    fn number(&self, name: &str, default: u64) -> Result<u64> {
        self.get(name).map_or(Ok(default), |value| {
            value
                .parse()
                .with_context(|| format!("--{name} takes a number"))
        })
    }
}

/// Makes a new signing key. Its secret is printed (only) to standard output, to be piped into
/// the repository secret; its public key, for the trusted keys, to standard error.
fn keygen() -> Result<()> {
    let mut secret = [0u8; 32];
    getrandom::fill(&mut secret).map_err(|error| anyhow::anyhow!("no randomness: {error}"))?;
    let key = SigningKey::from_bytes(&secret);
    eprintln!(
        "Its public key, to add to the BIRCHPAD_TRUSTED_KEYS repository variable: {}",
        manifest::public_key_text(&key.verifying_key())
    );
    eprintln!(
        "The secret key follows on standard output: store it as the BIRCHPAD_UPDATE_KEY secret \
         and keep a copy offline. Whoever has it can sign updates; without it, no update can be \
         signed for the Birchpad versions that trust only this key."
    );
    println!("{}", manifest::signing_key_text(&key));
    Ok(())
}

/// Prints the public keys of the secret keys in `BIRCHPAD_UPDATE_KEY`, one per line.
fn public_key(env: &Env) -> Result<()> {
    for key in secret_keys(env)? {
        println!("{}", manifest::public_key_text(&key.verifying_key()));
    }
    Ok(())
}

/// The keys a manifest must be signed with: `--keys-file`, or `BIRCHPAD_TRUSTED_KEYS`.
fn trusted(options: &Options, env: &Env) -> Result<Vec<VerifyingKey>> {
    let keys = match options.get("keys-file") {
        Some(path) => {
            let text = fs::read_to_string(path).with_context(|| format!("cannot read {path}"))?;
            manifest::parse_keys(&text)
                .map_err(anyhow::Error::msg)
                .with_context(|| path.to_owned())?
        }
        None => manifest::parse_keys(env.trusted_keys.as_deref().unwrap_or_default())
            .map_err(anyhow::Error::msg)
            .context("BIRCHPAD_TRUSTED_KEYS")?,
    };
    ensure!(
        !keys.is_empty(),
        "no trusted keys: set BIRCHPAD_TRUSTED_KEYS to the public keys release builds trust \
         (gh variable get BIRCHPAD_TRUSTED_KEYS), or pass --keys-file"
    );
    Ok(keys)
}

/// The secret keys from `BIRCHPAD_UPDATE_KEY`.
fn secret_keys(env: &Env) -> Result<Vec<SigningKey>> {
    let text = env
        .update_key
        .as_deref()
        .context("BIRCHPAD_UPDATE_KEY is not set: it holds the update signing key")?;
    let keys = text
        .split([',', '\n'])
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .map(|key| manifest::parse_signing_key(key).map_err(anyhow::Error::msg))
        .collect::<Result<Vec<_>>>()
        .context("BIRCHPAD_UPDATE_KEY")?;
    ensure!(!keys.is_empty(), "BIRCHPAD_UPDATE_KEY holds no key");
    Ok(keys)
}

/// The secret keys from `BIRCHPAD_UPDATE_KEY`; at least one must be trusted.
fn signing_keys(trusted: &[VerifyingKey], env: &Env) -> Result<Vec<SigningKey>> {
    let keys = secret_keys(env)?;
    ensure!(
        keys.iter()
            .any(|key| trusted.contains(&key.verifying_key())),
        "none of the keys in BIRCHPAD_UPDATE_KEY is trusted (BIRCHPAD_TRUSTED_KEYS), so Birchpad \
         would refuse what they sign"
    );
    Ok(keys)
}

/// The manifest to extend: the current one, whose signature must check out, or an empty one
/// with `--new` when there is none yet.
fn current(options: &Options, env: &Env) -> Result<Manifest> {
    let path = options.required("manifest")?;
    match fs::read(path) {
        Ok(bytes) => manifest::verify_signature(&bytes, &trusted(options, env)?)
            .with_context(|| format!("{path} is not a manifest Birchpad trusts")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && options.new => Ok(Manifest {
            schema: SCHEMA,
            product: PRODUCT.to_owned(),
            timestamp: 0,
            expires: 0,
            releases: Vec::new(),
        }),
        Err(error) => Err(error).with_context(|| format!("cannot read {path} (--new starts one)")),
    }
}

/// Signs `manifest` as of now and writes it to `--out`.
fn write_signed(mut manifest: Manifest, options: &Options, env: &Env) -> Result<()> {
    let keys = signing_keys(&trusted(options, env)?, env)?;
    let now = env.now;
    // Never older than the manifest it replaces, even with a clock that went back.
    manifest.timestamp = now.max(manifest.timestamp + 1);
    manifest.expires =
        manifest.timestamp + options.number("expires-days", DEFAULT_EXPIRY_DAYS)? * DAY;
    manifest.releases.sort_by(|a, b| b.version.cmp(&a.version));
    let envelope = manifest::sign(&manifest, &keys);
    let out = options.required("out")?;
    let text = serde_json::to_string_pretty(&envelope)?;
    fs::write(out, text + "\n").with_context(|| format!("cannot write {out}"))?;
    summarize(&manifest);
    Ok(())
}

fn summarize(manifest: &Manifest) {
    println!(
        "signed {}, expires {}",
        manifest::date(manifest.timestamp),
        manifest::date(manifest.expires)
    );
    for release in &manifest.releases {
        let platforms: Vec<&str> = release
            .packages
            .iter()
            .map(|package| package.platform.as_str())
            .collect();
        println!(
            "  {} [{}] {}",
            release.version,
            platforms.join(", "),
            release.page
        );
    }
}

/// The channel a version belongs to, for keeping a few releases of each.
fn channel_of(version: &Version) -> &'static str {
    let pre = version.pre.as_str();
    if pre.is_empty() {
        "stable"
    } else if pre.starts_with("beta") || pre.starts_with("rc") {
        "beta"
    } else {
        "nightly"
    }
}

/// Adds a release (or replaces one with the same version), keeping the newest `--keep` of each
/// channel.
fn add(options: &Options, env: &Env) -> Result<()> {
    let mut manifest = current(options, env)?;
    let version = Version::parse(options.required("version")?).context("--version")?;
    let page = options.required("page")?.to_owned();
    ensure!(
        page.starts_with("https://"),
        "--page must be an https:// address"
    );
    let base = options.required("download-base")?.trim_end_matches('/');
    let packages = options
        .packages
        .iter()
        .map(|(platform, path)| package(platform, Path::new(path), base))
        .collect::<Result<Vec<_>>>()?;
    manifest
        .releases
        .retain(|release| release.version != version);
    manifest.releases.push(ManifestRelease {
        version,
        page,
        packages,
    });
    manifest.releases.sort_by(|a, b| b.version.cmp(&a.version));
    let keep = options.number("keep", DEFAULT_KEEP as u64)? as usize;
    let mut kept: HashMap<&str, usize> = HashMap::new();
    manifest.releases.retain(|release| {
        let count = kept.entry(channel_of(&release.version)).or_default();
        *count += 1;
        *count <= keep
    });
    write_signed(manifest, options, env)
}

/// Describes a package file: its name, address, size and hashes.
fn package(platform: &str, path: &Path, base: &str) -> Result<Package> {
    ensure!(
        ["windows-x64", "windows-arm64"].contains(&platform),
        "unknown platform {platform:?}"
    );
    let mut file =
        fs::File::open(path).with_context(|| format!("cannot read {}", path.display()))?;
    let (mut sha256, mut sha1) = (Sha256::new(), Sha1::new());
    let mut buffer = vec![0; 1024 * 1024];
    let mut size = 0;
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        sha256.update(&buffer[..read]);
        sha1.update(&buffer[..read]);
        size += read as u64;
    }
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .context("the package needs a file name")?
        .to_owned();
    Ok(Package {
        platform: platform.to_owned(),
        url: format!("{base}/{name}"),
        file: name,
        size,
        sha256: hex(&sha256.finalize()),
        sha1: hex(&sha1.finalize()),
    })
}

/// Withdraws a release, for one that turned out broken. Installations that already have it keep
/// it: Birchpad never downgrades itself.
fn remove(options: &Options, env: &Env) -> Result<()> {
    let mut manifest = current(options, env)?;
    let version = Version::parse(options.required("version")?).context("--version")?;
    let before = manifest.releases.len();
    manifest
        .releases
        .retain(|release| release.version != version);
    ensure!(
        manifest.releases.len() < before,
        "{version} is not in the manifest"
    );
    write_signed(manifest, options, env)
}

/// Signs the manifest again with a new timestamp and expiry, unchanged otherwise, so that it does
/// not expire while nothing is released. With `--if-older-than-days`, only when it is that old.
fn refresh(options: &Options, env: &Env) -> Result<()> {
    let manifest = current(options, env)?;
    if let Some(days) = options.get("if-older-than-days") {
        let days: u64 = days
            .parse()
            .context("--if-older-than-days takes a number")?;
        if env.now.saturating_sub(manifest.timestamp) < days * DAY {
            println!(
                "signed {}, less than {days} days ago: nothing to do",
                manifest::date(manifest.timestamp)
            );
            return Ok(());
        }
    }
    write_signed(manifest, options, env)
}

/// Checks a manifest as Birchpad would, and lists its releases.
fn verify(options: &Options, env: &Env) -> Result<()> {
    let path = options.required("manifest")?;
    let bytes = fs::read(path).with_context(|| format!("cannot read {path}"))?;
    let manifest = manifest::verify(&bytes, &trusted(options, env)?, env.now, 0)
        .with_context(|| format!("{path} would be refused"))?;
    summarize(&manifest);
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    /// 2026-10-06, noon.
    const NOW: u64 = 1_791_288_000;

    fn key(byte: u8) -> SigningKey {
        SigningKey::from_bytes(&[byte; 32])
    }

    /// An environment with `secret` keys to sign with and `trusted` keys to check against.
    fn env(secret: &[u8], trusted: &[u8]) -> Env {
        let text = |keys: Vec<String>| keys.join(",");
        Env {
            update_key: Some(text(
                secret
                    .iter()
                    .map(|&b| manifest::signing_key_text(&key(b)))
                    .collect(),
            )),
            trusted_keys: Some(text(
                trusted
                    .iter()
                    .map(|&b| manifest::public_key_text(&key(b).verifying_key()))
                    .collect(),
            )),
            now: NOW,
        }
    }

    fn args(text: &str) -> Vec<String> {
        text.split_whitespace().map(str::to_owned).collect()
    }

    /// Runs `command` with `--manifest` and `--out` set to `manifest` in `dir`.
    fn run_in(dir: &Path, command: &str, rest: &str, env: &Env) -> Result<()> {
        let manifest = dir.join("manifest.json");
        let line = format!(
            "{command} --manifest {} --out {} {rest}",
            manifest.display(),
            manifest.display()
        );
        run(&args(&line), env)
    }

    fn read(dir: &Path, env: &Env) -> Manifest {
        let bytes = fs::read(dir.join("manifest.json")).unwrap();
        let trusted = trusted(&Options::parse(&[]).unwrap(), env).unwrap();
        manifest::verify(&bytes, &trusted, env.now, 0).unwrap()
    }

    fn versions(manifest: &Manifest) -> Vec<String> {
        manifest
            .releases
            .iter()
            .map(|release| release.version.to_string())
            .collect()
    }

    /// A package file of `bytes` in `dir`.
    fn package_file(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, bytes).unwrap();
        path
    }

    const PAGE: &str = "--page https://example.org/r --download-base https://example.org/d/";

    #[test]
    fn options_take_pairs_packages_and_a_flag() {
        let options = Options::parse(&args(
            "--version 1.2.3 --new --package windows-x64=a.nupkg --package windows-arm64=b.nupkg",
        ))
        .unwrap();
        assert_eq!(options.get("version"), Some("1.2.3"));
        assert!(options.new);
        assert_eq!(
            options.packages,
            [
                ("windows-x64".to_owned(), "a.nupkg".to_owned()),
                ("windows-arm64".to_owned(), "b.nupkg".to_owned())
            ]
        );
        assert_eq!(options.number("keep", 5).unwrap(), 5, "the default");
        assert!(options.required("page").is_err());
        // Nothing at all is fine too.
        let none = Options::parse(&[]).unwrap();
        assert!(!none.new && none.packages.is_empty());
    }

    #[test]
    fn options_refuse_what_they_cannot_read() {
        let error = |text: &str| Options::parse(&args(text)).err().unwrap().to_string();
        assert_eq!(error("stray"), "unexpected argument \"stray\"");
        assert_eq!(error("--version"), "--version needs a value");
        assert_eq!(
            error("--package windows-x64"),
            "--package takes PLATFORM=PATH"
        );
        let options = Options::parse(&args("--keep many")).unwrap();
        assert_eq!(
            options.number("keep", 5).unwrap_err().to_string(),
            "--keep takes a number"
        );
    }

    #[test]
    fn commands_are_required_and_known() {
        let env = env(&[1], &[1]);
        assert!(
            run(&[], &env)
                .unwrap_err()
                .to_string()
                .starts_with("usage:")
        );
        assert_eq!(
            run(&args("publish"), &env).unwrap_err().to_string(),
            "unknown command \"publish\""
        );
    }

    #[test]
    fn versions_belong_to_channels() {
        let channel = |text: &str| channel_of(&Version::parse(text).unwrap());
        assert_eq!(channel("1.0.0"), "stable");
        assert_eq!(channel("1.0.0-beta.1"), "beta");
        assert_eq!(channel("1.0.0-rc.2"), "beta");
        assert_eq!(channel("1.0.0-nightly.20261006"), "nightly");
        // Anything else that is a pre-release counts as nightly.
        assert_eq!(channel("1.0.0-alpha"), "nightly");
    }

    #[test]
    fn packages_are_described_by_size_and_hashes() {
        let dir = tempfile::tempdir().unwrap();
        let empty = package_file(dir.path(), "empty.nupkg", b"");
        let described = package("windows-x64", &empty, "https://example.org/d").unwrap();
        assert_eq!(described.size, 0);
        assert_eq!(
            described.sha256,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(described.sha1, "da39a3ee5e6b4b0d3255bfef95601890afd80709");
        assert_eq!(described.url, "https://example.org/d/empty.nupkg");
        // Larger than the read buffer.
        let big = package_file(dir.path(), "big.nupkg", &vec![7; 3 * 1024 * 1024 + 1]);
        let described = package("windows-arm64", &big, "https://example.org/d").unwrap();
        assert_eq!(described.size, 3 * 1024 * 1024 + 1);
        assert_eq!(described.platform, "windows-arm64");

        assert!(
            package("linux-x64", &empty, "b").is_err(),
            "an unknown platform"
        );
        assert!(package("windows-x64", &dir.path().join("missing"), "b").is_err());
    }

    #[test]
    fn add_starts_a_manifest_and_keeps_the_newest_of_each_channel() {
        let dir = tempfile::tempdir().unwrap();
        let env = env(&[1], &[1]);
        let file = package_file(dir.path(), "birchpad-full.nupkg", b"package");
        let add = |version: &str, extra: &str| {
            let rest = format!(
                "--version {version} {PAGE} --package windows-x64={} {extra}",
                file.display()
            );
            run_in(dir.path(), "add", &rest, &env)
        };
        // Without a manifest, only with --new.
        let error = add("1.0.0", "").unwrap_err();
        assert!(
            format!("{error:#}").contains("--new starts one"),
            "{error:#}"
        );
        add("1.0.0", "--new").unwrap();
        let manifest = read(dir.path(), &env);
        assert_eq!(versions(&manifest), ["1.0.0"]);
        assert_eq!(manifest.timestamp, NOW);
        assert_eq!(manifest.expires, NOW + DEFAULT_EXPIRY_DAYS * DAY);
        let package = &manifest.releases[0].packages[0];
        assert_eq!(package.url, "https://example.org/d/birchpad-full.nupkg");

        // Two of each channel kept, newest first; the same version again replaces it.
        for version in [
            "1.1.0",
            "1.2.0",
            "1.3.0-beta.1",
            "1.3.0-beta.2",
            "1.3.0-beta.3",
        ] {
            add(version, "--keep 2").unwrap();
        }
        add("1.2.0", "--keep 2 --expires-days 1").unwrap();
        let manifest = read(dir.path(), &env);
        assert_eq!(
            versions(&manifest),
            ["1.3.0-beta.3", "1.3.0-beta.2", "1.2.0", "1.1.0"]
        );
        assert_eq!(manifest.expires - manifest.timestamp, DAY);
        // Each signing is newer than the one before, even within a second.
        assert!(manifest.timestamp > NOW);
    }

    #[test]
    fn add_refuses_bad_versions_pages_and_packages() {
        let dir = tempfile::tempdir().unwrap();
        let env = env(&[1], &[1]);
        let add = |rest: &str| run_in(dir.path(), "add", &format!("{rest} --new"), &env);
        assert!(add(&format!("--version one {PAGE}")).is_err());
        assert!(
            add("--version 1.0.0 --page http://example.org --download-base https://e.org")
                .unwrap_err()
                .to_string()
                .contains("https://")
        );
        assert!(
            add(&format!(
                "--version 1.0.0 {PAGE} --package windows-x64=missing"
            ))
            .is_err()
        );
        assert!(add("--version 1.0.0").is_err(), "no page");
        assert!(
            !dir.path().join("manifest.json").exists(),
            "nothing written"
        );
    }

    #[test]
    fn remove_withdraws_a_release_that_is_there() {
        let dir = tempfile::tempdir().unwrap();
        let env = env(&[1], &[1]);
        for version in ["1.0.0", "1.1.0"] {
            run_in(
                dir.path(),
                "add",
                &format!("--version {version} {PAGE} --new"),
                &env,
            )
            .unwrap();
        }
        run_in(dir.path(), "remove", "--version 1.1.0", &env).unwrap();
        assert_eq!(versions(&read(dir.path(), &env)), ["1.0.0"]);
        let error = run_in(dir.path(), "remove", "--version 1.1.0", &env).unwrap_err();
        assert_eq!(error.to_string(), "1.1.0 is not in the manifest");
        // The last one leaves an empty manifest, still signed.
        run_in(dir.path(), "remove", "--version 1.0.0", &env).unwrap();
        assert!(read(dir.path(), &env).releases.is_empty());
    }

    #[test]
    fn refresh_signs_again_only_when_old_enough() {
        let dir = tempfile::tempdir().unwrap();
        let mut env = env(&[1], &[1]);
        run_in(
            dir.path(),
            "add",
            &format!("--version 1.0.0 {PAGE} --new"),
            &env,
        )
        .unwrap();
        let signed = read(dir.path(), &env).timestamp;

        // A day later, with a limit of two days: unchanged.
        env.now = signed + DAY;
        run_in(dir.path(), "refresh", "--if-older-than-days 2", &env).unwrap();
        assert_eq!(read(dir.path(), &env).timestamp, signed);
        // Exactly two days: signed again, with a new expiry.
        env.now = signed + 2 * DAY;
        run_in(dir.path(), "refresh", "--if-older-than-days 2", &env).unwrap();
        let refreshed = read(dir.path(), &env);
        assert_eq!(refreshed.timestamp, signed + 2 * DAY);
        assert_eq!(versions(&refreshed), ["1.0.0"]);
        // Without a limit, always; a limit that is no number is refused.
        run_in(dir.path(), "refresh", "", &env).unwrap();
        assert!(run_in(dir.path(), "refresh", "--if-older-than-days soon", &env).is_err());
    }

    #[test]
    fn verify_checks_like_birchpad() {
        let dir = tempfile::tempdir().unwrap();
        let env = env(&[1], &[1]);
        run_in(
            dir.path(),
            "add",
            &format!("--version 1.0.0 {PAGE} --new"),
            &env,
        )
        .unwrap();
        let path = dir.path().join("manifest.json");
        let verify = |env: &Env| run(&args(&format!("verify --manifest {}", path.display())), env);
        verify(&env).unwrap();

        // Expired, signed by a key nobody trusts, tampered with, and missing.
        let later = Env {
            now: env.now + 31 * DAY,
            ..env_of(&env)
        };
        assert!(verify(&later).is_err(), "expired");
        assert!(verify(&self::env(&[1], &[2])).is_err(), "untrusted");
        let text = fs::read_to_string(&path).unwrap().replace("1.0.0", "9.0.0");
        fs::write(&path, text).unwrap();
        assert!(verify(&env).is_err(), "tampered");
        fs::remove_file(&path).unwrap();
        assert!(verify(&env).is_err(), "missing");
    }

    fn env_of(env: &Env) -> Env {
        Env {
            update_key: env.update_key.clone(),
            trusted_keys: env.trusted_keys.clone(),
            now: env.now,
        }
    }

    #[test]
    fn keys_must_be_there_and_trusted() {
        let dir = tempfile::tempdir().unwrap();
        let add = |env: &Env| {
            run_in(
                dir.path(),
                "add",
                &format!("--version 1.0.0 {PAGE} --new"),
                env,
            )
            .map_err(|error| format!("{error:#}"))
        };
        // No secret key, an empty one, one that is no key, one nobody trusts.
        let mut env = env(&[1], &[1]);
        env.update_key = None;
        assert!(
            add(&env)
                .unwrap_err()
                .contains("BIRCHPAD_UPDATE_KEY is not set")
        );
        env.update_key = Some(" , \n".into());
        assert!(add(&env).unwrap_err().contains("holds no key"));
        env.update_key = Some("c2hvcnQ=".into());
        assert!(add(&env).is_err());
        assert!(
            add(&self::env(&[2], &[1]))
                .unwrap_err()
                .contains("is trusted")
        );
        // No trusted keys at all.
        let mut untrusting = self::env(&[1], &[]);
        assert!(add(&untrusting).unwrap_err().contains("no trusted keys"));
        untrusting.trusted_keys = None;
        assert!(add(&untrusting).is_err());
        assert!(!dir.path().join("manifest.json").exists());

        // While a key is being replaced: the old and the new one sign, the old one is trusted.
        add(&self::env(&[1, 2], &[1])).unwrap();
        let bytes = fs::read(dir.path().join("manifest.json")).unwrap();
        for trusted in [1, 2] {
            let keys = [key(trusted).verifying_key()];
            assert!(
                manifest::verify(&bytes, &keys, NOW, 0).is_ok(),
                "key {trusted}"
            );
        }
    }

    #[test]
    fn trusted_keys_can_come_from_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let keys_file = dir.path().join("keys.txt");
        let public = manifest::public_key_text(&key(1).verifying_key());
        fs::write(&keys_file, format!("# trusted\n{public}\n")).unwrap();
        let options =
            Options::parse(&args(&format!("--keys-file {}", keys_file.display()))).unwrap();
        let mut env = env(&[1], &[]);
        env.trusted_keys = None;
        assert_eq!(trusted(&options, &env).unwrap(), [key(1).verifying_key()]);
        // A missing file, one with no key, one with a broken key.
        let missing = Options::parse(&args("--keys-file nowhere.txt")).unwrap();
        assert!(trusted(&missing, &env).is_err());
        fs::write(&keys_file, "# nothing yet\n").unwrap();
        assert!(trusted(&options, &env).is_err());
        fs::write(&keys_file, "not-a-key").unwrap();
        assert!(trusted(&options, &env).is_err());
    }

    #[test]
    fn public_key_needs_a_secret_key() {
        assert!(public_key(&env(&[1], &[])).is_ok());
        let mut none = env(&[1], &[]);
        none.update_key = None;
        assert!(public_key(&none).is_err());
    }
}
