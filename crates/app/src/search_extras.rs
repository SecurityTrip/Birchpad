//! More of Notepad++'s Search menu: Change History (go to the next or previous change, clear
//! all changes) and Find Characters in Range, a small window that finds the next character
//! whose code is in a range (non-ASCII, ASCII, or one's own), up or down.

use std::ops::RangeInclusive;

use anyhow::{Result, anyhow, bail};
use birchpad_commands::Invocation;
use birchpad_core::search::{Direction, find_char_in_range};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::radio::{Radio, RadioGroup};
use gpui_kit::component::{Sizable as _, WindowExt as _};
use gpui_kit::{App, AppContext as _, Context, Entity, WeakEntity, Window, div, prelude::*, px};
use serde::Deserialize;

use crate::commands::{CommandRegistry, Register as _};
use crate::theme::{paint, ui};
use crate::workspace::Workspace;

/// The highest code a character can have.
const MAX_CODE: u32 = 0x10_FFFF;

pub(crate) fn register_commands(registry: &mut CommandRegistry) {
    for (id, forward) in [
        ("search.next-change", true),
        ("search.previous-change", false),
    ] {
        registry.editor(id, move |this, (), _, cx| {
            let text = this.text(cx).clone();
            let lines = {
                let (modified, saved) = this.buffer.read(cx).changed_lines();
                let mut lines = modified.lines(&text);
                lines.extend(saved.lines(&text));
                lines.sort_unstable();
                lines.dedup();
                lines
            };
            let caret = birchpad_core::motion::line_of(&text, this.selection.primary().head);
            if let Some(line) = birchpad_core::next_change(&lines, caret, forward) {
                let start = birchpad_core::motion::line_range(&text, line).start;
                this.go_to(start, cx);
            }
            Ok(())
        });
    }
    registry.editor("search.clear-changes", |this, (), _, cx| {
        let buffer = this.buffer.clone();
        buffer.update(cx, |buffer, cx| buffer.clear_change_history(cx));
        Ok(())
    });
    registry.editor("search.find-in-range", |this, args: RangeArgs, _, cx| {
        let codes = args.codes()?;
        let direction = if args.backward {
            Direction::Backward
        } else {
            Direction::Forward
        };
        let primary = this.selection.primary();
        let from = match direction {
            Direction::Forward => primary.to(),
            Direction::Backward => primary.from(),
        };
        let found = find_char_in_range(this.text(cx), from, codes, direction, args.wrap)
            .ok_or_else(|| anyhow!("{}", t!("search-extras.no-character-in-the-range")))?;
        this.select_range(found, cx);
        Ok(())
    });
    registry.workspace("search.find-characters-in-range", |_, (), window, cx| {
        open(cx.entity().downgrade(), window, cx);
        Ok(())
    });
}

/// What Find Characters in Range looks for.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub(crate) struct RangeArgs {
    pub(crate) from: u32,
    pub(crate) to: u32,
    #[serde(default)]
    pub(crate) backward: bool,
    #[serde(default)]
    pub(crate) wrap: bool,
}

impl RangeArgs {
    /// The codes, if the range is one.
    pub(crate) fn codes(&self) -> Result<RangeInclusive<u32>> {
        if self.from > self.to || self.to > MAX_CODE {
            bail!(
                "{}",
                t!(
                    "search-extras.is-not-a-range-of-character-codes",
                    from = self.from,
                    to = self.to
                )
            );
        }
        Ok(self.from..=self.to)
    }
}

/// A code as typed: decimal, or hexadecimal after `0x` or `U+`.
pub(crate) fn parse_code(text: &str) -> Option<u32> {
    let text = text.trim();
    let hex = text
        .strip_prefix("0x")
        .or_else(|| text.strip_prefix("0X"))
        .or_else(|| text.strip_prefix("U+"))
        .or_else(|| text.strip_prefix("u+"));
    let code = match hex {
        Some(hex) => u32::from_str_radix(hex, 16).ok()?,
        None => text.parse().ok()?,
    };
    (code <= MAX_CODE).then_some(code)
}

/// The ranges the window offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Range {
    NonAscii,
    Ascii,
    Own,
}

/// Search > Find Characters in Range...
pub(crate) struct RangeDialog {
    workspace: WeakEntity<Workspace>,
    pub(crate) range: Range,
    from: Entity<InputState>,
    to: Entity<InputState>,
    pub(crate) backward: bool,
    pub(crate) wrap: bool,
    pub(crate) error: Option<String>,
}

fn open(workspace: WeakEntity<Workspace>, window: &mut Window, cx: &mut App) {
    let dialog = cx.new(|cx| RangeDialog::new(workspace, window, cx));
    window.open_dialog(cx, move |modal, _, _| {
        modal
            .title(t!("search-extras.find-characters-in-range"))
            .w(px(420.))
            .child(dialog.clone())
            .footer(DialogFooter::new().child(crate::workspace::dialog_close("common.close")))
    });
}

impl RangeDialog {
    pub(crate) fn new(
        workspace: WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            workspace,
            range: Range::NonAscii,
            from: cx.new(|cx| InputState::new(window, cx).default_value("0")),
            to: cx.new(|cx| InputState::new(window, cx).default_value("255")),
            backward: false,
            wrap: true,
            error: None,
        }
    }

    /// The search the fields ask for.
    pub(crate) fn args(&self, cx: &App) -> Result<RangeArgs> {
        let (from, to) = match self.range {
            Range::NonAscii => (128, MAX_CODE),
            Range::Ascii => (0, 127),
            Range::Own => {
                let code = |input: &Entity<InputState>| {
                    let text = input.read(cx).value().to_string();
                    parse_code(&text).ok_or_else(|| {
                        anyhow!(
                            "{}",
                            t!("search-extras.is-not-a-character-code", text = text.trim())
                        )
                    })
                };
                (code(&self.from)?, code(&self.to)?)
            }
        };
        let args = RangeArgs {
            from,
            to,
            backward: self.backward,
            wrap: self.wrap,
        };
        args.codes()?;
        Ok(args)
    }

    /// Find: the next character of the range in the active document.
    pub(crate) fn find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let result = self.args(cx).and_then(|args| {
            let invocation = Invocation::with_args(
                "search.find-in-range",
                serde_json::json!({
                    "from": args.from,
                    "to": args.to,
                    "backward": args.backward,
                    "wrap": args.wrap,
                }),
            );
            self.workspace
                .update(cx, |workspace, cx| {
                    workspace.dispatch(&invocation, window, cx)
                })
                .unwrap_or(Ok(()))
        });
        self.error = result.err().map(|error| format!("{error:#}"));
        cx.notify();
    }
}

impl Render for RangeDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ranges = [Range::NonAscii, Range::Ascii, Range::Own];
        let own = self.range == Range::Own;
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                RadioGroup::vertical("character-range")
                    .selected_index(ranges.iter().position(|range| *range == self.range))
                    .on_click(cx.listener(move |this, index: &usize, _, cx| {
                        this.range = ranges[*index];
                        cx.notify();
                    }))
                    .child(Radio::new("range-non-ascii").label(t!("search-extras.non-ascii")))
                    .child(Radio::new("range-ascii").label(t!("search-extras.ascii")))
                    .child(Radio::new("range-own").label(t!("search-extras.my-range"))),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .pl_6()
                    .when(!own, |row| row.opacity(0.5))
                    .child(Input::new(&self.from).id("range-from").small().w(px(110.)))
                    .child("–")
                    .child(Input::new(&self.to).id("range-to").small().w(px(110.))),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .child(
                        RadioGroup::horizontal("range-direction")
                            .selected_index(Some(usize::from(!self.backward)))
                            .on_click(cx.listener(|this, index: &usize, _, cx| {
                                this.backward = *index == 0;
                                cx.notify();
                            }))
                            .child(Radio::new("range-up").label(t!("search-extras.up")))
                            .child(Radio::new("range-down").label(t!("search-extras.down"))),
                    )
                    .child(
                        Checkbox::new("range-wrap")
                            .label(t!("search-extras.wrap-around"))
                            .checked(self.wrap)
                            .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                this.wrap = *checked;
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div().flex().flex_row().items_center().gap_2().child(
                    Button::new("range-find")
                        .primary()
                        .label(t!("search-extras.find"))
                        .on_click(cx.listener(|this, _, window, cx| this.find(window, cx))),
                ),
            )
            .children(
                self.error
                    .clone()
                    .map(|error| div().text_color(paint(ui().error)).child(error)),
            )
    }
}

#[cfg(test)]
mod tests;
