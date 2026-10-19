//! Find in Files and Replace in Files, one file at a time: reading a file to search as File >
//! Open does, skipping binary files (a zero byte in the first 64 KB of text), the lines with
//! matches, and Replace All in a file that is not open, written back in its own encoding, byte
//! order mark and line endings through the same safe save as File > Save. Files that would not
//! save back unchanged, and read-only files, are not changed.

use std::path::Path;
use std::sync::atomic::AtomicU64;

use birchpad_core::search::{LineHit, Searcher, line_hits};
use birchpad_core::{ChangeSet, Edit, Encoding, Rope};

use crate::{LoadOptions, LoadedFile};

/// How much of a file's text is looked at for a zero byte.
pub const BINARY_SAMPLE: usize = 64 * 1024;

/// Why a file was not searched or not changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Skipped {
    /// It could not be read or written.
    Failed,
    Binary,
    /// It would not save back unchanged: a decoding problem, or a replacement with characters
    /// its encoding lacks.
    NotExact,
    ReadOnly,
}

/// Whether `text` looks binary: a zero byte in its first [`BINARY_SAMPLE`] bytes.
pub fn is_binary(text: &Rope) -> bool {
    let mut seen = 0;
    for chunk in text.chunks() {
        let part = &chunk.as_bytes()[..chunk.len().min(BINARY_SAMPLE - seen)];
        if part.contains(&0) {
            return true;
        }
        seen += part.len();
        if seen == BINARY_SAMPLE {
            break;
        }
    }
    false
}

/// Reads a file to search, as File > Open would; a binary file is skipped.
pub fn read_for_search(path: &Path, ansi: Encoding) -> Result<LoadedFile, Skipped> {
    let options = LoadOptions {
        encoding: None,
        ansi,
    };
    let loaded = crate::load(path, options, &AtomicU64::new(0)).map_err(|_| Skipped::Failed)?;
    if is_binary(&loaded.text) {
        return Err(Skipped::Binary);
    }
    Ok(loaded)
}

/// The lines of `text` with matches, and how many matches; `None` without any.
pub fn search_text(text: &Rope, searcher: &Searcher) -> Option<(Vec<LineHit>, usize)> {
    let matches = searcher.find_all(text);
    (!matches.is_empty()).then(|| (line_hits(text, &matches), matches.len()))
}

/// Replace All in a file that is not open; the file is written only if something was
/// replaced. Returns the number of replacements.
pub fn replace_in_file(
    path: &Path,
    searcher: &Searcher,
    template: &str,
    ansi: Encoding,
    recovery: Option<&Path>,
) -> Result<usize, Skipped> {
    let loaded = read_for_search(path, ansi)?;
    let replacements = searcher.replacements(&loaded.text, 0..loaded.text.len(), template);
    if replacements.is_empty() {
        return Ok(0);
    }
    if loaded.problem.is_some() {
        return Err(Skipped::NotExact);
    }
    if loaded.info.read_only {
        return Err(Skipped::ReadOnly);
    }
    let count = replacements.len();
    let mut text = loaded.text;
    let edits = replacements
        .into_iter()
        .map(|(range, replacement)| Edit::replace(range, replacement));
    ChangeSet::from_edits(&text, edits)
        .expect("matches do not overlap")
        .apply(&mut text);
    let bytes = crate::encode(&text, loaded.format.encoding, loaded.format.bom)
        .map_err(|_| Skipped::NotExact)?;
    crate::save(path, &bytes, recovery).map_err(|_| Skipped::Failed)?;
    Ok(count)
}

#[cfg(test)]
#[path = "../tests/unit/search_files/tests.rs"]
mod tests;
