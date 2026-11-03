//! The preview of a PlantUML document (ADR 0057): PlantUML draws the text as SVG in the
//! background once it has rested, a run the text has moved past is stopped, and its mistakes
//! are listed by the line of the document they are on, each a link to that line.

use std::io::{Read as _, Write as _};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use std::path::PathBuf;
use std::sync::LazyLock;

use anyhow::{Context as _, Result, anyhow, bail};
use birchpad_config::{DiagramBackground, DiagramLayout};
use gpui_kit::{App, AppContext as _, ClipboardItem, Context, Entity, Image, ImageFormat, Window};
use resvg::{tiny_skia, usvg};
use toml::Value;

use super::{Preview, PreviewKind};
use crate::app_state::AppState;
use crate::commands::{CommandRegistry, Register as _};
use crate::workspace::{Workspace, report_error};

/// What PlantUML writes after each diagram, to tell the diagrams of a file apart.
pub const DIAGRAM_END: &str = "<!--birchpad-diagram-end-->";

/// How long the text rests before PlantUML draws it again: it starts a Java program each time.
pub const PLANTUML_DELAY: Duration = Duration::from_millis(400);

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

/// How a zoom command changes the zoom.
pub type ZoomChange = fn(Zoom) -> Zoom;

/// View > Preview's zoom commands, and how each changes the zoom.
pub const ZOOM_COMMANDS: [(&str, ZoomChange); 5] = [
    ("preview.zoom-in", Zoom::zoom_in),
    ("preview.zoom-out", Zoom::zoom_out),
    ("preview.actual-size", |_| Zoom::Percent(100)),
    ("preview.fit", |_| Zoom::Fit),
    ("preview.fit-page", |_| Zoom::FitPage),
];

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

/// The name an exported diagram is offered: the document's (`flow.png`), with the diagram's
/// number after the first (`flow-2.png`); `diagram` for an untitled document.
pub fn export_name(stem: Option<&str>, diagram: usize, extension: &str) -> String {
    let stem = stem.filter(|stem| !stem.is_empty()).unwrap_or("diagram");
    match diagram {
        0 => format!("{stem}.{extension}"),
        index => format!("{stem}-{}.{extension}", index + 1),
    }
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

/// The longest side a PNG diagram has, in pixels; a larger scale is lowered to it.
pub const MAX_PNG_SIDE: f32 = 16_384.;

/// The system's fonts, read once, with the generic families set to fonts that are there.
fn fonts() -> Arc<usvg::fontdb::Database> {
    static FONTS: LazyLock<Arc<usvg::fontdb::Database>> = LazyLock::new(|| {
        let mut fonts = usvg::fontdb::Database::new();
        fonts.load_system_fonts();
        // The first of `names` the system has.
        fn first(fonts: &usvg::fontdb::Database, names: &[&str]) -> Option<String> {
            names
                .iter()
                .find(|name| {
                    fonts
                        .faces()
                        .any(|face| face.families.iter().any(|(family, _)| family == *name))
                })
                .map(|name| (*name).to_owned())
        }
        let sans = [
            "Arial",
            "Helvetica",
            "Liberation Sans",
            "DejaVu Sans",
            "Noto Sans",
        ];
        if let Some(name) = first(&fonts, &sans) {
            fonts.set_sans_serif_family(name);
        }
        let serif = [
            "Times New Roman",
            "Times",
            "Liberation Serif",
            "DejaVu Serif",
            "Noto Serif",
        ];
        if let Some(name) = first(&fonts, &serif) {
            fonts.set_serif_family(name);
        }
        let mono = [
            "Consolas",
            "Courier New",
            "Menlo",
            "Liberation Mono",
            "DejaVu Sans Mono",
        ];
        if let Some(name) = first(&fonts, &mono) {
            fonts.set_monospace_family(name);
        }
        Arc::new(fonts)
    });
    FONTS.clone()
}

/// `svg` drawn at `scale` times its size (lowered so that no side passes [`MAX_PNG_SIDE`]), on
/// `background` (RGBA) or transparent: its width, height and RGBA pixels, not premultiplied.
pub fn svg_to_pixels(
    svg: &[u8],
    scale: f32,
    background: Option<[u8; 4]>,
) -> Result<(u32, u32, Vec<u8>)> {
    let pixmap = render_svg(svg, scale, background)?;
    let pixels = pixmap
        .pixels()
        .iter()
        .flat_map(|pixel| {
            let color = pixel.demultiply();
            [color.red(), color.green(), color.blue(), color.alpha()]
        })
        .collect();
    Ok((pixmap.width(), pixmap.height(), pixels))
}

/// `svg` as a PNG, drawn as [`svg_to_pixels`] draws it.
pub fn svg_to_png(svg: &[u8], scale: f32, background: Option<[u8; 4]>) -> Result<Vec<u8>> {
    let pixmap = render_svg(svg, scale, background)?;
    pixmap.encode_png().map_err(|error| anyhow!("{error}"))
}

fn render_svg(svg: &[u8], scale: f32, background: Option<[u8; 4]>) -> Result<tiny_skia::Pixmap> {
    // Not a number is no scale either.
    if scale.is_nan() || scale <= 0. {
        bail!("{}", t!("preview.not-a-picture"));
    }
    let options = usvg::Options {
        fontdb: fonts(),
        ..usvg::Options::default()
    };
    let tree = usvg::Tree::from_data(svg, &options)
        .map_err(|error| anyhow!("{}: {error}", t!("preview.not-a-picture")))?;
    let size = tree.size();
    let longest = size.width().max(size.height());
    let scale = scale.min(MAX_PNG_SIDE / longest);
    let width = (size.width() * scale).ceil().max(1.) as u32;
    let height = (size.height() * scale).ceil().max(1.) as u32;
    let mut pixmap = tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| anyhow!("{}", t!("preview.not-a-picture")))?;
    if let Some([red, green, blue, alpha]) = background {
        pixmap.fill(tiny_skia::Color::from_rgba8(red, green, blue, alpha));
    }
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    Ok(pixmap)
}

/// What the preview draws PlantUML diagrams on (`preview.plantuml-background`).
pub fn background(cx: &App) -> DiagramBackground {
    AppState::global(cx).settings.preview.plantuml_background
}

/// The variable PlantUML finds Graphviz's `dot` by.
pub const GRAPHVIZ_DOT_VARIABLE: &str = "GRAPHVIZ_DOT";

/// What lays a diagram out, once the setting is resolved (ADR 0059).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Layout {
    /// Smetana, in `plantuml.jar`.
    Smetana,
    /// Graphviz: the `dot` found, or none for PlantUML to look for itself.
    Graphviz(Option<PathBuf>),
}

impl Layout {
    /// The layout `setting` asks for, with `dot` the Graphviz found, if any: automatic is
    /// Graphviz when there is one, smetana otherwise.
    pub fn resolve(setting: DiagramLayout, dot: Option<PathBuf>) -> Self {
        match (setting, dot) {
            (DiagramLayout::Smetana, _) | (DiagramLayout::Automatic, None) => Layout::Smetana,
            (DiagramLayout::Automatic | DiagramLayout::Graphviz, dot) => Layout::Graphviz(dot),
        }
    }

    /// Its name, as the preview shows it.
    pub fn name(&self) -> &'static str {
        match self {
            Layout::Smetana => "smetana",
            Layout::Graphviz(_) => "Graphviz",
        }
    }

    /// The variables PlantUML gets for it: where `dot` is.
    pub fn env(&self) -> Vec<(String, std::ffi::OsString)> {
        match self {
            Layout::Graphviz(Some(dot)) => vec![(
                GRAPHVIZ_DOT_VARIABLE.to_owned(),
                dot.clone().into_os_string(),
            )],
            _ => Vec::new(),
        }
    }
}

/// Graphviz's `dot`: the file `graphviz_dot` (the `GRAPHVIZ_DOT` variable) names, else `dot` in
/// one of the folders of `path` (as in `PATH`), else in one of `more` (where Graphviz installs
/// itself); `None` when there is none.
pub fn find_dot(
    graphviz_dot: Option<&std::ffi::OsStr>,
    path: Option<&std::ffi::OsStr>,
    more: &[PathBuf],
) -> Option<PathBuf> {
    if let Some(dot) = graphviz_dot.map(PathBuf::from).filter(|dot| dot.is_file()) {
        return Some(dot);
    }
    let name = if cfg!(windows) { "dot.exe" } else { "dot" };
    path.map(std::env::split_paths)
        .into_iter()
        .flatten()
        .chain(more.iter().cloned())
        .filter(|folder| !folder.as_os_str().is_empty())
        .map(|folder| folder.join(name))
        .find(|dot| dot.is_file())
}

#[cfg(feature = "test-support")]
thread_local! {
    /// The `dot` tests have the system find instead of looking for it: `Some(None)` for none.
    pub static DOT: std::cell::RefCell<Option<Option<PathBuf>>> = std::cell::RefCell::default();
}

/// Where Graphviz is downloaded from, for the link under a crash of smetana.
pub const GRAPHVIZ_DOWNLOAD: &str = "https://graphviz.org/download/";

/// How often a preview whose diagram smetana could not lay out looks for Graphviz again.
pub const DOT_POLL: Duration = Duration::from_secs(5);

/// Whether a preview showing `mistakes`, drawn with the layout named `layout` while the
/// setting is `setting`, waits for Graphviz to be installed, to draw again with it: a crash of
/// smetana, with the automatic layout.
pub fn waits_for_graphviz(mistakes: &[Mistake], layout: &str, setting: DiagramLayout) -> bool {
    setting == DiagramLayout::Automatic
        && layout == Layout::Smetana.name()
        && mistakes.iter().any(|mistake| mistake.details.is_some())
}

/// Graphviz's `dot` on this system, looked for as [`find_dot`] does: on Windows also in
/// `Program Files\Graphviz\bin`, where its installer puts it.
pub fn system_dot() -> Option<PathBuf> {
    #[cfg(feature = "test-support")]
    if let Some(dot) = DOT.with(|dot| dot.borrow().clone()) {
        return dot;
    }
    let more: Vec<PathBuf> = if cfg!(windows) {
        ["ProgramFiles", "ProgramW6432"]
            .iter()
            .filter_map(std::env::var_os)
            .map(|folder| PathBuf::from(folder).join("Graphviz").join("bin"))
            .collect()
    } else {
        Vec::new()
    };
    find_dot(
        std::env::var_os(GRAPHVIZ_DOT_VARIABLE).as_deref(),
        std::env::var_os("PATH").as_deref(),
        &more,
    )
}

/// The program and arguments that draw PlantUML's diagrams as SVG from the standard input, for
/// the `preview.plantuml` setting: a `.jar` runs with `java -jar`. With the setting as it comes
/// (`plantuml`) and PlantUML installed by Birchpad in `installed`, that one runs, with its own
/// Java. Smetana is asked for when it is the `layout`.
pub fn plantuml_command(
    setting: &str,
    installed: Option<&Path>,
    layout: &Layout,
) -> (String, Vec<String>) {
    let setting = match setting.trim() {
        "" => "plantuml",
        setting => setting,
    };
    let options = [
        "-tsvg",
        "-pipe",
        "-charset",
        "UTF-8",
        "-pipedelimitor",
        DIAGRAM_END,
    ]
    .map(str::to_owned)
    .into_iter()
    .chain((*layout == Layout::Smetana).then(|| "-Playout=smetana".to_owned()))
    .collect::<Vec<_>>();
    if let Some(folder) = installed.filter(|_| setting == "plantuml") {
        let java =
            folder
                .join("jre")
                .join("bin")
                .join(if cfg!(windows) { "java.exe" } else { "java" });
        let mut args = vec![
            "-Djava.awt.headless=true".to_owned(),
            "-jar".to_owned(),
            folder.join("plantuml.jar").display().to_string(),
        ];
        args.extend(options);
        return (java.display().to_string(), args);
    }
    let is_jar = Path::new(setting)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("jar"));
    if is_jar {
        let mut args = vec!["-jar".to_owned(), setting.to_owned()];
        args.extend(options);
        ("java".to_owned(), args)
    } else {
        (setting.to_owned(), options)
    }
}

/// The `preview.plantuml` setting with a relative path in it taken from `base`, Birchpad's
/// working folder, since PlantUML runs in the document's folder: a `.jar` (`plantuml.jar`,
/// `tools/plantuml.jar`) or a program by a path (`bin/plantuml`). A program's name alone
/// (`plantuml`) is looked for on `PATH`, and stays.
pub fn setting_path(setting: &str, base: &Path) -> String {
    let setting = setting.trim();
    let path = Path::new(setting);
    let is_jar = path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("jar"));
    let has_folder = setting.contains('/') || setting.contains(std::path::MAIN_SEPARATOR);
    if setting.is_empty() || path.is_absolute() || !(is_jar || has_folder) {
        return setting.to_owned();
    }
    base.join(path).display().to_string()
}

/// The environment variable PlantUML takes its include path from, as it takes the
/// `plantuml.include.path` property: the folders `!include` looks in besides the one it runs in.
pub const INCLUDE_PATH_VARIABLE: &str = "PLANTUML_INCLUDE_PATH";

/// The include path PlantUML gets for a document in `folder`: that folder first, so that a file
/// included from another folder finds what it includes by a path relative to the document (as
/// IDE plugins and Confluence's macro find it); then the folders of `setting`
/// (`preview.plantuml-include-path`, separated as in `PATH`, a relative one taken from the
/// document's folder); then those of `inherited`, the variable Birchpad was started with.
/// A folder that cannot be in such a list (its name has the separator in it) is left out, and
/// so is a relative one with no document's folder to take it from; `None` when none is left.
pub fn include_path(
    folder: Option<&Path>,
    setting: &str,
    inherited: Option<&std::ffi::OsStr>,
) -> Option<std::ffi::OsString> {
    let mut folders: Vec<PathBuf> = Vec::new();
    // Windows would put such a folder in quotes, which PlantUML does not take off.
    let separator = if cfg!(windows) { ';' } else { ':' };
    let mut add = |path: PathBuf| {
        let joinable =
            std::env::join_paths([&path]).is_ok() && !path.to_string_lossy().contains(separator);
        if joinable && !path.as_os_str().is_empty() && !folders.contains(&path) {
            folders.push(path);
        }
    };
    if let Some(folder) = folder {
        add(folder.to_path_buf());
    }
    for path in std::env::split_paths(setting) {
        let trimmed = PathBuf::from(path.to_string_lossy().trim());
        if trimmed.is_absolute() {
            add(trimmed);
        } else if let Some(folder) = folder.filter(|_| !trimmed.as_os_str().is_empty()) {
            add(folder.join(trimmed));
        }
    }
    for path in inherited.map(std::env::split_paths).into_iter().flatten() {
        add(path);
    }
    if folders.is_empty() {
        return None;
    }
    std::env::join_paths(folders).ok()
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

/// The file `target` (as an `!include` names it) is: itself when absolute, else the first of
/// `folders` (the include path, the document's folder first) that has it.
pub fn find_include(target: &str, folders: &[PathBuf]) -> Option<PathBuf> {
    let target = match target.strip_prefix('"') {
        Some(quoted) => quoted.split('"').next().unwrap_or_default(),
        None => target.split('!').next().unwrap_or_default().trim(),
    };
    if target.is_empty() {
        return None;
    }
    let target = Path::new(target);
    if target.is_absolute() {
        return Some(target.to_path_buf()).filter(|file| file.is_file());
    }
    folders
        .iter()
        .map(|folder| normalized(&folder.join(target)))
        .find(|file| file.is_file())
}

/// `path` without its `.` parts, and with each `..` taking off the part before it, so that
/// a file reached by two paths (`a/../b.puml`, `b.puml`) is found once.
pub fn normalized(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                let parent = out
                    .components()
                    .next_back()
                    .is_some_and(|last| matches!(last, Component::Normal(_)));
                if parent {
                    out.pop();
                } else {
                    out.push(part);
                }
            }
            part => out.push(part),
        }
    }
    out
}

/// Whether a change to the file at `path` draws the diagrams of the document at `document`
/// again: a file it includes (in `files`), or a PlantUML file (one may be included once it is
/// there); not the document itself, whose text the preview follows already.
pub fn redraws_for(path: &Path, document: Option<&Path>, files: &[PathBuf]) -> bool {
    let same = |a: &Path, b: &Path| {
        a == b
            || std::fs::canonicalize(a)
                .is_ok_and(|a| std::fs::canonicalize(b).is_ok_and(|b| a == b))
    };
    if document.is_some_and(|document| same(path, document)) {
        return false;
    }
    let plantuml = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            super::PLANTUML_EXTENSIONS
                .iter()
                .any(|known| known.eq_ignore_ascii_case(extension))
        });
    plantuml || files.iter().any(|file| same(path, file))
}

/// The folders to watch for a document in `folder` that includes `files`: its own and those
/// of the files, each once.
pub fn watched_folders(folder: Option<&Path>, files: &[PathBuf]) -> Vec<PathBuf> {
    let mut folders: Vec<PathBuf> = Vec::new();
    for candidate in folder.map(Path::to_path_buf).into_iter().chain(
        files
            .iter()
            .filter_map(|file| file.parent().map(Path::to_path_buf)),
    ) {
        if !folders.contains(&candidate) {
            folders.push(candidate);
        }
    }
    folders
}

/// The SVG images in PlantUML's output, one per diagram.
pub fn split_diagrams(output: &[u8]) -> Vec<Vec<u8>> {
    String::from_utf8_lossy(output)
        .split(DIAGRAM_END)
        .map(str::trim)
        .filter(|part| part.contains("<svg"))
        .map(|part| part.as_bytes().to_vec())
        .collect()
}

/// How large the diagrams of a PlantUML preview are shown.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Zoom {
    /// A diagram wider than the preview shrunk to its width; others at their size.
    #[default]
    Fit,
    /// A diagram larger than the preview, either way, shrunk to be seen whole; others at their
    /// size.
    FitPage,
    /// This percentage of their size, the preview scrolling both ways.
    Percent(u32),
}

/// The steps Zoom In and Zoom Out go by, in percent.
pub const ZOOM_STEPS: [u32; 15] = [
    25, 33, 50, 67, 75, 90, 100, 110, 125, 150, 175, 200, 250, 300, 400,
];

impl Zoom {
    /// The next step up; from Fit, the one above 100%; the largest stays.
    pub fn zoom_in(self) -> Self {
        let from = self.percent().unwrap_or(100);
        let last = ZOOM_STEPS[ZOOM_STEPS.len() - 1];
        Zoom::Percent(
            ZOOM_STEPS
                .into_iter()
                .find(|&step| step > from)
                .unwrap_or(last),
        )
    }

    /// The next step down; from Fit, the one below 100%; the smallest stays.
    pub fn zoom_out(self) -> Self {
        let from = self.percent().unwrap_or(100);
        Zoom::Percent(
            ZOOM_STEPS
                .into_iter()
                .rev()
                .find(|&step| step < from)
                .unwrap_or(ZOOM_STEPS[0]),
        )
    }

    pub fn percent(self) -> Option<u32> {
        match self {
            Zoom::Fit | Zoom::FitPage => None,
            Zoom::Percent(percent) => Some(percent),
        }
    }

    /// What the preview's toolbar shows: `Fit` or `125%`.
    pub fn label(self) -> String {
        match self {
            Zoom::Fit => t!("preview.zoom-fit"),
            Zoom::FitPage => t!("preview.zoom-fit-page"),
            Zoom::Percent(percent) => format!("{percent}%"),
        }
    }
}

/// How many times its size a diagram of `size` is shown at Fit in a preview whose diagrams
/// have `room` across (its width less its margins): as wide as the room when it is wider, at
/// its size otherwise; 1 when either is no size.
pub fn fit_width_scale(size: (f32, f32), room: f32) -> f32 {
    if size.0 <= 0. || room <= 0. {
        return 1.;
    }
    (room / size.0).min(1.)
}

/// How many times its size a diagram of `size` is shown at Fit Page in a preview whose diagrams
/// have `room` (their area less its margins): small enough to be seen whole, never enlarged;
/// 1 when either is no size.
pub fn fit_page_scale(size: (f32, f32), room: (f32, f32)) -> f32 {
    let (width, height) = size;
    let (room_width, room_height) = room;
    if width <= 0. || height <= 0. || room_width <= 0. || room_height <= 0. {
        return 1.;
    }
    (room_width / width).min(room_height / height).min(1.)
}

/// Where the diagrams are scrolled to while they are dragged as by a hand: the offset they had
/// when the drag started, moved with the mouse (from `from` to `to`), no further than their
/// edges (offsets are from `-most` to 0).
pub fn dragged_offset(
    start: (f32, f32),
    from: (f32, f32),
    to: (f32, f32),
    most: (f32, f32),
) -> (f32, f32) {
    let x = (start.0 + to.0 - from.0).clamp(-most.0.max(0.), 0.);
    let y = (start.1 + to.1 - from.1).clamp(-most.1.max(0.), 0.);
    (x, y)
}

/// The most pixels a zoomed diagram is drawn with (before GPUI's own doubling); a larger one is
/// enlarged as a picture rather than drawn again.
pub const MAX_ZOOMED_PIXELS: f32 = 4_000_000.;

/// How many times its size a diagram of `size` is drawn for `zoom` (a factor): the zoom, unless
/// that passes [`MAX_ZOOMED_PIXELS`]; never below 1, its own size.
pub fn raster_factor(size: (f32, f32), zoom: f32) -> f32 {
    let pixels = size.0 * size.1;
    let most = if pixels > 0. {
        (MAX_ZOOMED_PIXELS / pixels).sqrt()
    } else {
        1.
    };
    zoom.min(most).max(1.)
}

/// The root `<svg ...>` tag of `svg`: where it starts and ends (past its `>`).
fn root_tag(svg: &str) -> Option<(usize, usize)> {
    let start = svg.find("<svg")?;
    let end = start + svg[start..].find('>')? + 1;
    Some((start, end))
}

/// The value of attribute `name` in `tag`.
fn attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let pattern = format!(" {name}=\"");
    let start = tag.find(&pattern)? + pattern.len();
    let end = start + tag[start..].find('"')?;
    Some(&tag[start..end])
}

/// A length in pixels: `533`, `533px`, `533.5`. Other units (`%`, `em`) are no size.
fn pixels(length: &str) -> Option<f32> {
    let length = length.trim();
    let number = length.strip_suffix("px").unwrap_or(length);
    number.parse::<f32>().ok().filter(|value| *value > 0.)
}

/// The size of an SVG image in pixels: its `width` and `height`, else its `viewBox`.
pub fn svg_size(svg: &[u8]) -> Option<(f32, f32)> {
    let svg = std::str::from_utf8(svg).ok()?;
    let (start, end) = root_tag(svg)?;
    let tag = &svg[start..end];
    let sized = attribute(tag, "width")
        .and_then(pixels)
        .zip(attribute(tag, "height").and_then(pixels));
    sized.or_else(|| {
        let view_box: Vec<f32> = attribute(tag, "viewBox")?
            .split([' ', ','])
            .filter(|part| !part.is_empty())
            .map(|part| part.parse().ok())
            .collect::<Option<_>>()?;
        match view_box[..] {
            [_, _, width, height] if width > 0. && height > 0. => Some((width, height)),
            _ => None,
        }
    })
}

/// `svg` set to `size` in pixels: its root's `width` and `height` (and those of its `style`,
/// which PlantUML writes too), its `viewBox` kept, so that it is drawn at that size.
pub fn sized_svg(svg: &[u8], size: (f32, f32)) -> Vec<u8> {
    let Ok(text) = std::str::from_utf8(svg) else {
        return svg.to_vec();
    };
    let Some((start, end)) = root_tag(text) else {
        return svg.to_vec();
    };
    let mut tag = text[start..end].to_owned();
    for (name, value) in [("width", size.0), ("height", size.1)] {
        let value = format!("{value}px");
        match attribute(&tag, name) {
            Some(old) => {
                let old = format!(" {name}=\"{old}\"");
                tag = tag.replacen(&old, &format!(" {name}=\"{value}\""), 1);
            }
            None => tag.insert_str(4, &format!(" {name}=\"{value}\"")),
        }
    }
    if let Some(style) = attribute(&tag, "style") {
        let resized: Vec<String> = style
            .split(';')
            .filter(|part| !part.trim().is_empty())
            .map(|part| match part.split_once(':') {
                Some((name, _)) if name.trim() == "width" => format!("width:{}px", size.0),
                Some((name, _)) if name.trim() == "height" => format!("height:{}px", size.1),
                _ => part.to_owned(),
            })
            .collect();
        let old = format!(" style=\"{style}\"");
        tag = tag.replacen(&old, &format!(" style=\"{};\"", resized.join(";")), 1);
    }
    let mut sized = String::with_capacity(text.len() + 16);
    sized.push_str(&text[..start]);
    sized.push_str(&tag);
    sized.push_str(&text[end..]);
    sized.into_bytes()
}

/// A mistake PlantUML found: what it says, and the line of the document it is on (from 0),
/// when that is known. A mistake in an included file is on the document's `!include` line that
/// leads to it, and says which file it is in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mistake {
    pub line: Option<usize>,
    pub message: String,
    pub included: Option<Included>,
    /// For a crash of PlantUML (a Java exception rather than a mistake of the text): its stack
    /// trace, folded under the message, which is the exception's first line.
    pub details: Option<String>,
}

/// Where in an included file a mistake is: the file as PlantUML names it (empty when its name
/// is none of the files the diagram includes, and none of them was found to hold it), and the
/// line (from 1).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Included {
    pub file: String,
    pub line: usize,
    /// The line of text PlantUML's picture shows under the mistake: what it read there.
    pub shown: Option<String>,
    /// The file, found among those the diagram includes ([`place_in_includes`]): a click on the
    /// mistake opens it at the line.
    pub path: Option<PathBuf>,
}

/// The texts of an SVG picture, in order, as they read.
fn picture_texts(svg: &str) -> Vec<String> {
    let mut texts = Vec::new();
    let mut rest = svg;
    while let Some(at) = rest.find("<text") {
        rest = &rest[at..];
        let Some(open) = rest.find('>') else {
            break;
        };
        let body = &rest[open + 1..];
        let Some(close) = body.find("</text>") else {
            break;
        };
        texts.push(unescape_xml(&body[..close]).replace('\u{a0}', " "));
        rest = &body[close..];
    }
    texts
}

/// The line PlantUML's picture of a mistake shows just above its `message`: the line it read
/// where the mistake is.
pub fn pictured_line(svg: &str, message: &str) -> Option<String> {
    let texts = picture_texts(svg);
    let at = texts
        .iter()
        .rposition(|text| text.trim() == message.trim())?;
    texts[..at]
        .iter()
        .rev()
        .map(|text| text.trim())
        .find(|text| !text.is_empty())
        .map(str::to_owned)
}

/// `path` relative to `folder`, with `..` for each folder above it
/// (`../../parts/billing/containers.puml`), both taken without their `.` and `..` parts;
/// `None` when they have no root in common (another drive), or `path` is `folder` itself.
pub fn relative_to(path: &Path, folder: &Path) -> Option<PathBuf> {
    use std::path::Component;
    let path = normalized(path);
    let folder = normalized(folder);
    let mut parts = path.components().peekable();
    let mut above = folder.components().peekable();
    let mut shared = 0;
    while let (Some(a), Some(b)) = (parts.peek(), above.peek()) {
        if a != b {
            break;
        }
        parts.next();
        above.next();
        shared += 1;
    }
    let rooted = |part: &Component| matches!(part, Component::Prefix(_) | Component::RootDir);
    if shared == 0 && (path.components().next().is_some_and(|part| rooted(&part))) {
        return None;
    }
    let mut relative = PathBuf::new();
    for part in above {
        if matches!(part, Component::ParentDir) {
            return None;
        }
        relative.push("..");
    }
    relative.extend(parts);
    (!relative.as_os_str().is_empty()).then_some(relative)
}

/// Places the mistakes in included files that PlantUML did not trace to the document's lines
/// (it names the file with a name of its own, `desc2`) among `included`, the files the
/// diagram includes: the file of that name, else the one whose line at the mistake's line is
/// the line the picture shows. It is named relative to the document's `folder`, and kept for a
/// click to open; when none is found, the mistake says only that it is in an included file.
pub fn place_in_includes(mistakes: &mut [Mistake], folder: Option<&Path>, included: &[PathBuf]) {
    let mut lines: Vec<Option<Vec<String>>> = vec![None; included.len()];
    for mistake in mistakes.iter_mut() {
        let Some(place) = mistake.included.as_mut() else {
            continue;
        };
        if place.path.is_some() {
            continue;
        }
        // By its name: the end of an included file's path.
        let name: PathBuf = Path::new(&place.file)
            .components()
            .skip_while(|part| {
                matches!(
                    part,
                    std::path::Component::ParentDir | std::path::Component::CurDir
                )
            })
            .collect();
        let named = (!name.as_os_str().is_empty())
            .then(|| included.iter().position(|file| file.ends_with(&name)))
            .flatten();
        // By what the picture shows at the line, when the name is no file of the diagram's.
        let shown = place.shown.as_deref().map(str::trim);
        let found = named.or_else(|| {
            let shown = shown.filter(|shown| !shown.is_empty())?;
            (0..included.len()).find(|&index| {
                let text = lines[index].get_or_insert_with(|| {
                    std::fs::read(&included[index])
                        .map(|bytes| {
                            String::from_utf8_lossy(&bytes)
                                .lines()
                                .map(str::to_owned)
                                .collect()
                        })
                        .unwrap_or_default()
                });
                place
                    .line
                    .checked_sub(1)
                    .and_then(|at| text.get(at))
                    .is_some_and(|line| line.trim() == shown)
            })
        });
        match found {
            Some(index) => {
                let file = &included[index];
                if named.is_none() || mistake.line.is_none() {
                    place.file = folder
                        .and_then(|folder| relative_to(file, folder))
                        .unwrap_or_else(|| file.clone())
                        .display()
                        .to_string();
                }
                place.path = Some(file.clone());
            }
            None if mistake.line.is_none() => place.file.clear(),
            None => {}
        }
    }
}

/// What PlantUML calls the document it reads from its input in a mistake's trace.
const DOCUMENT_SOURCE: &str = "string";

/// The trace in PlantUML's picture of a mistake: the `[From name (line N) ]` lines, where the
/// mistake is first, then each `!include` that led to it, the document (`string`) last. Lines
/// count from 1, from the diagram's `@start` line in the document.
pub fn mistake_trace(svg: &str) -> Vec<(String, usize)> {
    const FROM: &str = "[From ";
    const LINE: &str = " (line ";
    let mut trace = Vec::new();
    let mut rest = svg;
    while let Some(at) = rest.find(FROM) {
        rest = &rest[at + FROM.len()..];
        // The name ends at the last ` (line ` of the text it is in.
        let text = &rest[..rest.find('<').unwrap_or(rest.len())];
        let Some(name_end) = text.rfind(LINE) else {
            continue;
        };
        let number = &text[name_end + LINE.len()..];
        let digits = number
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(number.len());
        let (Ok(line), Some(')')) = (
            number[..digits].parse::<usize>(),
            number[digits..].chars().next(),
        ) else {
            continue;
        };
        let name = unescape_xml(&text[..name_end]);
        if !name.is_empty() {
            trace.push((name, line));
        }
    }
    trace
}

fn unescape_xml(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

/// The lines (from 0) where the diagrams of `text` start: `@startuml`, `@startmindmap` and the
/// other `@start` lines.
pub fn diagram_starts(text: &str) -> Vec<usize> {
    text.lines()
        .enumerate()
        .filter(|(_, line)| {
            let line = line.trim_start();
            line.get(..6)
                .is_some_and(|start| start.eq_ignore_ascii_case("@start"))
        })
        .map(|(number, _)| number)
        .collect()
}

fn escape_xml(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;")
}

/// The mistakes in PlantUML's error output: for each diagram with mistakes, `ERROR`, the line
/// in the diagram (from its `@start` line, 0) and what is wrong, in the order of the diagrams.
/// Each is placed in the document `text` by the diagram whose picture (in `diagrams`) shows its
/// message, and by the trace in that picture ([`mistake_trace`]) when it is in an included file.
/// Output in another form is one mistake, without a line.
pub fn mistakes(stderr: &str, text: &str, diagrams: &[Vec<u8>]) -> Vec<Mistake> {
    let lines: Vec<&str> = stderr
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    let starts = diagram_starts(text);
    let mut found = Vec::new();
    // The diagram after the one the last mistake was in.
    let mut next = 0;
    let mut at = 0;
    while at < lines.len() {
        let (true, Some(Ok(line)), Some(message)) = (
            lines[at] == "ERROR",
            lines.get(at + 1).map(|line| line.parse::<usize>()),
            lines.get(at + 2),
        ) else {
            break;
        };
        let shown = escape_xml(message);
        let pictured = (next..diagrams.len())
            .find(|&index| String::from_utf8_lossy(&diagrams[index]).contains(&shown));
        let diagram = pictured.unwrap_or(next);
        next = diagram + 1;
        let start = starts.get(diagram).copied();
        let trace = pictured
            .map(|index| mistake_trace(&String::from_utf8_lossy(&diagrams[index])))
            .unwrap_or_default();
        // The line in the error output is in the file the mistake is in: in an included file,
        // the mistake is placed on the last of the document's lines that led to it.
        let (line, included) = match trace.first() {
            Some((file, at)) if file != DOCUMENT_SOURCE => {
                let document = trace
                    .iter()
                    .rev()
                    .find(|(name, _)| name == DOCUMENT_SOURCE)
                    .and_then(|(_, at)| start?.checked_add(at.checked_sub(1)?));
                let included = Included {
                    file: file.clone(),
                    line: *at,
                    shown: pictured.and_then(|index| {
                        pictured_line(&String::from_utf8_lossy(&diagrams[index]), message)
                    }),
                    path: None,
                };
                (document, Some(included))
            }
            _ => (start.and_then(|start| start.checked_add(line)), None),
        };
        found.push(Mistake {
            line,
            message: (*message).to_owned(),
            included,
            details: None,
        });
        at += 3;
    }
    if found.is_empty() && !lines.is_empty() {
        found.push(crash(&lines).unwrap_or_else(|| Mistake {
            line: None,
            message: lines.join("\n"),
            included: None,
            details: None,
        }));
    }
    found
}

/// A crash of PlantUML in its error output `lines` (trimmed, none empty): a Java exception and
/// its stack trace (`at` lines). The exception's line is the message, without Java's
/// `Exception in thread "main"`; the whole output is its details.
fn crash(lines: &[&str]) -> Option<Mistake> {
    let is_frame = |line: &&str| line.starts_with("at ") && line.ends_with(')');
    let first_frame = lines.iter().position(is_frame)?;
    let exception = lines[..first_frame]
        .iter()
        .rev()
        .find(|line| line.contains("Exception") || line.contains("Error"))
        .or_else(|| lines[..first_frame].last())?;
    let exception = match exception.strip_prefix("Exception in thread ") {
        // `"main" java.lang.NullPointerException`: the thread's name in quotes, then it.
        Some(rest) => rest
            .strip_prefix('"')
            .and_then(|rest| rest.split_once("\" "))
            .map_or(rest, |(_, exception)| exception),
        None => exception,
    };
    Some(Mistake {
        line: None,
        message: exception.to_owned(),
        included: None,
        details: Some(lines.join("\n")),
    })
}

/// Where line `line` (from 0) of `text` starts, in bytes; the end of the text past its last
/// line. `\n`, `\r\n` and `\r` end lines.
pub fn line_offset(text: &str, line: usize) -> usize {
    let bytes = text.as_bytes();
    let mut current = 0;
    let mut at = 0;
    while current < line {
        match bytes[at..].iter().position(|&b| b == b'\n' || b == b'\r') {
            Some(end) => {
                at += end;
                at += if bytes[at..].starts_with(b"\r\n") {
                    2
                } else {
                    1
                };
                current += 1;
            }
            None => return text.len(),
        }
    }
    at
}

/// Why PlantUML drew nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotDrawn {
    /// There is no PlantUML to run: Birchpad can install it.
    Missing(String),
    Failed(String),
    /// The run was stopped: the text changed since.
    Stopped,
}

/// What PlantUML drew: the diagrams (a diagram with mistakes is a picture of them), and the
/// mistakes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Drawn {
    pub diagrams: Vec<Vec<u8>>,
    pub mistakes: Vec<Mistake>,
}

/// Stops a PlantUML run: the process is ended, and a run not started yet does not start.
#[derive(Clone, Default)]
pub struct Stop(Arc<Mutex<(bool, Option<Child>)>>);

impl Stop {
    pub fn stop(&self) {
        let mut state = self.0.lock().unwrap_or_else(|error| error.into_inner());
        state.0 = true;
        if let Some(mut child) = state.1.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    pub fn is_stopped(&self) -> bool {
        self.0.lock().unwrap_or_else(|error| error.into_inner()).0
    }

    /// Keeps `child` to be stopped; false, and the child ended, if the run was stopped already.
    pub(super) fn keep(&self, mut child: Child) -> bool {
        let mut state = self.0.lock().unwrap_or_else(|error| error.into_inner());
        if state.0 {
            let _ = child.kill();
            let _ = child.wait();
            return false;
        }
        state.1 = Some(child);
        true
    }

    /// The child back once its output is read, unless it was stopped meanwhile.
    pub(super) fn take(&self) -> Option<Child> {
        self.0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .1
            .take()
    }
}

/// Stops its run when it is dropped: the drawing a newer one replaces.
pub struct StopOnDrop(pub Stop);

impl Drop for StopOnDrop {
    fn drop(&mut self) {
        self.0.stop();
    }
}

/// Runs `program` with `args` on `text`, as PlantUML, in `folder` (the document's, so that
/// `!include` and images by relative path are found next to it), with `include_path` (see
/// [`include_path`]) for the files included files include and the variables of `env` (the
/// layout's): the diagrams it wrote and its mistakes, or why there are none. `stop` ends the
/// run early.
pub fn run_plantuml(
    program: &str,
    args: &[String],
    folder: Option<&Path>,
    include_path: Option<&std::ffi::OsStr>,
    env: &[(String, std::ffi::OsString)],
    text: &str,
    stop: &Stop,
) -> Result<Drawn, NotDrawn> {
    if stop.is_stopped() {
        return Err(NotDrawn::Stopped);
    }
    let mut command = Command::new(program);
    // A folder that is gone leaves PlantUML in Birchpad's, rather than not running it.
    if let Some(folder) = folder.filter(|folder| folder.is_dir()) {
        command.current_dir(folder);
    }
    if let Some(include_path) = include_path {
        command.env(INCLUDE_PATH_VARIABLE, include_path);
    }
    command.envs(env.iter().map(|(name, value)| (name, value)));
    command
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        // CREATE_NO_WINDOW: no console flashes up for Java.
        command.creation_flags(0x0800_0000);
    }
    let mut child = command.spawn().map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            NotDrawn::Missing(t!("preview.plantuml-not-found", program = program))
        } else {
            NotDrawn::Failed(t!("preview.plantuml-failed", error = error))
        }
    })?;
    let mut stdin = child.stdin.take().expect("piped");
    let mut stdout = child.stdout.take().expect("piped");
    let mut stderr = child.stderr.take().expect("piped");
    if !stop.keep(child) {
        return Err(NotDrawn::Stopped);
    }
    let input = text.to_owned();
    // Written and read in other threads, so that a large diagram cannot block on a full pipe.
    let writer = std::thread::spawn(move || stdin.write_all(input.as_bytes()));
    let errors = std::thread::spawn(move || {
        let mut errors = Vec::new();
        let _ = stderr.read_to_end(&mut errors);
        errors
    });
    let mut output = Vec::new();
    let read = stdout.read_to_end(&mut output);
    let errors = errors.join().unwrap_or_default();
    let _ = writer.join();
    let Some(mut child) = stop.take() else {
        return Err(NotDrawn::Stopped);
    };
    let succeeded = child.wait().is_ok_and(|status| status.success());
    read.map_err(|error| NotDrawn::Failed(t!("preview.plantuml-failed", error = error)))?;
    // A diagram with mistakes is drawn as a picture of them, with a status that is not 0.
    let diagrams = split_diagrams(&output);
    let errors = String::from_utf8_lossy(&errors);
    if diagrams.is_empty() {
        let message = errors.trim().to_owned();
        return Err(NotDrawn::Failed(if message.is_empty() {
            t!("preview.plantuml-no-diagram")
        } else {
            message
        }));
    }
    // What a run that succeeded writes there is no mistake: Java's warnings.
    let mistakes = if succeeded {
        Vec::new()
    } else {
        mistakes(&errors, text, &diagrams)
    };
    Ok(Drawn { diagrams, mistakes })
}
