//! Previews beside the text, as IntelliJ IDEA shows them: a Markdown document is shown as its
//! text and its rendered preview side by side, and the buttons at the top right of the text (or
//! View > Preview) show the text alone, both, or the preview alone.
//!
//! Each view of a document has its own preview, kept by its pane, which shows the text as it
//! is being edited. The mode last chosen is remembered in `state.toml` (`preview`) and applies
//! to documents opened after it.
//!
//! Links in the preview open in the browser, or in Birchpad when they name a file; images are
//! read from files next to the document, never from the network.

use std::path::{Path, PathBuf};

use anyhow::bail;
use birchpad_config::PreviewMode;
use birchpad_core::RevisionId;
use gpui_kit::base::text::{TextView, TextViewState};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{IconName, Selectable as _, Sizable};
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, ImageSource, Resource, SharedString,
    SharedUri, Subscription, WeakEntity, Window, div, prelude::*, px,
};

use crate::app_state::AppState;
use crate::buffer::Buffer;
use crate::commands::{CommandRegistry, Register as _};
use crate::editor::EditorView;
use crate::theme::{paint, ui};
use crate::workspace::Workspace;

/// What a document's preview shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PreviewKind {
    Markdown,
}

/// The preview `buffer` has, from its language.
pub(crate) fn kind_of(buffer: &Buffer) -> Option<PreviewKind> {
    match buffer.language()?.id {
        "markdown" => Some(PreviewKind::Markdown),
        _ => None,
    }
}

/// The commands of View > Preview and the mode each one shows.
pub(crate) const COMMANDS: [(&str, PreviewMode); 3] = [
    ("view.preview-editor", PreviewMode::Editor),
    ("view.preview-split", PreviewMode::Split),
    ("view.preview-only", PreviewMode::Preview),
];

pub(crate) fn register_commands(registry: &mut CommandRegistry) {
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
pub(crate) fn is_checked(command: &str, mode: Option<PreviewMode>) -> Option<bool> {
    COMMANDS
        .iter()
        .find(|(id, _)| *id == command)
        .map(|(_, shows)| Some(*shows) == mode)
}

/// What a link in the preview of a document in `folder` leads to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LinkTarget {
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
fn local_path(link: &str, folder: Option<&Path>) -> Option<PathBuf> {
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
fn percent_decode(text: &str) -> String {
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
pub(crate) fn link_target(link: &str, folder: Option<&Path>) -> LinkTarget {
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
pub(crate) fn image_path(source: &str, folder: Option<&Path>) -> Option<PathBuf> {
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

/// The preview of one view: the rendered text, kept up to date as the text changes.
pub(crate) struct Preview {
    view: WeakEntity<EditorView>,
    workspace: WeakEntity<Workspace>,
    pub(crate) mode: PreviewMode,
    markdown: Entity<TextViewState>,
    /// The state of the text the preview shows.
    shown: Option<(RevisionId, usize)>,
    _subscription: Subscription,
}

impl Preview {
    pub(crate) fn new(
        view: &Entity<EditorView>,
        workspace: WeakEntity<Workspace>,
        cx: &mut Context<Self>,
    ) -> Self {
        let buffer = view.read(cx).buffer.clone();
        let subscription = cx.observe(&buffer, |this, buffer, cx| this.update_text(&buffer, cx));
        let markdown = cx.new(|cx| {
            TextViewState::markdown("", cx)
                .scrollable(true)
                .selectable(true)
        });
        let mut this = Self {
            view: view.downgrade(),
            workspace,
            mode: AppState::global(cx).state.preview,
            markdown,
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
        self.shown = Some(state);
        let text = doc.text().to_string();
        self.markdown
            .update(cx, |markdown, cx| markdown.set_text(&text, cx));
        cx.notify();
    }

    /// The text the preview shows, without its formatting (for tests).
    #[cfg(test)]
    pub(crate) fn rendered(&self, cx: &App) -> String {
        self.markdown.read(cx).rendered_text().as_str().to_owned()
    }

    fn folder(&self, cx: &App) -> Option<PathBuf> {
        let view = self.view.upgrade()?;
        let buffer = view.read(cx).buffer.read(cx);
        buffer.path()?.parent().map(Path::to_path_buf)
    }
}

impl Render for Preview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let folder = self.folder(cx);
        let images = folder.clone();
        let workspace = self.workspace.clone();
        let colors = crate::theme::current().editor;
        div()
            .id("preview")
            .size_full()
            .bg(paint(colors.background))
            .text_color(paint(colors.text))
            .child(
                TextView::new(&self.markdown)
                    .scrollable(true)
                    .selectable(true)
                    .size_full()
                    .px_4()
                    .image_source(move |uri| image_source(uri, images.as_deref()))
                    .on_link_click(move |link, _, window, cx| {
                        follow_link(link, folder.as_deref(), &workspace, window, cx);
                    }),
            )
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
pub(crate) fn render_switch(
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
pub(crate) fn remember(mode: PreviewMode, cx: &mut App) {
    AppState::update_state(cx, |state, _| state.preview = mode);
}

#[cfg(test)]
mod tests;
