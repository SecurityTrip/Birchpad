//! Files as `file:` URIs (RFC 8089) and back: the path's bytes percent-encoded but for the
//! characters a path may keep, Windows drive letters as `file:///C:/...`, UNC paths as
//! `file://server/share/...`.

use std::path::{Path, PathBuf};

use lsp_types::Uri;

/// Bytes a path keeps in its URI as they are: RFC 3986's unreserved characters, the path
/// separator, and `:` after a drive letter (encoded elsewhere, as servers expect).
fn keeps(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'/')
}

fn encode(path: &str, out: &mut String) {
    for byte in path.bytes() {
        if keeps(byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
}

/// The URI of the absolute path `path`; `None` for a relative one.
pub fn file_uri(path: &Path) -> Option<Uri> {
    file_uri_text(path)?.parse().ok()
}

/// [`file_uri`] as text.
pub fn file_uri_text(path: &Path) -> Option<String> {
    let text = path.to_str()?;
    let mut uri = String::from("file://");
    if cfg!(windows) {
        let text = text
            .strip_prefix(r"\\?\")
            .unwrap_or(text)
            .replace('\\', "/");
        if let Some(unc) = text.strip_prefix("//") {
            let (server, rest) = unc.split_once('/').unwrap_or((unc, ""));
            if server.is_empty() {
                return None;
            }
            encode(server, &mut uri);
            uri.push('/');
            encode(rest, &mut uri);
            return Some(uri);
        }
        let bytes = text.as_bytes();
        if bytes.len() < 2 || !bytes[0].is_ascii_alphabetic() || bytes[1] != b':' {
            return None;
        }
        uri.push('/');
        uri.push(bytes[0] as char);
        uri.push(':');
        encode(&text[2..], &mut uri);
        Some(uri)
    } else {
        if !text.starts_with('/') {
            return None;
        }
        encode(text, &mut uri);
        Some(uri)
    }
}

/// The path of a `file:` URI; `None` for another scheme, or text that is not one.
pub fn file_path(uri: &Uri) -> Option<PathBuf> {
    file_path_text(uri.as_str())
}

/// [`file_path`] of a URI's text.
pub fn file_path_text(uri: &str) -> Option<PathBuf> {
    let rest = uri
        .get(..5)
        .filter(|scheme| scheme.eq_ignore_ascii_case("file:"))
        .map(|_| &uri[5..])?;
    let rest = rest.split(['?', '#']).next().unwrap_or_default();
    let (host, path) = match rest.strip_prefix("//") {
        Some(authority) => match authority.find('/') {
            Some(slash) => (&authority[..slash], &authority[slash..]),
            None => (authority, "/"),
        },
        None => ("", rest),
    };
    let host = decode(host)?;
    let path = decode(path)?;
    if !path.starts_with('/') {
        return None;
    }
    let local = host.is_empty() || host.eq_ignore_ascii_case("localhost");
    if cfg!(windows) {
        if !local {
            return Some(PathBuf::from(format!(
                r"\\{host}{}",
                path.replace('/', "\\")
            )));
        }
        let bytes = path.as_bytes();
        if bytes.len() >= 3 && bytes[1].is_ascii_alphabetic() && bytes[2] == b':' {
            let mut drive = path[1..].replace('/', "\\");
            if drive.len() == 2 {
                drive.push('\\');
            }
            return Some(PathBuf::from(drive));
        }
        None
    } else {
        local.then(|| PathBuf::from(path))
    }
}

/// `%XX` decoded; `None` for a broken escape or bytes that are not UTF-8.
fn decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = text.get(index + 1..index + 3)?;
            if !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return None;
            }
            out.push(u8::from_str_radix(hex, 16).ok()?);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).ok()
}
