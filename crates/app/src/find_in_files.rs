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
use std::sync::atomic::{AtomicBool, Ordering};

use birchpad_config::FindInFilesState;
use birchpad_core::Rope;
use birchpad_core::search::{Query, Searcher};
use birchpad_io::search_files::{Skipped, read_for_search, replace_in_file, search_text};
use birchpad_io::{Filters, FolderOptions};
use gpui_kit::{App, AppContext as _, AsyncApp, Context, Entity, PromptLevel, WeakEntity, Window};

use crate::app_state::AppState;
use crate::buffer::ReadOnly;
use crate::editor::EditorView;
use crate::find::{FindTab, plural};
use crate::search_results::{FileResults, ResultLocation, SearchRun};
use crate::workspace::Workspace;

/// Files searched between two progress reports and chances to stop.
const BATCH: usize = 32;
/// Where and how to search, from the tab's fields.
#[derive(Debug, Clone)]
struct Request {
    root: PathBuf,
    filters: Filters,
    options: FolderOptions,
}

/// The files a search goes through.
enum FileSource {
    /// Those a folder holds, listed first.
    Listing(Request),
    /// These.
    Files(Vec<PathBuf>),
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
            [one] => format!("; {}", t!("find-in-files.skipped", files = one)),
            [rest @ .., last] => format!(
                "; {}",
                t!(
                    "find-in-files.skipped-several",
                    files = rest.join(", "),
                    last = last
                )
            ),
        }
    }
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

    pub(crate) fn files_running(&self, cx: &App) -> bool {
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

    /// Find in Files... from a folder (Folder as Workspace): the tab with that folder.
    pub(crate) fn find_in_folder(
        &mut self,
        folder: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dispatch(
            &birchpad_commands::Invocation::new("search.find-in-files"),
            window,
            cx,
        )
        .ok();
        self.find_bar.update(cx, |bar, cx| {
            bar.folder.follow_current_document = false;
            bar.set_directory(folder, window, cx);
        });
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
        self.search_files(WHAT, searcher, query, FileSource::Listing(request), cx);
    }

    /// Search > Find in Projects: Find All in the files of the project panels chosen in the
    /// panel, read from their workspace files.
    pub(crate) fn find_in_projects(&mut self, cx: &mut Context<Self>) {
        const WHAT: &str = "Find in Projects";
        if self.files_running(cx) {
            return;
        }
        let Some((searcher, query)) = self.searcher(cx) else {
            return;
        };
        let panels = self.find_bar.read(cx).project_panels;
        let workspaces = AppState::global(cx).state.panels.projects.clone();
        let mut files: Vec<PathBuf> = Vec::new();
        for (index, chosen) in panels.iter().enumerate() {
            let Some(file) = workspaces.get(&(index + 1).to_string()).filter(|_| *chosen) else {
                continue;
            };
            match birchpad_config::ProjectWorkspace::load(file) {
                Ok(workspace) => {
                    for path in workspace.files() {
                        if !files.contains(&path) {
                            files.push(path);
                        }
                    }
                }
                Err(error) => {
                    self.report(Some(format!("{WHAT}: {error}")), true, cx);
                    return;
                }
            }
        }
        if files.is_empty() {
            self.report(Some(t!("find-in-files.no-project-files")), true, cx);
            return;
        }
        self.search_files(WHAT, searcher, query, FileSource::Files(files), cx);
    }

    /// Searches the files of `source` (open documents as they are in their tabs) and shows
    /// what it finds in the search results.
    fn search_files(
        &mut self,
        what: &'static str,
        searcher: Searcher,
        query: Query,
        source: FileSource,
        cx: &mut Context<Self>,
    ) {
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
        self.report(Some(format!("{what}: listing files...")), false, cx);
        cx.spawn(async move |this, cx| {
            let files = match source {
                FileSource::Listing(request) => {
                    let Some(files) = list(&this, &request, &cancel, what, cx).await else {
                        return;
                    };
                    files
                }
                FileSource::Files(files) => files,
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
                                    None => read_for_search(&path, ansi)
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
                let progress = format!("{what}: {searched} of {total} files searched...");
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
                    "{}: {}{}{}",
                    t!("find-in-files.find-in-files"),
                    t!(
                        "find-in-files.found",
                        hits = plural(hits, "hit", "hits"),
                        files = plural(run.files.len(), "file", "files"),
                        searched = searched
                    ),
                    skipped.describe(),
                    if stopped {
                        format!(" ({})", t!("find-in-files.stopped"))
                    } else {
                        String::new()
                    }
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
        self.replace_in_files_asking(true, window, cx);
    }

    /// Replace in Files as if the question had been answered Replace.
    #[cfg(feature = "e2e")]
    pub(crate) fn replace_in_files_without_asking(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.replace_in_files_asking(false, window, cx);
    }

    fn replace_in_files_asking(&mut self, ask: bool, window: &mut Window, cx: &mut Context<Self>) {
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
        let answers = [t!("common.replace"), t!("common.cancel")];
        let answers: Vec<&str> = answers.iter().map(String::as_str).collect();
        let answer = ask.then(|| {
            window.prompt(
            PromptLevel::Warning,
            &t!(
                "find-in-files.replace-question",
                folder = request.root.display()
            ),
            Some(&t!("find-in-files.open-documents-are-changed-in-their-tabs-where-undo-takes-the-change-back-other-files-are-changed-on-disk")),
            &answers,
            cx,
            )
        });
        let ansi = AppState::global(cx).ansi;
        let recovery = AppState::global(cx).recovery_dir();
        cx.spawn(async move |this, cx| {
            if let Some(answer) = answer
                && answer.await != Ok(0)
            {
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
                    "{}: {}{}{}",
                    t!("find-in-files.replace-in-files"),
                    t!(
                        "find-in-files.replaced",
                        replaced = plural(replaced, "occurrence replaced", "occurrences replaced"),
                        files = plural(changed, "file", "files")
                    ),
                    skipped.describe(),
                    if stopped {
                        format!(" ({})", t!("find-in-files.stopped"))
                    } else {
                        String::new()
                    }
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
