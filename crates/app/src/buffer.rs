//! A buffer: one open document (a file or an untitled "new N" tab) shared by the views showing it.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use birchpad_core::{
    Document, Encoding, Format, RevisionId, Rope, Selection, Transaction, UndoGrouping,
};
use birchpad_io::{DecodeProblem, LoadOptions, LoadedFile, ReadError};
use gpui_kit::{AppContext as _, Context, EntityId, EventEmitter, Task};

/// What changed in a buffer, so that every view showing it can follow.
#[derive(Debug, Clone)]
pub(crate) enum BufferEvent {
    /// The text changed. `origin` is the view that made the change; it has already updated its
    /// own selection, the other views map theirs through `transaction`.
    Edited {
        transaction: Transaction,
        origin: Option<EntityId>,
    },
    /// The whole document was replaced (loading finished, reinterpreted in another encoding).
    Reloaded,
    /// The modified flag, path, format or read-only state changed.
    StateChanged,
    /// The file could not be read; the buffer stays empty.
    LoadFailed(Arc<ReadError>),
}

/// Why a buffer cannot be edited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReadOnly {
    /// The file is still being read.
    Loading,
    /// The text does not represent the file's bytes exactly.
    Decoding(DecodeProblem),
    /// The file has the read-only attribute or no write permission.
    File,
}

struct Loading {
    progress: Arc<AtomicU64>,
    total: u64,
    tasks: [Task<()>; 2],
}

pub(crate) struct Buffer {
    doc: Document,
    path: Option<PathBuf>,
    /// The number of an untitled buffer: 1 for "new 1".
    untitled: Option<usize>,
    problem: Option<DecodeProblem>,
    file_read_only: bool,
    loading: Option<Loading>,
}

impl EventEmitter<BufferEvent> for Buffer {}

impl Buffer {
    pub(crate) fn untitled(number: usize) -> Self {
        Self {
            doc: Document::new(),
            path: None,
            untitled: Some(number),
            problem: None,
            file_read_only: false,
            loading: None,
        }
    }

    /// An untitled buffer with existing text.
    pub(crate) fn untitled_with(number: usize, doc: Document) -> Self {
        Self {
            doc,
            ..Self::untitled(number)
        }
    }

    /// A buffer for `path` whose content is read in the background.
    pub(crate) fn open(path: PathBuf, options: LoadOptions, cx: &mut Context<Self>) -> Self {
        let mut buffer = Self {
            doc: Document::new(),
            path: Some(path),
            untitled: None,
            problem: None,
            file_read_only: false,
            loading: None,
        };
        buffer.load(options, None, cx);
        buffer
    }

    /// (Re)reads the file, replacing the document when done. If `bom` is given and differs from
    /// the file, it is applied afterwards as an undoable format change.
    pub(crate) fn load(&mut self, options: LoadOptions, bom: Option<bool>, cx: &mut Context<Self>) {
        let Some(path) = self.path.clone() else {
            return;
        };
        let total = std::fs::metadata(&path).map_or(0, |metadata| metadata.len());
        let progress = Arc::new(AtomicU64::new(0));
        let reader_progress = progress.clone();
        let task = cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(
                    async move { birchpad_io::load(&path, options, &reader_progress) },
                )
                .await;
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

    fn finish_loading(
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
        match result {
            Ok(file) => {
                self.problem = file.problem;
                self.file_read_only = file.info.read_only;
                self.doc = Document::with_format(file.text, file.format);
                cx.emit(BufferEvent::Reloaded);
            }
            Err(ReadError::NotFound(_)) => {
                // Opening a path that does not exist yet: it is created on save, as in
                // Notepad++. The encoding is the requested one, or the default.
                let mut format = Format::new();
                if let Some(encoding) = options.encoding {
                    format.encoding = encoding;
                }
                self.problem = None;
                self.doc = Document::with_format(Rope::new(), format);
                cx.emit(BufferEvent::Reloaded);
            }
            Err(error) => cx.emit(BufferEvent::LoadFailed(Arc::new(error))),
        }
        cx.emit(BufferEvent::StateChanged);
        cx.notify();
    }

    pub(crate) fn doc(&self) -> &Document {
        &self.doc
    }

    pub(crate) fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub(crate) fn untitled_number(&self) -> Option<usize> {
        self.untitled
    }

    /// The name shown on the tab: the file name, or "new N".
    pub(crate) fn display_name(&self) -> String {
        match (&self.path, self.untitled) {
            (Some(path), _) => path.file_name().map_or_else(
                || path.display().to_string(),
                |name| name.to_string_lossy().into_owned(),
            ),
            (None, Some(number)) => format!("new {number}"),
            (None, None) => "new".to_owned(),
        }
    }

    pub(crate) fn is_modified(&self) -> bool {
        self.doc.is_modified()
    }

    /// Records that `revision` of the document was written to `path` (a new path after Save As).
    pub(crate) fn did_save(&mut self, path: PathBuf, revision: RevisionId, cx: &mut Context<Self>) {
        if self.path.as_deref() != Some(&path) {
            self.path = Some(path);
            self.untitled = None;
            self.file_read_only = false;
        }
        self.doc.mark_saved_at(revision);
        cx.emit(BufferEvent::StateChanged);
        cx.notify();
    }

    /// Fraction of the file read so far, while loading.
    pub(crate) fn loading_progress(&self) -> Option<f32> {
        self.loading.as_ref().map(|loading| {
            let read = loading.progress.load(Ordering::Relaxed);
            if loading.total == 0 {
                0.
            } else {
                (read as f32 / loading.total as f32).min(1.)
            }
        })
    }

    pub(crate) fn read_only(&self) -> Option<ReadOnly> {
        if self.loading.is_some() {
            Some(ReadOnly::Loading)
        } else if let Some(problem) = self.problem {
            Some(ReadOnly::Decoding(problem))
        } else if self.file_read_only {
            Some(ReadOnly::File)
        } else {
            None
        }
    }

    pub(crate) fn apply(
        &mut self,
        transaction: Transaction,
        selection_before: &Selection,
        grouping: UndoGrouping,
        origin: Option<EntityId>,
        cx: &mut Context<Self>,
    ) {
        let was_modified = self.doc.is_modified();
        let format_before = self.doc.format();
        self.doc.apply(&transaction, selection_before, grouping);
        cx.emit(BufferEvent::Edited {
            transaction,
            origin,
        });
        if was_modified != self.doc.is_modified() || format_before != self.doc.format() {
            cx.emit(BufferEvent::StateChanged);
        }
        cx.notify();
    }

    /// Changes how the document is saved, as an undoable edit ("Convert to").
    pub(crate) fn set_format(
        &mut self,
        encoding: Encoding,
        bom: bool,
        selection: &Selection,
        cx: &mut Context<Self>,
    ) {
        let format = Format {
            encoding,
            bom: bom && encoding.is_unicode(),
            ..self.doc.format()
        };
        if format == self.doc.format() {
            return;
        }
        let transaction = Transaction::set_format(self.doc.text().len(), format);
        self.apply(transaction, selection, UndoGrouping::NewStep, None, cx);
    }

    pub(crate) fn undo(
        &mut self,
        origin: Option<EntityId>,
        cx: &mut Context<Self>,
    ) -> Option<Transaction> {
        let transaction = self.doc.undo()?;
        self.after_history_step(transaction.clone(), origin, cx);
        Some(transaction)
    }

    pub(crate) fn redo(
        &mut self,
        origin: Option<EntityId>,
        cx: &mut Context<Self>,
    ) -> Option<Transaction> {
        let transaction = self.doc.redo()?;
        self.after_history_step(transaction.clone(), origin, cx);
        Some(transaction)
    }

    fn after_history_step(
        &mut self,
        transaction: Transaction,
        origin: Option<EntityId>,
        cx: &mut Context<Self>,
    ) {
        cx.emit(BufferEvent::Edited {
            transaction,
            origin,
        });
        cx.emit(BufferEvent::StateChanged);
        cx.notify();
    }
}
