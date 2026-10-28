//! The status bar, laid out like Notepad++'s:
//! `Length : N  Lines : M | Ln : x  Col : y  Pos : z | Sel : a | b | Windows (CR LF) | UTF-8 | INS`.
//! Clicking the line ending or the encoding opens the corresponding menu.

use std::rc::Rc;

use birchpad_commands::{Invocation, MenuItem, Placeholder};
use birchpad_core::motion::{line_count, line_of};
use birchpad_core::{Encoding, Format, LineEnding};
use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenu};
use gpui_kit::{App, Context, Window, div, prelude::*, px};
use serde_json::json;

use crate::commands::RunCommand;
use crate::editor::EditorView;
use crate::editor::widgets::{self, BarColors, Line};

/// What the status bar shows about the active view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusInfo {
    pub length: usize,
    pub lines: usize,
    /// 1-based line and column of the primary caret; the column counts tab stops.
    pub line: usize,
    pub column: usize,
    /// 1-based position (byte offset + 1), as Notepad++ shows it.
    pub pos: usize,
    pub selected_chars: usize,
    pub selected_lines: usize,
    pub format: Format,
    pub overwrite: bool,
    /// The language's description ("Rust file"), or `None` for normal text.
    pub language: Option<String>,
}

impl StatusInfo {
    pub fn of(view: &mut EditorView, cx: &App) -> Self {
        let text = view.text(cx).clone();
        let primary = view.selection.primary();
        let head = primary.head;
        let (mut selected_chars, mut selected_lines) = (0, 0);
        for range in view.selection.iter().filter(|range| !range.is_empty()) {
            selected_chars +=
                text.byte_to_char_idx(range.to()) - text.byte_to_char_idx(range.from());
            selected_lines += line_of(&text, range.to()) - line_of(&text, range.from()) + 1;
        }
        Self {
            length: text.len(),
            lines: line_count(&text),
            line: line_of(&text, head) + 1,
            // In virtual space (a rectangle past the end of the line) the column goes on.
            column: view.column_of(head, cx) + primary.head_virtual + 1,
            pos: head + 1,
            selected_chars,
            selected_lines,
            format: view.buffer.read(cx).doc().format(),
            overwrite: view.overwrite,
            language: {
                let buffer = view.buffer.read(cx);
                buffer
                    .language()
                    .map(|language| language.description.to_owned())
                    .or_else(|| {
                        buffer.user_language().map(|found| {
                            t!("status-bar.user-defined-language-file", name = found.name)
                        })
                    })
            },
        }
    }

    /// The texts of the sections, in order, in the interface's language.
    pub fn sections(&self) -> [String; 6] {
        [
            t!(
                "status-bar.length",
                length = self.length,
                lines = self.lines
            ),
            t!(
                "status-bar.position",
                line = self.line,
                column = self.column,
                pos = self.pos
            ),
            t!(
                "status-bar.selection",
                chars = self.selected_chars,
                lines = self.selected_lines
            ),
            translated_line_ending(self.format.line_ending),
            birchpad_io::display_name(self.format.encoding, self.format.bom),
            if self.overwrite {
                t!("status-bar.overwrite")
            } else {
                t!("status-bar.insert")
            },
        ]
    }
}

pub fn translated_line_ending(line_ending: LineEnding) -> String {
    let key = match line_ending {
        LineEnding::CrLf => "status-bar.eol-crlf",
        LineEnding::Lf => "status-bar.eol-lf",
        LineEnding::Cr => "status-bar.eol-cr",
        LineEnding::Nel => "status-bar.eol-nel",
    };
    t!(key)
}

fn section(child: impl IntoElement) -> gpui_kit::Div {
    div()
        .flex()
        .items_center()
        .h_full()
        .px_3()
        .border_l_1()
        .border_color(crate::theme::paint(crate::theme::ui().border))
        .child(child)
}

pub fn render<V: 'static>(
    info: Option<StatusInfo>,
    ansi: Encoding,
    cx: &mut Context<V>,
) -> impl IntoElement {
    let mut bar = widgets::bar(BarColors::surface(), Line::Over)
        .h(px(24.))
        .text_size(px(12.));
    // A download of an update that no window shows.
    let update = crate::updates::Updates::download(cx)
        .filter(|download| download.in_status_bar())
        .cloned()
        .map(|download| section(crate::updates::status_bar_item(download)));
    let Some(info) = info else {
        return bar
            .child(
                div()
                    .px_3()
                    .flex_1()
                    .child(t!("status-bar.normal-text-file")),
            )
            .children(update);
    };
    bar = bar.child(
        div().px_3().flex_1().child(
            info.language
                .clone()
                .unwrap_or_else(|| t!("status-bar.normal-text-file")),
        ),
    );
    let [length, position, selection, eol, encoding, mode] = info.sections();
    let format = info.format;
    bar = bar
        .children(update)
        .child(section(length))
        .child(section(position))
        .child(section(selection))
        .child(section(
            Button::new("status-eol")
                .debug_selector(|| "status-eol".into())
                .ghost()
                .xsmall()
                .label(eol)
                .dropdown_menu_with_anchor(gpui_kit::Anchor::BottomLeft, move |menu, _, _| {
                    eol_menu(menu, format.line_ending)
                }),
        ))
        .child(section(
            Button::new("status-encoding")
                .debug_selector(|| "status-encoding".into())
                .ghost()
                .xsmall()
                .label(encoding)
                .dropdown_menu_with_anchor(
                    gpui_kit::Anchor::BottomLeft,
                    move |menu, window, cx| encoding_menu(menu, format, ansi, window, cx),
                ),
        ))
        .child(section(mode));
    bar
}

fn eol_menu(mut menu: PopupMenu, current: LineEnding) -> PopupMenu {
    for (name, eol) in [
        ("crlf", LineEnding::CrLf),
        ("lf", LineEnding::Lf),
        ("cr", LineEnding::Cr),
    ] {
        let invocation = Invocation::with_args("edit.convert-eol", json!({ "eol": name }));
        menu = menu.menu_with_check(
            translated_line_ending(eol),
            eol == current,
            Box::new(RunCommand(invocation)),
        );
    }
    menu
}

/// The Encoding menu of the menu bar, as a popup.
fn encoding_menu(
    menu: PopupMenu,
    format: Format,
    ansi: Encoding,
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    let Some(model) = birchpad_commands::main_menu()
        .into_iter()
        .find(|menu| menu.title == "Encoding")
    else {
        return menu;
    };
    let checked: Rc<dyn Fn(&Invocation) -> bool> =
        Rc::new(move |invocation| crate::encoding_ui::is_current(invocation, format, ansi));
    add_items(menu, model.items, &checked, window, cx)
}

fn add_items(
    mut menu: PopupMenu,
    items: Vec<MenuItem>,
    checked: &Rc<dyn Fn(&Invocation) -> bool>,
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    for item in items {
        match item {
            MenuItem::Command { .. } => {
                let label = item.label().unwrap_or_default().to_owned();
                let MenuItem::Command { invocation, .. } = item else {
                    unreachable!()
                };
                let is_checked = checked(&invocation);
                menu = menu.menu_with_check(label, is_checked, Box::new(RunCommand(invocation)));
            }
            MenuItem::Separator => menu = menu.separator(),
            MenuItem::Submenu(submenu) => {
                let checked = checked.clone();
                menu = menu.submenu(submenu.title.clone(), window, cx, move |sub, window, cx| {
                    add_items(sub, submenu.items.clone(), &checked, window, cx)
                });
            }
            MenuItem::Placeholder(Placeholder::CharacterSets { command }) => {
                for (group, sets) in birchpad_io::CHARACTER_SETS {
                    let checked = checked.clone();
                    menu = menu.submenu(*group, window, cx, move |mut sub, _, _| {
                        for (name, label) in *sets {
                            let invocation =
                                Invocation::with_args(command, json!({ "encoding": name }));
                            let is_checked = checked(&invocation);
                            sub = sub.menu_with_check(
                                *label,
                                is_checked,
                                Box::new(RunCommand(invocation)),
                            );
                        }
                        sub
                    });
                }
            }
            // The status bar's menus show line endings and encodings only.
            MenuItem::Placeholder(
                Placeholder::RecentFiles
                | Placeholder::Languages
                | Placeholder::Themes
                | Placeholder::Macros
                | Placeholder::RunCommands
                | Placeholder::UserLanguages
                | Placeholder::SpellLanguages
                | Placeholder::Scripts
                | Placeholder::Plugins
                | Placeholder::Documents,
            ) => {}
        }
    }
    menu
}
