//! Checking for Birchpad updates.
//!
//! A release feed is a JSON document in the format of GitHub's releases API: an array of
//! releases (`/repos/{owner}/{repo}/releases`) or a single release (`.../releases/latest`).
//! Each release has a `tag_name` such as `v1.2.3` or `v1.3.0-beta.1`, an `html_url` to its
//! download page, and `draft`/`prerelease` flags. A mirror set in `updates.url` serves the same
//! format.
//!
//! [`check`] never touches the network when `updates.mode` is `off`. Nothing here checks on a
//! schedule: the application calls [`check`] only when the user asks (Help > Check for Updates).

use std::sync::Arc;
use std::time::Duration;

use birchpad_config::{UpdateChannel, UpdateMode, UpdateSettings};
use semver::Version;
use serde::Deserialize;

/// The official release feed.
pub const DEFAULT_FEED: &str =
    "https://api.github.com/repos/SecurityTrip/Birchpad/releases?per_page=30";

/// Feeds larger than this are rejected.
const MAX_FEED_SIZE: u64 = 8 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(20);

/// A published release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub version: Version,
    pub tag: String,
    /// The release's download page (http or https).
    pub page: String,
}

/// The result of a successful check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Nothing newer on the channel; `newest` is the newest release on it, if any.
    UpToDate { newest: Option<Version> },
    /// A newer release on the channel.
    Available(Release),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CheckError {
    #[error("checking for updates is turned off (updates.mode = \"off\")")]
    Disabled,
    #[error("cannot download {url}: {message}")]
    Transport { url: String, message: String },
    #[error("{url} is not a release feed: {message}")]
    Feed { url: String, message: String },
}

/// Fetches a URL. The application uses [`HttpTransport`]; tests use fakes.
pub trait Transport: Send + Sync {
    fn get(&self, url: &str) -> Result<Vec<u8>, String>;
}

/// The feed to read: `updates.url` if set, the official feed otherwise.
pub fn feed_url(settings: &UpdateSettings) -> &str {
    settings
        .url
        .as_deref()
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .unwrap_or(DEFAULT_FEED)
}

/// Looks for a release newer than `current` on the configured channel.
pub fn check(
    settings: &UpdateSettings,
    current: &Version,
    transport: &dyn Transport,
) -> Result<Outcome, CheckError> {
    if settings.mode == UpdateMode::Off {
        return Err(CheckError::Disabled);
    }
    let url = feed_url(settings);
    let body = transport
        .get(url)
        .map_err(|message| CheckError::Transport {
            url: url.to_owned(),
            message,
        })?;
    let releases = parse_feed(&body).map_err(|error| CheckError::Feed {
        url: url.to_owned(),
        message: error.to_string(),
    })?;
    Ok(match newest(&releases, settings.channel) {
        Some(release) if release.version > *current => Outcome::Available(release.clone()),
        newest => Outcome::UpToDate {
            newest: newest.map(|release| release.version.clone()),
        },
    })
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Feed {
    Many(Vec<FeedRelease>),
    One(FeedRelease),
}

#[derive(Deserialize)]
struct FeedRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

/// Parses a feed. Drafts, tags that are not versions and pages that are not web pages are
/// skipped. A release marked as a prerelease without a pre-release version (`v1.2.0`, flagged)
/// is treated as a beta.
pub fn parse_feed(json: &[u8]) -> Result<Vec<Release>, serde_json::Error> {
    let releases = match serde_json::from_slice(json)? {
        Feed::Many(releases) => releases,
        Feed::One(release) => vec![release],
    };
    Ok(releases
        .into_iter()
        .filter(|release| !release.draft)
        .filter(|release| {
            let page = release.html_url.to_ascii_lowercase();
            page.starts_with("https://") || page.starts_with("http://")
        })
        .filter_map(|release| {
            let tag = release.tag_name.trim();
            let mut version = Version::parse(tag.strip_prefix(['v', 'V']).unwrap_or(tag)).ok()?;
            if release.prerelease && version.pre.is_empty() {
                version.pre = semver::Prerelease::new("beta").ok()?;
            }
            Some(Release {
                version,
                tag: release.tag_name,
                page: release.html_url,
            })
        })
        .collect())
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

/// The newest release on `channel`.
pub fn newest(releases: &[Release], channel: UpdateChannel) -> Option<&Release> {
    releases
        .iter()
        .filter(|release| on_channel(&release.version, channel))
        .max_by(|a, b| a.version.cmp(&b.version))
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
            .timeout_global(Some(TIMEOUT))
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
            .header("Accept", "application/vnd.github+json, application/json")
            .call()
            .map_err(|error| error.to_string())?;
        response
            .body_mut()
            .with_config()
            .limit(MAX_FEED_SIZE)
            .read_to_vec()
            .map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::sync::Mutex;

    use super::*;

    const FEED: &str = r#"[
        {"tag_name": "v0.3.0-nightly.20261001", "html_url": "https://example.com/n", "prerelease": true},
        {"tag_name": "v0.2.1-beta.2", "html_url": "https://example.com/b", "prerelease": true},
        {"tag_name": "v0.4.0", "html_url": "https://example.com/draft", "draft": true},
        {"tag_name": "v0.2.0", "html_url": "https://example.com/s", "prerelease": false},
        {"tag_name": "latest", "html_url": "https://example.com/x"},
        {"tag_name": "v9.0.0", "html_url": "javascript:alert(1)"},
        {"tag_name": "v0.1.0", "html_url": "https://example.com/old"}
    ]"#;

    /// Serves canned answers and records what was asked.
    struct Fake {
        answer: Result<Vec<u8>, String>,
        requests: Mutex<Vec<String>>,
    }

    impl Fake {
        fn new(answer: Result<&str, &str>) -> Self {
            Self {
                answer: answer
                    .map(|body| body.as_bytes().to_vec())
                    .map_err(str::to_owned),
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

    #[test]
    fn off_means_no_request_at_all() {
        let fake = Fake::new(Ok(FEED));
        let off = settings(UpdateMode::Off, UpdateChannel::Nightly);
        assert_eq!(
            check(&off, &version("0.1.0"), &fake),
            Err(CheckError::Disabled)
        );
        assert!(fake.requests.lock().unwrap().is_empty());
    }

    #[test]
    fn channels_pick_their_newest_release() {
        let releases = parse_feed(FEED.as_bytes()).unwrap();
        let tags: Vec<&str> = releases.iter().map(|r| r.tag.as_str()).collect();
        assert_eq!(
            tags,
            [
                "v0.3.0-nightly.20261001",
                "v0.2.1-beta.2",
                "v0.2.0",
                "v0.1.0"
            ]
        );
        let newest_tag = |channel| newest(&releases, channel).map(|r| r.tag.as_str());
        assert_eq!(newest_tag(UpdateChannel::Stable), Some("v0.2.0"));
        assert_eq!(newest_tag(UpdateChannel::Beta), Some("v0.2.1-beta.2"));
        assert_eq!(
            newest_tag(UpdateChannel::Nightly),
            Some("v0.3.0-nightly.20261001")
        );
    }

    #[test]
    fn compares_with_the_running_version() {
        let fake = Fake::new(Ok(FEED));
        let stable = settings(UpdateMode::Notify, UpdateChannel::Stable);
        let Ok(Outcome::Available(release)) = check(&stable, &version("0.1.0"), &fake) else {
            panic!("0.2.0 is newer than 0.1.0");
        };
        assert_eq!(release.page, "https://example.com/s");
        assert_eq!(
            check(&stable, &version("0.2.0"), &fake),
            Ok(Outcome::UpToDate {
                newest: Some(version("0.2.0"))
            })
        );
        // A beta of the next version is newer than every stable release.
        assert!(matches!(
            check(&stable, &version("0.2.1-beta.1"), &fake),
            Ok(Outcome::UpToDate { .. })
        ));
        let beta = settings(UpdateMode::Auto, UpdateChannel::Beta);
        assert!(matches!(
            check(&beta, &version("0.2.1-beta.1"), &fake),
            Ok(Outcome::Available(release)) if release.tag == "v0.2.1-beta.2"
        ));
        assert_eq!(*fake.requests.lock().unwrap(), [DEFAULT_FEED; 4]);
    }

    #[test]
    fn single_release_feeds_and_mirrors() {
        let fake = Fake::new(Ok(
            r#"{"tag_name": "1.0.0", "html_url": "http://mirror.local/1.0.0", "prerelease": true}"#,
        ));
        let mut mirror = settings(UpdateMode::Notify, UpdateChannel::Beta);
        mirror.url = Some(" http://mirror.local/feed.json ".into());
        let outcome = check(&mirror, &version("0.9.0"), &fake).unwrap();
        // Flagged as a prerelease without a pre-release version: a beta.
        assert!(
            matches!(outcome, Outcome::Available(release) if release.version == version("1.0.0-beta"))
        );
        assert_eq!(
            *fake.requests.lock().unwrap(),
            ["http://mirror.local/feed.json"]
        );
        mirror.channel = UpdateChannel::Stable;
        assert_eq!(
            check(&mirror, &version("0.9.0"), &fake),
            Ok(Outcome::UpToDate { newest: None })
        );
    }

    #[test]
    fn errors_name_the_feed() {
        let on = settings(UpdateMode::Auto, UpdateChannel::Stable);
        let error = check(&on, &version("0.1.0"), &Fake::new(Err("offline"))).unwrap_err();
        assert_eq!(
            error.to_string(),
            format!("cannot download {DEFAULT_FEED}: offline")
        );
        let error = check(&on, &version("0.1.0"), &Fake::new(Ok("<html>"))).unwrap_err();
        assert!(matches!(error, CheckError::Feed { .. }));
    }

    #[test]
    fn http_transport_reads_a_feed() {
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
            let body = FEED.as_bytes();
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            stream.write_all(body).unwrap();
            request
        });
        let transport = HttpTransport::build("Birchpad/0.1.0 (test)", None);
        let body = transport.get(&format!("http://{address}/feed")).unwrap();
        assert_eq!(parse_feed(&body).unwrap().len(), 4);
        let request = server.join().unwrap();
        assert_eq!(request[0], "GET /feed HTTP/1.1");
        assert!(
            request
                .iter()
                .any(|line| line.eq_ignore_ascii_case("user-agent: Birchpad/0.1.0 (test)"))
        );
    }
}
