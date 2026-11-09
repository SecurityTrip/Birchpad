//! Checks through the Update API (ADR 0064): the signed manifest it answers with, GitHub when
//! it fails, an administrator's mirror alone, and nothing with updates off.

use std::sync::Mutex;

use birchpad_config::{UpdateChannel, UpdateMode, UpdateSettings};
use birchpad_update::manifest::{PRODUCT, SCHEMA};
use birchpad_update::statistics::{PlantUml, Statistics};
use birchpad_update::*;
use ed25519_dalek::SigningKey;
use semver::Version;

const NOW: u64 = 1_790_000_000;

fn key() -> SigningKey {
    SigningKey::from_bytes(&[7; 32])
}

/// A manifest of `version` signed by `signer` at `timestamp`, valid until `expires`.
fn signed_by(version: &str, signer: &SigningKey, timestamp: u64, expires: u64) -> Vec<u8> {
    let manifest = Manifest {
        schema: SCHEMA,
        product: PRODUCT.into(),
        timestamp,
        expires,
        releases: vec![ManifestRelease {
            version: Version::parse(version).unwrap(),
            page: format!("https://example.com/{version}"),
            packages: Vec::new(),
        }],
    };
    serde_json::to_vec(&manifest::sign(&manifest, std::slice::from_ref(signer))).unwrap()
}

fn signed(version: &str) -> Vec<u8> {
    signed_by(version, &key(), NOW, NOW + 86_400)
}

fn unsigned() -> Vec<u8> {
    br#"{"manifest": "{}", "signatures": []}"#.to_vec()
}

/// A request a transport was asked: `GET <url>`, or `POST <url>` with its body.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Asked {
    Get(String),
    Post(String, Vec<u8>),
}

/// Answers posts with `post` and gets with `get`, and records what it is asked.
struct Api {
    post: Result<Vec<u8>, String>,
    get: Result<Vec<u8>, String>,
    asked: Mutex<Vec<Asked>>,
}

impl Api {
    fn new(post: Result<Vec<u8>, &str>, get: Result<Vec<u8>, &str>) -> Self {
        Self {
            post: post.map_err(str::to_owned),
            get: get.map_err(str::to_owned),
            asked: Mutex::default(),
        }
    }

    fn asked(&self) -> Vec<Asked> {
        self.asked.lock().unwrap().clone()
    }
}

impl Transport for Api {
    fn get(&self, url: &str) -> Result<Vec<u8>, String> {
        self.asked.lock().unwrap().push(Asked::Get(url.to_owned()));
        self.get.clone()
    }

    fn post_json(&self, url: &str, body: &[u8]) -> Result<Vec<u8>, String> {
        self.asked
            .lock()
            .unwrap()
            .push(Asked::Post(url.to_owned(), body.to_vec()));
        self.post.clone()
    }
}

fn settings(url: Option<&str>) -> UpdateSettings {
    UpdateSettings {
        mode: UpdateMode::Notify,
        channel: UpdateChannel::Stable,
        url: url.map(str::to_owned),
        proxy: String::new(),
        statistics: true,
    }
}

fn statistics() -> Statistics {
    Statistics {
        install_id: "0b3c6f1e-6a1f-4c4e-9a59-3c1f7f0d2a11".into(),
        version: "0.8.0".into(),
        channel: UpdateChannel::Stable,
        platform: Some("windows-x64".into()),
        plantuml: PlantUml::Component {
            version: "1.2026.9-3".into(),
        },
    }
}

fn run_with(
    settings: &UpdateSettings,
    statistics: Option<&Statistics>,
    newest_seen: u64,
    transport: &dyn Transport,
) -> Result<Checked, CheckError> {
    let keys = [key().verifying_key()];
    let current = Version::new(0, 8, 0);
    let request = CheckRequest {
        settings,
        current: &current,
        platform: Some("windows-x64"),
        keys: &keys,
        now: NOW,
        newest_seen,
        statistics,
    };
    check(&request, transport)
}

fn run(settings: &UpdateSettings, transport: &dyn Transport) -> Result<Checked, CheckError> {
    run_with(settings, None, 0, transport)
}

fn available(checked: &Checked) -> Option<Version> {
    match &checked.outcome {
        Outcome::Available(release) => Some(release.version.clone()),
        Outcome::UpToDate { .. } => None,
    }
}

/// The post of a check without statistics.
fn posted() -> Asked {
    Asked::Post(UPDATE_API.into(), b"{}".to_vec())
}

#[test]
fn the_api_is_where_checks_read_unless_a_mirror_is_set() {
    assert_eq!(UPDATE_API, "https://api.birchpad.ru/v1/updates/check");
    assert_eq!(manifest_url(&settings(None)), UPDATE_API);
    assert_eq!(
        manifest_url(&settings(Some(" https://mirror.local/m.json "))),
        "https://mirror.local/m.json"
    );
}

#[test]
fn a_signed_manifest_from_the_api_is_the_answer_and_github_is_not_asked() {
    let api = Api::new(Ok(signed("1.0.0")), Err("GitHub must not be asked"));
    let checked = run(&settings(None), &api).unwrap();
    assert_eq!(available(&checked), Some(Version::new(1, 0, 0)));
    assert_eq!(checked.timestamp, NOW);
    assert_eq!(checked.api_error, None);
    assert_eq!(api.asked(), [posted()]);

    // The same answer as GitHub's manifest gives.
    let github = Api::new(Err("down"), Ok(signed("1.0.0")));
    let from_github = run(&settings(None), &github).unwrap();
    assert_eq!(
        (from_github.outcome, from_github.timestamp),
        (checked.outcome, checked.timestamp)
    );

    // Nothing newer: up to date, from the API alone.
    let api = Api::new(Ok(signed("0.8.0")), Err("GitHub must not be asked"));
    let checked = run(&settings(None), &api).unwrap();
    assert_eq!(
        checked.outcome,
        Outcome::UpToDate {
            newest: Some(Version::new(0, 8, 0))
        }
    );
    assert_eq!(api.asked(), [posted()]);
}

#[test]
fn whatever_the_api_does_wrong_github_answers_the_same() {
    let stranger = SigningKey::from_bytes(&[9; 32]);
    let seen = NOW - 50;
    let failures: Vec<(&str, Result<Vec<u8>, &str>)> = vec![
        ("no connection", Err("connection refused")),
        (
            "503",
            Err("https://api.birchpad.ru/v1/updates/check answered 503"),
        ),
        ("unsigned", Ok(unsigned())),
        ("not JSON", Ok(b"<html>Bad Gateway</html>".to_vec())),
        ("empty", Ok(Vec::new())),
        (
            "another key",
            Ok(signed_by("9.0.0", &stranger, NOW, NOW + 86_400)),
        ),
        (
            "expired",
            Ok(signed_by("9.0.0", &key(), NOW - 2 * 86_400, NOW - 1)),
        ),
        (
            "older than seen",
            Ok(signed_by("9.0.0", &key(), seen - 1, NOW + 86_400)),
        ),
    ];
    for (what, answer) in failures {
        let api = Api::new(answer, Ok(signed("1.0.0")));
        let checked = run_with(&settings(None), None, seen, &api).unwrap();
        assert_eq!(available(&checked), Some(Version::new(1, 0, 0)), "{what}");
        assert_eq!(checked.timestamp, NOW, "{what}");
        assert_eq!(
            api.asked(),
            [posted(), Asked::Get(DEFAULT_MANIFEST.into())],
            "{what}"
        );
        // The API's error, to log, names the API.
        let error = checked.api_error.expect(what);
        assert!(error.to_string().contains(UPDATE_API), "{what}: {error}");
    }
}

#[test]
fn a_manifest_as_old_as_the_newest_seen_is_still_taken_from_the_api() {
    let seen = NOW - 50;
    let api = Api::new(
        Ok(signed_by("1.0.0", &key(), seen, NOW + 86_400)),
        Err("GitHub must not be asked"),
    );
    let checked = run_with(&settings(None), None, seen, &api).unwrap();
    assert_eq!(checked.timestamp, seen);
    assert_eq!(api.asked(), [posted()]);
}

#[test]
fn when_both_fail_the_error_is_github_s() {
    let api = Api::new(Err("connection refused"), Err("offline"));
    let error = run(&settings(None), &api).unwrap_err();
    assert_eq!(
        error,
        CheckError::Transport {
            url: DEFAULT_MANIFEST.into(),
            message: "offline".into()
        }
    );
    let api = Api::new(Ok(unsigned()), Ok(unsigned()));
    let error = run(&settings(None), &api).unwrap_err();
    assert_eq!(
        error,
        CheckError::Manifest {
            url: DEFAULT_MANIFEST.into(),
            error: ManifestError::Untrusted
        }
    );
    assert_eq!(api.asked(), [posted(), Asked::Get(DEFAULT_MANIFEST.into())]);
}

#[test]
fn a_mirror_is_read_alone_and_told_nothing() {
    let mirror = "https://mirror.local/birchpad-updates.json";
    let api = Api::new(Ok(signed("2.0.0")), Ok(signed("1.0.0")));
    let checked = run_with(&settings(Some(mirror)), Some(&statistics()), 0, &api).unwrap();
    assert_eq!(available(&checked), Some(Version::new(1, 0, 0)));
    assert_eq!(checked.api_error, None);
    assert_eq!(api.asked(), [Asked::Get(mirror.into())], "no POST at all");

    // A mirror that fails is the error: neither the API nor GitHub is asked instead.
    let api = Api::new(Ok(signed("2.0.0")), Err("mirror down"));
    let error = run_with(&settings(Some(mirror)), Some(&statistics()), 0, &api).unwrap_err();
    assert_eq!(
        error,
        CheckError::Transport {
            url: mirror.into(),
            message: "mirror down".into()
        }
    );
    assert_eq!(api.asked(), [Asked::Get(mirror.into())]);
}

#[test]
fn an_empty_mirror_is_no_mirror() {
    for empty in ["", " ", "\t \n"] {
        let api = Api::new(Ok(signed("1.0.0")), Err("GitHub must not be asked"));
        let checked = run(&settings(Some(empty)), &api).unwrap();
        assert_eq!(
            available(&checked),
            Some(Version::new(1, 0, 0)),
            "{empty:?}"
        );
        assert_eq!(api.asked(), [posted()], "{empty:?}");
        assert_eq!(mirror(&settings(Some(empty))), None);
        assert_eq!(manifest_url(&settings(Some(empty))), UPDATE_API);
    }
}

#[test]
fn without_statistics_the_body_is_exactly_empty_and_with_them_the_contract_s_fields() {
    let api = Api::new(Ok(signed("1.0.0")), Err("unused"));
    run_with(&settings(None), None, 0, &api).unwrap();
    assert_eq!(
        api.asked(),
        [Asked::Post(UPDATE_API.into(), b"{}".to_vec())]
    );

    let api = Api::new(Ok(signed("1.0.0")), Err("unused"));
    run_with(&settings(None), Some(&statistics()), 0, &api).unwrap();
    let expected = concat!(
        r#"{"installId":"0b3c6f1e-6a1f-4c4e-9a59-3c1f7f0d2a11","version":"0.8.0","#,
        r#""channel":"stable","platform":"windows-x64","plantuml":"component","#,
        r#""plantumlVersion":"1.2026.9-3"}"#
    );
    assert_eq!(
        api.asked(),
        [Asked::Post(UPDATE_API.into(), expected.as_bytes().to_vec())]
    );

    // GitHub, when the API fails, is only read: a GET carries no statistics.
    let api = Api::new(Err("down"), Ok(signed("1.0.0")));
    run_with(&settings(None), Some(&statistics()), 0, &api).unwrap();
    assert_eq!(api.asked()[1], Asked::Get(DEFAULT_MANIFEST.into()));
}

#[test]
fn off_asks_nothing_at_all() {
    for url in [None, Some(""), Some("https://mirror.local/m.json")] {
        let mut off = settings(url);
        off.mode = UpdateMode::Off;
        for statistics in [None, Some(statistics())] {
            let api = Api::new(Ok(signed("1.0.0")), Ok(signed("1.0.0")));
            assert_eq!(
                run_with(&off, statistics.as_ref(), 0, &api),
                Err(CheckError::Disabled)
            );
            assert!(api.asked().is_empty(), "{url:?}");
        }
    }
}
