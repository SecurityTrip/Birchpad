//! Git for Birchpad without a window (ADR 0066), through the `git` program: [`Git`] runs it for
//! a file's text in HEAD and a line's blame ([`blame`]), and [`changes`] compares the lines of
//! two texts as the margin shows them.

#[macro_use]
extern crate birchpad_i18n;

pub mod blame;
pub mod changes;

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub use blame::Blame;
pub use changes::{GitChange, LineChanges, line_changes, next_change};

/// Why Git could not answer.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GitError {
    /// The program could not be started.
    #[error("{}", t!("git.cannot-run", program = .program, reason = .reason))]
    CannotRun { program: String, reason: String },
    /// It ran and failed: what it said.
    #[error("{}", t!("git.failed", reason = .0))]
    Failed(String),
    /// A path without a folder or a name.
    #[error("{}", t!("git.not-a-file", path = .0))]
    NotAFile(String),
}

/// The `git` program to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Git {
    pub program: String,
}

impl Default for Git {
    fn default() -> Self {
        Self::new("git")
    }
}

/// A file's folder and its name in it, as Git is asked about it.
fn split(path: &Path) -> Result<(PathBuf, String), GitError> {
    let not_a_file = || GitError::NotAFile(path.display().to_string());
    let folder = path
        .parent()
        .filter(|folder| !folder.as_os_str().is_empty())
        .ok_or_else(not_a_file)?;
    let name = path.file_name().ok_or_else(not_a_file)?;
    Ok((folder.to_owned(), name.to_string_lossy().into_owned()))
}

impl Git {
    pub fn new(program: impl Into<String>) -> Self {
        let program = program.into();
        let program = program.trim();
        Self {
            program: if program.is_empty() { "git" } else { program }.to_owned(),
        }
    }

    /// Runs Git in `folder` with `args`, `input` on its standard input: what it wrote, or what it
    /// said when it failed.
    pub fn run(
        &self,
        folder: &Path,
        args: &[&str],
        input: Option<&[u8]>,
    ) -> Result<Vec<u8>, GitError> {
        let mut command = Command::new(&self.program);
        command
            .arg("-C")
            .arg(folder)
            .args(args)
            .stdin(if input.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt as _;
            // CREATE_NO_WINDOW: no console flashes up for Git.
            command.creation_flags(0x0800_0000);
        }
        let cannot_run = |error: std::io::Error| GitError::CannotRun {
            program: self.program.clone(),
            reason: error.to_string(),
        };
        let mut child = command.spawn().map_err(cannot_run)?;
        if let Some(input) = input
            && let Some(mut stdin) = child.stdin.take()
        {
            // Written on a thread of its own, so that a large text cannot fill the pipe
            // while Git's output waits to be read.
            let input = input.to_vec();
            std::thread::spawn(move || {
                let _ = stdin.write_all(&input);
            });
        }
        let output = child.wait_with_output().map_err(cannot_run)?;
        if output.status.success() {
            Ok(output.stdout)
        } else {
            let said = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            Err(GitError::Failed(if said.is_empty() {
                output.status.to_string()
            } else {
                said
            }))
        }
    }

    /// The bytes of the file `path` in HEAD: `None` when it has none there (untracked, outside
    /// a repository, no commit yet), an error when Git cannot run.
    pub fn head_text(&self, path: &Path) -> Result<Option<Vec<u8>>, GitError> {
        let (folder, name) = split(path)?;
        match self.run(&folder, &["show", &format!("HEAD:./{name}")], None) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(GitError::Failed(_)) => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// The blame of line `line` (from 0) of the file `path` whose text is now `contents`.
    pub fn blame_line(&self, path: &Path, line: usize, contents: &[u8]) -> Result<Blame, GitError> {
        let (folder, name) = split(path)?;
        let range = format!("{0},{0}", line + 1);
        let output = self.run(
            &folder,
            &[
                "blame",
                "--porcelain",
                "-L",
                &range,
                "--contents",
                "-",
                "--",
                &name,
            ],
            Some(contents),
        )?;
        Blame::parse(&String::from_utf8_lossy(&output))
            .ok_or_else(|| GitError::Failed(t!("git.blame-unreadable")))
    }
}
