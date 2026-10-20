//! Noticing that another program changed a file: what the file looked like when it was read
//! or written, a fingerprint of its start, and reading only what was appended (tail -f).

use std::fs::File;
use std::hash::{DefaultHasher, Hasher as _};
use std::io::{self, Read as _, Seek as _, SeekFrom};
use std::path::Path;
use std::time::SystemTime;

use birchpad_core::Encoding;

use crate::encoding::to_codec;
use crate::file::FileInfo;

/// What a file looked like when it was read or written: another size or time means another
/// program changed it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiskStamp {
    pub len: u64,
    pub modified: Option<SystemTime>,
}

impl FileInfo {
    pub fn stamp(&self) -> DiskStamp {
        DiskStamp {
            len: self.len,
            modified: self.modified,
        }
    }
}

/// The file's stamp now, `None` if it does not exist.
pub fn stamp(path: &Path) -> io::Result<Option<DiskStamp>> {
    match std::fs::metadata(path) {
        Ok(metadata) => Ok(Some(DiskStamp {
            len: metadata.len(),
            modified: metadata.modified().ok(),
        })),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// How many bytes at the start of a file its fingerprint covers.
pub const HEAD_LEN: usize = 4096;

/// A fingerprint of the start of a file: if it is the same, a file that grew was only
/// appended to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Head {
    /// How many bytes it covers: the first [`HEAD_LEN`], or the whole file if shorter.
    pub len: usize,
    pub hash: u64,
}

impl Head {
    pub fn of(bytes: &[u8]) -> Self {
        let bytes = &bytes[..bytes.len().min(HEAD_LEN)];
        let mut hasher = DefaultHasher::new();
        hasher.write(bytes);
        Self {
            len: bytes.len(),
            hash: hasher.finish(),
        }
    }

    /// Whether the file at `path` still starts with the bytes this fingerprint was taken of.
    pub fn matches(&self, path: &Path) -> io::Result<bool> {
        let mut bytes = Vec::with_capacity(self.len);
        File::open(path)?
            .take(self.len as u64)
            .read_to_end(&mut bytes)?;
        Ok(bytes.len() == self.len && Self::of(&bytes) == *self)
    }
}

/// The bytes of the file after `offset`.
pub fn read_from(path: &Path, offset: u64) -> io::Result<Vec<u8>> {
    let mut file = File::open(path)?;
    file.seek(SeekFrom::Start(offset))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// Decodes bytes appended to a file in `encoding`, up to the last complete character: returns
/// the text and how many bytes it took. `None` if the bytes are not valid, or the encoding
/// cannot be decoded piece by piece (legacy multi-byte encodings): the file is then read again.
pub fn decode_appended(bytes: &[u8], encoding: Encoding) -> Option<(String, usize)> {
    match encoding {
        Encoding::Utf8 => match std::str::from_utf8(bytes) {
            Ok(text) => Some((text.to_owned(), bytes.len())),
            // Cut in the middle of a character: the rest comes with the next bytes.
            Err(error) if error.error_len().is_none() => {
                let valid = error.valid_up_to();
                let text = std::str::from_utf8(&bytes[..valid]).ok()?;
                Some((text.to_owned(), valid))
            }
            Err(_) => None,
        },
        Encoding::Utf16Le | Encoding::Utf16Be => {
            let units: Vec<u16> = bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|&pair| {
                    if encoding == Encoding::Utf16Le {
                        u16::from_le_bytes(pair)
                    } else {
                        u16::from_be_bytes(pair)
                    }
                })
                .collect();
            // A lead surrogate at the end waits for its trail.
            let complete = match units.last() {
                Some(unit) if (0xD800..0xDC00).contains(unit) => units.len() - 1,
                _ => units.len(),
            };
            let text = char::decode_utf16(units[..complete].iter().copied())
                .collect::<Result<String, _>>()
                .ok()?;
            Some((text, complete * 2))
        }
        Encoding::Legacy(_) => {
            let encoding = to_codec(encoding);
            if !encoding.is_single_byte() {
                return None;
            }
            let (text, had_errors) = encoding.decode_without_bom_handling(bytes);
            (!had_errors).then(|| (text.into_owned(), bytes.len()))
        }
    }
}
