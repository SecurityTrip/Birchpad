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

/// What the status bar shows about the active view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StatusInfo {
    pub(crate) length: usize,
    pub(crate) lines: usize,
    /// 1-based line and column of the primary caret; the column counts tab stops.
    pub(crate) line: usize,
    pub(crate) column: usize,
    /// 1-based position (byte offset + 1), as Notepad++ shows it.
    pub(crate) pos: usize,
    pub(crate) selected_chars: usize,
    pub(crate) selected_lines: usize,
    pub(crate) format: Format,
    pub(crate) overwrite: bool,
    /// The language's description ("Rust file"), or `None` for normal text.
    pub(crate) language: Option<&'static str>,
}

impl StatusInfo {
    pub(crate) fn of(view: &mut EditorView, cx: &App) -> Self {
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
            language: view
                .buffer
                .read(cx)
                .language()
                .map(|language| language.description),
        }
    }

    /// The texts of the sections, in order (also used by tests).
    pub(crate) fn sections(&self) -> [String; 6] {
        [
            format!("Length : {}    Lines : {}", self.length, self.lines),
            format!(
                "Ln : {}    Col : {}    Pos : {}",
                self.line, self.column, self.pos
            ),
            format!("Sel : {} | {}", self.selected_chars, self.selected_lines),
            line_ending_name(self.format.line_ending).to_owned(),
            birchpad_io::display_name(self.format.encoding, self.format.bom),
            if self.overwrite { "OVR" } else { "INS" }.to_owned(),
        ]
    }
}

pub(crate) fn line_ending_name(line_ending: LineEnding) -> &'static str {
    match line_ending {
        LineEnding::CrLf => "Windows (CR LF)",
        LineEnding::Lf => "Unix (LF)",
        LineEnding::Cr => "Macintosh (CR)",
    }
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

pub(crate) fn render<V: 'static>(
    info: Option<StatusInfo>,
    ansi: Encoding,
    _: &mut Context<V>,
) -> impl IntoElement {
    let mut bar = div()
        .flex()
        .flex_row()
        .h(px(24.))
        .flex_none()
        .items_center()
        .border_t_1()
        .border_color(crate::theme::paint(crate::theme::ui().border))
        .bg(crate::theme::paint(crate::theme::ui().surface))
        .text_size(px(12.));
    let Some(info) = info else {
        return bar.child(div().px_3().flex_1().child("Normal text file"));
    };
    bar = bar.child(
        div()
            .px_3()
            .flex_1()
            .child(info.language.unwrap_or("Normal text file")),
    );
    let [length, position, selection, eol, encoding, mode] = info.sections();
    let format = info.format;
    bar = bar
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
            line_ending_name(eol),
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
                Placeholder::RecentFiles | Placeholder::Languages | Placeholder::Themes,
            ) => {}
        }
    }
    menu
}

#[cfg(test)]
mod tests {
    use gpui_kit::{Entity, TestAppContext, VisualTestContext};

    use super::*;
    use crate::workspace::Workspace;
    use crate::workspace::tests::{document_start, open_workspace};

    fn sections(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> [String; 6] {
        workspace.update(cx, |workspace, cx| {
            let view = workspace.active_view(cx).unwrap();
            view.update(cx, |view, cx| StatusInfo::of(view, cx))
                .sections()
        })
    }

    #[gpui_kit::test]
    fn an_empty_document_is_one_empty_line(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        assert_eq!(
            sections(&workspace, cx),
            [
                "Length : 0    Lines : 1",
                "Ln : 1    Col : 1    Pos : 1",
                "Sel : 0 | 0",
                line_ending_name(LineEnding::native()),
                "UTF-8",
                "INS",
            ]
        );
    }

    #[test]
    fn line_endings_have_notepad_plus_plus_names() {
        assert_eq!(line_ending_name(LineEnding::CrLf), "Windows (CR LF)");
        assert_eq!(line_ending_name(LineEnding::Lf), "Unix (LF)");
        assert_eq!(line_ending_name(LineEnding::Cr), "Macintosh (CR)");
    }

    #[gpui_kit::test]
    fn several_selections_add_up_and_virtual_space_counts_columns(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        cx.simulate_input("abc\nde\nfgh");
        let view = workspace.read_with(cx, |workspace, cx| workspace.active_view(cx).unwrap());
        view.update(cx, |view, cx| {
            // "ab" on line 1, and from "e" over the line break to "f" (two lines).
            view.selection = birchpad_core::Selection::new(
                [
                    birchpad_core::Range::new(0, 2),
                    birchpad_core::Range::new(5, 8),
                ],
                1,
            );
            cx.notify();
        });
        let [_, position, selection, _, _, _] = sections(&workspace, cx);
        assert_eq!(selection, "Sel : 5 | 3");
        assert_eq!(position, "Ln : 3    Col : 2    Pos : 9", "after the f");
        // A caret three columns past the end of "de".
        view.update(cx, |view, cx| {
            view.selection =
                birchpad_core::Selection::single(birchpad_core::Range::point(6).with_virtual(3, 3));
            cx.notify();
        });
        let [_, position, _, _, _, _] = sections(&workspace, cx);
        assert_eq!(position, "Ln : 2    Col : 6    Pos : 7");
    }

    #[gpui_kit::test]
    fn the_language_and_the_format_are_shown(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        let info = |cx: &mut VisualTestContext| {
            workspace.update(cx, |workspace, cx| {
                let view = workspace.active_view(cx).unwrap();
                view.update(cx, |view, cx| StatusInfo::of(view, cx))
            })
        };
        assert_eq!(info(cx).language, None, "normal text");
        let run = |invocation: Invocation, cx: &mut VisualTestContext| {
            workspace.update_in(cx, |workspace, window, cx| {
                workspace.dispatch(&invocation, window, cx).unwrap();
            });
            cx.run_until_parked();
        };
        run(
            Invocation::with_args("language.set", json!({ "language": "rust" })),
            cx,
        );
        assert_eq!(info(cx).language, Some("Rust file"));
        run(
            Invocation::with_args("encoding.convert-to", json!({ "encoding": "utf-16be-bom" })),
            cx,
        );
        run(
            Invocation::with_args("edit.convert-eol", json!({ "eol": "cr" })),
            cx,
        );
        let [_, _, _, eol, encoding, _] = info(cx).sections();
        assert_eq!(
            (eol.as_str(), encoding.as_str()),
            ("Macintosh (CR)", "UTF-16 BE BOM")
        );
    }

    #[gpui_kit::test]
    fn the_line_ending_menu_converts(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        cx.simulate_input("a\nb");
        let button = cx.debug_bounds("status-eol").expect("drawn").center();
        cx.simulate_click(button, gpui_kit::Modifiers::none());
        // Windows (CR LF), Unix (LF), Macintosh (CR): the third.
        cx.simulate_keystrokes("down down down enter");
        let format = workspace.read_with(cx, |workspace, cx| {
            let view = workspace.active_view(cx).unwrap();
            let buffer = view.read(cx).buffer.read(cx);
            (
                buffer.doc().text().to_string(),
                buffer.doc().format().line_ending,
            )
        });
        assert_eq!(format, ("a\rb".to_owned(), LineEnding::Cr));
    }

    #[gpui_kit::test]
    fn the_encoding_menu_converts_and_escape_chooses_nothing(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        cx.simulate_input("text");
        let encoding = |cx: &mut VisualTestContext| {
            workspace.read_with(cx, |workspace, cx| {
                let view = workspace.active_view(cx).unwrap();
                let format = view.read(cx).buffer.read(cx).doc().format();
                birchpad_io::display_name(format.encoding, format.bom)
            })
        };
        let button = cx.debug_bounds("status-encoding").expect("drawn").center();
        cx.simulate_click(button, gpui_kit::Modifiers::none());
        cx.simulate_keystrokes("down escape");
        assert_eq!(encoding(cx), "UTF-8", "escape chose nothing");

        // Encode in ANSI and the four Unicode forms, Character Sets, then Convert to ANSI,
        // UTF-8, UTF-8-BOM and UTF-16 BE BOM: the tenth item past the separator.
        cx.simulate_click(button, gpui_kit::Modifiers::none());
        cx.simulate_keystrokes(&["down"; 10].join(" "));
        cx.simulate_keystrokes("enter");
        assert_eq!(encoding(cx), "UTF-16 BE BOM");
    }

    #[gpui_kit::test]
    fn shows_what_notepad_plus_plus_shows(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        cx.simulate_input("\tжук\nabc");
        let [length, position, selection, _, encoding, mode] = sections(&workspace, cx);
        assert_eq!(length, "Length : 11    Lines : 2");
        assert_eq!(position, "Ln : 2    Col : 4    Pos : 12");
        assert_eq!(selection, "Sel : 0 | 0");
        assert_eq!((encoding.as_str(), mode.as_str()), ("UTF-8", "INS"));

        // The column counts the tab to its stop; the selection counts characters and lines.
        cx.simulate_keystrokes(&format!("{} end", document_start()));
        let [_, position, _, _, _, _] = sections(&workspace, cx);
        assert_eq!(position, "Ln : 1    Col : 8    Pos : 8");
        cx.simulate_keystrokes("shift-down insert");
        let [_, _, selection, _, _, mode] = sections(&workspace, cx);
        assert_eq!(selection, "Sel : 4 | 2");
        assert_eq!(mode, "OVR");
    }
}
