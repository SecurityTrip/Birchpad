//! The file behind a buffer: reading it in the background, its name, saving, read-only states and
//! following its changes on disk.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use birchpad_core::{Document, Edit, Format, RevisionId, Rope, Selection, Transaction};
use birchpad_io::{ByteChanges, DiskStamp, Head, LoadOptions, LoadedFile, ReadError};
use gpui_kit::{AppContext as _, Context, Task};

use birchpad_ui::AppState;

use super::marks::DocumentMarks;
use super::{Buffer, BufferEvent, ChangeTracking, Loading, ReadOnly, SyntaxState};

impl Buffer {
    /// A buffer for `path` whose content is read in the background.
    pub fn open(
        path: PathBuf,
        options: LoadOptions,
        read_only: bool,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut buffer = Self {
            doc: Document::new(),
            path: Some(path),
            untitled: None,
            title: None,
            problem: None,
            decode_changes: ByteChanges::default(),
            file_read_only: false,
            requested_read_only: read_only,
            chosen_encoding: None,
            loading: None,
            marks: DocumentMarks::default(),
            changes: ChangeTracking::new(Rope::new()),
            pending_bookmarks: None,
            backup: None,
            disk: None,
            head: None,
            monitoring: false,
            saving: 0,
            follow_end: false,
            loaded: false,
            syntax: SyntaxState::default(),
        };
        buffer.load(options, None, cx);
        buffer
    }

    /// (Re)reads the file, replacing the document when done. If `bom` is given and differs from
    /// the file, it is applied afterwards as an undoable format change.
    pub fn load(&mut self, options: LoadOptions, bom: Option<bool>, cx: &mut Context<Self>) {
        let Some(path) = self.path.clone() else {
            return;
        };
        self.chosen_encoding = options.encoding;
        let total = std::fs::metadata(&path).map_or(0, |metadata| metadata.len());
        let progress = Arc::new(AtomicU64::new(0));
        let reader_progress = progress.clone();
        let started = Instant::now();
        let task = cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(
                    async move { birchpad_io::load(&path, options, &reader_progress) },
                )
                .await;
            if std::env::var_os("BIRCHPAD_TIMINGS").is_some() {
                eprintln!("timing: read and decoded in {:?}", started.elapsed());
            }
            this.update(cx, |buffer, cx| {
                buffer.finish_loading(result, options, cx);
                if let Some(bom) = bom {
                    let encoding = buffer.doc.format().encoding;
                    if buffer.problem.is_none() && encoding.is_unicode() {
                        buffer.set_format(encoding, bom, &Selection::point(0), cx);
                    }
                }
            })
            .ok();
        });
        // Redraw the progress indicator while reading.
        let ticker = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                let loading = this.update(cx, |buffer, cx| {
                    cx.notify();
                    buffer.loading.is_some()
                });
                if !loading.unwrap_or(false) {
                    break;
                }
            }
        });
        self.loading = Some(Loading {
            progress,
            total,
            tasks: [task, ticker],
        });
        cx.emit(BufferEvent::StateChanged);
        cx.notify();
    }

    pub(super) fn finish_loading(
        &mut self,
        result: Result<LoadedFile, ReadError>,
        options: LoadOptions,
        cx: &mut Context<Self>,
    ) {
        // This runs inside the loading task: let it and the ticker finish instead of
        // cancelling them from within.
        if let Some(loading) = self.loading.take() {
            loading.tasks.into_iter().for_each(Task::detach);
        }
        // Read again: bookmarks stay on their lines, views keep their places.
        let previous = self.loaded.then(|| self.doc.text().clone());
        let bookmarks = previous
            .as_ref()
            .map(|text| self.marks.bookmarks.lines(text));
        match result {
            Ok(file) => {
                // Performance > Deactivate Word Wrap globally: a large file turns it off.
                let limit =
                    u64::from(AppState::global(cx).settings.files.large_file_limit_mb) << 20;
                if file.text.len() as u64 > limit
                    && AppState::global(cx).settings.files.large_file_no_wrap
                {
                    AppState::update_state(cx, |state, _| state.word_wrap = Some(false));
                }
                self.problem = file.problem;
                self.decode_changes = file.changes;
                self.file_read_only = file.info.read_only;
                self.disk = Some(file.info.stamp());
                self.head = Some(file.head);
                self.doc = Document::with_format(file.text, file.format);
                self.marks = DocumentMarks::default();
                self.changes = ChangeTracking::new(self.doc.text().clone());
                if let Some(lines) = bookmarks {
                    self.marks.bookmarks.set_lines(self.doc.text(), lines);
                }
                self.set_pending_bookmarks();
                self.loaded = true;
                self.detect_language(cx);
                cx.emit(BufferEvent::Reloaded { previous });
                if std::mem::take(&mut self.follow_end) {
                    cx.emit(BufferEvent::FollowEnd);
                }
            }
            Err(ReadError::NotFound(_)) => {
                // Opening a path that does not exist yet: it is created on save, as in
                // Notepad++. The encoding is the requested one, or the default.
                let mut format = Format::new();
                if let Some(encoding) = options.encoding {
                    format.encoding = encoding;
                }
                self.problem = None;
                self.decode_changes = ByteChanges::default();
                self.disk = None;
                self.head = None;
                self.doc = Document::with_format(Rope::new(), format);
                self.marks = DocumentMarks::default();
                self.changes = ChangeTracking::new(Rope::new());
                self.set_pending_bookmarks();
                self.loaded = true;
                self.detect_language(cx);
                cx.emit(BufferEvent::Reloaded { previous });
            }
            Err(error) => cx.emit(BufferEvent::LoadFailed(Arc::new(error))),
        }
        cx.emit(BufferEvent::StateChanged);
        cx.notify();
    }
}

impl Buffer {
    /// Opened read-only on purpose (`-ro`).
    pub fn requested_read_only(&self) -> bool {
        self.requested_read_only
    }

    /// Edit > Set Read-Only: the document cannot be edited, whatever its file allows.
    pub fn set_requested_read_only(&mut self, on: bool, cx: &mut Context<Self>) {
        self.requested_read_only = on;
        cx.emit(BufferEvent::StateChanged);
        cx.notify();
    }

    /// Edit > Read-Only Attribute of the File made the file read-only.
    pub fn file_made_read_only(&mut self, cx: &mut Context<Self>) {
        self.file_read_only = true;
        cx.emit(BufferEvent::StateChanged);
        cx.notify();
    }

    /// Edit > Clear Read-Only Flag made the file writable.
    pub fn file_made_writable(&mut self, cx: &mut Context<Self>) {
        self.file_read_only = false;
        cx.emit(BufferEvent::StateChanged);
        cx.notify();
    }
}

impl Buffer {
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn untitled_number(&self) -> Option<usize> {
        self.untitled
    }

    /// The name shown on the tab: the file name, the name an untitled document was given, or
    /// "new N".
    pub fn display_name(&self) -> String {
        match (&self.path, &self.title, self.untitled) {
            (Some(path), _, _) => path.file_name().map_or_else(
                || path.display().to_string(),
                |name| name.to_string_lossy().into_owned(),
            ),
            (None, Some(title), _) => title.clone(),
            (None, None, Some(number)) => format!("new {number}"),
            (None, None, None) => "new".to_owned(),
        }
    }

    /// The language as `languages.<key>` names it: an id, `text`, or `udl:<name>`.
    pub fn language_key(&self) -> String {
        match (self.language(), self.user_language()) {
            (Some(language), _) => language.id.to_owned(),
            (None, Some(user)) => format!("{}{}", birchpad_config::USER_LANGUAGE_PREFIX, user.name),
            (None, None) => "text".to_owned(),
        }
    }

    /// The name an untitled document was given, if any.
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref().filter(|_| self.path.is_none())
    }

    /// Gives an untitled document a name (File > Rename); `None` goes back to "new N".
    pub fn set_title(&mut self, title: Option<String>, cx: &mut Context<Self>) {
        self.title = title;
        cx.emit(BufferEvent::StateChanged);
        cx.notify();
    }

    /// The file was renamed to `path` (File > Rename): the document follows it.
    pub fn renamed(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.disk = birchpad_io::stamp(&path).ok().flatten();
        self.path = Some(path);
        self.detect_language(cx);
        cx.emit(BufferEvent::StateChanged);
        cx.notify();
    }

    pub fn is_modified(&self) -> bool {
        self.doc.is_modified()
    }

    /// A save starts: until it ends, changes of the file are its own.
    pub fn begin_save(&mut self) {
        self.saving += 1;
    }

    /// A save failed.
    pub fn end_save(&mut self) {
        self.saving = self.saving.saturating_sub(1);
    }

    pub fn is_saving(&self) -> bool {
        self.saving > 0
    }

    /// The file as last read or written; `None` while it does not exist.
    pub fn disk(&self) -> Option<DiskStamp> {
        self.disk
    }

    pub fn head(&self) -> Option<Head> {
        self.head
    }

    /// For text restored from a backup copy: how the file looked when the copy was made.
    pub fn assume_disk(&mut self, disk: Option<DiskStamp>) {
        self.disk = disk;
    }

    pub fn is_monitoring(&self) -> bool {
        self.monitoring
    }

    /// View > Monitoring (tail -f): the document becomes read-only and follows the file.
    pub fn set_monitoring(&mut self, monitoring: bool, cx: &mut Context<Self>) {
        self.monitoring = monitoring;
        if monitoring {
            cx.emit(BufferEvent::FollowEnd);
        }
        cx.emit(BufferEvent::StateChanged);
        cx.notify();
    }

    /// Reads the file again because another program changed it. Views keep their places;
    /// with `follow_end`, they go to the end.
    pub fn reload(&mut self, follow_end: bool, cx: &mut Context<Self>) {
        self.follow_end = follow_end;
        let options = AppState::global(cx).load_options(self.chosen_encoding);
        self.load(options, None, cx);
    }

    /// Adds text that was appended to the file (tail -f). It is not an edit: the document
    /// stays unmodified and has nothing to undo.
    pub fn append_from_disk(
        &mut self,
        text: &str,
        stamp: DiskStamp,
        follow_end: bool,
        cx: &mut Context<Self>,
    ) {
        self.disk = Some(stamp);
        if !text.is_empty() {
            let old_text = self.doc.text().clone();
            let transaction =
                Transaction::from_edits(&old_text, [Edit::insert(old_text.len(), text)])
                    .expect("an insertion at the end");
            let mut new_text = old_text.clone();
            transaction.changes().apply(&mut new_text);
            self.doc = Document::with_format(new_text, self.doc.format());
            // Part of the file, not a change.
            self.changes.history.append(text);
            self.changes.generation += 1;
            self.follow_edit(&old_text, &transaction, cx);
            cx.emit(BufferEvent::Edited {
                transaction,
                origin: None,
            });
        }
        if follow_end {
            cx.emit(BufferEvent::FollowEnd);
        }
        cx.notify();
    }

    /// The file was deleted and the user keeps the document: saving creates it again.
    pub fn keep_deleted(&mut self, cx: &mut Context<Self>) {
        self.disk = None;
        self.head = None;
        self.doc.mark_unsaved();
        cx.emit(BufferEvent::StateChanged);
        cx.notify();
    }

    /// The file changed and the user keeps the document as it is: it differs from the file
    /// now, and that change is not asked about again.
    pub fn keep_changed(&mut self, stamp: Option<DiskStamp>, cx: &mut Context<Self>) {
        self.disk = stamp;
        self.head = None;
        if !self.doc.is_modified() {
            self.doc.mark_unsaved();
        }
        cx.emit(BufferEvent::StateChanged);
        cx.notify();
    }

    /// Records that `revision` of the document, `text`, was written to `path` (a new path after
    /// Save As), which then looked like `disk`.
    pub fn did_save(
        &mut self,
        path: PathBuf,
        revision: RevisionId,
        text: Rope,
        disk: Option<(DiskStamp, Head)>,
        cx: &mut Context<Self>,
    ) {
        self.end_save();
        self.disk = disk.map(|(stamp, _)| stamp);
        self.head = disk.map(|(_, head)| head);
        if self.path.as_deref() != Some(&path) {
            self.path = Some(path);
            self.untitled = None;
            self.file_read_only = false;
            // Saved under another name: the extension may say another language.
            self.detect_language(cx);
        }
        self.doc.mark_saved_at(revision);
        // Modified lines that were saved turn green.
        self.changes.history.save(text);
        self.changes.generation += 1;
        self.compare_changes(cx);
        cx.emit(BufferEvent::StateChanged);
        cx.notify();
    }

    /// Fraction of the file read so far, while loading.
    pub fn loading_progress(&self) -> Option<f32> {
        self.loading.as_ref().map(|loading| {
            let read = loading.progress.load(Ordering::Relaxed);
            if loading.total == 0 {
                0.
            } else {
                (read as f32 / loading.total as f32).min(1.)
            }
        })
    }

    /// Where saving would change a file that did not decode exactly.
    pub fn decode_changes(&self) -> &ByteChanges {
        &self.decode_changes
    }

    /// Makes a document that did not decode exactly editable: the user accepted that saving
    /// changes the bytes in [`Self::decode_changes`].
    pub fn edit_anyway(&mut self, cx: &mut Context<Self>) {
        if self.problem.take().is_some() {
            cx.emit(BufferEvent::StateChanged);
            cx.notify();
        }
    }

    pub fn read_only(&self) -> Option<ReadOnly> {
        if self.loading.is_some() {
            Some(ReadOnly::Loading)
        } else if let Some(problem) = self.problem {
            Some(ReadOnly::Decoding(problem))
        } else if self.monitoring {
            Some(ReadOnly::Monitoring)
        } else if self.file_read_only {
            Some(ReadOnly::File)
        } else if self.requested_read_only {
            Some(ReadOnly::Requested)
        } else {
            None
        }
    }
}
