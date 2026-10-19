//! More of Notepad++'s View menu: View Current File in a browser, and Summary, a window with
//! the document's path, times and counts (characters without line breaks, words, lines,
//! length) and those of the selection.

use std::path::Path;
use std::time::SystemTime;

use anyhow::{anyhow, bail};
use birchpad_core::motion::{TextCounts, count_text};
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::{App, Window, div, prelude::*, px};
use serde::Deserialize;

use crate::commands::{CommandRegistry, Register as _};
use crate::file_commands::{Shell, shell};

/// The browsers View > View Current File in offers besides the default one.
pub(crate) const BROWSERS: [&str; 4] = ["firefox", "chrome", "edge", "safari"];

pub(crate) fn register_commands(registry: &mut CommandRegistry) {
    registry.workspace("view.in-browser", |this, args: BrowserArgs, _, cx| {
        let view = this
            .active_view(cx)
            .ok_or_else(|| anyhow!("{}", t!("file-ops.no-document")))?;
        let buffer = view.read(cx).buffer.read(cx);
        let path = buffer.path().map(Path::to_owned).ok_or_else(|| {
            anyhow!(
                "{}",
                t!(
                    "file-commands.has-no-file-yet",
                    name = buffer.display_name()
                )
            )
        })?;
        match args.browser.as_str() {
            "default" => {
                let url = format!("file:///{}", path.display().to_string().replace('\\', "/"));
                let url = url.replacen("file:////", "file:///", 1).replace(' ', "%20");
                shell(Shell::Browse(url), cx)
            }
            browser if BROWSERS.contains(&browser) => {
                shell(Shell::Browser(browser.to_owned(), path), cx)
            }
            other => bail!("{}", t!("view-extras.unknown-browser", browser = other)),
        }
    });
    registry.workspace("view.summary", |this, (), window, cx| {
        let view = this
            .active_view(cx)
            .ok_or_else(|| anyhow!("{}", t!("file-ops.no-document")))?;
        let view = view.read(cx);
        let buffer = view.buffer.read(cx);
        let text = buffer.doc().text();
        let selected: Vec<_> = view
            .selection
            .iter()
            .filter(|range| !range.is_empty())
            .map(|range| count_text(&text.slice(range.from()..range.to()).into()))
            .collect();
        let rows = summary_rows(
            buffer.path(),
            &buffer.display_name(),
            count_text(text),
            &selected,
        );
        open(rows, window, cx);
        Ok(())
    });
}

#[derive(Deserialize)]
struct BrowserArgs {
    browser: String,
}

/// A time as the summary shows it.
fn time(time: SystemTime) -> String {
    chrono::DateTime::<chrono::Local>::from(time)
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}

/// The summary's rows: label and value.
pub(crate) fn summary_rows(
    path: Option<&Path>,
    name: &str,
    counts: TextCounts,
    selected: &[TextCounts],
) -> Vec<(String, String)> {
    let mut rows = vec![(
        t!("view-extras.full-file-path"),
        path.map_or_else(|| name.to_owned(), |path| path.display().to_string()),
    )];
    if let Some(metadata) = path.and_then(|path| std::fs::metadata(path).ok()) {
        if let Ok(created) = metadata.created() {
            rows.push((t!("view-extras.created"), time(created)));
        }
        if let Ok(modified) = metadata.modified() {
            rows.push((t!("view-extras.modified"), time(modified)));
        }
    }
    rows.extend([
        (
            t!("view-extras.characters-without-line-endings"),
            counts.characters.to_string(),
        ),
        (t!("view-extras.words"), counts.words.to_string()),
        (t!("view-extras.lines"), counts.lines.to_string()),
        (
            t!("view-extras.document-length"),
            t!("view-extras.bytes", count = counts.bytes),
        ),
    ]);
    if !selected.is_empty() {
        let characters: usize = selected.iter().map(|counts| counts.characters).sum();
        let bytes: usize = selected.iter().map(|counts| counts.bytes).sum();
        let lines: usize = selected.iter().map(|counts| counts.lines).sum();
        rows.push((
            t!("view-extras.selected"),
            t!(
                "view-extras.selected-counts",
                characters = characters,
                bytes = bytes,
                lines = lines,
                ranges = selected.len()
            ),
        ));
    }
    rows
}

fn open(rows: Vec<(String, String)>, window: &mut Window, cx: &mut App) {
    window.open_dialog(cx, move |dialog, _, _| {
        let lines = rows.iter().enumerate().map(|(index, (label, value))| {
            div()
                .debug_selector(move || format!("summary-{index}"))
                .flex()
                .flex_row()
                .gap_2()
                .child(div().w(px(220.)).flex_none().child(label.clone()))
                .child(div().flex_1().min_w(px(0.)).child(value.clone()))
        });
        dialog
            .title(t!("view-extras.summary"))
            .w(px(560.))
            .child(div().flex().flex_col().gap_1().children(lines))
            .footer(DialogFooter::new().child(crate::workspace::dialog_close("common.close")))
    });
}

#[cfg(test)]
#[path = "../tests/unit/view_extras/tests.rs"]
mod tests;
