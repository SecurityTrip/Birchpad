//! Checking for Birchpad updates.
//!
//! The update server publishes a signed manifest ([`manifest`], ADR 0020) listing recent releases
//! and their installer packages. [`check`] downloads it, checks its signature against the keys
//! compiled into Birchpad and its freshness, and looks for a release newer than the running one
//! on the configured channel. `updates.url` may point to a mirror of the manifest: the signature,
//! not the server, makes it trusted.
//!
//! [`check`] never touches the network when `updates.mode` is `off`.

pub mod install;
mod keys;
pub mod manifest;

use std::fs::File;
use std::io::{Read as _, Write as _};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use birchpad_config::{UpdateChannel, UpdateMode, UpdateSettings};
use semver::Version;
use sha2::{Digest as _, Sha256};

pub use ed25519_dalek::{SigningKey, VerifyingKey};
pub use manifest::{Manifest, ManifestError, ManifestRelease, Package};

/// The official update manifest: an asset of the `updates` release on GitHub, replaced by the
/// release workflows.
pub const DEFAULT_MANIFEST: &str =
    "https://github.com/SecurityTrip/Birchpad/releases/download/updates/birchpad-updates.json";

/// Manifests larger than this are rejected.
const MAX_MANIFEST_SIZE: u64 = 1024 * 1024;
/// A request that sends nothing for this long fails.
const TIMEOUT: Duration = Duration::from_secs(30);

/// A release newer than the running version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub version: Version,
    /// The release's download page (http or https).
    pub page: String,
    /// The installer package for this platform, if there is one.
    pub package: Option<Package>,
}

/// The result of a successful check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Nothing newer on the channel; `newest` is the newest release on it, if any.
    UpToDate { newest: Option<Version> },
    /// A newer release on the channel.
    Available(Release),
}

/// A successful check: its outcome and the manifest's timestamp, to remember so that an older
/// manifest is refused from now on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checked {
    pub outcome: Outcome,
    pub timestamp: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CheckError {
    #[error("checking for updates is turned off (updates.mode = \"off\")")]
    Disabled,
    #[error("cannot download {url}: {message}")]
    Transport { url: String, message: String },
    #[error("{error} ({url})")]
    Manifest { url: String, error: ManifestError },
}

/// What a check compares the manifest with.
pub struct CheckRequest<'a> {
    pub settings: &'a UpdateSettings,
    pub current: &'a Version,
    /// This build's platform, as manifests name it ([`manifest::PLATFORM`]).
    pub platform: Option<&'a str>,
    pub keys: &'a [VerifyingKey],
    /// Seconds since 1970.
    pub now: u64,
    /// The timestamp of the newest manifest seen before, or the time of this build: older
    /// manifests are refused.
    pub newest_seen: u64,
}

/// Fetches URLs. The application uses [`HttpTransport`]; tests use fakes.
pub trait Transport: Send + Sync {
    /// The body of `url`, at most [`MAX_MANIFEST_SIZE`] bytes.
    fn get(&self, url: &str) -> Result<Vec<u8>, String>;

    /// Downloads `url` into `to`, at most `limit` bytes, reporting the bytes received so far.
    fn download(
        &self,
        url: &str,
        to: &Path,
        limit: u64,
        progress: &dyn Fn(u64),
    ) -> Result<(), String> {
        let body = self.get(url)?;
        if body.len() as u64 > limit {
            return Err(format!("larger than {limit} bytes"));
        }
        progress(body.len() as u64);
        std::fs::write(to, body).map_err(|error| error.to_string())
    }
}

/// The manifest to read: `updates.url` if set, the official one otherwise.
pub fn manifest_url(settings: &UpdateSettings) -> &str {
    settings
        .url
        .as_deref()
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .unwrap_or(DEFAULT_MANIFEST)
}

/// Looks for a release newer than the running one on the configured channel.
pub fn check(request: &CheckRequest, transport: &dyn Transport) -> Result<Checked, CheckError> {
    if request.settings.mode == UpdateMode::Off {
        return Err(CheckError::Disabled);
    }
    let url = manifest_url(request.settings);
    let body = transport
        .get(url)
        .map_err(|message| CheckError::Transport {
            url: url.to_owned(),
            message,
        })?;
    let manifest = manifest::verify(&body, request.keys, request.now, request.newest_seen)
        .map_err(|error| CheckError::Manifest {
            url: url.to_owned(),
            error,
        })?;
    let newest = manifest.newest(request.settings.channel);
    let outcome = match newest {
        Some(release) if release.version > *request.current && is_web_page(&release.page) => {
            Outcome::Available(Release {
                version: release.version.clone(),
                page: release.page.clone(),
                package: request
                    .platform
                    .and_then(|platform| release.package(platform))
                    .cloned(),
            })
        }
        newest => Outcome::UpToDate {
            newest: newest.map(|release| release.version.clone()),
        },
    };
    Ok(Checked {
        outcome,
        timestamp: manifest.timestamp,
    })
}

/// Only web pages are opened from a manifest.
fn is_web_page(url: &str) -> bool {
    let url = url.to_ascii_lowercase();
    url.starts_with("https://") || url.starts_with("http://")
}

/// Whether a release with `version` belongs to `channel`. Stable takes plain versions, beta
/// also `-beta.N` and `-rc.N`, nightly takes everything.
pub fn on_channel(version: &Version, channel: UpdateChannel) -> bool {
    let pre = version.pre.as_str();
    match channel {
        UpdateChannel::Stable => pre.is_empty(),
        UpdateChannel::Beta => pre.is_empty() || pre.starts_with("beta") || pre.starts_with("rc"),
        UpdateChannel::Nightly => true,
    }
}

/// Checks that a downloaded package is the one the manifest describes.
pub fn verify_package(path: &Path, package: &Package) -> Result<(), String> {
    let mut file = File::open(path).map_err(|error| error.to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; 64 * 1024];
    let mut size = 0;
    loop {
        let read = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        size += read as u64;
        hasher.update(&buffer[..read]);
    }
    let sha256 = hex(&hasher.finalize());
    if size != package.size {
        return Err(format!(
            "{} has {size} bytes, the manifest says {}",
            package.file, package.size
        ));
    }
    if !sha256.eq_ignore_ascii_case(&package.sha256) {
        return Err(format!(
            "{} has SHA-256 {sha256}, the manifest says {}",
            package.file, package.sha256
        ));
    }
    Ok(())
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// HTTPS with the operating system's certificate store (so corporate root certificates work)
/// and the proxy from `HTTPS_PROXY`/`ALL_PROXY` or, on Windows, the system proxy settings.
pub struct HttpTransport {
    agent: ureq::Agent,
}

impl HttpTransport {
    /// Creating the transport does not touch the network.
    pub fn new(user_agent: &str) -> Self {
        Self::build(user_agent, ureq::Proxy::try_from_env())
    }

    fn build(user_agent: &str, proxy: Option<ureq::Proxy>) -> Self {
        use ureq::tls::{RootCerts, TlsConfig, TlsProvider};

        let crypto = Arc::new(rustls::crypto::ring::default_provider());
        let tls = TlsConfig::builder()
            .provider(TlsProvider::Rustls)
            .root_certs(RootCerts::PlatformVerifier)
            .unversioned_rustls_crypto_provider(crypto)
            .build();
        let agent = ureq::Agent::config_builder()
            .tls_config(tls)
            .proxy(proxy)
            .timeout_connect(Some(TIMEOUT))
            .timeout_recv_response(Some(TIMEOUT))
            .timeout_recv_body(Some(TIMEOUT))
            .user_agent(user_agent)
            .build()
            .new_agent();
        Self { agent }
    }
}

impl Transport for HttpTransport {
    fn get(&self, url: &str) -> Result<Vec<u8>, String> {
        let mut response = self
            .agent
            .get(url)
            .header("Accept", "application/json, application/octet-stream")
            .call()
            .map_err(|error| error.to_string())?;
        response
            .body_mut()
            .with_config()
            .limit(MAX_MANIFEST_SIZE)
            .read_to_vec()
            .map_err(|error| error.to_string())
    }

    fn download(
        &self,
        url: &str,
        to: &Path,
        limit: u64,
        progress: &dyn Fn(u64),
    ) -> Result<(), String> {
        let mut response = self
            .agent
            .get(url)
            .header("Accept", "application/octet-stream")
            .call()
            .map_err(|error| error.to_string())?;
        let mut reader = response.body_mut().with_config().limit(limit).reader();
        let mut file = File::create(to).map_err(|error| error.to_string())?;
        let mut buffer = vec![0; 64 * 1024];
        let mut received = 0;
        loop {
            let read = reader
                .read(&mut buffer)
                .map_err(|error| error.to_string())?;
            if read == 0 {
                break;
            }
            file.write_all(&buffer[..read])
                .map_err(|error| error.to_string())?;
            received += read as u64;
            progress(received);
        }
        file.sync_all().map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;
    use std::sync::Mutex;

    use ed25519_dalek::SigningKey;

    use super::*;
    use crate::manifest::{PRODUCT, SCHEMA};

    const NOW: u64 = 1_790_000_000;

    /// Serves canned answers and records what was asked.
    struct Fake {
        answer: Result<Vec<u8>, String>,
        requests: Mutex<Vec<String>>,
    }

    impl Fake {
        fn new(answer: Result<Vec<u8>, &str>) -> Self {
            Self {
                answer: answer.map_err(str::to_owned),
                requests: Mutex::default(),
            }
        }
    }

    impl Transport for Fake {
        fn get(&self, url: &str) -> Result<Vec<u8>, String> {
            self.requests.lock().unwrap().push(url.to_owned());
            self.answer.clone()
        }
    }

    fn key() -> SigningKey {
        SigningKey::from_bytes(&[7; 32])
    }

    fn package(version: &str) -> Package {
        Package {
            platform: "windows-x64".into(),
            file: format!("Birchpad-{version}-full.nupkg"),
            url: format!("https://example.com/{version}.nupkg"),
            size: 3,
            sha256: hex(&Sha256::digest(b"abc")),
            sha1: String::new(),
        }
    }

    fn signed(versions: &[&str]) -> Vec<u8> {
        let manifest = Manifest {
            schema: SCHEMA,
            product: PRODUCT.into(),
            timestamp: NOW,
            expires: NOW + 86_400,
            releases: versions
                .iter()
                .map(|version| ManifestRelease {
                    version: Version::parse(version).unwrap(),
                    page: format!("https://example.com/{version}"),
                    packages: vec![package(version)],
                })
                .collect(),
        };
        serde_json::to_vec(&manifest::sign(&manifest, &[key()])).unwrap()
    }

    fn settings(mode: UpdateMode, channel: UpdateChannel) -> UpdateSettings {
        UpdateSettings {
            mode,
            channel,
            url: None,
        }
    }

    fn version(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    fn run(
        settings: &UpdateSettings,
        current: &str,
        platform: Option<&str>,
        transport: &dyn Transport,
    ) -> Result<Checked, CheckError> {
        let keys = [key().verifying_key()];
        let request = CheckRequest {
            settings,
            current: &version(current),
            platform,
            keys: &keys,
            now: NOW,
            newest_seen: 0,
        };
        check(&request, transport)
    }

    #[test]
    fn off_means_no_request_at_all() {
        let fake = Fake::new(Ok(signed(&["9.0.0"])));
        let off = settings(UpdateMode::Off, UpdateChannel::Nightly);
        assert_eq!(run(&off, "0.1.0", None, &fake), Err(CheckError::Disabled));
        assert!(fake.requests.lock().unwrap().is_empty());
    }

    #[test]
    fn finds_newer_releases_on_the_channel_with_their_package() {
        let fake = Fake::new(Ok(signed(&[
            "0.3.0-nightly.20261005.7",
            "0.2.1-beta.2",
            "0.2.0",
        ])));
        let stable = settings(UpdateMode::Notify, UpdateChannel::Stable);
        let checked = run(&stable, "0.1.0", Some("windows-x64"), &fake).unwrap();
        assert_eq!(checked.timestamp, NOW);
        let Outcome::Available(release) = checked.outcome else {
            panic!("0.2.0 is newer than 0.1.0");
        };
        assert_eq!(release.version, version("0.2.0"));
        assert_eq!(release.package, Some(package("0.2.0")));
        let checked = run(&stable, "0.1.0", Some("windows-arm64"), &fake).unwrap();
        assert!(matches!(
            checked.outcome,
            Outcome::Available(Release { package: None, .. })
        ));

        assert_eq!(
            run(&stable, "0.2.0", None, &fake).unwrap().outcome,
            Outcome::UpToDate {
                newest: Some(version("0.2.0"))
            }
        );
        // A beta of the next version is newer than every stable release: no downgrade.
        assert!(matches!(
            run(&stable, "0.2.1-beta.1", None, &fake).unwrap().outcome,
            Outcome::UpToDate { .. }
        ));
        let nightly = settings(UpdateMode::Auto, UpdateChannel::Nightly);
        assert!(matches!(
            run(&nightly, "0.2.1-beta.1", None, &fake).unwrap().outcome,
            Outcome::Available(release) if release.version == version("0.3.0-nightly.20261005.7")
        ));
        assert_eq!(*fake.requests.lock().unwrap(), [DEFAULT_MANIFEST; 5]);
    }

    #[test]
    fn mirrors_serve_the_same_signed_manifest() {
        let fake = Fake::new(Ok(signed(&["1.0.0"])));
        let mut mirror = settings(UpdateMode::Notify, UpdateChannel::Stable);
        mirror.url = Some(" http://mirror.local/birchpad-updates.json ".into());
        assert!(matches!(
            run(&mirror, "0.9.0", None, &fake).unwrap().outcome,
            Outcome::Available(_)
        ));
        assert_eq!(
            *fake.requests.lock().unwrap(),
            ["http://mirror.local/birchpad-updates.json"]
        );
    }

    #[test]
    fn errors_name_the_manifest() {
        let on = settings(UpdateMode::Auto, UpdateChannel::Stable);
        let error = run(&on, "0.1.0", None, &Fake::new(Err("offline"))).unwrap_err();
        assert_eq!(
            error.to_string(),
            format!("cannot download {DEFAULT_MANIFEST}: offline")
        );
        let unsigned = br#"{"manifest": "{}", "signatures": []}"#.to_vec();
        let error = run(&on, "0.1.0", None, &Fake::new(Ok(unsigned))).unwrap_err();
        assert_eq!(
            error,
            CheckError::Manifest {
                url: DEFAULT_MANIFEST.into(),
                error: ManifestError::Untrusted
            }
        );
    }

    #[test]
    fn packages_are_checked_against_the_manifest() {
        let dir = std::env::temp_dir().join(format!("birchpad-update-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("package.nupkg");
        std::fs::write(&path, b"abc").unwrap();
        assert_eq!(verify_package(&path, &package("1.0.0")), Ok(()));
        std::fs::write(&path, b"abd").unwrap();
        assert!(
            verify_package(&path, &package("1.0.0"))
                .unwrap_err()
                .contains("SHA-256")
        );
        std::fs::write(&path, b"abcd").unwrap();
        assert!(
            verify_package(&path, &package("1.0.0"))
                .unwrap_err()
                .contains("4 bytes")
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Answers one request with `body`; returns the request's lines.
    fn serve_once(body: Vec<u8>) -> (String, std::thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            for line in BufReader::new(&stream).lines() {
                let line = line.unwrap();
                if line.is_empty() {
                    break;
                }
                request.push(line);
            }
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            stream.write_all(&body).unwrap();
            request
        });
        (format!("http://{address}"), server)
    }

    #[test]
    fn http_transport_reads_and_downloads() {
        let transport = HttpTransport::build("Birchpad/0.1.0 (test)", None);
        let (base, server) = serve_once(signed(&["1.0.0"]));
        let body = transport.get(&format!("{base}/manifest")).unwrap();
        assert!(manifest::verify(&body, &[key().verifying_key()], NOW, 0).is_ok());
        let request = server.join().unwrap();
        assert_eq!(request[0], "GET /manifest HTTP/1.1");
        assert!(
            request
                .iter()
                .any(|line| line.eq_ignore_ascii_case("user-agent: Birchpad/0.1.0 (test)"))
        );

        let dir = std::env::temp_dir().join(format!("birchpad-download-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("package.nupkg");
        let (base, server) = serve_once(vec![b'x'; 200_000]);
        let progress = Mutex::new(0);
        transport
            .download(&format!("{base}/p"), &path, 1_000_000, &|received| {
                *progress.lock().unwrap() = received;
            })
            .unwrap();
        server.join().unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().len(), 200_000);
        assert_eq!(*progress.lock().unwrap(), 200_000);

        let (base, server) = serve_once(vec![b'x'; 200_000]);
        assert!(
            transport
                .download(&format!("{base}/p"), &path, 1_000, &|_| {})
                .is_err(),
            "over the limit"
        );
        server.join().ok();
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
