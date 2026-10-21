//! The online catalog: read and checked, its files downloaded and checked against their pinned
//! size and SHA-256.

use std::collections::HashMap;
use std::io::Write as _;

use birchpad_update::catalog::*;
use birchpad_update::{Transport, hex};
use sha2::{Digest as _, Sha256};

/// Serves files by their address.
struct Serve(HashMap<String, Vec<u8>>);

impl Transport for Serve {
    fn get(&self, url: &str) -> Result<Vec<u8>, String> {
        self.0.get(url).cloned().ok_or_else(|| format!("no {url}"))
    }
}

fn pinned(url: &str, body: &[u8]) -> CatalogFile {
    CatalogFile {
        url: url.to_owned(),
        size: body.len() as u64,
        sha256: hex(&Sha256::digest(body)),
    }
}

fn zip_of(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    for (name, bytes) in files {
        zip.start_file(*name, options).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

const SHA: &str = "c7a8c4d08c29d237880844b1623099f59092602f189be38ce3912e457ff38bc1";

fn catalog_json(plugins: &str, dictionaries: &str) -> String {
    format!(r#"{{"format": 1, "plugins": [{plugins}], "dictionaries": [{dictionaries}]}}"#)
}

fn plugin_json(id: &str, url: &str, size: u64, sha: &str) -> String {
    format!(
        r#"{{"id": "{id}", "name": "Name", "version": "1.0.0", "api": "0.2", "file": {{"url": "{url}", "size": {size}, "sha256": "{sha}"}}}}"#
    )
}

#[test]
fn reads_birchpads_own_catalog() {
    let catalog = parse(include_bytes!(
        "../../../../../packaging/catalog/catalog.json"
    ))
    .unwrap();
    assert_eq!(catalog.format, FORMAT);
    assert!(catalog.dictionaries.iter().any(|d| d.language == "en_US"));
    // Its en_US is the dictionary Birchpad has built in.
    let en = &catalog.dictionaries[0];
    let aff = include_bytes!("../../../../app/assets/dictionaries/en_US.aff");
    let dic = include_bytes!("../../../../app/assets/dictionaries/en_US.dic");
    assert_eq!(en.aff.size, aff.len() as u64);
    assert_eq!(en.aff.sha256, hex(&Sha256::digest(aff)));
    assert_eq!(en.dic.sha256, hex(&Sha256::digest(dic)));
}

#[test]
fn reads_a_catalog_with_defaults() {
    let catalog = parse(br#"{"format": 1}"#).unwrap();
    assert!(catalog.plugins.is_empty() && catalog.dictionaries.is_empty());
    let one = catalog_json(
        &plugin_json("word-count", "https://x.org/a.zip", 10, SHA),
        "",
    );
    let catalog = parse(one.as_bytes()).unwrap();
    assert_eq!(catalog.plugins[0].id, "word-count");
    assert_eq!(catalog.plugins[0].description, "");
}

#[test]
fn refuses_catalogs_that_do_not_make_sense() {
    let plugin = |id: &str, url: &str, size: u64, sha: &str| {
        catalog_json(&plugin_json(id, url, size, sha), "")
    };
    let twice = catalog_json(
        &format!(
            "{},{}",
            plugin_json("a", "https://x.org/a.zip", 1, SHA),
            plugin_json("a", "https://x.org/b.zip", 1, SHA)
        ),
        "",
    );
    let dictionary = |language: &str| {
        format!(
            r#"{{"language": "{language}", "name": "N", "aff": {{"url": "https://x.org/a", "size": 1, "sha256": "{SHA}"}}, "dic": {{"url": "https://x.org/d", "size": 1, "sha256": "{SHA}"}}}}"#
        )
    };
    for (text, expected) in [
        ("not json".to_owned(), "does not read"),
        ("{}".to_owned(), "does not read"),
        (
            r#"{"format": 2, "plugins": "anything"}"#.to_owned(),
            "format 2",
        ),
        (r#"{"format": 1, "extra": 1}"#.to_owned(), "does not read"),
        (
            plugin("Bad", "https://x.org/a.zip", 1, SHA),
            "not a plugin id",
        ),
        (
            plugin("-a", "https://x.org/a.zip", 1, SHA),
            "not a plugin id",
        ),
        (
            plugin("a", "http://x.org/a.zip", 1, SHA),
            "not an https address",
        ),
        (plugin("a", "https://x.org/a.zip", 0, SHA), "0 bytes"),
        (
            plugin("a", "https://x.org/a.zip", MAX_FILE + 1, SHA),
            "bytes",
        ),
        (
            plugin("a", "https://x.org/a.zip", 1, "abc"),
            "not a SHA-256",
        ),
        (
            plugin("a", "https://x.org/a.zip", 1, &"z".repeat(64)),
            "not a SHA-256",
        ),
        (twice, "comes twice"),
        (catalog_json("", &dictionary("../x")), "not a language"),
        (
            catalog_json(
                "",
                &format!("{},{}", dictionary("en_GB"), dictionary("en_GB")),
            ),
            "comes twice",
        ),
    ] {
        let error = parse(text.as_bytes()).unwrap_err().to_string();
        assert!(error.contains(expected), "{text}: {error}");
    }
    // At the limits.
    assert!(parse(plugin("a", "https://x.org/a.zip", MAX_FILE, SHA).as_bytes()).is_ok());
    assert!(parse(plugin(&"a".repeat(64), "https://x.org/a.zip", 1, SHA).as_bytes()).is_ok());
    assert!(parse(plugin(&"a".repeat(65), "https://x.org/a.zip", 1, SHA).as_bytes()).is_err());
    let large = vec![b' '; MAX_CATALOG + 1];
    assert!(parse(&large).is_err());
}

#[test]
fn compares_versions() {
    assert!(is_newer("1.2.0", "1.1.9"));
    assert!(is_newer("1.10.0", "1.9.0"));
    assert!(!is_newer("1.0.0", "1.0.0"));
    assert!(!is_newer("0.9.0", "1.0.0"));
    assert!(!is_newer("1.0.0-beta", "1.0.0"));
    // Not semantic versions: newer when different.
    assert!(is_newer("2026a", "2025b"));
    assert!(!is_newer(" r5 ", "r5"));
}

#[test]
fn downloads_and_checks_files() {
    let dir = tempfile::tempdir().unwrap();
    let body = b"the file".to_vec();
    let file = pinned("https://x.org/f", &body);
    let transport = Serve(HashMap::from([(file.url.clone(), body.clone())]));
    let to = dir.path().join("f");
    download(&transport, &file, &to, &|_| {}).unwrap();
    assert_eq!(std::fs::read(&to).unwrap(), body);
    // Another SHA-256, or another size: refused, nothing left.
    let wrong = CatalogFile {
        sha256: SHA.to_owned(),
        ..file.clone()
    };
    let error = download(&transport, &wrong, &to, &|_| {}).unwrap_err();
    assert!(error.contains("not the file"), "{error}");
    assert!(!to.exists());
    let larger = CatalogFile {
        size: body.len() as u64 + 1,
        ..file.clone()
    };
    assert!(download(&transport, &larger, &to, &|_| {}).is_err());
    // Not there.
    let missing = CatalogFile {
        url: "https://x.org/missing".into(),
        ..file
    };
    let error = download(&transport, &missing, &to, &|_| {}).unwrap_err();
    assert!(error.contains("missing"), "{error}");
    let fetched = fetch(&transport, "https://x.org/f");
    assert!(fetched.unwrap_err().contains("does not read"));
}

#[test]
fn fetches_a_plugin_and_unpacks_it() {
    let dir = tempfile::tempdir().unwrap();
    let body = zip_of(&[("plugin.toml", b"id = \"p\""), ("plugin.wasm", b"\0asm")]);
    let file = pinned("https://x.org/p.zip", &body);
    let plugin = CatalogPlugin {
        id: "p".into(),
        name: "P".into(),
        version: "1".into(),
        description: String::new(),
        author: String::new(),
        api: "0.2".into(),
        file: file.clone(),
    };
    let transport = Serve(HashMap::from([(file.url.clone(), body)]));
    let work = dir.path().join("work");
    std::fs::create_dir_all(work.join("stale")).unwrap();
    let manifest = fetch_plugin(&transport, &plugin, &work, &|_| {}).unwrap();
    assert!(manifest.ends_with("plugin.toml"));
    assert_eq!(
        std::fs::read(manifest.with_file_name("plugin.wasm")).unwrap(),
        b"\0asm"
    );
    assert!(!work.join("stale").exists() && !work.join("plugin.zip").exists());

    // A ZIP without plugin.toml at the top is no plugin.
    let body = zip_of(&[("nested/plugin.toml", b"")]);
    let file = pinned("https://x.org/q.zip", &body);
    let transport = Serve(HashMap::from([(file.url.clone(), body)]));
    let error = fetch_plugin(
        &transport,
        &CatalogPlugin { file, ..plugin },
        &work,
        &|_| {},
    )
    .unwrap_err();
    assert!(error.contains("plugin.toml"), "{error}");
}

#[test]
fn fetches_a_dictionary_whole_or_not_at_all() {
    let dir = tempfile::tempdir().unwrap();
    let (aff, dic) = (b"SET UTF-8\n".to_vec(), b"1\nhello\n".to_vec());
    let dictionary = CatalogDictionary {
        language: "xx_TEST".into(),
        name: "Test".into(),
        aff: pinned("https://x.org/xx.aff", &aff),
        dic: pinned("https://x.org/xx.dic", &dic),
    };
    let both = Serve(HashMap::from([
        (dictionary.aff.url.clone(), aff.clone()),
        (dictionary.dic.url.clone(), dic.clone()),
    ]));
    let folder = dir.path().join("dictionaries");
    let (aff_path, dic_path) = fetch_dictionary(&both, &dictionary, &folder, &|_| {}).unwrap();
    assert_eq!(std::fs::read(&aff_path).unwrap(), aff);
    assert_eq!(std::fs::read(&dic_path).unwrap(), dic);
    // The .dic cannot be had: the .aff that came is not kept, the earlier files stay.
    let half = Serve(HashMap::from([(
        dictionary.aff.url.clone(),
        b"new".to_vec(),
    )]));
    assert!(fetch_dictionary(&half, &dictionary, &folder, &|_| {}).is_err());
    assert_eq!(std::fs::read(&aff_path).unwrap(), aff);
    let left: Vec<_> = std::fs::read_dir(&folder)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    assert!(left.iter().all(|name| !name.ends_with(".part")), "{left:?}");
}
