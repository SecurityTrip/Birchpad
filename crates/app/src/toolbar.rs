//! The toolbar under the menus, as Notepad++'s: a button for each command of
//! `toolbar.buttons`, with its title as tooltip, pressed while its switch is on (word wrap, a
//! panel, synchronized scrolling...) and disabled when the command is. Small or large icons
//! (`toolbar.size`); hidden with `toolbar.show = false`. The icons are Lucide's (ISC license,
//! `assets/toolbar/LICENSE`); a command without one shows its title.

use birchpad_commands::Invocation;
use birchpad_config::{ToolbarSettings, ToolbarSize};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{Disableable as _, Icon, Selectable as _, Sizable as _, Size};
use gpui_kit::{AnyElement, Context, div, prelude::*, px};

use crate::commands::CommandRegistry;
use crate::theme::{paint, ui};
use crate::workspace::Workspace;

/// The icon of each command that has one.
const ICONS: &[(&str, &[u8])] = &[
    (
        "file.new",
        include_bytes!("../assets/toolbar/file-plus.svg"),
    ),
    (
        "file.open",
        include_bytes!("../assets/toolbar/folder-open.svg"),
    ),
    ("file.save", include_bytes!("../assets/toolbar/save.svg")),
    (
        "file.save-all",
        include_bytes!("../assets/toolbar/save-all.svg"),
    ),
    ("file.close", include_bytes!("../assets/toolbar/file-x.svg")),
    (
        "file.close-all",
        include_bytes!("../assets/toolbar/copy-x.svg"),
    ),
    (
        "file.print",
        include_bytes!("../assets/toolbar/printer.svg"),
    ),
    ("edit.cut", include_bytes!("../assets/toolbar/scissors.svg")),
    ("edit.copy", include_bytes!("../assets/toolbar/copy.svg")),
    (
        "edit.paste",
        include_bytes!("../assets/toolbar/clipboard-paste.svg"),
    ),
    ("edit.undo", include_bytes!("../assets/toolbar/undo-2.svg")),
    ("edit.redo", include_bytes!("../assets/toolbar/redo-2.svg")),
    (
        "search.find",
        include_bytes!("../assets/toolbar/search.svg"),
    ),
    (
        "search.replace",
        include_bytes!("../assets/toolbar/replace.svg"),
    ),
    (
        "view.zoom-in",
        include_bytes!("../assets/toolbar/zoom-in.svg"),
    ),
    (
        "view.zoom-out",
        include_bytes!("../assets/toolbar/zoom-out.svg"),
    ),
    (
        "view.sync-vertical-scroll",
        include_bytes!("../assets/toolbar/arrow-down-up.svg"),
    ),
    (
        "view.sync-horizontal-scroll",
        include_bytes!("../assets/toolbar/arrow-left-right.svg"),
    ),
    (
        "view.word-wrap",
        include_bytes!("../assets/toolbar/text-wrap.svg"),
    ),
    (
        "view.show-all-characters",
        include_bytes!("../assets/toolbar/pilcrow.svg"),
    ),
    (
        "view.indent-guides",
        include_bytes!("../assets/toolbar/list-indent-increase.svg"),
    ),
    (
        "panel.document-map",
        include_bytes!("../assets/toolbar/map.svg"),
    ),
    (
        "panel.document-list",
        include_bytes!("../assets/toolbar/list.svg"),
    ),
    (
        "panel.function-list",
        include_bytes!("../assets/toolbar/list-tree.svg"),
    ),
    (
        "panel.folder-as-workspace",
        include_bytes!("../assets/toolbar/folder-tree.svg"),
    ),
    (
        "view.monitoring",
        include_bytes!("../assets/toolbar/scan-eye.svg"),
    ),
    (
        "macro.start-recording",
        include_bytes!("../assets/toolbar/circle-dot.svg"),
    ),
    (
        "macro.stop-recording",
        include_bytes!("../assets/toolbar/square.svg"),
    ),
    (
        "macro.playback",
        include_bytes!("../assets/toolbar/play.svg"),
    ),
    (
        "macro.run-multiple",
        include_bytes!("../assets/toolbar/repeat.svg"),
    ),
    ("macro.save", include_bytes!("../assets/toolbar/disc.svg")),
];

/// The icon of `command`, if it has one.
pub(crate) fn icon(command: &str) -> Option<&'static [u8]> {
    ICONS
        .iter()
        .find(|(id, _)| *id == command)
        .map(|(_, svg)| *svg)
}

/// What the toolbar shows: a button for a command, or a separator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Item {
    Button(String),
    Separator,
}

/// The items of `settings.buttons` that can show: commands the catalog knows, separators not
/// at either end nor two in a row.
pub(crate) fn items(settings: &ToolbarSettings) -> Vec<Item> {
    let mut items: Vec<Item> = Vec::new();
    for entry in &settings.buttons {
        let entry = entry.trim();
        if entry == "|" {
            if matches!(items.last(), Some(Item::Button(_))) {
                items.push(Item::Separator);
            }
        } else if birchpad_commands::find(entry).is_some() {
            items.push(Item::Button(entry.to_owned()));
        }
    }
    if items.last() == Some(&Item::Separator) {
        items.pop();
    }
    items
}

impl Workspace {
    /// The toolbar, or nothing when it is hidden or has no buttons.
    pub(crate) fn render_toolbar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let settings = crate::app_state::AppState::global(cx)
            .settings
            .toolbar
            .clone();
        if !settings.show {
            return None;
        }
        let items = items(&settings);
        if items.is_empty() {
            return None;
        }
        let large = settings.size == ToolbarSize::Large;
        let registry = cx.global::<CommandRegistry>();
        let enabled: Vec<bool> = items
            .iter()
            .map(|item| match item {
                Item::Button(id) => registry.is_enabled(id, cx),
                Item::Separator => true,
            })
            .collect();
        let buttons = items
            .into_iter()
            .zip(enabled)
            .enumerate()
            .map(|(index, (item, enabled))| match item {
                Item::Separator => div()
                    .mx_1()
                    .w(px(1.))
                    .h(px(if large { 24. } else { 16. }))
                    .bg(paint(ui().border))
                    .into_any_element(),
                Item::Button(id) => {
                    let title = crate::menus::command_title(&id);
                    let pressed = self.toolbar_checked.contains(&id);
                    let selector = format!("toolbar-{id}");
                    let button = Button::new(("toolbar", index))
                        .ghost()
                        .with_size(if large { Size::Medium } else { Size::Small })
                        .selected(pressed)
                        .disabled(!enabled)
                        .tooltip(title.clone())
                        .debug_selector(move || selector.clone());
                    let button = match icon(&id) {
                        Some(svg) => button.icon(Icon::default().data(svg)),
                        None => button.label(title),
                    };
                    button
                        .on_click(cx.listener(move |this, _, window, cx| {
                            let invocation = Invocation::new(&id);
                            if let Err(error) = this.dispatch(&invocation, window, cx) {
                                crate::workspace::report_error(&error, window, cx);
                            }
                        }))
                        .into_any_element()
                }
            });
        Some(
            div()
                .id("toolbar")
                .debug_selector(|| "toolbar".into())
                .flex()
                .flex_none()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap_0p5()
                .px_1()
                .py_0p5()
                .border_b_1()
                .border_color(paint(ui().border))
                .bg(paint(ui().surface))
                .children(buttons)
                .into_any_element(),
        )
    }
}

#[cfg(test)]
#[path = "../tests/unit/toolbar/tests.rs"]
mod tests;
