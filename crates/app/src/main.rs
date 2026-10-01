//! Birchpad desktop application.
//!
//! Phase 0 spike: one window with one document, to validate GPUI for the editor view.
//!
//! ```text
//! birchpad [FILE]
//! birchpad --generate [LINES]   # synthetic multilingual text, 1 000 000 lines by default
//! ```

mod editor;
mod line_element;

use std::fmt::Write as _;
use std::path::PathBuf;
use std::time::Instant;

use birchpad_core::{Document, Rope};
use gpui_kit::{App, AppContext as _, Bounds, WindowBounds, WindowOptions, px, size};

use crate::editor::{Editor, LoadInfo, Quit};

fn main() {
    let source = Source::from_args(std::env::args().skip(1));

    gpui_kit::application().run(move |cx: &mut App| {
        gpui_kit::init(cx);
        editor::bind_keys(cx);
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        let (document, info) = source.load();
        let bounds = Bounds::centered(None, size(px(1100.), px(760.)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            ..WindowOptions::default()
        };
        let opened = gpui_kit::open_window(options, cx, |window, cx| {
            cx.new(|cx| Editor::new(document, info, window, cx))
        });
        if let Err(error) = opened {
            eprintln!("failed to open the main window: {error:#}");
            cx.quit();
        }
        cx.activate(true);
    });
}

enum Source {
    Empty,
    File(PathBuf),
    Generated(usize),
}

impl Source {
    fn from_args(mut args: impl Iterator<Item = String>) -> Self {
        match args.next().as_deref() {
            None => Self::Empty,
            Some("--generate") => Self::Generated(
                args.next()
                    .and_then(|n| n.parse().ok())
                    .unwrap_or(1_000_000),
            ),
            Some(path) => Self::File(PathBuf::from(path)),
        }
    }

    fn load(self) -> (Document, LoadInfo) {
        let started = Instant::now();
        let (text, path, lossy) = match self {
            Self::Empty => (String::new(), None, false),
            Self::Generated(lines) => (generate(lines), None, false),
            Self::File(path) => match std::fs::read(&path) {
                Ok(bytes) => match String::from_utf8(bytes) {
                    Ok(text) => (text, Some(path), false),
                    Err(error) => {
                        let text = String::from_utf8_lossy(error.as_bytes()).into_owned();
                        (text, Some(path), true)
                    }
                },
                Err(error) => {
                    eprintln!("cannot open {}: {error}", path.display());
                    (String::new(), Some(path), false)
                }
            },
        };
        let document = Document::from_text(Rope::from_str(&text));
        (
            document,
            LoadInfo {
                path,
                load_time: started.elapsed(),
                lossy,
            },
        )
    }
}

/// Multilingual text for stress-testing scrolling and shaping.
fn generate(lines: usize) -> String {
    const SAMPLES: [&str; 4] = [
        "The quick brown fox jumps over the lazy dog.",
        "Съешь же ещё этих мягких французских булок, да выпей чаю.",
        "\tfn main() { println!(\"héllo, wörld\"); } // 😀 ✓",
        "日本語のテキストと English mixed together.",
    ];
    let mut text = String::with_capacity(lines * 72);
    for i in 0..lines {
        let _ = write!(text, "{:>8}  {}\r\n", i + 1, SAMPLES[i % SAMPLES.len()]);
    }
    text
}
