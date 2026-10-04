//! Messages shown above or over the text: read-only reasons and loading progress.

use birchpad_commands::Invocation;
use birchpad_io::{CHARACTER_SETS, DecodeProblem};
use gpui_kit::component::Sizable;
use gpui_kit::component::button::Button;
use gpui_kit::component::menu::{DropdownMenu, PopupMenu};
use gpui_kit::{AnyElement, Context, Window, div, prelude::*, px, rgb};
use serde_json::json;

use crate::commands::RunCommand;

fn bar(color: u32, border: u32) -> gpui_kit::Div {
    div()
        .flex()
        .flex_row()
        .flex_none()
        .items_center()
        .gap_3()
        .px_3()
        .py_1()
        .bg(rgb(color))
        .border_b_1()
        .border_color(rgb(border))
        .text_size(px(13.))
}

/// The text does not represent the file: read-only, with a way to pick another encoding.
pub(crate) fn decoding_problem(encoding: &str, problem: DecodeProblem) -> AnyElement {
    let message = match problem {
        DecodeProblem::Malformed { offset } => format!(
            "This file is not valid {encoding}: the byte at offset {offset} cannot be decoded. \
             It is open read-only so that saving cannot damage it."
        ),
        DecodeProblem::NotReversible { offset } => format!(
            "In {encoding}, this file contains byte sequences (first at offset {offset}) that \
             would change when saved. It is open read-only."
        ),
    };
    bar(0xfff8c5, 0xd4a72c)
        .child(div().flex_1().child(message))
        .child(
            Button::new("reopen-with-encoding")
                .small()
                .outline()
                .label("Reopen with Encoding")
                .dropdown_menu(encoding_menu),
        )
        .into_any_element()
}

pub(crate) fn read_only_requested() -> AnyElement {
    bar(0xf6f8fa, 0xd0d7de)
        .child("Opened read-only from the command line (-ro).")
        .into_any_element()
}

pub(crate) fn monitoring() -> AnyElement {
    bar(0xddf4ff, 0x54aeff)
        .child(
            "Monitoring (tail -f): the document follows the file and cannot be edited. \
             View > Monitoring stops it.",
        )
        .into_any_element()
}

pub(crate) fn read_only_file() -> AnyElement {
    bar(0xf6f8fa, 0xd0d7de)
        .child("This file is read-only. Use Save As to keep your changes in another file.")
        .into_any_element()
}

/// Centered progress over the text area while the file is read.
pub(crate) fn loading(progress: f32) -> AnyElement {
    div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .px_4()
                .py_2()
                .rounded_md()
                .bg(rgb(0xf6f8fa))
                .border_1()
                .border_color(rgb(0xd0d7de))
                .child(format!("Loading… {:.0}%", progress * 100.)),
        )
        .into_any_element()
}

fn reopen(encoding: &str) -> Box<dyn gpui_kit::Action> {
    Box::new(RunCommand(Invocation::with_args(
        "encoding.encode-in",
        json!({ "encoding": encoding }),
    )))
}

fn encoding_menu(menu: PopupMenu, window: &mut Window, cx: &mut Context<PopupMenu>) -> PopupMenu {
    let mut menu = menu
        .menu("UTF-8", reopen("utf-8"))
        .menu("UTF-16 LE", reopen("utf-16le"))
        .menu("UTF-16 BE", reopen("utf-16be"))
        .menu("ANSI", reopen("ansi"))
        .separator();
    for (group, sets) in CHARACTER_SETS {
        menu = menu.submenu(*group, window, cx, move |mut submenu, _, _| {
            for (name, label) in *sets {
                submenu = submenu.menu(*label, reopen(name));
            }
            submenu
        });
    }
    menu
}
