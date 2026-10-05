//! The signed update manifest (ADR 0020).
//!
//! The update server publishes one file, an [`Envelope`]: the manifest as JSON text and ed25519
//! signatures of exactly those bytes. Birchpad trusts the public keys compiled into it
//! (`trusted-keys.txt`), not the server or the connection: a manifest that is not signed by one
//! of them is refused, whoever serves it.
//!
//! The manifest lists recent releases, each with its page and its installer packages (Velopack,
//! per-user installs on Windows) with their sizes and SHA-256 hashes. Signing the hashes signs
//! the packages.
//!
//! Against an update server that serves old but validly signed manifests:
//! - a manifest older than the newest one seen before, or than the running build, is refused
//!   (rollback);
//! - a manifest past its expiry is refused, so an old one cannot hide new releases for long
//!   (freeze);
//! - only versions newer than the running one are offered, never a downgrade.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use birchpad_config::UpdateChannel;
use ed25519_dalek::{Signer as _, SigningKey, VerifyingKey};
use semver::Version;
use serde::{Deserialize, Serialize};

use crate::on_channel;

/// The manifest format this Birchpad reads.
pub const SCHEMA: u32 = 1;
/// The product a manifest must be for.
pub const PRODUCT: &str = "birchpad";
/// The Velopack package id of Birchpad's installer.
pub const PACKAGE_ID: &str = "Birchpad";

/// What the update server publishes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Envelope {
    /// The manifest as JSON text, exactly as signed.
    pub manifest: String,
    pub signatures: Vec<Signature>,
}

/// A signature of an envelope's manifest. Several keys may sign one manifest, so that a new key
/// can be introduced before the old one is retired.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signature {
    /// The signing key's public half, base64.
    pub key: String,
    /// The ed25519 signature of the manifest's bytes, base64.
    pub signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Manifest {
    pub schema: u32,
    pub product: String,
    /// When it was signed, in seconds since 1970.
    pub timestamp: u64,
    /// When it stops being accepted, in seconds since 1970.
    pub expires: u64,
    /// Newest first.
    pub releases: Vec<ManifestRelease>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ManifestRelease {
    pub version: Version,
    /// The release's page, with the downloads for every platform.
    pub page: String,
    /// Installer packages, for the platforms that have one.
    #[serde(default)]
    pub packages: Vec<Package>,
}

/// A full Velopack package of a release for one platform.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Package {
    /// `windows-x64` or `windows-arm64`.
    pub platform: String,
    pub file: String,
    pub url: String,
    pub size: u64,
    /// Lowercase hex.
    pub sha256: String,
    /// Lowercase hex; Velopack's feeds require it.
    pub sha1: String,
}

/// The platform of this build, as manifests name it, if it has installer packages.
pub const PLATFORM: Option<&str> = if cfg!(all(windows, target_arch = "x86_64")) {
    Some("windows-x64")
} else if cfg!(all(windows, target_arch = "aarch64")) {
    Some("windows-arm64")
} else {
    None
};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ManifestError {
    #[error("the update manifest cannot be read: {0}")]
    Format(String),
    #[error("this build of Birchpad has no key to check update manifests with")]
    NoKeys,
    #[error("the update manifest is not signed with a key this Birchpad trusts")]
    Untrusted,
    #[error("the update manifest's signature is not valid")]
    BadSignature,
    #[error("the update manifest is for {0:?}, not Birchpad")]
    Product(String),
    #[error(
        "the update manifest has format {0} and this Birchpad reads format {SCHEMA}: update it \
         from the download page"
    )]
    Schema(u32),
    #[error(
        "the update manifest expired on {0}: the update server may be serving an old one to hide \
         newer releases"
    )]
    Expired(String),
    #[error(
        "the update manifest was signed on {signed}, before one seen already or this build \
         ({newest}): the update server may be serving an old one"
    )]
    Rollback { signed: String, newest: String },
}

/// Parses the trusted public keys: one base64 key per line, `#` starts a comment.
pub fn parse_keys(text: &str) -> Result<Vec<VerifyingKey>, String> {
    text.lines()
        .map(|line| line.split('#').next().unwrap_or_default().trim())
        .filter(|line| !line.is_empty())
        .map(|line| {
            let bytes: [u8; 32] = BASE64
                .decode(line)
                .ok()
                .and_then(|bytes| bytes.try_into().ok())
                .ok_or_else(|| format!("{line:?} is not a base64 ed25519 public key"))?;
            VerifyingKey::from_bytes(&bytes).map_err(|error| format!("{line:?}: {error}"))
        })
        .collect()
}

/// The keys compiled into Birchpad, from `trusted-keys.txt`.
pub fn trusted_keys() -> Vec<VerifyingKey> {
    parse_keys(include_str!("../trusted-keys.txt")).expect("trusted-keys.txt holds valid keys")
}

pub fn public_key_text(key: &VerifyingKey) -> String {
    BASE64.encode(key.as_bytes())
}

/// Checks an envelope's signatures and the manifest's freshness. `now` and `newest_seen` (the
/// timestamp of the newest manifest seen before, or of this build) are seconds since 1970.
pub fn verify(
    bytes: &[u8],
    keys: &[VerifyingKey],
    now: u64,
    newest_seen: u64,
) -> Result<Manifest, ManifestError> {
    let manifest = verify_signature(bytes, keys)?;
    if manifest.timestamp < newest_seen {
        return Err(ManifestError::Rollback {
            signed: date(manifest.timestamp),
            newest: date(newest_seen),
        });
    }
    if now > manifest.expires {
        return Err(ManifestError::Expired(date(manifest.expires)));
    }
    Ok(manifest)
}

/// Checks an envelope's signatures and the manifest's format, but not its age: for the release
/// tooling, which extends the current manifest.
pub fn verify_signature(bytes: &[u8], keys: &[VerifyingKey]) -> Result<Manifest, ManifestError> {
    if keys.is_empty() {
        return Err(ManifestError::NoKeys);
    }
    let envelope: Envelope =
        serde_json::from_slice(bytes).map_err(|error| ManifestError::Format(error.to_string()))?;
    let mut result = Err(ManifestError::Untrusted);
    for signature in &envelope.signatures {
        let Some(key) = keys
            .iter()
            .find(|key| public_key_text(key) == signature.key.trim())
        else {
            continue;
        };
        let valid = BASE64
            .decode(signature.signature.trim())
            .ok()
            .and_then(|bytes| ed25519_dalek::Signature::from_slice(&bytes).ok())
            .is_some_and(|signature| {
                key.verify_strict(envelope.manifest.as_bytes(), &signature)
                    .is_ok()
            });
        if valid {
            result = Ok(());
            break;
        }
        result = Err(ManifestError::BadSignature);
    }
    result?;
    // Only signed bytes are parsed further.
    let header: Header = serde_json::from_str(&envelope.manifest)
        .map_err(|error| ManifestError::Format(error.to_string()))?;
    if header.product != PRODUCT {
        return Err(ManifestError::Product(header.product));
    }
    if header.schema != SCHEMA {
        return Err(ManifestError::Schema(header.schema));
    }
    serde_json::from_str(&envelope.manifest)
        .map_err(|error| ManifestError::Format(error.to_string()))
}

/// The fields every manifest format keeps, to report a newer format as such.
#[derive(Deserialize)]
struct Header {
    schema: u32,
    product: String,
}

/// Signs `manifest` with every key in `keys`.
pub fn sign(manifest: &Manifest, keys: &[SigningKey]) -> Envelope {
    let text = serde_json::to_string_pretty(manifest).expect("a manifest serializes");
    let signatures = keys
        .iter()
        .map(|key| Signature {
            key: public_key_text(&key.verifying_key()),
            signature: BASE64.encode(key.sign(text.as_bytes()).to_bytes()),
        })
        .collect();
    Envelope {
        manifest: text,
        signatures,
    }
}

/// Reads a signing key: the base64 of its 32-byte secret.
pub fn parse_signing_key(text: &str) -> Result<SigningKey, String> {
    let bytes: [u8; 32] = BASE64
        .decode(text.trim())
        .ok()
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or("not the base64 of a 32-byte ed25519 secret key")?;
    Ok(SigningKey::from_bytes(&bytes))
}

pub fn signing_key_text(key: &SigningKey) -> String {
    BASE64.encode(key.to_bytes())
}

impl Manifest {
    /// The newest release on `channel`.
    pub fn newest(&self, channel: UpdateChannel) -> Option<&ManifestRelease> {
        self.releases
            .iter()
            .filter(|release| on_channel(&release.version, channel))
            .max_by(|a, b| a.version.cmp(&b.version))
    }
}

impl ManifestRelease {
    /// This release's installer package for `platform`.
    pub fn package(&self, platform: &str) -> Option<&Package> {
        self.packages
            .iter()
            .find(|package| package.platform == platform)
    }
}

/// A date for messages: `2026-10-05` (UTC).
pub fn date(seconds: u64) -> String {
    // Days since 1970 to a civil date (Howard Hinnant's algorithm).
    let days = (seconds / 86_400) as i64 + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: u64 = 86_400;
    const NOW: u64 = 1_790_000_000;

    fn key(byte: u8) -> SigningKey {
        SigningKey::from_bytes(&[byte; 32])
    }

    fn release(version: &str) -> ManifestRelease {
        ManifestRelease {
            version: Version::parse(version).unwrap(),
            page: format!("https://example.com/{version}"),
            packages: vec![Package {
                platform: "windows-x64".into(),
                file: format!("Birchpad-{version}-full.nupkg"),
                url: format!("https://example.com/{version}/full.nupkg"),
                size: 10,
                sha256: "ab".repeat(32),
                sha1: "cd".repeat(20),
            }],
        }
    }

    fn manifest(timestamp: u64) -> Manifest {
        Manifest {
            schema: SCHEMA,
            product: PRODUCT.into(),
            timestamp,
            expires: timestamp + 30 * DAY,
            releases: vec![
                release("0.3.0-nightly.20261005.7"),
                release("0.2.1-beta.1"),
                release("0.2.0"),
            ],
        }
    }

    fn bytes(envelope: &Envelope) -> Vec<u8> {
        serde_json::to_vec(envelope).unwrap()
    }

    #[test]
    fn signed_manifests_verify_and_tampered_ones_do_not() {
        let keys = [key(1).verifying_key()];
        let envelope = sign(&manifest(NOW), &[key(1)]);
        assert_eq!(verify(&bytes(&envelope), &keys, NOW, 0), Ok(manifest(NOW)));

        let mut tampered = envelope.clone();
        tampered.manifest = tampered.manifest.replace("0.2.0", "9.9.9");
        assert_eq!(
            verify(&bytes(&tampered), &keys, NOW, 0),
            Err(ManifestError::BadSignature)
        );
        assert_eq!(
            verify(&bytes(&envelope), &[key(2).verifying_key()], NOW, 0),
            Err(ManifestError::Untrusted)
        );
        assert_eq!(
            verify(&bytes(&envelope), &[], NOW, 0),
            Err(ManifestError::NoKeys)
        );
        assert!(matches!(
            verify(b"<html>", &keys, NOW, 0),
            Err(ManifestError::Format(_))
        ));
    }

    #[test]
    fn a_new_key_can_sign_along_with_the_old_one() {
        let envelope = sign(&manifest(NOW), &[key(1), key(2)]);
        for trusted in [key(1), key(2)] {
            let keys = [trusted.verifying_key()];
            assert!(verify(&bytes(&envelope), &keys, NOW, 0).is_ok());
        }
        // A bad signature by one trusted key does not hide a good one by another.
        let mut envelope = envelope;
        envelope.signatures[0].signature = envelope.signatures[1].signature.clone();
        let keys = [key(1).verifying_key(), key(2).verifying_key()];
        assert!(verify(&bytes(&envelope), &keys, NOW, 0).is_ok());
    }

    #[test]
    fn old_and_expired_manifests_are_refused() {
        let keys = [key(1).verifying_key()];
        let envelope = bytes(&sign(&manifest(NOW), &[key(1)]));
        assert!(
            verify(&envelope, &keys, NOW, NOW).is_ok(),
            "the same one again"
        );
        assert_eq!(
            verify(&envelope, &keys, NOW, NOW + 1),
            Err(ManifestError::Rollback {
                signed: date(NOW),
                newest: date(NOW + 1)
            })
        );
        assert_eq!(
            verify(&envelope, &keys, NOW + 31 * DAY, 0),
            Err(ManifestError::Expired(date(NOW + 30 * DAY)))
        );
        // The release tooling extends an expired manifest.
        assert!(verify_signature(&envelope, &keys).is_ok());
    }

    #[test]
    fn other_products_and_formats_are_refused_after_the_signature() {
        let keys = [key(1).verifying_key()];
        let mut other = manifest(NOW);
        other.product = "notepad".into();
        assert_eq!(
            verify(&bytes(&sign(&other, &[key(1)])), &keys, NOW, 0),
            Err(ManifestError::Product("notepad".into()))
        );
        let mut newer = Envelope {
            manifest: r#"{"schema": 2, "product": "birchpad", "anything": true}"#.into(),
            signatures: Vec::new(),
        };
        newer.signatures = sign(&manifest(NOW), &[key(1)]).signatures;
        let text = newer.manifest.clone();
        newer.signatures[0].signature = BASE64.encode(key(1).sign(text.as_bytes()).to_bytes());
        assert_eq!(
            verify(&bytes(&newer), &keys, NOW, 0),
            Err(ManifestError::Schema(2))
        );
    }

    #[test]
    fn channels_and_platforms() {
        let manifest = manifest(NOW);
        let version = |channel| {
            manifest
                .newest(channel)
                .map(|release| release.version.to_string())
        };
        assert_eq!(version(UpdateChannel::Stable).as_deref(), Some("0.2.0"));
        assert_eq!(
            version(UpdateChannel::Beta).as_deref(),
            Some("0.2.1-beta.1")
        );
        assert_eq!(
            version(UpdateChannel::Nightly).as_deref(),
            Some("0.3.0-nightly.20261005.7")
        );
        let release = manifest.newest(UpdateChannel::Stable).unwrap();
        assert!(release.package("windows-x64").is_some());
        assert!(release.package("windows-arm64").is_none());
    }

    #[test]
    fn keys_round_trip_through_text() {
        let text = format!(
            "# Birchpad update keys\n{}  # 2026-10-05\n\n",
            public_key_text(&key(3).verifying_key())
        );
        assert_eq!(parse_keys(&text).unwrap(), [key(3).verifying_key()]);
        assert!(parse_keys("not a key").is_err());
        let secret = signing_key_text(&key(4));
        assert_eq!(parse_signing_key(&secret).unwrap().to_bytes(), [4; 32]);
        assert!(parse_signing_key("c2hvcnQ=").is_err());
    }

    #[test]
    fn dates() {
        assert_eq!(date(0), "1970-01-01");
        assert_eq!(date(951_782_400), "2000-02-29");
        assert_eq!(date(1_790_000_000), "2026-09-21");
    }
}
