//! Birchpad desktop application.
//!
//! ```text
//! birchpad [FILE]...
//! birchpad --generate [LINES]   # synthetic multilingual text, 1 000 000 lines by default
//! ```

mod buffer;
mod commands;
mod editor;
mod line_element;
mod menus;
mod pane;
mod workspace;

use std::fmt::Write as _;
use std::path::PathBuf;

use birchpad_core::{Document, Rope};
use gpui_kit::{App, AppContext as _, Bounds, WindowBounds, WindowOptions, px, size};

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
            let config = birchpad_config::ConfigPaths::platform();
            let user_keymap = config
                .user_settings
                .as_ref()
                .and_then(|settings| settings.parent())
                .and_then(|dir| std::fs::read_to_string(dir.join("keymap.toml")).ok());
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
                    for source in sources {
                        match source.load() {
                            Ok((doc, path)) => workspace.open_document(doc, path, window, cx),
                            Err(error) => eprintln!("{error:#}"),
                        }
                    }
                    if workspace.active_view(cx).is_none() {
                        workspace.new_file(window, cx);
                    }
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

    /// Reads the source. Files must be UTF-8 until encoding support lands.
    fn load(self) -> anyhow::Result<(Document, PathBuf)> {
        match self {
            Self::Generated(lines) => Ok((
                Document::from_text(Rope::from_str(&generate(lines))),
                PathBuf::from(format!("generated-{lines}.txt")),
            )),
            Self::File(path) => {
                let bytes = std::fs::read(&path)
                    .map_err(|error| anyhow::anyhow!("cannot open {}: {error}", path.display()))?;
                let text = String::from_utf8_lossy(&bytes);
                Ok((Document::from_text(Rope::from_str(&text)), path))
            }
        }
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
