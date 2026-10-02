//! The Column Editor dialog (Edit > Column Editor, Alt+C): text or a sequence of numbers to
//! insert into every row of the rectangle, every caret, or every line from the caret down.
//! It runs `edit.column-insert`, which does the work as one undo step.

use anyhow::{Result, anyhow, bail};
use birchpad_core::ops::{Base, parse_number};
use gpui_kit::component::button::Button;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::dialog::{DialogClose, DialogFooter};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::radio::RadioGroup;
use gpui_kit::component::{Disableable as _, WindowExt as _};
use gpui_kit::{App, AppContext as _, Context, Entity, Window, div, prelude::*, px, rgb};

use super::EditorView;
use super::multi::{ColumnInsertArgs, FormatName, LeadingName, NumberArgs};

pub(super) struct ColumnEditor {
    view: Entity<EditorView>,
    numbers: bool,
    text: Entity<InputState>,
    initial: Entity<InputState>,
    step: Entity<InputState>,
    repeat: Entity<InputState>,
    leading: LeadingName,
    format: FormatName,
    uppercase: bool,
    error: Option<String>,
}

/// Opens the dialog for `view`.
pub(super) fn open(view: Entity<EditorView>, window: &mut Window, cx: &mut App) {
    let editor = cx.new(|cx| ColumnEditor::new(view, window, cx));
    let focus = editor.read(cx).text.clone();
    window.open_dialog(cx, move |dialog, _, _| {
        let confirm = editor.clone();
        dialog
            .title("Column / Multi-Selection Editor")
            .w(px(420.))
            .child(editor.clone())
            .footer(
                DialogFooter::new()
                    .child(DialogClose::new().trigger(|button| button.label("Cancel")))
                    .child(crate::workspace::dialog_action(
                        Button::new("column-ok").label("OK"),
                    )),
            )
            .on_ok(move |_, _, cx| {
                confirm.update(cx, |editor, cx| match editor.insert(cx) {
                    Ok(()) => true,
                    Err(error) => {
                        editor.error = Some(error.to_string());
                        cx.notify();
                        false
                    }
                })
            })
    });
    focus.update(cx, |input, cx| input.focus(window, cx));
}

impl ColumnEditor {
    pub(super) fn new(
        view: Entity<EditorView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = |value: &str, window: &mut Window, cx: &mut Context<Self>| {
            let value = value.to_owned();
            cx.new(|cx| InputState::new(window, cx).default_value(value))
        };
        Self {
            view,
            numbers: false,
            text: input("", window, cx),
            initial: input("1", window, cx),
            step: input("1", window, cx),
            repeat: input("1", window, cx),
            leading: LeadingName::None,
            format: FormatName::Dec,
            uppercase: true,
            error: None,
        }
    }

    /// What the dialog's fields ask for. The number fields are read in the chosen format,
    /// as in Notepad++.
    fn args(&self, cx: &App) -> Result<ColumnInsertArgs> {
        if !self.numbers {
            return Ok(ColumnInsertArgs::Text {
                text: self.text.read(cx).value().to_string(),
            });
        }
        let base = Base::from(self.format);
        let field = |input: &Entity<InputState>| input.read(cx).value().to_string();
        let initial = field(&self.initial);
        let initial = parse_number(&initial, base)
            .ok_or_else(|| anyhow!("The initial number is not a {} number", base_name(base)))?;
        let step = field(&self.step);
        let step = if step.trim().is_empty() {
            0
        } else {
            parse_number(&step, base)
                .ok_or_else(|| anyhow!("Increase by is not a {} number", base_name(base)))?
        };
        let repeat = field(&self.repeat);
        let repeat = if repeat.trim().is_empty() {
            1
        } else {
            match parse_number(&repeat, base) {
                Some(repeat) if repeat > 0 => repeat as usize,
                _ => bail!("Repeat must be a positive {} number", base_name(base)),
            }
        };
        Ok(ColumnInsertArgs::Numbers(NumberArgs {
            initial,
            step,
            repeat,
            leading: self.leading,
            format: self.format,
            uppercase: self.uppercase,
        }))
    }

    pub(super) fn insert(&mut self, cx: &mut Context<Self>) -> Result<()> {
        let args = self.args(cx)?;
        self.view
            .update(cx, |view, cx| view.column_insert(args, cx));
        Ok(())
    }
}

fn base_name(base: Base) -> &'static str {
    match base {
        Base::Dec => "decimal",
        Base::Hex => "hexadecimal",
        Base::Oct => "octal",
        Base::Bin => "binary",
    }
}

impl Render for ColumnEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let this = cx.entity().downgrade();
        let update = move |change: fn(&mut ColumnEditor, usize), index: usize, cx: &mut App| {
            this.update(cx, |editor, cx| {
                change(editor, index);
                editor.error = None;
                cx.notify();
            })
            .ok();
        };
        let mode = update.clone();
        let leading = update.clone();
        let format = update.clone();
        let case = update;
        let label = |text: &'static str| {
            div()
                .flex_none()
                .w(px(120.))
                .whitespace_nowrap()
                .child(text)
        };
        let row = || div().flex().flex_row().items_center().gap_2();
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                RadioGroup::horizontal("column-mode")
                    .children(["Text to Insert", "Number to Insert"])
                    .selected_index(Some(usize::from(self.numbers)))
                    .on_click(move |index, _, cx| {
                        mode(|editor, index| editor.numbers = index == 1, *index, cx);
                    }),
            )
            .child(
                Input::new(&self.text)
                    .id("column-text")
                    .disabled(self.numbers),
            )
            .child(
                row().child(label("Initial number:")).child(
                    Input::new(&self.initial)
                        .id("column-initial")
                        .disabled(!self.numbers),
                ),
            )
            .child(
                row().child(label("Increase by:")).child(
                    Input::new(&self.step)
                        .id("column-step")
                        .disabled(!self.numbers),
                ),
            )
            .child(
                row().child(label("Repeat:")).child(
                    Input::new(&self.repeat)
                        .id("column-repeat")
                        .disabled(!self.numbers),
                ),
            )
            .child(
                row().child(label("Leading:")).child(
                    RadioGroup::horizontal("column-leading")
                        .children(["None", "Zeros", "Spaces"])
                        .disabled(!self.numbers)
                        .selected_index(Some(self.leading as usize))
                        .on_click(move |index, _, cx| {
                            leading(
                                |editor, index| {
                                    editor.leading = [
                                        LeadingName::None,
                                        LeadingName::Zeros,
                                        LeadingName::Spaces,
                                    ][index];
                                },
                                *index,
                                cx,
                            );
                        }),
                ),
            )
            .child(
                row().child(label("Format:")).child(
                    RadioGroup::horizontal("column-format")
                        .children(["Dec", "Hex", "Oct", "Bin"])
                        .disabled(!self.numbers)
                        .selected_index(Some(self.format as usize))
                        .on_click(move |index, _, cx| {
                            format(
                                |editor, index| {
                                    editor.format = [
                                        FormatName::Dec,
                                        FormatName::Hex,
                                        FormatName::Oct,
                                        FormatName::Bin,
                                    ][index];
                                },
                                *index,
                                cx,
                            );
                        }),
                ),
            )
            .child(
                Checkbox::new("column-uppercase")
                    .label("Hexadecimal in upper case (A-F)")
                    .checked(self.uppercase)
                    .disabled(!self.numbers || self.format != FormatName::Hex)
                    .on_click(move |checked, _, cx| {
                        case(
                            |editor, checked| editor.uppercase = checked == 1,
                            usize::from(*checked),
                            cx,
                        );
                    }),
            )
            .children(
                self.error
                    .clone()
                    .map(|error| div().text_color(rgb(0xcf222e)).child(error)),
            )
    }
}

#[cfg(test)]
impl ColumnEditor {
    /// Fills the dialog as a user would, for tests.
    pub(super) fn fill_numbers(
        &mut self,
        initial: &str,
        step: &str,
        repeat: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.numbers = true;
        for (input, value) in [
            (&self.initial, initial),
            (&self.step, step),
            (&self.repeat, repeat),
        ] {
            input.update(cx, |input, cx| {
                input.set_value(value.to_owned(), window, cx)
            });
        }
    }
}
