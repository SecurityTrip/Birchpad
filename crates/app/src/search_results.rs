//! The search results panel, as Notepad++'s Search results window: Find All in the current
//! document or in all opened documents (and Find in Files) list their matches here.
//!
//! Searches stack, the newest first. Each lists its documents, and under each document the lines
//! with matches, the matches highlighted. Clicking a search or a document folds it; double-
//! clicking a line, or Enter, goes to its first match; F4 and Shift+F4 go to the next and
//! previous line from anywhere. Lines are kept as their line number and the match's offsets
//! within the line, as Notepad++ does: an edit above a match moves it away from its result.

use std::ops::Range;
use std::path::PathBuf;

use birchpad_core::Rope;
use birchpad_core::motion::{line_of, line_range};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{IconName, Sizable};
use gpui_kit::{
    App, ClickEvent, Context, EventEmitter, FocusHandle, Focusable, FontWeight, HighlightStyle,
    KeyDownEvent, ScrollStrategy, SharedString, StyledText, UniformListScrollHandle, WeakEntity,
    Window, div, prelude::*, px, rgb, uniform_list,
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

/// One search: its description and its documents.
pub(crate) struct SearchRun {
    pub(crate) title: String,
    pub(crate) files: Vec<FileResults>,
    collapsed: bool,
}

impl SearchRun {
    /// Notepad++'s heading: `Search "foo" (3 hits in 2 files of 5 searched)`.
    pub(crate) fn new(pattern: &str, files: Vec<FileResults>, searched: usize) -> Self {
        let hits: usize = files.iter().map(|file| file.hits).sum();
        let plural = |count: usize, one: &str, many: &str| {
            format!("{count} {}", if count == 1 { one } else { many })
        };
        Self {
            title: format!(
                "Search \"{pattern}\" ({} in {} of {searched} searched)",
                plural(hits, "hit", "hits"),
                plural(files.len(), "file", "files")
            ),
            files,
            collapsed: false,
        }
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
            });
        }
        let hit = hits.last_mut().expect("just pushed");
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
                Row::Run(r) => self.runs[r].title.clone(),
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
            "down" => {
                self.selected = Some(self.selected.map_or(0, |i| (i + 1).min(last)));
            }
            "up" => self.selected = Some(self.selected.map_or(0, |i| i.saturating_sub(1))),
            "enter" => {
                if let Some(index) = self.selected {
                    match self.rows[index] {
                        Row::Line(..) => self.open(index, cx),
                        row => self.toggle(row, cx),
                    }
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
            .w_full()
            .h(px(ROW_HEIGHT))
            .flex()
            .flex_row()
            .items_center()
            .whitespace_nowrap()
            .overflow_hidden()
            .when(selected, |row| row.bg(rgb(0xddf4ff)))
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
                    .child(format!("{}{}", fold(run.collapsed), run.title))
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

    use super::*;

    #[test]
    fn lines_gather_their_matches() {
        let text = Rope::from_str("foo bar foo\r\nbaz\nfoo-\nfoo\nbar");
        let matches = [0..3, 8..11, 17..20, 22..29];
        let hits = line_hits(&text, &matches);
        let summary: Vec<(usize, &str, Vec<Range<usize>>, Range<usize>)> = hits
            .iter()
            .map(|hit| {
                (
                    hit.line,
                    hit.text.as_ref(),
                    hit.highlights.clone(),
                    hit.target.clone(),
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                (0, "foo bar foo", vec![0..3, 8..11], 0..3),
                (2, "foo-", vec![0..3], 0..3),
                // A match over a line break shows its first line's part.
                (3, "foo", vec![0..3], 0..7),
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
