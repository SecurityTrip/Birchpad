//! Birchpad desktop application.
//!
//! ```text
//! birchpad [-n<line>] [-c<column>] [-p<position>] [-multiInst] [-ro] [-nosession] [FILE]...
//! birchpad --generate [LINES]   # synthetic multilingual text, 1 000 000 lines by default
//! ```
//!
//! See `birchpad_cli::CommandLine` for every option.

// A GUI program: no console window on Windows (debug builds keep it for diagnostics).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app_state;
mod banner;
mod buffer;
mod commands;
mod disk;
#[cfg(test)]
mod disk_tests;
mod editor;
mod encoding_ui;
mod file_ops;
mod find;
mod help;
#[cfg(test)]
mod language_tests;
mod menus;
mod pane;
mod path_dialog;
mod session;
#[cfg(test)]
mod session_tests;
#[cfg(test)]
mod split_tests;
mod status_bar;
mod updates;
mod workspace;

use std::fmt::Write as _;

use birchpad_cli::{Address, CommandLine, Instance};
use futures::StreamExt as _;
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, Bounds, Entity, WindowBounds, WindowOptions, px, size,
};

use crate::app_state::AppState;
use crate::workspace::Workspace;

/// The application's id: the macOS bundle identifier, and the Linux desktop file's name.
const APP_ID: &str = "io.github.securitytrip.birchpad";

pub(crate) const MONOSPACE: &str = if cfg!(windows) {
    "Consolas"
} else if cfg!(target_os = "macos") {
    "Menlo"
} else {
    "DejaVu Sans Mono"
};

fn main() {
    birchpad_update::install::run_hooks();
    let cwd = std::env::current_dir().unwrap_or_default();
    let command_line = CommandLine::parse(std::env::args_os().skip(1), &cwd);
    for warning in &command_line.warnings {
        eprintln!("birchpad: {warning}");
    }
    let paths = birchpad_config::ConfigPaths::current();

    // A second launch hands its files to the running instance and exits.
    let server = match (&paths.user_data, command_line.multi_instance) {
        (Some(data), false) => {
            match birchpad_cli::start(&Address::for_installation(data), &command_line) {
                Instance::Secondary => return,
                Instance::Primary(server) => Some(server),
                Instance::Standalone(error) => {
                    eprintln!("birchpad: running without single-instance support: {error}");
                    None
                }
            }
        }
        _ => None,
    };

    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx: &mut App| {
            gpui_kit::init(cx);
            let settings = birchpad_config::resolve(birchpad_config::load(&paths));
            for diagnostic in &settings.diagnostics {
                eprintln!("settings ({}): {}", diagnostic.layer, diagnostic.message);
            }
            let user_keymap = paths
                .user_config_dir()
                .and_then(|dir| std::fs::read_to_string(dir.join("keymap.toml")).ok());
            let mut app_state = AppState::new(settings, paths);
            app_state.no_session = command_line.no_session;
            cx.set_global(app_state);
            updates::Updates::install(
                updates::Updates::http(),
                updates::trusted_keys(),
                birchpad_update::install::detect(),
                cx,
            );
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
                // Linux desktops match it to io.github.securitytrip.birchpad.desktop and its icon.
                app_id: Some(APP_ID.to_owned()),
                ..WindowOptions::default()
            };
            let opened = gpui_kit::open_window(options, cx, |window, cx| {
                cx.new(|cx| {
                    let mut workspace = Workspace::new(window, cx);
                    workspace.restore_last_session(window, cx);
                    workspace.open_command_line(&command_line, window, cx);
                    workspace.report_pending_recoveries(window, cx);
                    workspace.start_backups(cx);
                    workspace.start_watching(window, cx);
                    workspace
                })
            });
            let (window, workspace) = match opened {
                Ok(opened) => opened,
                Err(error) => {
                    eprintln!("failed to open the main window: {error:#}");
                    cx.quit();
                    return;
                }
            };
            // Quitting from the macOS Dock, logging out: no chance to ask, but the session (with
            // backups, the unsaved changes too) is saved.
            let quitting = workspace.downgrade();
            cx.on_app_quit(move |cx| {
                quitting
                    .update(cx, |workspace, cx| workspace.save_session_for_quit(cx))
                    .ok();
                async {}
            })
            .detach();
            updates::start_background_checks(window, workspace.downgrade(), cx);
            if let Some(server) = server {
                serve_later_launches(server, window, workspace, cx);
            }
            cx.activate(true);
        });
}

/// Opens what later launches send, in this window, and brings it to the front.
fn serve_later_launches(
    server: birchpad_cli::Server,
    window: AnyWindowHandle,
    workspace: Entity<Workspace>,
    cx: &mut App,
) {
    let (sender, mut requests) = futures::channel::mpsc::unbounded::<CommandLine>();
    server.serve(move |request| {
        let _ = sender.unbounded_send(request);
    });
    cx.spawn(async move |cx| {
        while let Some(request) = requests.next().await {
            let opened = cx.update_window(window, |_, window, cx| {
                workspace.update(cx, |workspace, cx| {
                    workspace.open_command_line(&request, window, cx);
                });
                window.activate_window();
            });
            if opened.is_err() {
                break;
            }
        }
    })
    .detach();
}

/// Multilingual text for stress-testing scrolling and shaping (`--generate N`), at most
/// 5 000 000 lines (about 350 MB), so that a typo in N cannot take the running instance down.
pub(crate) fn generate(lines: usize) -> String {
    let lines = lines.min(5_000_000);
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
