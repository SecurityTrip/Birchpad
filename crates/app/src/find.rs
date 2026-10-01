//! Find, Replace and Go To.
//!
//! The find panel sits at the bottom of the window, above the status bar (a simple panel; the
//! full Find dialog with Find in Files comes in phase 3). F3 and Shift+F3 repeat the last search
//! even when the panel is closed. Searching goes through `birchpad_core::search`, which reads the
//! rope directly.

use anyhow::{Context as _, Result, anyhow, bail};
use birchpad_core::motion::{line_count, line_of, line_range};
use birchpad_core::search::{Direction, Query, Searcher};
use birchpad_core::{Edit, Rope, Transaction};
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::dialog::{DialogClose, DialogFooter};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{IconName, Selectable as _, Sizable};
use gpui_kit::{
    App, AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable, Subscription,
    Window, div, prelude::*, px, rgb,
};

use crate::commands::CommandRegistry;
use crate::editor::EditorView;
use crate::workspace::Workspace;

pub(crate) fn register_commands(registry: &mut CommandRegistry) {
    registry.workspace("search.find", |this, (), window, cx| {
        this.open_find(false, window, cx);
        Ok(())
    });
    registry.workspace("search.replace", |this, (), window, cx| {
        this.open_find(true, window, cx);
        Ok(())
    });
    registry.workspace("search.find-next", |this, (), window, cx| {
        this.find(Direction::Forward, window, cx);
        Ok(())
    });
    registry.workspace("search.find-previous", |this, (), window, cx| {
        this.find(Direction::Backward, window, cx);
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

/// Options of the find panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FindOptions {
    pub(crate) match_case: bool,
    pub(crate) whole_word: bool,
    pub(crate) wrap_around: bool,
    pub(crate) backward: bool,
}

impl Default for FindOptions {
    fn default() -> Self {
        Self {
            match_case: false,
            whole_word: false,
            // Notepad++'s default.
            wrap_around: true,
            backward: false,
        }
    }
}

pub(crate) enum FindBarEvent {
    Find(Direction),
    Replace,
    ReplaceAll,
    Close,
}

pub(crate) struct FindBar {
    pub(crate) visible: bool,
    pub(crate) replace_mode: bool,
    find_input: Entity<InputState>,
    replace_input: Entity<InputState>,
    pub(crate) options: FindOptions,
    /// A message from the last search, and whether it is a failure.
    status: Option<(String, bool)>,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<FindBarEvent> for FindBar {}

impl Focusable for FindBar {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl FindBar {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let find_input = cx.new(|cx| InputState::new(window, cx).placeholder("Find what"));
        let replace_input = cx.new(|cx| InputState::new(window, cx).placeholder("Replace with"));
        let find_events =
            cx.subscribe_in(&find_input, window, |this, _, event: &InputEvent, _, cx| {
                if let InputEvent::PressEnter { shift, .. } = event {
                    let direction = if *shift != this.options.backward {
                        Direction::Backward
                    } else {
                        Direction::Forward
                    };
                    cx.emit(FindBarEvent::Find(direction));
                }
            });
        let replace_events =
            cx.subscribe_in(&replace_input, window, |_, _, event: &InputEvent, _, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    cx.emit(FindBarEvent::Replace);
                }
            });
        Self {
            visible: false,
            replace_mode: false,
            find_input,
            replace_input,
            options: FindOptions::default(),
            status: None,
            focus_handle: cx.focus_handle(),
            _subscriptions: vec![find_events, replace_events],
        }
    }

    pub(crate) fn query(&self, cx: &App) -> Query {
        Query {
            pattern: self.find_input.read(cx).value().to_string(),
            match_case: self.options.match_case,
            whole_word: self.options.whole_word,
            ..Query::default()
        }
    }

    pub(crate) fn replacement(&self, cx: &App) -> String {
        self.replace_input.read(cx).value().to_string()
    }

    pub(crate) fn set_status(&mut self, status: Option<(String, bool)>, cx: &mut Context<Self>) {
        self.status = status;
        cx.notify();
    }

    /// Shows the panel, optionally with a new search text, and focuses the find field.
    pub(crate) fn show(
        &mut self,
        replace: bool,
        text: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.visible = true;
        self.replace_mode = replace;
        self.status = None;
        if let Some(text) = text {
            self.find_input
                .update(cx, |input, cx| input.set_value(text, window, cx));
        }
        self.find_input.update(cx, |input, cx| {
            input.focus(window, cx);
            input.select_all(window, cx);
        });
        cx.notify();
    }

    fn option_box(
        &self,
        id: &'static str,
        label: &'static str,
        checked: bool,
        set: fn(&mut FindOptions, bool),
        cx: &mut Context<Self>,
    ) -> Checkbox {
        let this = cx.entity().downgrade();
        Checkbox::new(id)
            .label(label)
            .checked(checked)
            .small()
            .on_click(move |checked, _, cx| {
                this.update(cx, |this, cx| {
                    set(&mut this.options, *checked);
                    cx.notify();
                })
                .ok();
            })
    }
}

impl Render for FindBar {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let options = self.options;
        let option_boxes = [
            self.option_box(
                "match-case",
                "Match case",
                options.match_case,
                |o, v| o.match_case = v,
                cx,
            ),
            self.option_box(
                "whole-word",
                "Match whole word only",
                options.whole_word,
                |o, v| o.whole_word = v,
                cx,
            ),
            self.option_box(
                "wrap-around",
                "Wrap around",
                options.wrap_around,
                |o, v| o.wrap_around = v,
                cx,
            ),
            self.option_box(
                "backward",
                "Backward direction",
                options.backward,
                |o, v| o.backward = v,
                cx,
            ),
        ];
        let emit = |event: fn() -> FindBarEvent| {
            cx.listener(move |_, _, _, cx: &mut Context<Self>| cx.emit(event()))
        };
        let find_row = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .child(div().w(px(64.)).child("Find:"))
            .child(
                Input::new(&self.find_input)
                    .id("find-input")
                    .w(px(320.))
                    .small(),
            )
            .child(
                Button::new("find-previous")
                    .small()
                    .label("Find Previous")
                    .on_click(emit(|| FindBarEvent::Find(Direction::Backward))),
            )
            .child(
                Button::new("find-next")
                    .small()
                    .label("Find Next")
                    .on_click(emit(|| FindBarEvent::Find(Direction::Forward))),
            )
            .children(option_boxes)
            .child(div().flex_1())
            .child(
                Button::new("close-find")
                    .small()
                    .icon(IconName::Close)
                    .on_click(emit(|| FindBarEvent::Close)),
            );
        let replace_row = self.replace_mode.then(|| {
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(div().w(px(64.)).child("Replace:"))
                .child(
                    Input::new(&self.replace_input)
                        .id("replace-input")
                        .w(px(320.))
                        .small(),
                )
                .child(
                    Button::new("replace")
                        .small()
                        .label("Replace")
                        .on_click(emit(|| FindBarEvent::Replace)),
                )
                .child(
                    Button::new("replace-all")
                        .small()
                        .label("Replace All")
                        .on_click(emit(|| FindBarEvent::ReplaceAll)),
                )
        });
        let status = self.status.clone().map(|(message, failed)| {
            div()
                .text_color(rgb(if failed { 0xcf222e } else { 0x57606a }))
                .child(message)
        });
        div()
            .id("find-bar")
            .key_context("FindBar")
            .track_focus(&self.focus_handle)
            .flex()
            .flex_col()
            .flex_none()
            .gap_1()
            .px_3()
            .py_2()
            .border_t_1()
            .border_color(rgb(0xd0d7de))
            .bg(rgb(0xf6f8fa))
            .text_size(px(13.))
            .child(find_row)
            .children(replace_row)
            .children(status)
    }
}

impl Workspace {
    /// Ctrl+F / Ctrl+H: shows the panel, filled with the selected text if it is on one line.
    fn open_find(&mut self, replace: bool, window: &mut Window, cx: &mut Context<Self>) {
        let selected = self.active_view(cx).and_then(|view| {
            let view = view.read(cx);
            let range = view.selection.primary();
            let text = view.text(cx).slice(range.from()..range.to()).to_string();
            (!text.is_empty() && !text.contains(['\n', '\r'])).then_some(text)
        });
        self.find_bar
            .update(cx, |bar, cx| bar.show(replace, selected, window, cx));
        cx.notify();
    }

    fn close_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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

    pub(crate) fn on_find_bar_event(
        &mut self,
        _: &Entity<FindBar>,
        event: &FindBarEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            FindBarEvent::Find(direction) => self.find(*direction, window, cx),
            FindBarEvent::Replace => self.replace(window, cx),
            FindBarEvent::ReplaceAll => self.replace_all(window, cx),
            FindBarEvent::Close => self.close_find(window, cx),
        }
    }

    fn report(&mut self, message: Option<String>, failed: bool, cx: &mut Context<Self>) {
        self.find_bar.update(cx, |bar, cx| {
            bar.set_status(message.map(|message| (message, failed)), cx);
        });
    }

    fn searcher(&mut self, cx: &mut Context<Self>) -> Option<(Searcher, Query)> {
        let query = self.find_bar.read(cx).query(cx);
        match Searcher::new(&query) {
            Ok(searcher) => Some((searcher, query)),
            Err(error) => {
                self.report(Some(format!("Find: {error}")), true, cx);
                None
            }
        }
    }

    /// Find Next / Find Previous (F3 / Shift+F3), from the current selection.
    pub(crate) fn find(&mut self, direction: Direction, _: &mut Window, cx: &mut Context<Self>) {
        let Some(view) = self.active_view(cx) else {
            return;
        };
        let Some((searcher, query)) = self.searcher(cx) else {
            return;
        };
        let options = self.find_bar.read(cx).options;
        let (text, from) = {
            let view = view.read(cx);
            let selection = view.selection.primary();
            let from = match direction {
                Direction::Forward => selection.to(),
                Direction::Backward => selection.from(),
            };
            (view.text(cx).clone(), from)
        };
        match searcher.find(&text, from, direction, options.wrap_around) {
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
                self.report(
                    Some(format!("Can't find the text \"{}\"", query.pattern)),
                    true,
                    cx,
                );
            }
        }
    }

    /// Replace: replaces the selection if it is a match, then finds the next one.
    fn replace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(view) = self.active_view(cx) else {
            return;
        };
        let Some((searcher, _)) = self.searcher(cx) else {
            return;
        };
        let replacement = self.find_bar.read(cx).replacement(cx);
        let (text, selected) = {
            let view = view.read(cx);
            let range = view.selection.primary();
            (view.text(cx).clone(), range.from()..range.to())
        };
        if !selected.is_empty() && searcher.is_match(&text, selected.clone()) {
            let caret = selected.start + replacement.len();
            let transaction =
                Transaction::from_edits(&text, [Edit::replace(selected, replacement)])
                    .expect("a match lies on character boundaries");
            let applied = view.update(cx, |view, cx| {
                let applied = view.apply_command_edit(transaction, cx);
                if applied {
                    view.go_to(caret, cx);
                }
                applied
            });
            if !applied {
                self.report(Some("The document is read-only".into()), true, cx);
                return;
            }
        }
        self.find(Direction::Forward, window, cx);
    }

    /// Replace All: every match in the document, as one undo step.
    fn replace_all(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let Some(view) = self.active_view(cx) else {
            return;
        };
        let Some((searcher, _)) = self.searcher(cx) else {
            return;
        };
        let replacement = self.find_bar.read(cx).replacement(cx);
        let text = view.read(cx).text(cx).clone();
        let matches = searcher.find_all(&text);
        let count = matches.len();
        if count > 0 {
            let edits = matches
                .into_iter()
                .map(|range| Edit::replace(range, replacement.clone()));
            let transaction =
                Transaction::from_edits(&text, edits).expect("matches do not overlap");
            if !view.update(cx, |view, cx| view.apply_command_edit(transaction, cx)) {
                self.report(Some("The document is read-only".into()), true, cx);
                return;
            }
        }
        let plural = if count == 1 { "" } else { "s" };
        self.report(
            Some(format!("Replace All: {count} occurrence{plural} replaced")),
            count == 0,
            cx,
        );
    }

    /// Ctrl+G: Go To line or offset, as in Notepad++.
    fn open_go_to(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Result<()> {
        let view = self.active_view(cx).context("no document is open")?;
        let go_to = cx.new(|cx| GoTo::new(view, window, cx));
        let focus = go_to.read(cx).input.clone();
        window.open_dialog(cx, move |dialog, _, _| {
            let go_to = go_to.clone();
            let confirm = go_to.clone();
            dialog
                .title("Go To")
                .w(px(360.))
                .child(go_to)
                .footer(
                    DialogFooter::new()
                        .child(DialogClose::new().trigger(|button| button.label("Cancel")))
                        .child(crate::workspace::dialog_action(
                            Button::new("go").label("Go"),
                        )),
                )
                .on_ok(move |_, _, cx| {
                    confirm.update(cx, |go_to, cx| match go_to.go(cx) {
                        Ok(()) => true,
                        Err(error) => {
                            go_to.error = Some(error.to_string());
                            cx.notify();
                            false
                        }
                    })
                })
        });
        focus.update(cx, |input, cx| input.focus(window, cx));
        Ok(())
    }
}

/// Go To: a line number (1-based) or a byte offset (0-based, Notepad++'s "position").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GoToTarget {
    Line(usize),
    Offset(usize),
}

/// Where Go To puts the caret; out-of-range values go to the last line or the end.
pub(crate) fn go_to_position(text: &Rope, target: GoToTarget) -> usize {
    match target {
        GoToTarget::Line(line) => {
            let line = line.clamp(1, line_count(text)) - 1;
            line_range(text, line).start
        }
        GoToTarget::Offset(offset) => {
            let offset = text.floor_char_boundary(offset.min(text.len()));
            // Not between the CR and LF of a line break.
            if offset > 0 && text.byte(offset - 1) == b'\r' && text.get_byte(offset) == Some(b'\n')
            {
                offset - 1
            } else {
                offset
            }
        }
    }
}

struct GoTo {
    view: Entity<EditorView>,
    by_offset: bool,
    input: Entity<InputState>,
    error: Option<String>,
}

impl GoTo {
    fn new(view: Entity<EditorView>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            view,
            by_offset: false,
            input: cx.new(|cx| InputState::new(window, cx)),
            error: None,
        }
    }

    fn go(&mut self, cx: &mut Context<Self>) -> Result<()> {
        let value = self.input.read(cx).value();
        let number: usize = value.trim().parse().map_err(|_| anyhow!("Type a number"))?;
        let target = if self.by_offset {
            GoToTarget::Offset(number)
        } else {
            if number == 0 {
                bail!("Lines are numbered from 1");
            }
            GoToTarget::Line(number)
        };
        let text = self.view.read(cx).text(cx).clone();
        let pos = go_to_position(&text, target);
        self.view.update(cx, |view, cx| view.go_to(pos, cx));
        Ok(())
    }
}

impl Render for GoTo {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let view = self.view.read(cx);
        let text = view.text(cx);
        let head = view.selection.primary().head;
        let (here, limit) = if self.by_offset {
            (head, text.len())
        } else {
            (line_of(text, head) + 1, line_count(text))
        };
        let this = cx.entity().downgrade();
        let mode = move |by_offset: bool| {
            let this = this.clone();
            move |_: &gpui_kit::ClickEvent, _: &mut Window, cx: &mut App| {
                this.update(cx, |this, cx| {
                    this.by_offset = by_offset;
                    cx.notify();
                })
                .ok();
            }
        };
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap_2()
                    .child(
                        Button::new("by-line")
                            .small()
                            .label("Line")
                            .selected(!self.by_offset)
                            .when(!self.by_offset, |button| button.primary())
                            .on_click(mode(false)),
                    )
                    .child(
                        Button::new("by-offset")
                            .small()
                            .label("Offset")
                            .selected(self.by_offset)
                            .when(self.by_offset, |button| button.primary())
                            .on_click(mode(true)),
                    ),
            )
            .child(format!("You are here: {here}"))
            .child(Input::new(&self.input).id("go-to-input"))
            .child(format!("You can't go further than: {limit}"))
            .children(
                self.error
                    .clone()
                    .map(|error| div().text_color(rgb(0xcf222e)).child(error)),
            )
    }
}

#[cfg(test)]
mod tests {
    use gpui_kit::{TestAppContext, VisualTestContext};

    use super::*;
    use crate::workspace::tests::{active_text, document_start, open_workspace, secondary};

    fn search_for(
        workspace: &Entity<Workspace>,
        pattern: &str,
        options: FindOptions,
        cx: &mut VisualTestContext,
    ) {
        workspace.update_in(cx, |workspace, window, cx| {
            workspace.find_bar.update(cx, |bar, cx| {
                bar.show(true, Some(pattern.to_owned()), window, cx);
                bar.options = options;
            });
        });
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

    #[gpui_kit::test]
    fn f3_finds_next_and_wraps(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        cx.simulate_input("cat Cat category cat");
        cx.simulate_keystrokes(document_start());
        let options = FindOptions {
            whole_word: true,
            ..FindOptions::default()
        };
        search_for(&workspace, "cat", options, cx);
        cx.simulate_keystrokes("f3");
        assert_eq!(selection(&workspace, cx), (0, 3));
        cx.simulate_keystrokes("f3");
        assert_eq!(selection(&workspace, cx), (4, 7), "case-insensitive");
        cx.simulate_keystrokes("f3");
        assert_eq!(
            selection(&workspace, cx),
            (17, 20),
            "whole word skips category"
        );
        cx.simulate_keystrokes("f3");
        assert_eq!(selection(&workspace, cx), (0, 3), "wrapped around");
        cx.simulate_keystrokes("shift-f3");
        assert_eq!(selection(&workspace, cx), (17, 20), "backward wraps too");
    }

    #[gpui_kit::test]
    fn replace_all_is_one_undo_step(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        cx.simulate_input("a-b-c-d");
        search_for(&workspace, "-", FindOptions::default(), cx);
        workspace.update_in(cx, |workspace, window, cx| {
            workspace.find_bar.update(cx, |bar, cx| {
                bar.replace_input
                    .update(cx, |input, cx| input.set_value("+", window, cx));
            });
            workspace.replace_all(window, cx);
        });
        assert_eq!(active_text(&workspace, cx), "a+b+c+d");
        workspace.update_in(cx, |workspace, window, cx| {
            workspace.close_find(window, cx);
        });
        cx.simulate_keystrokes(&secondary("z"));
        assert_eq!(active_text(&workspace, cx), "a-b-c-d");
    }

    #[test]
    fn go_to_clamps_like_notepad_plus_plus() {
        let text = Rope::from_str("one\r\ntwo\nthree");
        assert_eq!(go_to_position(&text, GoToTarget::Line(2)), 5);
        assert_eq!(go_to_position(&text, GoToTarget::Line(99)), 9);
        assert_eq!(
            go_to_position(&text, GoToTarget::Offset(4)),
            3,
            "not inside CRLF"
        );
        assert_eq!(go_to_position(&text, GoToTarget::Offset(999)), text.len());
        let cyrillic = Rope::from_str("жж");
        assert_eq!(
            go_to_position(&cyrillic, GoToTarget::Offset(1)),
            0,
            "char boundary"
        );
    }
}
