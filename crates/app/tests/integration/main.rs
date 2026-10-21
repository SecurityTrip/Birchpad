//! The crate's tests through its public interface, one module per part of `src`, in one
//! test binary.

#[path = "command_line_tests.rs"]
mod command_line_tests;

#[path = "commands/tests.rs"]
mod commands;

#[path = "compare/tests.rs"]
mod compare;

#[path = "context_menu/tests.rs"]
mod context_menu;

#[path = "default_app/tests.rs"]
mod default_app;

#[path = "disk_tests.rs"]
mod disk_tests;

#[path = "edit_extras/tests.rs"]
mod edit_extras;

#[path = "editor_tests.rs"]
mod editor_tests;

#[path = "encoding_ui/tests.rs"]
mod encoding_ui;

#[path = "extensions/admin/tests.rs"]
mod extensions_admin;

#[path = "extensions/plugins/tests.rs"]
mod extensions_plugins;

#[path = "extensions/tests.rs"]
mod extensions;

#[path = "file_commands/tests.rs"]
mod file_commands;

#[path = "file_ops/tests.rs"]
mod file_ops;

#[path = "find/tests.rs"]
mod find;

#[path = "find_in_files_tests.rs"]
mod find_in_files_tests;

#[path = "find_mouse_tests.rs"]
mod find_mouse_tests;

#[path = "help/tests.rs"]
mod help;

#[path = "hex_editor/tests.rs"]
mod hex_editor;

#[path = "hex_view/tests.rs"]
mod hex_view;

#[path = "i18n/tests.rs"]
mod i18n;

#[path = "incremental/tests.rs"]
mod incremental;

#[path = "json_xml/tests.rs"]
mod json_xml;

#[path = "language_tests.rs"]
mod language_tests;

#[path = "macros/tests.rs"]
mod macros;

#[path = "menus/model_tests.rs"]
mod menus_model_tests;

#[cfg(not(target_os = "macos"))]
#[path = "menus/tests.rs"]
mod menus;

#[path = "navigation_tests.rs"]
mod navigation_tests;

#[path = "pane/tests.rs"]
mod pane;

#[path = "panels/character_panel/tests.rs"]
mod panels_character_panel;

#[path = "panels/clipboard_history/tests.rs"]
mod panels_clipboard_history;

#[path = "panels/document_map_tests.rs"]
mod panels_document_map_tests;

#[path = "panels/folder_workspace_tests.rs"]
mod panels_folder_workspace_tests;

#[path = "panels/function_list_tests.rs"]
mod panels_function_list_tests;

#[path = "panels/project_tests.rs"]
mod panels_project_tests;

#[path = "panels/tests.rs"]
mod panels;

#[path = "path_dialog/tests.rs"]
mod path_dialog;

#[path = "plantuml_tests.rs"]
mod plantuml_tests;

#[path = "preferences/dialog_tests.rs"]
mod preferences_dialog_tests;

#[path = "preferences/more_tests.rs"]
mod preferences_more_tests;

#[path = "preferences/tests.rs"]
mod preferences;

#[path = "preview/tests.rs"]
mod preview;

#[path = "print/tests.rs"]
mod print;

#[path = "run_menu/tests.rs"]
mod run_menu;

#[path = "search_extras/tests.rs"]
mod search_extras;

#[path = "search_results/tests.rs"]
mod search_results;

#[path = "session_tests.rs"]
mod session_tests;

#[path = "shortcut_mapper/dialog_tests.rs"]
mod shortcut_mapper_dialog_tests;

#[path = "shortcut_mapper/tests.rs"]
mod shortcut_mapper;

#[path = "spell_check/tests.rs"]
mod spell_check;

#[path = "split_tests.rs"]
mod split_tests;

#[path = "status_bar/tests.rs"]
mod status_bar;

#[path = "style_configurator/tests.rs"]
mod style_configurator;

#[path = "themes/interface_tests.rs"]
mod themes_interface_tests;

#[path = "themes/tests.rs"]
mod themes;

#[path = "toolbar/tests.rs"]
mod toolbar;

#[path = "tools/tests.rs"]
mod tools;

#[path = "updates/tests.rs"]
mod updates;

#[path = "user_languages/define_tests.rs"]
mod user_languages_define_tests;

#[path = "user_languages/tests.rs"]
mod user_languages;

#[path = "view_extras/tests.rs"]
mod view_extras;

#[path = "window_menu/tests.rs"]
mod window_menu;

#[path = "window_modes/tests.rs"]
mod window_modes;

#[path = "workspace/tests.rs"]
mod workspace;
