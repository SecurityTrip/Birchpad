//! A docked, read-only table that follows the active editor's document.

use std::ops::Range;
use std::rc::Rc;

use birchpad_core::RevisionId;
use birchpad_tools::csv::{self, StringRecord, TableError};
use gpui_kit::{
    App, Context, EntityId, FocusHandle, Focusable, UniformListScrollHandle, Window, div,
    prelude::*, px, uniform_list,
};

use crate::{FollowView, Host};

const CELL_WIDTH: f32 = 150.;

struct Parsed {
    document: (EntityId, RevisionId),
    table: Result<Rc<Vec<StringRecord>>, TableError>,
}

pub struct CsvViewer {
    host: Rc<dyn Host>,
    follow: FollowView,
    parsed: Option<Parsed>,
    focus: FocusHandle,
    scroll: UniformListScrollHandle,
}

impl CsvViewer {
    pub fn new(host: Rc<dyn Host>, cx: &mut Context<Self>) -> Self {
        Self {
            host,
            follow: FollowView::default(),
            parsed: None,
            focus: cx.focus_handle(),
            scroll: UniformListScrollHandle::new(),
        }
    }

    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        let view = self.host.active_view(cx);
        self.follow.follow(view.as_ref(), cx);
        let Some(view) = view else {
            self.parsed = None;
            return;
        };
        let buffer = &view.read(cx).buffer;
        let doc = buffer.read(cx).doc();
        let key = (buffer.entity_id(), doc.revision());
        if self
            .parsed
            .as_ref()
            .is_some_and(|parsed| parsed.document == key)
        {
            return;
        }
        if self
            .parsed
            .as_ref()
            .is_none_or(|parsed| parsed.document.0 != key.0)
        {
            self.scroll = UniformListScrollHandle::new();
        }
        let table = if doc.text().len() > csv::MAX_BYTES {
            Err(TableError::TooManyBytes)
        } else {
            csv::parse(&doc.text().to_string()).map(Rc::new)
        };
        self.parsed = Some(Parsed {
            document: key,
            table,
        });
    }

    #[cfg(feature = "test-support")]
    pub fn rows(&self) -> Option<Result<&[StringRecord], &TableError>> {
        self.parsed
            .as_ref()
            .map(|parsed| parsed.table.as_ref().map(|rows| rows.as_slice()))
    }
}

impl Focusable for CsvViewer {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

fn cell(text: String) -> gpui_kit::Div {
    div()
        .w(px(CELL_WIDTH))
        .flex_none()
        .px_2()
        .truncate()
        .border_r_1()
        .border_color(birchpad_widgets::theme::paint(
            birchpad_widgets::theme::ui().muted,
        ))
        .child(text)
}

fn display_cell(text: &str) -> String {
    text.chars()
        .take(256)
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect()
}

fn error_label(error: &TableError) -> String {
    match error {
        TableError::TooManyBytes => t!("panels.csv-viewer.too-large"),
        TableError::TooManyColumns => t!(
            "panels.csv-viewer.too-many-columns",
            count = csv::MAX_COLUMNS
        ),
        TableError::TooManyRows => t!("panels.csv-viewer.too-many-rows", count = csv::MAX_ROWS),
        TableError::Invalid(error) => t!("panels.csv-viewer.invalid", error = error),
    }
}

impl Render for CsvViewer {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.refresh(cx);
        let message = |text: String| div().p_2().child(text).into_any_element();
        let body = match self.parsed.as_ref().map(|parsed| &parsed.table) {
            None => message(t!("panels.csv-viewer.no-document")),
            Some(Err(error)) => message(error_label(error)),
            Some(Ok(rows)) if rows.is_empty() => message(t!("panels.csv-viewer.empty")),
            Some(Ok(rows)) => {
                let columns = rows[0].len();
                let rows = rows.clone();
                let heading = div()
                    .flex()
                    .flex_none()
                    .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                    .children((1..=columns).map(|index| cell(index.to_string())));
                let list = uniform_list(
                    "csv-viewer-rows",
                    rows.len(),
                    cx.processor(move |_, range: Range<usize>, _, _| {
                        range
                            .map(|index| {
                                div().flex().h(px(24.)).children(
                                    rows[index].iter().map(|value| cell(display_cell(value))),
                                )
                            })
                            .collect::<Vec<_>>()
                    }),
                )
                .track_scroll(&self.scroll)
                .flex_1();
                div()
                    .flex()
                    .flex_col()
                    .h_full()
                    .w(px(CELL_WIDTH * columns as f32))
                    .child(heading)
                    .child(list)
                    .into_any_element()
            }
        };
        div()
            .id("csv-viewer")
            .track_focus(&self.focus)
            .size_full()
            .overflow_x_scroll()
            .child(body)
    }
}
