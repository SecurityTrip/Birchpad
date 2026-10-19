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
use crate::ActiveFindOptions;
use birchpad_ui::AppState;

/// How long one frame may search.
const BUDGET: Duration = Duration::from_millis(2);
/// Bytes searched beyond the visible text on each side.
const MARGIN: usize = 16 * 1024;
/// Longer selections are not highlighted.
const MAX_TOKEN: usize = 64 * 1024;

/// The occurrences of the selected token around the visible text.
pub(crate) struct SmartHighlight {
    revision: RevisionId,
    query: Query,
    searcher: Searcher,
    /// One scan per searched range; sorted and disjoint.
    scans: Vec<Scan>,
    /// What the scans found so far.
    pub(crate) matches: RangeSet<()>,
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
    pub(crate) fn update_smart_highlight(
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
                .copied()
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
#[path = "../tests/unit/smart_highlight/tests.rs"]
mod tests;
