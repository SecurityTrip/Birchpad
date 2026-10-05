//! Compiles in the public keys that update manifests must be signed with (ADR 0020): those of
//! the `BIRCHPAD_TRUSTED_KEYS` environment variable, which the release workflows set from the
//! repository variable of that name. Builds without it, such as development builds, trust no key
//! and refuse every manifest. A key that is not valid fails the build, not the update check.

use std::path::Path;

#[allow(unreachable_pub, reason = "shared with the library")]
#[path = "src/keys.rs"]
mod keys;

fn main() {
    println!("cargo::rerun-if-env-changed=BIRCHPAD_TRUSTED_KEYS");
    let text = std::env::var("BIRCHPAD_TRUSTED_KEYS").unwrap_or_default();
    let keys = keys::parse_keys(&text).unwrap_or_else(|error| {
        panic!("BIRCHPAD_TRUSTED_KEYS holds a key that is not valid: {error}")
    });
    let text: String = keys
        .iter()
        .map(|key| keys::public_key_text(key) + "\n")
        .collect();
    let out = std::env::var_os("OUT_DIR").expect("cargo sets OUT_DIR");
    std::fs::write(Path::new(&out).join("trusted-keys.txt"), text)
        .expect("cannot write the trusted keys");
}
