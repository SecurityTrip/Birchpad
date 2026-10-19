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
    #[error("{}", birchpad_i18n::t!("config.cannot-read", path = .path.display(), reason = .source))]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{}", birchpad_i18n::t!("config.cannot-write", path = .path.display(), reason = .source))]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{}", birchpad_i18n::t!("config.not-a-workspace", path = .path.display(), reason = .reason))]
    Format { path: PathBuf, reason: String },
}

impl ProjectWorkspace {
    /// Every file of every project, in order, each once (Search > Find in Projects).
    pub fn files(&self) -> Vec<PathBuf> {
        fn walk(items: &[ProjectItem], out: &mut Vec<PathBuf>) {
            for item in items {
                match item {
                    ProjectItem::File(path) => {
                        if !out.contains(path) {
                            out.push(path.clone());
                        }
                    }
                    ProjectItem::Folder(folder) => walk(&folder.items, out),
                }
            }
        }
        let mut out = Vec::new();
        for project in &self.projects {
            walk(&project.items, &mut out);
        }
        out
    }

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

/// `path` relative to `base` if it is inside it, written with `\` as Notepad++ does; else, or
/// when it is `base` itself (an empty name reads as no file), as it is.
fn relative(path: &Path, base: &Path) -> String {
    match path.strip_prefix(base) {
        Ok(inside) if !base.as_os_str().is_empty() && !inside.as_os_str().is_empty() => inside
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
#[path = "../tests/unit/project/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/unit/project/files_tests.rs"]
mod files_tests;
