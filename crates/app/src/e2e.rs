//! End-to-end scripts (the `e2e` feature, for `crates/app/tests/e2e.rs`): the built application,
//! in a real window, follows the steps of the TOML file named by `BIRCHPAD_E2E_SCRIPT` once it
//! has started, and writes what its window shows to a report.
//!
//! ```toml
//! [[step]]
//! keys = "down end"              # keystrokes, as in keymap.toml; mind each platform's keys
//! [[step]]
//! text = "hello\nworld"          # typed, a key per character
//! [[step]]
//! command = "file.save"          # a command of the registry, with optional args
//! [[step]]
//! wait = 2000                    # milliseconds, for what happens outside: another launch
//! [[step]]
//! files = { pattern = "x", folder = ".", filters = "*.txt" }       # Find in Files
//! [[step]]
//! files = { pattern = "x", folder = ".", replace = "y" }           # Replace in Files
//! [[step]]
//! report = "after-save.json"     # the tabs, their texts, the caret, Find in Files' results
//! ```
//!
//! Replace in Files does not ask first here: its question is the system's dialog.
//!
//! A step that fails is recorded in the next report. Quitting is a step like any other
//! (`command = "file.exit"`); a script that does not quit leaves the application running.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context as _, Result, bail};
use birchpad_commands::{Invocation, Keystroke as KeymapKeystroke};
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, AsyncApp, Entity, Keystroke, Modifiers, Window,
};
use serde::{Deserialize, Serialize};

use crate::workspace::Workspace;

pub const SCRIPT_VARIABLE: &str = "BIRCHPAD_E2E_SCRIPT";

/// How long a step waits for the application to settle: a frame drawn, a file read.
const SETTLE: Duration = Duration::from_millis(150);

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Script {
    step: Vec<Step>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Step {
    keys: Option<String>,
    text: Option<String>,
    command: Option<String>,
    args: Option<serde_json::Value>,
    wait: Option<u64>,
    files: Option<FilesStep>,
    report: Option<PathBuf>,
}

/// Find in Files, or Replace in Files with `replace`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FilesStep {
    pattern: String,
    folder: PathBuf,
    #[serde(default)]
    filters: String,
    replace: Option<String>,
}

/// What the window shows, for the test to check.
#[derive(Debug, Serialize)]
struct Report {
    tabs: Vec<Tab>,
    /// The index in `tabs` of the active one.
    active: Option<usize>,
    /// The steps that failed since the last report.
    errors: Vec<String>,
    /// The rows of the search results panel, and the find panel's message.
    results: Vec<String>,
    status: Option<String>,
}

#[derive(Debug, Serialize)]
struct Tab {
    name: String,
    path: Option<PathBuf>,
    text: String,
    modified: bool,
    /// Why the document cannot be edited, if it cannot.
    read_only: Option<String>,
    /// The primary caret, as a byte offset.
    caret: usize,
}

/// Runs the script named by [`SCRIPT_VARIABLE`], if any, in `window`.
pub fn run_script(window: AnyWindowHandle, workspace: Entity<Workspace>, cx: &mut App) {
    let Some(path) = std::env::var_os(SCRIPT_VARIABLE).map(PathBuf::from) else {
        return;
    };
    cx.spawn(async move |cx| {
        if let Err(error) = run(&path, window, &workspace, cx).await {
            eprintln!("e2e: {}: {error:#}", path.display());
        }
    })
    .detach();
}

async fn run(
    path: &Path,
    window: AnyWindowHandle,
    workspace: &Entity<Workspace>,
    cx: &mut AsyncApp,
) -> Result<()> {
    let text = std::fs::read_to_string(path).context("reading the script")?;
    let script: Script = toml::from_str(&text).context("parsing the script")?;
    let dir = path.parent().unwrap_or(Path::new("."));
    let mut errors = Vec::new();
    for (index, step) in script.step.into_iter().enumerate() {
        settle(window, workspace, cx).await;
        if let Some(wait) = step.wait {
            cx.background_executor()
                .timer(Duration::from_millis(wait))
                .await;
            continue;
        }
        if let Some(report_path) = &step.report {
            let errors = std::mem::take(&mut errors);
            let report = cx.update_window(window, |_, _, cx| report(workspace, errors, cx))?;
            let json = serde_json::to_string_pretty(&report)?;
            std::fs::write(dir.join(report_path), json).context("writing the report")?;
            continue;
        }
        let done = cx.update_window(window, |_, window, cx| {
            perform(&step, dir, workspace, window, cx)
        })?;
        if let Err(error) = done {
            errors.push(format!("step {}: {error:#}", index + 1));
        }
    }
    Ok(())
}

/// Waits for a frame or two, then until no document is still being read or written and no Find
/// or Replace in Files runs (at most ten seconds): a save runs in the background, and a slow disk
/// can take longer than a frame.
async fn settle(window: AnyWindowHandle, workspace: &Entity<Workspace>, cx: &mut AsyncApp) {
    cx.background_executor().timer(SETTLE).await;
    for _ in 0..200 {
        let busy = cx.update_window(window, |_, _, cx| {
            let workspace = workspace.read(cx);
            workspace.files_running(cx)
                || workspace.all_views(cx).iter().any(|view| {
                    let buffer = view.read(cx).buffer.read(cx);
                    buffer.loading_progress().is_some() || buffer.is_saving()
                })
        });
        if !matches!(busy, Ok(true)) {
            return;
        }
        cx.background_executor()
            .timer(Duration::from_millis(50))
            .await;
    }
}

fn perform(
    step: &Step,
    dir: &Path,
    workspace: &Entity<Workspace>,
    window: &mut Window,
    cx: &mut App,
) -> Result<()> {
    if let Some(keys) = &step.keys {
        let platform = birchpad_commands::Platform::current();
        for keystroke in KeymapKeystroke::parse_sequence(keys, platform)? {
            let keystroke = Keystroke::parse(&keystroke.to_string())
                .with_context(|| format!("GPUI does not read {keystroke}"))?;
            window.dispatch_keystroke(keystroke, cx);
        }
    }
    if let Some(text) = &step.text {
        for character in text.chars() {
            let keystroke = match character {
                '\n' => Keystroke::parse("enter")?,
                '\t' => Keystroke::parse("tab")?,
                character => Keystroke {
                    modifiers: Modifiers::default(),
                    key: character.to_lowercase().to_string(),
                    key_char: Some(character.to_string()),
                },
            };
            if !window.dispatch_keystroke(keystroke, cx) {
                bail!("{character:?} was not typed: nothing has the focus");
            }
        }
    }
    if let Some(command) = &step.command {
        let invocation = match &step.args {
            Some(args) => Invocation::with_args(command, args.clone()),
            None => Invocation::new(command),
        };
        workspace.update(cx, |workspace, cx| {
            workspace.dispatch(&invocation, window, cx)
        })?;
    }
    if let Some(files) = &step.files {
        let folder = dir.join(&files.folder);
        workspace.update(cx, |workspace, cx| {
            workspace.find_bar.update(cx, |bar, cx| {
                bar.show(
                    crate::find::FindTab::FindInFiles,
                    Some(files.pattern.clone()),
                    None,
                    window,
                    cx,
                );
                bar.folder.follow_current_document = false;
                bar.set_directory(&folder, window, cx);
                bar.set_filters(&files.filters, window, cx);
                if let Some(replace) = &files.replace {
                    bar.replace_input_for_tests(replace, window, cx);
                }
            });
            match files.replace {
                Some(_) => workspace.replace_in_files_without_asking(window, cx),
                None => {
                    let bar = workspace.find_bar.clone();
                    let event = crate::find::FindBarEvent::FindInFiles;
                    workspace.on_find_bar_event(&bar, &event, window, cx);
                }
            }
        });
    }
    Ok(())
}

fn report(workspace: &Entity<Workspace>, errors: Vec<String>, cx: &App) -> Report {
    let workspace = workspace.read(cx);
    let active = workspace.active_view(cx);
    let views = workspace.all_views(cx);
    let tabs = views
        .iter()
        .map(|view| {
            let view = view.read(cx);
            let buffer = view.buffer.read(cx);
            Tab {
                name: buffer.display_name(),
                path: buffer.path().map(Path::to_owned),
                text: buffer.doc().text().to_string(),
                modified: buffer.is_modified(),
                read_only: buffer.read_only().map(|why| format!("{why:?}")),
                caret: view.selection.primary().head,
            }
        })
        .collect();
    Report {
        tabs,
        active: active.and_then(|active| views.iter().position(|view| *view == active)),
        errors,
        results: workspace.search_results.read(cx).rows_text(),
        status: workspace.find_bar.read(cx).status_text(),
    }
}
