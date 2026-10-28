//! Find, Replace, Find in Files, Mark and Go To.
//!
//! The find panel sits at the bottom of the window, above the status bar, with Notepad++'s Find,
//! Replace, Find in Files, Find in Projects and Mark tabs, search modes and options. It is a panel rather than a dialog, and like
//! Notepad++'s modeless dialog it leaves the document editable while it is open. F3 and
//! Shift+F3 repeat the last search even when the panel is closed. Searching goes through
//! `birchpad_core::search`.

mod bar;
mod go_to;
mod mark;
mod replace;
mod search;

pub use bar::{FindBar, FindBarEvent};

use birchpad_core::search::{Direction, Query, SearchMode, Searcher};
use gpui_kit::{App, Context, Entity, Window};

use crate::app_state::AppState;
use crate::commands::{CommandRegistry, Register as _};
use crate::editor::EditorView;
use crate::workspace::Workspace;

pub fn register_commands(registry: &mut CommandRegistry) {
    // Search > Copy Marked Text, as the Mark tab's button.
    registry.workspace("search.copy-marked-text", |this, (), window, cx| {
        let message = this.copy_marked(cx);
        crate::workspace::report_info(message, window, cx);
        Ok(())
    });
    registry.workspace("search.find", |this, (), window, cx| {
        this.open_find(FindTab::Find, window, cx);
        Ok(())
    });
    registry.workspace("search.replace", |this, (), window, cx| {
        this.open_find(FindTab::Replace, window, cx);
        Ok(())
    });
    registry.workspace("search.find-in-files", |this, (), window, cx| {
        this.open_find(FindTab::FindInFiles, window, cx);
        Ok(())
    });
    registry.workspace("search.find-in-projects", |this, (), window, cx| {
        this.open_find(FindTab::FindInProjects, window, cx);
        Ok(())
    });
    registry.workspace("search.mark", |this, (), window, cx| {
        this.open_find(FindTab::Mark, window, cx);
        Ok(())
    });
    registry.workspace("search.find-text", |this, args: SearchArgs, window, cx| {
        this.take_search(&args, window, cx)?;
        let direction = if args.backward {
            Direction::Backward
        } else {
            Direction::Forward
        };
        this.find(direction, window, cx);
        Ok(())
    });
    registry.workspace(
        "search.replace-text",
        |this, args: SearchArgs, window, cx| {
            this.take_search(&args, window, cx)?;
            if args.all {
                this.replace_all(window, cx);
            } else {
                this.replace(window, cx);
            }
            Ok(())
        },
    );
    registry.workspace("search.find-next", |this, (), window, cx| {
        this.find(Direction::Forward, window, cx);
        Ok(())
    });
    registry.workspace("search.find-previous", |this, (), window, cx| {
        this.find(Direction::Backward, window, cx);
        Ok(())
    });
    registry.workspace("search.select-and-find-next", |this, (), window, cx| {
        this.select_and_find(Direction::Forward, window, cx);
        Ok(())
    });
    registry.workspace("search.select-and-find-previous", |this, (), window, cx| {
        this.select_and_find(Direction::Backward, window, cx);
        Ok(())
    });
    registry.workspace("search.find-volatile-next", |this, (), _, cx| {
        this.find_volatile(Direction::Forward, cx);
        Ok(())
    });
    registry.workspace("search.find-volatile-previous", |this, (), _, cx| {
        this.find_volatile(Direction::Backward, cx);
        Ok(())
    });
    registry.workspace("search.next-result", |this, (), window, cx| {
        this.step_result(true, window, cx);
        Ok(())
    });
    registry.workspace("search.previous-result", |this, (), window, cx| {
        this.step_result(false, window, cx);
        Ok(())
    });
    registry.workspace("search.results-window", |this, (), _, cx| {
        this.search_results.update(cx, |results, cx| {
            results.visible ^= true;
            cx.notify();
        });
        cx.notify();
        Ok(())
    });
    registry.workspace("search.close", |this, (), window, cx| {
        this.close_find(window, cx);
        Ok(())
    });
    registry.workspace("search.go-to", |this, (), window, cx| {
        this.open_go_to(window, cx)
    });
}

/// A search as macros record it: the text and the options of the find panel at the time.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct SearchArgs {
    pub text: String,
    pub match_case: bool,
    pub whole_word: bool,
    /// `normal`, `extended` or `regex`.
    pub mode: String,
    pub dot_all: bool,
    pub backward: bool,
    pub wrap: bool,
    /// Replace: the replacement.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub with: Option<String>,
    /// Replace: all of them.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub all: bool,
}

impl Default for SearchArgs {
    fn default() -> Self {
        Self {
            text: String::new(),
            match_case: false,
            whole_word: false,
            mode: "normal".to_owned(),
            dot_all: false,
            backward: false,
            wrap: true,
            with: None,
            all: false,
        }
    }
}

fn mode_name(mode: SearchMode) -> &'static str {
    match mode {
        SearchMode::Normal => "normal",
        SearchMode::Extended => "extended",
        SearchMode::Regex => "regex",
    }
}

/// The tabs of the find panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindTab {
    Find,
    Replace,
    FindInFiles,
    /// The files of the project panels.
    FindInProjects,
    Mark,
}

impl FindTab {
    const ALL: [Self; 5] = [
        Self::Find,
        Self::Replace,
        Self::FindInFiles,
        Self::FindInProjects,
        Self::Mark,
    ];
}

/// The folder options of the Find in Files tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FolderSearch {
    /// In all sub-folders.
    pub subfolders: bool,
    /// In hidden folders.
    pub hidden: bool,
    /// Follow current doc.: the folder of the active document.
    pub follow_current_document: bool,
}

/// Options of the find panel, as in Notepad++'s Find dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FindOptions {
    pub match_case: bool,
    pub whole_word: bool,
    pub wrap_around: bool,
    pub backward: bool,
    /// Count, Replace All and Mark All work in the selection.
    pub in_selection: bool,
    pub mode: SearchMode,
    /// Regular expressions: `.` matches line breaks.
    pub dot_matches_newline: bool,
    /// Mark All also bookmarks the lines of its matches.
    pub bookmark_line: bool,
    /// Mark All first clears the marks of earlier searches.
    pub purge: bool,
}

impl Default for FindOptions {
    fn default() -> Self {
        Self {
            match_case: false,
            whole_word: false,
            // Notepad++'s default.
            wrap_around: true,
            backward: false,
            in_selection: false,
            mode: SearchMode::Normal,
            dot_matches_newline: false,
            bookmark_line: false,
            purge: false,
        }
    }
}

/// Where Count, Replace All and Mark All look: the selection with In selection, else the
/// whole text.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Scope {
    range: std::ops::Range<usize>,
    in_selection: bool,
}

impl Scope {
    fn describe(&self) -> String {
        if self.in_selection {
            t!("find.in-selection")
        } else {
            t!("find.in-entire-file")
        }
    }
}

pub fn plural(count: usize, one: &str, many: &str) -> String {
    crate::i18n::count(count, one, many)
}

impl Workspace {
    /// Ctrl+F, Ctrl+H, Ctrl+M: shows the panel on `tab`. A selection on one line becomes the
    /// search text; a selection over several lines turns In selection on instead, as in
    /// Notepad++.
    fn open_find(&mut self, tab: FindTab, window: &mut Window, cx: &mut Context<Self>) {
        let searching = AppState::global(cx).settings.searching.clone();
        let (text, in_selection) = self
            .active_view(cx)
            .map(|view| {
                let view = view.read(cx);
                let range = view.selection.primary();
                let text = view.text(cx).slice(range.from()..range.to()).to_string();
                find_field_fill(&text, view.text(cx), range.head, &searching)
            })
            .unwrap_or((None, None));
        self.find_bar
            .update(cx, |bar, cx| bar.show(tab, text, in_selection, window, cx));
        self.follow_current_document(window, cx);
        cx.notify();
    }

    /// Preferences > Searching > Find dialog remains open after search that outputs to
    /// results window: off, the panel closes.
    fn close_unless_staying(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !AppState::global(cx).settings.searching.stay_open {
            self.close_find(window, cx);
        }
    }

    /// Replace All in All Opened Documents, after asking if Preferences > Searching say so.
    fn confirm_replace_all_in_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !AppState::global(cx)
            .settings
            .searching
            .confirm_replace_all_documents
        {
            self.replace_all_in_all(cx);
            return;
        }
        let answers = [t!("find.replace-all"), t!("common.cancel")];
        let answers: Vec<&str> = answers.iter().map(String::as_str).collect();
        let answer = window.prompt(
            gpui_kit::PromptLevel::Warning,
            &t!("find.replace-in-every-open-document"),
            None,
            &answers,
            cx,
        );
        cx.spawn(async move |this, cx| {
            if answer.await.ok() == Some(0) {
                this.update(cx, |this, cx| this.replace_all_in_all(cx)).ok();
            }
        })
        .detach();
    }

    pub fn close_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.find_bar.update(cx, |bar, cx| {
            bar.visible = false;
            cx.notify();
        });
        if let Some(view) = self.active_view(cx) {
            let focus = view.read(cx).focus_handle.clone();
            window.focus(&focus, cx);
        }
        cx.notify();
    }

    pub fn on_find_bar_event(
        &mut self,
        _: &Entity<FindBar>,
        event: &FindBarEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            FindBarEvent::Find(direction) => self.find(*direction, window, cx),
            FindBarEvent::Count => self.count(cx),
            FindBarEvent::FindAll { all_documents } => {
                self.find_all(*all_documents, cx);
                self.close_unless_staying(window, cx);
            }
            FindBarEvent::Replace => self.replace(window, cx),
            FindBarEvent::ReplaceAll => self.replace_all(window, cx),
            FindBarEvent::ReplaceAllInAll => self.confirm_replace_all_in_all(window, cx),
            FindBarEvent::MarkAll => self.mark_all(cx),
            FindBarEvent::ClearMarks => self.clear_marks(cx),
            FindBarEvent::CopyMarked => {
                self.copy_marked(cx);
            }
            FindBarEvent::FindInFiles => {
                self.find_in_files(window, cx);
                self.close_unless_staying(window, cx);
            }
            FindBarEvent::FindInProjects => self.find_in_projects(cx),
            FindBarEvent::ReplaceInFiles => self.replace_in_files(window, cx),
            FindBarEvent::StopFiles => self.find_bar.update(cx, |bar, cx| bar.stop_files(cx)),
            FindBarEvent::BrowseFolder => self.browse_folder(window, cx),
            FindBarEvent::TabChanged => self.follow_current_document(window, cx),
            FindBarEvent::Close => self.close_find(window, cx),
        }
    }

    pub fn report(&mut self, message: Option<String>, failed: bool, cx: &mut Context<Self>) {
        self.find_bar.update(cx, |bar, cx| {
            bar.set_status(message.map(|message| (message, failed)), cx);
        });
    }

    pub fn searcher(&mut self, cx: &mut Context<Self>) -> Option<(Searcher, Query)> {
        let query = self.find_bar.read(cx).query(cx);
        match Searcher::new(&query) {
            Ok(searcher) => Some((searcher, query)),
            Err(error) => {
                let error = error.to_string();
                self.report(Some(format!("{}: {error}", t!("find.find"))), true, cx);
                None
            }
        }
    }

    /// Reports a regular expression that failed to match; true if it did.
    pub fn report_failure(&mut self, searcher: &Searcher, cx: &mut Context<Self>) -> bool {
        match searcher.failure() {
            Some(error) => {
                let error = error.to_string();
                self.report(Some(format!("{}: {error}", t!("find.find"))), true, cx);
                true
            }
            None => false,
        }
    }

    /// The range Count, Replace All and Mark All work on in `view`.
    fn scope(&self, view: &Entity<EditorView>, cx: &App) -> Scope {
        let view = view.read(cx);
        let selection = view.selection.primary();
        if self.find_bar.read(cx).options.in_selection && !selection.is_empty() {
            Scope {
                range: selection.from()..selection.to(),
                in_selection: true,
            }
        } else {
            Scope {
                range: 0..view.text(cx).len(),
                in_selection: false,
            }
        }
    }
}

/// What the find panel takes from the document when it opens, as Preferences > Searching
/// say: the selection (or, with none, the word at the caret) for the find field, and whether
/// In selection is checked. A selection of several lines never fills the field.
pub fn find_field_fill(
    selected: &str,
    text: &birchpad_core::Rope,
    caret: usize,
    searching: &birchpad_config::SearchingSettings,
) -> (Option<String>, Option<bool>) {
    let length = selected.chars().count();
    if length == 0 {
        let word = searching
            .fill_from_word
            .then(|| birchpad_core::motion::word_at(text, caret))
            .filter(|word| !word.is_empty())
            .map(|word| text.slice(word).to_string())
            .filter(|word| word.chars().any(birchpad_core::completion::is_word_char));
        return (word, Some(false));
    }
    let threshold = searching.in_selection_from as usize;
    let several_lines = selected.contains(['\n', '\r']);
    let in_selection = threshold > 0 && length >= threshold || several_lines && threshold > 0;
    let fills = !several_lines && length <= searching.fill_from_selection as usize;
    (fills.then(|| selected.to_owned()), Some(in_selection))
}
