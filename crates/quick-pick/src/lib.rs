//! A list to pick from by typing, which the command palette and Go to File show: a field above
//! the items, the items it matches below it (fuzzy, as `birchpad_core::fuzzy` does), the
//! characters matched in bold. Up and Down (Page Up and Page Down by a page) choose one, Enter
//! or a double-click picks it, Escape closes the dialog. The recently used items come first.

use std::ops::Range;
use std::rc::Rc;

use gpui_kit::component::WindowExt as _;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::{
    App, AppContext as _, ClickEvent, Context, Entity, FontWeight, HighlightStyle, KeyDownEvent,
    ScrollStrategy, StyledText, Subscription, UniformListScrollHandle, Window, div, prelude::*, px,
    uniform_list,
};

use birchpad_editor::theme::{paint, ui};

/// The rows a page holds, for Page Up and Page Down.
pub const PAGE: usize = 12;
const ROW_HEIGHT: f32 = 24.;

/// An item of the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickItem {
    /// What is shown, and matched against what is typed.
    pub text: String,
    /// Shown on the right, not matched: a shortcut, a folder.
    pub hint: String,
}

impl PickItem {
    pub fn new(text: impl Into<String>, hint: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            hint: hint.into(),
        }
    }
}

/// What picking an item does, given its index; the dialog is closed already.
pub type OnPick = Rc<dyn Fn(usize, &mut Window, &mut App)>;

pub struct QuickPick {
    pub input: Entity<InputState>,
    items: Vec<PickItem>,
    /// How many of the first items were used recently, the last first.
    recent: usize,
    /// The items the query matches, in the order shown: each one's index and the byte
    /// offsets of the characters matched.
    pub shown: Vec<(usize, Vec<usize>)>,
    /// The row chosen, an index into `shown`.
    pub selected: usize,
    /// Shown when no item matches, or there is none.
    empty: String,
    scroll: UniformListScrollHandle,
    on_pick: OnPick,
    _input: Subscription,
}

#[cfg(feature = "test-support")]
thread_local! {
    /// The list last opened, for tests to drive.
    pub static OPENED: std::cell::RefCell<Option<gpui_kit::WeakEntity<QuickPick>>> =
        std::cell::RefCell::default();
}

/// Opens a dialog titled `title` with a list of `items`, the first `recent` of them recently
/// used.
#[expect(
    clippy::too_many_arguments,
    reason = "the dialog's texts, its items and what picking does, each its own"
)]
pub fn open(
    title: String,
    placeholder: String,
    empty: String,
    items: Vec<PickItem>,
    recent: usize,
    on_pick: OnPick,
    window: &mut Window,
    cx: &mut App,
) -> Entity<QuickPick> {
    let pick = cx.new(|cx| QuickPick::new(placeholder, empty, items, recent, on_pick, window, cx));
    #[cfg(feature = "test-support")]
    OPENED.with(|opened| *opened.borrow_mut() = Some(pick.downgrade()));
    let shown = pick.clone();
    window.open_dialog(cx, move |dialog, _, _| {
        dialog.title(title.clone()).w(px(640.)).child(shown.clone())
    });
    let input = pick.read(cx).input.clone();
    input.update(cx, |input, cx| input.focus(window, cx));
    pick
}

impl QuickPick {
    pub fn new(
        placeholder: String,
        empty: String,
        items: Vec<PickItem>,
        recent: usize,
        on_pick: OnPick,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
        let subscription =
            cx.subscribe_in(&input, window, |this, _, event, window, cx| match event {
                InputEvent::Change => this.refilter(cx),
                InputEvent::PressEnter { .. } => this.pick(window, cx),
                _ => {}
            });
        let mut this = Self {
            input,
            items,
            recent,
            shown: Vec::new(),
            selected: 0,
            empty,
            scroll: UniformListScrollHandle::new(),
            on_pick,
            _input: subscription,
        };
        this.refilter(cx);
        this
    }

    /// What is typed.
    pub fn query(&self, cx: &App) -> String {
        self.input.read(cx).value().to_string()
    }

    pub fn items(&self) -> &[PickItem] {
        &self.items
    }

    /// What is shown when no item matches, or there is none.
    pub fn empty(&self) -> &str {
        &self.empty
    }

    pub fn set_empty(&mut self, empty: String) {
        self.empty = empty;
    }

    /// Takes new items (Go to File's, as its folders are read), keeping what is typed and
    /// the item chosen if it is still there.
    pub fn set_items(&mut self, items: Vec<PickItem>, recent: usize, cx: &mut Context<Self>) {
        let chosen = self.selected_item().cloned();
        self.items = items;
        self.recent = recent;
        self.refilter(cx);
        if let Some(chosen) = chosen
            && let Some(row) = self
                .shown
                .iter()
                .position(|(index, _)| self.items[*index] == chosen)
        {
            self.selected = row;
        }
    }

    /// Matches the items against what is typed again; the first one is chosen.
    pub fn refilter(&mut self, cx: &mut Context<Self>) {
        let query = self.query(cx);
        self.shown = birchpad_core::fuzzy::rank_recent(
            &query,
            self.items.iter().map(|item| item.text.as_str()),
            self.recent,
        )
        .into_iter()
        .map(|(index, found)| (index, found.positions))
        .collect();
        self.selected = 0;
        self.scroll.scroll_to_item(0, ScrollStrategy::Top);
        cx.notify();
    }

    /// The item chosen, if any is shown.
    pub fn selected_item(&self) -> Option<&PickItem> {
        self.shown
            .get(self.selected)
            .map(|(index, _)| &self.items[*index])
    }

    /// Chooses the row `delta` rows down (up if negative), stopping at the first and the last.
    pub fn move_by(&mut self, delta: isize, cx: &mut Context<Self>) {
        let Some(last) = self.shown.len().checked_sub(1) else {
            return;
        };
        self.selected = self.selected.saturating_add_signed(delta).min(last);
        self.scroll
            .scroll_to_item(self.selected, ScrollStrategy::Center);
        cx.notify();
    }

    /// Picks the item chosen: closes the dialog and does what picking does. Nothing happens
    /// when nothing is shown.
    pub fn pick(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(&(index, _)) = self.shown.get(self.selected) else {
            return;
        };
        window.close_dialog(cx);
        let on_pick = self.on_pick.clone();
        window.defer(cx, move |window, cx| on_pick(index, window, cx));
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if event.keystroke.modifiers.modified() {
            return;
        }
        let delta = match event.keystroke.key.as_str() {
            "down" => 1,
            "up" => -1,
            "pagedown" => PAGE as isize,
            "pageup" => -(PAGE as isize),
            _ => return,
        };
        self.move_by(delta, cx);
        cx.stop_propagation();
    }
}

/// The ranges of the characters at `positions` in `text`, for their highlight.
pub fn highlight_ranges(text: &str, positions: &[usize]) -> Vec<Range<usize>> {
    positions
        .iter()
        .filter_map(|&at| {
            let c = text.get(at..)?.chars().next()?;
            Some(at..at + c.len_utf8())
        })
        .collect()
}

impl Render for QuickPick {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = ui();
        let rows: Vec<(PickItem, Vec<usize>)> = self
            .shown
            .iter()
            .map(|(index, positions)| (self.items[*index].clone(), positions.clone()))
            .collect();
        let selected = self.selected;
        let body = if rows.is_empty() {
            div()
                .p_2()
                .text_color(paint(colors.muted))
                .child(self.empty.clone())
                .into_any_element()
        } else {
            uniform_list(
                "quick-pick-rows",
                rows.len(),
                cx.processor(move |_, range: Range<usize>, _, cx| {
                    range
                        .map(|row| {
                            let (item, positions) = &rows[row];
                            let bold = HighlightStyle {
                                font_weight: Some(FontWeight::BOLD),
                                color: Some(paint(colors.accent).into()),
                                ..HighlightStyle::default()
                            };
                            let highlights: Vec<(Range<usize>, HighlightStyle)> =
                                highlight_ranges(&item.text, positions)
                                    .into_iter()
                                    .map(|range| (range, bold))
                                    .collect();
                            div()
                                .id(("quick-pick-row", row))
                                .debug_selector(move || format!("quick-pick-row-{row}"))
                                .h(px(ROW_HEIGHT))
                                .px_2()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_3()
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .cursor_pointer()
                                .when(row == selected, |div| div.bg(paint(colors.selected)))
                                .hover(|div| div.bg(paint(colors.hovered)))
                                .child(div().flex_1().min_w(px(0.)).overflow_hidden().child(
                                    StyledText::new(item.text.clone()).with_highlights(highlights),
                                ))
                                .when(!item.hint.is_empty(), |div_| {
                                    div_.child(
                                        div()
                                            .flex_none()
                                            .max_w(px(260.))
                                            .overflow_hidden()
                                            .text_sm()
                                            .text_color(paint(colors.muted))
                                            .child(item.hint.clone()),
                                    )
                                })
                                .on_click(cx.listener(
                                    move |this, event: &ClickEvent, window, cx| {
                                        this.selected = row;
                                        cx.notify();
                                        if event.click_count() >= 2 {
                                            this.pick(window, cx);
                                        }
                                    },
                                ))
                        })
                        .collect::<Vec<_>>()
                }),
            )
            .track_scroll(&self.scroll)
            .h(px(ROW_HEIGHT * PAGE as f32))
            .into_any_element()
        };
        div()
            .id("quick-pick")
            .flex()
            .flex_col()
            .gap_2()
            .on_key_down(cx.listener(Self::on_key_down))
            .child(Input::new(&self.input).id("quick-pick-query"))
            .child(
                div()
                    .border_1()
                    .border_color(paint(colors.border))
                    .child(body),
            )
    }
}
