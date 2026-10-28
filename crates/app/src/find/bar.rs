//! The find panel: its tabs, fields, search modes and options.

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
    App, AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable, Subscription,
    Window, div, prelude::*, px, rgb,
};

use crate::app_state::AppState;

use super::{FindOptions, FindTab, FolderSearch};

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
    Close,
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

    pub(super) fn set_options(
        &mut self,
        change: impl FnOnce(&mut FindOptions),
        cx: &mut Context<Self>,
    ) {
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
                Button::new("close-find")
                    .small()
                    .icon(IconName::Close)
                    .debug_selector(|| "close-find".into())
                    .on_click(emit(|| FindBarEvent::Close)),
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
                    .when(monospaced, |input| input.font_family(crate::MONOSPACE)),
            )
            .children(actions);
        let row = || div().flex().flex_row().items_center().gap_2();
        let replace_row = matches!(tab, FindTab::Replace | FindTab::FindInFiles).then(|| {
            row()
                .child(label("Replace with:"))
                .child(
                    Input::new(&self.replace_input)
                        .id("replace-input")
                        .w(px(360.))
                        .small()
                        .when(monospaced, |input| input.font_family(crate::MONOSPACE)),
                )
                .when(tab == FindTab::Replace, |row| {
                    row.child(button("replace", "Replace", || FindBarEvent::Replace))
                        .child(button("replace-all", "Replace All", || {
                            FindBarEvent::ReplaceAll
                        }))
                        .child(button(
                            "replace-all-in-all",
                            "Replace All in All Opened Documents",
                            || FindBarEvent::ReplaceAllInAll,
                        ))
                })
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
        div()
            .id("find-bar")
            .key_context("FindBar")
            .track_focus(&self.focus_handle)
            .flex()
            .flex_col()
            .flex_none()
            .gap_1()
            .px_3()
            .pb_2()
            .border_t_1()
            .border_color(crate::theme::paint(crate::theme::ui().border))
            .bg(crate::theme::paint(crate::theme::ui().surface))
            .text_size(px(13.))
            .child(tabs)
            .child(find_row)
            .children(replace_row)
            .children(files_rows.into_iter().flatten())
            .child(options_row)
            .child(mode_row)
            .children(status)
    }
}
