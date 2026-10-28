//! A buffer: one open document (a file or an untitled "new N" tab) shared by the views showing it.

mod file;
mod marks;
mod syntax;

pub use marks::{DocumentMarks, MARK_STYLES};

use std::ops::Range as ByteRange;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use birchpad_core::{
    ChangeHistory, ChangeSet, Document, Encoding, Format, LineMarkers, RevisionId, Rope, Selection,
    Transaction, UndoGrouping,
};
use birchpad_io::{ByteChanges, DecodeProblem, DiskStamp, Head, ReadError};
use birchpad_syntax::{Language, Syntax, UdlKind, UserLanguage};
use gpui_kit::{Context, EntityId, EventEmitter, Task};

/// What changed in a buffer, so that every view showing it can follow.
#[derive(Debug, Clone)]
pub enum BufferEvent {
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
    /// Bookmarks, token styles or the changed lines changed without an edit.
    MarksChanged,
    /// The language changed or a new syntax tree arrived: highlights need redrawing.
    SyntaxChanged,
    /// The file could not be read; the buffer stays empty.
    LoadFailed(Arc<ReadError>),
}

/// Why a buffer cannot be edited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadOnly {
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
    /// A Notepad++ user defined language, in place of `language`.
    user_language: Option<std::rc::Rc<UserLanguage>>,
    /// Its pieces of text up to an offset, for a revision of the text.
    udl_cache: std::cell::RefCell<Option<UdlCache>>,
}

/// What a user defined language colored, up to `end`, in revision `revision` of the text.
struct UdlCache {
    revision: birchpad_core::RevisionId,
    end: usize,
    pieces: std::rc::Rc<Vec<(ByteRange<usize>, UdlKind)>>,
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

/// The change history (Notepad++'s orange and green lines, ADR 0023): the texts to compare
/// with, and the changed lines as of the last comparison, which follow edits until the next.
struct ChangeTracking {
    history: ChangeHistory,
    modified: LineMarkers,
    saved: LineMarkers,
    /// Bumped when `history` changes (a save, text appended to the file), so that a
    /// comparison started before keeps its result to itself.
    generation: u64,
    comparing: Option<Comparing>,
}

impl ChangeTracking {
    /// Starts with `text`: no line has changed.
    fn new(text: Rope) -> Self {
        Self {
            history: ChangeHistory::new(text),
            modified: LineMarkers::new(),
            saved: LineMarkers::new(),
            generation: 0,
            comparing: None,
        }
    }
}

/// A comparison running in the background.
struct Comparing {
    /// Edits made since it started, composed into one, to move its result with.
    since: Option<ChangeSet>,
    /// The text or the history changed since it started: compare again when it ends.
    again: bool,
    task: Task<()>,
}

/// The backup copy of a buffer's unsaved text (sessions, ADR 0017).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Backup {
    /// The file name in the backup folder.
    pub name: String,
    /// The revision the file holds; `None` until it is first written.
    pub revision: Option<RevisionId>,
}

pub struct Buffer {
    doc: Document,
    path: Option<PathBuf>,
    /// The number of an untitled buffer: 1 for "new 1".
    untitled: Option<usize>,
    /// The name an untitled buffer was given (File > Rename), shown instead of "new N".
    title: Option<String>,
    problem: Option<DecodeProblem>,
    /// With a problem: where saving the text would change the file.
    decode_changes: ByteChanges,
    file_read_only: bool,
    requested_read_only: bool,
    /// The encoding the file was read in on request (Encoding > Encode in), if any.
    chosen_encoding: Option<Encoding>,
    loading: Option<Loading>,
    marks: DocumentMarks,
    changes: ChangeTracking,
    /// Bookmarks to set once the file has been read (a restored session).
    pending_bookmarks: Option<Vec<usize>>,
    pub backup: Option<Backup>,
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
    pub fn untitled(number: usize) -> Self {
        Self {
            doc: Document::new(),
            path: None,
            untitled: Some(number),
            title: None,
            problem: None,
            decode_changes: ByteChanges::default(),
            file_read_only: false,
            requested_read_only: false,
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
        }
    }

    /// A buffer put back from a session with the text of a backup copy: unsaved changes of
    /// `path`, or an untitled document.
    pub fn restored(
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
        // The change history starts with the text put back.
        buffer.changes = ChangeTracking::new(buffer.doc.text().clone());
        buffer.detect_language(cx);
        buffer
    }

    /// An untitled buffer with existing text.
    pub fn untitled_with(number: usize, doc: Document) -> Self {
        Self {
            changes: ChangeTracking::new(doc.text().clone()),
            doc,
            ..Self::untitled(number)
        }
    }

    pub fn doc(&self) -> &Document {
        &self.doc
    }

    /// The language chosen in the Language menu or with `-l`, if any.
    pub fn chosen_language(&self) -> Option<Option<&'static Language>> {
        self.syntax.chosen.then_some(self.syntax.language)
    }

    pub fn chosen_encoding(&self) -> Option<Encoding> {
        self.chosen_encoding
    }

    /// Whether the folds may still change on their own: a language's first parse is running.
    pub fn folds_pending(&self) -> bool {
        self.syntax.folds_pending
    }

    /// Keeps decorations and the syntax tree in step with an edit of `old_text`.
    fn follow_edit(&mut self, old_text: &Rope, transaction: &Transaction, cx: &mut Context<Self>) {
        self.marks.map(transaction, self.doc.text());
        let changes = transaction.changes();
        if changes.is_identity() {
            return;
        }
        // The changed lines move with the text until the comparison brings them up to date.
        self.changes.modified.map(changes, self.doc.text());
        self.changes.saved.map(changes, self.doc.text());
        if let Some(comparing) = &mut self.changes.comparing {
            comparing.since = Some(match comparing.since.take() {
                Some(earlier) => earlier.compose(changes.clone()),
                None => changes.clone(),
            });
        }
        self.compare_changes(cx);
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

    pub fn apply(
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
    pub fn set_format(
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

    pub fn undo(
        &mut self,
        origin: Option<EntityId>,
        cx: &mut Context<Self>,
    ) -> Option<Transaction> {
        let old_text = self.doc.text().clone();
        let transaction = self.doc.undo()?;
        self.after_history_step(&old_text, transaction.clone(), origin, cx);
        Some(transaction)
    }

    pub fn redo(
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
