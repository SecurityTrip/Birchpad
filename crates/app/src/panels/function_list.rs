//! Function List, as Notepad++'s (View > Function List): the classes, functions, methods and
//! sections of the active document, as a tree.
//!
//! The definitions come from the document's syntax tree (`birchpad_syntax::outline`) and are
//! read again whenever the tree changes. A click goes to a definition and selects its name; the
//! definition the caret is in is highlighted. The field above the tree filters it by name
//! (keeping the parents of what matches), the sort button orders each level by name instead of
//! by position, and the arrows fold a level.

use std::collections::HashSet;
use std::ops::Range;

use birchpad_syntax::{Symbol, SymbolKind};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{Selectable as _, Sizable};
use gpui_kit::{
    App, AppContext as _, Context, Entity, EntityId, FocusHandle, Focusable, Subscription,
    UniformListScrollHandle, WeakEntity, Window, div, prelude::*, px, rgb, uniform_list,
};

use crate::editor::EditorView;
use crate::workspace::Workspace;

const ROW_HEIGHT: f32 = 22.;
const INDENT: f32 = 14.;

/// A row of the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Row {
    /// The symbol's index in the outline.
    pub(crate) symbol: usize,
    pub(crate) depth: usize,
    pub(crate) has_children: bool,
    pub(crate) collapsed: bool,
}

/// The key of a symbol that stays the same while the document changes: the names on its way
/// down from the top.
fn path_of(symbols: &[Symbol], parents: &[Option<usize>], mut index: usize) -> String {
    let mut names = vec![symbols[index].name.as_str()];
    while let Some(parent) = parents[index] {
        names.push(symbols[parent].name.as_str());
        index = parent;
    }
    names.reverse();
    names.join("\u{1f}")
}

/// The parent of each symbol: the nearest one before it with a smaller depth.
fn parents(symbols: &[Symbol]) -> Vec<Option<usize>> {
    let mut parents = Vec::with_capacity(symbols.len());
    let mut open: Vec<usize> = Vec::new();
    for (index, symbol) in symbols.iter().enumerate() {
        open.truncate(symbol.depth);
        parents.push(open.last().copied());
        open.push(index);
    }
    parents
}

/// The rows to show of `symbols` (in text order, with depths): children under their parent,
/// each level by name if `sorted`, nothing under a collapsed symbol, and with a `filter` only
/// the symbols whose names contain it (without regard to case) and their parents.
pub(crate) fn rows(
    symbols: &[Symbol],
    sorted: bool,
    collapsed: &HashSet<String>,
    filter: &str,
) -> Vec<Row> {
    let parents = parents(symbols);
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); symbols.len()];
    let mut top = Vec::new();
    for (index, parent) in parents.iter().enumerate() {
        match parent {
            Some(parent) => children[*parent].push(index),
            None => top.push(index),
        }
    }
    let filter = filter.trim().to_lowercase();
    let mut shown = vec![filter.is_empty(); symbols.len()];
    if !filter.is_empty() {
        for (index, symbol) in symbols.iter().enumerate() {
            if symbol.name.to_lowercase().contains(&filter) {
                let mut at = Some(index);
                while let Some(index) = at {
                    shown[index] = true;
                    at = parents[index];
                }
            }
        }
    }
    let order = |list: &mut Vec<usize>| {
        if sorted {
            list.sort_by(|&a, &b| {
                symbols[a]
                    .name
                    .to_lowercase()
                    .cmp(&symbols[b].name.to_lowercase())
                    .then(a.cmp(&b))
            });
        }
    };
    order(&mut top);
    let mut rows = Vec::new();
    let mut stack: Vec<usize> = top.into_iter().rev().collect();
    while let Some(index) = stack.pop() {
        if !shown[index] {
            continue;
        }
        let mut kids: Vec<usize> = children[index]
            .iter()
            .copied()
            .filter(|&kid| shown[kid])
            .collect();
        // A filter shows what matches inside folded symbols too.
        let is_collapsed = filter.is_empty()
            && !kids.is_empty()
            && collapsed.contains(&path_of(symbols, &parents, index));
        rows.push(Row {
            symbol: index,
            depth: symbols[index].depth,
            has_children: !kids.is_empty(),
            collapsed: is_collapsed,
        });
        if !is_collapsed {
            order(&mut kids);
            stack.extend(kids.into_iter().rev());
        }
    }
    rows
}

/// The innermost symbol the caret at `pos` is in.
pub(crate) fn current_symbol(symbols: &[Symbol], pos: usize) -> Option<usize> {
    symbols
        .iter()
        .rposition(|symbol| symbol.range.start <= pos && pos < symbol.range.end)
}

fn kind_mark(kind: SymbolKind) -> (&'static str, u32) {
    match kind {
        SymbolKind::Class => ("C", 0x8250df),
        SymbolKind::Interface => ("I", 0x1a7f37),
        SymbolKind::Module => ("N", 0x57606a),
        SymbolKind::Function => ("f", 0x0969da),
        SymbolKind::Method => ("m", 0x0969da),
        SymbolKind::Macro => ("!", 0xbc4c00),
        SymbolKind::Type => ("T", 0x8250df),
        SymbolKind::Section => ("§", 0x57606a),
    }
}

/// The outline of one state of one document.
struct Outline {
    buffer: EntityId,
    version: u64,
    symbols: Vec<Symbol>,
}

pub(crate) struct FunctionList {
    workspace: WeakEntity<Workspace>,
    filter: Entity<InputState>,
    pub(crate) sorted: bool,
    /// Folded symbols, by their paths.
    collapsed: HashSet<String>,
    outline: Option<Outline>,
    follow: super::FollowView,
    scroll: UniformListScrollHandle,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl Focusable for FunctionList {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

/// What the panel shows for the active document.
pub(crate) enum Contents {
    /// No document, a language without a function list, or no tree (yet, or ever: plain text,
    /// a file over the large file limit).
    Message(String),
    Tree {
        view: Entity<EditorView>,
        rows: Vec<Row>,
        current: Option<usize>,
    },
}

impl FunctionList {
    pub(crate) fn new(
        workspace: WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Search"));
        let changes = cx.subscribe(&filter, |_, _, event: &InputEvent, cx| {
            if let InputEvent::Change = event {
                cx.notify();
            }
        });
        Self {
            workspace,
            filter,
            sorted: false,
            collapsed: HashSet::new(),
            outline: None,
            follow: super::FollowView::default(),
            scroll: UniformListScrollHandle::new(),
            focus_handle: cx.focus_handle(),
            _subscriptions: vec![changes],
        }
    }

    #[cfg(test)]
    pub(crate) fn set_filter(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.filter
            .update(cx, |input, cx| input.set_value(text, window, cx));
        cx.notify();
    }

    /// The outline of the active document, read again if its tree changed.
    pub(crate) fn contents(&mut self, cx: &App) -> Contents {
        let Some(view) = self
            .workspace
            .upgrade()
            .and_then(|workspace| workspace.read(cx).active_view(cx))
        else {
            return Contents::Message("No document".into());
        };
        let buffer = view.read(cx).buffer.read(cx);
        let Some(language) = buffer.language() else {
            return Contents::Message("Plain text has no function list".into());
        };
        if !birchpad_syntax::has_outline(language) {
            return Contents::Message(format!("No function list for {}", language.name));
        }
        let Some(syntax) = buffer.syntax() else {
            return Contents::Message("The file is too large for a function list".into());
        };
        let buffer_id = view.read(cx).buffer.entity_id();
        let fresh = self.outline.as_ref().is_some_and(|outline| {
            outline.buffer == buffer_id && outline.version == syntax.version()
        });
        if !fresh {
            self.outline = Some(Outline {
                buffer: buffer_id,
                version: syntax.version(),
                symbols: birchpad_syntax::outline(syntax, buffer.doc().text()),
            });
        }
        let symbols = &self.outline.as_ref().expect("just set").symbols;
        let filter = self.filter.read(cx).value().to_string();
        let caret = view.read(cx).selection.primary().head;
        Contents::Tree {
            rows: rows(symbols, self.sorted, &self.collapsed, &filter),
            current: current_symbol(symbols, caret),
            view,
        }
    }

    pub(crate) fn symbol(&self, index: usize) -> Option<&Symbol> {
        self.outline.as_ref()?.symbols.get(index)
    }

    /// Folds or unfolds the symbol at `index`.
    pub(crate) fn toggle(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(outline) = &self.outline else {
            return;
        };
        let parents = parents(&outline.symbols);
        let path = path_of(&outline.symbols, &parents, index);
        if !self.collapsed.remove(&path) {
            self.collapsed.insert(path);
        }
        cx.notify();
    }

    /// Goes to the symbol at `index` in `view`: its name is selected.
    pub(crate) fn go_to(
        &mut self,
        view: &Entity<EditorView>,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(name) = self.symbol(index).map(|symbol| symbol.name_range.clone()) else {
            return;
        };
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };
        workspace.update(cx, |workspace, cx| {
            workspace.activate_view(view, window, cx);
        });
        view.update(cx, |view, cx| {
            let len = view.text(cx).len();
            view.select_range(name.start.min(len)..name.end.min(len), cx);
        });
        let focus = view.read(cx).focus_handle.clone();
        window.focus(&focus, cx);
    }

    fn render_row(
        &self,
        index: usize,
        row: &Row,
        current: bool,
        view: &Entity<EditorView>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let symbol = &self.outline.as_ref().expect("rows come from it").symbols[row.symbol];
        let (mark, color) = kind_mark(symbol.kind);
        let target = row.symbol;
        let view = view.clone();
        let fold = row.has_children.then(|| {
            div()
                .id(("function-list-fold", index))
                .debug_selector(move || format!("function-list-fold-{index}"))
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
            .id(("function-list-row", index))
            .w_full()
            .debug_selector(move || format!("function-list-{index}"))
            .h(px(ROW_HEIGHT))
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .pl(px(4. + row.depth as f32 * INDENT))
            .whitespace_nowrap()
            .overflow_hidden()
            .cursor_pointer()
            .when(current, |row| {
                row.bg(crate::theme::paint(crate::theme::ui().selected))
            })
            .hover(|row| row.bg(crate::theme::paint(crate::theme::ui().hovered)))
            .child(match fold {
                Some(fold) => fold.into_any_element(),
                None => div().w(px(INDENT)).flex_none().into_any_element(),
            })
            .child(
                div()
                    .w(px(12.))
                    .flex_none()
                    .text_color(rgb(color))
                    .child(mark),
            )
            .child(symbol.name.clone())
            .on_click(cx.listener(move |this, _, window, cx| {
                this.go_to(&view, target, window, cx);
            }))
    }
}

impl Render for FunctionList {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let active = self
            .workspace
            .upgrade()
            .and_then(|workspace| workspace.read(cx).active_view(cx));
        self.follow.follow(active.as_ref(), cx);
        let toolbar = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .p_1()
            .flex_none()
            .child(
                Input::new(&self.filter)
                    .id("function-list-filter")
                    .small()
                    .flex_1(),
            )
            .child(
                Button::new("function-list-sort")
                    .small()
                    .ghost()
                    .label("A-Z")
                    .selected(self.sorted)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.sorted ^= true;
                        cx.notify();
                    })),
            );
        let body = match self.contents(cx) {
            Contents::Message(message) => div()
                .p_2()
                .text_color(crate::theme::paint(crate::theme::ui().muted))
                .child(message)
                .into_any_element(),
            Contents::Tree {
                view,
                rows,
                current,
            } => {
                let rows = std::rc::Rc::new(rows);
                let list_rows = rows.clone();
                uniform_list(
                    "function-list-rows",
                    rows.len(),
                    cx.processor(move |this, range: Range<usize>, _, cx| {
                        range
                            .map(|index| {
                                let row = &list_rows[index];
                                let current = Some(row.symbol) == current;
                                this.render_row(index, row, current, &view, cx)
                            })
                            .collect::<Vec<_>>()
                    }),
                )
                .track_scroll(&self.scroll)
                .size_full()
                .into_any_element()
            }
        };
        div()
            .id("function-list")
            .track_focus(&self.focus_handle)
            .size_full()
            .flex()
            .flex_col()
            .child(toolbar)
            .child(div().flex_1().min_h(px(0.)).child(body))
    }
}

#[cfg(test)]
#[path = "function_list_tests.rs"]
mod tests;
