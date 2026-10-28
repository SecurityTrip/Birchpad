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

use crate::editor::widgets::ListRow as _;
use crate::workspace::Workspace;

/// Texts kept.
pub const LIMIT: usize = 30;
/// Longer texts are not kept: the history is for snippets.
pub const MAX_TEXT: usize = 1 << 20;
pub const POLL: Duration = Duration::from_secs(1);
const ROW_HEIGHT: f32 = 22.;
/// Characters a row shows of its text.
pub const PREVIEW: usize = 120;

/// Puts `text` at the top of `history`: a text already there moves up, an empty or huge one
/// is not kept, and the oldest go past the limit. Returns whether anything changed.
pub fn remember(history: &mut VecDeque<String>, text: &str) -> bool {
    if text.is_empty() || text.len() > MAX_TEXT || history.front().is_some_and(|top| top == text) {
        return false;
    }
    history.retain(|known| known != text);
    history.push_front(text.to_owned());
    history.truncate(LIMIT);
    true
}

/// A row's text: on one line, line breaks shown as `⏎`, tabs as spaces, cut with `…`.
pub fn preview(text: &str) -> String {
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

pub struct ClipboardHistory {
    workspace: WeakEntity<Workspace>,
    pub entries: VecDeque<String>,
    pub selected: Option<usize>,
    scroll: UniformListScrollHandle,
    pub focus_handle: FocusHandle,
    _poll: Task<()>,
}

impl Focusable for ClipboardHistory {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl ClipboardHistory {
    pub fn new(workspace: WeakEntity<Workspace>, _: &mut Window, cx: &mut Context<Self>) -> Self {
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
    pub fn read_clipboard(&mut self, cx: &mut Context<Self>) {
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
    pub fn paste(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
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
                .child(t!("panels.clipboard-history.copied-texts-show-here"))
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
                                .list_row(this.selected == Some(index))
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
