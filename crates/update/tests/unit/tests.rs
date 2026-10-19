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
    serve_status(200, body)
}

/// Answers one request with `status` and `body`; returns the request's lines.
fn serve_status(status: u16, body: Vec<u8>) -> (String, std::thread::JoinHandle<Vec<String>>) {
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
            "HTTP/1.1 {status} Status\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .unwrap();
        // The client may hang up early on a body that is too large.
        stream.write_all(&body).ok();
        request
    });
    (format!("http://{address}"), server)
}

#[test]
fn http_transport_reads_and_downloads() {
    let transport = HttpTransport::build("Birchpad/0.1.0 (test)", None, STALL);
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

    // Exactly as large as the limit, as a package is as large as its manifest says.
    let (base, server) = serve_once(vec![b'x'; 200_000]);
    transport
        .download(&format!("{base}/p"), &path, 200_000, &|_| {})
        .unwrap();
    server.join().unwrap();
    assert_eq!(std::fs::metadata(&path).unwrap().len(), 200_000);

    for limit in [1_000, 199_999] {
        let (base, server) = serve_once(vec![b'x'; 200_000]);
        assert!(
            transport
                .download(&format!("{base}/p"), &path, limit, &|_| {})
                .is_err(),
            "over the limit of {limit}"
        );
        server.join().ok();
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn manifests_up_to_the_limit_are_read_and_larger_ones_refused() {
    let transport = HttpTransport::build("Birchpad/0.1.0 (test)", None, STALL);
    let limit = MAX_MANIFEST_SIZE as usize;
    let (base, server) = serve_once(vec![b'{'; limit]);
    assert_eq!(transport.get(&format!("{base}/m")).unwrap().len(), limit);
    server.join().unwrap();
    let (base, server) = serve_once(vec![b'{'; limit + 1]);
    let error = transport.get(&format!("{base}/m")).unwrap_err();
    assert!(error.contains("larger than"), "{error}");
    server.join().ok();
    // Empty is a body too (the manifest check refuses it later).
    let (base, server) = serve_once(Vec::new());
    assert_eq!(transport.get(&format!("{base}/m")).unwrap(), b"");
    server.join().unwrap();
}

#[test]
fn http_errors_and_absent_servers_are_errors() {
    let transport = HttpTransport::new("Birchpad/0.1.0 (test)");
    let transport_without_proxy = HttpTransport::build("Birchpad/0.1.0 (test)", None, STALL);
    let (base, server) = serve_status(404, b"not here".to_vec());
    assert!(transport_without_proxy.get(&format!("{base}/m")).is_err());
    server.join().unwrap();
    // Nobody listens on a port that was just freed.
    let address = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap();
    assert!(
        transport_without_proxy
            .get(&format!("http://{address}/m"))
            .is_err()
    );
    drop(transport);
}

/// Answers one request with `size` bytes, sent in `chunks` pieces `pause` apart; after
/// `stop_after` pieces it holds the connection without sending more for `hold`.
fn serve_slowly(
    size: usize,
    chunks: usize,
    pause: Duration,
    stop_after: Option<(usize, Duration)>,
) -> (String, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        for line in BufReader::new(&stream).lines() {
            if line.unwrap().is_empty() {
                break;
            }
        }
        write!(
            stream,
            "HTTP/1.1 200 OK
Content-Length: {size}
Connection: close

"
        )
        .unwrap();
        let piece = size / chunks;
        for sent in 0..chunks {
            if let Some((after, hold)) = stop_after
                && sent == after
            {
                std::thread::sleep(hold);
                return;
            }
            let len = if sent + 1 == chunks {
                size - piece * sent
            } else {
                piece
            };
            if stream.write_all(&vec![b'x'; len]).is_err() {
                return;
            }
            stream.flush().ok();
            std::thread::sleep(pause);
        }
    });
    (format!("http://{address}"), server)
}

#[test]
fn a_slow_but_steady_download_completes_and_a_stalled_one_fails() {
    let stall = Duration::from_secs(1);
    let transport = HttpTransport::build("Birchpad/0.1.0 (test)", None, stall);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("package.nupkg");

    // 30 pieces 100 ms apart: three seconds, three times the stall limit, each read in time.
    let (base, server) = serve_slowly(20_000, 30, Duration::from_millis(100), None);
    let reports = Mutex::new(Vec::new());
    transport
        .download(&format!("{base}/p"), &path, 20_000, &|received| {
            reports.lock().unwrap().push(received);
        })
        .unwrap();
    server.join().unwrap();
    assert_eq!(std::fs::metadata(&path).unwrap().len(), 20_000);
    let reports = reports.into_inner().unwrap();
    assert!(reports.len() >= 10, "the progress moves: {reports:?}");
    assert!(reports.is_sorted());
    assert_eq!(reports.last(), Some(&20_000));

    // A piece just inside the limit still counts as arriving.
    let (base, server) = serve_slowly(4_000, 3, Duration::from_millis(800), None);
    transport
        .download(&format!("{base}/p"), &path, 4_000, &|_| {})
        .unwrap();
    server.join().unwrap();

    // Half sent, then nothing for longer than the limit: stalled, well before the server
    // gives up.
    let hold = Duration::from_secs(3);
    let (base, server) = serve_slowly(20_000, 20, Duration::ZERO, Some((10, hold)));
    let started = std::time::Instant::now();
    let error = transport
        .download(&format!("{base}/p"), &path, 20_000, &|_| {})
        .unwrap_err();
    assert!(started.elapsed() < hold, "{:?}", started.elapsed());
    assert_eq!(error, "the download stalled: nothing arrived in 1 s");
    // Not waited for: it only sleeps out its hold.
    drop(server);

    // The same for a manifest.
    let (base, server) = serve_slowly(1_000, 2, Duration::ZERO, Some((1, hold)));
    let error = transport.get(&format!("{base}/m")).unwrap_err();
    assert!(error.contains("stalled"), "{error}");
    drop(server);
}

/// A transport that answers with the same bytes, downloading the default way.
struct Fixed(Vec<u8>);

impl Transport for Fixed {
    fn get(&self, _url: &str) -> Result<Vec<u8>, String> {
        Ok(self.0.clone())
    }
}

#[test]
fn the_default_download_keeps_to_the_limit() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("p");
    let transport = Fixed(b"12345".to_vec());
    let received = Mutex::new(0);
    transport
        .download("u", &path, 5, &|bytes| *received.lock().unwrap() = bytes)
        .unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"12345");
    assert_eq!(*received.lock().unwrap(), 5);
    let error = transport.download("u", &path, 4, &|_| {}).unwrap_err();
    assert_eq!(error, "larger than 4 bytes");
    // A folder that does not exist cannot be written to.
    let nowhere = dir.path().join("missing").join("p");
    assert!(transport.download("u", &nowhere, 5, &|_| {}).is_err());
}
