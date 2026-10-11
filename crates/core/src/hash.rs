//! Hashes of text or files, as Notepad++'s Tools > MD5 / SHA-1 / SHA-256 / SHA-512 show them:
//! lowercase hexadecimal.

use sha2::Digest as _;

/// A hash Tools can compute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HashKind {
    Md5,
    Sha1,
    Sha256,
    Sha512,
}

impl HashKind {
    pub const ALL: [Self; 4] = [Self::Md5, Self::Sha1, Self::Sha256, Self::Sha512];

    pub fn name(self) -> &'static str {
        match self {
            Self::Md5 => "MD5",
            Self::Sha1 => "SHA-1",
            Self::Sha256 => "SHA-256",
            Self::Sha512 => "SHA-512",
        }
    }
}

/// The hash of `bytes` in lowercase hexadecimal.
pub fn hash_hex(kind: HashKind, bytes: &[u8]) -> String {
    fn hex(digest: &[u8]) -> String {
        digest.iter().map(|byte| format!("{byte:02x}")).collect()
    }
    match kind {
        HashKind::Md5 => hex(&md5::Md5::digest(bytes)),
        HashKind::Sha1 => hex(&sha1::Sha1::digest(bytes)),
        HashKind::Sha256 => hex(&sha2::Sha256::digest(bytes)),
        HashKind::Sha512 => hex(&sha2::Sha512::digest(bytes)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_hashes() {
        assert_eq!(
            hash_hex(HashKind::Md5, b""),
            "d41d8cd98f00b204e9800998ecf8427e"
        );
        assert_eq!(
            hash_hex(HashKind::Md5, b"abc"),
            "900150983cd24fb0d6963f7d28e17f72"
        );
        assert_eq!(
            hash_hex(HashKind::Sha1, b"abc"),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
        assert_eq!(
            hash_hex(HashKind::Sha256, b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(hash_hex(HashKind::Sha512, b"abc").len(), 128);
        assert!(hash_hex(HashKind::Sha512, b"abc").starts_with("ddaf35a193617aba"));
        let names: Vec<_> = HashKind::ALL.iter().map(|kind| kind.name()).collect();
        assert_eq!(names, ["MD5", "SHA-1", "SHA-256", "SHA-512"]);
    }
}
