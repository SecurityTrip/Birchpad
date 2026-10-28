use birchpad_editor::banner::{self, Kind};
use birchpad_editor::theme::{paint, set};
use birchpad_editor::widgets::{BarColors, Line, ListRow as _, bar, error_text, row_background};
use birchpad_theme::Theme;
use gpui_kit::{Fill, Styled as _, div, px};

#[test]
fn a_bar_of_controls_is_on_the_theme_s_surface() {
    for theme in [
        Theme::default_theme().clone(),
        Theme::builtin("Dark").unwrap().clone(),
    ] {
        let ui = theme.ui;
        set(theme);
        assert_eq!(
            BarColors::surface(),
            BarColors {
                background: ui.surface,
                border: ui.border,
                text: ui.text,
            }
        );
    }
    set(Theme::default_theme().clone());
}

#[test]
fn a_bar_has_its_colors_and_its_line_on_one_side() {
    set(Theme::default_theme().clone());
    let colors = BarColors::surface();
    let mut under = bar(colors, Line::Under);
    let style = under.style();
    assert_eq!(style.background, Some(Fill::from(paint(colors.background))));
    assert_eq!(style.border_color, Some(paint(colors.border).into()));
    assert_eq!(style.border_widths.bottom, Some(px(1.).into()));
    assert_eq!(style.border_widths.top, None);

    let mut over = bar(colors, Line::Over);
    let style = over.style();
    assert_eq!(style.border_widths.top, Some(px(1.).into()));
    assert_eq!(style.border_widths.bottom, None);
    assert_eq!(style.border_widths.left, None);
    assert_eq!(style.border_widths.right, None);
}

/// The messages above the text are bars too, in their own colors.
#[test]
fn a_message_above_the_text_keeps_its_own_colors() {
    set(Theme::default_theme().clone());
    let colors = banner::colors(Kind::Monitoring);
    assert_ne!(colors, BarColors::surface());
    let mut message = bar(colors, Line::Under);
    assert_eq!(
        message.style().background,
        Some(Fill::from(paint(colors.background)))
    );
}

#[test]
fn only_the_selected_row_of_a_list_is_colored() {
    set(Theme::default_theme().clone());
    let ui = Theme::default_theme().ui;
    assert_eq!(row_background(true), Some(ui.selected));
    assert_eq!(row_background(false), None);

    let mut selected = div().list_row(true);
    assert_eq!(
        selected.style().background,
        Some(Fill::from(paint(ui.selected)))
    );
    let mut other = div().list_row(false);
    assert_eq!(other.style().background, None);
}

#[test]
fn what_went_wrong_is_in_the_theme_s_error_color() {
    for theme in Theme::builtins() {
        let error = theme.ui.error;
        set(theme.clone());
        let mut text = error_text("The file is gone.");
        assert_eq!(text.style().text.color, Some(paint(error).into()));
    }
    set(Theme::default_theme().clone());
}
