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
    let (three, five) = (key(3).verifying_key(), key(5).verifying_key());
    let text = format!(
        "# Birchpad update keys\r\n{}  # 2026-10-05\n\n",
        public_key_text(&three)
    );
    assert_eq!(parse_keys(&text).unwrap(), [three]);
    // As a repository variable: on one line, or one per line.
    let text = format!("{}, {}", public_key_text(&three), public_key_text(&five));
    assert_eq!(parse_keys(&text).unwrap(), [three, five]);
    assert_eq!(parse_keys(" \n").unwrap(), []);
    assert!(parse_keys("not a key").is_err());
    // 31 or 33 bytes are no key; neither are 32 bytes that are no point of the curve.
    assert!(parse_keys(&BASE64.encode([1; 31])).is_err());
    assert!(parse_keys(&BASE64.encode([1; 33])).is_err());
    let off_curve = (0..=255u8)
        .map(|y| {
            let mut bytes = [0; 32];
            bytes[0] = y;
            bytes
        })
        .find(|bytes| VerifyingKey::from_bytes(bytes).is_err())
        .expect("some y has no point");
    let error = parse_keys(&BASE64.encode(off_curve)).unwrap_err();
    assert!(error.starts_with('"'), "{error}");
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
