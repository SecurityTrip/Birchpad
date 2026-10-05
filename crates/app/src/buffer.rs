//! A buffer: one open document (a file or an untitled "new N" tab) shared by the views showing it.

use std::ops::Range as ByteRange;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use birchpad_core::{
    ChangeSet, Document, Edit, Encoding, Format, LineMarkers, RangeSet, RevisionId, Rope,
    Selection, Transaction, UndoGrouping,
};
use birchpad_io::{
    ByteChanges, DecodeProblem, DiskStamp, Head, LoadOptions, LoadedFile, ReadError,
};
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
    /// The whole document was replaced: loading finished, or the file was read again (it
    /// changed on disk, or in another encoding). `previous` is the text before, if there was
    /// one: views then keep their places on the same lines.
    Reloaded { previous: Option<Rope> },
    /// Views go to the end of the text (tail -f, "scroll to the last line after update").
    FollowEnd,
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
    /// View > Monitoring (tail -f): the document follows the file.
    Monitoring,
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
    /// Bookmarks the line where each of `ranges` starts. This is the "Bookmark line" option of
    /// Notepad++'s Mark dialog, which comes with the Find dialog in phase 3 and calls this
    /// with the matches of Mark All (through `Buffer::update_marks`).
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "the Mark dialog comes in phase 3")
    )]
    pub(crate) fn bookmark_lines_of(&mut self, text: &Rope, ranges: &[ByteRange<usize>]) {
        let mut lines = self.bookmarks.lines(text);
        lines.extend(
            ranges
                .iter()
                .map(|range| birchpad_core::motion::line_of(text, range.start)),
        );
        self.bookmarks.set_lines(text, lines);
    }

    fn map(&mut self, transaction: &Transaction, text: &Rope) {
        let changes = transaction.changes();
        self.bookmarks.map(changes, text);
        for style in &mut self.styles {
            style.map(changes);
        }
    }
}

/// Plain text up to this size folds by indentation; computing that takes a pass over the text
/// on every edit.
const PLAIN_FOLD_LIMIT: usize = 1 << 20;

/// The buffer's language and syntax tree, and the background parse that keeps the tree current.
#[derive(Default)]
struct SyntaxState {
    language: Option<&'static Language>,
    /// Chosen in the Language menu or with `-l`: kept when the file gets another name.
    chosen: bool,
    /// `None` for plain text, and for files over the large file limit.
    syntax: Option<Syntax>,
    parsing: Option<Parsing>,
    /// Fold points as byte ranges sorted by start: from the tree, or from indentation for
    /// plain text. Mapped through every edit until the next parse replaces them.
    folds: Vec<ByteRange<usize>>,
    /// Bumped whenever `folds` changes, so views can cache the folds in lines.
    folds_version: u64,
    /// The version the folds were mapped from by the last edit, if that is how they changed:
    /// views can then shift their folds in lines instead of recomputing them.
    folds_mapped_from: Option<u64>,
    /// A language was installed and its first parse has not finished: there are no folds yet,
    /// but there will be.
    folds_pending: bool,
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

/// The backup copy of a buffer's unsaved text (sessions, ADR 0017).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Backup {
    /// The file name in the backup folder.
    pub(crate) name: String,
    /// The revision the file holds; `None` until it is first written.
    pub(crate) revision: Option<RevisionId>,
}

pub(crate) struct Buffer {
    doc: Document,
    path: Option<PathBuf>,
    /// The number of an untitled buffer: 1 for "new 1".
    untitled: Option<usize>,
    problem: Option<DecodeProblem>,
    /// With a problem: where saving the text would change the file.
    decode_changes: ByteChanges,
    file_read_only: bool,
    requested_read_only: bool,
    /// The encoding the file was read in on request (Encoding > Encode in), if any.
    chosen_encoding: Option<Encoding>,
    loading: Option<Loading>,
    marks: DocumentMarks,
    /// Bookmarks to set once the file has been read (a restored session).
    pending_bookmarks: Option<Vec<usize>>,
    pub(crate) backup: Option<Backup>,
    /// The file as last read or written; `None` while it does not exist.
    disk: Option<DiskStamp>,
    /// A fingerprint of the file's first bytes, to recognize appends (tail -f).
    head: Option<Head>,
    /// View > Monitoring (tail -f).
    monitoring: bool,
    /// Saves running: what they write is not a change made by another program.
    saving: usize,
    /// Views go to the end once the next load finishes.
    follow_end: bool,
    /// The document holds text read from the file: reading it again keeps the views' places.
    loaded: bool,
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
            decode_changes: ByteChanges::default(),
            file_read_only: false,
            requested_read_only: false,
            chosen_encoding: None,
            loading: None,
            marks: DocumentMarks::default(),
            pending_bookmarks: None,
            backup: None,
            disk: None,
            head: None,
            monitoring: false,
            saving: 0,
            follow_end: false,
            loaded: false,
            syntax: SyntaxState::default(),
        }
    }

    /// A buffer put back from a session with the text of a backup copy: unsaved changes of
    /// `path`, or an untitled document.
    pub(crate) fn restored(
        path: Option<PathBuf>,
        untitled: Option<usize>,
        mut doc: Document,
        modified: bool,
        read_only: bool,
        cx: &mut Context<Self>,
    ) -> Self {
        if modified {
            doc.mark_unsaved();
        }
        let mut buffer = Self {
            doc,
            path,
            untitled: untitled.or(Some(1)),
            requested_read_only: read_only,
            ..Self::untitled(1)
        };
        if buffer.path.is_some() {
            buffer.untitled = None;
        }
        buffer.detect_language(cx);
        buffer
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
            decode_changes: ByteChanges::default(),
            file_read_only: false,
            requested_read_only: read_only,
            chosen_encoding: None,
            loading: None,
            marks: DocumentMarks::default(),
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
    pub(crate) fn load(&mut self, options: LoadOptions, bom: Option<bool>, cx: &mut Context<Self>) {
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
        // Read again: bookmarks stay on their lines, views keep their places.
        let previous = self.loaded.then(|| self.doc.text().clone());
        let bookmarks = previous
            .as_ref()
            .map(|text| self.marks.bookmarks.lines(text));
        match result {
            Ok(file) => {
                self.problem = file.problem;
                self.decode_changes = file.changes;
                self.file_read_only = file.info.read_only;
                self.disk = Some(file.info.stamp());
                self.head = Some(file.head);
                self.doc = Document::with_format(file.text, file.format);
                self.marks = DocumentMarks::default();
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

    pub(crate) fn doc(&self) -> &Document {
        &self.doc
    }

    /// Bookmarks `lines` now, or once the file has been read.
    pub(crate) fn restore_bookmarks(&mut self, lines: Vec<usize>, cx: &mut Context<Self>) {
        self.pending_bookmarks = Some(lines);
        if self.loading.is_none() {
            self.set_pending_bookmarks();
            cx.emit(BufferEvent::MarksChanged);
        }
    }

    fn set_pending_bookmarks(&mut self) {
        if let Some(lines) = self.pending_bookmarks.take() {
            self.marks.bookmarks.set_lines(self.doc.text(), lines);
        }
    }

    /// Bookmarked lines, including those waiting for the file to be read.
    pub(crate) fn bookmark_lines(&self) -> Vec<usize> {
        match &self.pending_bookmarks {
            Some(lines) => lines.clone(),
            None => self.marks.bookmarks.lines(self.doc.text()),
        }
    }

    /// The language chosen in the Language menu or with `-l`, if any.
    pub(crate) fn chosen_language(&self) -> Option<Option<&'static Language>> {
        self.syntax.chosen.then_some(self.syntax.language)
    }

    pub(crate) fn chosen_encoding(&self) -> Option<Encoding> {
        self.chosen_encoding
    }

    /// Opened read-only on purpose (`-ro`).
    pub(crate) fn requested_read_only(&self) -> bool {
        self.requested_read_only
    }

    /// Whether the folds may still change on their own: a language's first parse is running.
    pub(crate) fn folds_pending(&self) -> bool {
        self.syntax.folds_pending
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

    /// Fold points as byte ranges sorted by start, their version, and the version they were
    /// mapped from if the last change was an edit mapping them.
    pub(crate) fn folds(&self) -> (&[ByteRange<usize>], u64, Option<u64>) {
        (
            &self.syntax.folds,
            self.syntax.folds_version,
            self.syntax.folds_mapped_from,
        )
    }

    fn set_folds(&mut self, folds: Vec<ByteRange<usize>>) {
        self.syntax.folds = folds;
        self.syntax.folds_version += 1;
        self.syntax.folds_mapped_from = None;
    }

    /// Indentation folds for plain text that is small enough; none otherwise.
    fn plain_folds(&self, cx: &Context<Self>) -> Vec<ByteRange<usize>> {
        let text = self.doc.text();
        if self.syntax.syntax.is_some() || text.len() > PLAIN_FOLD_LIMIT {
            return Vec::new();
        }
        let tab_width = usize::from(AppState::global(cx).settings.editor.tab_width);
        birchpad_syntax::indent_fold_ranges(text, tab_width)
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
        // Until the first parse, a language has no folds; plain text folds by indentation.
        let folds = self.plain_folds(cx);
        self.set_folds(folds);
        self.syntax.folds_pending = self.syntax.syntax.is_some();
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
            // Folds come from the new tree, computed on the same background thread.
            let parsed = cx
                .background_spawn(async move {
                    let text = job.text().clone();
                    let tree = job.run(&flag)?;
                    let folds = birchpad_syntax::fold_ranges(&tree, &text);
                    Some((tree, folds))
                })
                .await;
            if std::env::var_os("BIRCHPAD_TIMINGS").is_some() {
                eprintln!("timing: parsed in {:?}", started.elapsed());
            }
            this.update(cx, |buffer, cx| buffer.finish_parse(parsed, cx))
                .ok();
        });
        self.syntax.parsing = Some(Parsing {
            text,
            since: None,
            cancel,
            task: Some(task),
        });
    }

    fn finish_parse(
        &mut self,
        parsed: Option<(Tree, Vec<ByteRange<usize>>)>,
        cx: &mut Context<Self>,
    ) {
        let Some(mut parsing) = self.syntax.parsing.take() else {
            return;
        };
        // This runs inside the parse task: let it finish instead of cancelling it from within.
        if let Some(task) = parsing.task.take() {
            task.detach();
        }
        let (Some((tree, mut folds)), Some(syntax)) = (parsed, &mut self.syntax.syntax) else {
            return;
        };
        syntax.install(tree);
        self.syntax.folds_pending = false;
        let since = parsing.since.take();
        if let Some(since) = &since {
            // The text changed while parsing: bring the new tree and folds up to date, and
            // parse again.
            syntax.edit(&parsing.text, since);
            birchpad_core::map_ranges(&mut folds, since);
        }
        self.set_folds(folds);
        if since.is_some() {
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
        if self.syntax.syntax.is_some() {
            let mut folds = std::mem::take(&mut self.syntax.folds);
            birchpad_core::map_ranges(&mut folds, changes);
            let previous = self.syntax.folds_version;
            self.set_folds(folds);
            self.syntax.folds_mapped_from = Some(previous);
        } else {
            let folds = self.plain_folds(cx);
            self.set_folds(folds);
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

    /// A save starts: until it ends, changes of the file are its own.
    pub(crate) fn begin_save(&mut self) {
        self.saving += 1;
    }

    /// A save failed.
    pub(crate) fn end_save(&mut self) {
        self.saving = self.saving.saturating_sub(1);
    }

    pub(crate) fn is_saving(&self) -> bool {
        self.saving > 0
    }

    /// The file as last read or written; `None` while it does not exist.
    pub(crate) fn disk(&self) -> Option<DiskStamp> {
        self.disk
    }

    pub(crate) fn head(&self) -> Option<Head> {
        self.head
    }

    /// For text restored from a backup copy: how the file looked when the copy was made.
    pub(crate) fn assume_disk(&mut self, disk: Option<DiskStamp>) {
        self.disk = disk;
    }

    pub(crate) fn is_monitoring(&self) -> bool {
        self.monitoring
    }

    /// View > Monitoring (tail -f): the document becomes read-only and follows the file.
    pub(crate) fn set_monitoring(&mut self, monitoring: bool, cx: &mut Context<Self>) {
        self.monitoring = monitoring;
        if monitoring {
            cx.emit(BufferEvent::FollowEnd);
        }
        cx.emit(BufferEvent::StateChanged);
        cx.notify();
    }

    /// Reads the file again because another program changed it. Views keep their places;
    /// with `follow_end`, they go to the end.
    pub(crate) fn reload(&mut self, follow_end: bool, cx: &mut Context<Self>) {
        self.follow_end = follow_end;
        let options = AppState::global(cx).load_options(self.chosen_encoding);
        self.load(options, None, cx);
    }

    /// Adds text that was appended to the file (tail -f). It is not an edit: the document
    /// stays unmodified and has nothing to undo.
    pub(crate) fn append_from_disk(
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
    pub(crate) fn keep_deleted(&mut self, cx: &mut Context<Self>) {
        self.disk = None;
        self.head = None;
        self.doc.mark_unsaved();
        cx.emit(BufferEvent::StateChanged);
        cx.notify();
    }

    /// The file changed and the user keeps the document as it is: it differs from the file
    /// now, and that change is not asked about again.
    pub(crate) fn keep_changed(&mut self, stamp: Option<DiskStamp>, cx: &mut Context<Self>) {
        self.disk = stamp;
        self.head = None;
        if !self.doc.is_modified() {
            self.doc.mark_unsaved();
        }
        cx.emit(BufferEvent::StateChanged);
        cx.notify();
    }

    /// Records that `revision` of the document was written to `path` (a new path after Save As),
    /// which then looked like `disk`.
    pub(crate) fn did_save(
        &mut self,
        path: PathBuf,
        revision: RevisionId,
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

    /// Where saving would change a file that did not decode exactly.
    pub(crate) fn decode_changes(&self) -> &ByteChanges {
        &self.decode_changes
    }

    /// Makes a document that did not decode exactly editable: the user accepted that saving
    /// changes the bytes in [`Self::decode_changes`].
    pub(crate) fn edit_anyway(&mut self, cx: &mut Context<Self>) {
        if self.problem.take().is_some() {
            cx.emit(BufferEvent::StateChanged);
            cx.notify();
        }
    }

    pub(crate) fn read_only(&self) -> Option<ReadOnly> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mark_all_can_bookmark_the_lines_of_its_matches() {
        let text = Rope::from_str("a\nb b\nc\nb");
        let mut marks = DocumentMarks::default();
        marks.bookmarks.add(&text, 0);
        marks.bookmark_lines_of(&text, &[2..3, 4..5, 8..9]);
        assert_eq!(marks.bookmarks.lines(&text), [0, 1, 3]);
    }
}
