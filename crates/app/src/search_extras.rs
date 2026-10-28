//! More of Notepad++'s Search menu: Change History (go to the next or previous change, clear
//! all changes) and Find Characters in Range, a small window that finds the next character
//! whose code is in a range (non-ASCII, ASCII, or one's own), up or down; and the search
//! results' Find in These Search Results..., which searches the lines a search found.

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
use crate::editor::widgets::error_text;
use crate::workspace::Workspace;

/// The highest code a character can have.
pub const MAX_CODE: u32 = 0x10_FFFF;

pub fn register_commands(registry: &mut CommandRegistry) {
    registry.workspace("search.find-in-results", |this, (), window, cx| {
        if this.search_results.read(cx).run_of(None).is_none() {
            bail!("{}", t!("search-extras.there-are-no-search-results"));
        }
        open_in_results(cx.entity().downgrade(), window, cx);
        Ok(())
    });
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
pub struct RangeArgs {
    pub from: u32,
    pub to: u32,
    #[serde(default)]
    pub backward: bool,
    #[serde(default)]
    pub wrap: bool,
}

impl RangeArgs {
    /// The codes, if the range is one.
    pub fn codes(&self) -> Result<RangeInclusive<u32>> {
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
pub fn parse_code(text: &str) -> Option<u32> {
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
pub enum Range {
    NonAscii,
    Ascii,
    Own,
}

/// Search > Find Characters in Range...
pub struct RangeDialog {
    workspace: WeakEntity<Workspace>,
    pub range: Range,
    pub from: Entity<InputState>,
    pub to: Entity<InputState>,
    pub backward: bool,
    pub wrap: bool,
    pub error: Option<String>,
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
    pub fn new(
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
    pub fn args(&self, cx: &App) -> Result<RangeArgs> {
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
    pub fn find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
            .children(self.error.clone().map(error_text))
    }
}

/// Find in These Search Results...: what to find in the lines of the search chosen in the
/// results, or the newest one.
pub struct InResultsDialog {
    workspace: WeakEntity<Workspace>,
    pub input: Entity<InputState>,
    pub match_case: bool,
    pub whole_word: bool,
    pub regex: bool,
    pub error: Option<String>,
}

#[cfg(feature = "test-support")]
thread_local! {
    /// The dialog last opened, for tests to drive.
    pub static IN_RESULTS: std::cell::RefCell<Option<WeakEntity<InResultsDialog>>> =
        std::cell::RefCell::default();
}

fn open_in_results(workspace: WeakEntity<Workspace>, window: &mut Window, cx: &mut App) {
    let dialog = cx.new(|cx| InResultsDialog {
        workspace,
        input: cx.new(|cx| InputState::new(window, cx)),
        match_case: false,
        whole_word: false,
        regex: false,
        error: None,
    });
    #[cfg(feature = "test-support")]
    IN_RESULTS.with(|opened| *opened.borrow_mut() = Some(dialog.downgrade()));
    let input = dialog.read(cx).input.clone();
    window.open_dialog(cx, move |modal, _, _| {
        modal
            .title(t!("search-extras.find-in-these-search-results"))
            .w(px(460.))
            .child(dialog.clone())
            .footer(DialogFooter::new().child(crate::workspace::dialog_close("common.close")))
    });
    input.update(cx, |input, cx| input.focus(window, cx));
}

impl InResultsDialog {
    /// Find All: searches the lines and closes the dialog, or says why not.
    pub fn find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let pattern = self.input.read(cx).value().to_string();
        let query = birchpad_core::search::Query {
            pattern: pattern.clone(),
            match_case: self.match_case,
            whole_word: self.whole_word,
            mode: if self.regex {
                birchpad_core::search::SearchMode::Regex
            } else {
                birchpad_core::search::SearchMode::Normal
            },
            dot_matches_newline: false,
        };
        let searcher = match birchpad_core::search::Searcher::new(&query) {
            Ok(searcher) => searcher,
            Err(error) => {
                self.error = Some(error.to_string());
                cx.notify();
                return;
            }
        };
        let found = self.workspace.update(cx, |workspace, cx| {
            let results = workspace.search_results.clone();
            results.update(cx, |results, cx| {
                let run = results.run_of(results.selected)?;
                Some(results.search_within(run, &searcher, &pattern, cx))
            })
        });
        match found {
            Ok(Some(hits)) if hits > 0 => window.close_dialog(cx),
            Ok(Some(_)) => {
                self.error = Some(t!("search-extras.nothing-found-in-the-results"));
                cx.notify();
            }
            _ => {
                self.error = Some(t!("search-extras.there-are-no-search-results"));
                cx.notify();
            }
        }
    }
}

impl Render for InResultsDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let switch = |id: &'static str, label: String, on: bool, set: fn(&mut Self, bool)| {
            Checkbox::new(id)
                .label(label)
                .checked(on)
                .on_click(cx.listener(move |this, checked: &bool, _, cx| {
                    set(this, *checked);
                    cx.notify();
                }))
        };
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(Input::new(&self.input).id("in-results-pattern"))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap_3()
                    .child(switch(
                        "in-results-match-case",
                        t!("search-extras.match-case"),
                        self.match_case,
                        |this, on| this.match_case = on,
                    ))
                    .child(switch(
                        "in-results-whole-word",
                        t!("search-extras.match-whole-word-only"),
                        self.whole_word,
                        |this, on| this.whole_word = on,
                    ))
                    .child(switch(
                        "in-results-regex",
                        t!("search-extras.regular-expression"),
                        self.regex,
                        |this, on| this.regex = on,
                    )),
            )
            .child(
                div().child(
                    Button::new("in-results-find")
                        .primary()
                        .label(t!("search-extras.find-all"))
                        .on_click(cx.listener(|this, _, window, cx| this.find(window, cx))),
                ),
            )
            .children(self.error.clone().map(error_text))
    }
}
