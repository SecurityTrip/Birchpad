//! Birchpad desktop application: its window, panes, panels, dialogs and menus. The program
//! (`src/main.rs`) calls [`run`]; the tests drive the parts of the window through this library.
//!
//! ```text
//! birchpad [-n<line>] [-c<column>] [-p<position>] [-multiInst] [-ro] [-nosession] [FILE]...
//! birchpad --generate [LINES]   # synthetic multilingual text, 1 000 000 lines by default
//! ```
//!
//! See `birchpad_cli::CommandLine` for every option.

#[macro_use]
extern crate birchpad_i18n;

pub mod app_state;
pub mod commands;
pub mod compare;
pub mod context_menu;
pub mod date_time;
pub mod default_app;
pub mod disk;
#[cfg(feature = "e2e")]
pub mod e2e;
pub mod edit_extras;
pub mod editor_hooks;
pub mod encoding_ui;
pub mod extensions;
pub mod file_commands;
pub mod file_ops;
pub mod find;
pub mod find_in_files;
pub mod finder;
pub mod help;
pub mod hex_editor;
pub mod hex_view;
pub mod i18n;
pub mod incremental;
pub mod json_xml;
pub mod macros;
pub mod menus;
pub mod navigation;
pub mod online;
pub mod palette;
pub mod pane;
pub mod panels;
pub mod path_dialog;
pub mod plantuml;
pub mod preferences;
pub mod preview;
pub mod print;
pub mod quick_pick;
pub mod run_menu;
pub mod search_extras;
pub mod search_results;
pub mod session;
pub mod shortcut_mapper;
pub mod spell_check;
pub mod status_bar;
pub mod style_configurator;
pub mod themes;
pub mod toolbar;
pub mod tools;
pub mod updates;
pub mod user_languages;
pub mod view_extras;
pub mod window_menu;
pub mod window_modes;
pub mod workspace;

pub use birchpad_editor::{self as editor, buffer, theme};

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

pub use birchpad_editor::MONOSPACE;

/// Installed for the user: "Edit with Birchpad" in the Explorer's menu of files.
fn installed() {
    if let Ok(exe) = std::env::current_exe()
        && let Err(error) = birchpad_cli::explorer::add(&exe)
    {
        eprintln!("cannot add Edit with Birchpad to the Explorer: {error}");
    }
}

/// Updated: "Edit with Birchpad" in the Explorer's menu too, unless the user took it away.
fn updated() {
    if let Ok(exe) = std::env::current_exe()
        && let Err(error) = birchpad_cli::explorer::add_unless_removed(&exe)
    {
        eprintln!("cannot add Edit with Birchpad to the Explorer: {error}");
    }
}

/// About to be uninstalled: the Explorer's menu loses it, and the text file types Birchpad
/// registered for go.
fn uninstalling() {
    if let Err(error) = birchpad_cli::explorer::remove_for_uninstall() {
        eprintln!("cannot take Edit with Birchpad from the Explorer: {error}");
    }
    if let Err(error) = birchpad_cli::associations::unregister() {
        eprintln!("cannot take Birchpad's file types away: {error}");
    }
}

/// Runs Birchpad with the process's command line, until its last window closes.
pub fn run() {
    birchpad_update::install::run_hooks(installed, updated, uninstalling);
    let cwd = std::env::current_dir().unwrap_or_default();
    let command_line = CommandLine::parse(std::env::args_os().skip(1), &cwd);
    for warning in &command_line.warnings {
        eprintln!("birchpad: {warning}");
    }
    let paths = match &command_line.settings_dir {
        // `-settingsDir`: the settings and data in that folder.
        Some(dir) => birchpad_config::ConfigPaths::current().in_folder(dir),
        None => birchpad_config::ConfigPaths::current(),
    }
    .following_cloud();
    let settings = birchpad_config::resolve(birchpad_config::load(&paths));

    // A second launch hands its files to the running instance and exits, unless it asks for
    // an instance of its own or Preferences > Multi-Instance gives it one.
    let own_instance = command_line.multi_instance
        || settings
            .settings
            .instances
            .new_instance(command_line.open_session);
    let server = match (&paths.user_data, own_instance) {
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
            for diagnostic in &settings.diagnostics {
                eprintln!("settings ({}): {}", diagnostic.layer, diagnostic.message);
            }
            let user_keymap = paths
                .user_config_dir()
                .and_then(|dir| std::fs::read_to_string(dir.join("keymap.toml")).ok());
            let mut app_state = AppState::new(settings, paths);
            // Printing files at once (`-quickPrint`) leaves the session as it was.
            app_state.no_session = command_line.no_session || command_line.quick_print;
            app_state.full_read_only = command_line.full_read_only;
            app_state.saving_forbidden = command_line.saving_forbidden;
            cx.set_global(app_state);
            themes::init(cx);
            i18n::apply(cx);
            // `-L<code>`: the interface's language this time.
            if let Some(code) = &command_line.interface_language {
                i18n::set_language(i18n::for_this_run(code));
            }
            macros::init(cx);
            run_menu::init(cx);
            user_languages::init(cx);
            let proxy = AppState::global(cx).settings.updates.proxy.clone();
            updates::Updates::install(
                updates::Updates::http(&proxy),
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

            let mut bounds = Bounds::centered(None, size(px(1100.), px(760.)), cx);
            // `-x` and `-y`: where the window opens.
            if let Some(x) = command_line.x {
                bounds.origin.x = px(x as f32);
            }
            if let Some(y) = command_line.y {
                bounds.origin.y = px(y as f32);
            }
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                // Linux desktops match it to io.github.securitytrip.birchpad.desktop and its icon.
                app_id: Some(APP_ID.to_owned()),
                ..WindowOptions::default()
            };
            let opened = gpui_kit::open_window(options, cx, |window, cx| {
                cx.new(|cx| {
                    let mut workspace = Workspace::new(window, cx);
                    workspace.title_add = command_line.title_add.clone();
                    workspace.modes.no_tab_bar = command_line.no_tab_bar;
                    workspace.restore_last_session(window, cx);
                    workspace.restore_panels(window, cx);
                    workspace.show_tabs(cx);
                    if !command_line.no_plugins {
                        workspace.load_plugins(window, cx);
                    }
                    workspace.open_command_line(&command_line, window, cx);
                    workspace.open_startup_document(window, cx);
                    workspace.apply_full_read_only(cx);
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
            window
                .update(cx, |_, window, cx| default_app::offer(window, cx))
                .ok();
            window
                .update(cx, |_, window, cx| {
                    if command_line.always_on_top {
                        let invocation = birchpad_commands::Invocation::new("view.always-on-top");
                        let result = workspace.update(cx, |workspace, cx| {
                            workspace.dispatch(&invocation, window, cx)
                        });
                        if let Err(error) = result {
                            workspace::report_error(&error, window, cx);
                        }
                    }
                    // `-systemtray`: GPUI has no tray icon; the window starts minimized.
                    if command_line.minimized {
                        window.minimize_window();
                    }
                })
                .ok();
            // `-quickPrint`: Birchpad quits once the files are printed.
            if command_line.quick_print {
                let printing = workspace.update(cx, |workspace, _| {
                    std::mem::take(&mut workspace.quick_prints)
                });
                cx.spawn(async move |cx| {
                    for print in printing {
                        print.await;
                    }
                    cx.update(|cx| cx.quit());
                })
                .detach();
            }
            #[cfg(feature = "e2e")]
            e2e::run_script(window, workspace.clone(), cx);
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
                    for print in std::mem::take(&mut workspace.quick_prints) {
                        print.detach();
                    }
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
pub fn generate(lines: usize) -> String {
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
