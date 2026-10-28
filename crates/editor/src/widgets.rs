//! Parts that several parts of the window draw alike, in the theme's colors: bars across a part
//! of the window, the rows of lists, and the text that tells what went wrong.

use birchpad_theme::Color;
use gpui_kit::{Div, InteractiveElement, IntoElement, ParentElement as _, Styled, div};

use crate::theme::{paint, ui};

/// The colors of a bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BarColors {
    pub background: Color,
    pub border: Color,
    pub text: Color,
}

impl BarColors {
    /// A bar of controls (a toolbar, a panel's header, the status bar): the theme's surface,
    /// border and text.
    pub fn surface() -> Self {
        let ui = ui();
        Self {
            background: ui.surface,
            border: ui.border,
            text: ui.text,
        }
    }
}

/// Which side of a bar its line is on: under a bar at the top of what it belongs to, over one
/// at the bottom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Line {
    Under,
    Over,
}

/// A bar across a part of the window: a row on the background of `colors`, its items centered
/// across, and a line in their border color on the side `line` says.
pub fn bar(colors: BarColors, line: Line) -> Div {
    let bar = div()
        .flex()
        .flex_row()
        .flex_none()
        .items_center()
        .bg(paint(colors.background))
        .border_color(paint(colors.border));
    match line {
        Line::Under => bar.border_b_1(),
        Line::Over => bar.border_t_1(),
    }
}

/// The background of a row of a list: the selection's color for the selected row, none for
/// the others.
pub fn row_background(selected: bool) -> Option<Color> {
    selected.then(|| ui().selected)
}

/// A row of a list (a panel's, the document switcher's) in the theme's colors.
pub trait ListRow: InteractiveElement + Styled {
    /// On the selection's color when `selected`, and on the hover color under the mouse.
    fn list_row(self, selected: bool) -> Self {
        let row = match row_background(selected) {
            Some(color) => self.bg(paint(color)),
            None => self,
        };
        row.hover(|style| style.bg(paint(ui().hovered)))
    }
}

impl<E: InteractiveElement + Styled> ListRow for E {}

/// `text` telling what went wrong (a dialog's field, a failed command), in the theme's error
/// color.
pub fn error_text(text: impl IntoElement) -> Div {
    div().text_color(paint(ui().error)).child(text)
}
