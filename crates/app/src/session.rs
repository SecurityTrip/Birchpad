//! Sessions in the application (ADR 0017): what is open and where each view was is saved on
//! quitting and put back on the next launch. With `session.backup-unsaved`, unsaved text goes
//! to backup copies every few seconds and on quitting, so quitting does not ask about it and a
//! crash loses at most one interval. The format and its files are `birchpad_config::session`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU64;
use std::time::Duration;

use anyhow::{Context as _, Result};
use birchpad_config::{
    LanguageName, Session, SessionDocument, SessionTab, SessionView, write_private,
};
use birchpad_core::{Document, Encoding, Format, Range, RevisionId, Rope, Selection};
use birchpad_io::LoadOptions;
use gpui_kit::{App, AppContext as _, Context, Entity, Task, Window, px};

use crate::app_state::{AppState, SessionPaths};
use crate::buffer::{Backup, Buffer, ReadOnly};
use crate::commands::{CommandRegistry, Register as _};
use crate::editor::{EditorView, ViewState};
use crate::pane::TabMarks;
use crate::workspace::{Workspace, report_error};

pub fn register_commands(registry: &mut CommandRegistry) {
    registry.workspace("file.save-session", |this, (), window, cx| {
        let session = this.session(false, cx);
        let directory = this.default_directory(cx);
        cx.spawn_in(window, async move |_, cx| {
            let path = crate::path_dialog::ask_save(directory, "session.toml".into(), cx).await?;
            if let Err(error) = write_private(&path, session.to_toml().as_bytes()) {
                cx.update(|window, cx| {
                    let error = anyhow::Error::new(error)
                        .context(t!("session.cannot-save", path = path.display()));
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
pub struct SessionState {
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
    /// Files of the restored session that could not be read (Remember inaccessible files):
    /// they stay in the sessions written, to open once they can be.
    pub inaccessible: Vec<SessionDocument>,
}

/// What a snapshot writes: backup copies of unsaved text, then the session.
pub struct Snapshot {
    paths: SessionPaths,
    backups: Vec<(PathBuf, Rope)>,
    session: Option<Session>,
    /// Backup copies to keep; any other file in the backup folder is removed.
    keep: HashSet<String>,
}

/// What the workspace records once a snapshot is written.
pub struct Written {
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
    pub fn session(&self, backups: bool, cx: &App) -> Session {
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
                view.tabs
                    .push(tab_of(document, editor.view_state(cx), editor.tab));
            }
            *session.views_mut()[index] = view;
        }
        // Files that could not be read stay, at the end of the main view.
        for document in &self.session_state.inaccessible {
            if document.path.is_some()
                && !session
                    .documents
                    .iter()
                    .any(|known| known.path == document.path)
            {
                session.documents.push(document.clone());
                session.main_view.tabs.push(SessionTab {
                    document: session.documents.len() - 1,
                    ..SessionTab::default()
                });
            }
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
                    name: birchpad_config::backup_name(&display_name),
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
    pub fn restore_last_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
    pub fn write_snapshot_now(&mut self, cx: &mut Context<Self>) {
        if let Some((snapshot, written)) = self.plan_snapshot(cx) {
            let result = snapshot.write();
            self.finish_snapshot(written, result, cx);
        }
    }

    /// Writes the snapshot in the background, unless a write is still running.
    pub fn write_snapshot_later(&mut self, cx: &mut Context<Self>) -> Task<()> {
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
    pub fn start_backups(&mut self, cx: &mut Context<Self>) {
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
    pub fn save_session_for_quit(&mut self, cx: &mut Context<Self>) {
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
    pub fn load_session_file(
        &mut self,
        path: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let session = Session::read(path)?;
        if self.restore_session(&session, None, window, cx) == 0 {
            anyhow::bail!(
                "{}",
                t!(
                    "session.lists-no-document-that-could-be-opened",
                    path = path.display()
                )
            );
        }
        Ok(())
    }

    /// Opens the documents of `session` in their views, with their carets, scroll positions,
    /// folds and bookmarks. Backup copies named relative to `backups` give unsaved text.
    /// Returns the number of the session's tabs now shown, opened or already open.
    pub fn restore_session(
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
        // Preferences > Backup > Remember inaccessible files.
        if AppState::global(cx).settings.session.remember_inaccessible {
            let missing = session
                .documents
                .iter()
                .zip(&buffers)
                .filter(|(document, buffer)| {
                    buffer.is_none() && document.path.as_ref().is_some_and(|path| !path.exists())
                })
                .map(|(document, _)| SessionDocument {
                    backup: None,
                    modified: false,
                    ..document.clone()
                });
            self.session_state.inaccessible.extend(missing);
        }
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
                        editor.update(cx, |editor, cx| {
                            editor.tab = marks_of(tab);
                            editor.restore(state_of(tab), cx);
                        });
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
                if let Some(line_ending) = document
                    .line_ending
                    .as_deref()
                    .and_then(birchpad_config::parse_line_ending)
                {
                    format.line_ending = line_ending;
                }
                let untitled = path
                    .is_none()
                    .then(|| birchpad_config::free_untitled_number(document.untitled, numbers));
                let doc = Document::with_format(text, format);
                let (modified, read_only) = (document.modified, document.read_only);
                // The file as it was when the changes were backed up; unknown (an imported
                // session): as it is now, so that nothing is asked.
                let disk = match (document.file_len, &path) {
                    (Some(len), _) => Some(birchpad_io::DiskStamp {
                        len,
                        modified: document
                            .file_modified
                            .map(|nanos| std::time::UNIX_EPOCH + Duration::from_nanos(nanos)),
                    }),
                    (None, Some(path)) => birchpad_io::stamp(path).ok().flatten(),
                    (None, None) => None,
                };
                cx.new(|cx| {
                    let mut buffer = Buffer::restored(path, untitled, doc, modified, read_only, cx);
                    buffer.assume_disk(disk);
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
        match document.language.as_deref().map(LanguageName::parse) {
            Some(LanguageName::User(name)) => {
                if let Some(found) = crate::user_languages::find(name, cx) {
                    buffer.update(cx, |buffer, cx| buffer.set_user_language(Some(found), cx));
                }
            }
            Some(LanguageName::Text) => {
                buffer.update(cx, |buffer, cx| buffer.set_language(None, cx));
            }
            Some(LanguageName::Builtin(name)) => {
                if let Some(language) = language(name) {
                    buffer.update(cx, |buffer, cx| buffer.set_language(Some(language), cx));
                }
            }
            None => {}
        }
        if let Some(title) = document
            .title
            .clone()
            .filter(|title| !title.trim().is_empty())
            && document.path.is_none()
        {
            buffer.update(cx, |buffer, cx| buffer.set_title(Some(title), cx));
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
        title: buffer.title().map(str::to_owned),
        path,
        modified: backup.is_some() && buffer.is_modified(),
        line_ending: backup
            .is_some()
            .then(|| birchpad_config::line_ending_name(format.line_ending).to_owned()),
        backup,
        encoding,
        language: match (buffer.chosen_language(), buffer.user_language()) {
            (Some(_), Some(found)) => Some(LanguageName::User(&found.name).write()),
            (chosen, _) => chosen.map(|language| {
                language
                    .map_or(LanguageName::Text, |language| {
                        LanguageName::Builtin(language.id)
                    })
                    .write()
            }),
        },
        read_only: buffer.requested_read_only(),
        bookmarks: buffer.bookmark_lines(),
        file_len: buffer.disk().filter(|_| backups).map(|disk| disk.len),
        file_modified: buffer
            .disk()
            .filter(|_| backups)
            .and_then(|disk| disk.modified)
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .and_then(|elapsed| u64::try_from(elapsed.as_nanos()).ok()),
    })
}

/// Unsaved text that would be lost: changes to a file, or an untitled document with text.
fn needs_backup(buffer: &Buffer) -> bool {
    if buffer.read_only() == Some(ReadOnly::Loading) {
        return false;
    }
    buffer.is_modified() || (buffer.path().is_none() && buffer.doc().text().len() > 0)
}

fn tab_of(document: usize, state: ViewState, marks: TabMarks) -> SessionTab {
    SessionTab {
        pinned: marks.pinned,
        color: marks.color,
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
        rtl: state.rtl,
    }
}

/// A tab's pin and color; a color past the last is none.
fn marks_of(tab: &SessionTab) -> TabMarks {
    TabMarks {
        pinned: tab.pinned,
        color: tab
            .color
            .filter(|color| (1..=crate::pane::TAB_COLORS).contains(color)),
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
        rtl: tab.rtl,
    }
}

/// One of Birchpad's languages, by its id or its Notepad++ name.
fn language(name: &str) -> Option<&'static birchpad_syntax::Language> {
    birchpad_syntax::by_id(name).or_else(|| {
        birchpad_syntax::LANGUAGES
            .iter()
            .find(|language| language.name.eq_ignore_ascii_case(name))
    })
}
