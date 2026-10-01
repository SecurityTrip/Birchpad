//! Birchpad desktop application.
//!
//! ```text
//! birchpad [FILE]...
//! birchpad --generate [LINES]   # synthetic multilingual text, 1 000 000 lines by default
//! ```

mod app_state;
mod banner;
mod buffer;
mod commands;
mod editor;
mod encoding_ui;
mod line_element;
mod menus;
mod pane;
mod workspace;

use std::fmt::Write as _;
use std::path::PathBuf;

use birchpad_core::{Document, Rope};
use gpui_kit::{App, AppContext as _, Bounds, WindowBounds, WindowOptions, px, size};

use crate::app_state::AppState;
use crate::workspace::Workspace;

pub(crate) const MONOSPACE: &str = if cfg!(windows) {
    "Consolas"
} else if cfg!(target_os = "macos") {
    "Menlo"
} else {
    "DejaVu Sans Mono"
};

fn main() {
    let sources = Source::from_args(std::env::args().skip(1));

    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx: &mut App| {
            gpui_kit::init(cx);
            let paths = birchpad_config::ConfigPaths::platform();
            let settings = birchpad_config::resolve(birchpad_config::load(&paths));
            for diagnostic in &settings.diagnostics {
                eprintln!("settings ({}): {}", diagnostic.layer, diagnostic.message);
            }
            let user_keymap = paths
                .user_config_dir()
                .and_then(|dir| std::fs::read_to_string(dir.join("keymap.toml")).ok());
            cx.set_global(AppState::new(settings, paths));
            commands::init(user_keymap.as_deref(), cx);
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();

            let bounds = Bounds::centered(None, size(px(1100.), px(760.)), cx);
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..WindowOptions::default()
            };
            let opened = gpui_kit::open_window(options, cx, |window, cx| {
                cx.new(|cx| {
                    let mut workspace = Workspace::new(window, cx);
                    workspace.new_file(window, cx);
                    for source in sources {
                        match source {
                            Source::File(path) => workspace.open_path(&path, window, cx),
                            Source::Generated(lines) => {
                                let doc = Document::from_text(Rope::from_str(&generate(lines)));
                                workspace.open_document(doc, window, cx);
                            }
                        }
                    }
                    workspace.report_pending_recoveries(window, cx);
                    workspace
                })
            });
            if let Err(error) = opened {
                eprintln!("failed to open the main window: {error:#}");
                cx.quit();
            }
            cx.activate(true);
        });
}

enum Source {
    File(PathBuf),
    Generated(usize),
}

impl Source {
    fn from_args(args: impl Iterator<Item = String>) -> Vec<Self> {
        let mut sources = Vec::new();
        let mut args = args.peekable();
        while let Some(arg) = args.next() {
            if arg == "--generate" {
                let lines = args
                    .next_if(|next| next.parse::<usize>().is_ok())
                    .and_then(|n| n.parse().ok())
                    .unwrap_or(1_000_000);
                sources.push(Self::Generated(lines));
            } else {
                sources.push(Self::File(PathBuf::from(arg)));
            }
        }
        sources
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
