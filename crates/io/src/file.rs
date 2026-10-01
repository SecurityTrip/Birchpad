//! Reading and saving files. See ADR 0007 for the saving strategy.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use birchpad_core::{Encoding, Format, Rope};

use crate::decode::{DecodeProblem, decode};
use crate::detect::{DetectedBy, detect};

/// Files larger than this are refused instead of exhausting memory. The whole text lives in
/// memory; memory-mapped huge files are a phase 6 feature.
pub const MAX_FILE_SIZE: u64 = 2 * 1024 * 1024 * 1024;

const READ_CHUNK: usize = 4 * 1024 * 1024;

/// What the file system says about a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileInfo {
    pub len: u64,
    /// The read-only attribute (Windows) or no write permission bits (Unix).
    pub read_only: bool,
    pub modified: Option<SystemTime>,
}

impl FileInfo {
    fn from_metadata(metadata: &fs::Metadata) -> Self {
        Self {
            len: metadata.len(),
            read_only: metadata.permissions().readonly(),
            modified: metadata.modified().ok(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ReadError {
    #[error("{} does not exist", .0.display())]
    NotFound(PathBuf),
    #[error("{} is a folder, not a file", .0.display())]
    NotAFile(PathBuf),
    #[error(
        "{} is too large to open ({} MB; the limit is {} MB)",
        path.display(), size / 1024 / 1024, limit / 1024 / 1024
    )]
    TooLarge {
        path: PathBuf,
        size: u64,
        limit: u64,
    },
    #[error("cannot read {}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

/// Reads a whole file, refusing files over [`MAX_FILE_SIZE`]. `progress` receives the number of
/// bytes read so far.
pub fn read_file(path: &Path, progress: &AtomicU64) -> Result<(Vec<u8>, FileInfo), ReadError> {
    let io_error = |source: io::Error| match source.kind() {
        io::ErrorKind::NotFound => ReadError::NotFound(path.to_owned()),
        _ => ReadError::Io {
            path: path.to_owned(),
            source,
        },
    };
    let file = File::open(path).map_err(io_error)?;
    let metadata = file.metadata().map_err(io_error)?;
    if metadata.is_dir() {
        return Err(ReadError::NotAFile(path.to_owned()));
    }
    let too_large = |size| ReadError::TooLarge {
        path: path.to_owned(),
        size,
        limit: MAX_FILE_SIZE,
    };
    if metadata.len() > MAX_FILE_SIZE {
        return Err(too_large(metadata.len()));
    }

    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    // The file may grow while we read it; read at most one byte past the limit to notice.
    let mut reader = file.take(MAX_FILE_SIZE + 1);
    loop {
        let read = (&mut reader)
            .take(READ_CHUNK as u64)
            .read_to_end(&mut bytes)
            .map_err(io_error)?;
        progress.store(bytes.len() as u64, Ordering::Relaxed);
        if read == 0 {
            break;
        }
    }
    if bytes.len() as u64 > MAX_FILE_SIZE {
        return Err(too_large(bytes.len() as u64));
    }
    Ok((bytes, FileInfo::from_metadata(&metadata)))
}

/// How to interpret a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoadOptions {
    /// Decode as this encoding instead of detecting one ("Encode in ...").
    pub encoding: Option<Encoding>,
    /// The legacy encoding used when detection finds nothing better.
    pub ansi: Encoding,
}

/// A decoded file, ready to become a document.
#[derive(Debug, Clone)]
pub struct LoadedFile {
    pub text: Rope,
    /// Encoding and BOM of the file. The line-ending mode is a default: the document detects
    /// the actual one from the text.
    pub format: Format,
    pub detected_by: Option<DetectedBy>,
    /// Set if the text does not represent the file exactly; such files must not be edited.
    pub problem: Option<DecodeProblem>,
    pub info: FileInfo,
}

/// Reads and decodes a file.
pub fn load(
    path: &Path,
    options: LoadOptions,
    progress: &AtomicU64,
) -> Result<LoadedFile, ReadError> {
    let (bytes, info) = read_file(path, progress)?;
    Ok(decode_file(bytes, info, options))
}

/// Decodes bytes read from a file.
pub fn decode_file(bytes: Vec<u8>, info: FileInfo, options: LoadOptions) -> LoadedFile {
    let (encoding, detected_by) = match options.encoding {
        Some(encoding) => (encoding, None),
        None => {
            let detection = detect(&bytes, options.ansi);
            (detection.encoding, Some(detection.by))
        }
    };
    let decoded = decode(bytes, encoding);
    LoadedFile {
        text: Rope::from_str(&decoded.text),
        format: Format {
            encoding,
            bom: decoded.bom,
            ..Format::new()
        },
        detected_by,
        problem: decoded.problem,
        info,
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SaveError {
    #[error("{} is read-only", .0.display())]
    ReadOnly(PathBuf),
    #[error("{} is a folder, not a file", .0.display())]
    NotAFile(PathBuf),
    #[error("cannot save {}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    /// Writing failed after the file had been partly overwritten.
    #[error(
        "saving {} failed part way ({source}); the file may be damaged.{}",
        path.display(),
        recovery.as_ref().map(|r| format!(" A complete copy was kept in {}.", r.display())).unwrap_or_default()
    )]
    Interrupted {
        path: PathBuf,
        recovery: Option<PathBuf>,
        #[source]
        source: io::Error,
    },
}

/// Writes `bytes` to `path`, keeping the file's identity. See ADR 0007.
///
/// - Symbolic links are followed: the target is written, the link stays a link.
/// - An existing file is overwritten in place, so hard links, permissions, ACLs, owner,
///   extended attributes and alternate data streams stay as they are.
/// - Before an existing file is touched, the new content is written and flushed to a recovery
///   copy in `recovery_dir` (if given). It is removed once the save completed; after a crash it
///   is what the user gets back.
/// - Read-only files are refused rather than made writable.
pub fn save(path: &Path, bytes: &[u8], recovery_dir: Option<&Path>) -> Result<PathBuf, SaveError> {
    let target = resolve_links(path).map_err(|source| SaveError::Io {
        path: path.to_owned(),
        source,
    })?;
    let io_error = |source: io::Error| match source.kind() {
        io::ErrorKind::PermissionDenied => SaveError::ReadOnly(target.clone()),
        _ => SaveError::Io {
            path: target.clone(),
            source,
        },
    };

    let metadata = match fs::metadata(&target) {
        Ok(metadata) => Some(metadata),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(io_error(error)),
    };
    let Some(metadata) = metadata else {
        write_new(&target, bytes).map_err(io_error)?;
        return Ok(target);
    };
    if metadata.is_dir() {
        return Err(SaveError::NotAFile(target));
    }
    if metadata.permissions().readonly() {
        return Err(SaveError::ReadOnly(target));
    }

    let mut file = OpenOptions::new()
        .write(true)
        .open(&target)
        .map_err(io_error)?;
    let recovery = match recovery_dir {
        Some(dir) => Some(write_recovery(dir, &target, bytes).map_err(io_error)?),
        None => None,
    };

    let result = overwrite(&mut file, bytes, metadata.len());
    match result {
        Ok(()) => {
            if let Some(recovery) = &recovery {
                remove_recovery(recovery);
            }
            Ok(target)
        }
        Err(source) => Err(SaveError::Interrupted {
            path: target,
            recovery: recovery.map(|r| r.data),
            source,
        }),
    }
}

fn overwrite(file: &mut File, bytes: &[u8], old_len: u64) -> io::Result<()> {
    let new_len = bytes.len() as u64;
    // Grow first: on most file systems a full disk fails here, before any byte is overwritten.
    if new_len > old_len {
        file.set_len(new_len)?;
    }
    file.write_all(bytes)?;
    file.set_len(new_len)?;
    file.sync_all()
}

fn write_new(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    let written = file.write_all(bytes).and_then(|()| file.sync_all());
    if written.is_err() {
        // The file is ours and incomplete: do not leave a broken file behind.
        drop(file);
        let _ = fs::remove_file(path);
    }
    written
}

/// Follows symbolic links to the file that is actually written, even if it does not exist yet.
fn resolve_links(path: &Path) -> io::Result<PathBuf> {
    let mut current = path.to_owned();
    for _ in 0..40 {
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                let target = fs::read_link(&current)?;
                current = match current.parent() {
                    Some(parent) if target.is_relative() => parent.join(target),
                    _ => target,
                };
            }
            Ok(_) => return Ok(current),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(current),
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::other("too many levels of symbolic links"))
}

/// A recovery copy left by a save: the new content of `original`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recovery {
    pub original: PathBuf,
    pub data: PathBuf,
    path_file: PathBuf,
}

fn write_recovery(dir: &Path, target: &Path, bytes: &[u8]) -> io::Result<Recovery> {
    fs::create_dir_all(dir)?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let name = target
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let id = format!("{stamp}-{}-{name}", std::process::id());
    let data = dir.join(format!("{id}.data"));
    let path_file = dir.join(format!("{id}.path"));
    let mut file = File::create(&data)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    fs::write(&path_file, target.to_string_lossy().as_bytes())?;
    Ok(Recovery {
        original: target.to_owned(),
        data,
        path_file,
    })
}

fn remove_recovery(recovery: &Recovery) {
    let _ = fs::remove_file(&recovery.path_file);
    let _ = fs::remove_file(&recovery.data);
}

/// Recovery copies left by saves that did not complete (a crash or power loss while writing).
pub fn pending_recoveries(dir: &Path) -> Vec<Recovery> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<Recovery> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "path"))
        .filter_map(|path_file| {
            let original = PathBuf::from(fs::read_to_string(&path_file).ok()?);
            let data = path_file.with_extension("data");
            data.exists().then_some(Recovery {
                original,
                data,
                path_file,
            })
        })
        .collect();
    found.sort_by(|a, b| a.data.cmp(&b.data));
    found
}

/// Forgets a recovery copy the user has dealt with.
pub fn discard_recovery(recovery: &Recovery) {
    remove_recovery(recovery);
}
