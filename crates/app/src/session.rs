//! Sessions in the application (ADR 0017): what is open and where each view was is saved on
//! quitting and put back on the next launch. With `session.backup-unsaved`, unsaved text goes
//! to backup copies every few seconds and on quitting, so quitting does not ask about it and a
//! crash loses at most one interval. The format and its files are `birchpad_config::session`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU64;
use std::time::Duration;

use anyhow::{Context as _, Result};
use birchpad_config::{Session, SessionDocument, SessionTab, SessionView, write_private};
use birchpad_core::{Document, Encoding, Format, LineEnding, Range, RevisionId, Rope, Selection};
use birchpad_io::LoadOptions;
use gpui_kit::{App, AppContext as _, Context, Entity, Task, Window, px};

use crate::app_state::{AppState, SessionPaths};
use crate::buffer::{Backup, Buffer, ReadOnly};
use crate::commands::CommandRegistry;
use crate::editor::{EditorView, ViewState};
use crate::workspace::{Workspace, report_error};

pub(crate) fn register_commands(registry: &mut CommandRegistry) {
    registry.workspace("file.save-session", |this, (), window, cx| {
        let session = this.session(false, cx);
        let directory = this.default_directory(cx);
        cx.spawn_in(window, async move |_, cx| {
            let path = crate::path_dialog::ask_save(directory, "session.toml".into(), cx).await?;
            if let Err(error) = write_private(&path, session.to_toml().as_bytes()) {
                cx.update(|window, cx| {
                    let error = anyhow::Error::new(error)
                        .context(format!("Cannot save the session to {}", path.display()));
                    report_error(&error, window, cx);
                })
                .ok();
            }
            Some(())
        })
        .detach();
        Ok(())
    });
    registry.workspace("file.load-session", |this, (), window, cx| {
        let directory = this.default_directory(cx);
        cx.spawn_in(window, async move |this, cx| {
            let paths = crate::path_dialog::ask_open(directory, cx).await?;
            this.update_in(cx, |this, window, cx| {
                for path in paths {
                    if let Err(error) = this.load_session_file(&path, window, cx) {
                        report_error(&error, window, cx);
                    }
                }
            })
            .ok()
        })
        .detach();
        Ok(())
    });
}

/// The session bookkeeping of a workspace.
#[derive(Default)]
pub(crate) struct SessionState {
    /// The session last written, to skip writing it again unchanged.
    written: Option<String>,
    /// Backup copies the last written session uses: kept until the next one is written, as the
    /// previous session still names them.
    names: HashSet<String>,
    writing: bool,
    /// Backup copies only the previous session used are still on disk.
    stale_backups: bool,
    /// The session was saved for quitting; quitting again does not save it again.
    saved_for_quit: bool,
    timer: Option<Task<()>>,
}

/// What a snapshot writes: backup copies of unsaved text, then the session.
pub(crate) struct Snapshot {
    paths: SessionPaths,
    backups: Vec<(PathBuf, Rope)>,
    session: Option<Session>,
    /// Backup copies to keep; any other file in the backup folder is removed.
    keep: HashSet<String>,
}

/// What the workspace records once a snapshot is written.
pub(crate) struct Written {
    backups: Vec<(Entity<Buffer>, String, RevisionId)>,
    session: Option<String>,
    names: HashSet<String>,
}

impl Snapshot {
    /// Writes the backup copies, then the session, then removes backup copies neither the
    /// session nor the previous one uses.
    fn write(&self) -> Result<()> {
        for (path, text) in &self.backups {
            write_private(path, text.to_string().as_bytes())
                .with_context(|| format!("cannot write the backup copy {}", path.display()))?;
        }
        if let Some(session) = &self.session {
            session
                .save(&self.paths.file)
                .with_context(|| format!("cannot save {}", self.paths.file.display()))?;
        }
        if let Ok(entries) = std::fs::read_dir(&self.paths.backups) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                let is_file = entry.file_type().is_ok_and(|kind| kind.is_file());
                if is_file && !self.keep.contains(&name) {
                    std::fs::remove_file(entry.path()).ok();
                }
            }
        }
        Ok(())
    }
}

impl Workspace {
    /// The session of this window. With `backups`, unsaved text is referenced by its backup
    /// copy; without (Save Session, or quitting when nothing is backed up), documents reopen
    /// from their files and untitled ones are left out, as in Notepad++.
    pub(crate) fn session(&self, backups: bool, cx: &App) -> Session {
        let mut session = Session::new();
        session.active_view = self.active_pane_index();
        let mut buffers: Vec<Entity<Buffer>> = Vec::new();
        for (index, pane) in self.panes().iter().enumerate() {
            let pane = pane.read(cx);
            let mut view = SessionView::default();
            for item in pane.items() {
                let editor = item.read(cx);
                let document = match buffers.iter().position(|known| *known == editor.buffer) {
                    Some(known) => known,
                    None => {
                        let Some(document) = document_of(editor.buffer.read(cx), backups) else {
                            continue;
                        };
                        buffers.push(editor.buffer.clone());
                        session.documents.push(document);
                        buffers.len() - 1
                    }
                };
                if pane.active_item() == Some(item) {
                    view.active = view.tabs.len();
                }
                view.tabs.push(tab_of(document, editor.view_state(cx)));
            }
            *session.views_mut()[index] = view;
        }
        if session.views()[session.active_view].tabs.is_empty() {
            session.active_view = 0;
        }
        session
    }

    /// Gives every buffer with unsaved text a backup copy, and plans writing those that changed
    /// and the session if it changed. `None` when there is nothing to write.
    fn plan_snapshot(&mut self, cx: &mut Context<Self>) -> Option<(Snapshot, Written)> {
        let paths = AppState::global(cx).session_paths()?;
        let mut backups = Vec::new();
        let mut written = Vec::new();
        let buffers: Vec<Entity<Buffer>> = self
            .all_views(cx)
            .iter()
            .map(|view| view.read(cx).buffer.clone())
            .collect();
        let mut seen = HashSet::new();
        for entity in buffers {
            if !seen.insert(entity.entity_id()) {
                continue;
            }
            entity.clone().update(cx, |buffer, _| {
                if !needs_backup(buffer) {
                    buffer.backup = None;
                    return;
                }
                let display_name = buffer.display_name();
                let revision = buffer.doc().revision();
                let backup = buffer.backup.get_or_insert_with(|| Backup {
                    name: backup_name(&display_name),
                    revision: None,
                });
                if backup.revision != Some(revision) {
                    let name = backup.name.clone();
                    backups.push((paths.backups.join(&name), buffer.doc().text().clone()));
                    written.push((entity.clone(), name, revision));
                }
            });
        }
        let session = self.session(true, cx);
        let toml = session.to_toml();
        let changed = self.session_state.written.as_deref() != Some(toml.as_str());
        if backups.is_empty() && !changed && !self.session_state.stale_backups {
            return None;
        }
        let names: HashSet<String> = session.backup_names().map(str::to_owned).collect();
        // While the previous session is the fallback, its copies stay; the next snapshot
        // removes them.
        let keep: HashSet<String> = if changed {
            names.union(&self.session_state.names).cloned().collect()
        } else {
            names.clone()
        };
        Some((
            Snapshot {
                paths,
                backups,
                session: changed.then_some(session),
                keep,
            },
            Written {
                backups: written,
                session: changed.then_some(toml),
                names,
            },
        ))
    }

    fn finish_snapshot(&mut self, written: Written, result: Result<()>, cx: &mut Context<Self>) {
        if let Err(error) = result {
            eprintln!("session: {error:#}");
            return;
        }
        for (buffer, name, revision) in written.backups {
            buffer.update(cx, |buffer, _| {
                if let Some(backup) = &mut buffer.backup
                    && backup.name == name
                {
                    backup.revision = Some(revision);
                }
            });
        }
        if written.session.is_some() {
            self.session_state.stale_backups = !self.session_state.names.is_subset(&written.names);
            self.session_state.written = written.session;
            self.session_state.names = written.names;
        } else {
            self.session_state.stale_backups = false;
        }
    }

    /// At startup: the documents of the last session, or an empty "new 1".
    pub(crate) fn restore_last_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let restored = AppState::global(cx)
            .session_paths()
            .and_then(|paths| {
                let session = Session::load(&paths.file)?;
                Some(self.restore_session(&session, Some(&paths.backups), window, cx))
            })
            .unwrap_or(0);
        if restored == 0 {
            self.new_file(window, cx);
        }
    }

    /// Writes the snapshot now, on this thread (quitting).
    pub(crate) fn write_snapshot_now(&mut self, cx: &mut Context<Self>) {
        if let Some((snapshot, written)) = self.plan_snapshot(cx) {
            let result = snapshot.write();
            self.finish_snapshot(written, result, cx);
        }
    }

    /// Writes the snapshot in the background, unless a write is still running.
    pub(crate) fn write_snapshot_later(&mut self, cx: &mut Context<Self>) -> Task<()> {
        if self.session_state.writing {
            return Task::ready(());
        }
        let Some((snapshot, written)) = self.plan_snapshot(cx) else {
            return Task::ready(());
        };
        self.session_state.writing = true;
        cx.spawn(async move |this, cx| {
            let result = cx.background_spawn(async move { snapshot.write() }).await;
            this.update(cx, |this, cx| {
                this.session_state.writing = false;
                this.finish_snapshot(written, result, cx);
            })
            .ok();
        })
    }

    /// Backs up unsaved changes every `session.backup-interval-seconds`, if they are backed up.
    pub(crate) fn start_backups(&mut self, cx: &mut Context<Self>) {
        let state = AppState::global(cx);
        if !state.backs_up_unsaved() {
            return;
        }
        let seconds = state.settings.session.backup_interval_seconds.max(1);
        let interval = Duration::from_secs(u64::from(seconds));
        self.session_state.timer = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(interval).await;
                let Ok(writing) = this.update(cx, |this, cx| this.write_snapshot_later(cx)) else {
                    break;
                };
                writing.await;
            }
        }));
    }

    /// Saves what the next launch restores, once, when quitting: with backups everything,
    /// unsaved text included; otherwise the files (after the user answered about unsaved
    /// changes, or, when quitting cannot be asked about, without them).
    pub(crate) fn save_session_for_quit(&mut self, cx: &mut Context<Self>) {
        if std::mem::replace(&mut self.session_state.saved_for_quit, true) {
            return;
        }
        let state = AppState::global(cx);
        if state.backs_up_unsaved() {
            self.write_snapshot_now(cx);
        } else if let Some(paths) = state.session_paths() {
            if let Err(error) = self.session(false, cx).save(&paths.file) {
                eprintln!("session: cannot save {}: {error}", paths.file.display());
            }
        } else {
            // Nothing remembers the open files: they become recent files, as in Notepad++.
            self.remember_open_files(cx);
        }
    }

    /// Opens a session file (Birchpad's or Notepad++'s) in addition to what is open.
    pub(crate) fn load_session_file(
        &mut self,
        path: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let session = Session::read(path)?;
        if self.restore_session(&session, None, window, cx) == 0 {
            anyhow::bail!("{} lists no document that could be opened", path.display());
        }
        Ok(())
    }

    /// Opens the documents of `session` in their views, with their carets, scroll positions,
    /// folds and bookmarks. Backup copies named relative to `backups` give unsaved text.
    /// Returns the number of the session's tabs now shown, opened or already open.
    pub(crate) fn restore_session(
        &mut self,
        session: &Session,
        backups: Option<&Path>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> usize {
        if backups.is_some() {
            // The session being restored is the fallback until the next one is written.
            let names = session.backup_names().map(str::to_owned);
            self.session_state.names.extend(names);
        }
        let mut numbers: HashSet<usize> = self
            .all_views(cx)
            .iter()
            .filter_map(|view| view.read(cx).buffer.read(cx).untitled_number())
            .collect();
        let buffers: Vec<Option<Entity<Buffer>>> = session
            .documents
            .iter()
            .map(|document| self.restore_document(document, backups, &mut numbers, window, cx))
            .collect();
        let mut shown_tabs = 0;
        for (index, view) in session.views().into_iter().enumerate() {
            let pane = self.panes()[index].clone();
            let mut active = None;
            for (position, tab) in view.tabs.iter().enumerate() {
                let Some(Some(buffer)) = buffers.get(tab.document) else {
                    continue;
                };
                // A pane shows a document once: an open one stays as it is.
                let shown = pane
                    .read(cx)
                    .items()
                    .iter()
                    .find(|item| &item.read(cx).buffer == buffer)
                    .cloned();
                let editor = match shown {
                    Some(shown) => shown,
                    None => {
                        let editor = cx.new(|cx| EditorView::new(buffer.clone(), window, cx));
                        editor.update(cx, |editor, cx| editor.restore(state_of(tab), cx));
                        self.track_view(&editor, window, cx);
                        pane.update(cx, |pane, cx| {
                            let end = pane.items().len();
                            pane.insert(end, editor.clone(), window, cx);
                        });
                        editor
                    }
                };
                if position <= view.active || active.is_none() {
                    active = Some(editor);
                }
                shown_tabs += 1;
            }
            if let Some(active) = active {
                self.activate_view(&active, window, cx);
            }
        }
        let preferred = session.active_view.min(1);
        let target = [preferred, 1 - preferred]
            .into_iter()
            .find(|&index| self.panes()[index].read(cx).active_item().is_some());
        if let Some(view) =
            target.and_then(|index| self.panes()[index].read(cx).active_item().cloned())
        {
            self.activate_view(&view, window, cx);
        }
        shown_tabs
    }

    /// The buffer of a session's document: already open, read from its file, or from a backup
    /// copy of unsaved text. `None` if neither the file nor the copy exists any more.
    fn restore_document(
        &mut self,
        document: &SessionDocument,
        backups: Option<&Path>,
        numbers: &mut HashSet<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Entity<Buffer>> {
        if let Some(path) = &document.path
            && let Some(view) = self.view_for_path(path, cx)
        {
            return Some(view.read(cx).buffer.clone());
        }
        let ansi = AppState::global(cx).ansi;
        let encoding = document
            .encoding
            .as_deref()
            .and_then(|name| birchpad_io::parse_encoding(name, ansi));
        let backup = document.backup.as_deref().and_then(|name| {
            let own = !Path::new(name).is_absolute();
            let path = if own {
                backups?.join(name)
            } else {
                PathBuf::from(name)
            };
            // Birchpad's copies are UTF-8; Notepad++'s are in the document's encoding.
            let options = LoadOptions {
                encoding: own.then_some(Encoding::Utf8),
                ansi,
            };
            let file = birchpad_io::load(&path, options, &AtomicU64::new(0)).ok()?;
            Some((own.then(|| name.to_owned()), file.text))
        });
        let buffer = match (document.path.clone(), backup) {
            (path, Some((name, text))) => {
                let mut format = Format::new();
                if let Some((encoding, bom)) = encoding {
                    format.encoding = encoding;
                    format.bom = bom && encoding.is_unicode();
                }
                if let Some(line_ending) = document.line_ending.as_deref().and_then(line_ending) {
                    format.line_ending = line_ending;
                }
                let untitled = path
                    .is_none()
                    .then(|| free_number(document.untitled, numbers));
                let doc = Document::with_format(text, format);
                let (modified, read_only) = (document.modified, document.read_only);
                cx.new(|cx| {
                    let mut buffer = Buffer::restored(path, untitled, doc, modified, read_only, cx);
                    let revision = buffer.doc().revision();
                    buffer.backup = name.map(|name| Backup {
                        name,
                        revision: Some(revision),
                    });
                    buffer
                })
            }
            (Some(path), None) => {
                if !path.exists() {
                    return None;
                }
                let options =
                    AppState::global(cx).load_options(encoding.map(|(encoding, _)| encoding));
                let read_only = document.read_only;
                cx.new(|cx| Buffer::open(path, options, read_only, cx))
            }
            (None, None) => return None,
        };
        if let Some(language) = document.language.as_deref().and_then(language) {
            buffer.update(cx, |buffer, cx| buffer.set_language(language, cx));
        }
        if !document.bookmarks.is_empty() {
            let lines = document.bookmarks.clone();
            buffer.update(cx, |buffer, cx| buffer.restore_bookmarks(lines, cx));
        }
        self.watch_buffer(&buffer, window, cx);
        Some(buffer)
    }
}

/// A session document for `buffer`, or `None` for an untitled one with nothing to keep.
fn document_of(buffer: &Buffer, backups: bool) -> Option<SessionDocument> {
    let backup = buffer
        .backup
        .as_ref()
        .filter(|_| backups)
        .map(|backup| backup.name.clone());
    let path = buffer.path().map(Path::to_owned);
    if path.is_none() && backup.is_none() {
        return None;
    }
    let format = buffer.doc().format();
    let encoding = if backup.is_some() {
        Some(birchpad_io::encoding_name(format.encoding, format.bom))
    } else {
        buffer
            .chosen_encoding()
            .map(|encoding| birchpad_io::encoding_name(encoding, false))
    };
    Some(SessionDocument {
        untitled: path.is_none().then(|| buffer.untitled_number()).flatten(),
        path,
        modified: backup.is_some() && buffer.is_modified(),
        line_ending: backup
            .is_some()
            .then(|| line_ending_name(format.line_ending).to_owned()),
        backup,
        encoding,
        language: buffer
            .chosen_language()
            .map(|language| language.map_or("text", |language| language.id).to_owned()),
        read_only: buffer.requested_read_only(),
        bookmarks: buffer.bookmark_lines(),
    })
}

/// Unsaved text that would be lost: changes to a file, or an untitled document with text.
fn needs_backup(buffer: &Buffer) -> bool {
    if buffer.read_only() == Some(ReadOnly::Loading) {
        return false;
    }
    buffer.is_modified() || (buffer.path().is_none() && buffer.doc().text().len() > 0)
}

/// A new name for a backup copy, like Notepad++'s: the document's name and when.
fn backup_name(display_name: &str) -> String {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis());
    let count = COUNTER.fetch_add(1, Ordering::Relaxed);
    let name: String = display_name
        .chars()
        .map(|ch| if r#"\/:*?"<>|"#.contains(ch) { '_' } else { ch })
        .collect();
    format!("{name}@{millis}-{count}")
}

/// `wanted` if no untitled document has that number yet, else the lowest free one.
fn free_number(wanted: Option<usize>, numbers: &mut HashSet<usize>) -> usize {
    let number = wanted
        .filter(|number| *number > 0 && !numbers.contains(number))
        .unwrap_or_else(|| (1..).find(|n| !numbers.contains(n)).expect("a free number"));
    numbers.insert(number);
    number
}

fn tab_of(document: usize, state: ViewState) -> SessionTab {
    SessionTab {
        document,
        selections: state
            .selection
            .ranges()
            .iter()
            .map(|range| [range.anchor, range.head])
            .collect(),
        primary: state.selection.primary_index(),
        first_row: state.scroll_top,
        scroll_x: state.scroll_left / px(1.),
        folds: state.collapsed,
    }
}

fn state_of(tab: &SessionTab) -> ViewState {
    let ranges: Vec<Range> = tab
        .selections
        .iter()
        .map(|&[anchor, head]| Range::new(anchor, head))
        .collect();
    let selection = if ranges.is_empty() {
        Selection::point(0)
    } else {
        let primary = tab.primary.min(ranges.len() - 1);
        Selection::new(ranges, primary)
    };
    ViewState {
        selection,
        scroll_top: tab.first_row,
        scroll_left: px(tab.scroll_x),
        collapsed: tab.folds.clone(),
    }
}

fn line_ending_name(line_ending: LineEnding) -> &'static str {
    match line_ending {
        LineEnding::CrLf => "crlf",
        LineEnding::Lf => "lf",
        LineEnding::Cr => "cr",
    }
}

fn line_ending(name: &str) -> Option<LineEnding> {
    match name {
        "crlf" => Some(LineEnding::CrLf),
        "lf" => Some(LineEnding::Lf),
        "cr" => Some(LineEnding::Cr),
        _ => None,
    }
}

/// A chosen language: Birchpad's id, `text` for normal text, or a Notepad++ language name.
fn language(name: &str) -> Option<Option<&'static birchpad_syntax::Language>> {
    if name == "text" {
        return Some(None);
    }
    birchpad_syntax::by_id(name)
        .or_else(|| {
            birchpad_syntax::LANGUAGES
                .iter()
                .find(|language| language.name.eq_ignore_ascii_case(name))
        })
        .map(Some)
}
