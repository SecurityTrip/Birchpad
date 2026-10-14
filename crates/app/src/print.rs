//! File > Print... and File > Export to HTML... (ADR 0034): the document as HTML in the
//! theme's colors, with its syntax highlighting and line numbers (`birchpad_tools::html`).
//! Printing opens it in the browser, whose print dialog chooses the printer, the pages and the
//! paper; GPUI has no printing of its own.

use std::ops::Range;
use std::path::PathBuf;

use anyhow::{Context as _, Result, anyhow};
use birchpad_tools::html::to_html;
use gpui_kit::{App, Context, Window};

use crate::commands::{CommandRegistry, Register as _};
use crate::editor::theme;
use crate::workspace::{Workspace, report_error};

/// The active document as HTML.
fn active_html(workspace: &Workspace, print: bool, cx: &App) -> Result<(String, String)> {
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

pub(crate) fn register_commands(registry: &mut CommandRegistry) {
    registry.workspace("file.print", |this, (), _, cx| {
        let (title, html) = active_html(this, true, cx)?;
        let path = print_path(&title);
        std::fs::write(&path, html).with_context(|| format!("cannot write {}", path.display()))?;
        cx.open_url(&format!(
            "file:///{}",
            path.display()
                .to_string()
                .replace('\\', "/")
                .trim_start_matches('/')
        ));
        Ok(())
    });
    registry.workspace("file.export-html", |this, (), window, cx| {
        this.export_html(window, cx);
        Ok(())
    });
}

/// Where the page to print goes: a temporary file named after the document.
pub(crate) fn print_path(title: &str) -> PathBuf {
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

impl Workspace {
    fn export_html(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (title, html) = match active_html(self, false, cx) {
            Ok(found) => found,
            Err(error) => {
                report_error(&error, window, cx);
                return;
            }
        };
        let start = self.default_directory(cx);
        cx.spawn_in(window, async move |_, cx| {
            let Some(path) = crate::path_dialog::ask_save(start, format!("{title}.html"), cx).await
            else {
                return;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_to_print_is_named_after_the_document() {
        let path = print_path("new 1:<x>");
        assert!(path.ends_with("birchpad-print-new_1__x_.html"));
        assert!(
            print_path("").ends_with("birchpad-print-.html"),
            "an empty title"
        );
    }

    #[gpui_kit::test]
    fn print_writes_the_page_with_the_highlights(cx: &mut gpui_kit::TestAppContext) {
        use birchpad_commands::Invocation;
        use serde_json::json;
        let (workspace, cx) = crate::workspace::tests::open_workspace(cx);
        let run = |invocation: Invocation, cx: &mut gpui_kit::VisualTestContext| {
            let result = workspace.update_in(cx, |workspace, window, cx| {
                workspace.dispatch(&invocation, window, cx)
            });
            cx.run_until_parked();
            result
        };
        run(
            Invocation::with_args("language.set", json!({ "language": "rust" })),
            cx,
        )
        .unwrap();
        run(
            Invocation::with_args("edit.insert-text", json!({ "text": "fn main() {}" })),
            cx,
        )
        .unwrap();
        // Let the syntax tree be parsed.
        for _ in 0..50 {
            cx.run_until_parked();
        }
        run(Invocation::new("file.print"), cx).unwrap();
        let page = std::fs::read_to_string(print_path("new 1")).unwrap();
        assert!(page.contains("window.print()"));
        assert!(page.contains("main"));
        let html = workspace.read_with(cx, |workspace, cx| {
            active_html(workspace, false, cx).unwrap().1
        });
        assert!(html.contains("<span style="), "{html}");
        std::fs::remove_file(print_path("new 1")).ok();
    }
}
