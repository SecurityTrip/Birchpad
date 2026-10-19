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
