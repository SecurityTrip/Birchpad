//! Smart highlighting, as in Notepad++: when a word is selected (or, with
//! `highlighting.smart.whole-word` off, any text on one line), its other occurrences in the
//! visible text are highlighted.
//!
//! Only the text shown in the frame is searched, plus a margin so that short scrolls reuse the
//! result. The result belongs to the view (another view of the document has its own selection)
//! and is cached by the document revision, the token and the searched ranges. Each frame
//! searches for at most [`BUDGET`]; what is left continues in the next frames, so a large
//! visible range never delays typing.

use std::ops::Range as ByteRange;
use std::time::{Duration, Instant};

use birchpad_core::search::{Query, Scan, Searcher, smart_highlight_token};
use birchpad_core::{RangeSet, RevisionId, Rope};
use gpui_kit::App;

use super::EditorView;
use crate::app_state::AppState;
use crate::find::ActiveFindOptions;

/// How long one frame may search.
const BUDGET: Duration = Duration::from_millis(2);
/// Bytes searched beyond the visible text on each side.
const MARGIN: usize = 16 * 1024;
/// Longer selections are not highlighted.
const MAX_TOKEN: usize = 64 * 1024;

/// The occurrences of the selected token around the visible text.
pub(super) struct SmartHighlight {
    revision: RevisionId,
    query: Query,
    searcher: Searcher,
    /// One scan per searched range; sorted and disjoint.
    scans: Vec<Scan>,
    /// What the scans found so far.
    pub(super) matches: RangeSet<()>,
}

impl SmartHighlight {
    /// Whether this result is for `revision` and `query` and has searched every range of
    /// `shown`.
    fn covers(&self, revision: RevisionId, query: &Query, shown: &[ByteRange<usize>]) -> bool {
        self.revision == revision
            && self.query == *query
            && shown.iter().all(|range| {
                self.scans.iter().any(|scan| {
                    let searched = scan.range();
                    searched.start <= range.start && range.end <= searched.end
                })
            })
    }

    fn is_done(&self) -> bool {
        self.scans.iter().all(Scan::is_done)
    }
}

impl EditorView {
    /// Brings the smart highlights up to date for `shown`, the sorted ranges of text shown in
    /// this frame. Returns true if searching is not finished and needs another frame.
    pub(super) fn update_smart_highlight(
        &mut self,
        shown: &[ByteRange<usize>],
        text: &Rope,
        cx: &App,
    ) -> bool {
        let Some(query) = self.smart_highlight_query(text, cx) else {
            self.smart_highlight = None;
            return false;
        };
        let revision = self.buffer.read(cx).doc().revision();
        let current = self
            .smart_highlight
            .as_ref()
            .is_some_and(|smart| smart.covers(revision, &query, shown));
        if !current {
            let Ok(searcher) = Searcher::new(&query) else {
                self.smart_highlight = None;
                return false;
            };
            self.smart_highlight = Some(SmartHighlight {
                revision,
                query,
                searcher,
                scans: search_ranges(shown, text)
                    .into_iter()
                    .map(Scan::new)
                    .collect(),
                matches: RangeSet::new(),
            });
        }
        let Some(smart) = self.smart_highlight.as_mut() else {
            return false;
        };
        if smart.is_done() {
            return false;
        }
        let deadline = Instant::now() + BUDGET;
        for scan in &mut smart.scans {
            if Instant::now() >= deadline {
                break;
            }
            scan.run(&smart.searcher, text, || Instant::now() >= deadline);
        }
        smart.matches = RangeSet::from_sorted(
            smart
                .scans
                .iter()
                .flat_map(|scan| scan.matches().iter().map(|range| (range.clone(), ()))),
        );
        !smart.is_done()
    }

    /// What to highlight for the primary selection, if smart highlighting is on, the file is
    /// within the large file limit and the selection qualifies.
    fn smart_highlight_query(&self, text: &Rope, cx: &App) -> Option<Query> {
        let settings = &AppState::global(cx).settings;
        let smart = &settings.highlighting.smart;
        let limit = u64::from(settings.files.large_file_limit_mb) << 20;
        if !smart.enabled || text.len() as u64 > limit {
            return None;
        }
        let (match_case, whole_word) = if smart.use_find_options {
            let options = cx
                .try_global::<ActiveFindOptions>()
                .map(|active| active.0)
                .unwrap_or_default();
            (options.match_case, options.whole_word)
        } else {
            (smart.match_case, smart.whole_word)
        };
        let primary = self.selection.primary();
        if primary.len() > MAX_TOKEN {
            return None;
        }
        let token = smart_highlight_token(text, primary.from()..primary.to(), whole_word)?;
        Some(Query {
            pattern: text.slice(token).to_string(),
            match_case,
            whole_word,
            ..Query::default()
        })
    }
}

/// The ranges to search for `shown`: each extended by [`MARGIN`] on both sides (on character
/// boundaries), merged where they meet.
fn search_ranges(shown: &[ByteRange<usize>], text: &Rope) -> Vec<ByteRange<usize>> {
    let mut ranges: Vec<ByteRange<usize>> = Vec::new();
    for range in shown {
        let start = text.floor_char_boundary(range.start.saturating_sub(MARGIN));
        let end = text.ceil_char_boundary((range.end + MARGIN).min(text.len()));
        match ranges.last_mut() {
            Some(last) if start <= last.end => last.end = last.end.max(end),
            _ => ranges.push(start..end),
        }
    }
    ranges
}

#[cfg(test)]
#[allow(
    clippy::single_range_in_vec_init,
    reason = "decorations are lists of byte ranges, some with one range"
)]
mod tests {
    use birchpad_core::{Range, Selection};
    use gpui_kit::{
        Entity, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, TestAppContext,
        VisualTestContext, point,
    };

    use super::*;
    use crate::workspace::Workspace;
    use crate::workspace::tests::{document_start, open_workspace};

    fn view(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Entity<EditorView> {
        workspace.read_with(cx, |workspace, cx| workspace.active_view(cx).unwrap())
    }

    /// The smart highlights of the last frame.
    fn highlights(
        workspace: &Entity<Workspace>,
        cx: &mut VisualTestContext,
    ) -> Vec<ByteRange<usize>> {
        let view = view(workspace, cx);
        view.read_with(cx, |view, _| {
            view.smart_highlight
                .as_ref()
                .map(|smart| smart.matches.iter().map(|(range, _)| range).collect())
                .unwrap_or_default()
        })
    }

    fn select(workspace: &Entity<Workspace>, range: ByteRange<usize>, cx: &mut VisualTestContext) {
        let view = view(workspace, cx);
        view.update(cx, |view, cx| {
            view.selection = Selection::single(Range::new(range.start, range.end));
            cx.notify();
        });
        cx.run_until_parked();
    }

    fn settings(cx: &mut VisualTestContext, change: impl FnOnce(&mut birchpad_config::Settings)) {
        cx.update(|window, cx| {
            change(&mut cx.global_mut::<AppState>().settings);
            window.refresh();
        });
        cx.run_until_parked();
    }

    #[gpui_kit::test]
    fn double_clicking_a_word_highlights_its_occurrences(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        cx.simulate_input("cat dog cat\ncatalog Cat cat");
        cx.simulate_keystrokes(document_start());
        // Double click inside the first "cat".
        let view = view(&workspace, cx);
        let position = view.read_with(cx, |view, _| {
            let layout = view.layout.as_ref().expect("a frame was drawn");
            let row = &layout.rows[0];
            point(
                row.x_for(1).unwrap(),
                row.y + layout.metrics.line_height / 2.,
            )
        });
        let (button, modifiers) = (MouseButton::Left, Modifiers::none());
        for click_count in [1, 2] {
            cx.simulate_event(MouseDownEvent {
                position,
                modifiers,
                button,
                click_count,
                first_mouse: false,
            });
            cx.simulate_event(MouseUpEvent {
                position,
                modifiers,
                button,
                click_count,
            });
        }
        cx.run_until_parked();
        assert_eq!(
            view.read_with(cx, |view, _| view.selection.primary()),
            Range::new(0, 3)
        );
        // Whole words, ignoring case, by default.
        assert_eq!(highlights(&workspace, cx), [0..3, 8..11, 20..23, 24..27]);

        // Typing replaces the selection: nothing is selected, nothing highlighted.
        cx.simulate_input("x");
        assert!(highlights(&workspace, cx).is_empty());
    }

    #[gpui_kit::test]
    fn settings_choose_what_is_highlighted(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        cx.simulate_input("cat dog cat\ncatalog Cat cat");
        // Part of a word: only with whole word off, then anywhere.
        select(&workspace, 12..15, cx);
        assert!(highlights(&workspace, cx).is_empty());
        settings(cx, |s| s.highlighting.smart.whole_word = false);
        assert_eq!(
            highlights(&workspace, cx),
            [0..3, 8..11, 12..15, 20..23, 24..27]
        );
        settings(cx, |s| s.highlighting.smart.match_case = true);
        assert_eq!(highlights(&workspace, cx), [0..3, 8..11, 12..15, 24..27]);
        // Text over two lines is never highlighted.
        select(&workspace, 8..15, cx);
        assert!(highlights(&workspace, cx).is_empty());

        // The find panel's options instead (match case and whole word off by default).
        select(&workspace, 20..23, cx);
        settings(cx, |s| s.highlighting.smart.use_find_options = true);
        assert_eq!(
            highlights(&workspace, cx),
            [0..3, 8..11, 12..15, 20..23, 24..27]
        );

        settings(cx, |s| s.highlighting.smart.enabled = false);
        assert!(highlights(&workspace, cx).is_empty());
    }

    #[gpui_kit::test]
    fn large_files_are_not_smart_highlighted(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        let text = "word other word\n".repeat(10);
        let view = view(&workspace, cx);
        view.update(cx, |view, cx| {
            view.insert(&text, crate::editor::LastEdit::None, cx)
        });
        select(&workspace, 0..4, cx);
        assert_eq!(highlights(&workspace, cx).len(), 20);
        settings(cx, |s| s.files.large_file_limit_mb = 0);
        assert!(highlights(&workspace, cx).is_empty());
    }

    #[test]
    fn search_ranges_extend_and_merge() {
        let text = Rope::from_str(&"x".repeat(100_000));
        assert_eq!(
            search_ranges(&[50_000..50_010, 60_000..60_010, 99_000..99_500], &text),
            [50_000 - MARGIN..60_010 + MARGIN, 99_000 - MARGIN..100_000]
        );
        assert_eq!(search_ranges(&[10..20], &text), [0..20 + MARGIN]);
    }

    /// `cargo test -p birchpad --release smart_highlighting_cost -- --ignored --nocapture`
    #[gpui_kit::test]
    #[ignore = "performance check; run in release"]
    fn smart_highlighting_cost(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        let line = "let value = compute(value, other_value) + value * 2; // value again value\n";
        let text = line.repeat(10_000);
        let view = view(&workspace, cx);
        view.update(cx, |view, cx| {
            view.insert(&text, crate::editor::LastEdit::None, cx)
        });
        // Scroll to the middle and select a word that occurs six times per line.
        let middle = text.len() / 2;
        let start = middle - middle % line.len() + 4;
        select(&workspace, start..start + 5, cx);
        let shown = view.read_with(cx, |view, _| {
            let layout = view.layout.as_ref().unwrap();
            let first = layout.rows.first().unwrap().row.range.start;
            let last = layout.rows.last().unwrap().row.range.end;
            vec![first..last]
        });
        let found = highlights(&workspace, cx).len();
        let rope = view.read_with(cx, |view, cx| view.text(cx).clone());

        let rounds = 1000;
        let started = Instant::now();
        for _ in 0..rounds {
            view.update(cx, |view, cx| {
                view.smart_highlight = None;
                view.update_smart_highlight(&shown, &rope, cx);
            });
        }
        let cold = started.elapsed() / rounds;
        let started = Instant::now();
        for _ in 0..rounds {
            view.update(cx, |view, cx| {
                view.update_smart_highlight(&shown, &rope, cx)
            });
        }
        let cached = started.elapsed() / rounds;

        let frame = |cx: &mut VisualTestContext| {
            let started = Instant::now();
            for _ in 0..200 {
                view.update(cx, |_, cx| cx.notify());
                cx.run_until_parked();
            }
            started.elapsed() / 200
        };
        let with_smart = frame(cx);
        settings(cx, |s| s.highlighting.smart.enabled = false);
        let without = frame(cx);
        eprintln!(
            "10 000 lines, {} bytes shown, {found} highlights: search {cold:?} (cached {cached:?}); \
             frame {with_smart:?} with smart highlighting, {without:?} without",
            shown[0].len()
        );
    }
}
