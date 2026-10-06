//! The files of a folder that Find in Files searches, chosen by Notepad++'s filters.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// Notepad++'s Filters field: which files of a folder to search.
///
/// Patterns are separated by spaces or semicolons and use the wildcards `*` (any characters) and
/// `?` (one character); case does not matter.
///
/// - `*.rs *.toml`: the files whose names match one of them. No pattern at all, `*` and `*.*`
///   take every file (`*.*` also those without a dot, as on Windows).
/// - `!*.bak`: not the files that match.
/// - `!\target` (or `!/target`): not the folder `target` directly in the searched folder.
/// - `!+\target`: not a folder `target` anywhere below it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filters {
    include: Vec<Wildcard>,
    exclude: Vec<Wildcard>,
    /// Folders left out, and whether anywhere below (`!+\`) or only at the top (`!\`).
    exclude_folders: Vec<(Wildcard, bool)>,
}

/// An exclusion that names nothing: `!`, `!\`, `!+` or `!+\` alone.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("\"{0}\" leaves out nothing: write !*.bak for files, !\\folder or !+\\folder for folders")]
pub struct FilterError(pub String);

impl Filters {
    pub fn parse(text: &str) -> Result<Self, FilterError> {
        let mut filters = Self::default();
        for item in text.split([' ', ';']).filter(|item| !item.is_empty()) {
            let Some(excluded) = item.strip_prefix('!') else {
                filters.include.push(Wildcard::new(item));
                continue;
            };
            let (anywhere, excluded) = match excluded.strip_prefix('+') {
                Some(rest) => (true, rest),
                None => (false, excluded),
            };
            match excluded.strip_prefix(['\\', '/']) {
                Some(folder) if !folder.is_empty() => filters
                    .exclude_folders
                    .push((Wildcard::new(folder), anywhere)),
                None if !anywhere && !excluded.is_empty() => {
                    filters.exclude.push(Wildcard::new(excluded));
                }
                _ => return Err(FilterError(item.to_owned())),
            }
        }
        Ok(filters)
    }

    /// Whether a file named `name` is searched.
    pub fn takes_file(&self, name: &str) -> bool {
        (self.include.is_empty() || self.include.iter().any(|pattern| pattern.matches(name)))
            && !self.exclude.iter().any(|pattern| pattern.matches(name))
    }

    /// Whether a folder named `name` is left out; `top` if it is directly in the searched
    /// folder.
    pub fn leaves_out_folder(&self, name: &str, top: bool) -> bool {
        self.exclude_folders
            .iter()
            .any(|(pattern, anywhere)| (*anywhere || top) && pattern.matches(name))
    }
}

/// A file name pattern with `*` and `?`, compared in lowercase.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Wildcard(Vec<char>);

impl Wildcard {
    fn new(pattern: &str) -> Self {
        Self(pattern.chars().flat_map(char::to_lowercase).collect())
    }

    fn matches(&self, name: &str) -> bool {
        if self.0 == ['*', '.', '*'] {
            return true;
        }
        let name: Vec<char> = name.chars().flat_map(char::to_lowercase).collect();
        let pattern = &self.0;
        let (mut p, mut n) = (0, 0);
        // The last `*` seen, and the name position it currently stands for up to.
        let mut star: Option<(usize, usize)> = None;
        while n < name.len() {
            if p < pattern.len() && (pattern[p] == '?' || pattern[p] == name[n]) {
                p += 1;
                n += 1;
            } else if p < pattern.len() && pattern[p] == '*' {
                star = Some((p, n));
                p += 1;
            } else if let Some((star_at, until)) = star {
                // Let the star take one more character and try again after it.
                star = Some((star_at, until + 1));
                p = star_at + 1;
                n = until + 1;
            } else {
                return false;
            }
        }
        pattern[p..].iter().all(|&c| c == '*')
    }
}

/// The folder options of Find in Files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FolderOptions {
    /// In all sub-folders.
    pub subfolders: bool,
    /// In hidden folders: hidden files and folders too. Hidden means the hidden attribute on
    /// Windows and a name starting with a dot elsewhere.
    pub hidden: bool,
}

/// The files under `root` that `filters` take: each folder's files in name order, then its
/// subfolders. Links to folders are not followed, so a link back up cannot loop. Subfolders
/// that cannot be read are left out; a `root` that cannot be read is an error. Once `cancel`
/// is set, the files found so far are returned.
pub fn files_in(
    root: &Path,
    filters: &Filters,
    options: FolderOptions,
    cancel: &AtomicBool,
) -> io::Result<Vec<PathBuf>> {
    if !fs::metadata(root)?.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotADirectory,
            format!("{} is not a folder", root.display()),
        ));
    }
    let mut files = Vec::new();
    // Folders still to list, the next one last.
    let mut pending = vec![(root.to_owned(), true)];
    while let Some((folder, top)) = pending.pop() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let entries = match fs::read_dir(&folder) {
            Ok(entries) => entries,
            Err(error) if folder == root => return Err(error),
            Err(_) => continue,
        };
        let mut found = Vec::new();
        let mut subfolders = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let (Ok(kind), Ok(metadata)) = (entry.file_type(), entry.metadata()) else {
                continue;
            };
            if !options.hidden && is_hidden(&name, &metadata) {
                continue;
            }
            let path = entry.path();
            if kind.is_dir() {
                if options.subfolders && !filters.leaves_out_folder(&name, top) {
                    subfolders.push((name, path));
                }
            } else if (kind.is_file() || fs::metadata(&path).is_ok_and(|target| target.is_file()))
                && filters.takes_file(&name)
            {
                found.push((name, path));
            }
        }
        let by_name = |(a, _): &(String, PathBuf), (b, _): &(String, PathBuf)| {
            a.to_lowercase()
                .cmp(&b.to_lowercase())
                .then_with(|| a.cmp(b))
        };
        found.sort_by(by_name);
        subfolders.sort_by(by_name);
        files.extend(found.into_iter().map(|(_, path)| path));
        pending.extend(subfolders.into_iter().rev().map(|(_, path)| (path, false)));
    }
    Ok(files)
}

fn is_hidden(name: &str, metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt as _;
        const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
        let _ = name;
        metadata.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0
    }
    #[cfg(not(windows))]
    {
        let _ = metadata;
        name.starts_with('.')
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn takes(filters: &str, name: &str) -> bool {
        Filters::parse(filters).unwrap().takes_file(name)
    }

    #[test]
    fn wildcards_match_whole_names_without_regard_to_case() {
        assert!(takes("*.rs", "main.rs"));
        assert!(takes("*.rs", "MAIN.RS"));
        assert!(takes("журнал.*", "ЖУРНАЛ.txt"));
        assert!(takes("a?c", "abc"));
        assert!(takes("*a*b*", "xxaxxbxx"));
        assert!(takes("**.rs", "x.rs"), "two stars are one");
        // Not a part of the name only, and `?` is exactly one character.
        assert!(!takes("*.rs", "main.rsx"));
        assert!(!takes("main", "main.rs"));
        assert!(!takes("a?c", "ac"));
        assert!(!takes("a?c", "abbc"));
        assert!(!takes("*a*b", "xxaxxbx"));
    }

    #[test]
    fn every_file_without_patterns_and_with_the_catch_alls() {
        for filters in ["", "   ", ";", "*", "*.*"] {
            for name in ["Makefile", "main.rs", ".hidden", "a.b.c"] {
                assert!(takes(filters, name), "{filters:?} takes {name}");
            }
        }
        // At the edges of a name: the empty name, one character.
        assert!(takes("*", ""));
        assert!(!takes("?", ""));
        assert!(takes("?", "x"));
        assert!(!takes("x", ""));
    }

    #[test]
    fn exclusions_win_over_inclusions() {
        assert!(takes("*.rs;*.toml", "Cargo.toml"));
        assert!(takes("*.rs  *.toml", "lib.rs"), "several spaces");
        assert!(!takes("*.rs *.toml", "README.md"));
        assert!(!takes("*.rs !*_test.rs", "parse_test.rs"));
        assert!(takes("*.rs !*_test.rs", "parse.rs"));
        // An exclusion alone takes every other file.
        assert!(takes("!*.bak", "notes.txt"));
        assert!(!takes("!*.bak", "notes.BAK"));
        assert!(!takes("*.* !*", "anything"), "everything left out");
    }

    #[test]
    fn folders_are_left_out_at_the_top_or_anywhere() {
        let filters = Filters::parse(r"*.rs !\target !+/node_modules !+\.g?t").unwrap();
        assert!(filters.leaves_out_folder("target", true));
        assert!(
            !filters.leaves_out_folder("target", false),
            "only at the top"
        );
        assert!(filters.leaves_out_folder("TARGET", true));
        assert!(filters.leaves_out_folder("node_modules", true));
        assert!(filters.leaves_out_folder("node_modules", false));
        assert!(filters.leaves_out_folder(".git", false));
        assert!(!filters.leaves_out_folder("src", true));
        // Folder exclusions say nothing about files of that name.
        assert!(!filters.takes_file("target"));
        assert!(Filters::parse(r"!\target").unwrap().takes_file("target"));
    }

    #[test]
    fn exclusions_without_a_name_are_refused() {
        for bad in ["!", r"!\", "!/", "!+", r"!+\", "!+name", "*.rs !"] {
            let error = Filters::parse(bad).unwrap_err();
            assert!(
                error.to_string().contains("leaves out nothing"),
                "{bad}: {error}"
            );
        }
        // The shortest that are fine.
        assert!(Filters::parse("!a").is_ok());
        assert!(Filters::parse(r"!\a").is_ok());
        assert!(Filters::parse(r"!+\a").is_ok());
    }

    /// A tree of empty files and folders; paths ending with `/` are folders.
    fn tree(paths: &[&str]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for path in paths {
            let full = dir.path().join(path);
            if path.ends_with('/') {
                fs::create_dir_all(full).unwrap();
            } else {
                fs::create_dir_all(full.parent().unwrap()).unwrap();
                fs::write(full, "").unwrap();
            }
        }
        dir
    }

    fn list(root: &Path, filters: &str, subfolders: bool, hidden: bool) -> Vec<String> {
        let options = FolderOptions { subfolders, hidden };
        let filters = Filters::parse(filters).unwrap();
        files_in(root, &filters, options, &AtomicBool::new(false))
            .unwrap()
            .into_iter()
            .map(|path| {
                let relative = path.strip_prefix(root).unwrap();
                relative.to_string_lossy().replace('\\', "/")
            })
            .collect()
    }

    #[test]
    fn files_come_before_subfolders_each_in_name_order() {
        let dir = tree(&[
            "b.txt",
            "A.txt",
            "z/inner.txt",
            "a/deeper/last.txt",
            "a/first.txt",
            "c.rs",
            "empty/",
        ]);
        assert_eq!(
            list(dir.path(), "", true, false),
            [
                "A.txt",
                "b.txt",
                "c.rs",
                "a/first.txt",
                "a/deeper/last.txt",
                "z/inner.txt"
            ]
        );
        assert_eq!(
            list(dir.path(), "*.txt", false, false),
            ["A.txt", "b.txt"],
            "without subfolders"
        );
        assert_eq!(list(dir.path(), "*.md", true, false), Vec::<String>::new());
    }

    #[test]
    fn left_out_folders_are_not_entered() {
        let dir = tree(&[
            "target/a.rs",
            "src/target/b.rs",
            "src/node_modules/c.rs",
            "node_modules/d.rs",
            "e.rs",
        ]);
        assert_eq!(
            list(dir.path(), r"*.rs !\target !+\node_modules", true, false),
            ["e.rs", "src/target/b.rs"]
        );
    }

    #[test]
    fn hidden_files_and_folders_only_when_asked() {
        let dir = tree(&[".hidden/a.txt", ".secret.txt", "shown.txt"]);
        #[cfg(windows)]
        for hidden in [".hidden", ".secret.txt"] {
            let status = std::process::Command::new("attrib")
                .arg("+h")
                .arg(dir.path().join(hidden))
                .status()
                .unwrap();
            assert!(status.success());
        }
        assert_eq!(list(dir.path(), "", true, false), ["shown.txt"]);
        assert_eq!(
            list(dir.path(), "", true, true),
            [".secret.txt", "shown.txt", ".hidden/a.txt"]
        );
    }

    #[test]
    fn a_root_that_is_no_folder_is_an_error() {
        let dir = tree(&["file.txt", "empty/"]);
        let options = FolderOptions {
            subfolders: true,
            hidden: false,
        };
        let filters = Filters::default();
        let never = AtomicBool::new(false);
        let missing = files_in(&dir.path().join("missing"), &filters, options, &never);
        assert_eq!(missing.unwrap_err().kind(), io::ErrorKind::NotFound);
        let file = files_in(&dir.path().join("file.txt"), &filters, options, &never);
        assert_eq!(file.unwrap_err().kind(), io::ErrorKind::NotADirectory);
        // An empty folder is no error, only no files.
        assert!(
            files_in(&dir.path().join("empty"), &filters, options, &never)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn cancelling_stops_the_listing() {
        let dir = tree(&["a.txt", "sub/b.txt"]);
        let options = FolderOptions {
            subfolders: true,
            hidden: false,
        };
        let cancelled = AtomicBool::new(true);
        let files = files_in(dir.path(), &Filters::default(), options, &cancelled).unwrap();
        assert!(files.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn links_to_folders_are_not_followed() {
        let dir = tree(&["sub/a.txt"]);
        std::os::unix::fs::symlink(dir.path(), dir.path().join("sub/loop")).unwrap();
        std::os::unix::fs::symlink(dir.path().join("sub/a.txt"), dir.path().join("link.txt"))
            .unwrap();
        assert_eq!(list(dir.path(), "", true, false), ["link.txt", "sub/a.txt"]);
    }
}
