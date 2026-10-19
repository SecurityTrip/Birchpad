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
