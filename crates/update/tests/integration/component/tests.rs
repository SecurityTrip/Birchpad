//! Components installed on request: downloaded, checked against their pinned size and SHA-256,
//! and unpacked into the data folder.

use std::io::Write as _;
use std::path::Path;
use std::sync::Mutex;

use birchpad_update::component::*;
use birchpad_update::{Transport, hex};
use sha2::{Digest as _, Sha256};

/// Serves the same bytes for every URL, and notes what was asked.
struct Serve {
    body: Vec<u8>,
    asked: Mutex<Vec<String>>,
}

impl Serve {
    fn new(body: Vec<u8>) -> Self {
        Self {
            body,
            asked: Mutex::new(Vec::new()),
        }
    }
}

impl Transport for Serve {
    fn get(&self, url: &str) -> Result<Vec<u8>, String> {
        self.asked.lock().unwrap().push(url.to_owned());
        Ok(self.body.clone())
    }
}

/// A transport that cannot reach anything.
struct Offline;

impl Transport for Offline {
    fn get(&self, url: &str) -> Result<Vec<u8>, String> {
        Err(format!("cannot reach {url}"))
    }
}

/// A ZIP of `files`: (name, bytes).
fn zip_of(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o755);
    for (name, bytes) in files {
        zip.start_file(*name, options).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

const COMPONENT: Component = Component {
    name: "tool",
    version: "2.0-1",
    packages: &[],
};

/// The package of `body`, as Birchpad would pin it.
fn package_of(body: &[u8]) -> ComponentPackage {
    let sha256: &'static str = Box::leak(hex(&Sha256::digest(body)).into_boxed_str());
    ComponentPackage {
        platform: "test-x64",
        url: "https://example.org/tool.zip",
        size: body.len() as u64,
        sha256,
    }
}

fn leftovers(data: &Path) -> Vec<String> {
    std::fs::read_dir(data.join("components"))
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .filter(|name| name.ends_with(".part"))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn a_component_is_downloaded_checked_and_unpacked() {
    let data = tempfile::tempdir().unwrap();
    let body = zip_of(&[("bin/run", b"#!/bin/sh\n"), ("tool.jar", b"jar")]);
    let package = package_of(&body);
    let transport = Serve::new(body.clone());
    let received = Mutex::new(0);
    assert_eq!(COMPONENT.installed(data.path()), None);
    let folder = install_package(&COMPONENT, &package, &transport, data.path(), &|bytes| {
        *received.lock().unwrap() = bytes;
    })
    .unwrap();
    assert_eq!(folder, COMPONENT.folder(data.path()));
    assert!(folder.ends_with(Path::new("components").join("tool-2.0-1")));
    assert_eq!(std::fs::read(folder.join("tool.jar")).unwrap(), b"jar");
    assert_eq!(
        std::fs::read(folder.join("bin").join("run")).unwrap(),
        b"#!/bin/sh\n"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = std::fs::metadata(folder.join("bin").join("run"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o111, 0o111, "programs stay programs");
    }
    assert_eq!(COMPONENT.installed(data.path()), Some(folder.clone()));
    assert_eq!(*received.lock().unwrap(), body.len() as u64);
    assert_eq!(*transport.asked.lock().unwrap(), [package.url]);
    assert!(leftovers(data.path()).is_empty());
    // Again: the same, in place of the first.
    install_package(&COMPONENT, &package, &transport, data.path(), &|_| {}).unwrap();
    assert_eq!(COMPONENT.installed(data.path()), Some(folder));
}

#[test]
fn a_newer_version_takes_the_place_of_the_older_one() {
    let data = tempfile::tempdir().unwrap();
    let body = zip_of(&[("a", b"1")]);
    let package = package_of(&body);
    let transport = Serve::new(body);
    let older = Component {
        version: "1.0-1",
        ..COMPONENT
    };
    install_package(&older, &package, &transport, data.path(), &|_| {}).unwrap();
    // Another component's folder stays.
    std::fs::create_dir_all(data.path().join("components").join("other-1.0")).unwrap();
    install_package(&COMPONENT, &package, &transport, data.path(), &|_| {}).unwrap();
    assert_eq!(older.installed(data.path()), None);
    assert!(!older.folder(data.path()).exists());
    assert!(COMPONENT.installed(data.path()).is_some());
    assert!(data.path().join("components").join("other-1.0").is_dir());
}

#[test]
fn a_package_that_is_not_the_one_built_is_refused_and_left_nowhere() {
    let data = tempfile::tempdir().unwrap();
    let body = zip_of(&[("a", b"1")]);
    let package = package_of(&body);
    // Other bytes of the same size, and the same bytes with one more.
    let mut tampered = body.clone();
    let last = tampered.len() - 1;
    tampered[last] ^= 1;
    let mut longer = body.clone();
    longer.push(0);
    for served in [tampered, longer, Vec::new()] {
        let error = install_package(
            &COMPONENT,
            &package,
            &Serve::new(served),
            data.path(),
            &|_| {},
        )
        .unwrap_err();
        assert!(error.contains(package.url), "{error}");
        assert_eq!(COMPONENT.installed(data.path()), None);
        assert!(leftovers(data.path()).is_empty());
    }
    // Nothing to download from.
    let error = install_package(&COMPONENT, &package, &Offline, data.path(), &|_| {}).unwrap_err();
    assert!(error.contains("cannot reach"), "{error}");
    assert_eq!(COMPONENT.installed(data.path()), None);
}

#[test]
fn names_that_leave_the_folder_are_refused() {
    let data = tempfile::tempdir().unwrap();
    let body = zip_of(&[("ok", b"1"), ("../escaped", b"2")]);
    let package = package_of(&body);
    let error = install_package(
        &COMPONENT,
        &package,
        &Serve::new(body),
        data.path(),
        &|_| {},
    )
    .unwrap_err();
    assert!(error.contains("escaped"), "{error}");
    assert!(!data.path().join("escaped").exists());
    assert!(!data.path().join("components").join("escaped").exists());
    assert_eq!(COMPONENT.installed(data.path()), None);
    assert!(leftovers(data.path()).is_empty());
    // Bytes that are no ZIP at all.
    let junk = b"not a zip".to_vec();
    let package = package_of(&junk);
    assert!(
        install_package(
            &COMPONENT,
            &package,
            &Serve::new(junk),
            data.path(),
            &|_| {}
        )
        .is_err()
    );
}

#[test]
fn only_published_packages_are_offered() {
    let unpublished = Component {
        packages: &[ComponentPackage {
            platform: "test-x64",
            url: "https://example.org/x.zip",
            size: 0,
            sha256: "",
        }],
        ..COMPONENT
    };
    assert_eq!(unpublished.package("test-x64"), None);
    assert_eq!(unpublished.package("other"), None);
    let data = tempfile::tempdir().unwrap();
    let error = unpublished
        .install(&Offline, data.path(), "test-x64", &|_| {})
        .unwrap_err();
    assert!(error.contains("test-x64"), "{error}");
}

#[test]
fn plantuml_has_a_package_for_every_platform_birchpad_runs_on() {
    for platform in [
        "windows-x64",
        "windows-arm64",
        "linux-x64",
        "macos-arm64",
        "macos-x64",
    ] {
        let package = PLANTUML
            .packages
            .iter()
            .find(|package| package.platform == platform)
            .unwrap_or_else(|| panic!("{platform}"));
        assert!(package.url.starts_with(
            "https://github.com/SecurityTrip/Birchpad/releases/download/plantuml-1.2026.9-1/"
        ));
        assert!(package.url.ends_with(&format!("-{platform}.zip")));
        assert!(package.size > 1_000_000, "{platform} is published");
        assert_eq!(package.sha256.len(), 64, "{platform}");
        assert!(
            package
                .sha256
                .chars()
                .all(|ch| ch.is_ascii_hexdigit() && !ch.is_ascii_uppercase())
        );
        assert_eq!(PLANTUML.package(platform), Some(package));
    }
    // This build's platform is one of them.
    if cfg!(any(
        windows,
        target_os = "macos",
        all(target_os = "linux", target_arch = "x86_64")
    )) {
        assert!(platform().is_some_and(|platform| PLANTUML.package(platform).is_some()));
    }
}
