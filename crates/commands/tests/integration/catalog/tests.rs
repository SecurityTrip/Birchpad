use birchpad_commands::*;
use std::collections::HashSet;

#[test]
fn ids_are_unique_and_well_formed() {
    let mut seen = HashSet::new();
    for spec in COMMANDS {
        assert!(seen.insert(spec.id), "duplicate command id {}", spec.id);
        let (group, name) = spec.id.split_once('.').expect("ids are `group.name`");
        for part in [group, name] {
            assert!(
                !part.is_empty()
                    && part
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                "id {} must be lowercase kebab-case",
                spec.id
            );
        }
    }
}

#[test]
fn entries_carry_their_scope() {
    let new = find("file.new").expect("File > New is a command");
    assert_eq!(
        (new.id, new.title, new.scope),
        ("file.new", "New", Scope::Workspace)
    );
    let undo = find("edit.undo").expect("Edit > Undo is a command");
    assert_eq!(
        (undo.id, undo.title, undo.scope),
        ("edit.undo", "Undo", Scope::Editor)
    );
    // Neither a group alone, nor an empty id, nor another case finds anything.
    for unknown in ["", "file", "file.", "FILE.NEW", "file.new "] {
        assert_eq!(find(unknown), None, "{unknown:?}");
    }
}

#[test]
fn every_cursor_motion_has_a_select_twin() {
    for spec in COMMANDS {
        if let Some(motion) = spec.id.strip_prefix("cursor.") {
            assert!(
                find(&format!("select.{motion}")).is_some(),
                "missing select.{motion}"
            );
        }
    }
}
