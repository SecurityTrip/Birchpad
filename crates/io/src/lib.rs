//! Text files for Birchpad: detecting and converting encodings, reading files, and saving them
//! without losing data.
//!
//! The guiding rule is that no operation silently changes bytes of a file. Decoding reports
//! every reason the text might not save back to the same bytes ([`DecodeProblem`]); encoding
//! refuses characters the target encoding lacks ([`Unencodable`]) instead of replacing them;
//! saving overwrites files in place after keeping a recovery copy ([`save`]).

mod decode;
mod detect;
mod encode;
mod encoding;
mod file;

pub use decode::{DecodeProblem, Decoded, bom_bytes, decode, sniff_bom};
pub use detect::{DetectedBy, Detection, detect};
pub use encode::{Unencodable, check_encodable, encode};
pub use encoding::{
    CHARACTER_SETS, display_name, encoding_name, from_code_page, parse_encoding, system_ansi,
};
pub use file::{
    FileInfo, LoadOptions, LoadedFile, MAX_FILE_SIZE, ReadError, Recovery, SaveError, decode_file,
    discard_recovery, load, pending_recoveries, read_file, save,
};
