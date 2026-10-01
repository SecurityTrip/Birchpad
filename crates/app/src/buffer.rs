//! A buffer: one open document (a file or an untitled "new N" tab) shared by the views showing it.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use birchpad_core::{
    ChangeSet, Document, Encoding, Format, LineMarkers, RangeSet, RevisionId, Rope, Selection,
    Transaction, UndoGrouping,
};
use birchpad_io::{DecodeProblem, LoadOptions, LoadedFile, ReadError};
use birchpad_syntax::{Language, Syntax, Tree};
use gpui_kit::{AppContext as _, Context, EntityId, EventEmitter, Task};

use crate::app_state::AppState;

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
    /// Bookmarks or token styles changed without an edit.
    MarksChanged,
    /// The language changed or a new syntax tree arrived: highlights need redrawing.
    SyntaxChanged,
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
    /// Opened read-only on purpose (`-ro` on the command line).
    Requested,
}

/// Number of styles of Search > Style All Occurrences of Token, as in Notepad++.
pub(crate) const MARK_STYLES: usize = 5;

/// Decorations that belong to the document, so every view of it shows them: bookmarks and
/// the token styles. They follow every edit, undo and redo.
#[derive(Debug, Clone, Default)]
pub(crate) struct DocumentMarks {
    pub(crate) bookmarks: LineMarkers,
    pub(crate) styles: [RangeSet<()>; MARK_STYLES],
}

impl DocumentMarks {
    fn map(&mut self, transaction: &Transaction, text: &Rope) {
        let changes = transaction.changes();
        self.bookmarks.map(changes, text);
        for style in &mut self.styles {
            style.map(changes);
        }
    }
}

/// The buffer's language and syntax tree, and the background parse that keeps the tree current.
#[derive(Default)]
struct SyntaxState {
    language: Option<&'static Language>,
    /// Chosen in the Language menu or with `-l`: kept when the file gets another name.
    chosen: bool,
    /// `None` for plain text, and for files over the large file limit.
    syntax: Option<Syntax>,
    parsing: Option<Parsing>,
}

/// A parse running in the background.
struct Parsing {
    /// The text it parses.
    text: Rope,
    /// Edits made since it started, composed into one, to replay on its tree.
    since: Option<ChangeSet>,
    cancel: Arc<AtomicBool>,
    task: Option<Task<()>>,
}

impl Drop for Parsing {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
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
    requested_read_only: bool,
    loading: Option<Loading>,
    marks: DocumentMarks,
    syntax: SyntaxState,
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
            requested_read_only: false,
            loading: None,
            marks: DocumentMarks::default(),
            syntax: SyntaxState::default(),
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
    pub(crate) fn open(
        path: PathBuf,
        options: LoadOptions,
        read_only: bool,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut buffer = Self {
            doc: Document::new(),
            path: Some(path),
            untitled: None,
            problem: None,
            file_read_only: false,
            requested_read_only: read_only,
            loading: None,
            marks: DocumentMarks::default(),
            syntax: SyntaxState::default(),
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
                self.marks = DocumentMarks::default();
                self.detect_language(cx);
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
                self.marks = DocumentMarks::default();
                self.detect_language(cx);
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

    pub(crate) fn marks(&self) -> &DocumentMarks {
        &self.marks
    }

    /// Changes the document's decorations (bookmarks, token styles) and redraws its views.
    pub(crate) fn update_marks(
        &mut self,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut DocumentMarks, &Rope),
    ) {
        change(&mut self.marks, self.doc.text());
        cx.emit(BufferEvent::MarksChanged);
        cx.notify();
    }

    pub(crate) fn language(&self) -> Option<&'static Language> {
        self.syntax.language
    }

    /// The syntax tree, unless the buffer is plain text or over the large file limit.
    pub(crate) fn syntax(&self) -> Option<&Syntax> {
        self.syntax.syntax.as_ref()
    }

    /// Sets the language from the Language menu or the command line (`None` for normal text).
    /// A chosen language is kept when the file is saved under another name, and it applies
    /// even over the large file limit.
    pub(crate) fn set_language(
        &mut self,
        language: Option<&'static Language>,
        cx: &mut Context<Self>,
    ) {
        self.syntax.chosen = true;
        self.install_language(language, cx);
    }

    /// Recognizes the language from the file name and the first line, unless one was chosen.
    fn detect_language(&mut self, cx: &mut Context<Self>) {
        if self.syntax.chosen {
            self.install_language(self.syntax.language, cx);
            return;
        }
        let text = self.doc.text();
        let first_line = text
            .lines(birchpad_core::LINE_TYPE)
            .next()
            .map(|line| {
                let end = line.floor_char_boundary(line.len().min(512));
                line.slice(..end).to_string()
            })
            .unwrap_or_default();
        let language = birchpad_syntax::detect(self.path.as_deref(), &first_line);
        self.install_language(language, cx);
    }

    fn install_language(&mut self, language: Option<&'static Language>, cx: &mut Context<Self>) {
        self.syntax.parsing = None;
        let limit = u64::from(AppState::global(cx).settings.files.large_file_limit_mb) << 20;
        let too_large = self.doc.text().len() as u64 > limit && !self.syntax.chosen;
        let language = language.filter(|_| !too_large);
        self.syntax.language = language;
        self.syntax.syntax =
            language.and_then(|language| match birchpad_syntax::config(language) {
                Ok(config) => Some(Syntax::new(config)),
                Err(error) => {
                    eprintln!("syntax: {error}");
                    None
                }
            });
        self.start_parse(cx);
        cx.emit(BufferEvent::SyntaxChanged);
        cx.emit(BufferEvent::StateChanged);
        cx.notify();
    }

    /// Starts parsing the current text in the background, unless a parse is running.
    fn start_parse(&mut self, cx: &mut Context<Self>) {
        let Some(syntax) = &self.syntax.syntax else {
            return;
        };
        if self.syntax.parsing.is_some() || self.loading.is_some() {
            return;
        }
        let text = self.doc.text().clone();
        let job = syntax.parse_job(text.clone());
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let started = Instant::now();
        let task = cx.spawn(async move |this, cx| {
            let tree = cx.background_spawn(async move { job.run(&flag) }).await;
            if std::env::var_os("BIRCHPAD_TIMINGS").is_some() {
                eprintln!("timing: parsed in {:?}", started.elapsed());
            }
            this.update(cx, |buffer, cx| buffer.finish_parse(tree, cx))
                .ok();
        });
        self.syntax.parsing = Some(Parsing {
            text,
            since: None,
            cancel,
            task: Some(task),
        });
    }

    fn finish_parse(&mut self, tree: Option<Tree>, cx: &mut Context<Self>) {
        let Some(mut parsing) = self.syntax.parsing.take() else {
            return;
        };
        // This runs inside the parse task: let it finish instead of cancelling it from within.
        if let Some(task) = parsing.task.take() {
            task.detach();
        }
        let (Some(tree), Some(syntax)) = (tree, &mut self.syntax.syntax) else {
            return;
        };
        syntax.install(tree);
        if let Some(since) = parsing.since.take() {
            // The text changed while parsing: bring the new tree up to date and parse again.
            syntax.edit(&parsing.text, &since);
            self.start_parse(cx);
        }
        cx.emit(BufferEvent::SyntaxChanged);
        cx.notify();
    }

    /// Keeps decorations and the syntax tree in step with an edit of `old_text`.
    fn follow_edit(&mut self, old_text: &Rope, transaction: &Transaction, cx: &mut Context<Self>) {
        self.marks.map(transaction, self.doc.text());
        let changes = transaction.changes();
        if changes.is_identity() {
            return;
        }
        if let Some(syntax) = &mut self.syntax.syntax {
            syntax.edit(old_text, changes);
            match &mut self.syntax.parsing {
                Some(parsing) => {
                    parsing.since = Some(match parsing.since.take() {
                        Some(earlier) => earlier.compose(changes.clone()),
                        None => changes.clone(),
                    });
                }
                None => self.start_parse(cx),
            }
        }
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
            // Saved under another name: the extension may say another language.
            self.detect_language(cx);
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
        } else if self.requested_read_only {
            Some(ReadOnly::Requested)
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
        let old_text = self.doc.text().clone();
        self.doc.apply(&transaction, selection_before, grouping);
        self.follow_edit(&old_text, &transaction, cx);
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
        let old_text = self.doc.text().clone();
        let transaction = self.doc.undo()?;
        self.after_history_step(&old_text, transaction.clone(), origin, cx);
        Some(transaction)
    }

    pub(crate) fn redo(
        &mut self,
        origin: Option<EntityId>,
        cx: &mut Context<Self>,
    ) -> Option<Transaction> {
        let old_text = self.doc.text().clone();
        let transaction = self.doc.redo()?;
        self.after_history_step(&old_text, transaction.clone(), origin, cx);
        Some(transaction)
    }

    fn after_history_step(
        &mut self,
        old_text: &Rope,
        transaction: Transaction,
        origin: Option<EntityId>,
        cx: &mut Context<Self>,
    ) {
        self.follow_edit(old_text, &transaction, cx);
        cx.emit(BufferEvent::Edited {
            transaction,
            origin,
        });
        cx.emit(BufferEvent::StateChanged);
        cx.notify();
    }
}
