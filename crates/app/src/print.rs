//! File > Print..., Print Now and Export to HTML... (ADR 0034, ADR 0044).
//!
//! Print... shows Birchpad's print preview: the document, or its selection, laid out on pages as
//! Preferences > Print sets them up (`birchpad_tools::print`), with a header and a footer made of
//! Notepad++'s variables, line numbers and the colors chosen. On Windows the pages go to the
//! printer chosen in Windows' print dialog, and Print Now prints them on the default printer at
//! once; elsewhere, where GPUI and Birchpad have no printing of their own, they are opened in the
//! browser as a page of the same pages, whose print dialog prints them. Export to HTML saves
//! the document as one page in the theme's colors.

use std::ops::Range;
use std::path::PathBuf;
use std::rc::Rc;

use anyhow::{Result, anyhow};
use birchpad_config::{Paper, PrintColors, PrintSettings};
use birchpad_core::motion::line_of;
use birchpad_theme::{EditorColors, Style};
use birchpad_tools::html::to_html;
use birchpad_tools::print::{self as layout, Document, PageSetup, Pages, Variables};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::component::{Disableable as _, WindowExt as _};
use gpui_kit::{
    App, AppContext as _, Context, Task, WeakEntity, Window, div, prelude::*, px, relative,
};

use crate::commands::{CommandRegistry, Register as _};
use crate::editor::theme;
use crate::editor::widgets::error_text;
use crate::theme::{paint, ui};
use crate::workspace::{Workspace, report_error};

#[cfg(windows)]
pub mod windows;

/// What is printed: the active document, or its selection, with its styles.
#[derive(Debug, Clone)]
pub struct PrintSource {
    pub title: String,
    pub text: String,
    /// The number of the first line printed, from 1.
    pub first_line: usize,
    /// Styles of ranges of `text`, the inner one last.
    pub spans: Vec<(Range<usize>, Style)>,
    pub variables: Variables,
    pub editor: EditorColors,
    /// Only the selection is printed.
    pub selection: bool,
}

impl PrintSource {
    /// The active document's selection if it has one, else the whole document.
    pub fn active(workspace: &Workspace, cx: &App) -> Result<Self> {
        let view = workspace
            .active_view(cx)
            .ok_or_else(|| anyhow!("{}", t!("print.there-is-no-document")))?;
        let view = view.read(cx);
        let primary = view.selection.primary();
        let buffer = view.buffer.read(cx);
        let rope = buffer.doc().text();
        let selection = !primary.is_empty();
        let range = if selection {
            primary.from()..primary.to()
        } else {
            0..rope.len()
        };
        let mut spans: Vec<(Range<usize>, Style)> = Vec::new();
        let mut keep = |piece: Range<usize>, style: Style| {
            let start = piece.start.max(range.start);
            let end = piece.end.min(range.end);
            if start < end {
                spans.push((start - range.start..end - range.start, style));
            }
        };
        if let Some(syntax) = buffer.syntax() {
            let language = Some(syntax.language().id);
            for (piece, highlight) in syntax.all_highlights(rope, range.clone()) {
                if let Some(style) = theme::syntax_style(language, highlight) {
                    keep(piece, style.into());
                }
            }
        } else if let Some(language) = buffer.user_language() {
            for (piece, kind) in buffer.udl_pieces(rope.len()).iter() {
                if let Some(style) = crate::user_languages::style_of(language, *kind) {
                    keep(piece.clone(), style.into());
                }
            }
        }
        let title = buffer.display_name();
        let path = buffer.path();
        let now = chrono::Local::now().naive_local();
        let date = |pattern: &str| birchpad_tools::date_time::format(pattern, &now);
        let variables = Variables {
            full_path: path.map_or_else(|| title.clone(), |path| path.display().to_string()),
            file_name: path
                .and_then(|path| path.file_name())
                .map_or_else(|| title.clone(), |name| name.to_string_lossy().into_owned()),
            directory: path
                .and_then(|path| path.parent())
                .map(|folder| folder.display().to_string())
                .unwrap_or_default(),
            short_date: date("yyyy-MM-dd"),
            long_date: date("dddd, d MMMM yyyy"),
            time: date("HH:mm"),
        };
        Ok(Self {
            title,
            text: rope.slice(range.clone()).to_string(),
            first_line: line_of(rope, range.start) + 1,
            spans,
            variables,
            editor: theme::editor(),
            selection,
        })
    }

    /// The pages as `setup` lays them out.
    pub fn paginate(&self, setup: &PageSetup) -> Pages {
        let document = Document {
            text: &self.text,
            first_line: self.first_line,
            spans: &self.spans,
        };
        layout::paginate(&document, setup, &self.variables, &self.editor)
    }
}

/// The page setup of Preferences > Print, with the editor's tab width.
pub fn page_setup(cx: &App) -> PageSetup {
    let settings = &crate::app_state::AppState::global(cx).settings;
    setup_of(
        &settings.print,
        usize::from(settings.editor.tab_width.max(1)),
    )
}

/// `settings` as the layout takes them.
pub fn setup_of(settings: &PrintSettings, tab_width: usize) -> PageSetup {
    let millimeters = |value: u32| value as f32;
    PageSetup {
        paper: match settings.paper {
            Paper::A3 => layout::Paper::A3,
            Paper::A4 => layout::Paper::A4,
            Paper::A5 => layout::Paper::A5,
            Paper::Letter => layout::Paper::Letter,
            Paper::Legal => layout::Paper::Legal,
        },
        landscape: settings.landscape,
        margins: layout::Margins {
            top: millimeters(settings.margin_top),
            bottom: millimeters(settings.margin_bottom),
            left: millimeters(settings.margin_left),
            right: millimeters(settings.margin_right),
        },
        font_size: settings.font_size.clamp(1, 200) as f32,
        line_numbers: settings.line_numbers,
        header: [
            settings.header_left.clone(),
            settings.header_center.clone(),
            settings.header_right.clone(),
        ],
        form_feed: settings.form_feed_page_break,
        footer: [
            settings.footer_left.clone(),
            settings.footer_center.clone(),
            settings.footer_right.clone(),
        ],
        colors: match settings.colors {
            PrintColors::Wysiwyg => layout::PrintColors::Wysiwyg,
            PrintColors::Invert => layout::PrintColors::Invert,
            PrintColors::BlackOnWhite => layout::PrintColors::BlackOnWhite,
            PrintColors::NoBackground => layout::PrintColors::NoBackground,
        },
        tab_width,
    }
}

/// The active document as one HTML page in the theme's colors (Export to HTML).
pub fn active_html(workspace: &Workspace, print: bool, cx: &App) -> Result<(String, String)> {
    let view = workspace
        .active_view(cx)
        .ok_or_else(|| anyhow!("{}", t!("print.there-is-no-document")))?;
    let buffer = view.read(cx).buffer.read(cx);
    let text = buffer.doc().text().clone();
    let title = buffer.display_name();
    let mut spans: Vec<(Range<usize>, birchpad_theme::Style)> = Vec::new();
    if let Some(syntax) = buffer.syntax() {
        let language = Some(syntax.language().id);
        for (range, highlight) in syntax.all_highlights(&text, 0..text.len()) {
            if let Some(style) = theme::syntax_style(language, highlight) {
                spans.push((range, style.into()));
            }
        }
    } else if let Some(language) = buffer.user_language() {
        for (range, kind) in buffer.udl_pieces(text.len()).iter() {
            if let Some(style) = crate::user_languages::style_of(language, *kind) {
                spans.push((range.clone(), style.into()));
            }
        }
    }
    let html = to_html(&title, &text, &spans, &theme::editor(), print);
    Ok((title, html))
}

pub fn register_commands(registry: &mut CommandRegistry) {
    registry.workspace("file.print", |this, (), window, cx| {
        let source = PrintSource::active(this, cx)?;
        open_preview(cx.entity().downgrade(), source, window, cx);
        Ok(())
    });
    registry.workspace("file.print-now", |this, (), window, cx| {
        let source = PrintSource::active(this, cx)?;
        let setup = page_setup(cx);
        let pages = source.paginate(&setup);
        print_pages(&source.title, &pages, &setup, false, window, cx)
    });
    registry.workspace("file.export-html", |this, args: ExportArgs, window, cx| {
        this.export_html(args.path, window, cx);
        Ok(())
    });
}

/// File > Export to HTML: the file to write, or none to ask for one.
#[derive(Debug, Default, serde::Deserialize)]
struct ExportArgs {
    #[serde(default)]
    path: Option<PathBuf>,
}

/// Where the page to print goes: a temporary file named after the document.
pub fn print_path(title: &str) -> PathBuf {
    let safe: String = title
        .chars()
        .map(|ch| {
            if ch.is_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect();
    std::env::temp_dir().join(format!("birchpad-print-{safe}.html"))
}

#[cfg(feature = "test-support")]
thread_local! {
    /// What tests printed, instead of printing: the title, the number of pages and whether the
    /// printer was asked for.
    pub static PRINTED: std::cell::RefCell<Vec<(String, usize, bool)>> =
        std::cell::RefCell::default();
}

/// Prints `pages`: on Windows on the printer the user chooses (`ask`) or on the default one;
/// elsewhere through the browser's print dialog. A failure is said when it comes.
pub fn print_pages(
    title: &str,
    pages: &Pages,
    setup: &PageSetup,
    ask: bool,
    window: &mut Window,
    cx: &mut App,
) -> Result<()> {
    let printing = print_task(title, pages, setup, ask, window, cx);
    window
        .spawn(cx, async move |cx| {
            if let Err(error) = printing.await {
                cx.update(|window, cx| report_error(&error, window, cx))
                    .ok();
            }
        })
        .detach();
    Ok(())
}

/// Prints `pages` as [`print_pages`] does; the task ends when they went to the printer (or to
/// the browser).
pub fn print_task(
    title: &str,
    pages: &Pages,
    setup: &PageSetup,
    ask: bool,
    window: &mut Window,
    cx: &mut App,
) -> Task<Result<()>> {
    #[cfg(feature = "test-support")]
    {
        let _ = (setup, window, &cx);
        PRINTED.with(|printed| {
            printed
                .borrow_mut()
                .push((title.to_owned(), pages.pages.len(), ask))
        });
        Task::ready(Ok(()))
    }
    #[cfg(all(windows, not(feature = "test-support")))]
    {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        let owner = match HasWindowHandle::window_handle(window).map(|handle| handle.as_raw()) {
            Ok(RawWindowHandle::Win32(handle)) => handle.hwnd.get(),
            _ => 0,
        };
        let (title, pages, setup) = (title.to_owned(), pages.clone(), setup.clone());
        // The print dialog and the printing wait on their own thread, not on the window's.
        cx.background_spawn(async move {
            windows::print(&title, &pages, &setup, owner, ask)
                .map(|_| ())
                .map_err(|error| {
                    anyhow!("{}", t!("print.cannot-print", error = format!("{error:#}")))
                })
        })
    }
    #[cfg(all(not(windows), not(feature = "test-support")))]
    {
        let _ = (ask, window);
        let html = layout::to_html(title, pages, setup, true);
        let path = print_path(title);
        if let Err(error) = std::fs::write(&path, html) {
            return Task::ready(Err(anyhow!(
                "{}",
                t!("print.cannot-write", path = path.display(), error = error)
            )));
        }
        cx.open_url(&format!(
            "file:///{}",
            path.display()
                .to_string()
                .replace('\\', "/")
                .trim_start_matches('/')
        ));
        Task::ready(Ok(()))
    }
}

#[cfg(feature = "test-support")]
thread_local! {
    /// The print preview last opened, for tests to drive.
    pub static OPENED: std::cell::RefCell<Option<WeakEntity<PrintPreview>>> =
        std::cell::RefCell::default();
}

/// Opens the print preview of `source`.
pub fn open_preview(
    workspace: WeakEntity<Workspace>,
    source: PrintSource,
    window: &mut Window,
    cx: &mut App,
) {
    let preview = cx.new(|cx| PrintPreview::new(workspace, source, cx));
    #[cfg(feature = "test-support")]
    OPENED.with(|opened| *opened.borrow_mut() = Some(preview.downgrade()));
    let title = if preview.read(cx).source.selection {
        t!("print.print-preview-selection")
    } else {
        t!("print.print-preview")
    };
    window.open_dialog(cx, move |dialog, _, _| {
        dialog
            .title(title.clone())
            .w(px(640.))
            .child(preview.clone())
            .footer(DialogFooter::new().child(crate::workspace::dialog_close("common.close")))
    });
}

/// File > Print...: the pages as they will be printed, one at a time.
pub struct PrintPreview {
    workspace: WeakEntity<Workspace>,
    pub source: Rc<PrintSource>,
    pub setup: PageSetup,
    pub pages: Pages,
    /// The page shown, from 0.
    pub page: usize,
    pub error: Option<String>,
}

/// The width a page is shown at.
const PAGE_WIDTH: f32 = 600.0;

impl PrintPreview {
    pub fn new(workspace: WeakEntity<Workspace>, source: PrintSource, cx: &App) -> Self {
        let setup = page_setup(cx);
        let pages = source.paginate(&setup);
        Self {
            workspace,
            source: Rc::new(source),
            setup,
            pages,
            page: 0,
            error: None,
        }
    }

    /// Lays the pages out again if Preferences > Print changed since.
    pub fn refresh(&mut self, cx: &App) {
        let setup = page_setup(cx);
        if setup != self.setup {
            self.pages = self.source.paginate(&setup);
            self.setup = setup;
            self.page = self.page.min(self.pages.pages.len().saturating_sub(1));
        }
    }

    /// Shows the page `step` pages away, within the first and the last.
    pub fn go(&mut self, step: isize, cx: &mut Context<Self>) {
        let last = self.pages.pages.len().saturating_sub(1) as isize;
        self.page = (self.page as isize + step).clamp(0, last) as usize;
        cx.notify();
    }

    /// Print... in the preview: asks for the printer, and closes the preview.
    pub fn print(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.refresh(cx);
        match print_pages(
            &self.source.title,
            &self.pages,
            &self.setup,
            true,
            window,
            cx,
        ) {
            Ok(()) => window.close_dialog(cx),
            Err(error) => {
                self.error = Some(format!("{error:#}"));
                cx.notify();
            }
        }
    }

    /// Page Setup...: Preferences > Print, over the preview.
    pub fn page_setup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        crate::preferences::open_at(self.workspace.clone(), Some("Print"), window, cx);
    }

    /// The page shown, drawn at [`PAGE_WIDTH`].
    fn page_element(&self) -> impl IntoElement {
        let (width, height) = self.setup.page_size();
        let scale = PAGE_WIDTH / width;
        let (cell, row) = self.setup.cell();
        let margins = self.setup.margins;
        let font = self.setup.font_size * layout::MM_PER_POINT * scale;
        let mm = |value: f32| px(value * scale);
        let pages = &self.pages;
        let page = pages.pages.get(self.page);
        let parts = |parts: &[String; 3], top: f32| {
            let mut line = div()
                .absolute()
                .left(mm(margins.left))
                .w(mm(width - margins.left - margins.right))
                .top(mm(top))
                .h(mm(row))
                .flex()
                .flex_row()
                .text_color(paint(pages.text));
            for (index, part) in parts.iter().enumerate() {
                let cell = div().flex_1().overflow_hidden().child(part.clone());
                line = line.child(match index {
                    0 => cell,
                    1 => cell.text_center(),
                    _ => cell.text_right(),
                });
            }
            line
        };
        let mut sheet = div()
            .id("print-page")
            .relative()
            .w(mm(width))
            .h(mm(height))
            .flex_none()
            .overflow_hidden()
            .bg(paint(pages.background))
            .border_1()
            .border_color(paint(ui().border))
            .font_family(crate::MONOSPACE)
            .text_size(px(font))
            .line_height(mm(row));
        let Some(page) = page else {
            return sheet;
        };
        let mut top = margins.top;
        if page.header.iter().any(|part| !part.is_empty()) {
            sheet = sheet.child(parts(&page.header, top));
            top += 2.0 * row;
        }
        for line in &page.rows {
            if let Some(number) = line.number.filter(|_| pages.number_width > 0) {
                sheet = sheet.child(
                    div()
                        .absolute()
                        .left(mm(margins.left))
                        .top(mm(top))
                        .text_color(paint(pages.numbers))
                        .whitespace_nowrap()
                        .child(format!("{number:>width$}", width = pages.number_width - 1)),
                );
            }
            for piece in &line.pieces {
                let style = piece.style;
                let mut text = div()
                    .absolute()
                    .left(mm(
                        margins.left + (pages.number_width + piece.column) as f32 * cell
                    ))
                    .top(mm(top))
                    .whitespace_nowrap()
                    .child(piece.text.clone());
                if let Some(color) = style.color {
                    text = text.text_color(paint(color));
                }
                if let Some(background) = style.background {
                    text = text.bg(paint(background));
                }
                if style.bold {
                    text = text.font_weight(gpui_kit::FontWeight::BOLD);
                }
                if style.italic {
                    text = text.italic();
                }
                if style.underline {
                    text = text.underline();
                }
                sheet = sheet.child(text);
            }
            top += row;
        }
        if page.footer.iter().any(|part| !part.is_empty()) {
            sheet = sheet.child(parts(&page.footer, height - margins.bottom - row));
        }
        sheet
    }
}

impl Render for PrintPreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.refresh(cx);
        let count = self.pages.pages.len();
        let first = self.page == 0;
        let last = self.page + 1 >= count;
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("print-previous")
                            .label(t!("print.previous-page"))
                            .disabled(first)
                            .on_click(cx.listener(|this, _, _, cx| this.go(-1, cx))),
                    )
                    .child(div().id("print-page-number").child(t!(
                        "print.page-of",
                        page = self.page + 1,
                        pages = count
                    )))
                    .child(
                        Button::new("print-next")
                            .label(t!("print.next-page"))
                            .disabled(last)
                            .on_click(cx.listener(|this, _, _, cx| this.go(1, cx))),
                    )
                    .child(div().flex_1())
                    .child(
                        Button::new("print-page-setup")
                            .label(t!("print.page-setup"))
                            .on_click(
                                cx.listener(|this, _, window, cx| this.page_setup(window, cx)),
                            ),
                    )
                    .child(
                        Button::new("print-print")
                            .primary()
                            .label(t!("print.print"))
                            .on_click(cx.listener(|this, _, window, cx| this.print(window, cx))),
                    ),
            )
            .child(
                div()
                    .id("print-sheet")
                    .h(px(560.))
                    .w(relative(1.))
                    .overflow_y_scroll()
                    .flex()
                    .justify_center()
                    .child(self.page_element()),
            )
            .children(self.error.clone().map(|error| error_text(error)))
    }
}

impl Workspace {
    fn export_html(&mut self, to: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) {
        let (title, html) = match active_html(self, false, cx) {
            Ok(found) => found,
            Err(error) => {
                report_error(&error, window, cx);
                return;
            }
        };
        let start = self.default_directory(cx);
        cx.spawn_in(window, async move |_, cx| {
            let path = match to {
                Some(path) => path,
                None => {
                    let asked = crate::path_dialog::ask_save(start, format!("{title}.html"), cx);
                    let Some(path) = asked.await else {
                        return;
                    };
                    path
                }
            };
            if let Err(error) = std::fs::write(&path, html) {
                let error = anyhow!(
                    "{}",
                    t!("print.cannot-write", path = path.display(), error = error)
                );
                cx.update(|window, cx| report_error(&error, window, cx))
                    .ok();
            }
        })
        .detach();
    }
}
