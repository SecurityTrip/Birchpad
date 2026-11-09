//! Birchpad's language ids and the protocol's, and the folder a server works in.

use std::path::{Path, PathBuf};

/// The protocol's `languageId` of Birchpad's language `id`: the ids the protocol lists where
/// they differ from Birchpad's, the id itself otherwise.
pub fn protocol_language_id(id: &str) -> &str {
    match id {
        "bash" => "shellscript",
        "batch" => "bat",
        "cs" => "csharp",
        "objc" => "objective-c",
        "tsx" => "typescriptreact",
        other => other,
    }
}

/// The root folder of the file `path`: the nearest folder above it with `.git` in it (a folder,
/// or the file of a worktree), or else the file's own folder. `None` for a path without one.
pub fn root_folder(path: &Path) -> Option<PathBuf> {
    let folder = path
        .parent()
        .filter(|folder| !folder.as_os_str().is_empty())?;
    folder
        .ancestors()
        .find(|ancestor| ancestor.join(".git").exists())
        .unwrap_or(folder)
        .to_path_buf()
        .into()
}
