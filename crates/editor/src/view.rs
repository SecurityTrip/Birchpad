//! The editor view: one view of a buffer, with its own selection, scroll position and layout.

mod carets;
mod commands;
mod editing;
mod keyboard;
mod mouse;
mod settings;

pub use commands::{EditorCommands, register_commands};
pub use keyboard::without_control_characters;
pub use settings::{VIEW_SWITCHES, ViewSettings, ViewSwitch, all_characters};

use std::ops::Range as ByteRange;
use std::time::Duration;

use birchpad_cli::CaretTarget;
use birchpad_core::motion::{self, line_of, line_range};
use birchpad_core::{LineMarkers, Range, RevisionId, Rope, Selection};
use birchpad_ui::{AppState, CommandRegistry, RunCommand};
use birchpad_view::{BlockPoint, DisplayMap, LayoutConfig};
use gpui_kit::component::menu::ContextMenuExt as _;
use gpui_kit::{
    App, Context, Entity, EventEmitter, FocusHandle, Focusable, MouseButton, Pixels, Subscription,
    Task, Window, div, prelude::*, px,
};

use crate::buffer::{Buffer, BufferEvent, ReadOnly};
use crate::element::EditorElement;
use crate::layout::{self, Layout};
use crate::{completion, folding, multi, smart_highlight, theme};

/// Font size at zoom level 0, in pixels.
pub(crate) const BASE_FONT_SIZE: f32 = 14.;

/// Which kind of edit was made last, to group consecutive typing into one undo step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LastEdit {
    None,
    Typing,
    Deleting,
}

/// What a mouse drag extends by: characters, words (after a double click) or lines (triple).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Granularity {
    Char,
    Word,
    Line,
}

#[derive(Debug, Clone)]
pub(crate) enum Drag {
    /// Selecting text; `origin` is what the first click selected.
    Select {
        granularity: Granularity,
        origin: ByteRange<usize>,
    },
    /// Selecting a rectangle (Alt+drag) from `anchor`.
    Block {
        anchor: BlockPoint,
    },
    /// Dragging the selected text, pressed at `from`, now over `to`.
    Text {
        from: usize,
        to: usize,
    },
    /// Dragging a scrollbar thumb; `grab` is where in the thumb it was grabbed.
    VerticalThumb {
        grab: Pixels,
    },
    HorizontalThumb {
        grab: Pixels,
    },
}

/// A word wrap layout being computed in the background for a large document.
pub(crate) struct WrapJob {
    pub(crate) config: LayoutConfig,
    pub(crate) revision: RevisionId,
    pub(crate) _task: Task<()>,
}

/// What a view tells the workspace.
pub enum EditorEvent {
    /// The view scrolled by this many rows and pixels since the last frame (for synchronized
    /// scrolling).
    Scrolled { rows: f64, x: Pixels },
    /// An item of the context menu was chosen: the command runs on this view's document.
    Command(birchpad_commands::Invocation),
}

/// Where a view is in its buffer: what Clone to Other View copies and sessions save.
#[derive(Debug, Clone, PartialEq)]
pub struct ViewState {
    pub selection: Selection,
    pub scroll_top: f64,
    pub scroll_left: Pixels,
    /// First lines of the collapsed folds.
    pub collapsed: Vec<usize>,
    /// Edit > Text Direction RTL, remembered per tab in sessions.
    pub rtl: bool,
}

pub struct EditorView {
    pub focus_handle: FocusHandle,
    pub buffer: Entity<Buffer>,
    pub selection: Selection,
    /// Text being composed by an input method, as a byte range.
    pub marked: Option<ByteRange<usize>>,
    pub overwrite: bool,
    /// Edit > Text Direction RTL: rows run right to left and end at the right edge.
    pub rtl: bool,
    /// Its own zoom level, while View > Zoom > Synchronize Across Views is off.
    pub zoom: Option<i32>,
    /// Where the mouse is over the text, for the link it frames.
    pub hovered: Option<usize>,
    /// The completion list, while it shows.
    pub completion: Option<completion::Popup>,
    /// A function's parameters, while they show.
    pub call_tip: Option<completion::CallTip>,
    pub display: DisplayMap,
    pub(crate) wrap_job: Option<WrapJob>,
    /// Where to put the caret once the file has been read (`-n`, `-c`, `-p`).
    pending_caret: Option<CaretTarget>,
    /// A search result to select once the file has been read: a line, and the match's bytes
    /// from its start.
    pending_match: Option<(usize, ByteRange<usize>)>,
    /// Where to put the view once the file has been read (a restored session).
    pending_state: Option<ViewState>,
    /// Whether its tab is pinned, and its color.
    pub tab: crate::TabMarks,
    /// First visible row; fractional while scrolling smoothly.
    pub scroll_top: f64,
    /// Horizontal scroll offset of the text.
    pub scroll_left: Pixels,
    /// Widest content seen, for the horizontal scrollbar (as Scintilla tracks it).
    pub(crate) scroll_width: Pixels,
    /// Bring the primary caret into view on the next layout.
    pub autoscroll: bool,
    /// Column (in cells from the row start) that vertical movement aims for.
    pub(crate) goal_column: Option<usize>,
    pub(crate) last_edit: LastEdit,
    drag: Option<Drag>,
    pub(crate) caret_visible: bool,
    /// Whether the view has keyboard focus; the caret only blinks then.
    pub(crate) focused: bool,
    blink: Task<()>,
    /// Geometry of the last frame, for mouse and IME hit testing.
    pub layout: Option<Layout>,
    pub(crate) highlight_cache: Option<layout::HighlightCache>,
    /// Occurrences of the selected word around the visible text.
    pub smart_highlight: Option<smart_highlight::SmartHighlight>,
    /// Headers of this view's collapsed folds.
    pub(crate) collapsed: LineMarkers,
    /// View > Hide Lines: the lines hidden, each marked; shown again from the line above them.
    pub user_hidden: LineMarkers,
    /// The buffer's folds in lines.
    pub fold_cache: Option<folding::FoldCache>,
    /// The rectangular selection, valid while `selection` is still the one it made.
    pub(crate) block: Option<multi::BlockState>,
    /// The order in which Multi-select added ranges, valid while `selection` is still the one
    /// they make.
    pub(crate) multi_order: Option<(Vec<Range>, Selection)>,
    /// The selections Expand Selection grew from, the last one last, valid while `selection`
    /// is still the one it made.
    pub(crate) expansions: Option<(Vec<Selection>, Selection)>,
    /// Highlight Matching Tags of the last frame: for which text, caret and comments setting,
    /// and the tags found.
    pub(crate) tag_match: Option<(
        birchpad_core::RevisionId,
        usize,
        bool,
        Option<birchpad_core::ops::TagPair>,
    )>,
    /// Begin/End Select: where the first invocation put the start, and in which mode.
    pub(crate) begin_select: Option<multi::BeginSelect>,
    /// The scroll position of the last frame, to report scrolling.
    last_scroll: Option<(f64, Pixels)>,
    /// The current scroll came from the other view: do not report it back.
    synced_scroll: bool,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<EditorEvent> for EditorView {}

impl EditorView {
    pub fn new(buffer: Entity<Buffer>, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        let events = cx.subscribe(&buffer, |this, buffer, event, cx| match event {
            BufferEvent::Edited {
                transaction,
                origin,
            } => {
                if *origin != Some(cx.entity_id()) {
                    let text = buffer.read(cx).doc().text().clone();
                    this.display.edit(&text, transaction.changes());
                    this.collapsed.map(transaction.changes(), &text);
                    this.follow_folds(transaction.changes(), cx);
                    this.sync_hidden(cx);
                    this.selection = this
                        .selection
                        .map(transaction.changes())
                        .clip_virtual(&text);
                    this.map_begin_select(transaction.changes());
                    this.marked = None;
                }
                cx.notify();
            }
            BufferEvent::Reloaded {
                previous: Some(previous),
            } => {
                this.follow_reload(previous, cx);
            }
            BufferEvent::FollowEnd => {
                let end = buffer.read(cx).doc().text().len();
                this.selection = Selection::point(end);
                this.request_autoscroll(cx);
            }
            BufferEvent::Reloaded { previous: None } => {
                let text = buffer.read(cx).doc().text().clone();
                this.display.reset(&text);
                this.collapsed.clear();
                this.fold_cache = None;
                // A new document starts its revisions over.
                this.smart_highlight = None;
                this.sync_hidden(cx);
                this.selection = Selection::point(0);
                this.begin_select = None;
                this.marked = None;
                this.goal_column = None;
                this.last_edit = LastEdit::None;
                this.scroll_top = 0.;
                this.scroll_left = px(0.);
                this.scroll_width = px(0.);
                if let Some(state) = this.pending_state.take() {
                    this.restore(state, cx);
                }
                if let Some(target) = this.pending_caret.take() {
                    this.place_caret(target, cx);
                }
                if let Some((line, target)) = this.pending_match.take() {
                    this.select_in_line(line, target, cx);
                }
                cx.notify();
            }
            BufferEvent::SyntaxChanged => {
                // New folds from the reparse: drop collapsed headers that no longer fold.
                this.prune_collapsed(cx);
                cx.notify();
            }
            BufferEvent::StateChanged | BufferEvent::MarksChanged | BufferEvent::LoadFailed(_) => {
                cx.notify()
            }
        });
        let focus_handle = cx.focus_handle();
        let text = buffer.read(cx).doc().text().clone();
        let settings = ViewSettings::for_buffer(buffer.read(cx), cx);
        let display = DisplayMap::new(
            &text,
            LayoutConfig {
                tab_width: settings.tab_width,
                labels: settings.labels(),
                ..LayoutConfig::default()
            },
        );
        Self {
            focus_handle,
            buffer,
            selection: Selection::point(0),
            marked: None,
            overwrite: false,
            rtl: false,
            completion: None,
            call_tip: None,
            display,
            wrap_job: None,
            pending_caret: None,
            pending_match: None,
            pending_state: None,
            tab: crate::TabMarks::default(),
            scroll_top: 0.,
            scroll_left: px(0.),
            scroll_width: px(0.),
            autoscroll: false,
            goal_column: None,
            last_edit: LastEdit::None,
            drag: None,
            caret_visible: true,
            focused: false,
            blink: Task::ready(()),
            layout: None,
            highlight_cache: None,
            smart_highlight: None,
            collapsed: LineMarkers::new(),
            user_hidden: LineMarkers::new(),
            fold_cache: None,
            block: None,
            multi_order: None,
            expansions: None,
            tag_match: None,
            zoom: None,
            hovered: None,
            begin_select: None,
            last_scroll: None,
            synced_scroll: false,
            _subscriptions: vec![events],
        }
    }

    /// The file was read again: carets and selections stay at the same lines and columns,
    /// collapsed folds on the same lines, the scroll position as it was, as far as the new
    /// text has them.
    fn follow_reload(&mut self, previous: &Rope, cx: &mut Context<Self>) {
        let text = self.text(cx).clone();
        let last_line = motion::line_count(&text) - 1;
        let map = |pos: usize| {
            let line = line_of(previous, pos);
            let column = pos - line_range(previous, line).start;
            let range = line_range(&text, line.min(last_line));
            text.floor_char_boundary((range.start + column).min(range.end))
        };
        let ranges: Vec<Range> = self
            .selection
            .ranges()
            .iter()
            .map(|range| Range::new(map(range.anchor), map(range.head)))
            .collect();
        self.selection = Selection::new(ranges, self.selection.primary_index());
        let collapsed = self.collapsed.lines(previous);
        self.display.reset(&text);
        self.collapsed.set_lines(&text, collapsed);
        self.fold_cache = None;
        self.smart_highlight = None;
        self.sync_hidden(cx);
        self.block = None;
        self.multi_order = None;
        self.begin_select = None;
        self.marked = None;
        cx.notify();
    }

    /// Where the view is, or will be once its file has been read.
    pub fn view_state(&self, cx: &App) -> ViewState {
        if let Some(pending) = &self.pending_state {
            return pending.clone();
        }
        ViewState {
            selection: self.selection.clone(),
            scroll_top: self.scroll_top,
            scroll_left: self.scroll_left,
            collapsed: self.collapsed.lines(self.text(cx)),
            rtl: self.rtl,
        }
    }

    /// Puts the view where `state` says, now or, while the file is being read, once it has
    /// been. Positions past the end of the text (the file changed meanwhile) move to the end.
    pub fn restore(&mut self, state: ViewState, cx: &mut Context<Self>) {
        // The direction shows at once, the file read or not.
        self.rtl = state.rtl;
        if self.buffer.read(cx).read_only() == Some(ReadOnly::Loading) {
            self.pending_state = Some(state);
            return;
        }
        let text = self.text(cx).clone();
        let clamp = |pos: usize| {
            let pos = text.floor_char_boundary(pos.min(text.len()));
            // Not between the CR and the LF of a line break.
            if pos > 0 && text.get_byte(pos - 1) == Some(b'\r') && text.get_byte(pos) == Some(b'\n')
            {
                pos - 1
            } else {
                pos
            }
        };
        let ranges: Vec<Range> = state
            .selection
            .ranges()
            .iter()
            .map(|range| Range::new(clamp(range.anchor), clamp(range.head)))
            .collect();
        self.selection = Selection::new(ranges, state.selection.primary_index());
        self.scroll_top = state.scroll_top.max(0.);
        self.scroll_left = state.scroll_left.max(px(0.));
        self.collapsed.set_lines(&text, state.collapsed);
        self.autoscroll = false;
        cx.notify();
    }

    /// The last frame's layout.
    #[cfg(feature = "test-support")]
    pub fn layout(&self) -> Option<&Layout> {
        self.layout.as_ref()
    }

    /// Where the text was drawn in the last frame.
    #[cfg(feature = "test-support")]
    pub fn bounds(&self) -> Option<gpui_kit::Bounds<Pixels>> {
        self.layout.as_ref().map(|layout| layout.text_bounds)
    }

    /// The first visible row and the horizontal scroll offset.
    #[cfg(feature = "test-support")]
    pub fn scroll_position(&self) -> (f64, Pixels) {
        (self.scroll_top, self.scroll_left)
    }

    /// Scrolls along with the other view (synchronized scrolling).
    pub fn scroll_by(&mut self, rows: f64, x: Pixels, cx: &mut Context<Self>) {
        self.scroll_top = (self.scroll_top + rows).max(0.);
        self.scroll_left = (self.scroll_left + x).max(px(0.));
        self.synced_scroll = true;
        cx.notify();
    }

    /// Reports how far the view scrolled since the last frame, unless the scroll came from the
    /// other view.
    pub(crate) fn report_scroll(&mut self, cx: &mut Context<Self>) {
        let position = (self.scroll_top, self.scroll_left);
        let synced = std::mem::take(&mut self.synced_scroll);
        if let Some(last) = self.last_scroll.replace(position)
            && last != position
            && !synced
        {
            cx.emit(EditorEvent::Scrolled {
                rows: position.0 - last.0,
                x: position.1 - last.1,
            });
        }
    }

    pub fn text<'a>(&self, cx: &'a App) -> &'a Rope {
        self.buffer.read(cx).doc().text()
    }

    pub fn run_command(
        &mut self,
        action: &RunCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let invocation = &action.0;
        self.close_completion_for(&invocation.command);
        let handler = cx
            .global::<CommandRegistry>()
            .handler::<Self>(&invocation.command);
        match handler {
            Some(run) => match run(self, invocation, window, cx) {
                Ok(()) => (crate::EditorHooks::get(cx).ran)(invocation, cx),
                Err(error) => birchpad_ui::report_error(&error, window, cx),
            },
            None => cx.propagate(),
        }
    }

    /// Starts or stops blinking when the view gains or loses focus; called on every frame.
    pub(crate) fn track_focus(&mut self, window: &Window, cx: &mut Context<Self>) {
        let focused = self.focus_handle.is_focused(window);
        if focused != self.focused {
            self.focused = focused;
            if focused {
                self.pause_blink(cx);
            } else {
                self.blink = Task::ready(());
            }
        }
    }

    /// Restarts the caret blink cycle with the caret visible, as after every key press.
    pub(crate) fn pause_blink(&mut self, cx: &mut Context<Self>) {
        self.caret_visible = true;
        if !self.focused {
            return;
        }
        // Preferences > Editing > Caret blink rate; 0 does not blink.
        let period = AppState::global(cx)
            .settings
            .editor
            .caret_blink_ms
            .min(2000);
        if period == 0 {
            self.blink = Task::ready(());
            return;
        }
        let period = Duration::from_millis(u64::from(period));
        self.blink = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(period).await;
                let blinking = this.update(cx, |this, cx| {
                    this.caret_visible = !this.caret_visible;
                    cx.notify();
                });
                if blinking.is_err() {
                    break;
                }
            }
        });
    }

    /// Zoom, word wrap or Show Symbol changed: keep the caret in view after the relayout.
    pub fn settings_changed(&mut self, cx: &mut Context<Self>) {
        self.buffer
            .update(cx, |buffer, cx| buffer.sync_syntax_options(cx));
        self.scroll_width = px(0.);
        self.request_autoscroll(cx);
    }

    /// Brings the primary caret into view on the next frame, expanding folds that hide carets.
    pub(crate) fn request_autoscroll(&mut self, cx: &mut Context<Self>) {
        self.reveal_carets(cx);
        self.autoscroll = true;
        self.pause_blink(cx);
        cx.notify();
    }

    /// Rows that fit in the view, as of the last frame.
    pub(crate) fn page_rows(&self) -> isize {
        self.layout.as_ref().map_or(30, |layout| {
            layout.full_rows.saturating_sub(1).max(1) as isize
        })
    }

    /// Columns that fit across the text, as of the last frame.
    pub(crate) fn visible_columns(&self) -> Option<usize> {
        self.layout.as_ref().map(Layout::text_columns)
    }
}

impl Render for EditorView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let buffer = self.buffer.read(cx);
        let banner = match buffer.read_only() {
            Some(ReadOnly::Decoding(problem)) => {
                let encoding = birchpad_io::display_name(buffer.doc().format().encoding, false);
                Some(crate::banner::decoding_problem(
                    &encoding,
                    problem,
                    buffer.decode_changes().count,
                    self.focus_handle.clone(),
                ))
            }
            Some(ReadOnly::File) => Some(crate::banner::read_only_file()),
            Some(ReadOnly::Requested) => Some(crate::banner::read_only_requested()),
            Some(ReadOnly::Monitoring) => Some(crate::banner::monitoring()),
            Some(ReadOnly::Loading) | None => None,
        };
        let loading = buffer.loading_progress();

        div()
            .id("editor")
            .key_context(self.key_context())
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::run_command))
            .size_full()
            .flex()
            .flex_col()
            .bg(theme::paint(theme::editor().background))
            .text_color(theme::paint(theme::editor().text))
            .children(banner)
            .child(
                div()
                    .id("text")
                    .relative()
                    .flex_1()
                    .min_h(px(0.))
                    .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
                    .on_mouse_down(MouseButton::Right, cx.listener(Self::right_mouse_down))
                    .on_mouse_move(cx.listener(Self::mouse_move))
                    .on_mouse_up(MouseButton::Left, cx.listener(Self::mouse_up))
                    .on_mouse_up_out(MouseButton::Left, cx.listener(Self::mouse_up))
                    .on_scroll_wheel(cx.listener(Self::scroll_wheel))
                    .child(EditorElement::new(cx.entity()))
                    .children(self.render_completion(cx))
                    .children(loading.map(crate::banner::loading))
                    .context_menu({
                        let view = cx.entity().downgrade();
                        move |menu, window, cx| {
                            (crate::EditorHooks::get(cx).context_menu)(menu, &view, window, cx)
                        }
                    }),
            )
    }
}

impl Focusable for EditorView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
