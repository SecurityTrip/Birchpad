//! File > Print... and File > Export to HTML... (ADR 0034): the document as HTML in the
//! theme's colors, with its syntax highlighting and line numbers. Printing opens it in the
//! browser, whose print dialog chooses the printer, the pages and the paper; GPUI has no
//! printing of its own.

use std::fmt::Write as _;
use std::ops::Range;
use std::path::PathBuf;

use anyhow::{Context as _, Result, anyhow};
use birchpad_core::Rope;
use gpui_kit::{App, Context, Window};

use crate::commands::CommandRegistry;
use crate::editor::theme::{self, TextStyle};
use crate::workspace::{Workspace, report_error};

/// Escapes text for HTML.
fn escape(text: &str, out: &mut String) {
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(ch),
        }
    }
}

fn css_color(color: u32) -> String {
    let [r, g, b, a] = color.to_be_bytes();
    if a == 0xff {
        format!("#{r:02x}{g:02x}{b:02x}")
    } else {
        format!("rgba({r},{g},{b},{:.3})", f32::from(a) / 255.)
    }
}

fn css(style: &TextStyle) -> String {
    let mut css = String::new();
    if let Some(color) = style.color {
        let _ = write!(css, "color:{};", css_color(color));
    }
    if let Some(background) = style.background {
        let _ = write!(css, "background:{};", css_color(background));
    }
    if style.bold {
        css.push_str("font-weight:bold;");
    }
    if style.italic {
        css.push_str("font-style:italic;");
    }
    if style.underline {
        css.push_str("text-decoration:underline;");
    }
    css
}

/// Pieces of `len` bytes that do not overlap, each with the style of the last span over it:
/// highlights nest, and the inner (later) one wins, as on screen.
pub(crate) fn flatten(
    spans: &[(Range<usize>, TextStyle)],
    len: usize,
) -> Vec<(Range<usize>, Option<usize>)> {
    let mut events: Vec<(usize, bool, usize)> = Vec::with_capacity(spans.len() * 2);
    for (index, (range, _)) in spans.iter().enumerate() {
        let (start, end) = (range.start.min(len), range.end.min(len));
        if start < end {
            events.push((start, true, index));
            events.push((end, false, index));
        }
    }
    events.sort_unstable_by_key(|&(pos, starts, index)| (pos, starts, index));
    let mut active = std::collections::BTreeSet::new();
    let mut pieces: Vec<(Range<usize>, Option<usize>)> = Vec::new();
    let mut pos = 0;
    for (at, starts, index) in events {
        if at > pos {
            let style = active.last().copied();
            match pieces.last_mut() {
                Some((last, last_style)) if *last_style == style => last.end = at,
                _ => pieces.push((pos..at, style)),
            }
            pos = at;
        }
        if starts {
            active.insert(index);
        } else {
            active.remove(&index);
        }
    }
    if pos < len {
        match pieces.last_mut() {
            Some((last, None)) => last.end = len,
            _ => pieces.push((pos..len, None)),
        }
    }
    pieces
}

/// The document as an HTML page: `title`, its text in `spans`' styles, with line numbers.
pub(crate) fn to_html(
    title: &str,
    text: &Rope,
    spans: &[(Range<usize>, TextStyle)],
    print: bool,
) -> String {
    let editor = theme::editor();
    let source = text.to_string();
    let mut html = String::with_capacity(source.len() * 2 + 1024);
    html.push_str("<!DOCTYPE html>\n<html><head><meta charset=\"utf-8\"><title>");
    escape(title, &mut html);
    let _ = write!(
        html,
        "</title><style>body{{margin:0;background:{bg};color:{fg}}}\
         pre{{margin:0;padding:8px;font:10pt Consolas,'DejaVu Sans Mono',Menlo,monospace;\
         white-space:pre-wrap;word-wrap:break-word}}\
         .n{{color:{gutter};user-select:none;display:inline-block;min-width:4ch;\
         text-align:right;margin-right:1ch}}</style>",
        bg = css_color(editor.background.0),
        fg = css_color(editor.text.0),
        gutter = css_color(editor.gutter_text.0),
    );
    if print {
        html.push_str("<script>window.addEventListener('load',()=>window.print())</script>");
    }
    html.push_str("</head><body><pre>");
    let lines = source.split_inclusive('\n').count().max(1);
    let width = lines.to_string().len();
    let mut line = 1;
    let number = |line: usize, html: &mut String| {
        let _ = write!(html, "<span class=\"n\">{line:>width$}</span>");
    };
    number(line, &mut html);
    for (range, style) in flatten(spans, source.len()) {
        let piece = &source[range];
        let open = style
            .map(|index| css(&spans[index].1))
            .filter(|css| !css.is_empty());
        let mut first = true;
        for part in piece.split_inclusive('\n') {
            if !first {
                line += 1;
                number(line, &mut html);
            }
            first = false;
            let (body, newline) = match part.strip_suffix('\n') {
                Some(body) => (body.strip_suffix('\r').unwrap_or(body), true),
                None => (part, false),
            };
            if let Some(css) = &open {
                let _ = write!(html, "<span style=\"{css}\">");
                escape(body, &mut html);
                html.push_str("</span>");
            } else {
                escape(body, &mut html);
            }
            if newline {
                html.push('\n');
            }
        }
    }
    html.push_str("</pre></body></html>\n");
    html
}

/// The active document as HTML.
fn active_html(workspace: &Workspace, print: bool, cx: &App) -> Result<(String, String)> {
    let view = workspace
        .active_view(cx)
        .ok_or_else(|| anyhow!("there is no document"))?;
    let buffer = view.read(cx).buffer.read(cx);
    let text = buffer.doc().text().clone();
    let title = buffer.display_name();
    let mut spans: Vec<(Range<usize>, TextStyle)> = Vec::new();
    if let Some(syntax) = buffer.syntax() {
        let language = Some(syntax.language().id);
        for (range, highlight) in syntax.all_highlights(&text, 0..text.len()) {
            if let Some(style) = theme::syntax_style(language, highlight) {
                spans.push((range, style));
            }
        }
    } else if let Some(language) = buffer.user_language() {
        for (range, kind) in buffer.udl_pieces(text.len()).iter() {
            if let Some(style) = crate::user_languages::style_of(language, *kind) {
                spans.push((range.clone(), style));
            }
        }
    }
    Ok((title.clone(), to_html(&title, &text, &spans, print)))
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
                let error = anyhow!("cannot write {}: {error}", path.display());
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

    fn style(color: u32) -> TextStyle {
        TextStyle {
            color: Some(color),
            background: None,
            bold: false,
            italic: false,
            underline: false,
        }
    }

    #[test]
    fn nested_spans_flatten_with_the_inner_one_winning() {
        let spans = vec![(0..10, style(1)), (2..4, style(2)), (8..20, style(3))];
        assert_eq!(
            flatten(&spans, 12),
            [
                (0..2, Some(0)),
                (2..4, Some(1)),
                (4..8, Some(0)),
                (8..12, Some(2)),
            ]
        );
        assert_eq!(flatten(&[], 3), [(0..3, None)]);
        assert!(flatten(&[], 0).is_empty());
        // Empty and out-of-range spans are left out.
        assert_eq!(
            flatten(&[(5..5, style(1)), (9..12, style(1))], 4),
            [(0..4, None)]
        );
    }

    #[test]
    fn html_escapes_numbers_lines_and_colors() {
        let text = Rope::from_str("if a < b {\r\n  x\n}");
        let spans = vec![(
            0..2,
            TextStyle {
                bold: true,
                ..style(0x0000ffff)
            },
        )];
        let html = to_html("a <b>.rs", &text, &spans, false);
        assert!(html.contains("<title>a &lt;b&gt;.rs</title>"), "{html}");
        assert!(
            html.contains("<span style=\"color:#0000ff;font-weight:bold;\">if</span> a &lt; b {\n")
        );
        assert!(html.contains("<span class=\"n\">3</span>}"));
        assert!(!html.contains('\r'));
        assert!(!html.contains("window.print"));
        assert!(to_html("x", &text, &[], true).contains("window.print()"));
        // An empty document has its one line.
        assert!(
            to_html("x", &Rope::new(), &[], false).contains("<span class=\"n\">1</span></pre>")
        );
    }

    #[test]
    fn colors_and_paths() {
        assert_eq!(css_color(0x12345680), "rgba(18,52,86,0.502)");
        assert_eq!(
            css(&TextStyle {
                color: None,
                background: Some(0xffffffff),
                bold: false,
                italic: true,
                underline: true
            }),
            "background:#ffffff;font-style:italic;text-decoration:underline;"
        );
        let path = print_path("new 1:<x>");
        assert!(path.ends_with("birchpad-print-new_1__x_.html"));
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
