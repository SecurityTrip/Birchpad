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

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::bail;
use birchpad_config::PreviewMode;
use birchpad_core::RevisionId;
use gpui_kit::base::text::{TextView, TextViewState};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{IconName, Selectable as _, Sizable};
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, Image, ImageFormat, ImageSource, ObjectFit,
    Resource, SharedString, SharedUri, Subscription, Task, WeakEntity, Window, div, img,
    prelude::*, px,
};

use crate::app_state::AppState;
use crate::buffer::Buffer;
use crate::commands::{CommandRegistry, Register as _};
use crate::editor::EditorView;
use crate::theme::{paint, ui};
use crate::workspace::Workspace;

pub use diagrams::{
    DIAGRAM_END, Drawn, Mistake, NotDrawn, PLANTUML_DELAY, Stop, StopOnDrop, diagram_starts,
    line_offset, mistakes, plantuml_command, run_plantuml, setting_path, split_diagrams,
};

/// What a document's preview shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewKind {
    Markdown,
    /// Diagrams drawn by PlantUML, installed apart from Birchpad.
    PlantUml,
}

/// PlantUML's files: `.puml` and the other extensions its tools know.
const PLANTUML_EXTENSIONS: [&str; 5] = ["puml", "plantuml", "pu", "iuml", "wsd"];

/// The preview `buffer` has: from its language, or for PlantUML (which has no language of its
/// own yet) from its file's extension.
pub fn kind_of(buffer: &Buffer) -> Option<PreviewKind> {
    kind_for(buffer.language().map(|language| language.id), buffer.path())
}

/// The preview of a document in `language` (by id) kept at `path`.
pub fn kind_for(language: Option<&str>, path: Option<&Path>) -> Option<PreviewKind> {
    if language == Some("markdown") {
        return Some(PreviewKind::Markdown);
    }
    let extension = path?.extension()?.to_str()?;
    PLANTUML_EXTENSIONS
        .iter()
        .any(|known| known.eq_ignore_ascii_case(extension))
        .then_some(PreviewKind::PlantUml)
}

/// The commands of View > Preview and the mode each one shows.
pub const COMMANDS: [(&str, PreviewMode); 3] = [
    ("view.preview-editor", PreviewMode::Editor),
    ("view.preview-split", PreviewMode::Split),
    ("view.preview-only", PreviewMode::Preview),
];

pub fn register_commands(registry: &mut CommandRegistry) {
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
    images: Vec<Arc<Image>>,
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
                let setting = &AppState::global(cx).settings.preview.plantuml;
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
                    let drawn = executor
                        .spawn(async move {
                            let (program, args) = plantuml_command(&setting, installed.as_deref());
                            run_plantuml(&program, &args, folder.as_deref(), &text, &running)
                        })
                        .await;
                    drop(stop);
                    this.update(cx, |this, cx| {
                        this.show_diagrams(drawn);
                        cx.notify();
                    })
                    .ok();
                }));
            }
        }
        cx.notify();
    }

    fn show_diagrams(&mut self, drawn: Result<Drawn, NotDrawn>) {
        let Content::PlantUml(diagrams) = &mut self.content else {
            return;
        };
        match drawn {
            Ok(drawn) => {
                diagrams.images = drawn
                    .diagrams
                    .into_iter()
                    .map(|svg| Arc::new(Image::from_bytes(ImageFormat::Svg, svg)))
                    .collect();
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
                .map(|image| image.bytes().to_vec())
                .collect(),
            Content::Markdown(_) => Vec::new(),
        }
    }

    /// The mistakes PlantUML found in the text shown (for tests).
    #[cfg(feature = "test-support")]
    pub fn mistakes(&self) -> Vec<Mistake> {
        match &self.content {
            Content::PlantUml(diagrams) => diagrams.mistakes.clone(),
            Content::Markdown(_) => Vec::new(),
        }
    }

    /// Puts the caret at the start of `line` (from 0) of the text, in its view: a mistake's
    /// line.
    pub fn go_to_line(&self, line: usize, window: &mut Window, cx: &mut App) {
        let Some(view) = self.view.upgrade() else {
            return;
        };
        if let Some(workspace) = self.workspace.upgrade() {
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
                .children(
                    state
                        .and_then(|state| state.error.clone())
                        .map(|error| div().text_color(paint(ui().error)).child(error)),
                )
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

    /// PlantUML's mistakes, each a link to its line.
    fn render_mistakes(mistakes: &[Mistake], this: &WeakEntity<Self>) -> impl IntoElement {
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap_1()
            .children(mistakes.iter().enumerate().map(|(index, mistake)| {
                let text = match mistake.line {
                    Some(line) => t!(
                        "preview.plantuml-mistake",
                        line = line + 1,
                        message = mistake.message.as_str()
                    ),
                    None => mistake.message.clone(),
                };
                let this = this.clone();
                let line = mistake.line;
                div()
                    .id(("plantuml-mistake", index))
                    .text_color(paint(ui().error))
                    .when(line.is_some(), |row| {
                        row.cursor_pointer().hover(|row| row.underline())
                    })
                    .on_click(move |_, window, cx| {
                        if let Some(line) = line {
                            this.update(cx, |this, cx| this.go_to_line(line, window, cx))
                                .ok();
                        }
                    })
                    .child(text)
            }))
    }

    fn render_diagrams(diagrams: &Diagrams, this: &WeakEntity<Self>, cx: &App) -> AnyElement {
        let message = match (&diagrams.error, &diagrams.drawing) {
            (Some(error), _) => Some((error.clone(), ui().error)),
            (None, Some(_)) if diagrams.images.is_empty() => {
                Some((t!("preview.plantuml-drawing"), ui().muted))
            }
            _ => None,
        };
        div()
            .id("plantuml-diagrams")
            .size_full()
            .overflow_scroll()
            .p_4()
            .flex()
            .flex_col()
            .items_center()
            .gap_4()
            .when_some(message, |list, (message, color)| {
                list.child(div().w_full().text_color(paint(color)).child(message))
            })
            .when(!diagrams.mistakes.is_empty(), |list| {
                list.child(Self::render_mistakes(&diagrams.mistakes, this))
            })
            .when(diagrams.missing, |list| {
                list.child(Self::render_install(cx))
            })
            .children(diagrams.images.iter().map(|image| {
                img(ImageSource::Image(image.clone()))
                    .flex_none()
                    .max_w_full()
                    .object_fit(ObjectFit::Contain)
            }))
            .into_any_element()
    }
}

impl Render for Preview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // PlantUML was missing and has been installed since: draw again.
        let installs = crate::plantuml::state(cx).map_or(0, |state| state.generation);
        if let Content::PlantUml(diagrams) = &self.content
            && diagrams.missing
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
            Content::PlantUml(diagrams) => {
                Self::render_diagrams(diagrams, &cx.entity().downgrade(), cx)
            }
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
