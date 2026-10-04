//! Noticing that another program changed a file: what the file looked like when it was read
//! or written, a fingerprint of its start, and reading only what was appended (tail -f).

use std::fs::File;
use std::hash::{DefaultHasher, Hasher as _};
use std::io::{self, Read as _, Seek as _, SeekFrom};
use std::path::Path;
use std::time::SystemTime;

use birchpad_core::Encoding;

use crate::encoding::to_encoding_rs;
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
            let encoding = to_encoding_rs(encoding);
            if !encoding.is_single_byte() {
                return None;
            }
            let (text, had_errors) = encoding.decode_without_bom_handling(bytes);
            (!had_errors).then(|| (text.into_owned(), bytes.len()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appended_utf8_waits_for_whole_characters() {
        let bytes = "ab ж".as_bytes();
        assert_eq!(
            decode_appended(&bytes[..4], Encoding::Utf8),
            Some(("ab ".to_owned(), 3)),
            "half of ж"
        );
        assert_eq!(
            decode_appended(bytes, Encoding::Utf8),
            Some(("ab ж".to_owned(), 5))
        );
        assert_eq!(decode_appended(b"a\xffb", Encoding::Utf8), None);
    }

    #[test]
    fn appended_utf16_and_single_byte() {
        let mut bytes: Vec<u8> = "a😀".encode_utf16().flat_map(u16::to_le_bytes).collect();
        let whole = bytes.len();
        assert_eq!(
            decode_appended(&bytes, Encoding::Utf16Le),
            Some(("a😀".to_owned(), whole))
        );
        bytes.truncate(whole - 2);
        assert_eq!(
            decode_appended(&bytes, Encoding::Utf16Le),
            Some(("a".to_owned(), 2)),
            "the lead surrogate waits"
        );
        assert_eq!(
            decode_appended(&[0xC0, 0xE4], Encoding::Legacy("windows-1251")),
            Some(("Ад".to_owned(), 2))
        );
        assert_eq!(decode_appended(b"x", Encoding::Legacy("Shift_JIS")), None);
    }

    #[test]
    fn stamps_heads_and_tails() {
        let dir = std::env::temp_dir().join(format!("birchpad-disk-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("log.txt");
        assert_eq!(stamp(&path).unwrap(), None);
        std::fs::write(&path, "first\n").unwrap();
        let head = Head::of(b"first\n");
        assert_eq!(stamp(&path).unwrap().unwrap().len, 6);

        std::fs::write(&path, "first\nsecond\n").unwrap();
        assert!(head.matches(&path).unwrap(), "only appended to");
        assert_eq!(read_from(&path, 6).unwrap(), b"second\n");
        std::fs::write(&path, "other\nsecond\n").unwrap();
        assert!(!head.matches(&path).unwrap(), "rewritten");
        std::fs::write(&path, "fir").unwrap();
        assert!(!head.matches(&path).unwrap(), "truncated");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
