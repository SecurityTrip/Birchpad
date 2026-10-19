//! The signed update manifest (ADR 0020).
//!
//! The update server publishes one file, an [`Envelope`]: the manifest as JSON text and ed25519
//! signatures of exactly those bytes. Birchpad trusts the public keys compiled into it (those of
//! `BIRCHPAD_TRUSTED_KEYS` when it was built, see build.rs), not the server or the connection: a
//! manifest that is not signed by one of them is refused, whoever serves it.
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

pub use crate::keys::{parse_keys, public_key_text};
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
    #[error("{}", birchpad_i18n::t!("update.manifest-does-not-read", reason = .0))]
    Format(String),
    #[error("{}", birchpad_i18n::t!("update.no-keys"))]
    NoKeys,
    #[error("{}", birchpad_i18n::t!("update.untrusted"))]
    Untrusted,
    #[error("{}", birchpad_i18n::t!("update.bad-signature"))]
    BadSignature,
    #[error("{}", birchpad_i18n::t!("update.other-product", product = format!("{:?}", .0)))]
    Product(String),
    #[error("{}", birchpad_i18n::t!("update.newer-format", format = .0, known = SCHEMA))]
    Schema(u32),
    #[error("{}", birchpad_i18n::t!("update.expired", date = .0))]
    Expired(String),
    #[error("{}", birchpad_i18n::t!("update.rollback", signed = .signed, newest = .newest))]
    Rollback { signed: String, newest: String },
}

/// The keys compiled into Birchpad: those of `BIRCHPAD_TRUSTED_KEYS` when it was built, which
/// build.rs checked. Development builds have none.
pub fn trusted_keys() -> Vec<VerifyingKey> {
    parse_keys(include_str!(concat!(env!("OUT_DIR"), "/trusted-keys.txt")))
        .expect("build.rs checked the trusted keys")
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
#[path = "../tests/unit/manifest/tests.rs"]
mod tests;
