//! Incremental search, as Notepad++'s (Search > Incremental Search, Ctrl+Alt+I): a bar at the
//! bottom of the window that finds the text while it is typed.
//!
//! Each change of the text searches again from the start of the selection, so the match grows
//! with the text. Enter, or the > button, finds the next match, and Shift+Enter, or <, the
//! previous one; the search wraps around and says so. The field turns red when nothing matches.
//! Highlight all marks every match of the active document in Notepad++'s "Incremental
//! highlight all" blue and counts them. Escape, or the close button, removes the highlights and
//! returns to the text.

use birchpad_core::search::{Direction, Query, SearchMode, Searcher};
use gpui_kit::component::IconName;
use gpui_kit::component::Sizable;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::{
    App, AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable, Subscription,
    WeakEntity, Window, div, prelude::*, px, rgb,
};

use crate::buffer::Buffer;
use crate::commands::CommandRegistry;
use crate::find::plural;
use crate::workspace::Workspace;

pub(crate) fn register_commands(registry: &mut CommandRegistry) {
    registry.workspace("search.incremental", |this, (), window, cx| {
        this.open_incremental(window, cx);
        Ok(())
    });
    registry.workspace("search.close-incremental", |this, (), window, cx| {
        this.close_incremental(window, cx);
        Ok(())
    });
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IncrementalEvent {
    /// The text or an option changed: search again from the start of the selection.
    Changed,
    Find(Direction),
    Close,
}

pub(crate) struct IncrementalBar {
    pub(crate) visible: bool,
    input: Entity<InputState>,
    /// The text last searched for: the field also reports a change after Enter, when nothing
    /// changed, which must not search again from the start of the match.
    last_text: String,
    pub(crate) highlight_all: bool,
    pub(crate) match_case: bool,
    pub(crate) whole_word: bool,
    /// What the last search says: that it wrapped around, or found nothing.
    status: Option<&'static str>,
    not_found: bool,
    /// How many matches Highlight all marked.
    count: Option<usize>,
    /// The buffer whose matches are highlighted.
    highlighted: Option<WeakEntity<Buffer>>,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<IncrementalEvent> for IncrementalBar {}

impl Focusable for IncrementalBar {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl IncrementalBar {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx));
        let events =
            cx.subscribe_in(
                &input,
                window,
                |this, _, event: &InputEvent, _, cx| match event {
                    InputEvent::Change => this.text_changed(cx),
                    InputEvent::PressEnter { shift, .. } => {
                        cx.emit(IncrementalEvent::Find(if *shift {
                            Direction::Backward
                        } else {
                            Direction::Forward
                        }))
                    }
                    _ => {}
                },
            );
        Self {
            visible: false,
            input,
            last_text: String::new(),
            highlight_all: false,
            match_case: false,
            whole_word: false,
            status: None,
            not_found: false,
            count: None,
            highlighted: None,
            focus_handle: cx.focus_handle(),
            _subscriptions: vec![events],
        }
    }

    fn query(&self, cx: &App) -> Query {
        Query {
            pattern: self.input.read(cx).value().to_string(),
            match_case: self.match_case,
            whole_word: self.whole_word,
            mode: SearchMode::Normal,
            dot_matches_newline: false,
        }
    }

    /// Searches again if the text is not the one last searched for.
    fn text_changed(&mut self, cx: &mut Context<Self>) {
        let text = self.input.read(cx).value().to_string();
        if text != self.last_text {
            self.last_text = text;
            cx.emit(IncrementalEvent::Changed);
        }
    }

    /// Shows the bar, with `text` to search for if given, and focuses the field.
    fn show(&mut self, text: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        self.visible = true;
        if let Some(text) = text {
            self.last_text.clone_from(&text);
            self.input
                .update(cx, |input, cx| input.set_value(text, window, cx));
        }
        self.input.update(cx, |input, cx| {
            input.focus(window, cx);
            input.select_all(window, cx);
        });
        cx.notify();
    }

    /// Sets the text as if typed, for tests: the field's own change events need a keyboard.
    #[cfg(test)]
    pub(crate) fn type_text(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.input
            .update(cx, |input, cx| input.set_value(text, window, cx));
        self.text_changed(cx);
    }

    #[cfg(test)]
    pub(crate) fn state(&self) -> (Option<&'static str>, bool, Option<usize>) {
        (self.status, self.not_found, self.count)
    }

    fn option_box(
        &self,
        id: &'static str,
        label: &'static str,
        checked: bool,
        set: fn(&mut Self, bool),
        cx: &Context<Self>,
    ) -> Checkbox {
        let this = cx.entity().downgrade();
        Checkbox::new(id)
            .label(label)
            .checked(checked)
            .small()
            .on_click(move |checked, _, cx| {
                let checked = *checked;
                this.update(cx, |this, cx| {
                    set(this, checked);
                    cx.emit(IncrementalEvent::Changed);
                    cx.notify();
                })
                .ok();
            })
    }
}

impl Render for IncrementalBar {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let emit = |event: IncrementalEvent| cx.listener(move |_, _, _, cx| cx.emit(event));
        let failed = self.not_found;
        div()
            .id("incremental-bar")
            .key_context("IncrementalBar")
            .track_focus(&self.focus_handle)
            .flex()
            .flex_row()
            .flex_none()
            .items_center()
            .gap_2()
            .px_3()
            .py_1()
            .border_t_1()
            .border_color(crate::theme::paint(crate::theme::ui().border))
            .bg(crate::theme::paint(crate::theme::ui().surface))
            .text_size(px(13.))
            .child(
                Button::new("close-incremental")
                    .small()
                    .ghost()
                    .icon(IconName::Close)
                    .on_click(emit(IncrementalEvent::Close)),
            )
            .child("Find:")
            .child(
                Input::new(&self.input)
                    .id("incremental-input")
                    .w(px(240.))
                    .small()
                    .when(failed, |input| {
                        input.bg(crate::theme::paint(crate::theme::ui().error_background))
                    }),
            )
            .child(
                Button::new("incremental-previous")
                    .small()
                    .label("<")
                    .on_click(emit(IncrementalEvent::Find(Direction::Backward))),
            )
            .child(
                Button::new("incremental-next")
                    .small()
                    .label(">")
                    .on_click(emit(IncrementalEvent::Find(Direction::Forward))),
            )
            .child(self.option_box(
                "incremental-highlight",
                "Highlight all",
                self.highlight_all,
                |this, on| this.highlight_all = on,
                cx,
            ))
            .child(self.option_box(
                "incremental-case",
                "Match case",
                self.match_case,
                |this, on| this.match_case = on,
                cx,
            ))
            .child(self.option_box(
                "incremental-word",
                "Whole word",
                self.whole_word,
                |this, on| this.whole_word = on,
                cx,
            ))
            .children(self.status.map(|status| {
                div()
                    .text_color(rgb(if failed { 0xcf222e } else { 0x57606a }))
                    .child(status)
            }))
            .children(
                self.count
                    .map(|count| div().child(plural(count, "match", "matches"))),
            )
    }
}

impl Workspace {
    /// Ctrl+Alt+I: shows the bar, with the selection as the text to find if it is on one line.
    fn open_incremental(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.active_view(cx).and_then(|view| {
            let view = view.read(cx);
            let range = view.selection.primary();
            let text = view.text(cx).slice(range.from()..range.to()).to_string();
            (!text.is_empty() && !text.contains(['\n', '\r'])).then_some(text)
        });
        let search = text.is_some();
        self.incremental
            .update(cx, |bar, cx| bar.show(text, window, cx));
        if search {
            self.incremental_search(None, cx);
        }
        cx.notify();
    }

    /// Escape: hides the bar and its highlights, and returns to the text.
    fn close_incremental(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.clear_incremental_highlights(cx);
        self.incremental.update(cx, |bar, cx| {
            bar.visible = false;
            bar.status = None;
            bar.not_found = false;
            bar.count = None;
            cx.notify();
        });
        if let Some(view) = self.active_view(cx) {
            let focus = view.read(cx).focus_handle.clone();
            window.focus(&focus, cx);
        }
        cx.notify();
    }

    pub(crate) fn on_incremental_event(
        &mut self,
        _: &Entity<IncrementalBar>,
        event: &IncrementalEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            IncrementalEvent::Changed => self.incremental_search(None, cx),
            IncrementalEvent::Find(direction) => self.incremental_search(Some(*direction), cx),
            IncrementalEvent::Close => self.close_incremental(window, cx),
        }
    }

    fn clear_incremental_highlights(&mut self, cx: &mut Context<Self>) {
        let highlighted = self.incremental.update(cx, |bar, _| bar.highlighted.take());
        if let Some(buffer) = highlighted.and_then(|buffer| buffer.upgrade()) {
            buffer.update(cx, |buffer, cx| {
                buffer.update_marks(cx, |marks, _| marks.incremental.clear());
            });
        }
    }

    /// Searches again: from the start of the selection when the text changed (`None`), or for
    /// the next or previous match.
    pub(crate) fn incremental_search(&mut self, step: Option<Direction>, cx: &mut Context<Self>) {
        self.clear_incremental_highlights(cx);
        let Some(view) = self.active_view(cx) else {
            return;
        };
        let bar = self.incremental.read(cx);
        let highlight_all = bar.highlight_all;
        let Ok(searcher) = Searcher::new(&bar.query(cx)) else {
            // Nothing typed: nothing found, and the caret where the match started.
            if step.is_none() {
                view.update(cx, |view, cx| {
                    let start = view.selection.primary().from();
                    view.go_to(start, cx);
                });
            }
            self.incremental.update(cx, |bar, cx| {
                bar.status = None;
                bar.not_found = false;
                bar.count = None;
                cx.notify();
            });
            return;
        };
        let (text, selection, buffer) = {
            let view = view.read(cx);
            let range = view.selection.primary();
            (
                view.text(cx).clone(),
                range.from()..range.to(),
                view.buffer.clone(),
            )
        };
        let (from, direction) = match step {
            None => (selection.start, Direction::Forward),
            Some(Direction::Forward) => (selection.end, Direction::Forward),
            Some(Direction::Backward) => (selection.start, Direction::Backward),
        };
        let found = searcher.find(&text, from, direction, true);
        let status = match &found {
            None => Some("Phrase not found"),
            Some(found) => match direction {
                Direction::Forward if found.start < from => {
                    Some("Reached end of page, continued from top")
                }
                Direction::Backward if found.start >= from => {
                    Some("Reached top of page, continued from bottom")
                }
                _ => None,
            },
        };
        if let Some(found) = found.clone() {
            view.update(cx, |view, cx| view.select_range(found, cx));
        }
        let count = highlight_all.then(|| {
            let matches = searcher.find_all(&text);
            let count = matches.len();
            buffer.update(cx, |buffer, cx| {
                buffer.update_marks(cx, |marks, _| marks.incremental.insert_all(matches, ()));
            });
            count
        });
        let highlighted = highlight_all.then(|| buffer.downgrade());
        self.incremental.update(cx, |bar, cx| {
            bar.status = status;
            bar.not_found = found.is_none();
            bar.count = count;
            bar.highlighted = highlighted;
            cx.notify();
        });
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::single_range_in_vec_init,
        reason = "expected highlights written out"
    )]

    use birchpad_core::{Range, Selection};
    use gpui_kit::{TestAppContext, VisualTestContext};

    use super::*;
    use crate::workspace::tests::{document_start, open_workspace, secondary};

    fn type_text(workspace: &Entity<Workspace>, text: &str, cx: &mut VisualTestContext) {
        workspace.update_in(cx, |workspace, window, cx| {
            workspace
                .incremental
                .update(cx, |bar, cx| bar.type_text(text, window, cx));
        });
        cx.run_until_parked();
    }

    fn selection(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> (usize, usize) {
        workspace.read_with(cx, |workspace, cx| {
            let range = workspace
                .active_view(cx)
                .unwrap()
                .read(cx)
                .selection
                .primary();
            (range.anchor, range.head)
        })
    }

    fn state(
        workspace: &Entity<Workspace>,
        cx: &mut VisualTestContext,
    ) -> (Option<&'static str>, bool, Option<usize>) {
        workspace.read_with(cx, |workspace, cx| workspace.incremental.read(cx).state())
    }

    fn highlights(
        workspace: &Entity<Workspace>,
        cx: &mut VisualTestContext,
    ) -> Vec<std::ops::Range<usize>> {
        workspace.read_with(cx, |workspace, cx| {
            let view = workspace.active_view(cx).unwrap();
            let buffer = view.read(cx).buffer.read(cx);
            buffer
                .marks()
                .incremental
                .iter()
                .map(|(range, _)| range)
                .collect()
        })
    }

    fn incremental() -> String {
        secondary("alt-i")
    }

    #[gpui_kit::test]
    fn the_match_grows_with_the_text_and_enter_steps_through(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        cx.simulate_input("cat car cart\ncar");
        cx.simulate_keystrokes(document_start());
        cx.simulate_keystrokes(&incremental());
        type_text(&workspace, "c", cx);
        assert_eq!(selection(&workspace, cx), (0, 1));
        type_text(&workspace, "car", cx);
        assert_eq!(
            selection(&workspace, cx),
            (4, 7),
            "from the start of the match"
        );
        type_text(&workspace, "cart", cx);
        assert_eq!(selection(&workspace, cx), (8, 12));
        assert_eq!(state(&workspace, cx), (None, false, None));

        // Enter: the next one, wrapping around; Shift+Enter back.
        type_text(&workspace, "car", cx);
        assert_eq!(
            selection(&workspace, cx),
            (8, 11),
            "cart still starts with car"
        );
        cx.simulate_keystrokes("enter");
        assert_eq!(selection(&workspace, cx), (13, 16));
        cx.simulate_keystrokes("enter");
        assert_eq!(selection(&workspace, cx), (4, 7));
        assert_eq!(
            state(&workspace, cx).0,
            Some("Reached end of page, continued from top")
        );
        cx.simulate_keystrokes("shift-enter");
        assert_eq!(selection(&workspace, cx), (13, 16));
        assert_eq!(
            state(&workspace, cx).0,
            Some("Reached top of page, continued from bottom")
        );
    }

    #[gpui_kit::test]
    fn nothing_found_turns_red_and_an_empty_field_finds_nothing(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        cx.simulate_input("alpha beta");
        cx.simulate_keystrokes(document_start());
        cx.simulate_keystrokes(&incremental());
        type_text(&workspace, "bet", cx);
        assert_eq!(selection(&workspace, cx), (6, 9));
        type_text(&workspace, "betx", cx);
        assert_eq!(
            state(&workspace, cx),
            (Some("Phrase not found"), true, None)
        );
        assert_eq!(selection(&workspace, cx), (6, 9), "the last match stays");
        // Emptied: no message, and the caret where the match was.
        type_text(&workspace, "", cx);
        assert_eq!(state(&workspace, cx), (None, false, None));
        assert_eq!(selection(&workspace, cx), (6, 6));
        // Enter on an empty field does nothing.
        cx.simulate_keystrokes("enter shift-enter");
        assert_eq!(selection(&workspace, cx), (6, 6));
    }

    #[gpui_kit::test]
    fn options_and_highlight_all(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        cx.simulate_input("Word word sword word");
        cx.simulate_keystrokes(document_start());
        cx.simulate_keystrokes(&incremental());
        let set = |change: fn(&mut IncrementalBar), cx: &mut VisualTestContext| {
            workspace.update(cx, |workspace, cx| {
                workspace.incremental.update(cx, |bar, cx| {
                    change(bar);
                    cx.emit(IncrementalEvent::Changed);
                });
            });
            cx.run_until_parked();
        };
        set(|bar| bar.highlight_all = true, cx);
        type_text(&workspace, "word", cx);
        assert_eq!(state(&workspace, cx), (None, false, Some(4)));
        assert_eq!(highlights(&workspace, cx), [0..4, 5..9, 11..15, 16..20]);
        set(|bar| bar.match_case = true, cx);
        assert_eq!(state(&workspace, cx).2, Some(3));
        assert_eq!(selection(&workspace, cx), (5, 9), "Word no longer matches");
        set(|bar| bar.whole_word = true, cx);
        assert_eq!(highlights(&workspace, cx), [5..9, 16..20], "not in sword");
        // Off: the highlights go, and so does the count.
        set(|bar| bar.highlight_all = false, cx);
        assert!(highlights(&workspace, cx).is_empty());
        assert_eq!(state(&workspace, cx).2, None);
        // Nothing found with Highlight all: zero, and nothing highlighted.
        set(|bar| bar.highlight_all = true, cx);
        type_text(&workspace, "none", cx);
        assert_eq!(
            state(&workspace, cx),
            (Some("Phrase not found"), true, Some(0))
        );
        assert!(highlights(&workspace, cx).is_empty());
    }

    #[gpui_kit::test]
    fn escape_closes_and_the_selection_becomes_the_text(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        cx.simulate_input("one two one");
        workspace.update(cx, |workspace, cx| {
            let view = workspace.active_view(cx).unwrap();
            view.update(cx, |view, cx| {
                view.selection = Selection::single(Range::new(4, 7));
                cx.notify();
            });
            workspace
                .incremental
                .update(cx, |bar, _| bar.highlight_all = true);
        });
        cx.simulate_keystrokes(&incremental());
        let (visible, text) = workspace.read_with(cx, |workspace, cx| {
            let bar = workspace.incremental.read(cx);
            (bar.visible, bar.query(cx).pattern)
        });
        assert!(visible);
        assert_eq!(text, "two");
        assert_eq!(
            selection(&workspace, cx),
            (4, 7),
            "the selection is its own match"
        );
        assert_eq!(highlights(&workspace, cx), [4..7]);

        cx.simulate_keystrokes("escape");
        let (visible, focused) = workspace.update_in(cx, |workspace, window, cx| {
            let view = workspace.active_view(cx).unwrap();
            let focused = view.read(cx).focus_handle.is_focused(window);
            (workspace.incremental.read(cx).visible, focused)
        });
        assert!(!visible);
        assert!(focused, "back in the text");
        assert!(
            highlights(&workspace, cx).is_empty(),
            "highlights go with the bar"
        );
        // A selection over two lines is not taken as the text; nor is an empty one.
        workspace.update(cx, |workspace, cx| {
            let view = workspace.active_view(cx).unwrap();
            view.update(cx, |view, _| view.selection = Selection::point(0));
        });
        cx.simulate_keystrokes(&incremental());
        let text = workspace.read_with(cx, |workspace, cx| {
            workspace.incremental.read(cx).query(cx).pattern
        });
        assert_eq!(text, "two", "the field keeps the last text");
        // Closing again when closed does nothing.
        workspace.update_in(cx, |workspace, window, cx| {
            workspace.close_incremental(window, cx);
            workspace.close_incremental(window, cx);
        });
    }

    #[gpui_kit::test]
    fn an_empty_document_and_the_same_text_again(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        cx.simulate_keystrokes(&incremental());
        type_text(&workspace, "x", cx);
        assert_eq!(
            state(&workspace, cx),
            (Some("Phrase not found"), true, None)
        );
        assert_eq!(selection(&workspace, cx), (0, 0));
        // Enter reports a change of the field without one: no search from the match's start.
        workspace.update_in(cx, |workspace, window, cx| {
            workspace.incremental.update(cx, |bar, cx| {
                bar.status = Some("kept");
                bar.type_text("x", window, cx);
            });
        });
        assert_eq!(state(&workspace, cx).0, Some("kept"), "not searched again");
    }
}
