//! Character Panel, as Notepad++'s ASCII Codes Insertion Panel (Edit > Character Panel): the
//! 256 values of the document's code page with their characters and HTML entities.
//!
//! Values 128 to 255 are the characters of the active document's legacy encoding, or of the
//! ANSI code page for a Unicode document, as in Notepad++; a value its code page leaves
//! undefined has no character. The C0 controls and DEL show their names. A double-click on a
//! row's value, hex or character inserts the character; on an HTML column, that entity.

use std::ops::Range;

use birchpad_core::Encoding;
use gpui_kit::{
    App, ClickEvent, Context, FocusHandle, Focusable, FontWeight, SharedString,
    UniformListScrollHandle, WeakEntity, Window, div, prelude::*, px, uniform_list,
};

use crate::app_state::AppState;
use crate::workspace::Workspace;

const ROW_HEIGHT: f32 = 22.;

/// The names of the C0 controls.
const CONTROLS: [&str; 32] = [
    "NUL", "SOH", "STX", "ETX", "EOT", "ENQ", "ACK", "BEL", "BS", "TAB", "LF", "VT", "FF", "CR",
    "SO", "SI", "DLE", "DC1", "DC2", "DC3", "DC4", "NAK", "SYN", "ETB", "CAN", "EM", "SUB", "ESC",
    "FS", "GS", "RS", "US",
];

/// HTML 4's named entities of Latin-1, from U+00A0.
const LATIN_1: [&str; 96] = [
    "nbsp", "iexcl", "cent", "pound", "curren", "yen", "brvbar", "sect", "uml", "copy", "ordf",
    "laquo", "not", "shy", "reg", "macr", "deg", "plusmn", "sup2", "sup3", "acute", "micro",
    "para", "middot", "cedil", "sup1", "ordm", "raquo", "frac14", "frac12", "frac34", "iquest",
    "Agrave", "Aacute", "Acirc", "Atilde", "Auml", "Aring", "AElig", "Ccedil", "Egrave", "Eacute",
    "Ecirc", "Euml", "Igrave", "Iacute", "Icirc", "Iuml", "ETH", "Ntilde", "Ograve", "Oacute",
    "Ocirc", "Otilde", "Ouml", "times", "Oslash", "Ugrave", "Uacute", "Ucirc", "Uuml", "Yacute",
    "THORN", "szlig", "agrave", "aacute", "acirc", "atilde", "auml", "aring", "aelig", "ccedil",
    "egrave", "eacute", "ecirc", "euml", "igrave", "iacute", "icirc", "iuml", "eth", "ntilde",
    "ograve", "oacute", "ocirc", "otilde", "ouml", "divide", "oslash", "ugrave", "uacute", "ucirc",
    "uuml", "yacute", "thorn", "yuml",
];

/// HTML 4's named entities of the characters Windows-1252 has from 0x80 to 0x9F.
const WINDOWS_1252: [(char, &str); 25] = [
    ('\u{20AC}', "euro"),
    ('\u{201A}', "sbquo"),
    ('\u{0192}', "fnof"),
    ('\u{201E}', "bdquo"),
    ('\u{2026}', "hellip"),
    ('\u{2020}', "dagger"),
    ('\u{2021}', "Dagger"),
    ('\u{02C6}', "circ"),
    ('\u{2030}', "permil"),
    ('\u{0160}', "Scaron"),
    ('\u{2039}', "lsaquo"),
    ('\u{0152}', "OElig"),
    ('\u{2018}', "lsquo"),
    ('\u{2019}', "rsquo"),
    ('\u{201C}', "ldquo"),
    ('\u{201D}', "rdquo"),
    ('\u{2022}', "bull"),
    ('\u{2013}', "ndash"),
    ('\u{2014}', "mdash"),
    ('\u{02DC}', "tilde"),
    ('\u{2122}', "trade"),
    ('\u{0161}', "scaron"),
    ('\u{203A}', "rsaquo"),
    ('\u{0153}', "oelig"),
    ('\u{0178}', "Yuml"),
];

/// The HTML entity name of `ch`, if HTML 4 has one.
pub fn entity_name(ch: char) -> Option<&'static str> {
    match ch {
        '"' => Some("quot"),
        '&' => Some("amp"),
        '<' => Some("lt"),
        '>' => Some("gt"),
        '\u{A0}'..='\u{FF}' => Some(LATIN_1[ch as usize - 0xA0]),
        _ => WINDOWS_1252
            .iter()
            .find(|(known, _)| *known == ch)
            .map(|(_, name)| *name),
    }
}

/// One value of the code page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharRow {
    pub value: u8,
    /// `None` where the code page has no character.
    pub ch: Option<char>,
    /// What the Character column shows: the character, or a control's name.
    pub shown: String,
    pub name: Option<&'static str>,
}

impl CharRow {
    pub fn html_name(&self) -> Option<String> {
        self.name.map(|name| format!("&{name};"))
    }

    pub fn html_decimal(&self) -> Option<String> {
        self.ch.map(|ch| format!("&#{};", u32::from(ch)))
    }

    pub fn html_hex(&self) -> Option<String> {
        self.ch.map(|ch| format!("&#x{:X};", u32::from(ch)))
    }
}

/// The 256 rows for the legacy `encoding`.
pub fn rows(encoding: Encoding) -> Vec<CharRow> {
    (0..=255u8)
        .map(|value| {
            let ch = if value < 0x80 {
                Some(char::from(value))
            } else {
                let decoded = birchpad_io::decode(vec![value], encoding);
                let mut chars = decoded.text.chars();
                match (decoded.problem, chars.next(), chars.next()) {
                    (None, Some(ch), None) => Some(ch),
                    _ => None,
                }
            };
            let shown = match value {
                0..=31 => CONTROLS[usize::from(value)].to_owned(),
                127 => "DEL".to_owned(),
                _ => ch.map(String::from).unwrap_or_default(),
            };
            CharRow {
                value,
                ch,
                shown,
                name: ch.and_then(entity_name),
            }
        })
        .collect()
}

/// The columns, as (heading, width): narrow enough for the dock's width.
const COLUMNS: [(&str, f32); 6] = [
    ("Value", 42.),
    ("Hex", 30.),
    ("Char", 44.),
    ("HTML name", 66.),
    ("HTML dec", 56.),
    ("HTML hex", 60.),
];

pub struct CharacterPanel {
    workspace: WeakEntity<Workspace>,
    /// The rows, and the encoding they are of.
    rows: Option<(Encoding, Vec<CharRow>)>,
    pub selected: Option<u8>,
    scroll: UniformListScrollHandle,
    focus_handle: FocusHandle,
}

impl Focusable for CharacterPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl CharacterPanel {
    pub fn new(workspace: WeakEntity<Workspace>, _: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            workspace,
            rows: None,
            selected: None,
            scroll: UniformListScrollHandle::new(),
            focus_handle: cx.focus_handle(),
        }
    }

    /// The legacy encoding of the active document, or the ANSI code page.
    pub fn encoding(&self, cx: &App) -> Encoding {
        let active = self.workspace.upgrade().and_then(|workspace| {
            let view = workspace.read(cx).active_view(cx)?;
            Some(view.read(cx).buffer.read(cx).doc().format().encoding)
        });
        match active {
            Some(encoding @ Encoding::Legacy(_)) => encoding,
            _ => AppState::global(cx).ansi,
        }
    }

    /// The rows for the active document.
    pub fn current_rows(&mut self, cx: &App) -> &[CharRow] {
        let encoding = self.encoding(cx);
        if self
            .rows
            .as_ref()
            .is_none_or(|(known, _)| *known != encoding)
        {
            self.rows = Some((encoding, rows(encoding)));
        }
        &self.rows.as_ref().expect("just set").1
    }

    /// Inserts what column `column` of row `value` holds, if anything.
    pub fn insert(
        &mut self,
        value: u8,
        column: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(row) = self.current_rows(cx).get(usize::from(value)).cloned() else {
            return;
        };
        let text = match column {
            0..=2 => row.ch.map(String::from),
            3 => row.html_name(),
            4 => row.html_decimal(),
            _ => row.html_hex(),
        };
        if let Some(text) = text {
            self.workspace
                .update(cx, |workspace, cx| workspace.insert_text(&text, window, cx))
                .ok();
        }
    }
}

impl Render for CharacterPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let rows: Vec<[SharedString; 6]> = self
            .current_rows(cx)
            .iter()
            .map(|row| {
                [
                    row.value.to_string().into(),
                    format!("{:02X}", row.value).into(),
                    row.shown.clone().into(),
                    row.html_name().unwrap_or_default().into(),
                    row.html_decimal().unwrap_or_default().into(),
                    row.html_hex().unwrap_or_default().into(),
                ]
            })
            .collect();
        let header = div()
            .flex()
            .flex_row()
            .flex_none()
            .h(px(24.))
            .items_center()
            .border_b_1()
            .border_color(crate::theme::paint(crate::theme::ui().border))
            .font_weight(FontWeight::SEMIBOLD)
            .children(COLUMNS.iter().map(|(heading, width)| {
                div()
                    .w(px(*width))
                    .px_1()
                    .flex_none()
                    .text_size(px(11.))
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .child(*heading)
            }));
        let list = uniform_list(
            "character-rows",
            rows.len(),
            cx.processor(move |this, range: Range<usize>, _, cx| {
                range
                    .map(|index| {
                        let value = index as u8;
                        div()
                            .id(("character-row", index))
                            .w_full()
                            .h(px(ROW_HEIGHT))
                            .flex()
                            .flex_row()
                            .items_center()
                            .whitespace_nowrap()
                            .when(this.selected == Some(value), |row| {
                                row.bg(crate::theme::paint(crate::theme::ui().selected))
                            })
                            .hover(|row| row.bg(crate::theme::paint(crate::theme::ui().hovered)))
                            .children(rows[index].iter().enumerate().map(|(column, text)| {
                                div()
                                    .id(("character-cell", index * 8 + column))
                                    .debug_selector(move || format!("character-{index}-{column}"))
                                    .w(px(COLUMNS[column].1))
                                    .h_full()
                                    .px_1()
                                    .flex_none()
                                    .flex()
                                    .items_center()
                                    .overflow_hidden()
                                    .cursor_pointer()
                                    .child(text.clone())
                                    .on_click(cx.listener(
                                        move |this, event: &ClickEvent, window, cx| {
                                            window.focus(&this.focus_handle, cx);
                                            this.selected = Some(value);
                                            if event.click_count() >= 2 {
                                                this.insert(value, column, window, cx);
                                            }
                                            cx.notify();
                                        },
                                    ))
                            }))
                    })
                    .collect::<Vec<_>>()
            }),
        )
        .track_scroll(&self.scroll)
        .flex_1();
        div()
            .id("character-panel")
            .debug_selector(|| "character-panel".into())
            .track_focus(&self.focus_handle)
            .size_full()
            .flex()
            .flex_col()
            .overflow_x_scroll()
            .child(header)
            .child(list)
    }
}
