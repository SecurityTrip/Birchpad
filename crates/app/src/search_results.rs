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
//! does too, and copies the selected line or path, folds and unfolds everything, and clears the
//! list.

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

const ROW_HEIGHT: f32 = 20.;

/// Where a document with results is.
#[derive(Clone)]
pub(crate) enum ResultLocation {
    /// An open document.
    Buffer(WeakEntity<Buffer>),
    /// A file on disk (Find in Files).
    File(PathBuf),
}

/// The results of one search in one document.
pub(crate) struct FileResults {
    pub(crate) location: ResultLocation,
    /// The path, or the name of an untitled document.
    pub(crate) name: String,
    pub(crate) lines: Vec<LineHit>,
    pub(crate) hits: usize,
    collapsed: bool,
}

impl FileResults {
    pub(crate) fn new(
        location: ResultLocation,
        name: String,
        lines: Vec<LineHit>,
        hits: usize,
    ) -> Self {
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
pub(crate) struct SearchRun {
    pattern: String,
    /// How many documents were searched.
    searched: usize,
    pub(crate) files: Vec<FileResults>,
    collapsed: bool,
}

impl SearchRun {
    pub(crate) fn new(pattern: &str, files: Vec<FileResults>, searched: usize) -> Self {
        Self {
            pattern: pattern.to_owned(),
            searched,
            files,
            collapsed: false,
        }
    }

    /// Notepad++'s heading: `Search "foo" (3 hits in 2 files of 5 searched)`. Results removed
    /// from the list no longer count.
    pub(crate) fn title(&self) -> String {
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

/// A row of the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Row {
    Run(usize),
    File(usize, usize),
    Line(usize, usize, usize),
}

pub(crate) enum SearchResultsEvent {
    /// Go to a line's first match.
    Open {
        location: ResultLocation,
        line: usize,
        target: Range<usize>,
    },
    Close,
}

pub(crate) struct SearchResults {
    pub(crate) visible: bool,
    /// Long lines wrap (as rows of their own height) instead of being cut.
    pub(crate) wrap: bool,
    /// Each new search clears the earlier ones.
    pub(crate) purge: bool,
    runs: Vec<SearchRun>,
    /// The rows shown: searches, documents and lines, without those inside folded rows.
    rows: Vec<Row>,
    selected: Option<usize>,
    scroll: UniformListScrollHandle,
    /// The rows' heights when long lines wrap.
    list_state: ListState,
    focus_handle: FocusHandle,
}

impl EventEmitter<SearchResultsEvent> for SearchResults {}

impl Focusable for SearchResults {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl SearchResults {
    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
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
    pub(crate) fn add(&mut self, run: SearchRun, cx: &mut Context<Self>) {
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

    pub(crate) fn clear(&mut self, cx: &mut Context<Self>) {
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
    pub(crate) fn set_switches(&mut self, wrap: bool, purge: bool, cx: &mut Context<Self>) {
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
    pub(crate) fn open_documents(&mut self, index: usize, all: bool, cx: &mut Context<Self>) {
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
    pub(crate) fn remove(&mut self, index: usize, cx: &mut Context<Self>) {
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

    /// The text of the line in row `index`, without its "Line N:".
    fn line_text(&self, index: usize) -> Option<SharedString> {
        match self.rows.get(index)? {
            Row::Line(r, f, l) => Some(self.runs[*r].files[*f].lines[*l].text.clone().into()),
            _ => None,
        }
    }

    /// The path, or untitled name, of the document in row `index` or of its line.
    fn path_text(&self, index: usize) -> Option<String> {
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
    pub(crate) fn step(&mut self, forward: bool, cx: &mut Context<Self>) -> bool {
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
    #[cfg(any(test, feature = "e2e"))]
    pub(crate) fn rows_text(&self) -> Vec<String> {
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
        let header = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .px_2()
            .h(px(28.))
            .flex_none()
            .border_b_1()
            .border_color(crate::theme::paint(crate::theme::ui().border))
            .bg(crate::theme::paint(crate::theme::ui().surface))
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

#[cfg(test)]
mod tests {
    #![allow(
        clippy::single_range_in_vec_init,
        clippy::type_complexity,
        reason = "expected results written out"
    )]

    use gpui_kit::{Entity, Modifiers, TestAppContext, VisualTestContext};

    use super::*;
    use crate::workspace::tests::open_workspace;

    fn hit(line: usize, text: &str, matches: usize) -> LineHit {
        LineHit {
            line,
            text: text.to_owned(),
            highlights: Vec::new(),
            target: 0..1,
            matches,
        }
    }

    fn file(name: &str, lines: Vec<LineHit>) -> FileResults {
        let hits = lines.iter().map(|line| line.matches).sum();
        FileResults::new(
            ResultLocation::File(name.into()),
            name.to_owned(),
            lines,
            hits,
        )
    }

    /// The panel of a new window, focused, with a search for "foo" in a.txt (three hits on two
    /// lines) and b.txt (one), of five documents searched.
    fn panel_with_results(
        cx: &mut TestAppContext,
    ) -> (Entity<SearchResults>, &mut VisualTestContext) {
        let (workspace, cx) = open_workspace(cx);
        let results = workspace.read_with(cx, |workspace, _| workspace.search_results.clone());
        workspace.update_in(cx, |workspace, window, cx| {
            let run = SearchRun::new(
                "foo",
                vec![
                    file("a.txt", vec![hit(0, "foo foo", 2), hit(2, "foo", 1)]),
                    file("b.txt", vec![hit(1, "x foo", 1)]),
                ],
                5,
            );
            workspace.search_results.update(cx, |results, cx| {
                results.add(run, cx);
                window.focus(&results.focus_handle, cx);
            });
            cx.notify();
        });
        cx.run_until_parked();
        (results, cx)
    }

    fn rows(results: &Entity<SearchResults>, cx: &mut VisualTestContext) -> Vec<String> {
        results.read_with(cx, |results, _| results.rows_text())
    }

    fn selected(results: &Entity<SearchResults>, cx: &mut VisualTestContext) -> Option<usize> {
        results.read_with(cx, |results, _| results.selected)
    }

    #[gpui_kit::test]
    fn delete_removes_the_selected_line_and_what_it_leaves_empty(cx: &mut TestAppContext) {
        let (results, cx) = panel_with_results(cx);
        // Nothing selected: Delete removes nothing.
        cx.simulate_keystrokes("delete");
        assert_eq!(rows(&results, cx).len(), 6);

        cx.simulate_keystrokes("down down down delete");
        assert_eq!(
            rows(&results, cx),
            [
                "Search \"foo\" (2 hits in 2 files of 5 searched)",
                "  a.txt (1)",
                "    Line 3: foo",
                "  b.txt (1)",
                "    Line 2: x foo",
            ],
            "the line's two hits no longer count"
        );
        assert_eq!(selected(&results, cx), Some(2), "the next line");
        // The last line of a document takes the document with it.
        cx.simulate_keystrokes("delete");
        assert_eq!(
            rows(&results, cx),
            [
                "Search \"foo\" (1 hit in 1 file of 5 searched)",
                "  b.txt (1)",
                "    Line 2: x foo",
            ]
        );
        assert_eq!(
            selected(&results, cx),
            Some(2),
            "the last row, past the end"
        );
        // The last document of a search takes the search with it.
        cx.simulate_keystrokes("delete");
        assert!(rows(&results, cx).is_empty());
        assert_eq!(selected(&results, cx), None);
        cx.simulate_keystrokes("delete down enter shift-delete");
        assert!(rows(&results, cx).is_empty());
    }

    #[gpui_kit::test]
    fn delete_on_a_search_or_a_document_removes_everything_under_it(cx: &mut TestAppContext) {
        let (results, cx) = panel_with_results(cx);
        results.update(cx, |results, cx| {
            results.add(
                SearchRun::new("x", vec![file("c.txt", vec![hit(4, "x", 1)])], 1),
                cx,
            );
        });
        // The newer search is on top; its heading is row 0, the older one's row 3.
        cx.simulate_keystrokes("down down down down delete");
        assert_eq!(
            rows(&results, cx),
            [
                "Search \"x\" (1 hit in 1 file of 1 searched)",
                "  c.txt (1)",
                "    Line 5: x",
            ]
        );
        // Delete with a modifier is not Delete.
        cx.simulate_keystrokes("up shift-delete");
        assert_eq!(rows(&results, cx).len(), 3);
        cx.simulate_keystrokes("delete");
        assert!(
            rows(&results, cx).is_empty(),
            "the search went with its only document"
        );
        // F4 has nothing to go to.
        let stepped = results.update(cx, |results, cx| results.step(true, cx));
        assert!(!stepped);
    }

    #[gpui_kit::test]
    fn the_right_click_menu_removes_copies_folds_and_clears(cx: &mut TestAppContext) {
        let (results, cx) = panel_with_results(cx);
        let right_click = |index: usize, cx: &mut VisualTestContext| {
            let selector: &'static str = format!("search-result-{index}").leak();
            let center = cx.debug_bounds(selector).expect("drawn").center();
            cx.simulate_mouse_down(center, MouseButton::Right, Modifiers::none());
            cx.simulate_mouse_up(center, MouseButton::Right, Modifiers::none());
            cx.run_until_parked();
        };
        let clipboard = |cx: &mut VisualTestContext| {
            cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()))
        };

        // Remove, Copy Selected Line, Copy Selected Pathname, Open Selected Pathname, Open All,
        // Fold All, Unfold All, Wrap Long Lines, Purge for Every Search, Clear All; the arrow
        // keys skip the items that are disabled for the row.
        right_click(5, cx);
        assert_eq!(selected(&results, cx), Some(5), "the right click selects");
        cx.simulate_keystrokes("down down enter");
        assert_eq!(clipboard(cx).as_deref(), Some("x foo"));
        right_click(2, cx);
        cx.simulate_keystrokes("down down down enter");
        assert_eq!(clipboard(cx).as_deref(), Some("a.txt"));
        right_click(2, cx);
        cx.simulate_keystrokes("down enter");
        assert_eq!(rows(&results, cx)[2], "    Line 3: foo", "removed");

        // A search heading has neither a line nor a path to copy or open.
        right_click(0, cx);
        cx.simulate_keystrokes("down down down enter");
        assert_eq!(
            rows(&results, cx),
            ["Search \"foo\" (2 hits in 2 files of 5 searched)"],
            "folded"
        );
        right_click(0, cx);
        cx.simulate_keystrokes("down down down down enter");
        assert_eq!(rows(&results, cx).len(), 5, "unfolded");
        // A document has no line to copy.
        right_click(1, cx);
        cx.simulate_keystrokes("down down enter");
        assert_eq!(clipboard(cx).as_deref(), Some("a.txt"));
        // Escape closes the menu without doing anything.
        right_click(1, cx);
        cx.simulate_keystrokes("down escape");
        assert_eq!(rows(&results, cx).len(), 5);
        right_click(1, cx);
        cx.simulate_keystrokes("down down down down down down down down down enter");
        assert!(rows(&results, cx).is_empty(), "cleared");
    }

    #[gpui_kit::test]
    fn a_search_heading_has_nothing_to_copy(cx: &mut TestAppContext) {
        let results = cx.new(SearchResults::new);
        results.update(cx, |results, cx| {
            results.add(
                SearchRun::new("foo", vec![file("a.txt", vec![hit(0, "foo", 1)])], 1),
                cx,
            );
            assert_eq!(results.line_text(0), None);
            assert_eq!(results.path_text(0), None);
            assert_eq!(results.line_text(1), None, "a document has no line");
            assert_eq!(results.path_text(1).as_deref(), Some("a.txt"));
            assert_eq!(results.line_text(2).as_deref(), Some("foo"));
            assert_eq!(results.path_text(2).as_deref(), Some("a.txt"));
            assert_eq!(results.line_text(3), None, "past the end");
            assert_eq!(results.path_text(3), None);
            // Removing a row that is not there changes nothing.
            results.remove(3, cx);
            assert_eq!(results.rows_text().len(), 3);
        });
    }
}
