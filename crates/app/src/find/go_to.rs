//! Search > Go To...: a line or an offset.

use anyhow::{Context as _, Result, anyhow, bail};
use birchpad_core::motion::{line_count, line_of};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{Selectable as _, Sizable, WindowExt as _};
use gpui_kit::{App, AppContext as _, Context, Entity, Window, div, prelude::*, px};

use crate::editor::EditorView;
use crate::editor::widgets::error_text;
use crate::workspace::Workspace;

impl Workspace {
    /// Ctrl+G: Go To line or offset, as in Notepad++.
    pub(super) fn open_go_to(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Result<()> {
        let view = self.active_view(cx).context("no document is open")?;
        let go_to = cx.new(|cx| GoTo::new(view, window, cx));
        let focus = go_to.read(cx).input.clone();
        window.open_dialog(cx, move |dialog, _, _| {
            let go_to = go_to.clone();
            let confirm = go_to.clone();
            dialog
                .title(t!("find.go-to"))
                .w(px(360.))
                .child(go_to)
                .footer(
                    DialogFooter::new()
                        .child(crate::workspace::dialog_close("common.cancel"))
                        .child(crate::workspace::dialog_action(
                            Button::new("go").label(t!("find.go")),
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

pub(super) struct GoTo {
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
        let number: usize = value
            .trim()
            .parse()
            .map_err(|_| anyhow!("{}", t!("find.type-a-number")))?;
        let target = if self.by_offset {
            birchpad_core::motion::GoToTarget::Offset(number)
        } else {
            if number == 0 {
                bail!("{}", t!("find.lines-are-numbered-from-1"));
            }
            birchpad_core::motion::GoToTarget::Line(number)
        };
        let text = self.view.read(cx).text(cx).clone();
        let pos = birchpad_core::motion::go_to_position(&text, target);
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
                            .label(t!("common.line"))
                            .selected(!self.by_offset)
                            .when(!self.by_offset, |button| button.primary())
                            .on_click(mode(false)),
                    )
                    .child(
                        Button::new("by-offset")
                            .small()
                            .label(t!("find.offset"))
                            .selected(self.by_offset)
                            .when(self.by_offset, |button| button.primary())
                            .on_click(mode(true)),
                    ),
            )
            .child(t!("find.you-are-here", here = here))
            .child(Input::new(&self.input).id("go-to-input"))
            .child(t!("find.you-can-t-go-further-than", limit = limit))
            .children(self.error.clone().map(|error| error_text(error)))
    }
}
