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

use birchpad_core::Rope;
use birchpad_core::motion::{line_of, line_range};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::{IconName, Sizable};
use gpui_kit::{
    App, ClickEvent, ClipboardItem, Context, EventEmitter, FocusHandle, Focusable, FontWeight,
    HighlightStyle, KeyDownEvent, MouseButton, ScrollStrategy, SharedString, StyledText,
    UniformListScrollHandle, WeakEntity, Window, div, prelude::*, px, rgb, uniform_list,
};

use crate::buffer::Buffer;

/// The most of a line a result shows; longer lines are cut there.
const MAX_LINE_SHOWN: usize = 1024;
const ROW_HEIGHT: f32 = 20.;

/// Where a document with results is.
#[derive(Clone)]
pub(crate) enum ResultLocation {
    /// An open document.
    Buffer(WeakEntity<Buffer>),
    /// A file on disk (Find in Files).
    File(PathBuf),
}

/// A line with matches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LineHit {
    /// 0-based.
    pub(crate) line: usize,
    /// The line, without its line break, cut at [`MAX_LINE_SHOWN`] bytes.
    pub(crate) text: SharedString,
    /// The matches within `text`.
    pub(crate) highlights: Vec<Range<usize>>,
    /// The first match, in bytes from the start of the line (it may run past its end).
    pub(crate) target: Range<usize>,
    /// How many matches start on the line.
    pub(crate) matches: usize,
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
        let plural = |count: usize, one: &str, many: &str| {
            format!("{count} {}", if count == 1 { one } else { many })
        };
        format!(
            "Search \"{}\" ({} in {} of {} searched)",
            self.pattern,
            plural(hits, "hit", "hits"),
            plural(self.files.len(), "file", "files"),
            self.searched
        )
    }
}

/// The lines of `text` with `matches` (sorted), each with its matches.
pub(crate) fn line_hits(text: &Rope, matches: &[Range<usize>]) -> Vec<LineHit> {
    let mut hits: Vec<LineHit> = Vec::new();
    for found in matches {
        let line = line_of(text, found.start);
        let bounds = line_range(text, line);
        if hits.last().is_none_or(|last| last.line != line) {
            let mut end = bounds.end.min(bounds.start + MAX_LINE_SHOWN);
            end = text.floor_char_boundary(end);
            hits.push(LineHit {
                line,
                text: text.slice(bounds.start..end).to_string().into(),
                highlights: Vec::new(),
                target: found.start - bounds.start..found.end - bounds.start,
                matches: 0,
            });
        }
        let hit = hits.last_mut().expect("just pushed");
        hit.matches += 1;
        let shown = hit.text.len();
        let start = found.start - bounds.start;
        let end = (found.end.min(bounds.end) - bounds.start).min(shown);
        if start < end {
            hit.highlights.push(start..end);
        }
    }
    hits
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
    runs: Vec<SearchRun>,
    /// The rows shown: searches, documents and lines, without those inside folded rows.
    rows: Vec<Row>,
    selected: Option<usize>,
    scroll: UniformListScrollHandle,
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
        Self {
            visible: false,
            runs: Vec::new(),
            rows: Vec::new(),
            selected: None,
            scroll: UniformListScrollHandle::new(),
            focus_handle: cx.focus_handle(),
        }
    }

    /// Shows a new search above the earlier ones, with its first line selected.
    pub(crate) fn add(&mut self, run: SearchRun, cx: &mut Context<Self>) {
        self.visible = true;
        self.runs.insert(0, run);
        self.rebuild_rows();
        self.selected = None;
        self.scroll.scroll_to_item(0, ScrollStrategy::Top);
        cx.notify();
    }

    pub(crate) fn clear(&mut self, cx: &mut Context<Self>) {
        self.runs.clear();
        self.rows.clear();
        self.selected = None;
        cx.notify();
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
            Row::Line(r, f, l) => Some(self.runs[*r].files[*f].lines[*l].text.clone()),
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
                PopupMenuItem::new("Remove").on_click(act(|this, index, cx| {
                    this.remove(index, cx);
                })),
            )
            .separator()
            .item(
                PopupMenuItem::new("Copy Selected Line")
                    .disabled(line.is_none())
                    .on_click(copy(line.as_ref().map(ToString::to_string))),
            )
            .item(
                PopupMenuItem::new("Copy Selected Pathname")
                    .disabled(path.is_none())
                    .on_click(copy(path.clone())),
            )
            .separator()
            .item(PopupMenuItem::new("Fold All").on_click(act(|this, _, cx| {
                this.fold_all(true, cx);
            })))
            .item(
                PopupMenuItem::new("Unfold All").on_click(act(|this, _, cx| {
                    this.fold_all(false, cx);
                })),
            )
            .separator()
            .item(PopupMenuItem::new("Clear All").on_click(act(|this, _, cx| this.clear(cx))))
        }
    }

    fn rebuild_rows(&mut self) {
        self.rows.clear();
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
        self.scroll.scroll_to_item(index, ScrollStrategy::Center);
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
    #[cfg(test)]
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
            self.scroll.scroll_to_item(index, ScrollStrategy::Center);
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn render_row(&self, index: usize, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let row = self.rows[index];
        let selected = self.selected == Some(index);
        let base = div()
            .id(("search-result", index))
            .debug_selector(move || format!("search-result-{index}"))
            .w_full()
            .h(px(ROW_HEIGHT))
            .flex()
            .flex_row()
            .items_center()
            .whitespace_nowrap()
            .overflow_hidden()
            .when(selected, |row| row.bg(rgb(0xddf4ff)))
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
                        rgb(0xddf4ff)
                    } else {
                        rgb(0xe8eef6)
                    })
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(format!("{}{}", fold(run.collapsed), run.title()))
                    .context_menu(menu)
                    .into_any_element()
            }
            Row::File(r, f) => {
                let file = &self.runs[r].files[f];
                let plural = if file.hits == 1 { "hit" } else { "hits" };
                base.pl(px(16.))
                    .text_color(rgb(0x1a7f37))
                    .child(format!(
                        "{}{} ({} {plural})",
                        fold(file.collapsed),
                        file.name,
                        file.hits
                    ))
                    .context_menu(menu)
                    .into_any_element()
            }
            Row::Line(r, f, l) => {
                let hit = &self.runs[r].files[f].lines[l];
                let prefix = format!("Line {}: ", hit.line + 1);
                let text = format!("{prefix}{}", hit.text);
                let highlight = HighlightStyle {
                    background_color: Some(rgb(0xfff8c5).into()),
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
            .border_color(rgb(0xd0d7de))
            .bg(rgb(0xf6f8fa))
            .child(div().flex_1().child("Search results"))
            .child(
                Button::new("clear-results")
                    .small()
                    .ghost()
                    .label("Clear")
                    .on_click(cx.listener(|this, _, _, cx| this.clear(cx))),
            )
            .child(
                Button::new("close-results")
                    .small()
                    .ghost()
                    .icon(IconName::Close)
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(SearchResultsEvent::Close))),
            );
        let list = uniform_list(
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
        .text_size(px(13.));
        div()
            .id("search-results")
            .key_context("SearchResults")
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0xffffff))
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
            text: text.to_owned().into(),
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

        // Remove, Copy Selected Line, Copy Selected Pathname, Fold All, Unfold All, Clear All;
        // the arrow keys skip the items that are disabled for the row.
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

        // A search heading has neither a line nor a path to copy.
        right_click(0, cx);
        cx.simulate_keystrokes("down down enter");
        assert_eq!(
            rows(&results, cx),
            ["Search \"foo\" (2 hits in 2 files of 5 searched)"],
            "folded"
        );
        right_click(0, cx);
        cx.simulate_keystrokes("down down down enter");
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
        cx.simulate_keystrokes("down down down down down enter");
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

    #[test]
    fn lines_gather_their_matches() {
        let text = Rope::from_str("foo bar foo\r\nbaz\nfoo-\nfoo\nbar");
        let matches = [0..3, 8..11, 17..20, 22..29];
        let hits = line_hits(&text, &matches);
        let summary: Vec<(usize, &str, Vec<Range<usize>>, Range<usize>, usize)> = hits
            .iter()
            .map(|hit| {
                (
                    hit.line,
                    hit.text.as_ref(),
                    hit.highlights.clone(),
                    hit.target.clone(),
                    hit.matches,
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                (0, "foo bar foo", vec![0..3, 8..11], 0..3, 2),
                (2, "foo-", vec![0..3], 0..3, 1),
                // A match over a line break shows its first line's part.
                (3, "foo", vec![0..3], 0..7, 1),
            ]
        );
    }

    #[test]
    fn long_lines_are_cut() {
        let long = format!("{}needle", "x".repeat(2000));
        let text = Rope::from_str(&long);
        let hits = line_hits(&text, &[2000..2006]);
        assert_eq!(hits[0].text.len(), MAX_LINE_SHOWN);
        assert!(hits[0].highlights.is_empty(), "the match is past the cut");
        assert_eq!(hits[0].target, 2000..2006, "but going there still works");
    }
}
