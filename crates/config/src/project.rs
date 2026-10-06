//! Notepad++'s project workspaces: the `.workspace` files of the Project Panels.
//!
//! A workspace holds projects; a project holds folders and files; folders hold folders and
//! files. The folders are the project's own grouping, not folders on disk. The format is
//! Notepad++'s, so that workspaces move between the two editors:
//!
//! ```xml
//! <NotepadPlus>
//!     <Project name="Editor">
//!         <Folder name="Sources">
//!             <File name="src\main.rs" />
//!         </Folder>
//!         <File name="README.md" />
//!     </Project>
//! </NotepadPlus>
//! ```
//!
//! File paths are relative to the workspace file's folder when the file is inside it, and
//! absolute otherwise. Notepad++ writes `\`; on reading, both separators are taken.

use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

/// A project, or a folder of one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectFolder {
    pub name: String,
    pub items: Vec<ProjectItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectItem {
    Folder(ProjectFolder),
    /// An absolute path.
    File(PathBuf),
}

/// The projects of a workspace.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProjectWorkspace {
    pub projects: Vec<ProjectFolder>,
}

#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error("cannot read {}: {source}", path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("cannot write {}: {source}", path.display())]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{} is not a Notepad++ workspace: {reason}", path.display())]
    Format { path: PathBuf, reason: String },
}

impl ProjectWorkspace {
    /// Reads a workspace written by Notepad++ or Birchpad. Relative file paths are taken
    /// relative to `base`, the workspace file's folder.
    pub fn parse(xml: &str, base: &Path) -> Result<Self, String> {
        let document = roxmltree::Document::parse(xml).map_err(|error| error.to_string())?;
        let root = document.root_element();
        if !root.has_tag_name("NotepadPlus") {
            return Err(format!(
                "<{}> where <NotepadPlus> was expected",
                root.tag_name().name()
            ));
        }
        let projects = root
            .children()
            .filter(|node| node.has_tag_name("Project"))
            .map(|node| folder(node, base))
            .collect();
        Ok(Self { projects })
    }

    pub fn load(path: &Path) -> Result<Self, ProjectError> {
        let xml = fs::read_to_string(path).map_err(|source| ProjectError::Read {
            path: path.to_owned(),
            source,
        })?;
        let base = path.parent().unwrap_or(Path::new(""));
        Self::parse(&xml, base).map_err(|reason| ProjectError::Format {
            path: path.to_owned(),
            reason,
        })
    }

    /// The workspace as Notepad++ writes it, with paths relative to `base` where they can be.
    pub fn to_xml(&self, base: &Path) -> String {
        let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" ?>\n<NotepadPlus>\n");
        for project in &self.projects {
            write_folder(&mut xml, "Project", project, base, 1);
        }
        xml.push_str("</NotepadPlus>\n");
        xml
    }

    pub fn save(&self, path: &Path) -> Result<(), ProjectError> {
        let base = path.parent().unwrap_or(Path::new(""));
        fs::write(path, self.to_xml(base)).map_err(|source| ProjectError::Write {
            path: path.to_owned(),
            source,
        })
    }
}

fn folder(node: roxmltree::Node, base: &Path) -> ProjectFolder {
    let items = node
        .children()
        .filter_map(|child| {
            if child.has_tag_name("Folder") {
                Some(ProjectItem::Folder(folder(child, base)))
            } else if child.has_tag_name("File") {
                let name = child.attribute("name").filter(|name| !name.is_empty())?;
                Some(ProjectItem::File(resolve(name, base)))
            } else {
                None
            }
        })
        .collect();
    ProjectFolder {
        name: node.attribute("name").unwrap_or_default().to_owned(),
        items,
    }
}

/// A file name of a workspace as a path: relative ones from `base`, either separator.
fn resolve(name: &str, base: &Path) -> PathBuf {
    let native = if cfg!(windows) {
        name.replace('/', "\\")
    } else {
        name.replace('\\', "/")
    };
    let path = PathBuf::from(native);
    let joined = if path.is_absolute() {
        path
    } else {
        base.join(path)
    };
    normalize(&joined)
}

/// Drops `.` and folds `..` into the folder before it, without touching the disk.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other),
        }
    }
    out
}

/// `path` relative to `base` if it is inside it, written with `\` as Notepad++ does; else as
/// it is.
fn relative(path: &Path, base: &Path) -> String {
    match path.strip_prefix(base) {
        Ok(inside) if !base.as_os_str().is_empty() => inside
            .components()
            .map(|component| component.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("\\"),
        _ => path.display().to_string(),
    }
}

fn write_folder(xml: &mut String, tag: &str, folder: &ProjectFolder, base: &Path, depth: usize) {
    let indent = "    ".repeat(depth);
    xml.push_str(&format!(
        "{indent}<{tag} name=\"{}\">\n",
        escape(&folder.name)
    ));
    for item in &folder.items {
        match item {
            ProjectItem::Folder(inner) => write_folder(xml, "Folder", inner, base, depth + 1),
            ProjectItem::File(path) => xml.push_str(&format!(
                "{indent}    <File name=\"{}\" />\n",
                escape(&relative(path, base))
            )),
        }
    }
    xml.push_str(&format!("{indent}</{tag}>\n"));
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            '\t' => out.push_str("&#9;"),
            ch => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> PathBuf {
        if cfg!(windows) {
            PathBuf::from(r"C:\work")
        } else {
            PathBuf::from("/work")
        }
    }

    fn file(path: &str) -> ProjectItem {
        ProjectItem::File(base().join(path))
    }

    #[test]
    fn reads_notepad_plus_plus_workspaces() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8" ?>
<NotepadPlus>
    <Project name="Editor">
        <Folder name="Sources">
            <File name="src\main.rs" />
            <Folder name="Empty" />
        </Folder>
        <File name="README.md" />
        <File name="" />
        <Unknown name="x" />
    </Project>
    <Project name="Second &amp; last" />
</NotepadPlus>"#;
        let workspace = ProjectWorkspace::parse(xml, &base()).unwrap();
        let src = if cfg!(windows) {
            r"src\main.rs"
        } else {
            "src/main.rs"
        };
        assert_eq!(
            workspace,
            ProjectWorkspace {
                projects: vec![
                    ProjectFolder {
                        name: "Editor".into(),
                        items: vec![
                            ProjectItem::Folder(ProjectFolder {
                                name: "Sources".into(),
                                items: vec![
                                    file(src),
                                    ProjectItem::Folder(ProjectFolder {
                                        name: "Empty".into(),
                                        items: vec![],
                                    }),
                                ],
                            }),
                            file("README.md"),
                        ],
                    },
                    ProjectFolder {
                        name: "Second & last".into(),
                        items: vec![],
                    },
                ],
            },
            "empty file names and unknown tags are left out"
        );
    }

    #[test]
    fn paths_resolve_with_either_separator_and_dot_dots() {
        let resolved = |name: &str| resolve(name, &base());
        assert_eq!(resolved("a/b.txt"), base().join("a").join("b.txt"));
        assert_eq!(resolved(r"a\b.txt"), base().join("a").join("b.txt"));
        assert_eq!(resolved(r"a\..\.\c.txt"), base().join("c.txt"));
        let outside = resolved(r"..\other\d.txt");
        assert_eq!(
            outside,
            base().parent().unwrap().join("other").join("d.txt")
        );
        let absolute = if cfg!(windows) { r"D:\x.txt" } else { "/x.txt" };
        assert_eq!(resolved(absolute), PathBuf::from(absolute));
    }

    #[test]
    fn writes_paths_inside_relative_and_round_trips() {
        let elsewhere = if cfg!(windows) {
            r"D:\other\x.txt"
        } else {
            "/other/x.txt"
        };
        let workspace = ProjectWorkspace {
            projects: vec![ProjectFolder {
                name: "<Tricky> \"name\" & 'quotes'\n".into(),
                items: vec![
                    file("a/b.txt"),
                    ProjectItem::File(PathBuf::from(elsewhere)),
                    ProjectItem::Folder(ProjectFolder {
                        name: "Group".into(),
                        items: vec![file("c.txt")],
                    }),
                ],
            }],
        };
        let xml = workspace.to_xml(&base());
        assert!(xml.contains(r#"<File name="a\b.txt" />"#), "{xml}");
        assert!(
            xml.contains(&format!("<File name=\"{elsewhere}\" />")),
            "{xml}"
        );
        assert!(xml.contains("&lt;Tricky&gt; &quot;name&quot; &amp; &apos;quotes&apos;&#10;"));
        assert_eq!(ProjectWorkspace::parse(&xml, &base()).unwrap(), workspace);
        assert_eq!(
            ProjectWorkspace::default().to_xml(&base()),
            "<?xml version=\"1.0\" encoding=\"UTF-8\" ?>\n<NotepadPlus>\n</NotepadPlus>\n"
        );
    }

    #[test]
    fn files_that_are_no_workspace_are_refused() {
        for (xml, reason) in [
            ("", "no XML"),
            ("<Session />", "another root"),
            ("<NotepadPlus><Project>", "unclosed"),
        ] {
            assert!(ProjectWorkspace::parse(xml, &base()).is_err(), "{reason}");
        }
        assert_eq!(
            ProjectWorkspace::parse("<NotepadPlus/>", &base()).unwrap(),
            ProjectWorkspace::default()
        );
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(
            ProjectWorkspace::load(&dir.path().join("missing.workspace")),
            Err(ProjectError::Read { .. })
        ));
        let broken = dir.path().join("broken.workspace");
        fs::write(&broken, "<Session/>").unwrap();
        let error = ProjectWorkspace::load(&broken).unwrap_err();
        assert!(
            error.to_string().contains("is not a Notepad++ workspace"),
            "{error}"
        );
        // Saving into a folder that does not exist fails without creating it.
        let nowhere = dir.path().join("no").join("x.workspace");
        assert!(matches!(
            ProjectWorkspace::default().save(&nowhere),
            Err(ProjectError::Write { .. })
        ));
    }

    #[test]
    fn saved_workspaces_load_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("p.workspace");
        let workspace = ProjectWorkspace {
            projects: vec![ProjectFolder {
                name: "P".into(),
                items: vec![ProjectItem::File(dir.path().join("sub").join("f.txt"))],
            }],
        };
        workspace.save(&path).unwrap();
        assert!(
            fs::read_to_string(&path)
                .unwrap()
                .contains(r#"name="sub\f.txt""#)
        );
        assert_eq!(ProjectWorkspace::load(&path).unwrap(), workspace);
    }
}
