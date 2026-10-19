use std::sync::{Mutex, mpsc};

use sha2::{Digest as _, Sha256};

use super::*;

/// A server that answers every request with the same bytes.
struct Serve(&'static [u8]);

impl Transport for Serve {
    fn get(&self, _url: &str) -> Result<Vec<u8>, String> {
        Ok(self.0.to_vec())
    }
}

/// The source of a manifest that lists `listed`, from a server that serves `served`.
fn source(listed: &[u8], served: &'static [u8]) -> SignedSource {
    let file = "birchpad-0.2.0-windows-x64-full.nupkg";
    SignedSource {
        version: "0.2.0".to_owned(),
        package: Package {
            platform: "windows-x64".to_owned(),
            file: file.to_owned(),
            url: format!("https://example.org/{file}"),
            size: listed.len() as u64,
            sha256: crate::hex(&Sha256::digest(listed)),
            sha1: "unused".to_owned(),
        },
        transport: Arc::new(Serve(served)),
        progress: Arc::new(|_| {}),
    }
}

fn feed(source: &SignedSource) -> Vec<VelopackAsset> {
    source
        .get_release_feed("windows-x64-stable", &bundle::Manifest::default(), "")
        .unwrap()
        .Assets
}

#[test]
fn the_feed_is_the_package_of_the_manifest() {
    let source = source(b"package", b"package");
    let feed = feed(&source);
    assert_eq!(feed.len(), 1);
    let asset = &feed[0];
    assert_eq!(
        (asset.PackageId.as_str(), asset.Version.as_str()),
        (PACKAGE_ID, "0.2.0")
    );
    assert_eq!(asset.Type, "Full");
    assert_eq!(asset.FileName, source.package.file);
    assert_eq!(asset.SHA256, source.package.sha256);
    assert_eq!(asset.Size, 7);
}

#[test]
fn only_the_package_of_the_manifest_is_downloaded() {
    let dir = tempfile::tempdir().unwrap();
    let to = dir.path().join("package.nupkg");
    let mut good = source(b"package", b"package");
    let received = Arc::new(Mutex::new(Vec::new()));
    let reports = received.clone();
    good.progress = Arc::new(move |bytes| reports.lock().unwrap().push(bytes));
    let asset = feed(&good).remove(0);
    let (sender, progress) = mpsc::channel();
    good.download_release_entry(&asset, &to, Some(sender))
        .unwrap();
    assert_eq!(std::fs::read(&to).unwrap(), b"package");
    assert_eq!(progress.try_iter().last(), Some(100));
    assert_eq!(
        *received.lock().unwrap(),
        [7],
        "the bytes, not a percentage"
    );

    // Other bytes of the same size, from a server or a mirror.
    let tampered = source(b"package", b"pockage");
    let error = tampered
        .download_release_entry(&asset, &to, None)
        .unwrap_err();
    assert!(error.to_string().contains("SHA-256"), "{error}");
    assert!(!to.exists(), "a package that does not match is deleted");

    // Velopack asking for another file than the manifest's.
    let mut other = asset.clone();
    other.FileName = "other.nupkg".to_owned();
    assert!(good.download_release_entry(&other, &to, None).is_err());
    assert!(!to.exists());

    // A server that sends fewer bytes than the manifest lists.
    let short = source(b"package", b"pack");
    assert!(short.download_release_entry(&asset, &to, None).is_err());
    assert!(!to.exists());
}

#[test]
fn an_empty_package_reports_progress_without_dividing_by_zero() {
    let dir = tempfile::tempdir().unwrap();
    let to = dir.path().join("package.nupkg");
    let empty = source(b"", b"");
    let asset = feed(&empty).remove(0);
    let (sender, progress) = mpsc::channel();
    empty
        .download_release_entry(&asset, &to, Some(sender))
        .unwrap();
    assert_eq!(std::fs::read(&to).unwrap(), b"");
    assert!(progress.try_iter().all(|percent| percent == 0));
}

#[test]
fn restarting_without_a_download_is_refused() {
    assert_eq!(
        Velopack::default().restart(),
        Err("nothing was downloaded".to_owned())
    );
}

#[test]
fn a_development_build_is_no_installation() {
    // Tests run from target/, which Velopack did not install: nothing to update with,
    // and a download attempt fails cleanly instead of panicking.
    assert!(detect().is_none());
    let source = source(b"package", b"package");
    let result = Velopack::default().download(
        &Version::new(0, 2, 0),
        &source.package,
        source.transport.clone(),
        Arc::new(|_| {}),
    );
    assert!(result.is_err());
}
