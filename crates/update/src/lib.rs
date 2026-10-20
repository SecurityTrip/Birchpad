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
pub const MAX_MANIFEST_SIZE: u64 = 1024 * 1024;
/// Connecting, or waiting for the answer to begin, fails after this long.
const TIMEOUT: Duration = Duration::from_secs(30);
/// A body that receives nothing for this long fails as stalled. There is no limit on the time a
/// body takes as a whole: while bytes arrive, however slowly, the download goes on.
pub const STALL: Duration = Duration::from_secs(60);

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
    #[error("{}", birchpad_i18n::t!("update.turned-off"))]
    Disabled,
    #[error("{}", birchpad_i18n::t!("update.cannot-download", url = .url, reason = .message))]
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
    stall: Duration,
}

impl HttpTransport {
    /// Creating the transport does not touch the network.
    pub fn new(user_agent: &str) -> Self {
        Self::build(user_agent, ureq::Proxy::try_from_env(), STALL)
    }

    /// A transport through `proxy` (`None`: direct) whose bodies fail as stalled when nothing
    /// arrives for `stall`. Creating it does not touch the network.
    pub fn build(user_agent: &str, proxy: Option<ureq::Proxy>, stall: Duration) -> Self {
        use ureq::tls::{RootCerts, TlsConfig, TlsProvider};
        use ureq::unversioned::resolver::DefaultResolver;
        use ureq::unversioned::transport::{Connector as _, DefaultConnector};

        let crypto = Arc::new(rustls::crypto::ring::default_provider());
        let tls = TlsConfig::builder()
            .provider(TlsProvider::Rustls)
            .root_certs(RootCerts::PlatformVerifier)
            .unversioned_rustls_crypto_provider(crypto)
            .build();
        // No `timeout_recv_body`: it bounds the body as a whole. Each read waits at most `stall`.
        let config = ureq::Agent::config_builder()
            .tls_config(tls)
            .proxy(proxy)
            .timeout_connect(Some(TIMEOUT))
            .timeout_recv_response(Some(TIMEOUT))
            .user_agent(user_agent)
            .build();
        let connector = DefaultConnector::new().chain(stall::Connector(stall));
        let agent = ureq::Agent::with_parts(config, connector, DefaultResolver::default());
        Self { agent, stall }
    }

    /// A failed read of a body, said as a stall when nothing arrived in time.
    fn read_error(&self, error: &std::io::Error) -> String {
        let timed_out = error.kind() == std::io::ErrorKind::TimedOut
            || error
                .get_ref()
                .and_then(|inner| inner.downcast_ref::<ureq::Error>())
                .is_some_and(|inner| matches!(inner, ureq::Error::Timeout(_)));
        if timed_out {
            birchpad_i18n::t!("update.stalled", seconds = self.stall.as_secs())
        } else {
            error.to_string()
        }
    }
}

mod stall {
    //! Reads that wait at most a given time for the next bytes, whatever the timeouts ureq
    //! would allow them: a stalled body fails without a limit on the body as a whole.

    use std::time::Duration;

    use ureq::Timeout;
    use ureq::unversioned::transport::{self, Buffers, ConnectionDetails, NextTimeout, Transport};

    #[derive(Debug)]
    pub(super) struct Connector(pub(super) Duration);

    impl transport::Connector<Box<dyn Transport>> for Connector {
        type Out = Stalling;

        fn connect(
            &self,
            _: &ConnectionDetails,
            chained: Option<Box<dyn Transport>>,
        ) -> Result<Option<Self::Out>, ureq::Error> {
            Ok(chained.map(|inner| Stalling {
                inner,
                stall: self.0,
            }))
        }
    }

    #[derive(Debug)]
    pub(super) struct Stalling {
        inner: Box<dyn Transport>,
        stall: Duration,
    }

    impl Transport for Stalling {
        fn buffers(&mut self) -> &mut dyn Buffers {
            self.inner.buffers()
        }

        fn transmit_output(
            &mut self,
            amount: usize,
            timeout: NextTimeout,
        ) -> Result<(), ureq::Error> {
            self.inner.transmit_output(amount, timeout)
        }

        fn await_input(&mut self, timeout: NextTimeout) -> Result<bool, ureq::Error> {
            let timeout = if *timeout.after > self.stall {
                NextTimeout {
                    after: self.stall.into(),
                    reason: Timeout::RecvBody,
                }
            } else {
                timeout
            };
            self.inner.await_input(timeout)
        }

        fn is_open(&mut self) -> bool {
            self.inner.is_open()
        }

        fn is_tls(&self) -> bool {
            self.inner.is_tls()
        }
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
        // ureq fails the read after its limit is reached even when the body ends there: allow a
        // byte more, so that a body of exactly the limit ends normally, and check the size here.
        let body = response
            .body_mut()
            .with_config()
            .limit(MAX_MANIFEST_SIZE + 1)
            .read_to_vec()
            .map_err(|error| self.read_error(&error.into_io()))?;
        if body.len() as u64 > MAX_MANIFEST_SIZE {
            return Err(format!("larger than {MAX_MANIFEST_SIZE} bytes"));
        }
        Ok(body)
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
        // As in `get`: a package is exactly as large as the manifest says, which is the limit.
        let mut reader = response
            .body_mut()
            .with_config()
            .limit(limit.saturating_add(1))
            .reader();
        let mut file = File::create(to).map_err(|error| error.to_string())?;
        let mut buffer = vec![0; 64 * 1024];
        let mut received = 0;
        loop {
            let read = reader
                .read(&mut buffer)
                .map_err(|error| self.read_error(&error))?;
            if read == 0 {
                break;
            }
            file.write_all(&buffer[..read])
                .map_err(|error| error.to_string())?;
            received += read as u64;
            if received > limit {
                return Err(format!("larger than {limit} bytes"));
            }
            progress(received);
        }
        file.sync_all().map_err(|error| error.to_string())
    }
}
