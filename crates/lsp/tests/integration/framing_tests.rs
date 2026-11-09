use std::io::{BufReader, Cursor};

use birchpad_lsp::framing::*;

fn read_all(bytes: &[u8]) -> Vec<std::io::Result<Option<Vec<u8>>>> {
    let mut reader = BufReader::new(Cursor::new(bytes.to_vec()));
    let mut out = Vec::new();
    loop {
        let next = read_message(&mut reader);
        let stop = !matches!(next, Ok(Some(_)));
        out.push(next);
        if stop {
            return out;
        }
    }
}

#[test]
fn writes_and_reads_messages_back() {
    let mut bytes = Vec::new();
    write_message(&mut bytes, br#"{"a":1}"#).unwrap();
    write_message(&mut bytes, "{\"text\":\"ёж\"}".as_bytes()).unwrap();
    assert!(bytes.starts_with(b"Content-Length: 7\r\n\r\n{\"a\":1}"));
    let read = read_all(&bytes);
    assert_eq!(read.len(), 3);
    assert_eq!(
        read[0].as_ref().unwrap().as_deref(),
        Some(&br#"{"a":1}"#[..])
    );
    // The length is in bytes, not characters.
    assert_eq!(
        read[1].as_ref().unwrap().as_deref(),
        Some("{\"text\":\"ёж\"}".as_bytes())
    );
    assert!(matches!(read[2], Ok(None)), "the end between messages");
}

#[test]
fn reads_other_headers_and_any_case() {
    let bytes =
        b"content-length:2\r\nContent-Type: application/vscode-jsonrpc; charset=utf-8\r\n\r\n{}";
    let read = read_all(bytes);
    assert_eq!(read[0].as_ref().unwrap().as_deref(), Some(&b"{}"[..]));
    // Lines ending with LF alone are read too.
    let read = read_all(b"Content-Length: 2\n\n[]");
    assert_eq!(read[0].as_ref().unwrap().as_deref(), Some(&b"[]"[..]));
}

#[test]
fn an_empty_message_and_an_empty_stream() {
    let read = read_all(b"Content-Length: 0\r\n\r\n");
    assert_eq!(read[0].as_ref().unwrap().as_deref(), Some(&b""[..]));
    assert!(matches!(read_all(b"")[0], Ok(None)));
}

#[test]
fn refuses_broken_headers() {
    for broken in [
        &b"Content-Type: x\r\n\r\n{}"[..],
        b"Content-Length: two\r\n\r\n{}",
        b"Content-Length: -1\r\n\r\n",
        b"no colon here\r\n\r\n",
        b"Content-Length: 2\r\n",
        b"Content-Length: 2",
        b"Content-Length: 10\r\n\r\n{}",
        b"\xff\xfe: 1\r\n\r\n",
    ] {
        let read = read_all(broken);
        let error = read[0].as_ref().unwrap_err();
        assert!(
            matches!(
                error.kind(),
                std::io::ErrorKind::InvalidData | std::io::ErrorKind::UnexpectedEof
            ),
            "{:?}: {error}",
            String::from_utf8_lossy(broken)
        );
    }
}

#[test]
fn limits_the_size_of_messages_and_header_lines() {
    let at_limit = format!("Content-Length: {MAX_MESSAGE}\r\n\r\n");
    // At the limit the header is accepted (and the content then missing).
    let read = read_all(at_limit.as_bytes());
    assert_eq!(
        read[0].as_ref().unwrap_err().kind(),
        std::io::ErrorKind::UnexpectedEof
    );
    let past = format!("Content-Length: {}\r\n\r\n", MAX_MESSAGE + 1);
    let read = read_all(past.as_bytes());
    assert_eq!(
        read[0].as_ref().unwrap_err().kind(),
        std::io::ErrorKind::InvalidData
    );
    let long = format!(
        "X-Long: {}\r\nContent-Length: 2\r\n\r\n{{}}",
        "x".repeat(1024)
    );
    let read = read_all(long.as_bytes());
    assert_eq!(
        read[0].as_ref().unwrap_err().kind(),
        std::io::ErrorKind::InvalidData
    );
    // A header line of exactly the limit, its line break included, is read.
    let fits = format!(
        "X: {}\r\nContent-Length: 2\r\n\r\n{{}}",
        "x".repeat(1024 - 5)
    );
    let read = read_all(fits.as_bytes());
    assert_eq!(read[0].as_ref().unwrap().as_deref(), Some(&b"{}"[..]));
}
