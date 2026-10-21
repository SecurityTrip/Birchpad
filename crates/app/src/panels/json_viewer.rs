//! JSON Viewer, as Notepad++'s JSON Viewer plugin shows it (Plugins > JSON Viewer > Show JSON
//! Viewer): the active document's JSON as a tree of keys and values. A click on a value selects
//! it in the text; the field above finds a value by its path (`$.store.book[0].title`); Copy
//! Value, Copy Key and Copy Path copy what the selected value is. The tree is read again when
//! the document changes; folded values stay folded by their paths.

use std::collections::HashSet;
use std::ops::Range;

use anyhow::{Result, anyhow};
use birchpad_core::RevisionId;
use birchpad_tools::json_tree::{JsonKey, JsonKind, JsonTree};
use gpui_kit::component::Sizable;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::{
    App, AppContext as _, ClipboardItem, Context, Entity, EntityId, FocusHandle, Focusable,
    Subscription, UniformListScrollHandle, WeakEntity, Window, div, prelude::*, px, uniform_list,
};

use crate::editor::EditorView;
use crate::workspace::{Workspace, report_error};

const ROW_HEIGHT: f32 = 22.;
const INDENT: f32 = 14.;
/// The longest value a row shows, in characters.
const PREVIEW: usize = 80;

/// A row of the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub node: usize,
    pub depth: usize,
    pub has_children: bool,
    pub collapsed: bool,
}

/// The rows to show: each value under its parent, nothing under a folded one (by path).
pub fn rows(tree: &JsonTree, collapsed: &HashSet<String>) -> Vec<Row> {
    let mut rows = Vec::new();
    let mut stack = vec![0];
    while let Some(index) = stack.pop() {
        let node = &tree.nodes[index];
        let folded = !node.children.is_empty() && collapsed.contains(&tree.path(index));
        rows.push(Row {
            node: index,
            depth: node.depth,
            has_children: !node.children.is_empty(),
            collapsed: folded,
        });
        if !folded {
            stack.extend(node.children.iter().rev());
        }
    }
    rows
}

/// How a row shows a value: its key or index, and the value (`{3}` and `[2]` for objects and
/// arrays, the value as written for others, cut short).
pub fn label(tree: &JsonTree, index: usize, text: &str) -> String {
    let node = &tree.nodes[index];
    let key = match &node.key {
        Some(JsonKey::Name(name)) => format!("{name}: "),
        Some(JsonKey::Index(index)) => format!("[{index}]: "),
        None => String::new(),
    };
    let value = match node.kind {
        JsonKind::Object => format!("{{{}}}", node.children.len()),
        JsonKind::Array => format!("[{}]", node.children.len()),
        _ => {
            let raw = &text[node.range.clone()];
            if raw.chars().count() > PREVIEW {
                format!("{}…", raw.chars().take(PREVIEW).collect::<String>())
            } else {
                raw.to_owned()
            }
        }
    };
    format!("{key}{value}")
}

/// The innermost value whose text holds `pos`.
pub fn node_at(tree: &JsonTree, pos: usize) -> Option<usize> {
    tree.nodes
        .iter()
        .rposition(|node| node.range.start <= pos && pos <= node.range.end)
}

/// The tree of one state of one document.
pub struct Parsed {
    pub buffer: EntityId,
    pub revision: RevisionId,
    pub text: String,
    pub tree: Result<JsonTree, birchpad_tools::SyntaxError>,
}

/// The tree of `view`'s document, read again if `cached` is of another state.
pub fn parse(view: &Entity<EditorView>, cached: Option<Parsed>, cx: &App) -> Parsed {
    let buffer = view.read(cx).buffer.read(cx);
    let id = view.read(cx).buffer.entity_id();
    let revision = buffer.doc().revision();
    if let Some(cached) = cached
        && cached.buffer == id
        && cached.revision == revision
    {
        return cached;
    }
    let text = buffer.doc().text().to_string();
    let tree = JsonTree::parse(&text);
    Parsed {
        buffer: id,
        revision,
        text,
        tree,
    }
}

pub struct JsonViewer {
    workspace: WeakEntity<Workspace>,
    pub path: Entity<InputState>,
    pub parsed: Option<Parsed>,
    /// Folded values, by their paths.
    pub collapsed: HashSet<String>,
    /// The value chosen, by its path.
    pub selected: Option<String>,
    follow: super::FollowView,
    scroll: UniformListScrollHandle,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl Focusable for JsonViewer {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl JsonViewer {
    pub fn new(
        workspace: WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let path =
            cx.new(|cx| InputState::new(window, cx).placeholder(t!("panels.json-viewer.path")));
        let enter = cx.subscribe_in(
            &path,
            window,
            |this, path, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    let typed = path.read(cx).value().to_string();
                    if let Err(error) = this.find(&typed, window, cx) {
                        report_error(&error, window, cx);
                    }
                }
            },
        );
        Self {
            workspace,
            path,
            parsed: None,
            collapsed: HashSet::new(),
            selected: None,
            follow: super::FollowView::default(),
            scroll: UniformListScrollHandle::new(),
            focus_handle: cx.focus_handle(),
            _subscriptions: vec![enter],
        }
    }

    fn active_view(&self, cx: &App) -> Option<Entity<EditorView>> {
        self.workspace
            .upgrade()
            .and_then(|workspace| workspace.read(cx).active_view(cx))
    }

    /// Reads the active document again if it changed.
    pub fn refresh(&mut self, cx: &App) {
        self.parsed = self
            .active_view(cx)
            .map(|view| parse(&view, self.parsed.take(), cx));
    }

    /// The tree and the text it was read from.
    fn tree(&self) -> Option<(&JsonTree, &str)> {
        let parsed = self.parsed.as_ref()?;
        Some((parsed.tree.as_ref().ok()?, parsed.text.as_str()))
    }

    /// The value chosen: the one clicked or found, or the one at the caret.
    pub fn target(&mut self, cx: &App) -> Option<usize> {
        self.refresh(cx);
        let caret = self
            .active_view(cx)
            .map(|view| view.read(cx).selection.primary().head);
        let (tree, _) = self.tree()?;
        self.selected
            .as_ref()
            .and_then(|path| tree.find(path).ok())
            .or_else(|| caret.and_then(|caret| node_at(tree, caret)))
    }

    /// Selects value `index` in the tree and in the text.
    pub fn choose(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some((tree, _)) = self.tree() else {
            return;
        };
        let range = tree.nodes[index].range.clone();
        // Its parents unfold, so that it shows.
        let mut unfold = Vec::new();
        let mut parent = tree.nodes[index].parent;
        while let Some(at) = parent {
            unfold.push(tree.path(at));
            parent = tree.nodes[at].parent;
        }
        let path = tree.path(index);
        for path in unfold {
            self.collapsed.remove(&path);
        }
        self.selected = Some(path);
        if let Some(view) = self.active_view(cx) {
            select_in(&view, range, window, cx);
        }
        cx.notify();
    }

    /// Finds the value `path` names and chooses it.
    pub fn find(&mut self, path: &str, window: &mut Window, cx: &mut Context<Self>) -> Result<()> {
        self.refresh(cx);
        let found = match &self.parsed {
            None => return Err(anyhow!("{}", t!("panels.json-viewer.no-document"))),
            Some(Parsed {
                tree: Err(error),
                text,
                ..
            }) => {
                let (line, column) = error.line_column(text);
                return Err(anyhow!(
                    "{}",
                    t!(
                        "json-xml.line-column",
                        line = line,
                        column = column,
                        error = error.message
                    )
                ));
            }
            Some(Parsed { tree: Ok(tree), .. }) => tree.find(path)?,
        };
        self.choose(found, window, cx);
        Ok(())
    }

    /// What Copy Value, Copy Key or Copy Path copies of the chosen value.
    pub fn copied(&mut self, what: Copy, cx: &App) -> Result<String> {
        let target = self
            .target(cx)
            .ok_or_else(|| anyhow!("{}", t!("panels.json-viewer.nothing-chosen")))?;
        let (tree, text) = self
            .tree()
            .ok_or_else(|| anyhow!("{}", t!("panels.json-viewer.nothing-chosen")))?;
        Ok(match what {
            Copy::Value => tree.value(target, text).into_owned(),
            Copy::Path => tree.path(target),
            Copy::Key => match &tree.nodes[target].key {
                Some(JsonKey::Name(name)) => name.clone(),
                Some(JsonKey::Index(index)) => index.to_string(),
                None => String::new(),
            },
        })
    }

    pub fn toggle(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some((tree, _)) = self.tree() else {
            return;
        };
        let path = tree.path(index);
        if !self.collapsed.remove(&path) {
            self.collapsed.insert(path);
        }
        cx.notify();
    }

    fn render_row(
        &self,
        index: usize,
        row: &Row,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let (tree, text) = self.tree().expect("rows come from the tree");
        let target = row.node;
        let chosen = self.selected.as_deref() == Some(tree.path(target).as_str());
        let fold = row.has_children.then(|| {
            div()
                .id(("json-viewer-fold", index))
                .debug_selector(move || format!("json-viewer-fold-{index}"))
                .w(px(INDENT))
                .flex_none()
                .cursor_pointer()
                .child(if row.collapsed { "▸" } else { "▾" })
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.toggle(target, cx);
                }))
        });
        div()
            .id(("json-viewer-row", index))
            .w_full()
            .debug_selector(move || format!("json-viewer-{index}"))
            .h(px(ROW_HEIGHT))
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .pl(px(4. + row.depth as f32 * INDENT))
            .whitespace_nowrap()
            .overflow_hidden()
            .cursor_pointer()
            .when(chosen, |row| {
                row.bg(crate::theme::paint(crate::theme::ui().selected))
            })
            .hover(|row| row.bg(crate::theme::paint(crate::theme::ui().hovered)))
            .child(match fold {
                Some(fold) => fold.into_any_element(),
                None => div().w(px(INDENT)).flex_none().into_any_element(),
            })
            .child(label(tree, target, text))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.choose(target, window, cx);
            }))
    }
}

/// What a Copy button copies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Copy {
    Value,
    Key,
    Path,
}

/// Selects `range` in `view` and shows it.
fn select_in(view: &Entity<EditorView>, range: Range<usize>, window: &mut Window, cx: &mut App) {
    view.update(cx, |view, cx| {
        let len = view.text(cx).len();
        view.select_range(range.start.min(len)..range.end.min(len), cx);
        cx.notify();
    });
    let _ = window;
}

impl Render for JsonViewer {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let active = self.active_view(cx);
        self.follow.follow(active.as_ref(), cx);
        self.refresh(cx);
        let copy = |id: &'static str, label: String, what: Copy| {
            Button::new(id)
                .debug_selector(move || id.into())
                .small()
                .ghost()
                .label(label)
                .on_click(
                    cx.listener(move |this, _, window, cx| match this.copied(what, cx) {
                        Ok(text) => cx.write_to_clipboard(ClipboardItem::new_string(text)),
                        Err(error) => report_error(&error, window, cx),
                    }),
                )
        };
        let toolbar = div()
            .flex()
            .flex_col()
            .gap_1()
            .p_1()
            .flex_none()
            .child(Input::new(&self.path).id("json-viewer-path").small())
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap_1()
                    .child(copy(
                        "json-viewer-copy-value",
                        t!("panels.json-viewer.copy-value"),
                        Copy::Value,
                    ))
                    .child(copy(
                        "json-viewer-copy-key",
                        t!("panels.json-viewer.copy-key"),
                        Copy::Key,
                    ))
                    .child(copy(
                        "json-viewer-copy-path",
                        t!("panels.json-viewer.copy-path"),
                        Copy::Path,
                    )),
            );
        let message = |text: String| {
            div()
                .p_2()
                .text_color(crate::theme::paint(crate::theme::ui().muted))
                .child(text)
                .into_any_element()
        };
        let body = match &self.parsed {
            None => message(t!("panels.json-viewer.no-document")),
            Some(Parsed {
                tree: Err(error),
                text,
                ..
            }) => {
                let (line, column) = error.line_column(text);
                message(t!(
                    "json-xml.line-column",
                    line = line,
                    column = column,
                    error = error.message
                ))
            }
            Some(Parsed { tree: Ok(tree), .. }) => {
                let rows = std::rc::Rc::new(rows(tree, &self.collapsed));
                let list_rows = rows.clone();
                uniform_list(
                    "json-viewer-rows",
                    rows.len(),
                    cx.processor(move |this, range: Range<usize>, _, cx| {
                        range
                            .map(|index| this.render_row(index, &list_rows[index], cx))
                            .collect::<Vec<_>>()
                    }),
                )
                .track_scroll(&self.scroll)
                .size_full()
                .into_any_element()
            }
        };
        div()
            .id("json-viewer")
            .track_focus(&self.focus_handle)
            .size_full()
            .flex()
            .flex_col()
            .child(toolbar)
            .child(div().flex_1().min_h(px(0.)).child(body))
    }
}
