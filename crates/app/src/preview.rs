//! Previews beside the text, as IntelliJ IDEA shows them: a Markdown or PlantUML document is
//! shown as its text and its rendered preview side by side, and the buttons at the top right of
//! the text (or View > Preview) show the text alone, both, or the preview alone.
//!
//! Each view of a document has its own preview, kept by its pane, which shows the text as it
//! is being edited. The mode last chosen is remembered in `state.toml` (`preview`) and applies
//! to documents opened after it.
//!
//! Links in the Markdown preview open in the browser, or in Birchpad when they name a file;
//! images are read from files next to the document, never from the network. PlantUML diagrams
//! are drawn by PlantUML itself: the one `preview.plantuml` names, or the one Birchpad installs
//! when the user asks (`plantuml.rs`).

mod diagrams;
mod includes;
pub mod server;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::bail;
use birchpad_commands::Invocation;
use birchpad_config::{DiagramBackground, PreviewMode};
use birchpad_core::RevisionId;
use gpui_kit::base::text::{TextView, TextViewState};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::scroll::{Scrollbar, ScrollbarAxis, ScrollbarMode};
use gpui_kit::component::{IconName, Selectable as _, Sizable};
use gpui_kit::{
    AnyElement, App, AppContext as _, ClickEvent, Context, CursorStyle, Entity, Image, ImageFormat,
    ImageSource, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ObjectFit, Pixels,
    Point, Resource, ScrollHandle, ScrollWheelEvent, SharedString, SharedUri, Subscription, Task,
    WeakEntity, Window, div, img, point, prelude::*, px,
};

use crate::app_state::AppState;
use crate::buffer::Buffer;
use crate::commands::{CommandRegistry, Register as _};
use crate::editor::EditorView;
use crate::editor::widgets::{BarColors, Line, bar, error_text};
use crate::theme::{paint, ui};
use crate::workspace::Workspace;

#[cfg(feature = "test-support")]
pub use diagrams::DOT;
pub use diagrams::{
    DIAGRAM_END, DOT_POLL, Drawn, GRAPHVIZ_DOT_VARIABLE, GRAPHVIZ_DOWNLOAD, INCLUDE_PATH_VARIABLE,
    Included, Layout, MAX_PNG_SIDE, MAX_ZOOMED_PIXELS, Mistake, NotDrawn, PLANTUML_DELAY,
    PLANTUML_MAIN, Stop, StopOnDrop, ZOOM_COMMANDS, ZOOM_STEPS, Zoom, ZoomChange, active_diagrams,
    background, diagram_starts, dragged_offset, draws_again_with_elk, export_name, find_dot,
    find_include, fit_page_scale, fit_width_scale, folded_pictures, include_path, line_offset,
    mistake_trace, mistakes, normalized, pictured_line, place_in_includes, plantuml_command,
    raster_factor, redraws_for, relative_to, run_plantuml, setting_path, sized_svg, split_diagrams,
    svg_size, svg_to_pixels, svg_to_png, system_dot, waits_for_graphviz, watched_folders,
};
pub use includes::{IncludeLink, Includes, MOST_INCLUDED_FILES, Preprocessor, walk_includes};

/// What a document's preview shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewKind {
    Markdown,
    /// Diagrams drawn by PlantUML, installed apart from Birchpad.
    PlantUml,
}

/// PlantUML's files: `.puml` and the other extensions its tools know.
pub(crate) const PLANTUML_EXTENSIONS: [&str; 5] = ["puml", "plantuml", "pu", "iuml", "wsd"];

/// The preview `buffer` has: from its language, or for PlantUML from its file's extension too
/// (a `.puml` shown as another language still has its diagrams).
pub fn kind_of(buffer: &Buffer) -> Option<PreviewKind> {
    kind_for(buffer.language().map(|language| language.id), buffer.path())
}

/// The preview of a document in `language` (by id) kept at `path`.
pub fn kind_for(language: Option<&str>, path: Option<&Path>) -> Option<PreviewKind> {
    match language {
        Some("markdown") => return Some(PreviewKind::Markdown),
        Some("plantuml") => return Some(PreviewKind::PlantUml),
        _ => {}
    }
    let extension = path?.extension()?.to_str()?;
    PLANTUML_EXTENSIONS
        .iter()
        .any(|known| known.eq_ignore_ascii_case(extension))
        .then_some(PreviewKind::PlantUml)
}

/// How far Ctrl with the mouse wheel scrolls for a step of zoom, in pixels: a notch of a wheel.
const WHEEL_STEP: f32 = 40.;

/// The commands of View > Preview and the mode each one shows.
pub const COMMANDS: [(&str, PreviewMode); 3] = [
    ("view.preview-editor", PreviewMode::Editor),
    ("view.preview-split", PreviewMode::Split),
    ("view.preview-only", PreviewMode::Preview),
];

pub fn register_commands(registry: &mut CommandRegistry) {
    diagrams::register_commands(registry);
    for (id, mode) in COMMANDS {
        registry.workspace(id, move |this, (), _, cx| {
            let pane = this.active_pane().clone();
            if !pane.update(cx, |pane, cx| pane.set_preview_mode(mode, cx)) {
                bail!("{}", t!("preview.no-preview"));
            }
            this.refresh_menus(cx);
            Ok(())
        });
    }
}

/// Whether View > Preview's `invocation` is checked, for the active view's `mode`.
pub fn is_checked(command: &str, mode: Option<PreviewMode>) -> Option<bool> {
    COMMANDS
        .iter()
        .find(|(id, _)| *id == command)
        .map(|(_, shows)| Some(*shows) == mode)
}

/// What a link in the preview of a document in `folder` leads to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkTarget {
    /// A web page, a mail address: handed to the system.
    Url(String),
    /// A file, opened in Birchpad.
    File(PathBuf),
    /// A place in the same document, or nothing that can be followed.
    None,
}

/// Whether `link` starts with a URL scheme (`https:`, `mailto:`), but not a drive (`C:`).
fn scheme(link: &str) -> Option<&str> {
    let (scheme, _) = link.split_once(':')?;
    let valid = scheme.len() > 1
        && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    valid.then_some(scheme)
}

/// A path from a link or an image source: `file:` URLs and paths relative to `folder` (the
/// document's), without a `#fragment` or `?query`, with `%20` and its kin decoded.
pub fn local_path(link: &str, folder: Option<&Path>) -> Option<PathBuf> {
    let link = link.strip_prefix("file://").unwrap_or(link);
    // `file:///C:/notes` leaves `/C:/notes`.
    let link = match link.as_bytes() {
        [b'/', drive, b':', ..] if drive.is_ascii_alphabetic() => &link[1..],
        _ => link,
    };
    let link = link.split(['#', '?']).next().unwrap_or_default();
    if link.is_empty() {
        return None;
    }
    let path = PathBuf::from(percent_decode(link));
    if path.is_absolute() {
        Some(path)
    } else {
        folder.map(|folder| folder.join(path))
    }
}

/// `%XX` escapes decoded; text that is not UTF-8 afterwards, or a broken escape, stays as is.
pub fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let hex = bytes
            .get(at + 1..at + 3)
            .and_then(|hex| std::str::from_utf8(hex).ok())
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        match (bytes[at], hex) {
            (b'%', Some(byte)) => {
                decoded.push(byte);
                at += 3;
            }
            (byte, _) => {
                decoded.push(byte);
                at += 1;
            }
        }
    }
    String::from_utf8(decoded).unwrap_or_else(|_| text.to_owned())
}

/// Where a click on `link` in the preview of a document in `folder` goes.
pub fn link_target(link: &str, folder: Option<&Path>) -> LinkTarget {
    let link = link.trim();
    match scheme(link) {
        Some(scheme) if scheme.eq_ignore_ascii_case("file") => {
            local_path(link, folder).map_or(LinkTarget::None, LinkTarget::File)
        }
        Some(_) => LinkTarget::Url(link.to_owned()),
        None if link.starts_with('#') => LinkTarget::None,
        None => local_path(link, folder).map_or(LinkTarget::None, LinkTarget::File),
    }
}

/// Where an image of the preview of a document in `folder` is read from: a file, or the data
/// of a `data:` URL. Images on the web are not fetched (Birchpad connects to the network only
/// for updates): `None`.
pub fn image_path(source: &str, folder: Option<&Path>) -> Option<PathBuf> {
    match scheme(source) {
        Some(scheme) if scheme.eq_ignore_ascii_case("file") => local_path(source, folder),
        Some(_) => None,
        None => local_path(source, folder),
    }
}

fn image_source(uri: &SharedUri, folder: Option<&Path>) -> ImageSource {
    if uri.starts_with("data:") {
        return ImageSource::Resource(Resource::Uri(uri.clone()));
    }
    match image_path(uri, folder) {
        Some(path) => ImageSource::Resource(Resource::Path(path.into())),
        // An asset that does not exist: the image is left out.
        None => ImageSource::Resource(Resource::Embedded("preview/blocked-image".into())),
    }
}

/// What a PlantUML preview shows.
#[derive(Default)]
struct Diagrams {
    images: Vec<Diagram>,
    /// How large the diagrams are shown.
    zoom: Zoom,
    /// Ctrl with the mouse wheel: the scrolling not yet turned into a zoom step.
    wheel: f32,
    /// A drag of the diagrams with the mouse, as by a hand: where the mouse was and where the
    /// diagrams were scrolled to when it started.
    drag: Option<(Point<Pixels>, Point<Pixels>)>,
    /// Where the diagrams are scrolled to, both ways.
    scroll: ScrollHandle,
    /// The mistakes PlantUML found in the text the diagrams show.
    mistakes: Vec<Mistake>,
    /// Why PlantUML drew nothing; the diagrams it drew last stay.
    error: Option<String>,
    /// There was no PlantUML to run.
    missing: bool,
    /// PlantUML drawing the text; dropping it stops the run ([`StopOnDrop`]).
    drawing: Option<Task<()>>,
    /// The installations of PlantUML when it last drew ([`crate::plantuml::PlantUmlInstall`]).
    installs: u64,
    /// The name of the layout the diagrams were last drawn with.
    layout: &'static str,
    /// Smetana crashed, and ELK drew the diagrams instead (ADR 0063).
    fell_back: bool,
    /// The details of a crash of PlantUML are shown.
    details: bool,
    /// After a crash of smetana with the automatic layout: looking for Graphviz now and then,
    /// to draw again with it once it is installed.
    graphviz: Option<Task<()>>,
    /// The diagrams that are pictures of mistakes placed above them, folded ([`folded_pictures`]).
    folded: Vec<usize>,
    /// Those of them shown all the same, by a click.
    unfolded: Vec<usize>,
    /// The files the document includes, found when it was last drawn.
    included: Vec<PathBuf>,
    /// Watches the folders of the document and of the files it includes.
    watch: Option<IncludeWatch>,
}

/// Watching the files a diagram includes, so that it is drawn again when one changes.
struct IncludeWatch {
    watcher: notify::RecommendedWatcher,
    folders: Vec<PathBuf>,
    _listening: Task<()>,
}

/// One diagram PlantUML drew.
struct Diagram {
    /// Its SVG, drawn at its own size.
    image: Arc<Image>,
    size: Option<(f32, f32)>,
    /// Drawn again larger for a zoom above 100%, so that it stays sharp: the factor and the
    /// image.
    zoomed: Option<(f32, Arc<Image>)>,
}

impl Diagram {
    fn new(svg: Vec<u8>, zoom: Zoom) -> Self {
        let mut diagram = Self {
            size: svg_size(&svg),
            image: Arc::new(Image::from_bytes(ImageFormat::Svg, svg)),
            zoomed: None,
        };
        diagram.prepare(zoom);
        diagram
    }

    /// Draws the SVG again for `zoom`, if it needs to be larger than it is.
    fn prepare(&mut self, zoom: Zoom) {
        let (Some(percent), Some(size)) = (zoom.percent(), self.size) else {
            self.zoomed = None;
            return;
        };
        let factor = raster_factor(size, percent as f32 / 100.);
        if factor <= 1. {
            self.zoomed = None;
        } else if self
            .zoomed
            .as_ref()
            .is_none_or(|(drawn, _)| *drawn != factor)
        {
            let svg = sized_svg(self.image.bytes(), (size.0 * factor, size.1 * factor));
            self.zoomed = Some((factor, Arc::new(Image::from_bytes(ImageFormat::Svg, svg))));
        }
    }

    fn shown(&self) -> Arc<Image> {
        self.zoomed
            .as_ref()
            .map_or_else(|| self.image.clone(), |(_, image)| image.clone())
    }
}

enum Content {
    Markdown(Entity<TextViewState>),
    PlantUml(Diagrams),
}

/// The preview of one view: the rendered text, kept up to date as the text changes.
pub struct Preview {
    view: WeakEntity<EditorView>,
    workspace: WeakEntity<Workspace>,
    pub kind: PreviewKind,
    pub mode: PreviewMode,
    content: Content,
    /// The state of the text the preview shows.
    shown: Option<(RevisionId, usize)>,
    _subscription: Subscription,
}

impl Preview {
    pub fn new(
        view: &Entity<EditorView>,
        kind: PreviewKind,
        workspace: WeakEntity<Workspace>,
        cx: &mut Context<Self>,
    ) -> Self {
        let buffer = view.read(cx).buffer.clone();
        let subscription = cx.observe(&buffer, |this, buffer, cx| this.update_text(&buffer, cx));
        let content = match kind {
            PreviewKind::Markdown => Content::Markdown(cx.new(|cx| {
                TextViewState::markdown("", cx)
                    .scrollable(true)
                    .selectable(true)
            })),
            PreviewKind::PlantUml => Content::PlantUml(Diagrams::default()),
        };
        let mut this = Self {
            view: view.downgrade(),
            workspace,
            kind,
            mode: AppState::global(cx).state.preview,
            content,
            shown: None,
            _subscription: subscription,
        };
        this.update_text(&buffer, cx);
        this
    }

    fn update_text(&mut self, buffer: &Entity<Buffer>, cx: &mut Context<Self>) {
        let doc = buffer.read(cx).doc();
        let state = (doc.revision(), doc.text().len());
        if self.shown == Some(state) {
            return;
        }
        let first = self.shown.is_none();
        self.shown = Some(state);
        let text = doc.text().to_string();
        match &mut self.content {
            Content::Markdown(markdown) => {
                markdown.update(cx, |markdown, cx| markdown.set_text(&text, cx));
            }
            Content::PlantUml(diagrams) => {
                let settings = &AppState::global(cx).settings.preview;
                let setting = &settings.plantuml;
                let includes = settings.plantuml_include_path.clone();
                let layout_setting = settings.plantuml_layout;
                // PlantUML runs in the document's folder: a relative setting is Birchpad's.
                let setting = match std::env::current_dir() {
                    Ok(base) => setting_path(setting, &base),
                    Err(_) => setting.clone(),
                };
                let folder = buffer
                    .read(cx)
                    .path()
                    .and_then(Path::parent)
                    .map(Path::to_path_buf);
                let include = include_path(
                    folder.as_deref(),
                    &includes,
                    std::env::var_os(INCLUDE_PATH_VARIABLE).as_deref(),
                );
                let installed = crate::plantuml::installed(cx);
                diagrams.installs = crate::plantuml::state(cx).map_or(0, |state| state.generation);
                let executor = cx.background_executor().clone();
                // A new drawing replaces the one under way, which stops.
                diagrams.drawing = Some(cx.spawn(async move |this, cx| {
                    if !first {
                        executor.timer(PLANTUML_DELAY).await;
                    }
                    let stop = StopOnDrop(Stop::default());
                    let running = stop.0.clone();
                    let (drawn, layout, included, fell_back) = executor
                        .spawn(async move {
                            let folders: Vec<PathBuf> = include
                                .as_deref()
                                .map(|path| std::env::split_paths(path).collect())
                                .unwrap_or_default();
                            let included = walk_includes(&text, folder.as_deref(), &folders).files;
                            // Graphviz is looked for at each drawing: it may have been installed.
                            let dot = match layout_setting {
                                birchpad_config::DiagramLayout::Smetana => None,
                                _ => system_dot(),
                            };
                            let mut layout = Layout::resolve(layout_setting, dot);
                            let draw = |layout: &Layout| {
                                let (program, args) =
                                    plantuml_command(&setting, installed.as_deref(), layout);
                                server::draw(
                                    &program,
                                    &args,
                                    folder.as_deref(),
                                    include.as_deref(),
                                    &layout.env(),
                                    &text,
                                    &running,
                                )
                            };
                            let mut drawn = draw(&layout);
                            // Smetana crashed: ELK, where PlantUML has it, lays the diagram out.
                            let mut fell_back = false;
                            let again = drawn.as_ref().is_ok_and(|drawn| {
                                draws_again_with_elk(&drawn.mistakes, &layout, layout_setting)
                            });
                            if again {
                                let with_elk = draw(&Layout::Elk);
                                let crashed = with_elk.as_ref().map_or(true, |drawn| {
                                    drawn
                                        .mistakes
                                        .iter()
                                        .any(|mistake| mistake.details.is_some())
                                });
                                if !crashed {
                                    drawn = with_elk;
                                    layout = Layout::Elk;
                                    fell_back = true;
                                }
                            }
                            // Mistakes PlantUML traced to no file of the diagram's, placed.
                            let drawn = drawn.map(|mut drawn| {
                                place_in_includes(
                                    &mut drawn.mistakes,
                                    folder.as_deref(),
                                    &included,
                                );
                                drawn
                            });
                            (drawn, layout.name(), included, fell_back)
                        })
                        .await;
                    drop(stop);
                    this.update(cx, |this, cx| {
                        if let Content::PlantUml(diagrams) = &mut this.content
                            && !matches!(drawn, Err(NotDrawn::Stopped))
                        {
                            diagrams.layout = layout;
                            diagrams.fell_back = fell_back;
                        }
                        let waits = drawn.as_ref().is_ok_and(|drawn| {
                            waits_for_graphviz(&drawn.mistakes, layout, layout_setting)
                        });
                        if !matches!(drawn, Err(NotDrawn::Stopped)) {
                            this.watch_included(included, cx);
                        }
                        this.show_diagrams(drawn);
                        this.wait_for_graphviz(waits, cx);
                        cx.notify();
                    })
                    .ok();
                }));
            }
        }
        cx.notify();
    }

    /// Whether the diagrams are being dragged with the mouse (for tests).
    #[cfg(feature = "test-support")]
    pub fn dragging(&self) -> bool {
        matches!(&self.content, Content::PlantUml(diagrams) if diagrams.drag.is_some())
    }

    /// Draws the diagrams again, as they are on disk now: View > Preview > Redraw, and a change
    /// to a file they include.
    pub fn redraw(&mut self, cx: &mut Context<Self>) {
        if let Some(view) = self.view.upgrade() {
            self.shown = None;
            let buffer = view.read(cx).buffer.clone();
            self.update_text(&buffer, cx);
        }
    }

    /// Watches the folders of the document and of `included`, the files it includes.
    fn watch_included(&mut self, included: Vec<PathBuf>, cx: &mut Context<Self>) {
        let folder = self.folder(cx);
        let Content::PlantUml(diagrams) = &mut self.content else {
            return;
        };
        let folders = watched_folders(folder.as_deref(), &included);
        diagrams.included = included;
        if diagrams.watch.is_none() {
            let (sender, mut events) = futures::channel::mpsc::unbounded::<Vec<PathBuf>>();
            let watcher =
                notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                    if let Ok(event) = event
                        && !event.kind.is_access()
                    {
                        sender.unbounded_send(event.paths).ok();
                    }
                });
            let Ok(watcher) = watcher else {
                return;
            };
            let listening = cx.spawn(async move |this, cx| {
                use futures::StreamExt as _;
                while let Some(mut paths) = events.next().await {
                    // A program writing a file sets off several events: draw once they stop.
                    cx.background_executor()
                        .timer(Duration::from_millis(150))
                        .await;
                    while let Ok(more) = events.try_recv() {
                        paths.extend(more);
                    }
                    let redrawn = this.update(cx, |this, cx| {
                        let document = this.path(cx);
                        let included = match &this.content {
                            Content::PlantUml(diagrams) => diagrams.included.clone(),
                            Content::Markdown(_) => Vec::new(),
                        };
                        if paths
                            .iter()
                            .any(|path| redraws_for(path, document.as_deref(), &included))
                        {
                            this.redraw(cx);
                        }
                    });
                    if redrawn.is_err() {
                        break;
                    }
                }
            });
            diagrams.watch = Some(IncludeWatch {
                watcher,
                folders: Vec::new(),
                _listening: listening,
            });
        }
        let Some(watch) = &mut diagrams.watch else {
            return;
        };
        use notify::Watcher as _;
        for gone in watch.folders.iter().filter(|old| !folders.contains(old)) {
            watch.watcher.unwatch(gone).ok();
        }
        let mut watched = Vec::new();
        for folder in folders {
            let ok = watch.folders.contains(&folder)
                || watch
                    .watcher
                    .watch(&folder, notify::RecursiveMode::NonRecursive)
                    .is_ok();
            if ok {
                watched.push(folder);
            }
        }
        watch.folders = watched;
    }

    /// The folders watched for changes to the files the diagrams include (for tests).
    #[cfg(feature = "test-support")]
    pub fn watched_folders(&self) -> Vec<PathBuf> {
        match &self.content {
            Content::PlantUml(diagrams) => diagrams
                .watch
                .as_ref()
                .map(|watch| watch.folders.clone())
                .unwrap_or_default(),
            Content::Markdown(_) => Vec::new(),
        }
    }

    /// The files the diagrams include, as found when they were last drawn (for tests).
    #[cfg(feature = "test-support")]
    pub fn included(&self) -> Vec<PathBuf> {
        match &self.content {
            Content::PlantUml(diagrams) => diagrams.included.clone(),
            Content::Markdown(_) => Vec::new(),
        }
    }

    fn show_diagrams(&mut self, drawn: Result<Drawn, NotDrawn>) {
        let Content::PlantUml(diagrams) = &mut self.content else {
            return;
        };
        match drawn {
            Ok(drawn) => {
                let zoom = diagrams.zoom;
                diagrams.images = drawn
                    .diagrams
                    .iter()
                    .cloned()
                    .map(|svg| Diagram::new(svg, zoom))
                    .collect();
                diagrams.folded = folded_pictures(&drawn.mistakes, &drawn.diagrams);
                diagrams.unfolded.clear();
                diagrams.mistakes = drawn.mistakes;
                diagrams.error = None;
                diagrams.missing = false;
            }
            // A newer drawing is on its way.
            Err(NotDrawn::Stopped) => return,
            Err(NotDrawn::Missing(error)) => {
                diagrams.error = Some(error);
                diagrams.missing = true;
            }
            Err(NotDrawn::Failed(error)) => {
                diagrams.error = Some(error);
                diagrams.missing = false;
            }
        }
        diagrams.drawing = None;
    }

    /// The SVG of each diagram shown (for tests).
    #[cfg(feature = "test-support")]
    pub fn svgs(&self) -> Vec<Vec<u8>> {
        match &self.content {
            Content::PlantUml(diagrams) => diagrams
                .images
                .iter()
                .map(|diagram| diagram.image.bytes().to_vec())
                .collect(),
            Content::Markdown(_) => Vec::new(),
        }
    }

    /// The SVG of diagram `index` (from 0), if PlantUML drew it.
    pub fn diagram_svg(&self, index: usize) -> Option<Vec<u8>> {
        match &self.content {
            Content::PlantUml(diagrams) => diagrams
                .images
                .get(index)
                .map(|diagram| diagram.image.bytes().to_vec()),
            Content::Markdown(_) => None,
        }
    }

    /// The file of the document shown, if it has one.
    pub fn path(&self, cx: &App) -> Option<PathBuf> {
        let view = self.view.upgrade()?;
        view.read(cx).buffer.read(cx).path().map(Path::to_path_buf)
    }

    /// How large the diagrams are shown; Fit for a Markdown preview.
    pub fn zoom(&self) -> Zoom {
        match &self.content {
            Content::PlantUml(diagrams) => diagrams.zoom,
            Content::Markdown(_) => Zoom::Fit,
        }
    }

    /// Shows the diagrams at `zoom`.
    pub fn set_zoom(&mut self, zoom: Zoom, cx: &mut Context<Self>) {
        if let Content::PlantUml(diagrams) = &mut self.content {
            diagrams.zoom = zoom;
            for diagram in &mut diagrams.images {
                diagram.prepare(zoom);
            }
            cx.notify();
        }
    }

    /// How far the diagrams are scrolled, and how far they can be, across and down (offsets
    /// are negative or zero); `None` but for PlantUML.
    pub fn scroll(&self) -> Option<(Point<Pixels>, Point<Pixels>)> {
        match &self.content {
            Content::PlantUml(diagrams) => {
                Some((diagrams.scroll.offset(), diagrams.scroll.max_offset()))
            }
            Content::Markdown(_) => None,
        }
    }

    /// Shift with the mouse wheel over the diagrams: across instead of down, where the system
    /// does not do it itself. Whether it scrolled.
    fn wheel_across(
        &mut self,
        event: &ScrollWheelEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Content::PlantUml(diagrams) = &self.content else {
            return false;
        };
        let delta = event.delta.pixel_delta(window.line_height());
        if delta.x != px(0.) || delta.y == px(0.) {
            return false;
        }
        let scroll = &diagrams.scroll;
        let offset = scroll.offset();
        let x = (offset.x + delta.y).clamp(-scroll.max_offset().x, px(0.));
        scroll.set_offset(point(x, offset.y));
        cx.notify();
        true
    }

    /// The left button pressed on the diagrams: they follow the mouse until it is let go.
    fn start_drag(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        if let Content::PlantUml(diagrams) = &mut self.content {
            diagrams.drag = Some((position, diagrams.scroll.offset()));
            cx.notify();
        }
    }

    /// The mouse moved while the diagrams are dragged: they are scrolled with it. Whether they
    /// are dragged.
    fn drag_to(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) -> bool {
        let Content::PlantUml(diagrams) = &self.content else {
            return false;
        };
        let Some((from, start)) = diagrams.drag else {
            return false;
        };
        let most = diagrams.scroll.max_offset();
        let (x, y) = dragged_offset(
            (f32::from(start.x), f32::from(start.y)),
            (f32::from(from.x), f32::from(from.y)),
            (f32::from(position.x), f32::from(position.y)),
            (f32::from(most.x), f32::from(most.y)),
        );
        diagrams.scroll.set_offset(point(px(x), px(y)));
        cx.notify();
        true
    }

    /// The mouse let go: the drag ends.
    fn end_drag(&mut self, cx: &mut Context<Self>) {
        if let Content::PlantUml(diagrams) = &mut self.content
            && diagrams.drag.take().is_some()
        {
            cx.notify();
        }
    }

    /// Ctrl with the mouse wheel over the diagrams: a step of zoom for each notch.
    fn wheel_zoom(&mut self, event: &ScrollWheelEvent, cx: &mut Context<Self>) {
        let Content::PlantUml(diagrams) = &mut self.content else {
            return;
        };
        diagrams.wheel += f32::from(event.delta.pixel_delta(px(20.)).y);
        let zoom = diagrams.zoom;
        let zoom = if diagrams.wheel >= WHEEL_STEP {
            zoom.zoom_in()
        } else if diagrams.wheel <= -WHEEL_STEP {
            zoom.zoom_out()
        } else {
            return;
        };
        diagrams.wheel = 0.;
        self.set_zoom(zoom, cx);
    }

    /// The mistakes PlantUML found in the text shown (for tests).
    #[cfg(feature = "test-support")]
    pub fn mistakes(&self) -> Vec<Mistake> {
        match &self.content {
            Content::PlantUml(diagrams) => diagrams.mistakes.clone(),
            Content::Markdown(_) => Vec::new(),
        }
    }

    /// Puts the caret at the start of `line` (from 0) of the text `this` shows, in its view: a
    /// mistake's line. Not while `this` is being updated: activating the view draws its preview.
    pub fn go_to_line(this: &Entity<Self>, line: usize, window: &mut Window, cx: &mut App) {
        let (view, workspace) = {
            let preview = this.read(cx);
            (preview.view.upgrade(), preview.workspace.upgrade())
        };
        let Some(view) = view else {
            return;
        };
        if let Some(workspace) = workspace {
            workspace.update(cx, |workspace, cx| {
                workspace.activate_view(&view, window, cx);
            });
        }
        view.update(cx, |view, cx| {
            let text = view.text(cx).to_string();
            let at = line_offset(&text, line);
            view.select_range(at..at, cx);
        });
        let focus = view.read(cx).focus_handle.clone();
        window.focus(&focus, cx);
    }

    /// Opens `path` at `line` (from 1) in the window of `this`: a mistake in an included file.
    pub fn open_at(
        this: &Entity<Self>,
        path: &Path,
        line: usize,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(workspace) = this.read(cx).workspace.upgrade() {
            workspace.update(cx, |workspace, cx| {
                workspace.open_path_at_line(path, line, window, cx);
            });
        }
    }

    /// The text the preview shows, without its formatting, or what PlantUML did (for tests).
    #[cfg(feature = "test-support")]
    pub fn rendered(&self, cx: &App) -> String {
        match &self.content {
            Content::Markdown(markdown) => markdown.read(cx).rendered_text().as_str().to_owned(),
            Content::PlantUml(diagrams) => diagrams
                .error
                .clone()
                .unwrap_or_else(|| format!("{} diagrams", diagrams.images.len())),
        }
    }

    fn folder(&self, cx: &App) -> Option<PathBuf> {
        let view = self.view.upgrade()?;
        let buffer = view.read(cx).buffer.read(cx);
        buffer.path()?.parent().map(Path::to_path_buf)
    }

    fn render_markdown(&self, markdown: &Entity<TextViewState>, cx: &App) -> AnyElement {
        let folder = self.folder(cx);
        let images = folder.clone();
        let workspace = self.workspace.clone();
        TextView::new(markdown)
            .scrollable(true)
            .selectable(true)
            .size_full()
            .px_4()
            .image_source(move |uri| image_source(uri, images.as_deref()))
            .on_link_click(move |link, _, window, cx| {
                follow_link(link, folder.as_deref(), &workspace, window, cx);
            })
            .into_any_element()
    }

    /// The quiet line above diagrams drawn by an earlier PlantUML of Birchpad's: a newer one lays
    /// out what smetana cannot, with Update, then its progress, or why it failed.
    fn render_update(cx: &App) -> AnyElement {
        let state = crate::plantuml::state(cx);
        let megabytes = |bytes: u64| format!("{:.1}", bytes as f64 / (1024. * 1024.));
        let size = crate::plantuml::can_install(cx).unwrap_or_default();
        let action = match state {
            Some(state) if state.running => div()
                .child(t!(
                    "plantuml.installing",
                    received = megabytes(state.received.load(std::sync::atomic::Ordering::Relaxed)),
                    size = megabytes(size)
                ))
                .into_any_element(),
            _ => div()
                .id("plantuml-update")
                .text_color(paint(ui().accent))
                .cursor_pointer()
                .hover(|link| link.underline())
                .on_click(|_, window, cx| {
                    if let Err(error) = crate::plantuml::install(cx) {
                        crate::workspace::report_error(&error, window, cx);
                    }
                })
                .child(t!("plantuml.update", size = megabytes(size)))
                .into_any_element(),
        };
        div()
            .w_full()
            .flex()
            .flex_wrap()
            .gap_2()
            .text_color(paint(ui().muted))
            .child(t!("plantuml.update-available"))
            .child(action)
            .children(state.and_then(|state| state.error.clone()).map(error_text))
            .into_any_element()
    }

    /// What the preview offers when there is no PlantUML: to install it, its progress, or why
    /// it cannot be.
    fn render_install(cx: &App) -> AnyElement {
        let state = crate::plantuml::state(cx);
        let megabytes = |bytes: u64| format!("{:.1}", bytes as f64 / (1024. * 1024.));
        let body = match (crate::plantuml::can_install(cx), state) {
            (Ok(size), Some(state)) if state.running => div()
                .text_color(paint(ui().muted))
                .child(t!(
                    "plantuml.installing",
                    received = megabytes(state.received.load(std::sync::atomic::Ordering::Relaxed)),
                    size = megabytes(size)
                ))
                .into_any_element(),
            (Ok(size), state) => div()
                .flex()
                .flex_col()
                .items_center()
                .gap_2()
                .child(
                    Button::new("install-plantuml")
                        .primary()
                        .label(t!("plantuml.install", size = megabytes(size)))
                        .on_click(|_, window, cx| {
                            if let Err(error) = crate::plantuml::install(cx) {
                                crate::workspace::report_error(&error, window, cx);
                            }
                        }),
                )
                .child(
                    div()
                        .text_color(paint(ui().muted))
                        .child(t!("plantuml.what-is-installed")),
                )
                .children(state.and_then(|state| state.error.clone()).map(error_text))
                .into_any_element(),
            (Err(why), _) => div()
                .text_color(paint(ui().muted))
                .child(format!("{why:#}"))
                .into_any_element(),
        };
        div()
            .w_full()
            .flex()
            .justify_center()
            .child(body)
            .into_any_element()
    }

    /// What a crash of PlantUML shows: one line, with the layout it used, and its stack trace
    /// folded under it.
    fn render_crash(
        mistake: &Mistake,
        details: &str,
        diagrams: &Diagrams,
        this: &WeakEntity<Self>,
        workspace: &WeakEntity<Workspace>,
        outdated: bool,
    ) -> AnyElement {
        let this = this.clone();
        let toggle = if diagrams.details {
            t!("preview.plantuml-hide-details")
        } else {
            t!("preview.plantuml-show-details")
        };
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap_1()
            .child(div().text_color(paint(ui().error)).child(t!(
                "preview.plantuml-crashed",
                layout = diagrams.layout,
                error = mistake.message.as_str()
            )))
            .when(diagrams.layout == Layout::Smetana.name(), |crash| {
                let workspace = workspace.clone();
                let link = |id: &'static str, text: String, command: &'static str| {
                    let workspace = workspace.clone();
                    div()
                        .id(id)
                        .text_color(paint(ui().accent))
                        .cursor_pointer()
                        .hover(|link| link.underline())
                        .on_click(move |_, window, cx| {
                            if let Some(workspace) = workspace.upgrade() {
                                workspace.update(cx, |workspace, cx| {
                                    workspace.run(&Invocation::new(command), window, cx)
                                });
                            }
                        })
                        .child(text)
                };
                crash
                    .child(
                        div()
                            .text_color(paint(ui().muted))
                            .child(t!("preview.plantuml-try-graphviz")),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_4()
                            .child(link(
                                "plantuml-get-graphviz",
                                t!("preview.plantuml-get-graphviz", url = GRAPHVIZ_DOWNLOAD),
                                "preview.get-graphviz",
                            ))
                            .child(link(
                                "plantuml-choose-layout",
                                t!("preview.plantuml-choose-layout"),
                                "preview.choose-layout",
                            ))
                            // An earlier PlantUML of Birchpad's, without ELK (ADR 0063).
                            .when(outdated, |links| {
                                links.child(link(
                                    "plantuml-install-elk",
                                    t!("preview.plantuml-install-with-elk"),
                                    "preview.install-plantuml",
                                ))
                            }),
                    )
            })
            .child(
                div()
                    .id("plantuml-crash-details")
                    .text_color(paint(ui().muted))
                    .cursor_pointer()
                    .hover(|row| row.underline())
                    .on_click(move |_, _, cx| {
                        this.update(cx, |this, cx| this.toggle_details(cx)).ok();
                    })
                    .child(toggle),
            )
            .when(diagrams.details, |crash| {
                crash.child(
                    div()
                        .font_family(crate::MONOSPACE)
                        .text_xs()
                        .text_color(paint(ui().muted))
                        .whitespace_normal()
                        .children(details.lines().map(|line| div().child(line.to_owned()))),
                )
            })
            .into_any_element()
    }

    /// Looks for Graphviz every few seconds while `waits`, and draws again once it is there.
    fn wait_for_graphviz(&mut self, waits: bool, cx: &mut Context<Self>) {
        let Content::PlantUml(diagrams) = &mut self.content else {
            return;
        };
        if !waits {
            diagrams.graphviz = None;
            return;
        }
        if diagrams.graphviz.is_some() {
            return;
        }
        diagrams.graphviz = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(DOT_POLL).await;
                if system_dot().is_some() {
                    this.update(cx, |this, cx| {
                        if let Content::PlantUml(diagrams) = &mut this.content {
                            diagrams.graphviz = None;
                        }
                        this.redraw(cx);
                    })
                    .ok();
                    break;
                }
            }
        }));
    }

    /// Whether smetana crashed and ELK drew the diagrams instead (for tests).
    #[cfg(feature = "test-support")]
    pub fn fell_back_to_elk(&self) -> bool {
        matches!(&self.content, Content::PlantUml(diagrams) if diagrams.fell_back)
    }

    /// Whether the preview looks for Graphviz to draw again with it (for tests).
    #[cfg(feature = "test-support")]
    pub fn waiting_for_graphviz(&self) -> bool {
        matches!(&self.content, Content::PlantUml(diagrams) if diagrams.graphviz.is_some())
    }

    /// Shows the folded picture of a placed mistake, diagram `index`, or folds it again.
    pub fn toggle_picture(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Content::PlantUml(diagrams) = &mut self.content
            && diagrams.folded.contains(&index)
        {
            match diagrams.unfolded.iter().position(|&shown| shown == index) {
                Some(at) => {
                    diagrams.unfolded.remove(at);
                }
                None => diagrams.unfolded.push(index),
            }
            cx.notify();
        }
    }

    /// The diagrams folded as pictures of placed mistakes, and those shown all the same (for
    /// tests).
    #[cfg(feature = "test-support")]
    pub fn folded(&self) -> (Vec<usize>, Vec<usize>) {
        match &self.content {
            Content::PlantUml(diagrams) => (diagrams.folded.clone(), diagrams.unfolded.clone()),
            Content::Markdown(_) => (Vec::new(), Vec::new()),
        }
    }

    /// Shows or folds the details of a crash of PlantUML.
    pub fn toggle_details(&mut self, cx: &mut Context<Self>) {
        if let Content::PlantUml(diagrams) = &mut self.content {
            diagrams.details = !diagrams.details;
            cx.notify();
        }
    }

    /// Whether the details of a crash are shown (for tests).
    #[cfg(feature = "test-support")]
    pub fn details_shown(&self) -> bool {
        matches!(&self.content, Content::PlantUml(diagrams) if diagrams.details)
    }

    /// The name of the layout the diagrams were last drawn with (for tests).
    #[cfg(feature = "test-support")]
    pub fn layout(&self) -> &'static str {
        match &self.content {
            Content::PlantUml(diagrams) => diagrams.layout,
            Content::Markdown(_) => "",
        }
    }

    /// PlantUML's mistakes, each a link to its line.
    fn render_mistakes(
        diagrams: &Diagrams,
        this: &WeakEntity<Self>,
        workspace: &WeakEntity<Workspace>,
        outdated: bool,
    ) -> impl IntoElement {
        let mistakes = &diagrams.mistakes;
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap_1()
            .children(mistakes.iter().enumerate().map(|(index, mistake)| {
                if let Some(details) = &mistake.details {
                    return Self::render_crash(
                        mistake, details, diagrams, this, workspace, outdated,
                    );
                }
                let message = mistake.message.as_str();
                let text = match (mistake.line, &mistake.included) {
                    (Some(line), Some(included)) => t!(
                        "preview.plantuml-mistake-included",
                        line = line + 1,
                        file = included.file.as_str(),
                        at = included.line,
                        message = message
                    ),
                    (None, Some(included)) if included.file.is_empty() => {
                        t!("preview.plantuml-mistake-in-an-include", message = message)
                    }
                    (None, Some(included)) => t!(
                        "preview.plantuml-mistake-in-file",
                        file = included.file.as_str(),
                        at = included.line,
                        message = message
                    ),
                    (Some(line), None) => {
                        t!(
                            "preview.plantuml-mistake",
                            line = line + 1,
                            message = message
                        )
                    }
                    (None, None) => mistake.message.clone(),
                };
                let this = this.clone();
                let line = mistake.line;
                // Placed in no line of the document: its file, at its line.
                let file = mistake
                    .included
                    .as_ref()
                    .and_then(|included| Some((included.path.clone()?, included.line)));
                let linked = line.is_some() || file.is_some();
                div()
                    .id(("plantuml-mistake", index))
                    .text_color(paint(ui().error))
                    .when(linked, |row| {
                        row.cursor_pointer().hover(|row| row.underline())
                    })
                    .on_click(move |_, window, cx| {
                        let Some(this) = this.upgrade() else {
                            return;
                        };
                        if let Some(line) = line {
                            Self::go_to_line(&this, line, window, cx);
                        } else if let Some((path, at)) = &file {
                            Self::open_at(&this, path, *at, window, cx);
                        }
                    })
                    .child(text)
                    .into_any_element()
            }))
    }

    fn render_diagrams(
        diagrams: &Diagrams,
        workspace: &WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let this = &cx.entity().downgrade();
        let white = background(cx) == DiagramBackground::White;
        let zoom = diagrams.zoom.percent().map(|percent| percent as f32 / 100.);
        // At Fit and Fit Page, the room the diagrams have: the preview's area, less its margins.
        // Each diagram is given its size from its own and the room's, rather than from its
        // image's, which is not known before the image is decoded: the first drawing at Fit
        // was otherwise as tall as the preview, its diagram at the top (roadmap 7.4x).
        let viewport = diagrams.scroll.bounds().size;
        let room = (
            f32::from(viewport.width) - 32.,
            f32::from(viewport.height) - 32.,
        );
        let laid_out = room.0 > 0. && room.1 > 0.;
        let fit_page = diagrams.zoom == Zoom::FitPage && laid_out;
        let fit = diagrams.zoom == Zoom::Fit && laid_out;
        // Not laid out yet: drawn again once the preview has its size.
        if !laid_out && zoom.is_none() && !diagrams.images.is_empty() {
            window.request_animation_frame();
        }
        let dragging = diagrams.drag.is_some();
        let message = match (&diagrams.error, &diagrams.drawing) {
            (Some(error), _) => Some((error.clone(), ui().error)),
            (None, Some(_)) if diagrams.images.is_empty() => {
                Some((t!("preview.plantuml-drawing"), ui().muted))
            }
            _ => None,
        };
        // Zoomed, as wide as the widest diagram, so that the preview scrolls across to it; at
        // Fit, as wide as the preview.
        let content = div()
            .map(|content| match zoom {
                Some(_) => content.flex_none(),
                None => content.w_full(),
            })
            .min_w_full()
            .min_h_full()
            .p_4()
            .flex()
            .flex_col()
            .items_start()
            .gap_4()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, _, cx| {
                    this.start_drag(event.position, cx);
                }),
            )
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, window, cx| {
                if event.modifiers.secondary() {
                    this.wheel_zoom(event, cx);
                    cx.stop_propagation();
                } else if event.modifiers.shift && this.wheel_across(event, window, cx) {
                    cx.stop_propagation();
                }
            }))
            .when_some(message, |list, (message, color)| {
                list.child(div().w_full().text_color(paint(color)).child(message))
            })
            .when(crate::plantuml::offers_update(cx), |list| {
                list.child(Self::render_update(cx))
            })
            .when(diagrams.fell_back, |list| {
                list.child(
                    div()
                        .w_full()
                        .text_color(paint(ui().muted))
                        .child(t!("preview.plantuml-drawn-with-elk")),
                )
            })
            .when(!diagrams.mistakes.is_empty(), |list| {
                list.child(Self::render_mistakes(
                    diagrams,
                    this,
                    workspace,
                    crate::plantuml::outdated(cx),
                ))
            })
            .when(diagrams.missing, |list| {
                list.child(Self::render_install(cx))
            })
            .children(diagrams.images.iter().enumerate().map(|(index, diagram)| {
                let folded = diagrams.folded.contains(&index);
                let shown = !folded || diagrams.unfolded.contains(&index);
                // The picture of a placed mistake: a line that shows it, or hides it again.
                let toggle = folded.then(|| {
                    let this = this.clone();
                    div()
                        .id(("plantuml-picture-toggle", index))
                        .w_full()
                        .text_color(paint(ui().muted))
                        .cursor_pointer()
                        .hover(|row| row.underline())
                        .on_click(move |_, _, cx| {
                            this.update(cx, |this, cx| this.toggle_picture(index, cx))
                                .ok();
                        })
                        .child(if shown {
                            t!("preview.plantuml-hide-picture")
                        } else {
                            t!("preview.plantuml-show-picture")
                        })
                });
                let image = img(ImageSource::Image(diagram.shown()));
                // At a zoom, the diagram's size times it; at Fit, no wider than the preview.
                let image = match (zoom, diagram.size) {
                    (Some(zoom), Some((width, height))) => {
                        image.w(px(width * zoom)).h(px(height * zoom))
                    }
                    (None, Some((width, height))) if fit_page => {
                        let scale = fit_page_scale((width, height), room);
                        image.w(px(width * scale)).h(px(height * scale))
                    }
                    (None, Some((width, height))) if fit => {
                        let scale = fit_width_scale((width, height), room.0);
                        image.w(px(width * scale)).h(px(height * scale))
                    }
                    _ => image.max_w_full().object_fit(ObjectFit::Contain),
                };
                // Diagrams are made for paper: on white, unless the theme's background is asked.
                // Centered while it fits; a wider one starts at the left, to scroll to.
                let card = div()
                    .id(("plantuml-diagram", index))
                    .debug_selector(move || format!("plantuml-diagram-{index}"))
                    .flex_none()
                    .mx_auto()
                    .when(zoom.is_none(), |card| card.max_w_full())
                    .when(white, |card| card.bg(gpui_kit::white()))
                    .child(image)
                    .context_menu(Self::diagram_menu(index, workspace.clone()));
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .children(toggle)
                    .when(shown, |column| column.child(card))
            }));
        div()
            .size_full()
            .relative()
            .child(
                div()
                    .id("plantuml-diagrams")
                    .debug_selector(|| "plantuml-diagrams".into())
                    .size_full()
                    // A hand that drags the diagrams, held while it does.
                    .cursor(if dragging {
                        CursorStyle::ClosedHand
                    } else {
                        CursorStyle::OpenHand
                    })
                    .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                        if event.pressed_button == Some(MouseButton::Left) {
                            this.drag_to(event.position, cx);
                        } else {
                            this.end_drag(cx);
                        }
                    }))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _: &MouseUpEvent, _, cx| this.end_drag(cx)),
                    )
                    .on_mouse_up_out(
                        MouseButton::Left,
                        cx.listener(|this, _: &MouseUpEvent, _, cx| this.end_drag(cx)),
                    )
                    // Not stretched to the preview's height: as tall as the diagrams.
                    .flex()
                    .items_start()
                    .track_scroll(&diagrams.scroll)
                    .overflow_scroll()
                    .child(content),
            )
            .child(
                div().absolute().inset_0().child(
                    Scrollbar::new(&diagrams.scroll)
                        .id("plantuml-scrollbars")
                        .axis(ScrollbarAxis::Both)
                        .mode(ScrollbarMode::Always)
                        .viewport_from_layout(),
                ),
            )
            .into_any_element()
    }

    /// The context menu of diagram `index`: copied as a picture, or exported as PNG or SVG.
    fn diagram_menu(
        index: usize,
        workspace: WeakEntity<Workspace>,
    ) -> impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static {
        move |menu, _, _| {
            let item = |command: &'static str| {
                let workspace = workspace.clone();
                PopupMenuItem::new(crate::menus::command_title(command)).on_click(
                    move |_: &ClickEvent, window, cx| {
                        let invocation =
                            Invocation::with_args(command, serde_json::json!({ "diagram": index }));
                        workspace
                            .update(cx, |workspace, cx| workspace.run(&invocation, window, cx))
                            .ok();
                    },
                )
            };
            menu.item(item("preview.copy-diagram"))
                .separator()
                .item(item("preview.export-png"))
                .item(item("preview.export-svg"))
        }
    }

    /// The bar above the diagrams: their zoom, and the background they are drawn on.
    fn render_toolbar(&self, cx: &App) -> impl IntoElement {
        let white = background(cx) == DiagramBackground::White;
        let zoom = self.zoom();
        let workspace = self.workspace.clone();
        let run = move |command: &'static str| {
            let workspace = workspace.clone();
            move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                workspace
                    .update(cx, |workspace, cx| {
                        workspace.run(&Invocation::new(command), window, cx)
                    })
                    .ok();
            }
        };
        bar(BarColors::surface(), Line::Under)
            .gap_1()
            .px_2()
            .py_1()
            .child(
                Button::new("zoom-out")
                    .xsmall()
                    .ghost()
                    .icon(IconName::Minus)
                    .tooltip(crate::menus::command_title("preview.zoom-out"))
                    .on_click(run("preview.zoom-out")),
            )
            .child(
                Button::new("zoom")
                    .xsmall()
                    .ghost()
                    .label(zoom.label())
                    .tooltip(crate::menus::command_title("preview.actual-size"))
                    .on_click(run("preview.actual-size")),
            )
            .child(
                Button::new("zoom-in")
                    .xsmall()
                    .ghost()
                    .icon(IconName::Plus)
                    .tooltip(crate::menus::command_title("preview.zoom-in"))
                    .on_click(run("preview.zoom-in")),
            )
            .child(
                Button::new("zoom-fit")
                    .xsmall()
                    .ghost()
                    .icon(IconName::Maximize)
                    .tooltip(crate::menus::command_title("preview.fit"))
                    .selected(zoom == Zoom::Fit)
                    .on_click(run("preview.fit")),
            )
            .child(
                Button::new("zoom-fit-page")
                    .xsmall()
                    .ghost()
                    .icon(IconName::Frame)
                    .tooltip(crate::menus::command_title("preview.fit-page"))
                    .selected(zoom == Zoom::FitPage)
                    .on_click(run("preview.fit-page")),
            )
            .child(div().w(px(1.)).h(px(16.)).bg(paint(ui().border)))
            .child(
                Button::new("redraw")
                    .xsmall()
                    .ghost()
                    .icon(IconName::RefreshCw)
                    .tooltip(crate::menus::command_title("preview.redraw"))
                    .on_click(run("preview.redraw")),
            )
            .child(
                Button::new("diagram-background")
                    .xsmall()
                    .ghost()
                    .icon(if white { IconName::Sun } else { IconName::Moon })
                    .tooltip(crate::menus::command_title("preview.white-background"))
                    .selected(white)
                    .on_click(run("preview.white-background")),
            )
    }
}

impl Render for Preview {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // PlantUML was installed since the diagrams were drawn (missing, crashed, or drawn by an
        // earlier revision): draw again with it.
        let installs = crate::plantuml::state(cx).map_or(0, |state| state.generation);
        if let Content::PlantUml(diagrams) = &self.content
            && diagrams.installs != installs
            && let Some(view) = self.view.upgrade()
        {
            self.shown = None;
            let buffer = view.read(cx).buffer.clone();
            self.update_text(&buffer, cx);
        }
        let colors = crate::theme::current().editor;
        let body = match &self.content {
            Content::Markdown(markdown) => self.render_markdown(markdown, cx),
            Content::PlantUml(diagrams) => div()
                .size_full()
                .flex()
                .flex_col()
                .child(self.render_toolbar(cx))
                .child(div().flex_1().min_h_0().child(Self::render_diagrams(
                    diagrams,
                    &self.workspace,
                    window,
                    cx,
                )))
                .into_any_element(),
        };
        div()
            .id("preview")
            .size_full()
            .bg(paint(colors.background))
            .text_color(paint(colors.text))
            .child(body)
    }
}

fn follow_link(
    link: &SharedString,
    folder: Option<&Path>,
    workspace: &WeakEntity<Workspace>,
    window: &mut Window,
    cx: &mut App,
) {
    match link_target(link, folder) {
        LinkTarget::Url(url) => {
            let _ = crate::file_commands::shell(crate::file_commands::Shell::Browse(url), cx);
        }
        LinkTarget::File(path) => {
            let _ = workspace.update(cx, |workspace, cx| workspace.open_path(&path, window, cx));
        }
        LinkTarget::None => {}
    }
}

/// The buttons at the top right of a document with a preview: the text alone, both, the
/// preview alone.
pub fn render_switch(
    mode: PreviewMode,
    on_choose: impl Fn(PreviewMode, &mut Window, &mut App) + Clone + 'static,
) -> AnyElement {
    let button = |id: &'static str, icon: IconName, shows: PreviewMode, tooltip: String| {
        let on_choose = on_choose.clone();
        Button::new(id)
            .xsmall()
            .ghost()
            .icon(icon)
            .tooltip(tooltip)
            .selected(mode == shows)
            .on_click(move |_, window, cx| on_choose(shows, window, cx))
    };
    div()
        .absolute()
        .top_1()
        .right(px(20.))
        .flex()
        .flex_row()
        .gap_0p5()
        .p_0p5()
        .rounded_md()
        .border_1()
        .border_color(paint(ui().border))
        .bg(paint(ui().surface))
        .child(button(
            "preview-editor",
            IconName::FileText,
            PreviewMode::Editor,
            t!("preview.editor-only"),
        ))
        .child(button(
            "preview-split",
            IconName::PanelRight,
            PreviewMode::Split,
            t!("preview.editor-and-preview"),
        ))
        .child(button(
            "preview-only",
            IconName::Eye,
            PreviewMode::Preview,
            t!("preview.preview-only"),
        ))
        .into_any_element()
}

/// Remembers `mode` for documents opened later.
pub fn remember(mode: PreviewMode, cx: &mut App) {
    AppState::update_state(cx, |state, _| state.preview = mode);
}
