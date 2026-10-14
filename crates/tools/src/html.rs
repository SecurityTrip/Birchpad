//! A document as an HTML page, for File > Print... and File > Export to HTML... (ADR 0034): its
//! text in the theme's colors and its highlighting styles, with line numbers.

use std::fmt::Write as _;
use std::ops::Range;

use birchpad_core::Rope;
use birchpad_theme::{EditorColors, Style};

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

fn css(style: &Style) -> String {
    let mut css = String::new();
    if let Some(color) = style.color {
        let _ = write!(css, "color:{};", css_color(color.0));
    }
    if let Some(background) = style.background {
        let _ = write!(css, "background:{};", css_color(background.0));
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
pub fn flatten(spans: &[(Range<usize>, Style)], len: usize) -> Vec<(Range<usize>, Option<usize>)> {
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

/// The document as an HTML page: `title`, its text in `spans`' styles on the theme's `editor`
/// colors, with line numbers; with `print`, the page opens the browser's print dialog.
pub fn to_html(
    title: &str,
    text: &Rope,
    spans: &[(Range<usize>, Style)],
    editor: &EditorColors,
    print: bool,
) -> String {
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

#[cfg(test)]
mod tests {
    use birchpad_theme::Color;

    use super::*;

    fn style(color: u32) -> Style {
        Style::color(Color(color))
    }

    fn colors() -> EditorColors {
        birchpad_theme::Theme::default_theme().editor
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
            Style {
                bold: true,
                ..style(0x0000ffff)
            },
        )];
        let html = to_html("a <b>.rs", &text, &spans, &colors(), false);
        assert!(html.contains("<title>a &lt;b&gt;.rs</title>"), "{html}");
        assert!(
            html.contains("<span style=\"color:#0000ff;font-weight:bold;\">if</span> a &lt; b {\n")
        );
        assert!(html.contains("<span class=\"n\">3</span>}"));
        assert!(!html.contains('\r'));
        assert!(!html.contains("window.print"));
        assert!(to_html("x", &text, &[], &colors(), true).contains("window.print()"));
        // An empty document has its one line.
        assert!(
            to_html("x", &Rope::new(), &[], &colors(), false)
                .contains("<span class=\"n\">1</span></pre>")
        );
    }

    #[test]
    fn colors_in_css() {
        assert_eq!(css_color(0x12345680), "rgba(18,52,86,0.502)");
        assert_eq!(
            css(&Style {
                color: None,
                background: Some(Color(0xffffffff)),
                bold: false,
                italic: true,
                underline: true
            }),
            "background:#ffffff;font-style:italic;text-decoration:underline;"
        );
    }
}
