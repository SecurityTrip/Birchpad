//! The find panel: its tabs, fields, search modes and options ([`FindBar`]). It searches
//! nothing itself: what the user asks for is an event ([`FindBarEvent`]) the window answers.
//!
//! The panel is docked under the documents or floats over them, as Preferences > Searching say
//! (`searching.find-panel-floats`). Floating, it is moved by its title, and the window keeps it
//! where it was put ([`FloatingPlace`]).

#[macro_use]
extern crate birchpad_i18n;

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use birchpad_core::search::{Direction, Query, SearchMode};
use gpui_kit::component::button::Button;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::radio::{Radio, RadioGroup};
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::{Disableable as _, IconName, Sizable};
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, CursorStyle, EmptyView, Entity, EventEmitter,
    FocusHandle, Focusable, MouseButton, MouseDownEvent, Pixels, Point, Size, Subscription, Window,
    div, prelude::*, px, rgb,
};

use birchpad_ui::AppState;

pub enum FindBarEvent {
    Find(Direction),
    /// Find in Projects: Find All in the files of the chosen project panels.
    FindInProjects,
    Count,
    /// Find All in Current Document, or in All Opened Documents.
    FindAll {
        all_documents: bool,
    },
    Replace,
    ReplaceAll,
    ReplaceAllInAll,
    MarkAll,
    ClearMarks,
    CopyMarked,
    /// Find All in the files of the Find in Files tab.
    FindInFiles,
    ReplaceInFiles,
    /// Stops a Find in Files or Replace in Files that is running.
    StopFiles,
    /// The "..." button: choose the folder.
    BrowseFolder,
    /// Another tab was chosen.
    TabChanged,
    /// The button that docks a floating panel, or floats a docked one.
    ToggleFloating,
    Close,
}

/// What is dragged while the floating panel is moved by its title. The window follows it with
/// [`FindBar::drag_to`].
pub struct DraggedFindPanel;

/// Where the floating panel is: how far its right edge is from the right of the documents'
/// area, and its top from the area's top. From the right, so that it stays in the corner it was
/// put in when the window is made wider.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FloatingPlace {
    pub right: Pixels,
    pub top: Pixels,
}

impl FloatingPlace {
    /// How much of the panel stays in the area, across and down, however far it is dragged:
    /// enough of its title to take it back.
    pub const KEPT: Size<Pixels> = Size {
        width: px(120.),
        height: px(24.),
    };

    /// Moved as the mouse went from `from` to `to`, kept where [`Self::KEPT`] of it stays in an
    /// area of `area`.
    pub fn moved(self, from: Point<Pixels>, to: Point<Pixels>, area: Size<Pixels>) -> Self {
        let right = self.right - (to.x - from.x);
        let top = self.top + (to.y - from.y);
        Self {
            right: right.min(area.width - Self::KEPT.width).max(px(0.)),
            top: top.min(area.height - Self::KEPT.height).max(px(0.)),
        }
    }
}

impl Default for FloatingPlace {
    /// The top right corner, a little in, as the find widgets of editors open.
    fn default() -> Self {
        Self {
            right: px(16.),
            top: px(8.),
        }
    }
}

pub struct FindBar {
    pub visible: bool,
    pub tab: FindTab,
    pub find_input: Entity<InputState>,
    pub replace_input: Entity<InputState>,
    filters_input: Entity<InputState>,
    directory_input: Entity<InputState>,
    pub options: FindOptions,
    pub folder: FolderSearch,
    /// Find in Projects: the project panels whose files are searched.
    pub project_panels: [bool; 3],
    /// Set while Find in Files or Replace in Files runs; setting the flag stops it.
    pub files_running: Option<Arc<AtomicBool>>,
    /// A message from the last search, and whether it is a failure.
    pub status: Option<(String, bool)>,
    /// Where the panel is when it floats.
    pub place: FloatingPlace,
    /// While the panel is moved: where the mouse was pressed, and where the panel was then.
    drag: Option<(Point<Pixels>, FloatingPlace)>,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<FindBarEvent> for FindBar {}

impl Focusable for FindBar {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl FindBar {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let remembered = cx
            .try_global::<AppState>()
            .map(|app| app.state.find_in_files.clone())
            .unwrap_or_default();
        let find_input = cx.new(|cx| InputState::new(window, cx).placeholder(t!("find.find-what")));
        let replace_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(t!("find.replace-with")));
        let filters_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("*.*")
                .default_value(remembered.filters.clone())
        });
        let directory = remembered
            .directory
            .as_deref()
            .map(|dir| dir.display().to_string())
            .unwrap_or_default();
        let directory_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(t!("find.folder"))
                .default_value(directory)
        });
        let find_events =
            cx.subscribe_in(&find_input, window, |this, _, event: &InputEvent, _, cx| {
                if let InputEvent::PressEnter { shift, .. } = event {
                    // Enter does the tab's main action: Find Next, Find All in files, or Mark
                    // All.
                    if this.tab == FindTab::Mark {
                        cx.emit(FindBarEvent::MarkAll);
                    } else if this.tab == FindTab::FindInFiles {
                        cx.emit(FindBarEvent::FindInFiles);
                    } else if *shift != this.options.backward {
                        cx.emit(FindBarEvent::Find(Direction::Backward));
                    } else {
                        cx.emit(FindBarEvent::Find(Direction::Forward));
                    }
                }
            });
        // Enter replaces on the Replace tab; replacing in files only ever takes a click.
        let replace_events = cx.subscribe_in(
            &replace_input,
            window,
            |this, _, event: &InputEvent, _, cx| {
                if let InputEvent::PressEnter { .. } = event
                    && this.tab == FindTab::Replace
                {
                    cx.emit(FindBarEvent::Replace);
                }
            },
        );
        let mut subscriptions = vec![find_events, replace_events];
        for input in [&filters_input, &directory_input] {
            subscriptions.push(cx.subscribe_in(
                input,
                window,
                |_, _, event: &InputEvent, _, cx| {
                    if let InputEvent::PressEnter { .. } = event {
                        cx.emit(FindBarEvent::FindInFiles);
                    }
                },
            ));
        }
        Self {
            visible: false,
            tab: FindTab::Find,
            find_input,
            replace_input,
            filters_input,
            directory_input,
            options: FindOptions::default(),
            folder: FolderSearch {
                subfolders: remembered.subfolders,
                hidden: remembered.hidden,
                follow_current_document: remembered.follow_current_document,
            },
            project_panels: [true; 3],
            files_running: None,
            status: None,
            place: FloatingPlace::default(),
            drag: None,
            focus_handle: cx.focus_handle(),
            _subscriptions: subscriptions,
        }
    }

    /// The Filters field of Find in Files.
    pub fn filters(&self, cx: &App) -> String {
        self.filters_input.read(cx).value().to_string()
    }

    /// The Directory field of Find in Files.
    pub fn directory(&self, cx: &App) -> String {
        self.directory_input.read(cx).value().to_string()
    }

    pub fn set_directory(&mut self, directory: &Path, window: &mut Window, cx: &mut App) {
        let directory = directory.display().to_string();
        self.directory_input
            .update(cx, |input, cx| input.set_value(directory, window, cx));
    }

    #[cfg(any(feature = "test-support", feature = "e2e"))]
    pub fn set_filters(&mut self, filters: &str, window: &mut Window, cx: &mut App) {
        self.filters_input
            .update(cx, |input, cx| input.set_value(filters, window, cx));
    }

    #[cfg(any(feature = "test-support", feature = "e2e"))]
    pub fn replace_input_for_tests(&mut self, text: &str, window: &mut Window, cx: &mut App) {
        self.replace_input
            .update(cx, |input, cx| input.set_value(text, window, cx));
    }

    /// The panel's message, for tests.
    #[cfg(any(feature = "test-support", feature = "e2e"))]
    pub fn status_text(&self) -> Option<String> {
        self.status.as_ref().map(|(message, _)| message.clone())
    }

    /// Whether the panel floats over the documents (Preferences > Searching).
    pub fn floats(cx: &App) -> bool {
        cx.try_global::<AppState>()
            .is_some_and(|app| app.settings.searching.find_panel_floats)
    }

    /// The mouse pressed on the title of the floating panel, at `mouse`.
    pub fn start_drag(&mut self, mouse: Point<Pixels>) {
        self.drag = Some((mouse, self.place));
    }

    /// The panel's title dragged to `mouse`, over the documents' area of `area`.
    pub fn drag_to(&mut self, mouse: Point<Pixels>, area: Size<Pixels>, cx: &mut Context<Self>) {
        if let Some((from, place)) = self.drag {
            let moved = place.moved(from, mouse, area);
            if moved != self.place {
                self.place = moved;
                cx.notify();
            }
        }
    }

    /// Stops Find in Files or Replace in Files, if one runs.
    pub fn stop_files(&mut self, cx: &mut Context<Self>) {
        if let Some(cancel) = &self.files_running {
            cancel.store(true, Ordering::Relaxed);
        }
        cx.notify();
    }

    pub fn query(&self, cx: &App) -> Query {
        Query {
            pattern: self.find_input.read(cx).value().to_string(),
            match_case: self.options.match_case,
            whole_word: self.options.whole_word,
            mode: self.options.mode,
            dot_matches_newline: self.options.dot_matches_newline,
        }
    }

    pub fn replacement(&self, cx: &App) -> String {
        self.replace_input.read(cx).value().to_string()
    }

    pub fn set_status(&mut self, status: Option<(String, bool)>, cx: &mut Context<Self>) {
        self.status = status;
        cx.notify();
    }

    /// Shows the panel on `tab`, optionally with a new search text and the In selection
    /// option, and focuses the find field.
    pub fn show(
        &mut self,
        tab: FindTab,
        text: Option<String>,
        in_selection: Option<bool>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.visible = true;
        self.tab = tab;
        self.status = None;
        if let Some(in_selection) = in_selection {
            self.options.in_selection = in_selection;
        }
        if let Some(text) = text {
            self.find_input
                .update(cx, |input, cx| input.set_value(text, window, cx));
        }
        self.find_input.update(cx, |input, cx| {
            input.focus(window, cx);
            input.select_all(window, cx);
        });
        cx.notify();
    }

    pub fn set_options(&mut self, change: impl FnOnce(&mut FindOptions), cx: &mut Context<Self>) {
        change(&mut self.options);
        // Smart highlighting may follow the case and whole-word options.
        cx.set_global(birchpad_editor::ActiveFindOptions {
            match_case: self.options.match_case,
            whole_word: self.options.whole_word,
        });
        cx.notify();
        cx.refresh_windows();
    }

    fn option_box(
        &self,
        id: &'static str,
        label: &'static str,
        checked: bool,
        set: fn(&mut FindOptions, bool),
        cx: &mut Context<Self>,
    ) -> Checkbox {
        let this = cx.entity().downgrade();
        Checkbox::new(id)
            .label(label)
            .checked(checked)
            .small()
            .debug_selector(move || id.to_owned())
            .on_click(move |checked, _, cx| {
                let checked = *checked;
                this.update(cx, |this, cx| this.set_options(|o| set(o, checked), cx))
                    .ok();
            })
    }

    /// Find in Projects: whether Project Panel `panel + 1`'s files are searched.
    fn project_box(&self, panel: usize, cx: &Context<Self>) -> Checkbox {
        const IDS: [&str; 3] = ["project-panel-1", "project-panel-2", "project-panel-3"];
        let this = cx.entity().downgrade();
        let id = IDS[panel];
        Checkbox::new(id)
            .label(t!("find.project-panel", number = panel + 1))
            .checked(self.project_panels[panel])
            .small()
            .debug_selector(move || id.to_owned())
            .on_click(move |checked, _, cx| {
                let checked = *checked;
                this.update(cx, |this, cx| {
                    this.project_panels[panel] = checked;
                    cx.notify();
                })
                .ok();
            })
    }

    fn folder_box(
        &self,
        id: &'static str,
        label: &'static str,
        checked: bool,
        set: fn(&mut FolderSearch, bool),
        cx: &Context<Self>,
    ) -> Checkbox {
        let this = cx.entity().downgrade();
        Checkbox::new(id)
            .label(label)
            .checked(checked)
            .small()
            .debug_selector(move || id.to_owned())
            .on_click(move |checked, _, cx| {
                let checked = *checked;
                this.update(cx, |this, cx| {
                    set(&mut this.folder, checked);
                    cx.notify();
                })
                .ok();
            })
    }
}

impl Render for FindBar {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let monospaced = AppState::global(cx).settings.searching.monospaced;
        let floats = Self::floats(cx);
        let options = self.options;
        let tab = self.tab;
        let regex = options.mode == SearchMode::Regex;
        let emit = |event: fn() -> FindBarEvent| {
            cx.listener(move |_, _, _, cx: &mut Context<Self>| cx.emit(event()))
        };
        let button = |id: &'static str, label: &'static str, event: fn() -> FindBarEvent| {
            Button::new(id)
                .small()
                .label(label)
                .debug_selector(move || id.to_owned())
                .on_click(emit(event))
        };

        let tabs = TabBar::new("find-tabs")
            .underline()
            .small()
            .selected_index(FindTab::ALL.iter().position(|t| *t == tab).unwrap_or(0))
            .on_click(cx.listener(|this, index: &usize, _, cx| {
                this.tab = FindTab::ALL[*index];
                this.status = None;
                cx.emit(FindBarEvent::TabChanged);
                cx.notify();
            }))
            .child(
                Tab::new()
                    .label(t!("find.find"))
                    .debug_selector(|| "tab-find".into()),
            )
            .child(
                Tab::new()
                    .label(t!("find.replace"))
                    .debug_selector(|| "tab-replace".into()),
            )
            .child(
                Tab::new()
                    .label(t!("find.find-in-files"))
                    .debug_selector(|| "tab-find-in-files".into()),
            )
            .child(
                Tab::new()
                    .label(t!("find.find-in-projects"))
                    .debug_selector(|| "tab-find-in-projects".into()),
            )
            .child(
                Tab::new()
                    .label(t!("find.mark"))
                    .debug_selector(|| "tab-mark".into()),
            )
            .suffix(
                div()
                    .flex()
                    .flex_row()
                    .child(
                        Button::new("float-find")
                            .small()
                            .icon(if floats {
                                IconName::PanelBottom
                            } else {
                                IconName::ExternalLink
                            })
                            .tooltip(if floats {
                                t!("find.dock")
                            } else {
                                t!("find.float")
                            })
                            .debug_selector(|| "float-find".into())
                            .on_click(emit(|| FindBarEvent::ToggleFloating)),
                    )
                    .child(
                        Button::new("close-find")
                            .small()
                            .icon(IconName::Close)
                            .debug_selector(|| "close-find".into())
                            .on_click(emit(|| FindBarEvent::Close)),
                    ),
            );

        let actions: Vec<Button> = match tab {
            FindTab::Find => vec![
                button("find-previous", "Find Previous", || {
                    FindBarEvent::Find(Direction::Backward)
                }),
                button("find-next", "Find Next", || {
                    FindBarEvent::Find(Direction::Forward)
                }),
                button("count", "Count", || FindBarEvent::Count),
                button("find-all", "Find All in Current Document", || {
                    FindBarEvent::FindAll {
                        all_documents: false,
                    }
                }),
                button(
                    "find-all-in-all",
                    "Find All in All Opened Documents",
                    || FindBarEvent::FindAll {
                        all_documents: true,
                    },
                ),
            ],
            FindTab::Replace => vec![button("find-next", "Find Next", || {
                FindBarEvent::Find(Direction::Forward)
            })],
            FindTab::FindInFiles | FindTab::FindInProjects if self.files_running.is_some() => {
                vec![button("stop-files", "Stop", || FindBarEvent::StopFiles)]
            }
            FindTab::FindInFiles => vec![
                button("find-in-files", "Find All", || FindBarEvent::FindInFiles),
                button("replace-in-files", "Replace in Files", || {
                    FindBarEvent::ReplaceInFiles
                }),
            ],
            FindTab::FindInProjects => vec![button("find-in-projects", "Find All", || {
                FindBarEvent::FindInProjects
            })],
            FindTab::Mark => vec![
                button("mark-all", "Mark All", || FindBarEvent::MarkAll),
                button("clear-marks", "Clear All Marks", || {
                    FindBarEvent::ClearMarks
                }),
                button("copy-marked", "Copy Marked Text", || {
                    FindBarEvent::CopyMarked
                }),
            ],
        };
        let replace_actions: Vec<Button> = if tab == FindTab::Replace {
            vec![
                button("replace", "Replace", || FindBarEvent::Replace),
                button("replace-all", "Replace All", || FindBarEvent::ReplaceAll),
                button(
                    "replace-all-in-all",
                    "Replace All in All Opened Documents",
                    || FindBarEvent::ReplaceAllInAll,
                ),
            ]
        } else {
            Vec::new()
        };
        // Beside the fields when docked; in a column of their own when floating.
        let (inline_actions, inline_replace, column) = if floats {
            let column: Vec<Button> = actions.into_iter().chain(replace_actions).collect();
            (Vec::new(), Vec::new(), column)
        } else {
            (actions, replace_actions, Vec::new())
        };
        let label = |text: &'static str| div().w(px(84.)).flex_none().child(text);
        let find_row = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .child(label("Find what:"))
            .child(
                Input::new(&self.find_input)
                    .id("find-input")
                    .w(px(360.))
                    .small()
                    .when(monospaced, |input| {
                        input.font_family(birchpad_widgets::MONOSPACE)
                    }),
            )
            .children(inline_actions);
        let row = || div().flex().flex_row().items_center().gap_2();
        let replace_row = matches!(tab, FindTab::Replace | FindTab::FindInFiles).then(|| {
            row()
                .child(label("Replace with:"))
                .child(
                    Input::new(&self.replace_input)
                        .id("replace-input")
                        .w(px(360.))
                        .small()
                        .when(monospaced, |input| {
                            input.font_family(birchpad_widgets::MONOSPACE)
                        }),
                )
                .children(inline_replace)
        });
        let files_rows = (tab == FindTab::FindInFiles).then(|| {
            let folder = self.folder;
            [
                row()
                    .child(label("Filters:"))
                    .child(
                        Input::new(&self.filters_input)
                            .id("filters-input")
                            .w(px(360.))
                            .small(),
                    )
                    .into_any_element(),
                row()
                    .child(label("Directory:"))
                    .child(
                        Input::new(&self.directory_input)
                            .id("directory-input")
                            .w(px(360.))
                            .small(),
                    )
                    .child(button("browse-folder", "...", || {
                        FindBarEvent::BrowseFolder
                    }))
                    .child(self.folder_box(
                        "follow-current",
                        "Follow current doc.",
                        folder.follow_current_document,
                        |f, v| f.follow_current_document = v,
                        cx,
                    ))
                    .into_any_element(),
            ]
        });

        let mut boxes = vec![
            self.option_box(
                "whole-word",
                "Match whole word only",
                options.whole_word,
                |o, v| o.whole_word = v,
                cx,
            )
            .disabled(regex),
            self.option_box(
                "match-case",
                "Match case",
                options.match_case,
                |o, v| o.match_case = v,
                cx,
            ),
        ];
        if tab == FindTab::FindInProjects {
            for panel in 0..3 {
                boxes.push(self.project_box(panel, cx));
            }
        }
        if tab == FindTab::FindInFiles {
            boxes.push(self.folder_box(
                "subfolders",
                "In all sub-folders",
                self.folder.subfolders,
                |f, v| f.subfolders = v,
                cx,
            ));
            boxes.push(self.folder_box(
                "hidden-folders",
                "In hidden folders",
                self.folder.hidden,
                |f, v| f.hidden = v,
                cx,
            ));
        }
        if matches!(tab, FindTab::Find | FindTab::Replace) {
            boxes.push(self.option_box(
                "wrap-around",
                "Wrap around",
                options.wrap_around,
                |o, v| o.wrap_around = v,
                cx,
            ));
            boxes.push(self.option_box(
                "backward",
                "Backward direction",
                options.backward,
                |o, v| o.backward = v,
                cx,
            ));
        }
        if !matches!(tab, FindTab::FindInFiles | FindTab::FindInProjects) {
            boxes.push(self.option_box(
                "in-selection",
                "In selection",
                options.in_selection,
                |o, v| o.in_selection = v,
                cx,
            ));
        }
        if tab == FindTab::Mark {
            boxes.push(self.option_box(
                "bookmark-line",
                "Bookmark line",
                options.bookmark_line,
                |o, v| o.bookmark_line = v,
                cx,
            ));
            boxes.push(self.option_box(
                "purge",
                "Purge for each search",
                options.purge,
                |o, v| o.purge = v,
                cx,
            ));
        }
        let options_row = div()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .gap_3()
            .child(label(""))
            .children(boxes);

        let modes = [SearchMode::Normal, SearchMode::Extended, SearchMode::Regex];
        let mode_row = div()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .gap_3()
            .child(label("Search mode:"))
            // Not stretched, so the regular expression option follows the modes.
            .child(
                div().flex_none().child(
                    RadioGroup::horizontal("search-mode")
                        .selected_index(modes.iter().position(|m| *m == options.mode))
                        .on_click(cx.listener(move |this, index: &usize, _, cx| {
                            let mode = modes[*index];
                            this.set_options(|o| o.mode = mode, cx);
                        }))
                        .child(
                            Radio::new("mode-normal")
                                .label(t!("find.normal"))
                                .debug_selector(|| "mode-normal".into()),
                        )
                        .child(
                            Radio::new("mode-extended")
                                .label(t!("find.extended-n-r-t-0-x"))
                                .debug_selector(|| "mode-extended".into()),
                        )
                        .child(
                            Radio::new("mode-regex")
                                .label(t!("find.regular-expression"))
                                .debug_selector(|| "mode-regex".into()),
                        ),
                ),
            )
            .child(
                self.option_box(
                    "dot-newline",
                    ". matches newline",
                    options.dot_matches_newline,
                    |o, v| o.dot_matches_newline = v,
                    cx,
                )
                .disabled(!regex),
            );

        let status = self.status.clone().map(|(message, failed)| {
            div()
                .text_color(rgb(if failed { 0xcf222e } else { 0x57606a }))
                .child(message)
        });
        if floats {
            let fields = div()
                .flex()
                .flex_col()
                .gap_1()
                .w(px(470.))
                .child(find_row)
                .children(replace_row)
                .children(files_rows.into_iter().flatten())
                .child(options_row)
                .child(mode_row);
            // The buttons in a column at the right, as in Notepad++'s Find dialog.
            let buttons = div()
                .flex()
                .flex_col()
                .gap_1()
                .w(px(230.))
                .children(column.into_iter().map(|button| button.w_full()));
            let title = match tab {
                FindTab::Find => t!("find.find"),
                FindTab::Replace => t!("find.replace"),
                FindTab::FindInFiles => t!("find.find-in-files"),
                FindTab::FindInProjects => t!("find.find-in-projects"),
                FindTab::Mark => t!("find.mark"),
            };
            return self
                .frame(cx)
                .absolute()
                .top(self.place.top)
                .right(self.place.right)
                // Over the documents: what is under it gets no clicks.
                .occlude()
                .border_1()
                .rounded_md()
                .shadow_lg()
                .pt_1()
                .child(self.title_bar(title, cx))
                .child(tabs)
                .child(div().flex().flex_row().gap_3().child(fields).child(buttons))
                .children(status)
                .into_any_element();
        }
        self.frame(cx)
            .border_t_1()
            .child(tabs)
            .child(find_row)
            .children(replace_row)
            .children(files_rows.into_iter().flatten())
            .child(options_row)
            .child(mode_row)
            .children(status)
            .into_any_element()
    }
}

impl FindBar {
    /// What the docked and the floating panel share: its focus, its colors and its text.
    fn frame(&self, _: &App) -> gpui_kit::Stateful<gpui_kit::Div> {
        div()
            .id("find-bar")
            .debug_selector(|| "find-bar".into())
            .key_context("FindBar")
            .track_focus(&self.focus_handle)
            .flex()
            .flex_col()
            .flex_none()
            .gap_1()
            .px_3()
            .pb_2()
            .border_color(birchpad_widgets::theme::paint(
                birchpad_widgets::theme::ui().border,
            ))
            .bg(birchpad_widgets::theme::paint(
                birchpad_widgets::theme::ui().surface,
            ))
            .text_size(px(13.))
    }

    /// The title of the floating panel, which moves it.
    fn title_bar(&self, title: String, cx: &mut Context<Self>) -> AnyElement {
        div()
            .id("find-panel-title")
            .debug_selector(|| "find-panel-title".into())
            .h(px(20.))
            .flex()
            .items_center()
            .cursor(CursorStyle::OpenHand)
            .text_color(birchpad_widgets::theme::paint(
                birchpad_widgets::theme::ui().muted,
            ))
            .child(title)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, _, _| this.start_drag(event.position)),
            )
            .on_drag(DraggedFindPanel, |_, _, _, cx| cx.new(|_| EmptyView))
            .into_any_element()
    }
}

/// The tabs of the find panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindTab {
    Find,
    Replace,
    FindInFiles,
    /// The files of the project panels.
    FindInProjects,
    Mark,
}

impl FindTab {
    const ALL: [Self; 5] = [
        Self::Find,
        Self::Replace,
        Self::FindInFiles,
        Self::FindInProjects,
        Self::Mark,
    ];
}

/// The folder options of the Find in Files tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FolderSearch {
    /// In all sub-folders.
    pub subfolders: bool,
    /// In hidden folders.
    pub hidden: bool,
    /// Follow current doc.: the folder of the active document.
    pub follow_current_document: bool,
}

/// Options of the find panel, as in Notepad++'s Find dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FindOptions {
    pub match_case: bool,
    pub whole_word: bool,
    pub wrap_around: bool,
    pub backward: bool,
    /// Count, Replace All and Mark All work in the selection.
    pub in_selection: bool,
    pub mode: SearchMode,
    /// Regular expressions: `.` matches line breaks.
    pub dot_matches_newline: bool,
    /// Mark All also bookmarks the lines of its matches.
    pub bookmark_line: bool,
    /// Mark All first clears the marks of earlier searches.
    pub purge: bool,
}

impl Default for FindOptions {
    fn default() -> Self {
        Self {
            match_case: false,
            whole_word: false,
            // Notepad++'s default.
            wrap_around: true,
            backward: false,
            in_selection: false,
            mode: SearchMode::Normal,
            dot_matches_newline: false,
            bookmark_line: false,
            purge: false,
        }
    }
}
