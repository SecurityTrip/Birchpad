//! Files and folders made, renamed, copied and moved, as a project tree does it: Folder as
//! Workspace's file operations, without a window.
//!
//! Nothing here overwrites: a name already taken is refused ([`EntryError::AlreadyExists`]),
//! except that a copy takes a free name of its own ([`free_name`]: `a copy.txt`,
//! `a copy 2.txt`, ...). A folder is never copied or moved into itself.

use std::io;
use std::path::{Path, PathBuf};

use birchpad_i18n::t;

/// Why a file operation was refused or failed.
#[derive(Debug, thiserror::Error)]
pub enum EntryError {
    #[error("{}", t!("io.the-name-is-empty"))]
    EmptyName,
    #[error("{}", t!("io.not-a-name", name = .0))]
    NotAName(String),
    #[error("{}", t!("io.already-exists", path = .0.display()))]
    AlreadyExists(PathBuf),
    #[error("{}", t!("io.into-itself", path = .0.display()))]
    IntoItself(PathBuf),
    #[error("{}", t!("io.cannot-change", path = .path.display(), reason = .source))]
    Io { path: PathBuf, source: io::Error },
}

impl EntryError {
    fn io(path: &Path) -> impl FnOnce(io::Error) -> Self + '_ {
        move |source| match source.kind() {
            io::ErrorKind::AlreadyExists => Self::AlreadyExists(path.to_owned()),
            _ => Self::Io {
                path: path.to_owned(),
                source,
            },
        }
    }
}

/// The characters Windows refuses in a name, besides the separators.
const WINDOWS_RESERVED: [char; 7] = ['<', '>', ':', '"', '|', '?', '*'];

/// Why `name` cannot name a file or a folder in a folder, if it cannot: it is empty or only
/// spaces, `.` or `..`, or holds a separator or a NUL (on Windows, also `<>:"|?*`).
pub fn check_name(name: &str) -> Result<(), EntryError> {
    if name.trim().is_empty() {
        return Err(EntryError::EmptyName);
    }
    let reserved = cfg!(windows) && name.contains(WINDOWS_RESERVED);
    if reserved || name.contains(['/', '\\', '\0']) || matches!(name, "." | "..") {
        return Err(EntryError::NotAName(name.to_owned()));
    }
    Ok(())
}

/// Makes the empty file `name` in `folder`; a name taken is refused.
pub fn create_file(folder: &Path, name: &str) -> Result<PathBuf, EntryError> {
    check_name(name)?;
    let path = folder.join(name);
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(EntryError::io(&path))?;
    Ok(path)
}

/// Makes the folder `name` in `folder`; a name taken is refused.
pub fn create_folder(folder: &Path, name: &str) -> Result<PathBuf, EntryError> {
    check_name(name)?;
    let path = folder.join(name);
    std::fs::create_dir(&path).map_err(EntryError::io(&path))?;
    Ok(path)
}

/// Whether `a` and `b` name the same entry on a system whose names ignore case (Windows and
/// macOS): the same path but for case.
fn same_but_case(a: &Path, b: &Path) -> bool {
    (cfg!(windows) || cfg!(target_os = "macos"))
        && a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
}

/// Gives the file or folder `path` the name `name` in its folder; the new path. A name another
/// entry has is refused; one that differs only in case renames it where names ignore case.
pub fn rename(path: &Path, name: &str) -> Result<PathBuf, EntryError> {
    check_name(name)?;
    let target = path.with_file_name(name);
    if target == path {
        return Ok(target);
    }
    std::fs::symlink_metadata(path).map_err(EntryError::io(path))?;
    if std::fs::symlink_metadata(&target).is_ok() && !same_but_case(path, &target) {
        return Err(EntryError::AlreadyExists(target));
    }
    std::fs::rename(path, &target).map_err(EntryError::io(path))?;
    Ok(target)
}

/// The path for `name` in `folder` that no entry has: `name` itself, or with ` copy`, then
/// ` copy 2`, ` copy 3`, ... before its extension (`a.txt` → `a copy.txt`; `.gitignore` →
/// `.gitignore copy`; a folder `src` → `src copy`).
pub fn free_name(folder: &Path, name: &str) -> PathBuf {
    let path = folder.join(name);
    if std::fs::symlink_metadata(&path).is_err() {
        return path;
    }
    let (stem, extension) = match name.rfind('.') {
        Some(dot) if dot > 0 => name.split_at(dot),
        _ => (name, ""),
    };
    (1u64..)
        .map(|n| match n {
            1 => folder.join(format!("{stem} copy{extension}")),
            n => folder.join(format!("{stem} copy {n}{extension}")),
        })
        .find(|path| std::fs::symlink_metadata(path).is_err())
        .expect("a free name")
}

/// Refuses to put the folder `source` in `folder` when `folder` is it or inside it.
fn check_not_into_itself(source: &Path, folder: &Path) -> Result<(), EntryError> {
    if source.is_dir() && folder.starts_with(source) {
        return Err(EntryError::IntoItself(source.to_owned()));
    }
    Ok(())
}

/// Copies the file or folder `source`, with everything in it, into `folder`, under its own
/// name or, if that is taken, a [`free_name`]; the copy's path. Links to folders inside a
/// copied folder are left out, so that a link cannot copy a folder into itself.
pub fn copy_into(source: &Path, folder: &Path) -> Result<PathBuf, EntryError> {
    let name = source
        .file_name()
        .ok_or_else(|| EntryError::NotAName(source.display().to_string()))?;
    std::fs::symlink_metadata(source).map_err(EntryError::io(source))?;
    check_not_into_itself(source, folder)?;
    let target = free_name(folder, &name.to_string_lossy());
    copy_entry(source, &target)?;
    Ok(target)
}

/// Copies `source` beside itself, as [`copy_into`] its own folder does.
pub fn duplicate(source: &Path) -> Result<PathBuf, EntryError> {
    let folder = source.parent().unwrap_or(Path::new(""));
    copy_into(source, folder)
}

fn copy_entry(source: &Path, target: &Path) -> Result<(), EntryError> {
    if !source.is_dir() {
        std::fs::copy(source, target).map_err(EntryError::io(source))?;
        return Ok(());
    }
    std::fs::create_dir(target).map_err(EntryError::io(target))?;
    for entry in std::fs::read_dir(source).map_err(EntryError::io(source))? {
        let entry = entry.map_err(EntryError::io(source))?;
        let kind = entry.file_type().map_err(EntryError::io(&entry.path()))?;
        if kind.is_symlink() && entry.path().is_dir() {
            continue;
        }
        copy_entry(&entry.path(), &target.join(entry.file_name()))?;
    }
    Ok(())
}

/// Moves the file or folder `source` into `folder`, under its own name; the new path. A name
/// taken there is refused; `source` already in `folder` stays where it is. Between drives,
/// it is copied and then removed.
pub fn move_into(source: &Path, folder: &Path) -> Result<PathBuf, EntryError> {
    let name = source
        .file_name()
        .ok_or_else(|| EntryError::NotAName(source.display().to_string()))?;
    std::fs::symlink_metadata(source).map_err(EntryError::io(source))?;
    if source.parent() == Some(folder) {
        return Ok(source.to_owned());
    }
    check_not_into_itself(source, folder)?;
    let target = folder.join(name);
    if std::fs::symlink_metadata(&target).is_ok() {
        return Err(EntryError::AlreadyExists(target));
    }
    match std::fs::rename(source, &target) {
        Ok(()) => Ok(target),
        Err(error) if error.kind() == io::ErrorKind::CrossesDevices => {
            copy_entry(source, &target)?;
            let removed = if source.is_dir() {
                std::fs::remove_dir_all(source)
            } else {
                std::fs::remove_file(source)
            };
            removed.map_err(EntryError::io(source))?;
            Ok(target)
        }
        Err(error) => Err(EntryError::io(source)(error)),
    }
}

/// Where `path` is once the file or folder `from` became `to`: `to` for `from` itself, the
/// same place inside `to` for what was inside `from`, `None` for anything else.
pub fn relocated(path: &Path, from: &Path, to: &Path) -> Option<PathBuf> {
    let rest = path.strip_prefix(from).ok()?;
    Some(if rest.as_os_str().is_empty() {
        to.to_owned()
    } else {
        to.join(rest)
    })
}
