//! Find, Replace, Find in Files, Mark and Go To.
//!
//! The find panel sits at the bottom of the window, above the status bar, with Notepad++'s Find,
//! Replace, Find in Files and Mark tabs, search modes and options. It is a panel rather than a dialog, and like
//! Notepad++'s modeless dialog it leaves the document editable while it is open. F3 and
//! Shift+F3 repeat the last search even when the panel is closed. Searching goes through
//! `birchpad_core::search`.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context as _, Result, anyhow, bail};
use birchpad_core::motion::{line_count, line_of, line_range, next_boundary, prev_boundary};
use birchpad_core::search::{Direction, Query, SearchMode, Searcher, token_at};
use birchpad_core::{Edit, Rope, Transaction, ops};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::radio::{Radio, RadioGroup};
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::{Disableable as _, IconName, Selectable as _, Sizable, WindowExt as _};
use gpui_kit::{
    App, AppContext as _, ClipboardItem, Context, Entity, EventEmitter, FocusHandle, Focusable,
    Subscription, Window, div, prelude::*, px, rgb,
};

use crate::app_state::AppState;
use crate::commands::CommandRegistry;
use crate::editor::EditorView;
use crate::search_results::{
    FileResults, ResultLocation, SearchResults, SearchResultsEvent, SearchRun, line_hits,
};
use crate::workspace::Workspace;

pub(crate) fn register_commands(registry: &mut CommandRegistry) {
    registry.workspace("search.find", |this, (), window, cx| {
        this.open_find(FindTab::Find, window, cx);
        Ok(())
    });
    registry.workspace("search.replace", |this, (), window, cx| {
        this.open_find(FindTab::Replace, window, cx);
        Ok(())
    });
    registry.workspace("search.find-in-files", |this, (), window, cx| {
        this.open_find(FindTab::FindInFiles, window, cx);
        Ok(())
    });
    registry.workspace("search.mark", |this, (), window, cx| {
        this.open_find(FindTab::Mark, window, cx);
        Ok(())
    });
    registry.workspace("search.find-next", |this, (), window, cx| {
        this.find(Direction::Forward, window, cx);
        Ok(())
    });
    registry.workspace("search.find-previous", |this, (), window, cx| {
        this.find(Direction::Backward, window, cx);
        Ok(())
    });
    registry.workspace("search.select-and-find-next", |this, (), window, cx| {
        this.select_and_find(Direction::Forward, window, cx);
        Ok(())
    });
    registry.workspace("search.select-and-find-previous", |this, (), window, cx| {
        this.select_and_find(Direction::Backward, window, cx);
        Ok(())
    });
    registry.workspace("search.next-result", |this, (), window, cx| {
        this.step_result(true, window, cx);
        Ok(())
    });
    registry.workspace("search.previous-result", |this, (), window, cx| {
        this.step_result(false, window, cx);
        Ok(())
    });
    registry.workspace("search.results-window", |this, (), _, cx| {
        this.search_results.update(cx, |results, cx| {
            results.visible ^= true;
            cx.notify();
        });
        cx.notify();
        Ok(())
    });
    registry.workspace("search.close", |this, (), window, cx| {
        this.close_find(window, cx);
        Ok(())
    });
    registry.workspace("search.go-to", |this, (), window, cx| {
        this.open_go_to(window, cx)
    });
}

/// The tabs of the find panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FindTab {
    Find,
    Replace,
    FindInFiles,
    Mark,
}

impl FindTab {
    const ALL: [Self; 4] = [Self::Find, Self::Replace, Self::FindInFiles, Self::Mark];
}

/// The folder options of the Find in Files tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FolderSearch {
    /// In all sub-folders.
    pub(crate) subfolders: bool,
    /// In hidden folders.
    pub(crate) hidden: bool,
    /// Follow current doc.: the folder of the active document.
    pub(crate) follow_current_document: bool,
}

/// Options of the find panel, as in Notepad++'s Find dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FindOptions {
    pub(crate) match_case: bool,
    pub(crate) whole_word: bool,
    pub(crate) wrap_around: bool,
    pub(crate) backward: bool,
    /// Count, Replace All and Mark All work in the selection.
    pub(crate) in_selection: bool,
    pub(crate) mode: SearchMode,
    /// Regular expressions: `.` matches line breaks.
    pub(crate) dot_matches_newline: bool,
    /// Mark All also bookmarks the lines of its matches.
    pub(crate) bookmark_line: bool,
    /// Mark All first clears the marks of earlier searches.
    pub(crate) purge: bool,
}

/// The find panel's options, for smart highlighting with
/// `highlighting.smart.use-find-options`.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ActiveFindOptions(pub(crate) FindOptions);

impl gpui_kit::Global for ActiveFindOptions {}

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

pub(crate) enum FindBarEvent {
    Find(Direction),
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

pub(crate) struct FindBar {
    pub(crate) visible: bool,
    pub(crate) tab: FindTab,
    find_input: Entity<InputState>,
    replace_input: Entity<InputState>,
    filters_input: Entity<InputState>,
    directory_input: Entity<InputState>,
    pub(crate) options: FindOptions,
    pub(crate) folder: FolderSearch,
    /// Set while Find in Files or Replace in Files runs; setting the flag stops it.
    pub(crate) files_running: Option<Arc<AtomicBool>>,
    /// A message from the last search, and whether it is a failure.
    status: Option<(String, bool)>,
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
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let remembered = cx
            .try_global::<AppState>()
            .map(|app| app.state.find_in_files.clone())
            .unwrap_or_default();
        let find_input = cx.new(|cx| InputState::new(window, cx).placeholder("Find what"));
        let replace_input = cx.new(|cx| InputState::new(window, cx).placeholder("Replace with"));
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
                .placeholder("Folder")
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
            files_running: None,
            status: None,
            focus_handle: cx.focus_handle(),
            _subscriptions: subscriptions,
        }
    }

    /// The Filters field of Find in Files.
    pub(crate) fn filters(&self, cx: &App) -> String {
        self.filters_input.read(cx).value().to_string()
    }

    /// The Directory field of Find in Files.
    pub(crate) fn directory(&self, cx: &App) -> String {
        self.directory_input.read(cx).value().to_string()
    }

    pub(crate) fn set_directory(&mut self, directory: &Path, window: &mut Window, cx: &mut App) {
        let directory = directory.display().to_string();
        self.directory_input
            .update(cx, |input, cx| input.set_value(directory, window, cx));
    }

    #[cfg(test)]
    pub(crate) fn set_filters(&mut self, filters: &str, window: &mut Window, cx: &mut App) {
        self.filters_input
            .update(cx, |input, cx| input.set_value(filters, window, cx));
    }

    #[cfg(test)]
    pub(crate) fn replace_input_for_tests(
        &mut self,
        text: &str,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.replace_input
            .update(cx, |input, cx| input.set_value(text, window, cx));
    }

    /// The panel's message, for tests.
    #[cfg(test)]
    pub(crate) fn status_text(&self) -> Option<String> {
        self.status.as_ref().map(|(message, _)| message.clone())
    }

    /// Stops Find in Files or Replace in Files, if one runs.
    pub(crate) fn stop_files(&mut self, cx: &mut Context<Self>) {
        if let Some(cancel) = &self.files_running {
            cancel.store(true, Ordering::Relaxed);
        }
        cx.notify();
    }

    pub(crate) fn query(&self, cx: &App) -> Query {
        Query {
            pattern: self.find_input.read(cx).value().to_string(),
            match_case: self.options.match_case,
            whole_word: self.options.whole_word,
            mode: self.options.mode,
            dot_matches_newline: self.options.dot_matches_newline,
        }
    }

    pub(crate) fn replacement(&self, cx: &App) -> String {
        self.replace_input.read(cx).value().to_string()
    }

    pub(crate) fn set_status(&mut self, status: Option<(String, bool)>, cx: &mut Context<Self>) {
        self.status = status;
        cx.notify();
    }

    /// Shows the panel on `tab`, optionally with a new search text and the In selection
    /// option, and focuses the find field.
    pub(crate) fn show(
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

    fn set_options(&mut self, change: impl FnOnce(&mut FindOptions), cx: &mut Context<Self>) {
        change(&mut self.options);
        cx.set_global(ActiveFindOptions(self.options));
        cx.notify();
        // Smart highlighting may follow these options.
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
                    .label("Find")
                    .debug_selector(|| "tab-find".into()),
            )
            .child(
                Tab::new()
                    .label("Replace")
                    .debug_selector(|| "tab-replace".into()),
            )
            .child(
                Tab::new()
                    .label("Find in Files")
                    .debug_selector(|| "tab-find-in-files".into()),
            )
            .child(
                Tab::new()
                    .label("Mark")
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
            FindTab::FindInFiles if self.files_running.is_some() => {
                vec![button("stop-files", "Stop", || FindBarEvent::StopFiles)]
            }
            FindTab::FindInFiles => vec![
                button("find-in-files", "Find All", || FindBarEvent::FindInFiles),
                button("replace-in-files", "Replace in Files", || {
                    FindBarEvent::ReplaceInFiles
                }),
            ],
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
                    .small(),
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
                        .small(),
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
        if tab != FindTab::FindInFiles {
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
                                .label("Normal")
                                .debug_selector(|| "mode-normal".into()),
                        )
                        .child(
                            Radio::new("mode-extended")
                                .label("Extended (\\n, \\r, \\t, \\0, \\x...)")
                                .debug_selector(|| "mode-extended".into()),
                        )
                        .child(
                            Radio::new("mode-regex")
                                .label("Regular expression")
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

/// Where Count, Replace All and Mark All look: the selection with In selection, else the
/// whole text.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Scope {
    range: std::ops::Range<usize>,
    in_selection: bool,
}

impl Scope {
    fn describe(&self) -> &'static str {
        if self.in_selection {
            "in selection"
        } else {
            "in entire file"
        }
    }
}

pub(crate) fn plural(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

impl Workspace {
    /// Ctrl+F, Ctrl+H, Ctrl+M: shows the panel on `tab`. A selection on one line becomes the
    /// search text; a selection over several lines turns In selection on instead, as in
    /// Notepad++.
    fn open_find(&mut self, tab: FindTab, window: &mut Window, cx: &mut Context<Self>) {
        let (text, in_selection) = self
            .active_view(cx)
            .map(|view| {
                let view = view.read(cx);
                let range = view.selection.primary();
                let text = view.text(cx).slice(range.from()..range.to()).to_string();
                if text.is_empty() {
                    (None, Some(false))
                } else if text.contains(['\n', '\r']) {
                    (None, Some(true))
                } else {
                    (Some(text), Some(false))
                }
            })
            .unwrap_or((None, None));
        self.find_bar
            .update(cx, |bar, cx| bar.show(tab, text, in_selection, window, cx));
        self.follow_current_document(window, cx);
        cx.notify();
    }

    fn close_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.find_bar.update(cx, |bar, cx| {
            bar.visible = false;
            cx.notify();
        });
        if let Some(view) = self.active_view(cx) {
            let focus = view.read(cx).focus_handle.clone();
            window.focus(&focus, cx);
        }
        cx.notify();
    }

    pub(crate) fn on_find_bar_event(
        &mut self,
        _: &Entity<FindBar>,
        event: &FindBarEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            FindBarEvent::Find(direction) => self.find(*direction, window, cx),
            FindBarEvent::Count => self.count(cx),
            FindBarEvent::FindAll { all_documents } => self.find_all(*all_documents, cx),
            FindBarEvent::Replace => self.replace(window, cx),
            FindBarEvent::ReplaceAll => self.replace_all(window, cx),
            FindBarEvent::ReplaceAllInAll => self.replace_all_in_all(cx),
            FindBarEvent::MarkAll => self.mark_all(cx),
            FindBarEvent::ClearMarks => self.clear_marks(cx),
            FindBarEvent::CopyMarked => self.copy_marked(cx),
            FindBarEvent::FindInFiles => self.find_in_files(window, cx),
            FindBarEvent::ReplaceInFiles => self.replace_in_files(window, cx),
            FindBarEvent::StopFiles => self.find_bar.update(cx, |bar, cx| bar.stop_files(cx)),
            FindBarEvent::BrowseFolder => self.browse_folder(window, cx),
            FindBarEvent::TabChanged => self.follow_current_document(window, cx),
            FindBarEvent::Close => self.close_find(window, cx),
        }
    }

    pub(crate) fn report(&mut self, message: Option<String>, failed: bool, cx: &mut Context<Self>) {
        self.find_bar.update(cx, |bar, cx| {
            bar.set_status(message.map(|message| (message, failed)), cx);
        });
    }

    pub(crate) fn searcher(&mut self, cx: &mut Context<Self>) -> Option<(Searcher, Query)> {
        let query = self.find_bar.read(cx).query(cx);
        match Searcher::new(&query) {
            Ok(searcher) => Some((searcher, query)),
            Err(error) => {
                self.report(Some(format!("Find: {error}")), true, cx);
                None
            }
        }
    }

    /// Reports a regular expression that failed to match; true if it did.
    pub(crate) fn report_failure(&mut self, searcher: &Searcher, cx: &mut Context<Self>) -> bool {
        match searcher.failure() {
            Some(error) => {
                self.report(Some(format!("Find: {error}")), true, cx);
                true
            }
            None => false,
        }
    }

    /// The range Count, Replace All and Mark All work on in `view`.
    fn scope(&self, view: &Entity<EditorView>, cx: &App) -> Scope {
        let view = view.read(cx);
        let selection = view.selection.primary();
        if self.find_bar.read(cx).options.in_selection && !selection.is_empty() {
            Scope {
                range: selection.from()..selection.to(),
                in_selection: true,
            }
        } else {
            Scope {
                range: 0..view.text(cx).len(),
                in_selection: false,
            }
        }
    }

    /// Find Next / Find Previous (F3 / Shift+F3), from the current selection.
    pub(crate) fn find(&mut self, direction: Direction, _: &mut Window, cx: &mut Context<Self>) {
        let Some(view) = self.active_view(cx) else {
            return;
        };
        let Some((searcher, query)) = self.searcher(cx) else {
            return;
        };
        let options = self.find_bar.read(cx).options;
        let (text, selection) = {
            let view = view.read(cx);
            let range = view.selection.primary();
            (view.text(cx).clone(), range.from()..range.to())
        };
        let from = match direction {
            Direction::Forward => selection.end,
            Direction::Backward => selection.start,
        };
        let mut found = searcher.find(&text, from, direction, options.wrap_around);
        // An empty match where the caret already is (a regular expression like `^`): step over
        // it, or the search would never move.
        if let Some(range) = &found
            && range.is_empty()
            && selection.is_empty()
            && range.start == from
        {
            found = match direction {
                Direction::Forward if from < text.len() => searcher.find(
                    &text,
                    next_boundary(&text, from),
                    direction,
                    options.wrap_around,
                ),
                Direction::Backward if from > 0 => searcher.find(
                    &text,
                    prev_boundary(&text, from),
                    direction,
                    options.wrap_around,
                ),
                _ if options.wrap_around => {
                    let restart = match direction {
                        Direction::Forward => 0,
                        Direction::Backward => text.len(),
                    };
                    searcher
                        .find(&text, restart, direction, false)
                        .filter(|again| again.start != from)
                }
                _ => None,
            };
        }
        match found {
            Some(found) => {
                let wrapped = match direction {
                    Direction::Forward => found.start < from,
                    Direction::Backward => found.start >= from,
                };
                let message = wrapped.then(|| match direction {
                    Direction::Forward => {
                        "Reached the end of the document; continued from the top.".to_owned()
                    }
                    Direction::Backward => {
                        "Reached the start of the document; continued from the bottom.".to_owned()
                    }
                });
                self.report(message, false, cx);
                view.update(cx, |view, cx| view.select_range(found, cx));
            }
            None => {
                if !self.report_failure(&searcher, cx) {
                    self.report(
                        Some(format!("Can't find the text \"{}\"", query.pattern)),
                        true,
                        cx,
                    );
                }
            }
        }
    }

    /// Select and Find Next / Previous (Ctrl+F3 / Ctrl+Shift+F3): searches for the selection, or
    /// the word at the caret, as a whole word if it is the word at the caret.
    fn select_and_find(
        &mut self,
        direction: Direction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(view) = self.active_view(cx) else {
            return;
        };
        let (token, selected) = {
            let view = view.read(cx);
            let range = view.selection.primary();
            let text = view.text(cx);
            let Some(token) = token_at(text, range.from()..range.to()) else {
                return;
            };
            (text.slice(token.clone()).to_string(), token)
        };
        if token.contains(['\n', '\r']) {
            return;
        }
        view.update(cx, |view, cx| view.select_range(selected, cx));
        self.find_bar.update(cx, |bar, cx| {
            bar.find_input
                .update(cx, |input, cx| input.set_value(token, window, cx));
            bar.set_options(|o| o.mode = SearchMode::Normal, cx);
        });
        self.find(direction, window, cx);
    }

    /// Count: the matches in the document or the selection.
    fn count(&mut self, cx: &mut Context<Self>) {
        let Some(view) = self.active_view(cx) else {
            return;
        };
        let Some((searcher, _)) = self.searcher(cx) else {
            return;
        };
        let scope = self.scope(&view, cx);
        let text = view.read(cx).text(cx).clone();
        let count = searcher.find_all_in(&text, scope.range.clone()).len();
        if self.report_failure(&searcher, cx) {
            return;
        }
        self.report(
            Some(format!(
                "Count: {} {}",
                plural(count, "match", "matches"),
                scope.describe()
            )),
            count == 0,
            cx,
        );
    }

    /// One view per open document: views of one buffer share its text. The active document
    /// comes first.
    pub(crate) fn document_views(&self, cx: &App) -> Vec<Entity<EditorView>> {
        let mut views = self.all_views(cx);
        if let Some(active) = self.active_view(cx) {
            views.retain(|view| *view != active);
            views.insert(0, active);
        }
        let mut seen = Vec::new();
        views.retain(|view| {
            let buffer = view.read(cx).buffer.entity_id();
            let new = !seen.contains(&buffer);
            seen.push(buffer);
            new
        });
        views
    }

    /// Find All in Current Document (in the selection with In selection) or in All Opened
    /// Documents: lists the lines with matches in the search results panel.
    fn find_all(&mut self, all_documents: bool, cx: &mut Context<Self>) {
        let Some((searcher, query)) = self.searcher(cx) else {
            return;
        };
        let views = if all_documents {
            self.document_views(cx)
        } else {
            self.active_view(cx).into_iter().collect()
        };
        let searched = views.len();
        let mut files = Vec::new();
        for view in views {
            let range = if all_documents {
                0..view.read(cx).text(cx).len()
            } else {
                self.scope(&view, cx).range
            };
            let view = view.read(cx);
            let buffer = view.buffer.read(cx);
            let text = view.text(cx);
            let matches = searcher.find_all_in(text, range);
            if matches.is_empty() {
                continue;
            }
            let name = buffer
                .path()
                .map_or_else(|| buffer.display_name(), |path| path.display().to_string());
            files.push(FileResults::new(
                ResultLocation::Buffer(view.buffer.downgrade()),
                name,
                line_hits(text, &matches),
                matches.len(),
            ));
        }
        if self.report_failure(&searcher, cx) {
            return;
        }
        let run = SearchRun::new(&query.pattern, files, searched);
        let hits: usize = run.files.iter().map(|file| file.hits).sum();
        self.search_results
            .update(cx, |results, cx| results.add(run, cx));
        self.report(
            Some(format!("Find All: {}", plural(hits, "hit", "hits"))),
            hits == 0,
            cx,
        );
        cx.notify();
    }

    /// F4 / Shift+F4: the next or previous search result.
    fn step_result(&mut self, forward: bool, _: &mut Window, cx: &mut Context<Self>) {
        let found = self
            .search_results
            .update(cx, |results, cx| results.step(forward, cx));
        if !found {
            self.report(Some("No search results".into()), true, cx);
        }
        cx.notify();
    }

    pub(crate) fn on_search_results_event(
        &mut self,
        _: &Entity<SearchResults>,
        event: &SearchResultsEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            SearchResultsEvent::Close => {
                self.search_results.update(cx, |results, cx| {
                    results.visible = false;
                    cx.notify();
                });
                cx.notify();
            }
            SearchResultsEvent::Open {
                location,
                line,
                target,
            } => self.open_result(location, *line, target.clone(), window, cx),
        }
    }

    /// Goes to a search result: the tab of its document, and the match on its line.
    fn open_result(
        &mut self,
        location: &ResultLocation,
        line: usize,
        target: std::ops::Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = match location {
            ResultLocation::Buffer(buffer) => {
                let Some(buffer) = buffer.upgrade() else {
                    self.report(Some("The document was closed".into()), true, cx);
                    return;
                };
                let active = self
                    .active_view(cx)
                    .filter(|view| view.read(cx).buffer == buffer);
                let Some(view) = active.or_else(|| {
                    self.all_views(cx)
                        .into_iter()
                        .find(|view| view.read(cx).buffer == buffer)
                }) else {
                    self.report(Some("The document was closed".into()), true, cx);
                    return;
                };
                self.activate_view(&view, window, cx);
                view
            }
            ResultLocation::File(path) => {
                self.open_path(path, window, cx);
                let Some(view) = self.view_for_path(path, cx) else {
                    return;
                };
                view
            }
        };
        view.update(cx, |view, cx| view.select_in_line(line, target, cx));
        let focus = view.read(cx).focus_handle.clone();
        window.focus(&focus, cx);
        cx.notify();
    }

    /// Replace: replaces the selection if it is a match, then finds the next one.
    fn replace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(view) = self.active_view(cx) else {
            return;
        };
        let Some((searcher, _)) = self.searcher(cx) else {
            return;
        };
        let template = self.find_bar.read(cx).replacement(cx);
        let (text, selected) = {
            let view = view.read(cx);
            let range = view.selection.primary();
            (view.text(cx).clone(), range.from()..range.to())
        };
        if searcher.is_match(&text, selected.clone())
            && (!selected.is_empty() || searcher.is_regex())
        {
            let replacement = searcher.replacement(&text, selected.clone(), &template);
            let caret = selected.start + replacement.len();
            let transaction =
                Transaction::from_edits(&text, [Edit::replace(selected, replacement)])
                    .expect("a match lies on character boundaries");
            let applied = view.update(cx, |view, cx| {
                let applied = view.apply_command_edit(transaction, cx);
                if applied {
                    view.go_to(caret, cx);
                }
                applied
            });
            if !applied {
                self.report(Some("The document is read-only".into()), true, cx);
                return;
            }
        }
        self.find(Direction::Forward, window, cx);
    }

    /// The edits of Replace All in `view` (in its scope), and how many there are.
    pub(crate) fn replace_all_edits(
        &self,
        searcher: &Searcher,
        template: &str,
        view: &Entity<EditorView>,
        range: std::ops::Range<usize>,
        cx: &App,
    ) -> Option<(Transaction, usize)> {
        let text = view.read(cx).text(cx);
        let replacements = searcher.replacements(text, range, template);
        let count = replacements.len();
        if count == 0 {
            return None;
        }
        let edits = replacements
            .into_iter()
            .map(|(range, replacement)| Edit::replace(range, replacement));
        let transaction = Transaction::from_edits(text, edits).expect("matches do not overlap");
        Some((transaction, count))
    }

    /// Replace All: every match in the document or the selection, as one undo step.
    fn replace_all(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let Some(view) = self.active_view(cx) else {
            return;
        };
        let Some((searcher, _)) = self.searcher(cx) else {
            return;
        };
        let template = self.find_bar.read(cx).replacement(cx);
        let scope = self.scope(&view, cx);
        let edits = self.replace_all_edits(&searcher, &template, &view, scope.range.clone(), cx);
        if self.report_failure(&searcher, cx) {
            return;
        }
        let count = edits.as_ref().map_or(0, |(_, count)| *count);
        if let Some((transaction, _)) = edits
            && !view.update(cx, |view, cx| view.apply_command_edit(transaction, cx))
        {
            self.report(Some("The document is read-only".into()), true, cx);
            return;
        }
        self.report(
            Some(format!(
                "Replace All: {} replaced {}",
                plural(count, "occurrence", "occurrences"),
                scope.describe()
            )),
            count == 0,
            cx,
        );
    }

    /// Replace All in All Opened Documents: each document as one undo step; read-only ones are
    /// left as they are.
    fn replace_all_in_all(&mut self, cx: &mut Context<Self>) {
        let Some((searcher, _)) = self.searcher(cx) else {
            return;
        };
        let template = self.find_bar.read(cx).replacement(cx);
        // One view per document: views of one buffer share its text.
        let mut seen = Vec::new();
        let views: Vec<Entity<EditorView>> = self
            .all_views(cx)
            .into_iter()
            .filter(|view| {
                let buffer = view.read(cx).buffer.entity_id();
                let new = !seen.contains(&buffer);
                seen.push(buffer);
                new
            })
            .collect();
        let (mut total, mut documents, mut read_only) = (0, 0, 0);
        for view in views {
            let range = 0..view.read(cx).text(cx).len();
            let Some((transaction, count)) =
                self.replace_all_edits(&searcher, &template, &view, range, cx)
            else {
                continue;
            };
            if view.update(cx, |view, cx| view.apply_command_edit(transaction, cx)) {
                total += count;
                documents += 1;
            } else {
                read_only += 1;
            }
        }
        if self.report_failure(&searcher, cx) {
            return;
        }
        let mut message = format!(
            "Replace All in all opened documents: {} replaced in {}",
            plural(total, "occurrence", "occurrences"),
            plural(documents, "document", "documents")
        );
        if read_only > 0 {
            message.push_str(&format!(
                "; {} read-only",
                plural(read_only, "document is", "documents are")
            ));
        }
        self.report(Some(message), total == 0, cx);
    }

    /// Mark All: marks the matches in the document or the selection, and with Bookmark line
    /// bookmarks their lines. With Purge for each search, earlier marks go first.
    fn mark_all(&mut self, cx: &mut Context<Self>) {
        let Some(view) = self.active_view(cx) else {
            return;
        };
        let Some((searcher, _)) = self.searcher(cx) else {
            return;
        };
        let options = self.find_bar.read(cx).options;
        let scope = self.scope(&view, cx);
        let text = view.read(cx).text(cx).clone();
        // An empty match has nothing to mark.
        let matches: Vec<_> = searcher
            .find_all_in(&text, scope.range.clone())
            .into_iter()
            .filter(|found| !found.is_empty())
            .collect();
        if self.report_failure(&searcher, cx) {
            return;
        }
        let count = matches.len();
        let buffer = view.read(cx).buffer.clone();
        buffer.update(cx, |buffer, cx| {
            buffer.update_marks(cx, |marks, text| {
                if options.purge {
                    marks.found.clear();
                }
                if options.bookmark_line {
                    marks.bookmark_lines_of(text, &matches);
                }
                marks.found.insert_all(matches, ());
            });
        });
        self.report(
            Some(format!(
                "Mark: {} {}",
                plural(count, "match", "matches"),
                scope.describe()
            )),
            count == 0,
            cx,
        );
    }

    fn clear_marks(&mut self, cx: &mut Context<Self>) {
        let Some(view) = self.active_view(cx) else {
            return;
        };
        let buffer = view.read(cx).buffer.clone();
        buffer.update(cx, |buffer, cx| {
            buffer.update_marks(cx, |marks, _| marks.found.clear());
        });
        self.report(Some("Marks cleared".into()), false, cx);
    }

    /// Copy Marked Text: the marked texts, one per line.
    fn copy_marked(&mut self, cx: &mut Context<Self>) {
        let Some(view) = self.active_view(cx) else {
            return;
        };
        let (copied, count) = {
            let buffer = view.read(cx).buffer.read(cx);
            let ranges: Vec<_> = buffer
                .marks()
                .found
                .iter()
                .map(|(range, _)| range)
                .collect();
            let doc = buffer.doc();
            (
                ops::copy_ranges(doc.text(), ranges.iter().cloned(), doc.line_ending()),
                ranges.len(),
            )
        };
        if count > 0 {
            cx.write_to_clipboard(ClipboardItem::new_string(copied));
        }
        self.report(
            Some(format!(
                "Copied {}",
                plural(count, "marked text", "marked texts")
            )),
            count == 0,
            cx,
        );
    }

    /// Ctrl+G: Go To line or offset, as in Notepad++.
    fn open_go_to(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Result<()> {
        let view = self.active_view(cx).context("no document is open")?;
        let go_to = cx.new(|cx| GoTo::new(view, window, cx));
        let focus = go_to.read(cx).input.clone();
        window.open_dialog(cx, move |dialog, _, _| {
            let go_to = go_to.clone();
            let confirm = go_to.clone();
            dialog
                .title("Go To")
                .w(px(360.))
                .child(go_to)
                .footer(
                    DialogFooter::new()
                        .child(crate::workspace::dialog_close("Cancel"))
                        .child(crate::workspace::dialog_action(
                            Button::new("go").label("Go"),
                        )),
                )
                .on_ok(move |_, _, cx| {
                    confirm.update(cx, |go_to, cx| match go_to.go(cx) {
                        Ok(()) => true,
                        Err(error) => {
                            go_to.error = Some(error.to_string());
                            cx.notify();
                            false
                        }
                    })
                })
        });
        focus.update(cx, |input, cx| input.focus(window, cx));
        Ok(())
    }
}

/// Go To: a line number (1-based) or a byte offset (0-based, Notepad++'s "position").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GoToTarget {
    Line(usize),
    Offset(usize),
}

/// Where Go To puts the caret; out-of-range values go to the last line or the end.
pub(crate) fn go_to_position(text: &Rope, target: GoToTarget) -> usize {
    match target {
        GoToTarget::Line(line) => {
            let line = line.clamp(1, line_count(text)) - 1;
            line_range(text, line).start
        }
        GoToTarget::Offset(offset) => {
            let offset = text.floor_char_boundary(offset.min(text.len()));
            // Not between the CR and LF of a line break.
            if offset > 0 && text.byte(offset - 1) == b'\r' && text.get_byte(offset) == Some(b'\n')
            {
                offset - 1
            } else {
                offset
            }
        }
    }
}

struct GoTo {
    view: Entity<EditorView>,
    by_offset: bool,
    input: Entity<InputState>,
    error: Option<String>,
}

impl GoTo {
    fn new(view: Entity<EditorView>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            view,
            by_offset: false,
            input: cx.new(|cx| InputState::new(window, cx)),
            error: None,
        }
    }

    fn go(&mut self, cx: &mut Context<Self>) -> Result<()> {
        let value = self.input.read(cx).value();
        let number: usize = value.trim().parse().map_err(|_| anyhow!("Type a number"))?;
        let target = if self.by_offset {
            GoToTarget::Offset(number)
        } else {
            if number == 0 {
                bail!("Lines are numbered from 1");
            }
            GoToTarget::Line(number)
        };
        let text = self.view.read(cx).text(cx).clone();
        let pos = go_to_position(&text, target);
        self.view.update(cx, |view, cx| view.go_to(pos, cx));
        Ok(())
    }
}

impl Render for GoTo {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let view = self.view.read(cx);
        let text = view.text(cx);
        let head = view.selection.primary().head;
        let (here, limit) = if self.by_offset {
            (head, text.len())
        } else {
            (line_of(text, head) + 1, line_count(text))
        };
        let this = cx.entity().downgrade();
        let mode = move |by_offset: bool| {
            let this = this.clone();
            move |_: &gpui_kit::ClickEvent, _: &mut Window, cx: &mut App| {
                this.update(cx, |this, cx| {
                    this.by_offset = by_offset;
                    cx.notify();
                })
                .ok();
            }
        };
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap_2()
                    .child(
                        Button::new("by-line")
                            .small()
                            .label("Line")
                            .selected(!self.by_offset)
                            .when(!self.by_offset, |button| button.primary())
                            .on_click(mode(false)),
                    )
                    .child(
                        Button::new("by-offset")
                            .small()
                            .label("Offset")
                            .selected(self.by_offset)
                            .when(self.by_offset, |button| button.primary())
                            .on_click(mode(true)),
                    ),
            )
            .child(format!("You are here: {here}"))
            .child(Input::new(&self.input).id("go-to-input"))
            .child(format!("You can't go further than: {limit}"))
            .children(self.error.clone().map(|error| {
                div()
                    .text_color(crate::theme::paint(crate::theme::ui().error))
                    .child(error)
            }))
    }
}

#[cfg(test)]
#[path = "find_mouse_tests.rs"]
mod mouse_tests;

#[cfg(test)]
mod tests {
    use birchpad_core::{Range, Selection};
    use gpui_kit::{TestAppContext, VisualTestContext};

    use super::*;
    use crate::workspace::tests::{active_text, document_start, open_workspace, secondary};

    fn search_for(
        workspace: &Entity<Workspace>,
        pattern: &str,
        options: FindOptions,
        cx: &mut VisualTestContext,
    ) {
        workspace.update_in(cx, |workspace, window, cx| {
            workspace.find_bar.update(cx, |bar, cx| {
                bar.show(FindTab::Replace, Some(pattern.to_owned()), None, window, cx);
                bar.options = options;
            });
        });
    }

    fn set_replacement(workspace: &Entity<Workspace>, text: &str, cx: &mut VisualTestContext) {
        workspace.update_in(cx, |workspace, window, cx| {
            workspace.find_bar.update(cx, |bar, cx| {
                bar.replace_input
                    .update(cx, |input, cx| input.set_value(text, window, cx));
            });
        });
    }

    fn act(
        workspace: &Entity<Workspace>,
        event: FindBarEvent,
        cx: &mut VisualTestContext,
    ) -> Option<String> {
        workspace.update_in(cx, |workspace, window, cx| {
            let bar = workspace.find_bar.clone();
            workspace.on_find_bar_event(&bar, &event, window, cx);
            bar.read(cx).status.clone().map(|(message, _)| message)
        })
    }

    fn selection(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> (usize, usize) {
        workspace.read_with(cx, |workspace, cx| {
            let range = workspace
                .active_view(cx)
                .unwrap()
                .read(cx)
                .selection
                .primary();
            (range.anchor, range.head)
        })
    }

    fn select(
        workspace: &Entity<Workspace>,
        anchor: usize,
        head: usize,
        cx: &mut VisualTestContext,
    ) {
        workspace.update_in(cx, |workspace, _, cx| {
            let view = workspace.active_view(cx).unwrap();
            view.update(cx, |view, cx| {
                view.selection = Selection::single(Range::new(anchor, head));
                cx.notify();
            });
        });
    }

    fn regex() -> FindOptions {
        FindOptions {
            mode: SearchMode::Regex,
            match_case: true,
            ..FindOptions::default()
        }
    }

    #[gpui_kit::test]
    fn f3_finds_next_and_wraps(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        cx.simulate_input("cat Cat category cat");
        cx.simulate_keystrokes(document_start());
        let options = FindOptions {
            whole_word: true,
            ..FindOptions::default()
        };
        search_for(&workspace, "cat", options, cx);
        cx.simulate_keystrokes("f3");
        assert_eq!(selection(&workspace, cx), (0, 3));
        cx.simulate_keystrokes("f3");
        assert_eq!(selection(&workspace, cx), (4, 7), "case-insensitive");
        cx.simulate_keystrokes("f3");
        assert_eq!(
            selection(&workspace, cx),
            (17, 20),
            "whole word skips category"
        );
        cx.simulate_keystrokes("f3");
        assert_eq!(selection(&workspace, cx), (0, 3), "wrapped around");
        cx.simulate_keystrokes("shift-f3");
        assert_eq!(selection(&workspace, cx), (17, 20), "backward wraps too");
    }

    #[gpui_kit::test]
    fn replace_all_is_one_undo_step(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        cx.simulate_input("a-b-c-d");
        search_for(&workspace, "-", FindOptions::default(), cx);
        set_replacement(&workspace, "+", cx);
        let status = act(&workspace, FindBarEvent::ReplaceAll, cx);
        assert_eq!(active_text(&workspace, cx), "a+b+c+d");
        assert_eq!(
            status.as_deref(),
            Some("Replace All: 3 occurrences replaced in entire file")
        );
        workspace.update_in(cx, |workspace, window, cx| {
            workspace.close_find(window, cx);
        });
        cx.simulate_keystrokes(&secondary("z"));
        assert_eq!(active_text(&workspace, cx), "a-b-c-d");
    }

    #[gpui_kit::test]
    fn regular_expressions_replace_with_groups(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        cx.simulate_input("a=1, b=22");
        search_for(&workspace, r"(\w)=(\d+)", regex(), cx);
        set_replacement(&workspace, r"$2:\U$1", cx);
        act(&workspace, FindBarEvent::ReplaceAll, cx);
        assert_eq!(active_text(&workspace, cx), "1:A, 22:B");

        // Replace replaces the selected match with its own groups, then finds the next one.
        cx.simulate_keystrokes(document_start());
        search_for(&workspace, r"(\d+):(\w)", regex(), cx);
        set_replacement(&workspace, "$2", cx);
        cx.simulate_keystrokes("f3");
        assert_eq!(selection(&workspace, cx), (0, 3));
        act(&workspace, FindBarEvent::Replace, cx);
        assert_eq!(active_text(&workspace, cx), "A, 22:B");
        assert_eq!(selection(&workspace, cx), (3, 7), "the next match");
    }

    #[gpui_kit::test]
    fn in_selection_limits_replace_all_and_count(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        cx.simulate_input("x x x\nx x x");
        let in_selection = FindOptions {
            in_selection: true,
            ..FindOptions::default()
        };
        search_for(&workspace, "x", in_selection, cx);
        select(&workspace, 6, 11, cx);
        let status = act(&workspace, FindBarEvent::Count, cx);
        assert_eq!(status.as_deref(), Some("Count: 3 matches in selection"));
        set_replacement(&workspace, "y", cx);
        act(&workspace, FindBarEvent::ReplaceAll, cx);
        assert_eq!(active_text(&workspace, cx), "x x x\ny y y");

        // Opening the panel with a selection over several lines turns In selection on.
        select(&workspace, 0, 11, cx);
        cx.simulate_keystrokes(&secondary("h"));
        let options = workspace.read_with(cx, |workspace, cx| workspace.find_bar.read(cx).options);
        assert!(options.in_selection);
    }

    #[gpui_kit::test]
    fn find_next_steps_over_empty_matches(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        cx.simulate_input("x\ny\nz");
        cx.simulate_keystrokes(document_start());
        search_for(&workspace, "^", regex(), cx);
        let mut starts = Vec::new();
        for _ in 0..4 {
            cx.simulate_keystrokes("f3");
            starts.push(selection(&workspace, cx).0);
        }
        assert_eq!(starts, [2, 4, 0, 2], "line starts, wrapping around");
        let status = act(&workspace, FindBarEvent::Count, cx);
        assert_eq!(status.as_deref(), Some("Count: 3 matches in entire file"));
    }

    #[gpui_kit::test]
    fn extended_mode_finds_line_breaks(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        cx.simulate_input("a\tb");
        cx.simulate_keystrokes(document_start());
        let extended = FindOptions {
            mode: SearchMode::Extended,
            ..FindOptions::default()
        };
        search_for(&workspace, r"a\tb", extended, cx);
        cx.simulate_keystrokes("f3");
        assert_eq!(selection(&workspace, cx), (0, 3));
        search_for(&workspace, "(", regex(), cx);
        let status = act(&workspace, FindBarEvent::Count, cx);
        assert!(
            status
                .as_deref()
                .is_some_and(|status| status.starts_with("Find: invalid regular expression")),
            "{status:?}"
        );
    }

    #[gpui_kit::test]
    fn mark_all_marks_bookmarks_purges_and_copies(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        cx.simulate_input("foo bar\nbaz foo\nqux");
        let marking = FindOptions {
            bookmark_line: true,
            ..FindOptions::default()
        };
        search_for(&workspace, "foo", marking, cx);
        let status = act(&workspace, FindBarEvent::MarkAll, cx);
        assert_eq!(status.as_deref(), Some("Mark: 2 matches in entire file"));
        let marks = |cx: &mut VisualTestContext| {
            workspace.read_with(cx, |workspace, cx| {
                let view = workspace.active_view(cx).unwrap();
                let buffer = view.read(cx).buffer.read(cx);
                let found: Vec<_> = buffer.marks().found.iter().map(|(r, _)| r).collect();
                (found, buffer.bookmark_lines())
            })
        };
        assert_eq!(marks(cx), (vec![0..3, 12..15], vec![0, 1]));

        let line_ending = workspace.read_with(cx, |workspace, cx| {
            let view = workspace.active_view(cx).unwrap();
            view.read(cx).buffer.read(cx).doc().line_ending().as_str()
        });
        act(&workspace, FindBarEvent::CopyMarked, cx);
        let copied = cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()));
        assert_eq!(copied, Some(format!("foo{line_ending}foo{line_ending}")));

        // Purge for each search: the next Mark All replaces the marks.
        let purging = FindOptions {
            purge: true,
            ..FindOptions::default()
        };
        search_for(&workspace, "ba", purging, cx);
        act(&workspace, FindBarEvent::MarkAll, cx);
        assert_eq!(marks(cx).0, [4..6, 8..10]);
        act(&workspace, FindBarEvent::ClearMarks, cx);
        assert!(marks(cx).0.is_empty());
    }

    #[gpui_kit::test]
    fn replace_all_in_all_opened_documents(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        cx.simulate_input("a a");
        cx.simulate_keystrokes(&secondary("n"));
        cx.simulate_input("a");
        search_for(&workspace, "a", FindOptions::default(), cx);
        set_replacement(&workspace, "b", cx);
        let status = act(&workspace, FindBarEvent::ReplaceAllInAll, cx);
        assert_eq!(
            status.as_deref(),
            Some("Replace All in all opened documents: 3 occurrences replaced in 2 documents")
        );
        let texts = workspace.read_with(cx, |workspace, cx| {
            workspace
                .all_views(cx)
                .iter()
                .map(|view| view.read(cx).text(cx).to_string())
                .collect::<Vec<_>>()
        });
        assert_eq!(texts, ["b b", "b"]);
    }

    #[gpui_kit::test]
    fn select_and_find_next_takes_the_word_at_the_caret(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        cx.simulate_input("foo bar foo");
        cx.simulate_keystrokes(document_start());
        cx.simulate_keystrokes("right");
        cx.simulate_keystrokes(&secondary("f3"));
        assert_eq!(selection(&workspace, cx), (8, 11));
        cx.simulate_keystrokes(&secondary("shift-f3"));
        assert_eq!(selection(&workspace, cx), (0, 3));
    }

    #[gpui_kit::test]
    fn find_all_lists_lines_and_f4_walks_them(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        cx.simulate_input("foo\nbar foo\nfoo foo");
        cx.simulate_keystrokes(&secondary("n"));
        cx.simulate_input("no match\nfoo");
        search_for(&workspace, "foo", FindOptions::default(), cx);
        let status = act(
            &workspace,
            FindBarEvent::FindAll {
                all_documents: true,
            },
            cx,
        );
        assert_eq!(status.as_deref(), Some("Find All: 5 hits"));
        let rows = workspace.read_with(cx, |workspace, cx| {
            workspace.search_results.read(cx).rows_text()
        });
        assert_eq!(
            rows,
            [
                "Search \"foo\" (5 hits in 2 files of 2 searched)",
                "  new 2 (1)",
                "    Line 2: foo",
                "  new 1 (4)",
                "    Line 1: foo",
                "    Line 2: bar foo",
                "    Line 3: foo foo",
            ],
            "the active document first, one row per line"
        );
        let active = |cx: &mut VisualTestContext| {
            workspace.read_with(cx, |workspace, cx| {
                let view = workspace.active_view(cx).unwrap();
                let view = view.read(cx);
                let range = view.selection.primary();
                (
                    view.buffer.read(cx).display_name(),
                    range.anchor,
                    range.head,
                )
            })
        };
        cx.simulate_keystrokes("f4");
        assert_eq!(active(cx), ("new 2".to_owned(), 9, 12));
        cx.simulate_keystrokes("f4");
        assert_eq!(active(cx), ("new 1".to_owned(), 0, 3), "into the other tab");
        cx.simulate_keystrokes("f4 f4");
        assert_eq!(
            active(cx),
            ("new 1".to_owned(), 12, 15),
            "the first match of the line"
        );
        cx.simulate_keystrokes("shift-f4");
        assert_eq!(active(cx), ("new 1".to_owned(), 8, 11));
    }

    #[test]
    fn go_to_clamps_like_notepad_plus_plus() {
        let text = Rope::from_str("one\r\ntwo\nthree");
        assert_eq!(go_to_position(&text, GoToTarget::Line(2)), 5);
        assert_eq!(go_to_position(&text, GoToTarget::Line(99)), 9);
        assert_eq!(
            go_to_position(&text, GoToTarget::Offset(4)),
            3,
            "not inside CRLF"
        );
        assert_eq!(go_to_position(&text, GoToTarget::Offset(999)), text.len());
        let cyrillic = Rope::from_str("жж");
        assert_eq!(
            go_to_position(&cyrillic, GoToTarget::Offset(1)),
            0,
            "char boundary"
        );
    }
}
