//! `birchpad-release`: the update manifest tooling of the release workflows (ADR 0020).
//!
//! ```text
//! birchpad-release keygen [--keys-file crates/update/trusted-keys.txt]
//! birchpad-release add --manifest FILE --out FILE --version V --page URL --download-base URL
//!                      [--package PLATFORM=PATH]... [--keep N] [--expires-days N] [--new]
//! birchpad-release remove --manifest FILE --out FILE --version V [--expires-days N]
//! birchpad-release refresh --manifest FILE --out FILE [--expires-days N] [--if-older-than-days N]
//! birchpad-release verify --manifest FILE
//! ```
//!
//! Signing commands read the secret keys from `BIRCHPAD_UPDATE_KEY` (base64, several separated
//! by commas or newlines while a key is being replaced). An existing manifest is extended only if
//! its signature checks out against the trusted keys: those compiled in from
//! `crates/update/trusted-keys.txt`, or those of `--keys-file`.

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
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("birchpad-release: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<()> {
    let Some((command, rest)) = args.split_first() else {
        bail!("usage: birchpad-release keygen|add|remove|refresh|verify ... (see the source)");
    };
    let options = Options::parse(rest)?;
    match command.as_str() {
        "keygen" => keygen(&options),
        "add" => add(&options),
        "remove" => remove(&options),
        "refresh" => refresh(&options),
        "verify" => verify(&options),
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

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("the clock is after 1970")
        .as_secs()
}

/// Makes a new signing key: its public half is added to the trusted keys file, its secret is
/// printed (only) to standard output, to be piped into the repository secret.
fn keygen(options: &Options) -> Result<()> {
    let keys_file = options
        .get("keys-file")
        .unwrap_or("crates/update/trusted-keys.txt");
    let mut secret = [0u8; 32];
    getrandom::fill(&mut secret).map_err(|error| anyhow::anyhow!("no randomness: {error}"))?;
    let key = SigningKey::from_bytes(&secret);
    let public = manifest::public_key_text(&key.verifying_key());
    let mut text = fs::read_to_string(keys_file)
        .with_context(|| format!("cannot read {keys_file} (run this in the repository root)"))?;
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(&format!("{public}  # added {}\n", manifest::date(now())));
    fs::write(keys_file, text).with_context(|| format!("cannot write {keys_file}"))?;
    eprintln!("Added the public key {public} to {keys_file}: commit it.");
    eprintln!(
        "The secret key follows on standard output: store it as the BIRCHPAD_UPDATE_KEY secret \
         and keep a copy offline. Whoever has it can sign updates; without it, no update can be \
         signed for the Birchpad versions that trust only this key."
    );
    println!("{}", manifest::signing_key_text(&key));
    Ok(())
}

/// The keys a manifest must be signed with: `--keys-file`, or those compiled in.
fn trusted(options: &Options) -> Result<Vec<VerifyingKey>> {
    match options.get("keys-file") {
        Some(path) => {
            let text = fs::read_to_string(path).with_context(|| format!("cannot read {path}"))?;
            manifest::parse_keys(&text).map_err(anyhow::Error::msg)
        }
        None => Ok(manifest::trusted_keys()),
    }
}

/// The secret keys from `BIRCHPAD_UPDATE_KEY`; at least one must be trusted.
fn signing_keys(trusted: &[VerifyingKey]) -> Result<Vec<SigningKey>> {
    let text = std::env::var("BIRCHPAD_UPDATE_KEY")
        .context("BIRCHPAD_UPDATE_KEY is not set: it holds the update signing key")?;
    let keys = text
        .split([',', '\n'])
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .map(|key| manifest::parse_signing_key(key).map_err(anyhow::Error::msg))
        .collect::<Result<Vec<_>>>()
        .context("BIRCHPAD_UPDATE_KEY")?;
    ensure!(
        keys.iter()
            .any(|key| trusted.contains(&key.verifying_key())),
        "none of the keys in BIRCHPAD_UPDATE_KEY is trusted (crates/update/trusted-keys.txt), \
         so Birchpad would refuse what they sign"
    );
    Ok(keys)
}

/// The manifest to extend: the current one, whose signature must check out, or an empty one
/// with `--new` when there is none yet.
fn current(options: &Options) -> Result<Manifest> {
    let path = options.required("manifest")?;
    match fs::read(path) {
        Ok(bytes) => manifest::verify_signature(&bytes, &trusted(options)?)
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
fn write_signed(mut manifest: Manifest, options: &Options) -> Result<()> {
    let keys = signing_keys(&trusted(options)?)?;
    let now = now();
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
fn add(options: &Options) -> Result<()> {
    let mut manifest = current(options)?;
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
    write_signed(manifest, options)
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
fn remove(options: &Options) -> Result<()> {
    let mut manifest = current(options)?;
    let version = Version::parse(options.required("version")?).context("--version")?;
    let before = manifest.releases.len();
    manifest
        .releases
        .retain(|release| release.version != version);
    ensure!(
        manifest.releases.len() < before,
        "{version} is not in the manifest"
    );
    write_signed(manifest, options)
}

/// Signs the manifest again with a new timestamp and expiry, unchanged otherwise, so that it does
/// not expire while nothing is released. With `--if-older-than-days`, only when it is that old.
fn refresh(options: &Options) -> Result<()> {
    let manifest = current(options)?;
    if let Some(days) = options.get("if-older-than-days") {
        let days: u64 = days
            .parse()
            .context("--if-older-than-days takes a number")?;
        if now().saturating_sub(manifest.timestamp) < days * DAY {
            println!(
                "signed {}, less than {days} days ago: nothing to do",
                manifest::date(manifest.timestamp)
            );
            return Ok(());
        }
    }
    write_signed(manifest, options)
}

/// Checks a manifest as Birchpad would, and lists its releases.
fn verify(options: &Options) -> Result<()> {
    let path = options.required("manifest")?;
    let bytes = fs::read(path).with_context(|| format!("cannot read {path}"))?;
    let manifest = manifest::verify(&bytes, &trusted(options)?, now(), 0)
        .with_context(|| format!("{path} would be refused"))?;
    summarize(&manifest);
    Ok(())
}
