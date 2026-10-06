//! Find in Files: Find All and Replace in the files of a folder, as Notepad++'s Find in Files
//! tab does.
//!
//! The folder is listed with Notepad++'s filters ([`birchpad_io::files_in`]) and searched in the
//! background, a batch of files at a time, so that the panel counts the files searched and Stop
//! takes effect between batches. The matches go to the search results panel.
//!
//! Files that are open are searched as they are in their tabs, unsaved changes included, and
//! Replace in Files changes them there, as one undo step, without saving them. Other files are
//! read with the encoding detection of File > Open and written back in their own encoding, byte
//! order mark and line endings, through the same safe save as File > Save. Binary files (a zero
//! byte in the first 64 KB of text) are not searched; files that would not save back unchanged,
//! and read-only files, are not changed.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use birchpad_config::FindInFilesState;
use birchpad_core::search::Searcher;
use birchpad_core::{ChangeSet, Edit, Encoding, Rope};
use birchpad_io::{Filters, FolderOptions, LoadOptions, LoadedFile};
use gpui_kit::{App, AppContext as _, AsyncApp, Context, Entity, PromptLevel, WeakEntity, Window};

use crate::app_state::AppState;
use crate::buffer::ReadOnly;
use crate::editor::EditorView;
use crate::find::{FindTab, plural};
use crate::search_results::{FileResults, LineHit, ResultLocation, SearchRun, line_hits};
use crate::workspace::Workspace;

/// Files searched between two progress reports and chances to stop.
const BATCH: usize = 32;
/// How much of a file's text is looked at for a zero byte.
const BINARY_SAMPLE: usize = 64 * 1024;

/// Where and how to search, from the tab's fields.
#[derive(Debug, Clone)]
struct Request {
    root: PathBuf,
    filters: Filters,
    options: FolderOptions,
}

/// Why a file was not searched or not changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Skipped {
    /// It could not be read or written.
    Failed,
    Binary,
    /// It would not save back unchanged: a decoding problem, or a replacement with characters
    /// its encoding lacks.
    NotExact,
    ReadOnly,
}

/// How many files were skipped, and why.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct SkipCounts {
    failed: usize,
    binary: usize,
    not_exact: usize,
    read_only: usize,
}

impl SkipCounts {
    fn add(&mut self, skipped: Skipped) {
        match skipped {
            Skipped::Failed => self.failed += 1,
            Skipped::Binary => self.binary += 1,
            Skipped::NotExact => self.not_exact += 1,
            Skipped::ReadOnly => self.read_only += 1,
        }
    }

    /// "; 2 binary files and 1 read-only file skipped", or nothing.
    fn describe(&self) -> String {
        let parts: Vec<String> = [
            (self.failed, "unreadable"),
            (self.binary, "binary"),
            (self.not_exact, "not exactly decodable"),
            (self.read_only, "read-only"),
        ]
        .into_iter()
        .filter(|(count, _)| *count > 0)
        .map(|(count, why)| plural(count, &format!("{why} file"), &format!("{why} files")))
        .collect();
        match parts.as_slice() {
            [] => String::new(),
            [one] => format!("; {one} skipped"),
            [rest @ .., last] => format!("; {} and {last} skipped", rest.join(", ")),
        }
    }
}

/// Whether `text` looks binary: a zero byte in its first [`BINARY_SAMPLE`] bytes.
fn is_binary(text: &Rope) -> bool {
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

/// Reads a file that is not open, as File > Open would.
fn read(path: &Path, ansi: Encoding) -> Result<LoadedFile, Skipped> {
    let options = LoadOptions {
        encoding: None,
        ansi,
    };
    let loaded =
        birchpad_io::load(path, options, &AtomicU64::new(0)).map_err(|_| Skipped::Failed)?;
    if is_binary(&loaded.text) {
        return Err(Skipped::Binary);
    }
    Ok(loaded)
}

/// The lines of `text` with matches, and how many matches; `None` without any.
fn search_text(text: &Rope, searcher: &Searcher) -> Option<(Vec<LineHit>, usize)> {
    let matches = searcher.find_all(text);
    (!matches.is_empty()).then(|| (line_hits(text, &matches), matches.len()))
}

/// Replace All in a file that is not open; the file is written only if something was
/// replaced. Returns the number of replacements.
fn replace_in_file(
    path: &Path,
    searcher: &Searcher,
    template: &str,
    ansi: Encoding,
    recovery: Option<&Path>,
) -> Result<usize, Skipped> {
    let loaded = read(path, ansi)?;
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
    let bytes = birchpad_io::encode(&text, loaded.format.encoding, loaded.format.bom)
        .map_err(|_| Skipped::NotExact)?;
    birchpad_io::save(path, &bytes, recovery).map_err(|_| Skipped::Failed)?;
    Ok(count)
}

/// Lists the files to search; reports a folder that cannot be listed and ends the run.
async fn list(
    this: &WeakEntity<Workspace>,
    request: &Request,
    cancel: &Arc<AtomicBool>,
    what: &'static str,
    cx: &mut AsyncApp,
) -> Option<Vec<PathBuf>> {
    let (listing, stop) = (request.clone(), cancel.clone());
    let listed = cx
        .background_spawn(async move {
            birchpad_io::files_in(&listing.root, &listing.filters, listing.options, &stop)
        })
        .await;
    match listed {
        Ok(files) => Some(files),
        Err(error) => {
            let root = request.root.display();
            let message = match error.kind() {
                io::ErrorKind::NotFound => format!("{what}: {root} does not exist"),
                io::ErrorKind::NotADirectory => format!("{what}: {root} is not a folder"),
                _ => format!("{what}: cannot read {root}: {error}"),
            };
            this.update(cx, |this, cx| {
                this.finish_files(cx);
                this.report(Some(message), true, cx);
            })
            .ok();
            None
        }
    }
}

impl Workspace {
    /// The folder, filters and options of the tab, remembered for the next run. Problems are
    /// reported in the panel.
    fn files_request(
        &mut self,
        what: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Request> {
        self.follow_current_document(window, cx);
        let bar = self.find_bar.read(cx);
        let (filters_text, directory, folder) = (
            bar.filters(cx),
            bar.directory(cx).trim().to_owned(),
            bar.folder,
        );
        if directory.is_empty() {
            self.report(Some(format!("{what}: choose a folder")), true, cx);
            return None;
        }
        let filters = match Filters::parse(&filters_text) {
            Ok(filters) => filters,
            Err(error) => {
                self.report(Some(format!("{what}: {error}")), true, cx);
                return None;
            }
        };
        let root = std::path::absolute(&directory).unwrap_or_else(|_| PathBuf::from(&directory));
        // Checked before anything is asked or started; listing reports what fails later.
        let problem = match std::fs::metadata(&root) {
            Ok(metadata) if metadata.is_dir() => None,
            Ok(_) => Some("is not a folder"),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Some("does not exist"),
            Err(_) => None,
        };
        if let Some(problem) = problem {
            let message = format!("{what}: {} {problem}", root.display());
            self.report(Some(message), true, cx);
            return None;
        }
        AppState::update_state(cx, |state, _| {
            state.find_in_files = FindInFilesState {
                filters: filters_text,
                directory: Some(root.clone()),
                subfolders: folder.subfolders,
                hidden: folder.hidden,
                follow_current_document: folder.follow_current_document,
            };
        });
        Some(Request {
            root,
            filters,
            options: FolderOptions {
                subfolders: folder.subfolders,
                hidden: folder.hidden,
            },
        })
    }

    /// The open documents with a file, by path, one view each.
    fn open_files(&self, cx: &App) -> HashMap<PathBuf, Entity<EditorView>> {
        self.document_views(cx)
            .into_iter()
            .filter_map(|view| {
                let buffer = view.read(cx).buffer.read(cx);
                if buffer.read_only() == Some(ReadOnly::Loading) {
                    return None;
                }
                Some((buffer.path()?.to_owned(), view.clone()))
            })
            .collect()
    }

    /// Marks Find in Files as running; the flag stops it.
    fn start_files(&mut self, cx: &mut Context<Self>) -> Arc<AtomicBool> {
        let cancel = Arc::new(AtomicBool::new(false));
        self.find_bar.update(cx, |bar, cx| {
            bar.files_running = Some(cancel.clone());
            cx.notify();
        });
        cancel
    }

    fn finish_files(&mut self, cx: &mut Context<Self>) {
        self.find_bar.update(cx, |bar, cx| {
            bar.files_running = None;
            cx.notify();
        });
    }

    fn files_running(&self, cx: &App) -> bool {
        self.find_bar.read(cx).files_running.is_some()
    }

    /// Follow current doc.: the Directory field becomes the folder of the active document. An
    /// empty field is filled in that way too.
    pub(crate) fn follow_current_document(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let bar = self.find_bar.read(cx);
        if bar.tab != FindTab::FindInFiles
            || !(bar.folder.follow_current_document || bar.directory(cx).trim().is_empty())
        {
            return;
        }
        let folder = self.active_view(cx).and_then(|view| {
            let buffer = view.read(cx).buffer.read(cx);
            buffer.path().and_then(Path::parent).map(Path::to_owned)
        });
        if let Some(folder) = folder {
            self.find_bar
                .update(cx, |bar, cx| bar.set_directory(&folder, window, cx));
        }
    }

    /// The "..." button: the folder dialog, starting from the Directory field.
    pub(crate) fn browse_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let typed = PathBuf::from(self.find_bar.read(cx).directory(cx).trim());
        let start = if typed.is_dir() {
            typed
        } else {
            self.default_directory(cx)
        };
        cx.spawn_in(window, async move |this, cx| {
            let Some(folder) = crate::path_dialog::ask_folder(start, cx).await else {
                return;
            };
            this.update_in(cx, |this, window, cx| {
                this.find_bar
                    .update(cx, |bar, cx| bar.set_directory(&folder, window, cx));
            })
            .ok();
        })
        .detach();
    }

    /// Find All in the files of the folder: their matching lines go to the search results.
    pub(crate) fn find_in_files(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        const WHAT: &str = "Find in Files";
        if self.files_running(cx) {
            return;
        }
        let Some((searcher, query)) = self.searcher(cx) else {
            return;
        };
        let Some(request) = self.files_request(WHAT, window, cx) else {
            return;
        };
        let open = self.open_files(cx);
        let texts: Arc<HashMap<PathBuf, Rope>> = Arc::new(
            open.iter()
                .map(|(path, view)| (path.clone(), view.read(cx).text(cx).clone()))
                .collect(),
        );
        let buffers: HashMap<PathBuf, _> = open
            .into_iter()
            .map(|(path, view)| (path, view.read(cx).buffer.downgrade()))
            .collect();
        let ansi = AppState::global(cx).ansi;
        let cancel = self.start_files(cx);
        self.report(Some(format!("{WHAT}: listing files...")), false, cx);
        cx.spawn(async move |this, cx| {
            let Some(files) = list(&this, &request, &cancel, WHAT, cx).await else {
                return;
            };
            let total = files.len();
            let (mut found, mut skipped, mut searched) = (Vec::new(), SkipCounts::default(), 0);
            for batch in files.chunks(BATCH) {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                let (batch, searcher, texts) = (batch.to_vec(), searcher.clone(), texts.clone());
                let results = cx
                    .background_spawn(async move {
                        batch
                            .into_iter()
                            .map(|path| {
                                let result = match texts.get(&path) {
                                    Some(text) => Ok(search_text(text, &searcher)),
                                    None => read(&path, ansi)
                                        .map(|loaded| search_text(&loaded.text, &searcher)),
                                };
                                (path, result)
                            })
                            .collect::<Vec<_>>()
                    })
                    .await;
                for (path, result) in results {
                    searched += 1;
                    match result {
                        Ok(Some((lines, hits))) => found.push((path, lines, hits)),
                        Ok(None) => {}
                        Err(why) => skipped.add(why),
                    }
                }
                let progress = format!("{WHAT}: {searched} of {total} files searched...");
                this.update(cx, |this, cx| this.report(Some(progress), false, cx))
                    .ok();
            }
            let stopped = cancel.load(Ordering::Relaxed);
            this.update(cx, |this, cx| {
                this.finish_files(cx);
                if this.report_failure(&searcher, cx) {
                    return;
                }
                let files: Vec<FileResults> = found
                    .into_iter()
                    .map(|(path, lines, hits)| {
                        let location = match buffers.get(&path) {
                            Some(buffer) => ResultLocation::Buffer(buffer.clone()),
                            None => ResultLocation::File(path.clone()),
                        };
                        FileResults::new(location, path.display().to_string(), lines, hits)
                    })
                    .collect();
                let run = SearchRun::new(&query.pattern, files, searched);
                let hits: usize = run.files.iter().map(|file| file.hits).sum();
                let message = format!(
                    "{WHAT}: {} in {} of {searched} searched{}{}",
                    plural(hits, "hit", "hits"),
                    plural(run.files.len(), "file", "files"),
                    skipped.describe(),
                    if stopped { " (stopped)" } else { "" }
                );
                this.search_results
                    .update(cx, |results, cx| results.add(run, cx));
                this.report(Some(message), hits == 0, cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Replace in Files, after asking: open documents are changed in their tabs, the other
    /// files on disk.
    pub(crate) fn replace_in_files(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        const WHAT: &str = "Replace in Files";
        if self.files_running(cx) {
            return;
        }
        let Some((searcher, _)) = self.searcher(cx) else {
            return;
        };
        let Some(request) = self.files_request(WHAT, window, cx) else {
            return;
        };
        let template = self.find_bar.read(cx).replacement(cx);
        let answer = window.prompt(
            PromptLevel::Warning,
            &format!(
                "Replace all occurrences in the files of {}?",
                request.root.display()
            ),
            Some(
                "Open documents are changed in their tabs, where Undo takes the change back. \
                 Other files are changed on disk.",
            ),
            &["Replace", "Cancel"],
            cx,
        );
        let ansi = AppState::global(cx).ansi;
        let recovery = AppState::global(cx).recovery_dir();
        cx.spawn(async move |this, cx| {
            if answer.await != Ok(0) {
                return;
            }
            let Ok(cancel) = this.update(cx, |this, cx| this.start_files(cx)) else {
                return;
            };
            let Some(files) = list(&this, &request, &cancel, WHAT, cx).await else {
                return;
            };
            let total = files.len();
            let (mut replaced, mut changed, mut done) = (0, 0, 0);
            let mut skipped = SkipCounts::default();
            let mut count = |result: Result<usize, Skipped>| match result {
                Ok(0) => {}
                Ok(count) => {
                    replaced += count;
                    changed += 1;
                }
                Err(why) => skipped.add(why),
            };

            // Open documents, as Replace All in All Opened Documents does.
            let open = this
                .update(cx, |this, cx| this.open_files(cx))
                .unwrap_or_default();
            let mut closed = Vec::new();
            for path in files {
                let Some(view) = open.get(&path) else {
                    closed.push(path);
                    continue;
                };
                done += 1;
                let result = this.update(cx, |this, cx| {
                    let range = 0..view.read(cx).text(cx).len();
                    match this.replace_all_edits(&searcher, &template, view, range, cx) {
                        None => Ok(0),
                        Some((transaction, replacements)) => {
                            if view.update(cx, |view, cx| view.apply_command_edit(transaction, cx))
                            {
                                Ok(replacements)
                            } else {
                                Err(Skipped::ReadOnly)
                            }
                        }
                    }
                });
                count(result.unwrap_or(Err(Skipped::Failed)));
            }

            for batch in closed.chunks(BATCH) {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                let (batch, searcher, template, recovery) = (
                    batch.to_vec(),
                    searcher.clone(),
                    template.clone(),
                    recovery.clone(),
                );
                let results = cx
                    .background_spawn(async move {
                        batch
                            .iter()
                            .map(|path| {
                                replace_in_file(
                                    path,
                                    &searcher,
                                    &template,
                                    ansi,
                                    recovery.as_deref(),
                                )
                            })
                            .collect::<Vec<_>>()
                    })
                    .await;
                done += results.len();
                results.into_iter().for_each(&mut count);
                let progress = format!("{WHAT}: {done} of {total} files done...");
                this.update(cx, |this, cx| this.report(Some(progress), false, cx))
                    .ok();
            }
            let stopped = cancel.load(Ordering::Relaxed);
            this.update(cx, |this, cx| {
                this.finish_files(cx);
                if this.report_failure(&searcher, cx) {
                    return;
                }
                let message = format!(
                    "{WHAT}: {} replaced in {}{}{}",
                    plural(replaced, "occurrence", "occurrences"),
                    plural(changed, "file", "files"),
                    skipped.describe(),
                    if stopped { " (stopped)" } else { "" }
                );
                this.report(Some(message), replaced == 0, cx);
            })
            .ok();
        })
        .detach();
    }
}

#[cfg(test)]
#[path = "find_in_files_tests.rs"]
mod tests;
