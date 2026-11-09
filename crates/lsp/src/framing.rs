//! The base protocol: each message is a header part (`Content-Length: <bytes>`, other fields
//! ignored, each line ending with CRLF), a blank line, and that many bytes of JSON.

use std::io::{self, BufRead, Read, Write};

/// The largest message read: a server that announces more is broken or hostile.
pub const MAX_MESSAGE: usize = 64 * 1024 * 1024;
/// The longest header line read.
const MAX_HEADER_LINE: usize = 1024;

/// Reads the next message's content; `None` at the end of the stream between messages.
pub fn read_message(reader: &mut impl BufRead) -> io::Result<Option<Vec<u8>>> {
    let mut length: Option<usize> = None;
    let mut first = true;
    loop {
        let mut line = Vec::new();
        let read = (&mut *reader)
            .take(MAX_HEADER_LINE as u64 + 1)
            .read_until(b'\n', &mut line)?;
        if read == 0 {
            if first {
                return Ok(None);
            }
            return Err(invalid("the stream ended inside a header"));
        }
        if line.len() > MAX_HEADER_LINE {
            return Err(invalid("a header line is too long"));
        }
        if !line.ends_with(b"\n") {
            return Err(invalid("the stream ended inside a header"));
        }
        first = false;
        let line = std::str::from_utf8(&line)
            .map_err(|_| invalid("a header is not text"))?
            .trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            break;
        }
        let (name, value) = line
            .split_once(':')
            .ok_or_else(|| invalid("a header line has no colon"))?;
        if name.trim().eq_ignore_ascii_case("content-length") {
            let value: usize = value
                .trim()
                .parse()
                .map_err(|_| invalid("Content-Length is not a number"))?;
            if value > MAX_MESSAGE {
                return Err(invalid("the message is too large"));
            }
            length = Some(value);
        }
    }
    let length = length.ok_or_else(|| invalid("a message has no Content-Length"))?;
    let mut content = vec![0; length];
    reader.read_exact(&mut content)?;
    Ok(Some(content))
}

/// Writes `content` as one message, and flushes it.
pub fn write_message(writer: &mut impl Write, content: &[u8]) -> io::Result<()> {
    write!(writer, "Content-Length: {}\r\n\r\n", content.len())?;
    writer.write_all(content)?;
    writer.flush()
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.to_owned())
}
