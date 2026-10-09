//! Clipboard History, as Notepad++'s (Edit > Clipboard History): the texts copied while the
//! panel is open, newest first, from Birchpad and from other programs alike. A double-click,
//! or Enter, puts one into the document as a paste would, at every caret.
//!
//! GPUI says nothing when the clipboard changes, so the panel reads it every second: a copy in
//! another program shows within a second, or when Birchpad comes back to the front. A text
//! copied again moves to the top instead of showing twice. The history lasts until Birchpad
//! quits, as in Notepad++.

use std::collections::VecDeque;
use std::ops::Range;
use std::time::Duration;

use gpui_kit::{
    App, ClickEvent, Context, FocusHandle, Focusable, KeyDownEvent, ScrollStrategy, Task,
    UniformListScrollHandle, WeakEntity, Window, div, prelude::*, px, uniform_list,
};

use crate::workspace::Workspace;

/// Texts kept.
pub(crate) const LIMIT: usize = 30;
/// Longer texts are not kept: the history is for snippets.
pub(crate) const MAX_TEXT: usize = 1 << 20;
const POLL: Duration = Duration::from_secs(1);
const ROW_HEIGHT: f32 = 22.;
/// Characters a row shows of its text.
const PREVIEW: usize = 120;

/// Puts `text` at the top of `history`: a text already there moves up, an empty or huge one
/// is not kept, and the oldest go past the limit. Returns whether anything changed.
pub(crate) fn remember(history: &mut VecDeque<String>, text: &str) -> bool {
    if text.is_empty() || text.len() > MAX_TEXT || history.front().is_some_and(|top| top == text) {
        return false;
    }
    history.retain(|known| known != text);
    history.push_front(text.to_owned());
    history.truncate(LIMIT);
    true
}

/// A row's text: on one line, line breaks shown as `⏎`, tabs as spaces, cut with `…`.
pub(crate) fn preview(text: &str) -> String {
    let mut shown: String = text
        .chars()
        .take(PREVIEW)
        .map(|ch| match ch {
            '\n' => '⏎',
            '\r' => '⏎',
            '\t' => ' ',
            ch => ch,
        })
        .collect();
    // CRLF is one break.
    shown = shown.replace("⏎⏎", "⏎");
    if text.chars().nth(PREVIEW).is_some() {
        shown.push('…');
    }
    shown
}

pub(crate) struct ClipboardHistory {
    workspace: WeakEntity<Workspace>,
    pub(crate) entries: VecDeque<String>,
    pub(crate) selected: Option<usize>,
    scroll: UniformListScrollHandle,
    focus_handle: FocusHandle,
    _poll: Task<()>,
}

impl Focusable for ClipboardHistory {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl ClipboardHistory {
    pub(crate) fn new(
        workspace: WeakEntity<Workspace>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let poll = cx.spawn(async move |this, cx| {
            loop {
                if this.update(cx, |this, cx| this.read_clipboard(cx)).is_err() {
                    break;
                }
                cx.background_executor().timer(POLL).await;
            }
        });
        Self {
            workspace,
            entries: VecDeque::new(),
            selected: None,
            scroll: UniformListScrollHandle::new(),
            focus_handle: cx.focus_handle(),
            _poll: poll,
        }
    }

    /// Takes the clipboard's text into the history.
    pub(crate) fn read_clipboard(&mut self, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        if remember(&mut self.entries, &text) {
            // The selection stays on the same text.
            self.selected = self
                .selected
                .map(|index| (index + 1).min(self.entries.len() - 1));
            cx.notify();
        }
    }

    /// Pastes entry `index` into the active document.
    pub(crate) fn paste(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = self.entries.get(index).cloned() else {
            return;
        };
        self.workspace
            .update(cx, |workspace, cx| workspace.insert_text(&text, window, cx))
            .ok();
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if event.keystroke.modifiers.modified() || self.entries.is_empty() {
            return;
        }
        let last = self.entries.len() - 1;
        match event.keystroke.key.as_str() {
            "down" => self.selected = Some(self.selected.map_or(0, |i| (i + 1).min(last))),
            "up" => self.selected = Some(self.selected.map_or(0, |i| i.saturating_sub(1))),
            "enter" => {
                if let Some(index) = self.selected {
                    self.paste(index, window, cx);
                }
            }
            "delete" => {
                if let Some(index) = self.selected {
                    self.entries.remove(index);
                    self.selected =
                        (!self.entries.is_empty()).then(|| index.min(self.entries.len() - 1));
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
}

impl Render for ClipboardHistory {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = if self.entries.is_empty() {
            div()
                .p_2()
                .text_color(crate::theme::paint(crate::theme::ui().muted))
                .child("Copied texts show here")
                .into_any_element()
        } else {
            let rows: Vec<(String, usize)> = self
                .entries
                .iter()
                .map(|text| (preview(text), text.chars().count()))
                .collect();
            uniform_list(
                "clipboard-rows",
                rows.len(),
                cx.processor(move |this, range: Range<usize>, _, cx| {
                    range
                        .map(|index| {
                            let (text, length) = &rows[index];
                            div()
                                .id(("clipboard-row", index))
                                .debug_selector(move || format!("clipboard-row-{index}"))
                                .w_full()
                                .h(px(ROW_HEIGHT))
                                .px_2()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_2()
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .cursor_pointer()
                                .when(this.selected == Some(index), |row| {
                                    row.bg(crate::theme::paint(crate::theme::ui().selected))
                                })
                                .hover(|row| {
                                    row.bg(crate::theme::paint(crate::theme::ui().hovered))
                                })
                                .child(
                                    div()
                                        .flex_1()
                                        .overflow_hidden()
                                        .font_family(crate::MONOSPACE)
                                        .child(text.clone()),
                                )
                                .child(
                                    div()
                                        .flex_none()
                                        .text_color(crate::theme::paint(crate::theme::ui().faint))
                                        .child(length.to_string()),
                                )
                                .on_click(cx.listener(
                                    move |this, event: &ClickEvent, window, cx| {
                                        window.focus(&this.focus_handle, cx);
                                        this.selected = Some(index);
                                        if event.click_count() >= 2 {
                                            this.paste(index, window, cx);
                                        }
                                        cx.notify();
                                    },
                                ))
                        })
                        .collect::<Vec<_>>()
                }),
            )
            .track_scroll(&self.scroll)
            .size_full()
            .into_any_element()
        };
        div()
            .id("clipboard-history")
            .key_context("ClipboardHistory")
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .size_full()
            .child(body)
    }
}

#[cfg(test)]
mod tests {
    use gpui_kit::{ClipboardItem, Entity, Modifiers, TestAppContext, VisualTestContext};

    use super::*;
    use crate::panels::PanelKind;
    use crate::workspace::tests::{active_text, open_workspace};

    #[test]
    fn the_newest_text_comes_first_once() {
        let mut history = VecDeque::new();
        assert!(remember(&mut history, "a"));
        assert!(remember(&mut history, "b"));
        assert!(!remember(&mut history, "b"), "the same as the top");
        assert!(remember(&mut history, "a"), "moves up");
        assert_eq!(history, ["a", "b"]);
        // Empty and huge texts are not kept.
        assert!(!remember(&mut history, ""));
        assert!(!remember(&mut history, &"x".repeat(MAX_TEXT + 1)));
        assert!(
            remember(&mut history, &"x".repeat(MAX_TEXT)),
            "exactly the limit"
        );
        // The oldest go past the limit.
        for n in 0..LIMIT + 5 {
            remember(&mut history, &n.to_string());
        }
        assert_eq!(history.len(), LIMIT);
        assert_eq!(history.front().map(String::as_str), Some("34"));
        assert_eq!(history.back().map(String::as_str), Some("5"));
    }

    #[test]
    fn previews_are_one_line_and_short() {
        assert_eq!(preview("one\r\ntwo\nthree\tfour"), "one⏎two⏎three four");
        assert_eq!(preview(""), "");
        let long = "ж".repeat(PREVIEW);
        assert_eq!(preview(&long), long, "exactly the length shown");
        let longer = "ж".repeat(PREVIEW + 1);
        assert_eq!(preview(&longer), format!("{long}…"));
    }

    fn panel(
        workspace: &Entity<Workspace>,
        cx: &mut VisualTestContext,
    ) -> Entity<ClipboardHistory> {
        workspace.update_in(cx, |workspace, window, cx| {
            workspace.open_panel(PanelKind::ClipboardHistory, window, cx);
            let view = workspace.docks.view(PanelKind::ClipboardHistory).unwrap();
            view.clone().downcast::<ClipboardHistory>().unwrap()
        })
    }

    #[gpui_kit::test]
    fn copies_are_collected_and_pasted_back(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        cx.write_to_clipboard(ClipboardItem::new_string("first".into()));
        let history = panel(&workspace, cx);
        cx.run_until_parked();
        let entries = |cx: &mut VisualTestContext| {
            history.read_with(cx, |history, _| {
                history.entries.iter().cloned().collect::<Vec<_>>()
            })
        };
        assert_eq!(entries(cx), ["first"], "read when the panel opens");
        // A copy in the editor shows within the next second.
        cx.simulate_input("typed text");
        cx.simulate_keystrokes(&crate::workspace::tests::secondary("a"));
        cx.simulate_keystrokes(&crate::workspace::tests::secondary("c"));
        cx.executor().advance_clock(POLL);
        cx.run_until_parked();
        assert_eq!(entries(cx), ["typed text", "first"]);

        // A double-click pastes the text at the caret.
        cx.simulate_keystrokes("end");
        let row = cx.debug_bounds("clipboard-row-1").expect("drawn").center();
        cx.simulate_event(gpui_kit::MouseDownEvent {
            position: row,
            button: gpui_kit::MouseButton::Left,
            modifiers: Modifiers::none(),
            click_count: 2,
            first_mouse: false,
        });
        cx.simulate_event(gpui_kit::MouseUpEvent {
            position: row,
            button: gpui_kit::MouseButton::Left,
            modifiers: Modifiers::none(),
            click_count: 2,
        });
        cx.run_until_parked();
        assert_eq!(active_text(&workspace, cx), "typed textfirst");

        // Keys: Delete forgets the selected text, Enter pastes; nothing selected, nothing.
        workspace.update_in(cx, |_, window, cx| {
            let focus = history.read(cx).focus_handle.clone();
            window.focus(&focus, cx);
        });
        cx.simulate_keystrokes("up delete");
        assert_eq!(entries(cx), ["first"]);
        cx.simulate_keystrokes("enter");
        assert_eq!(active_text(&workspace, cx), "typed textfirstfirst");
        // Pasting goes back to the text; in the panel again, the history empties.
        workspace.update_in(cx, |_, window, cx| {
            let focus = history.read(cx).focus_handle.clone();
            window.focus(&focus, cx);
        });
        cx.simulate_keystrokes("delete delete enter");
        assert!(entries(cx).is_empty());
        assert_eq!(active_text(&workspace, cx), "typed textfirstfirst");
    }
}
