//! A buffer's language: choosing it, the syntax tree parsed in the background, folds.

use std::ops::Range as ByteRange;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use birchpad_syntax::{Language, Parsed, Syntax, UdlKind, UserLanguage};
use gpui_kit::{AppContext as _, Context};

use birchpad_ui::AppState;

use super::{Buffer, BufferEvent, Parsing, UdlCache};

/// Plain text up to this size folds by indentation; computing that takes a pass over the text
/// on every edit.
pub(super) const PLAIN_FOLD_LIMIT: usize = 1 << 20;

impl Buffer {
    pub fn language(&self) -> Option<&'static Language> {
        self.syntax.language
    }

    /// The user defined language, if the buffer has one.
    pub fn user_language(&self) -> Option<&std::rc::Rc<UserLanguage>> {
        self.syntax.user_language.as_ref()
    }

    /// The pieces the user defined language colors up to `end` at least, lexed from the start
    /// of the text (comments may begin far above) and kept until the text changes.
    pub fn udl_pieces(&self, end: usize) -> std::rc::Rc<Vec<(ByteRange<usize>, UdlKind)>> {
        let Some(language) = &self.syntax.user_language else {
            return std::rc::Rc::default();
        };
        let revision = self.doc.revision();
        let text = self.doc.text();
        let end = end.min(text.len());
        if let Some(cache) = &*self.syntax.udl_cache.borrow()
            && cache.revision == revision
            && cache.end >= end
        {
            return cache.pieces.clone();
        }
        // Lex a little past what is asked, so that scrolling down reuses it.
        let end = text.ceil_char_boundary((end + 64 * 1024).min(text.len()));
        let pieces = std::rc::Rc::new(language.highlight(&text.slice(..end).to_string(), 0));
        *self.syntax.udl_cache.borrow_mut() = Some(UdlCache {
            revision,
            end,
            pieces: pieces.clone(),
        });
        pieces
    }

    /// Uses a user defined language (`None`: normal text), as chosen in the Language menu.
    pub fn set_user_language(
        &mut self,
        language: Option<std::rc::Rc<UserLanguage>>,
        cx: &mut Context<Self>,
    ) {
        self.syntax.chosen = true;
        self.install_language(None, cx);
        self.syntax.user_language = language;
        cx.emit(BufferEvent::SyntaxChanged);
        cx.emit(BufferEvent::StateChanged);
        cx.notify();
    }

    /// The syntax tree, unless the buffer is plain text or over the large file limit.
    pub fn syntax(&self) -> Option<&Syntax> {
        self.syntax.syntax.as_ref()
    }

    /// The language at `pos`: one embedded in the document's (a script in HTML), or the
    /// document's.
    pub fn language_at(&self, pos: usize) -> Option<&'static Language> {
        self.syntax()
            .map(|syntax| syntax.language_at(pos))
            .or_else(|| self.language())
    }

    /// Fold points as byte ranges sorted by start, their version, and the version they were
    /// mapped from if the last change was an edit mapping them.
    pub fn folds(&self) -> (&[ByteRange<usize>], u64, Option<u64>) {
        (
            &self.syntax.folds,
            self.syntax.folds_version,
            self.syntax.folds_mapped_from,
        )
    }

    pub(super) fn set_folds(&mut self, folds: Vec<ByteRange<usize>>) {
        self.syntax.folds = folds;
        self.syntax.folds_version += 1;
        self.syntax.folds_mapped_from = None;
    }

    /// Folds of a document without a tree that is small enough: a user defined language's, or
    /// by indentation; none otherwise.
    pub(super) fn plain_folds(&self, cx: &Context<Self>) -> Vec<ByteRange<usize>> {
        let text = self.doc.text();
        if self.syntax.syntax.is_some() || text.len() > PLAIN_FOLD_LIMIT {
            return Vec::new();
        }
        // A user defined language folds at its own words, as Notepad++'s UDL does.
        if let Some(language) = &self.syntax.user_language {
            return language.folds(&text.to_string());
        }
        let tab_width = usize::from(AppState::global(cx).settings.editor.tab_width);
        birchpad_syntax::indent_fold_ranges(text, tab_width)
    }

    /// Sets the language from the Language menu or the command line (`None` for normal text).
    /// A chosen language is kept when the file is saved under another name, and it applies
    /// even over the large file limit.
    pub fn set_language(&mut self, language: Option<&'static Language>, cx: &mut Context<Self>) {
        self.syntax.chosen = true;
        self.install_language(language, cx);
    }

    /// Recognizes the language from the file name and the first line, unless one was chosen.
    pub(super) fn detect_language(&mut self, cx: &mut Context<Self>) {
        if self.syntax.chosen {
            let user_language = self.syntax.user_language.clone();
            self.install_language(self.syntax.language, cx);
            self.syntax.user_language = user_language;
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
        let limit = u64::from(AppState::global(cx).settings.files.large_file_limit_mb) << 20;
        if language.is_none()
            && self.doc.text().len() as u64 <= limit
            && let Some(name) = self
                .path
                .as_deref()
                .and_then(|path| path.file_name()?.to_str())
        {
            self.syntax.user_language =
                (crate::EditorHooks::get(cx).user_language_for_file)(name, cx);
        }
    }

    /// Whether the document is over Preferences > Performance's large file limit.
    pub fn is_large(&self, cx: &gpui_kit::App) -> bool {
        let limit = u64::from(AppState::global(cx).settings.files.large_file_limit_mb) << 20;
        self.doc.text().len() as u64 > limit
    }

    pub(super) fn install_language(
        &mut self,
        language: Option<&'static Language>,
        cx: &mut Context<Self>,
    ) {
        self.syntax.parsing = None;
        self.syntax.user_language = None;
        *self.syntax.udl_cache.borrow_mut() = None;
        let limit = u64::from(AppState::global(cx).settings.files.large_file_limit_mb) << 20;
        let too_large = self.doc.text().len() as u64 > limit && !self.syntax.chosen;
        let language = language.filter(|_| !too_large);
        self.syntax.language = language;
        self.syntax.syntax =
            language.and_then(|language| match birchpad_syntax::config(language) {
                Ok(config) => {
                    let mut syntax = Syntax::new(config);
                    syntax.set_backslash_escapes(backslash_escapes(cx));
                    Some(syntax)
                }
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

    /// Follows the settings the parse reads the text by (Preferences > Language > SQL's
    /// backslash): a change parses the whole text again.
    pub fn sync_syntax_options(&mut self, cx: &mut Context<Self>) {
        let escapes = backslash_escapes(cx);
        let Some(syntax) = &mut self.syntax.syntax else {
            return;
        };
        if syntax.set_backslash_escapes(escapes) {
            // A parse running reads the text the old way.
            self.syntax.parsing = None;
            self.start_parse(cx);
            cx.emit(BufferEvent::SyntaxChanged);
            cx.notify();
        }
    }

    /// Starts parsing the current text in the background, unless a parse is running.
    pub(super) fn start_parse(&mut self, cx: &mut Context<Self>) {
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
                    let parsed = job.run(&flag)?;
                    let folds = birchpad_syntax::fold_ranges(parsed.tree(), &text);
                    Some((parsed, folds))
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

    pub(super) fn finish_parse(
        &mut self,
        parsed: Option<(Parsed, Vec<ByteRange<usize>>)>,
        cx: &mut Context<Self>,
    ) {
        let Some(mut parsing) = self.syntax.parsing.take() else {
            return;
        };
        // This runs inside the parse task: let it finish instead of cancelling it from within.
        if let Some(task) = parsing.task.take() {
            task.detach();
        }
        let (Some((parsed, mut folds)), Some(syntax)) = (parsed, &mut self.syntax.syntax) else {
            return;
        };
        syntax.install(parsed);
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
}

/// Whether SQL's strings take backslash escapes (Preferences > Language).
pub(super) fn backslash_escapes(cx: &gpui_kit::App) -> bool {
    AppState::global(cx).settings.sql.backslash_escapes
}
