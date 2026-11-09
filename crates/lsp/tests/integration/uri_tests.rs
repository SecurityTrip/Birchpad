use std::path::{Path, PathBuf};

use birchpad_lsp::uri::*;

#[cfg(windows)]
#[test]
fn windows_paths_and_their_uris() {
    let cases = [
        (r"C:\src\main.rs", "file:///C:/src/main.rs"),
        (
            r"C:\My Files\ё #1%.rs",
            "file:///C:/My%20Files/%D1%91%20%231%25.rs",
        ),
        (r"\\server\share\a.txt", "file://server/share/a.txt"),
        (r"\\?\D:\long\path.txt", "file:///D:/long/path.txt"),
    ];
    for (path, uri) in cases {
        assert_eq!(
            file_uri_text(Path::new(path)).as_deref(),
            Some(uri),
            "{path}"
        );
        let back = file_path_text(uri).unwrap();
        let expected = path.strip_prefix(r"\\?\").unwrap_or(path);
        assert_eq!(back, PathBuf::from(expected), "{uri}");
        assert!(file_uri(Path::new(path)).is_some());
    }
    // Servers that encode the drive's colon, use a lowercase drive, or name localhost.
    assert_eq!(
        file_path_text("file:///c%3A/src/a.rs"),
        Some(PathBuf::from(r"c:\src\a.rs"))
    );
    assert_eq!(
        file_path_text("FILE://localhost/C:/a.rs"),
        Some(PathBuf::from(r"C:\a.rs"))
    );
    // A drive alone is its root folder.
    assert_eq!(file_path_text("file:///C:"), Some(PathBuf::from(r"C:\")));
    assert_eq!(
        file_uri_text(Path::new(r"C:\")).as_deref(),
        Some("file:///C:/")
    );
    // Relative paths and paths without a drive have no URI.
    for relative in [r"src\main.rs", r"\no-drive", "", r"\\"] {
        assert_eq!(file_uri_text(Path::new(relative)), None, "{relative}");
    }
    // A path with no drive in a URI is no Windows path.
    assert_eq!(file_path_text("file:///src/main.rs"), None);
}

#[cfg(not(windows))]
#[test]
fn unix_paths_and_their_uris() {
    let cases = [
        ("/src/main.rs", "file:///src/main.rs"),
        (
            "/My Files/ё #1%.rs",
            "file:///My%20Files/%D1%91%20%231%25.rs",
        ),
        ("/", "file:///"),
    ];
    for (path, uri) in cases {
        assert_eq!(
            file_uri_text(Path::new(path)).as_deref(),
            Some(uri),
            "{path}"
        );
        assert_eq!(file_path_text(uri), Some(PathBuf::from(path)), "{uri}");
        assert!(file_uri(Path::new(path)).is_some());
    }
    assert_eq!(
        file_path_text("file://localhost/etc/hosts"),
        Some(PathBuf::from("/etc/hosts"))
    );
    // Another host's file is not one here.
    assert_eq!(file_path_text("file://server/share/a.txt"), None);
    for relative in ["src/main.rs", ""] {
        assert_eq!(file_uri_text(Path::new(relative)), None, "{relative}");
    }
}

#[test]
fn what_is_not_a_file_uri_has_no_path() {
    for uri in [
        "https://example.com/a.rs",
        "untitled:Untitled-1",
        "file",
        "",
        "file:relative/path",
        // Broken escapes, and escapes that are not UTF-8.
        "file:///a%2",
        "file:///a%zz",
        "file:///a%+1",
        "file:///%FF",
    ] {
        assert_eq!(file_path_text(uri), None, "{uri}");
    }
}

#[test]
fn queries_and_fragments_are_not_part_of_the_path() {
    let uri = if cfg!(windows) {
        "file:///C:/a.rs?x=1#L3"
    } else {
        "file:///a.rs?x=1#L3"
    };
    let expected = if cfg!(windows) { r"C:\a.rs" } else { "/a.rs" };
    assert_eq!(file_path_text(uri), Some(PathBuf::from(expected)));
    let parsed: birchpad_lsp::lsp_types::Uri = uri.parse().unwrap();
    assert_eq!(file_path(&parsed), Some(PathBuf::from(expected)));
}
