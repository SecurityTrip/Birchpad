//! Messages shown above or over the text: read-only reasons and loading progress.

use birchpad_commands::Invocation;
use birchpad_io::{CHARACTER_SETS, DecodeProblem};
use gpui_kit::component::Sizable;
use gpui_kit::component::button::Button;
use gpui_kit::component::menu::{DropdownMenu, PopupMenu};
use gpui_kit::{AnyElement, Context, FocusHandle, Window, div, prelude::*, px};
use serde_json::json;

use birchpad_ui::RunCommand;

use crate::theme::{paint, ui};
pub use crate::widgets::BarColors;
use crate::widgets::{self, Line};

/// What a message above the text is about, which picks its colors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// The file or the command line makes the document read-only.
    ReadOnly,
    /// View > Monitoring follows the file.
    Monitoring,
    /// The text does not represent the file.
    Decoding,
}

/// The colors of a message of `kind`, from the interface colors of the theme in use, so that
/// it reads on a dark theme as on a light one.
pub fn colors(kind: Kind) -> BarColors {
    let ui = ui();
    let (background, border) = match kind {
        Kind::ReadOnly => (ui.surface, ui.border),
        Kind::Monitoring => (ui.selected, ui.accent),
        Kind::Decoding => (ui.match_background, ui.directory),
    };
    BarColors {
        background,
        border,
        text: ui.text,
    }
}

fn bar(kind: Kind) -> gpui_kit::Div {
    let colors = colors(kind);
    widgets::bar(colors, Line::Under)
        .gap_3()
        .px_3()
        .py_1()
        .text_color(paint(colors.text))
        .text_size(px(13.))
}

/// The text does not represent the file: read-only, with a way to pick another encoding or to
/// edit it anyway. `changes` is how many places saving would change; `editor` is the view's.
pub fn decoding_problem(
    encoding: &str,
    problem: DecodeProblem,
    changes: usize,
    editor: FocusHandle,
) -> AnyElement {
    let places = if changes == 1 {
        "1 place".to_owned()
    } else {
        format!("{changes} places")
    };
    let message = match problem {
        DecodeProblem::Malformed { offset } => format!(
            "This file is not valid {encoding}: the byte at offset {offset} cannot be decoded, \
             and saving would change {places}. It is open read-only so that saving cannot \
             damage it."
        ),
        DecodeProblem::NotReversible { offset } => format!(
            "In {encoding}, this file contains byte sequences that would change when saved: \
             {places}, the first at offset {offset}. It is open read-only."
        ),
    };
    bar(Kind::Decoding)
        .debug_selector(|| "banner-decoding".into())
        .child(div().flex_1().child(message))
        .child(
            Button::new("reopen-with-encoding")
                .debug_selector(|| "banner-reopen".into())
                .small()
                .outline()
                .label(t!("banner.reopen-with-encoding"))
                .dropdown_menu(encoding_menu),
        )
        .child(
            Button::new("edit-anyway")
                .debug_selector(|| "banner-edit-anyway".into())
                .small()
                .outline()
                .label(t!("banner.edit-anyway"))
                .on_click(move |_, window, cx| {
                    let action = RunCommand(Invocation::new("encoding.edit-anyway"));
                    editor.dispatch_action(&action, window, cx);
                }),
        )
        .into_any_element()
}

pub fn read_only_requested() -> AnyElement {
    bar(Kind::ReadOnly)
        .child(t!("banner.opened-read-only-from-the-command-line-ro"))
        .into_any_element()
}

pub fn monitoring() -> AnyElement {
    bar(Kind::Monitoring)
        .child(t!("banner.monitoring-tail-f-the-document-follows-the-file-and-cannot-be-edited-view-monitoring-stops-it"))
        .into_any_element()
}

pub fn read_only_file() -> AnyElement {
    bar(Kind::ReadOnly)
        .child(t!(
            "banner.this-file-is-read-only-use-save-as-to-keep-your-changes-in-another-file"
        ))
        .into_any_element()
}

/// Centered progress over the text area while the file is read.
pub fn loading(progress: f32) -> AnyElement {
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
                .bg(paint(ui().surface))
                .text_color(paint(ui().text))
                .border_1()
                .border_color(paint(ui().border))
                .child(format!("{} {:.0}%", t!("banner.loading"), progress * 100.)),
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
