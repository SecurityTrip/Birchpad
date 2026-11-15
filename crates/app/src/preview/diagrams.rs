//! The commands of the preview of a PlantUML document (ADR 0057): the diagrams' background,
//! zoom, copying and exporting them, and following an `!include`. Running PlantUML and reading
//! what it draws is `birchpad-diagrams`', without a window.

use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, anyhow};
use birchpad_config::DiagramBackground;
use birchpad_diagrams::{
    GRAPHVIZ_DOWNLOAD, INCLUDE_PATH_VARIABLE, ZOOM_COMMANDS, export_name, find_include,
    include_path, svg_to_png,
};
use gpui_kit::{App, AppContext as _, ClipboardItem, Context, Entity, Image, ImageFormat, Window};
use toml::Value;

use super::{Preview, PreviewKind};
use crate::app_state::AppState;
use crate::commands::{CommandRegistry, Register as _};
use crate::workspace::{Workspace, report_error};

/// View > Preview > White Diagram Background: switches the background of PlantUML diagrams
/// between white and the theme's, saved in the settings for the diagrams shown later.
pub(super) fn register_commands(registry: &mut CommandRegistry) {
    registry.workspace("preview.white-background", |_, (), _, cx| {
        let next = match background(cx) {
            DiagramBackground::White => "theme",
            DiagramBackground::Theme => "white",
        };
        crate::preferences::set(
            "preview.plantuml-background",
            Some(Value::String(next.to_owned())),
            cx,
        )
    });
    registry.workspace(
        "preview.copy-diagram",
        |this, args: ExportArgs, window, cx| {
            let (svg, _) = diagram_to_export(this, args.diagram, cx)?;
            let (scale, background) = png_options(cx);
            cx.spawn_in(window, async move |_, cx| {
                let png = cx
                    .background_spawn(async move { svg_to_png(&svg, scale, background) })
                    .await;
                cx.update(|window, cx| match png {
                    Ok(png) => cx.write_to_clipboard(ClipboardItem::new_image(&Image::from_bytes(
                        ImageFormat::Png,
                        png,
                    ))),
                    Err(error) => report_error(&error, window, cx),
                })
                .ok();
            })
            .detach();
            Ok(())
        },
    );
    registry.workspace(
        "preview.export-png",
        |this, args: ExportArgs, window, cx| {
            let (svg, export) = diagram_to_export(this, args.diagram, cx)?;
            let (scale, background) = png_options(cx);
            export_file(export, "png", args.path, window, cx, move |path| {
                let png = svg_to_png(&svg, scale, background)?;
                std::fs::write(path, png).with_context(|| path.display().to_string())
            });
            Ok(())
        },
    );
    registry.workspace(
        "preview.export-svg",
        |this, args: ExportArgs, window, cx| {
            let (svg, export) = diagram_to_export(this, args.diagram, cx)?;
            export_file(export, "svg", args.path, window, cx, move |path| {
                std::fs::write(path, &svg).with_context(|| path.display().to_string())
            });
            Ok(())
        },
    );
    registry.workspace(
        "preview.open-include",
        |this, args: IncludeArgs, window, cx| {
            let settings = &AppState::global(cx).settings.preview;
            let folder = args.document.as_deref().and_then(Path::parent);
            let include = include_path(
                folder,
                &settings.plantuml_include_path,
                std::env::var_os(INCLUDE_PATH_VARIABLE).as_deref(),
            );
            let folders: Vec<PathBuf> = include
                .as_deref()
                .map(|path| std::env::split_paths(path).collect())
                .unwrap_or_default();
            // Written with variables or calls: as the preprocessor reads the document.
            let worked_out = || {
                let document = args.document.as_deref()?;
                let text = open_text(this, document, cx)?;
                super::walk_includes(&text, folder, &folders)
                    .links
                    .into_iter()
                    .find(|link| link.from.is_none() && link.written == args.target)
                    .map(|link| link.path)
            };
            let path = find_include(&args.target, &folders)
                .or_else(worked_out)
                .ok_or_else(|| anyhow!("{}", t!("preview.no-include", file = args.target)))?;
            this.open_path(&path, window, cx);
            Ok(())
        },
    );
    registry.workspace("preview.get-graphviz", |_, (), _, cx| {
        crate::file_commands::shell(
            crate::file_commands::Shell::Browse(GRAPHVIZ_DOWNLOAD.to_owned()),
            cx,
        )
    });
    registry.workspace("preview.choose-layout", |_, (), window, cx| {
        crate::preferences::open_at(cx.entity().downgrade(), Some("Preview"), window, cx);
        Ok(())
    });
    registry.workspace("preview.redraw", |this, (), _, cx| {
        let preview = active_diagrams(this, cx)?;
        preview.update(cx, |preview, cx| preview.redraw(cx));
        Ok(())
    });
    for (id, change) in ZOOM_COMMANDS {
        registry.workspace(id, move |this, (), _, cx| {
            let preview = active_diagrams(this, cx)?;
            preview.update(cx, |preview, cx| {
                preview.set_zoom(change(preview.zoom()), cx)
            });
            Ok(())
        });
    }
}

/// The preview of the active document, if it shows PlantUML diagrams.
pub fn active_diagrams(workspace: &Workspace, cx: &mut App) -> Result<Entity<Preview>> {
    let view = workspace
        .active_view(cx)
        .ok_or_else(|| anyhow!("{}", t!("preview.no-diagrams")))?;
    workspace
        .active_pane()
        .clone()
        .update(cx, |pane, cx| pane.preview(&view, cx))
        .filter(|preview| preview.read(cx).kind == PreviewKind::PlantUml)
        .ok_or_else(|| anyhow!("{}", t!("preview.no-diagrams")))
}

/// Copy Diagram and Export Diagram as PNG or SVG: which diagram of the active document (the
/// first by default), and the file to write, or none to ask for one.
#[derive(Debug, Default, serde::Deserialize)]
struct ExportArgs {
    #[serde(default)]
    diagram: usize,
    #[serde(default)]
    path: Option<PathBuf>,
}

/// Where an exported diagram goes by default: the document's folder and name.
struct Export {
    folder: PathBuf,
    stem: Option<String>,
    diagram: usize,
}

/// The SVG of diagram `index` of the active document's preview, and where it is exported.
fn diagram_to_export(
    workspace: &Workspace,
    index: usize,
    cx: &mut Context<Workspace>,
) -> Result<(Vec<u8>, Export)> {
    let preview = active_diagrams(workspace, cx)?;
    let preview = preview.read(cx);
    let svg = preview
        .diagram_svg(index)
        .ok_or_else(|| anyhow!("{}", t!("preview.no-diagram-to-export")))?;
    let path = preview.path(cx);
    let export = Export {
        folder: path
            .as_deref()
            .and_then(Path::parent)
            .map_or_else(|| workspace.default_directory(cx), Path::to_path_buf),
        stem: path
            .as_deref()
            .and_then(Path::file_stem)
            .map(|stem| stem.to_string_lossy().into_owned()),
        diagram: index,
    };
    Ok((svg, export))
}

/// Writes a diagram with `write` to `path`, or to the file the user chooses.
fn export_file(
    export: Export,
    extension: &'static str,
    path: Option<PathBuf>,
    window: &mut Window,
    cx: &mut Context<Workspace>,
    write: impl FnOnce(&Path) -> Result<()> + Send + 'static,
) {
    let start = export.folder;
    let name = export_name(export.stem.as_deref(), export.diagram, extension);
    cx.spawn_in(window, async move |_, cx| {
        let path = match path {
            Some(path) => path,
            None => match crate::path_dialog::ask_save(start, name, cx).await {
                Some(path) => path,
                None => return,
            },
        };
        let written = cx.background_spawn(async move { write(&path) }).await;
        if let Err(error) = written {
            let error = anyhow!(
                "{}",
                t!("preview.cannot-export", error = format!("{error:#}"))
            );
            cx.update(|window, cx| report_error(&error, window, cx))
                .ok();
        }
    })
    .detach();
}

/// The scale (`preview.plantuml-png-scale`, 1 to 8) and the background of PNG diagrams: white,
/// or transparent with the theme's.
fn png_options(cx: &App) -> (f32, Option<[u8; 4]>) {
    let settings = &AppState::global(cx).settings.preview;
    let scale = settings.plantuml_png_scale.clamp(1, 8) as f32;
    let background = match settings.plantuml_background {
        DiagramBackground::White => Some([255, 255, 255, 255]),
        DiagramBackground::Theme => None,
    };
    (scale, background)
}

/// What the preview draws PlantUML diagrams on (`preview.plantuml-background`).
pub fn background(cx: &App) -> DiagramBackground {
    AppState::global(cx).settings.preview.plantuml_background
}

/// Ctrl+click on an `!include` line: the file it names, and the document it is in.
#[derive(Debug, Default, serde::Deserialize)]
struct IncludeArgs {
    target: String,
    #[serde(default)]
    document: Option<PathBuf>,
}

/// The text of the document at `path`: its open buffer's, else the file's.
fn open_text(workspace: &Workspace, path: &Path, cx: &App) -> Option<String> {
    let open = workspace.all_views(cx).into_iter().find_map(|view| {
        let buffer = view.read(cx).buffer.read(cx);
        (buffer.path() == Some(path)).then(|| buffer.doc().text().to_string())
    });
    open.or_else(|| {
        std::fs::read(path)
            .ok()
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
    })
}
