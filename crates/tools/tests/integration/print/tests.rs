//! Pages: the grid, wrapping, line numbers, the header and footer, colors and HTML.

use birchpad_theme::{Color, EditorColors, Style, Theme};
use birchpad_tools::print::*;

fn editor() -> EditorColors {
    Theme::default_theme().editor.clone()
}

fn variables() -> Variables {
    Variables {
        full_path: "C:\\docs\\a.txt".into(),
        file_name: "a.txt".into(),
        directory: "C:\\docs".into(),
        short_date: "2026-10-20".into(),
        long_date: "Tuesday, 20 October 2026".into(),
        time: "12:34".into(),
    }
}

/// A setup whose page holds `columns` cells and `rows` rows: 10 points, margins that leave
/// exactly that much of an A4 page.
fn grid(columns: usize, rows: usize) -> PageSetup {
    let setup = PageSetup::default();
    let (cell, row) = setup.cell();
    let (width, height) = setup.page_size();
    let across = width - columns as f32 * cell - 0.01;
    let down = height - rows as f32 * row - 0.01;
    PageSetup {
        margins: Margins {
            top: down / 2.0,
            bottom: down / 2.0,
            left: across / 2.0,
            right: across / 2.0,
        },
        line_numbers: false,
        ..setup
    }
}

fn pages_of(text: &str, setup: &PageSetup) -> Pages {
    let document = Document {
        text,
        first_line: 1,
        spans: &[],
    };
    paginate(&document, setup, &variables(), &editor())
}

/// Each page's rows as text, with their numbers.
fn texts(pages: &Pages) -> Vec<Vec<(Option<usize>, String)>> {
    pages
        .pages
        .iter()
        .map(|page| {
            page.rows
                .iter()
                .map(|row| {
                    let mut text = String::new();
                    for piece in &row.pieces {
                        while text.chars().count() < piece.column {
                            text.push('.');
                        }
                        text.push_str(&piece.text);
                    }
                    (row.number, text)
                })
                .collect()
        })
        .collect()
}

#[test]
fn variables_fill_in_and_the_rest_stays() {
    let variables = variables();
    assert_eq!(
        expand(
            "$(FILE_NAME) - page $(CURRENT_PRINTING_PAGE)",
            &variables,
            3
        ),
        "a.txt - page 3"
    );
    assert_eq!(
        expand(
            "$(FULL_CURRENT_PATH)|$(CURRENT_DIRECTORY)|$(SHORT_DATE)|$(LONG_DATE)|$(TIME)",
            &variables,
            1
        ),
        "C:\\docs\\a.txt|C:\\docs|2026-10-20|Tuesday, 20 October 2026|12:34"
    );
    // An unknown variable, one without its end, and nothing at all stay as written.
    assert_eq!(
        expand("$(NOPE) $(FILE_NAME", &variables, 1),
        "$(NOPE) $(FILE_NAME"
    );
    assert_eq!(expand("", &variables, 1), "");
    assert_eq!(expand("$", &variables, 1), "$");
    assert_eq!(expand("$()", &variables, 1), "$()");
}

#[test]
fn papers_and_landscape() {
    assert_eq!(Paper::A4.size(), (210.0, 297.0));
    assert_eq!(Paper::Letter.size(), (215.9, 279.4));
    for paper in [Paper::A3, Paper::A5, Paper::Legal] {
        let (width, height) = paper.size();
        assert!(width < height, "{paper:?} upright");
    }
    let landscape = PageSetup {
        landscape: true,
        ..PageSetup::default()
    };
    assert_eq!(landscape.page_size(), (297.0, 210.0));
    let (upright_columns, upright_rows) = PageSetup::default().grid();
    let (columns, rows) = landscape.grid();
    assert!(columns > upright_columns && rows < upright_rows);
}

#[test]
fn a_page_holds_its_rows_and_the_next_one_starts_a_page() {
    let setup = grid(20, 3);
    assert_eq!(setup.grid(), (20, 3));
    assert_eq!(texts(&pages_of("a\nb\nc", &setup)).len(), 1, "exactly full");
    let four = pages_of("a\nb\nc\nd", &setup);
    assert_eq!(
        texts(&four),
        [
            vec![
                (Some(1), "a".into()),
                (Some(2), "b".into()),
                (Some(3), "c".into())
            ],
            vec![(Some(4), "d".into())],
        ]
    );
    assert_eq!(
        four.pages
            .iter()
            .map(|page| page.number)
            .collect::<Vec<_>>(),
        [1, 2]
    );
}

#[test]
fn an_empty_document_is_one_empty_page() {
    let pages = pages_of("", &PageSetup::default());
    assert_eq!(pages.pages.len(), 1);
    assert_eq!(pages.pages[0].rows.len(), 1);
    assert!(pages.pages[0].rows[0].pieces.is_empty());
    assert_eq!(pages.pages[0].rows[0].number, Some(1));
}

#[test]
fn line_breaks_of_every_kind_and_the_last_line() {
    let setup = grid(20, 10);
    assert_eq!(
        texts(&pages_of("a\r\nb\nc\r", &setup))[0],
        [
            (Some(1), "a".to_owned()),
            (Some(2), "b".to_owned()),
            (Some(3), "c".to_owned())
        ]
    );
    // A final line break ends the last line: no empty line after it.
    assert_eq!(
        texts(&pages_of("a\n", &setup))[0],
        [(Some(1), "a".to_owned())]
    );
    // Empty lines in between are rows.
    assert_eq!(
        texts(&pages_of("a\n\nb", &setup))[0],
        [
            (Some(1), "a".to_owned()),
            (Some(2), String::new()),
            (Some(3), "b".to_owned())
        ]
    );
}

#[test]
fn long_lines_wrap_at_the_last_cell() {
    let setup = grid(4, 10);
    assert_eq!(
        texts(&pages_of("abcd", &setup))[0],
        [(Some(1), "abcd".to_owned())],
        "exactly as wide"
    );
    assert_eq!(
        texts(&pages_of("abcde", &setup))[0],
        [(Some(1), "abcd".to_owned()), (None, "e".to_owned())],
        "one more"
    );
    // A wide character that does not fit goes to the next row whole.
    assert_eq!(
        texts(&pages_of("abc日", &setup))[0],
        [(Some(1), "abc".to_owned()), (None, "日".to_owned())]
    );
    // A page one cell wide still takes every character, one a row.
    let narrow = grid(1, 10);
    assert_eq!(
        texts(&pages_of("ab", &narrow))[0],
        [(Some(1), "a".to_owned()), (None, "b".to_owned())]
    );
}

#[test]
fn tabs_become_spaces_to_the_next_stop() {
    let setup = PageSetup {
        tab_width: 4,
        ..grid(20, 10)
    };
    assert_eq!(
        texts(&pages_of("a\tb", &setup))[0],
        [(Some(1), "a   b".to_owned())]
    );
    let zero = PageSetup {
        tab_width: 0,
        ..grid(20, 10)
    };
    assert_eq!(
        texts(&pages_of("\tb", &zero))[0],
        [(Some(1), " b".to_owned())],
        "a tab width of 0 counts as 1"
    );
}

#[test]
fn line_numbers_take_the_width_of_the_last_one() {
    let setup = PageSetup {
        line_numbers: true,
        ..grid(10, 20)
    };
    let text: Vec<String> = (1..=10).map(|n| n.to_string()).collect();
    let pages = pages_of(&text.join("\n"), &setup);
    assert_eq!(pages.number_width, 3, "two digits and a space");
    // A selection starting at line 99 numbers from there.
    let document = Document {
        text: "x\ny",
        first_line: 99,
        spans: &[],
    };
    let pages = paginate(&document, &setup, &variables(), &editor());
    assert_eq!(pages.number_width, 4);
    assert_eq!(pages.pages[0].rows[1].number, Some(100));
    // The text has the cells left after the numbers.
    let narrow = PageSetup {
        line_numbers: true,
        ..grid(5, 20)
    };
    assert_eq!(
        texts(&pages_of("abcde", &narrow))[0],
        [(Some(1), "abc".to_owned()), (None, "de".to_owned())]
    );
}

#[test]
fn header_and_footer_take_their_rows_when_they_have_text() {
    let mut setup = grid(20, 5);
    setup.header = ["$(FILE_NAME)".into(), String::new(), " ".into()];
    setup.footer = [
        String::new(),
        "$(CURRENT_PRINTING_PAGE)".into(),
        String::new(),
    ];
    let pages = pages_of("1\n2\n3", &setup);
    // Five rows: the header, a gap, one line, a gap, the footer.
    assert_eq!(pages.pages.len(), 3);
    assert_eq!(pages.pages[0].header, ["a.txt", "", ""]);
    assert_eq!(pages.pages[2].footer, ["", "3", ""]);
    // Only spaces: no header.
    setup.header = [" ".into(), String::new(), String::new()];
    setup.footer = Default::default();
    let pages = pages_of("1\n2\n3", &setup);
    assert_eq!(pages.pages.len(), 1);
    assert_eq!(pages.pages[0].header, ["", "", ""]);
    // A page too small for the header and the footer still has a row of text.
    let mut tiny = grid(20, 1);
    tiny.header = ["h".into(), String::new(), String::new()];
    tiny.footer = ["f".into(), String::new(), String::new()];
    assert_eq!(pages_of("1\n2", &tiny).pages.len(), 2);
}

#[test]
fn margins_larger_than_the_paper_leave_one_cell() {
    let setup = PageSetup {
        margins: Margins {
            top: 500.0,
            bottom: 500.0,
            left: 500.0,
            right: 500.0,
        },
        font_size: 0.0,
        ..PageSetup::default()
    };
    assert_eq!(setup.grid(), (1, 1));
    assert_eq!(pages_of("ab", &setup).pages.len(), 2);
}

#[test]
fn styles_cover_their_ranges_and_the_inner_one_wins() {
    let red = Style::color(Color::rgb(0xff0000));
    let blue = Style::color(Color::rgb(0x0000ff));
    let spans = [(0..5, red), (2..3, blue), (9..99, red)];
    let document = Document {
        text: "abcde\nfghij",
        first_line: 1,
        spans: &spans,
    };
    let setup = grid(20, 10);
    let pages = paginate(&document, &setup, &variables(), &editor());
    let first = &pages.pages[0].rows[0].pieces;
    let shown: Vec<(&str, usize, Option<Color>)> = first
        .iter()
        .map(|piece| (piece.text.as_str(), piece.column, piece.style.color))
        .collect();
    assert_eq!(
        shown,
        [
            ("ab", 0, red.color),
            ("c", 2, blue.color),
            ("de", 3, red.color)
        ]
    );
    let second = &pages.pages[0].rows[1].pieces;
    assert_eq!(second[0].text, "fgh");
    assert_eq!(second[0].style.color, Some(editor().text), "unstyled text");
    assert_eq!(second[1].text, "ij");
}

#[test]
fn colors_as_the_mode_says() {
    let editor = editor();
    let style = Style {
        color: Some(Color::rgb(0x102030)),
        background: Some(Color::rgb(0xffff00)),
        bold: true,
        italic: false,
        underline: true,
    };
    let wysiwyg = print_style(style, &editor, PrintColors::Wysiwyg);
    assert_eq!(wysiwyg, style);
    let inverted = print_style(style, &editor, PrintColors::Invert);
    assert_eq!(inverted.color, Some(Color::rgb(0xefdfcf)));
    assert_eq!(inverted.background, Some(Color::rgb(0x0000ff)));
    let black = print_style(style, &editor, PrintColors::BlackOnWhite);
    assert_eq!((black.color, black.background), (Some(Color::rgb(0)), None));
    assert!(black.bold && black.underline, "the styles stay");
    let plain = print_style(style, &editor, PrintColors::NoBackground);
    assert_eq!((plain.color, plain.background), (style.color, None));
    // Unstyled text takes the theme's text color.
    assert_eq!(
        print_style(Style::default(), &editor, PrintColors::Wysiwyg).color,
        Some(editor.text)
    );
    // The paper: the theme's background, or white.
    let on = |colors| {
        let setup = PageSetup {
            colors,
            ..PageSetup::default()
        };
        pages_of("x", &setup).background
    };
    assert_eq!(on(PrintColors::Wysiwyg), editor.background);
    assert_eq!(on(PrintColors::BlackOnWhite), Color::rgb(0xffffff));
    assert_eq!(on(PrintColors::NoBackground), Color::rgb(0xffffff));
    assert_ne!(on(PrintColors::Invert), editor.background);
}

#[test]
fn the_page_for_the_browser_has_every_page() {
    let mut setup = grid(20, 3);
    setup.line_numbers = true;
    setup.header = ["<$(FILE_NAME)>".into(), String::new(), String::new()];
    setup.footer = [
        String::new(),
        String::new(),
        "$(CURRENT_PRINTING_PAGE)".into(),
    ];
    let bold = Style {
        bold: true,
        italic: true,
        underline: true,
        background: Some(Color::rgb(0xeeeeee)),
        ..Style::color(Color(0x11223380))
    };
    let spans = [(0..1, bold)];
    let document = Document {
        text: "a & b\nc\nd",
        first_line: 1,
        spans: &spans,
    };
    let pages = paginate(&document, &setup, &variables(), &editor());
    let html = to_html("x < y", &pages, &setup, true);
    assert_eq!(
        html.matches("<div class=\"page\">").count(),
        pages.pages.len()
    );
    assert!(html.contains("<title>x &lt; y</title>"));
    assert!(html.contains("&lt;a.txt&gt;"));
    assert!(html.contains("a &amp; b") || html.contains("&amp;"));
    assert!(html.contains("window.print()"));
    assert!(html.contains("size:210mm 297mm"));
    assert!(html.contains("font-weight:bold;font-style:italic;text-decoration:underline;"));
    assert!(html.contains("rgba(17,34,51,0.502)"));
    assert!(!to_html("x", &pages, &setup, false).contains("window.print()"));
}
