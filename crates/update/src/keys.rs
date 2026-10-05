//! The text form of the public keys that update manifests are signed with (ADR 0020). Shared
//! with build.rs, which checks the keys of `BIRCHPAD_TRUSTED_KEYS` and compiles them in.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use ed25519_dalek::VerifyingKey;

/// Parses public keys: base64, separated by commas, spaces or new lines; `#` starts a comment
/// that runs to the end of the line.
pub fn parse_keys(text: &str) -> Result<Vec<VerifyingKey>, String> {
    text.lines()
        .flat_map(|line| {
            line.split('#')
                .next()
                .unwrap_or_default()
                .split(|c: char| c == ',' || c.is_whitespace())
        })
        .filter(|key| !key.is_empty())
        .map(|key| {
            let bytes: [u8; 32] = BASE64
                .decode(key)
                .ok()
                .and_then(|bytes| bytes.try_into().ok())
                .ok_or_else(|| format!("{key:?} is not a base64 ed25519 public key"))?;
            VerifyingKey::from_bytes(&bytes).map_err(|error| format!("{key:?}: {error}"))
        })
        .collect()
}

pub fn public_key_text(key: &VerifyingKey) -> String {
    BASE64.encode(key.as_bytes())
}
