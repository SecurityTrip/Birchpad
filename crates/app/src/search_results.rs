//! The search results panel, as Notepad++'s Search results window: Find All in the current
//! document or in all opened documents (and Find in Files) list their matches here.
//!
//! Searches stack, the newest first. Each lists its documents, and under each document the lines
//! with matches, the matches highlighted. Clicking a search or a document folds it; double-
//! clicking a line, or Enter, goes to its first match; F4 and Shift+F4 go to the next and
//! previous line from anywhere. Lines are kept as their line number and the match's offsets
//! within the line, as Notepad++ does: an edit above a match moves it away from its result.
//!
//! Delete removes the selected line, document or search from the list; the right-click menu
//! does too, and copies the selected line or path, folds and unfolds everything, clears the
//! list, and searches the lines of a search again (Find in These Search Results...), which
//! lists what it finds as a search of its own.

use std::ops::Range;
use std::path::PathBuf;

use birchpad_core::search::LineHit;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::{IconName, Sizable};
use gpui_kit::{
    App, ClickEvent, ClipboardItem, Context, EventEmitter, FocusHandle, Focusable, FontWeight,
    HighlightStyle, KeyDownEvent, ListAlignment, ListState, MouseButton, ScrollStrategy,
    SharedString, StyledText, UniformListScrollHandle, WeakEntity, Window, div, list, prelude::*,
    px, uniform_list,
};

use birchpad_config::SearchResultsState;

use crate::app_state::AppState;
use crate::buffer::Buffer;
use crate::editor::widgets::{BarColors, Line, bar};

const ROW_HEIGHT: f32 = 20.;

/// Where a document with results is.
#[derive(Clone)]
pub enum ResultLocation {
    /// An open document.
    Buffer(WeakEntity<Buffer>),
    /// A file on disk (Find in Files).
    File(PathBuf),
}

/// The results of one search in one document.
pub struct FileResults {
    pub location: ResultLocation,
    /// The path, or the name of an untitled document.
    pub name: String,
    pub lines: Vec<LineHit>,
    pub hits: usize,
    collapsed: bool,
}

impl FileResults {
    pub fn new(location: ResultLocation, name: String, lines: Vec<LineHit>, hits: usize) -> Self {
        Self {
            location,
            name,
            lines,
            hits,
            collapsed: false,
        }
    }
}

/// One search: what was searched for, and its documents.
pub struct SearchRun {
    pattern: String,
    /// How many documents were searched.
    searched: usize,
    pub files: Vec<FileResults>,
    collapsed: bool,
}

impl SearchRun {
    pub fn new(pattern: &str, files: Vec<FileResults>, searched: usize) -> Self {
        Self {
            pattern: pattern.to_owned(),
            searched,
            files,
            collapsed: false,
        }
    }

    /// Notepad++'s heading: `Search "foo" (3 hits in 2 files of 5 searched)`. Results removed
    /// from the list no longer count.
    pub fn title(&self) -> String {
        let hits: usize = self.files.iter().map(|file| file.hits).sum();
        let count = crate::i18n::count;
        t!(
            "search-results.title",
            pattern = self.pattern,
            found = t!(
                "find-in-files.found",
                hits = count(hits, "hit", "hits"),
                files = count(self.files.len(), "file", "files"),
                searched = self.searched
            )
        )
    }
}

/// The lines found, a line with several matches once for each of them.
pub fn entry_per_match(lines: Vec<LineHit>) -> Vec<LineHit> {
    let mut split = Vec::with_capacity(lines.len());
    for line in lines {
        if line.highlights.len() <= 1 {
            split.push(line);
            continue;
        }
        for range in &line.highlights {
            split.push(LineHit {
                line: line.line,
                text: line.text.clone(),
                highlights: vec![range.clone()],
                target: range.clone(),
                matches: 1,
            });
        }
    }
    split
}

/// A row of the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Row {
    Run(usize),
    File(usize, usize),
    Line(usize, usize, usize),
}

pub enum SearchResultsEvent {
    /// Go to a line's first match.
    Open {
        location: ResultLocation,
        line: usize,
        target: Range<usize>,
    },
    Close,
}

pub struct SearchResults {
    pub visible: bool,
    /// Long lines wrap (as rows of their own height) instead of being cut.
    pub wrap: bool,
    /// Each new search clears the earlier ones.
    pub purge: bool,
    runs: Vec<SearchRun>,
    /// The rows shown: searches, documents and lines, without those inside folded rows.
    rows: Vec<Row>,
    pub selected: Option<usize>,
    scroll: UniformListScrollHandle,
    /// The rows' heights when long lines wrap.
    list_state: ListState,
    pub focus_handle: FocusHandle,
}

impl EventEmitter<SearchResultsEvent> for SearchResults {}

impl Focusable for SearchResults {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl SearchResults {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let remembered = cx
            .try_global::<AppState>()
            .map(|state| state.state.search_results.clone())
            .unwrap_or_default();
        Self {
            visible: false,
            wrap: remembered.wrap_long_lines,
            purge: remembered.purge,
            runs: Vec::new(),
            rows: Vec::new(),
            selected: None,
            scroll: UniformListScrollHandle::new(),
            list_state: ListState::new(0, ListAlignment::Top, px(200.)),
            focus_handle: cx.focus_handle(),
        }
    }

    /// Shows a new search above the earlier ones, with its first line selected.
    pub fn add(&mut self, mut run: SearchRun, cx: &mut Context<Self>) {
        // Preferences > Searching: off, a line with several matches has a row for each.
        let one_per_line = cx
            .try_global::<AppState>()
            .is_none_or(|state| state.settings.searching.one_entry_per_line);
        if !one_per_line {
            for file in &mut run.files {
                file.lines = entry_per_match(std::mem::take(&mut file.lines));
            }
        }
        self.visible = true;
        if self.purge {
            self.runs.clear();
        }
        self.runs.insert(0, run);
        self.rebuild_rows();
        self.selected = None;
        self.scroll_to(0);
        cx.notify();
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.runs.clear();
        self.rebuild_rows();
        self.selected = None;
        cx.notify();
    }

    /// Brings row `index` into view, as the list is drawn now.
    fn scroll_to(&mut self, index: usize) {
        if self.wrap {
            self.list_state.scroll_to_reveal_item(index);
        } else {
            self.scroll.scroll_to_item(index, ScrollStrategy::Center);
        }
    }

    /// Wrap Long Lines and Purge for Every Search, remembered for the next start.
    pub fn set_switches(&mut self, wrap: bool, purge: bool, cx: &mut Context<Self>) {
        self.wrap = wrap;
        self.purge = purge;
        AppState::update_state(cx, |state, _| {
            state.search_results = SearchResultsState {
                wrap_long_lines: wrap,
                purge,
            };
        });
        self.rebuild_rows();
        cx.notify();
    }

    /// Open Selected Pathname: the document of row `index` at its first line found; Open All
    /// (`all`): every document of its search.
    pub fn open_documents(&mut self, index: usize, all: bool, cx: &mut Context<Self>) {
        let (r, only) = match self.rows.get(index).copied() {
            Some(Row::Run(r)) => (r, None),
            Some(Row::File(r, f) | Row::Line(r, f, _)) => (r, Some(f)),
            None => return,
        };
        let files: Vec<usize> = match (all, only) {
            (false, Some(f)) => vec![f],
            (false, None) => return,
            (true, _) => (0..self.runs[r].files.len()).collect(),
        };
        for f in files {
            let file = &self.runs[r].files[f];
            if let Some(hit) = file.lines.first() {
                cx.emit(SearchResultsEvent::Open {
                    location: file.location.clone(),
                    line: hit.line,
                    target: hit.target.clone(),
                });
            }
        }
    }

    /// Removes the search, document or line in row `index` from the list. A document without
    /// lines left goes too, and so does a search without documents left. The row that takes
    /// its place is selected.
    pub fn remove(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(row) = self.rows.get(index).copied() else {
            return;
        };
        match row {
            Row::Run(r) => {
                self.runs.remove(r);
            }
            Row::File(r, f) => {
                self.runs[r].files.remove(f);
            }
            Row::Line(r, f, l) => {
                let file = &mut self.runs[r].files[f];
                let line = file.lines.remove(l);
                file.hits = file.hits.saturating_sub(line.matches);
                if file.lines.is_empty() {
                    self.runs[r].files.remove(f);
                }
            }
        }
        if let Row::File(r, _) | Row::Line(r, ..) = row
            && self.runs[r].files.is_empty()
        {
            self.runs.remove(r);
        }
        self.rebuild_rows();
        self.selected = (!self.rows.is_empty()).then(|| index.min(self.rows.len() - 1));
        cx.notify();
    }

    /// Folds or unfolds every search and document.
    fn fold_all(&mut self, collapsed: bool, cx: &mut Context<Self>) {
        for run in &mut self.runs {
            run.collapsed = collapsed;
            for file in &mut run.files {
                file.collapsed = collapsed;
            }
        }
        self.rebuild_rows();
        self.selected = None;
        cx.notify();
    }

    /// The search that row `index` belongs to, or the newest one.
    pub fn run_of(&self, index: Option<usize>) -> Option<usize> {
        match index.and_then(|index| self.rows.get(index)) {
            Some(Row::Run(r) | Row::File(r, _) | Row::Line(r, ..)) => Some(*r),
            None => (!self.runs.is_empty()).then_some(0),
        }
    }

    /// Find in These Search Results: the lines of search `run` that `searcher` finds
    /// something in, with what it found, as a new search above the others named `pattern`.
    /// Returns how many matches it found; nothing is added when there are none.
    pub fn search_within(
        &mut self,
        run: usize,
        searcher: &birchpad_core::search::Searcher,
        pattern: &str,
        cx: &mut Context<Self>,
    ) -> usize {
        let Some(source) = self.runs.get(run) else {
            return 0;
        };
        let mut files = Vec::new();
        for file in &source.files {
            let mut lines = Vec::new();
            let mut hits = 0;
            for hit in &file.lines {
                let found = searcher.find_all(&birchpad_core::Rope::from_str(&hit.text));
                let Some(first) = found.first() else {
                    continue;
                };
                hits += found.len();
                lines.push(LineHit {
                    line: hit.line,
                    text: hit.text.clone(),
                    target: first.clone(),
                    matches: found.len(),
                    highlights: found,
                });
            }
            if !lines.is_empty() {
                files.push(FileResults::new(
                    file.location.clone(),
                    file.name.clone(),
                    lines,
                    hits,
                ));
            }
        }
        let searched = source.files.len();
        let hits = files.iter().map(|file| file.hits).sum();
        if hits > 0 {
            let purge = std::mem::replace(&mut self.purge, false);
            self.add(SearchRun::new(pattern, files, searched), cx);
            self.purge = purge;
        }
        hits
    }

    /// The text of the line in row `index`, without its "Line N:".
    pub fn line_text(&self, index: usize) -> Option<SharedString> {
        match self.rows.get(index)? {
            Row::Line(r, f, l) => Some(self.runs[*r].files[*f].lines[*l].text.clone().into()),
            _ => None,
        }
    }

    /// The path, or untitled name, of the document in row `index` or of its line.
    pub fn path_text(&self, index: usize) -> Option<String> {
        match self.rows.get(index)? {
            Row::File(r, f) | Row::Line(r, f, _) => Some(self.runs[*r].files[*f].name.clone()),
            Row::Run(_) => None,
        }
    }

    /// The right-click menu of row `index`.
    fn context_menu(
        &self,
        index: usize,
        cx: &mut Context<Self>,
    ) -> impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static {
        let this = cx.entity().downgrade();
        let line = self.line_text(index);
        let path = self.path_text(index);
        let (wrap, purge) = (self.wrap, self.purge);
        move |menu, _, _| {
            let act = |run: fn(&mut SearchResults, usize, &mut Context<SearchResults>)| {
                let this = this.clone();
                move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                    this.update(cx, |this, cx| run(this, index, cx)).ok();
                }
            };
            let copy = |text: Option<String>| {
                move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                    if let Some(text) = &text {
                        cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                    }
                }
            };
            menu.item(
                PopupMenuItem::new(t!("search-results.remove")).on_click(act(|this, index, cx| {
                    this.remove(index, cx);
                })),
            )
            .separator()
            .item(
                PopupMenuItem::new(t!("search-results.copy-selected-line"))
                    .disabled(line.is_none())
                    .on_click(copy(line.as_ref().map(ToString::to_string))),
            )
            .item(
                PopupMenuItem::new(t!("search-results.copy-selected-pathname"))
                    .disabled(path.is_none())
                    .on_click(copy(path.clone())),
            )
            .separator()
            .item(
                PopupMenuItem::new(t!("search-results.open-selected-pathname"))
                    .disabled(path.is_none())
                    .on_click(act(|this, index, cx| this.open_documents(index, false, cx))),
            )
            .item(
                PopupMenuItem::new(t!("search-results.open-all"))
                    .on_click(act(|this, index, cx| this.open_documents(index, true, cx))),
            )
            .separator()
            .item(
                PopupMenuItem::new(t!("search-results.fold-all")).on_click(act(|this, _, cx| {
                    this.fold_all(true, cx);
                })),
            )
            .item(
                PopupMenuItem::new(t!("search-results.unfold-all")).on_click(act(|this, _, cx| {
                    this.fold_all(false, cx);
                })),
            )
            .separator()
            .item(
                PopupMenuItem::new(t!("search-results.wrap-long-lines"))
                    .checked(wrap)
                    .on_click(act(|this, _, cx| {
                        this.set_switches(!this.wrap, this.purge, cx)
                    })),
            )
            .item(
                PopupMenuItem::new(t!("search-results.purge-for-every-search"))
                    .checked(purge)
                    .on_click(act(|this, _, cx| {
                        this.set_switches(this.wrap, !this.purge, cx)
                    })),
            )
            .separator()
            .item(
                PopupMenuItem::new(t!("search-results.find-in-these-search-results")).on_click(
                    |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                        window.dispatch_action(
                            Box::new(crate::commands::RunCommand::new("search.find-in-results")),
                            cx,
                        );
                    },
                ),
            )
            .item(
                PopupMenuItem::new(t!("search-results.clear-all"))
                    .on_click(act(|this, _, cx| this.clear(cx))),
            )
        }
    }

    fn rebuild_rows(&mut self) {
        self.rows.clear();
        self.push_rows();
    }

    /// The rows of the searches and documents that are not folded.
    fn push_rows(&mut self) {
        for (r, run) in self.runs.iter().enumerate() {
            self.rows.push(Row::Run(r));
            if run.collapsed {
                continue;
            }
            for (f, file) in run.files.iter().enumerate() {
                self.rows.push(Row::File(r, f));
                if file.collapsed {
                    continue;
                }
                self.rows
                    .extend((0..file.lines.len()).map(|l| Row::Line(r, f, l)));
            }
        }
        self.list_state.reset(self.rows.len());
    }

    /// Folds or unfolds a search or a document.
    fn toggle(&mut self, row: Row, cx: &mut Context<Self>) {
        match row {
            Row::Run(r) => self.runs[r].collapsed ^= true,
            Row::File(r, f) => self.runs[r].files[f].collapsed ^= true,
            Row::Line(..) => return,
        }
        self.rebuild_rows();
        self.selected = self.rows.iter().position(|shown| *shown == row);
        cx.notify();
    }

    /// Opens the line in `row`, if it is one.
    fn open(&mut self, index: usize, cx: &mut Context<Self>) {
        self.selected = Some(index);
        self.scroll_to(index);
        if let Some(Row::Line(r, f, l)) = self.rows.get(index).copied() {
            let file = &self.runs[r].files[f];
            let hit = &file.lines[l];
            cx.emit(SearchResultsEvent::Open {
                location: file.location.clone(),
                line: hit.line,
                target: hit.target.clone(),
            });
        }
        cx.notify();
    }

    /// F4 / Shift+F4: the next or previous line, wrapping around. Returns false when there is
    /// none.
    pub fn step(&mut self, forward: bool, cx: &mut Context<Self>) -> bool {
        let lines: Vec<usize> = (0..self.rows.len())
            .filter(|&i| matches!(self.rows[i], Row::Line(..)))
            .collect();
        if lines.is_empty() {
            return false;
        }
        let next = match self.selected {
            None if forward => lines[0],
            None => lines[lines.len() - 1],
            Some(current) if forward => lines
                .iter()
                .copied()
                .find(|&i| i > current)
                .unwrap_or(lines[0]),
            Some(current) => lines
                .iter()
                .rev()
                .copied()
                .find(|&i| i < current)
                .unwrap_or(lines[lines.len() - 1]),
        };
        self.visible = true;
        self.open(next, cx);
        true
    }

    /// The rows as text, indented by level: for tests.
    #[cfg(any(feature = "test-support", feature = "e2e"))]
    pub fn rows_text(&self) -> Vec<String> {
        self.rows
            .iter()
            .map(|row| match *row {
                Row::Run(r) => self.runs[r].title(),
                Row::File(r, f) => {
                    let file = &self.runs[r].files[f];
                    format!("  {} ({})", file.name, file.hits)
                }
                Row::Line(r, f, l) => {
                    let hit = &self.runs[r].files[f].lines[l];
                    format!("    Line {}: {}", hit.line + 1, hit.text)
                }
            })
            .collect()
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        if keystroke.modifiers.modified() {
            return;
        }
        let last = self.rows.len().saturating_sub(1);
        match keystroke.key.as_str() {
            // An empty list has no row to select.
            "down" | "up" if self.rows.is_empty() => {}
            "down" => {
                self.selected = Some(self.selected.map_or(0, |i| (i + 1).min(last)));
            }
            "up" => self.selected = Some(self.selected.map_or(0, |i| i.saturating_sub(1))),
            "enter" => {
                if let Some(index) = self.selected
                    && let Some(row) = self.rows.get(index).copied()
                {
                    match row {
                        Row::Line(..) => self.open(index, cx),
                        row => self.toggle(row, cx),
                    }
                }
            }
            "delete" => {
                if let Some(index) = self.selected {
                    self.remove(index, cx);
                }
            }
            _ => return,
        }
        if let Some(index) = self.selected {
            self.scroll_to(index);
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn render_row(&self, index: usize, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let row = self.rows[index];
        let selected = self.selected == Some(index);
        let wrap = self.wrap;
        let base = div()
            .id(("search-result", index))
            .debug_selector(move || format!("search-result-{index}"))
            .w_full()
            .when(!wrap, |row| {
                row.h(px(ROW_HEIGHT)).whitespace_nowrap().overflow_hidden()
            })
            .when(wrap, |row| row.min_h(px(ROW_HEIGHT)))
            .flex()
            .flex_row()
            .items_center()
            .when(selected, |row| {
                row.bg(crate::theme::paint(crate::theme::ui().selected))
            })
            // A right click selects the row its menu works on.
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, _, window, cx| {
                    window.focus(&this.focus_handle, cx);
                    this.selected = Some(index);
                    cx.notify();
                }),
            )
            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                window.focus(&this.focus_handle, cx);
                match row {
                    Row::Line(..) if event.click_count() >= 2 => this.open(index, cx),
                    Row::Line(..) => {
                        this.selected = Some(index);
                        cx.notify();
                    }
                    row => this.toggle(row, cx),
                }
            }));
        let fold = |collapsed: bool| if collapsed { "▸ " } else { "▾ " };
        let menu = self.context_menu(index, cx);
        match row {
            Row::Run(r) => {
                let run = &self.runs[r];
                base.pl_1()
                    .bg(if selected {
                        crate::theme::paint(crate::theme::ui().selected)
                    } else {
                        crate::theme::paint(crate::theme::ui().heading)
                    })
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(format!("{}{}", fold(run.collapsed), run.title()))
                    .context_menu(menu)
                    .into_any_element()
            }
            Row::File(r, f) => {
                let file = &self.runs[r].files[f];
                base.pl(px(16.))
                    .text_color(crate::theme::paint(crate::theme::ui().line_number))
                    .child(format!(
                        "{}{} ({})",
                        fold(file.collapsed),
                        file.name,
                        crate::i18n::count(file.hits, "hit", "hits")
                    ))
                    .context_menu(menu)
                    .into_any_element()
            }
            Row::Line(r, f, l) => {
                let hit = &self.runs[r].files[f].lines[l];
                let prefix = t!("search-results.line", line = hit.line + 1);
                let text = format!("{prefix}{}", hit.text);
                let highlight = HighlightStyle {
                    background_color: Some(
                        crate::theme::paint(crate::theme::ui().match_background).into(),
                    ),
                    font_weight: Some(FontWeight::BOLD),
                    ..HighlightStyle::default()
                };
                let highlights: Vec<(Range<usize>, HighlightStyle)> = hit
                    .highlights
                    .iter()
                    .map(|range| {
                        (
                            range.start + prefix.len()..range.end + prefix.len(),
                            highlight,
                        )
                    })
                    .collect();
                base.pl(px(36.))
                    .font_family(crate::MONOSPACE)
                    .child(StyledText::new(text).with_highlights(highlights))
                    .context_menu(menu)
                    .into_any_element()
            }
        }
    }
}

impl Render for SearchResults {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let header = bar(BarColors::surface(), Line::Under)
            .gap_2()
            .px_2()
            .h(px(28.))
            .child(div().flex_1().child(t!("search-results.search-results")))
            .child(
                Button::new("clear-results")
                    .small()
                    .ghost()
                    .label(t!("search-results.clear"))
                    .on_click(cx.listener(|this, _, _, cx| this.clear(cx))),
            )
            .child(
                Button::new("close-results")
                    .small()
                    .ghost()
                    .icon(IconName::Close)
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(SearchResultsEvent::Close))),
            );
        let list = if self.wrap {
            list(
                self.list_state.clone(),
                cx.processor(|this, index: usize, _, cx| {
                    this.render_row(index, cx).into_any_element()
                }),
            )
            .flex_1()
            .text_size(px(13.))
            .into_any_element()
        } else {
            uniform_list(
                "search-results-list",
                self.rows.len(),
                cx.processor(|this, range: Range<usize>, _, cx| {
                    range
                        .map(|index| this.render_row(index, cx))
                        .collect::<Vec<_>>()
                }),
            )
            .track_scroll(&self.scroll)
            .flex_1()
            .text_size(px(13.))
            .into_any_element()
        };
        div()
            .id("search-results")
            .key_context("SearchResults")
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .size_full()
            .flex()
            .flex_col()
            .bg(crate::theme::paint(crate::theme::ui().background))
            .child(header)
            .child(list)
    }
}
