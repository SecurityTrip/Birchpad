//! Every setting the dialog shows, a module per section; `SECTIONS` lists them in the order
//! of the dialog.

mod appearance;
mod auto_completion;
mod backup;
mod cloud_and_link;
mod command_palette;
mod compare;
mod default_directory;
mod delimiter;
mod editing;
mod files;
mod go_to_file;
mod hex_editor;
mod highlighting;
mod language;
mod margins;
mod misc;
mod multi_instance_and_date;
mod new_document;
mod performance;
mod plugins;
mod preview;
mod print;
mod search_engine;
mod searching;
mod spell_checker;
mod tab_bar;
mod toolbar;
mod updates;

use super::Entry;

/// The sections' settings, in the order the dialog lists the sections.
const SECTIONS: &[&[Entry]] = &[
    appearance::ENTRIES,
    toolbar::ENTRIES,
    tab_bar::ENTRIES,
    new_document::ENTRIES,
    language::ENTRIES,
    default_directory::ENTRIES,
    multi_instance_and_date::ENTRIES,
    cloud_and_link::ENTRIES,
    delimiter::ENTRIES,
    search_engine::ENTRIES,
    editing::ENTRIES,
    auto_completion::ENTRIES,
    margins::ENTRIES,
    highlighting::ENTRIES,
    print::ENTRIES,
    preview::ENTRIES,
    files::ENTRIES,
    searching::ENTRIES,
    backup::ENTRIES,
    go_to_file::ENTRIES,
    command_palette::ENTRIES,
    performance::ENTRIES,
    misc::ENTRIES,
    spell_checker::ENTRIES,
    hex_editor::ENTRIES,
    compare::ENTRIES,
    plugins::ENTRIES,
    updates::ENTRIES,
];

const COUNT: usize = count(SECTIONS);

/// Every setting the dialog shows, by section. View > Word Wrap and Show Symbol stay in the
/// View menu, which remembers them.
pub const ENTRIES: &[Entry] = &concat::<COUNT>(SECTIONS);

/// How many settings `sections` have.
const fn count(sections: &[&[Entry]]) -> usize {
    let (mut count, mut section) = (0, 0);
    while section < sections.len() {
        count += sections[section].len();
        section += 1;
    }
    count
}

/// The settings of `sections` one after another, `N` in all.
const fn concat<const N: usize>(sections: &[&[Entry]]) -> [Entry; N] {
    let mut all = [sections[0][0]; N];
    let (mut at, mut section, mut index) = (0, 0, 0);
    while section < sections.len() {
        if index < sections[section].len() {
            all[at] = sections[section][index];
            at += 1;
            index += 1;
        } else {
            section += 1;
            index = 0;
        }
    }
    assert!(at == N);
    all
}
