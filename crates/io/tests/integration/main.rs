//! The crate's tests through its public interface, one module per part of `src`, in one
//! test binary.

#[path = "changes/tests.rs"]
mod changes;

#[path = "codec/tests.rs"]
mod codec;

#[path = "decode/tests.rs"]
mod decode;

#[path = "detect/tests.rs"]
mod detect;

#[path = "disk/tests.rs"]
mod disk;

#[path = "encode/tests.rs"]
mod encode;

#[path = "encoding/tests.rs"]
mod encoding;

#[path = "file/tests.rs"]
mod file;

#[path = "folder/tests.rs"]
mod folder;

#[path = "search_files/tests.rs"]
mod search_files;
