//! Find Next and Previous, Count, Find All and its results, and searches recorded in macros.

use anyhow::{Result, bail};
use birchpad_commands::Invocation;
use birchpad_core::motion::{next_boundary, prev_boundary};
use birchpad_core::search::{Direction, Query, SearchMode, Searcher, line_hits, token_at};
use gpui_kit::{App, Context, Entity, Window};

use crate::editor::EditorView;
use crate::search_results::{
    FileResults, ResultLocation, SearchResults, SearchResultsEvent, SearchRun,
};
use crate::workspace::Workspace;

use super::{FindOptions, SearchArgs, mode_name, plural};

impl Workspace {
    /// Find Next / Find Previous (F3 / Shift+F3), from the current selection.
    pub fn find(&mut self, direction: Direction, _: &mut Window, cx: &mut Context<Self>) {
        let Some(view) = self.active_view(cx) else {
            return;
        };
        let Some((searcher, query)) = self.searcher(cx) else {
            return;
        };
        self.record_search("search.find-text", direction, None, false, cx);
        let wrap_around = self.find_bar.read(cx).options.wrap_around;
        self.find_with(&view, &searcher, &query, wrap_around, direction, cx);
    }

    /// Find (Volatile) Next / Previous (Ctrl+Alt+F3 / Ctrl+Alt+Shift+F3): as Select and Find,
    /// but the find panel keeps its own search text.
    pub(super) fn find_volatile(&mut self, direction: Direction, cx: &mut Context<Self>) {
        let Some(view) = self.active_view(cx) else {
            return;
        };
        let (token, selected, word) = {
            let view = view.read(cx);
            let range = view.selection.primary();
            let text = view.text(cx);
            let Some(token) = token_at(text, range.from()..range.to()) else {
                return;
            };
            (
                text.slice(token.clone()).to_string(),
                token,
                range.is_empty(),
            )
        };
        if token.contains(['\n', '\r']) {
            return;
        }
        let options = self.find_bar.read(cx).options;
        let query = Query {
            pattern: token,
            match_case: options.match_case,
            whole_word: word || options.whole_word,
            mode: SearchMode::Normal,
            dot_matches_newline: false,
        };
        let Ok(searcher) = Searcher::new(&query) else {
            return;
        };
        view.update(cx, |view, cx| view.select_range(selected, cx));
        self.find_with(&view, &searcher, &query, options.wrap_around, direction, cx);
    }

    /// Selects the next (or previous) match of `searcher` from the selection of `view`.
    pub(super) fn find_with(
        &mut self,
        view: &Entity<EditorView>,
        searcher: &Searcher,
        query: &Query,
        wrap_around: bool,
        direction: Direction,
        cx: &mut Context<Self>,
    ) {
        let options = FindOptions {
            wrap_around,
            ..FindOptions::default()
        };
        let (text, selection) = {
            let view = view.read(cx);
            let range = view.selection.primary();
            (view.text(cx).clone(), range.from()..range.to())
        };
        let from = match direction {
            Direction::Forward => selection.end,
            Direction::Backward => selection.start,
        };
        let mut found = searcher.find(&text, from, direction, options.wrap_around);
        // An empty match where the caret already is (a regular expression like `^`): step over
        // it, or the search would never move.
        if let Some(range) = &found
            && range.is_empty()
            && selection.is_empty()
            && range.start == from
        {
            found = match direction {
                Direction::Forward if from < text.len() => searcher.find(
                    &text,
                    next_boundary(&text, from),
                    direction,
                    options.wrap_around,
                ),
                Direction::Backward if from > 0 => searcher.find(
                    &text,
                    prev_boundary(&text, from),
                    direction,
                    options.wrap_around,
                ),
                _ if options.wrap_around => {
                    let restart = match direction {
                        Direction::Forward => 0,
                        Direction::Backward => text.len(),
                    };
                    searcher
                        .find(&text, restart, direction, false)
                        .filter(|again| again.start != from)
                }
                _ => None,
            };
        }
        match found {
            Some(found) => {
                let wrapped = match direction {
                    Direction::Forward => found.start < from,
                    Direction::Backward => found.start >= from,
                };
                let message = wrapped.then(|| match direction {
                    Direction::Forward => {
                        "Reached the end of the document; continued from the top.".to_owned()
                    }
                    Direction::Backward => {
                        "Reached the start of the document; continued from the bottom.".to_owned()
                    }
                });
                self.report(message, false, cx);
                view.update(cx, |view, cx| view.select_range(found, cx));
            }
            None => {
                if !self.report_failure(searcher, cx) {
                    self.report(Some(t!("find.not-found", text = query.pattern)), true, cx);
                }
            }
        }
    }

    /// Select and Find Next / Previous (Ctrl+F3 / Ctrl+Shift+F3): searches for the selection, or
    /// the word at the caret, as a whole word if it is the word at the caret.
    pub(super) fn select_and_find(
        &mut self,
        direction: Direction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(view) = self.active_view(cx) else {
            return;
        };
        let (token, selected) = {
            let view = view.read(cx);
            let range = view.selection.primary();
            let text = view.text(cx);
            let Some(token) = token_at(text, range.from()..range.to()) else {
                return;
            };
            (text.slice(token.clone()).to_string(), token)
        };
        if token.contains(['\n', '\r']) {
            return;
        }
        view.update(cx, |view, cx| view.select_range(selected, cx));
        self.find_bar.update(cx, |bar, cx| {
            bar.find_input
                .update(cx, |input, cx| input.set_value(token, window, cx));
            bar.set_options(|o| o.mode = SearchMode::Normal, cx);
        });
        self.find(direction, window, cx);
    }

    /// Count: the matches in the document or the selection.
    pub(super) fn count(&mut self, cx: &mut Context<Self>) {
        let Some(view) = self.active_view(cx) else {
            return;
        };
        let Some((searcher, _)) = self.searcher(cx) else {
            return;
        };
        let scope = self.scope(&view, cx);
        let text = view.read(cx).text(cx).clone();
        let count = searcher.find_all_in(&text, scope.range.clone()).len();
        if self.report_failure(&searcher, cx) {
            return;
        }
        self.report(
            Some(format!(
                "{}: {} {}",
                t!("find.count"),
                plural(count, "match", "matches"),
                scope.describe()
            )),
            count == 0,
            cx,
        );
    }

    /// One view per open document: views of one buffer share its text. The active document
    /// comes first.
    pub fn document_views(&self, cx: &App) -> Vec<Entity<EditorView>> {
        let mut views = self.all_views(cx);
        if let Some(active) = self.active_view(cx) {
            views.retain(|view| *view != active);
            views.insert(0, active);
        }
        let mut seen = Vec::new();
        views.retain(|view| {
            let buffer = view.read(cx).buffer.entity_id();
            let new = !seen.contains(&buffer);
            seen.push(buffer);
            new
        });
        views
    }

    /// Find All in Current Document (in the selection with In selection) or in All Opened
    /// Documents: lists the lines with matches in the search results panel.
    pub(super) fn find_all(&mut self, all_documents: bool, cx: &mut Context<Self>) {
        let Some((searcher, query)) = self.searcher(cx) else {
            return;
        };
        let views = if all_documents {
            self.document_views(cx)
        } else {
            self.active_view(cx).into_iter().collect()
        };
        let searched = views.len();
        let mut files = Vec::new();
        for view in views {
            let range = if all_documents {
                0..view.read(cx).text(cx).len()
            } else {
                self.scope(&view, cx).range
            };
            let view = view.read(cx);
            let buffer = view.buffer.read(cx);
            let text = view.text(cx);
            let matches = searcher.find_all_in(text, range);
            if matches.is_empty() {
                continue;
            }
            let name = buffer
                .path()
                .map_or_else(|| buffer.display_name(), |path| path.display().to_string());
            files.push(FileResults::new(
                ResultLocation::Buffer(view.buffer.downgrade()),
                name,
                line_hits(text, &matches),
                matches.len(),
            ));
        }
        if self.report_failure(&searcher, cx) {
            return;
        }
        let run = SearchRun::new(&query.pattern, files, searched);
        let hits: usize = run.files.iter().map(|file| file.hits).sum();
        self.search_results
            .update(cx, |results, cx| results.add(run, cx));
        self.report(
            Some(format!(
                "{}: {}",
                t!("find.find-all"),
                plural(hits, "hit", "hits")
            )),
            hits == 0,
            cx,
        );
        cx.notify();
    }

    /// F4 / Shift+F4: the next or previous search result.
    pub(super) fn step_result(&mut self, forward: bool, _: &mut Window, cx: &mut Context<Self>) {
        let found = self
            .search_results
            .update(cx, |results, cx| results.step(forward, cx));
        if !found {
            self.report(Some(t!("find.no-search-results")), true, cx);
        }
        cx.notify();
    }

    pub fn on_search_results_event(
        &mut self,
        _: &Entity<SearchResults>,
        event: &SearchResultsEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            SearchResultsEvent::Close => {
                self.search_results.update(cx, |results, cx| {
                    results.visible = false;
                    cx.notify();
                });
                cx.notify();
            }
            SearchResultsEvent::Open {
                location,
                line,
                target,
            } => self.open_result(location, *line, target.clone(), window, cx),
        }
    }

    /// Goes to a search result: the tab of its document, and the match on its line.
    pub(super) fn open_result(
        &mut self,
        location: &ResultLocation,
        line: usize,
        target: std::ops::Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = match location {
            ResultLocation::Buffer(buffer) => {
                let Some(buffer) = buffer.upgrade() else {
                    self.report(Some(t!("find.the-document-was-closed")), true, cx);
                    return;
                };
                let active = self
                    .active_view(cx)
                    .filter(|view| view.read(cx).buffer == buffer);
                let Some(view) = active.or_else(|| {
                    self.all_views(cx)
                        .into_iter()
                        .find(|view| view.read(cx).buffer == buffer)
                }) else {
                    self.report(Some(t!("find.the-document-was-closed")), true, cx);
                    return;
                };
                self.activate_view(&view, window, cx);
                view
            }
            ResultLocation::File(path) => {
                self.open_path(path, window, cx);
                let Some(view) = self.view_for_path(path, cx) else {
                    return;
                };
                view
            }
        };
        view.update(cx, |view, cx| view.select_in_line(line, target, cx));
        let focus = view.read(cx).focus_handle.clone();
        window.focus(&focus, cx);
        cx.notify();
    }
}

impl Workspace {
    /// Notes a search, with the find panel's text and options, if a macro is being recorded.
    pub(super) fn record_search(
        &mut self,
        command: &str,
        direction: Direction,
        replacement: Option<&str>,
        all: bool,
        cx: &mut Context<Self>,
    ) {
        let bar = self.find_bar.read(cx);
        let options = bar.options;
        let args = SearchArgs {
            text: bar.find_input.read(cx).value().to_string(),
            match_case: options.match_case,
            whole_word: options.whole_word,
            mode: mode_name(options.mode).to_owned(),
            dot_all: options.dot_matches_newline,
            backward: direction == Direction::Backward,
            wrap: options.wrap_around,
            with: replacement.map(str::to_owned),
            all,
        };
        let args = serde_json::to_value(args).expect("search arguments serialize");
        crate::macros::record_always(&Invocation::with_args(command, args), cx);
    }

    /// A recorded search: the find panel takes its text and options, as if they were typed.
    pub(super) fn take_search(
        &mut self,
        args: &SearchArgs,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let mode = match args.mode.as_str() {
            "normal" => SearchMode::Normal,
            "extended" => SearchMode::Extended,
            "regex" => SearchMode::Regex,
            other => bail!("{}", t!("find.unknown-search-mode", mode = other)),
        };
        let text = args.text.clone();
        let with = args.with.clone();
        self.find_bar.update(cx, |bar, cx| {
            bar.find_input
                .update(cx, |input, cx| input.set_value(text, window, cx));
            if let Some(with) = with {
                bar.replace_input
                    .update(cx, |input, cx| input.set_value(with, window, cx));
            }
            let (match_case, whole_word, dot_all, wrap) =
                (args.match_case, args.whole_word, args.dot_all, args.wrap);
            bar.set_options(
                |options| {
                    options.match_case = match_case;
                    options.whole_word = whole_word;
                    options.mode = mode;
                    options.dot_matches_newline = dot_all;
                    options.wrap_around = wrap;
                },
                cx,
            );
        });
        Ok(())
    }
}
