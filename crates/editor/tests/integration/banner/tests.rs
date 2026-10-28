use birchpad_editor::banner::{BarColors, Kind, colors};
use birchpad_editor::theme::{set, set_interface};
use birchpad_theme::{Color, Theme};

const KINDS: [Kind; 3] = [Kind::ReadOnly, Kind::Monitoring, Kind::Decoding];

/// The WCAG contrast ratio of two opaque colors, from 1 to 21.
fn contrast(a: Color, b: Color) -> f32 {
    let luminance = |color: Color| {
        let [r, g, b, _] = color.0.to_be_bytes();
        let linear = |channel: u8| {
            let c = f32::from(channel) / 255.;
            if c <= 0.039_28 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b)
    };
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

#[test]
fn the_light_theme_keeps_its_colors() {
    set(Theme::default_theme().clone());
    let ui = Theme::default_theme().ui;
    assert_eq!(
        colors(Kind::ReadOnly),
        BarColors {
            background: ui.surface,
            border: ui.border,
            text: ui.text,
        }
    );
    assert_eq!(colors(Kind::Monitoring).background, ui.selected);
    assert_eq!(colors(Kind::Monitoring).border, ui.accent);
    assert_eq!(colors(Kind::Decoding).background, ui.match_background);
}

/// #57: on the dark theme the messages were drawn light, with the theme's light text on them.
#[test]
fn the_dark_theme_draws_them_dark() {
    let dark = Theme::builtin("Dark").unwrap().clone();
    let text = dark.ui.text;
    set(dark);
    for kind in KINDS {
        let colors = colors(kind);
        assert!(colors.background.is_dark(), "{kind:?} on a dark background");
        assert_eq!(colors.text, text, "{kind:?} in the theme's text color");
        assert!(!colors.text.is_dark(), "{kind:?} in light text");
    }
    set(Theme::default_theme().clone());
}

#[test]
fn every_message_reads_on_every_built_in_theme() {
    for theme in Theme::builtins() {
        set(theme.clone());
        for kind in KINDS {
            let colors = colors(kind);
            let ratio = contrast(colors.text, colors.background);
            assert!(ratio >= 4.5, "{kind:?} on {}: {ratio}", theme.name);
            assert_ne!(colors.background, colors.border, "{kind:?} has an edge");
        }
    }
    set(Theme::default_theme().clone());
}

/// Notepad++'s Dark Mode tones replace the interface colors alone; the messages follow them.
#[test]
fn an_interface_tone_over_the_theme_is_followed() {
    set(Theme::default_theme().clone());
    let toned = Theme::builtin("Dark").unwrap().ui.toned(200.);
    set_interface(Some(toned));
    assert_eq!(colors(Kind::ReadOnly).background, toned.surface);
    assert_eq!(colors(Kind::Decoding).text, toned.text);
    set_interface(None);
    assert_eq!(
        colors(Kind::ReadOnly).background,
        Theme::default_theme().ui.surface
    );
}
