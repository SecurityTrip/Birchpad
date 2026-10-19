use std::path::PathBuf;

use super::*;

/// 2026-10-06, noon.
const NOW: u64 = 1_791_288_000;

fn key(byte: u8) -> SigningKey {
    SigningKey::from_bytes(&[byte; 32])
}

/// An environment with `secret` keys to sign with and `public` keys to check against.
fn env(secret: &[u8], public: &[u8]) -> Env {
    let text = |keys: Vec<String>| keys.join(",");
    Env {
        update_key: Some(text(
            secret
                .iter()
                .map(|&b| manifest::signing_key_text(&key(b)))
                .collect(),
        )),
        public_keys: Some(text(
            public
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
    let keys = verifying_keys(&Options::parse(&[]).unwrap(), env).unwrap();
    manifest::verify(&bytes, &keys, env.now, 0).unwrap()
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
        public_keys: env.public_keys.clone(),
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
    untrusting.public_keys = None;
    assert!(add(&untrusting).is_err());
    assert!(!dir.path().join("manifest.json").exists());

    // While a key is being replaced: the old and the new one sign, the old one is trusted.
    add(&self::env(&[1, 2], &[1])).unwrap();
    let bytes = fs::read(dir.path().join("manifest.json")).unwrap();
    for signer in [1, 2] {
        let keys = [key(signer).verifying_key()];
        assert!(
            manifest::verify(&bytes, &keys, NOW, 0).is_ok(),
            "key {signer}"
        );
    }
}

#[test]
fn trusted_keys_can_come_from_a_file() {
    let dir = tempfile::tempdir().unwrap();
    let keys_file = dir.path().join("keys.txt");
    let public = manifest::public_key_text(&key(1).verifying_key());
    fs::write(&keys_file, format!("# trusted\n{public}\n")).unwrap();
    let options = Options::parse(&args(&format!("--keys-file {}", keys_file.display()))).unwrap();
    let mut env = env(&[1], &[]);
    env.public_keys = None;
    assert_eq!(
        verifying_keys(&options, &env).unwrap(),
        [key(1).verifying_key()]
    );
    // A missing file, one with no key, one with a broken key.
    let missing = Options::parse(&args("--keys-file nowhere.txt")).unwrap();
    assert!(verifying_keys(&missing, &env).is_err());
    fs::write(&keys_file, "# nothing yet\n").unwrap();
    assert!(verifying_keys(&options, &env).is_err());
    fs::write(&keys_file, "not-a-key").unwrap();
    assert!(verifying_keys(&options, &env).is_err());
}

#[test]
fn public_key_needs_a_secret_key() {
    assert!(public_key(&env(&[1], &[])).is_ok());
    let mut none = env(&[1], &[]);
    none.update_key = None;
    assert!(public_key(&none).is_err());
}
