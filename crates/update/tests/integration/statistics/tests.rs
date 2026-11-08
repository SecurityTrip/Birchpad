use birchpad_update::statistics::*;

#[test]
fn install_ids_are_random_version_4_uuids() {
    let id = new_install_id();
    assert!(is_install_id(&id), "{id}");
    assert_eq!(id.len(), 36);
    assert_eq!(id.as_bytes()[14], b'4', "version 4: {id}");
    assert_ne!(
        new_install_id(),
        id,
        "random, not derived from the computer"
    );
    assert!(is_install_id("0b3c6f1e-6a1f-4c4e-9a59-3c1f7f0d2a11"));
    for wrong in [
        "",
        "not an id",
        // Version 1 and the nil UUID.
        "0b3c6f1e-6a1f-1c4e-9a59-3c1f7f0d2a11",
        "00000000-0000-0000-0000-000000000000",
        // A character short, and one too many.
        "0b3c6f1e-6a1f-4c4e-9a59-3c1f7f0d2a1",
        "0b3c6f1e-6a1f-4c4e-9a59-3c1f7f0d2a111",
    ] {
        assert!(!is_install_id(wrong), "{wrong}");
    }
}

#[test]
fn an_install_id_is_made_once_and_kept() {
    let mut id = None;
    assert!(keep_install_id(&mut id, true), "made: to be saved");
    let made = id.clone().unwrap();
    assert!(is_install_id(&made));
    for _ in 0..3 {
        assert!(!keep_install_id(&mut id, true), "kept: nothing to save");
        assert_eq!(id.as_deref(), Some(made.as_str()));
    }
}

#[test]
fn turning_statistics_off_deletes_the_id_and_on_again_makes_another() {
    let mut id = None;
    keep_install_id(&mut id, true);
    let first = id.clone().unwrap();
    assert!(keep_install_id(&mut id, false), "deleted: to be saved");
    assert_eq!(id, None);
    assert!(!keep_install_id(&mut id, false), "none stays none");
    assert!(keep_install_id(&mut id, true));
    let second = id.unwrap();
    assert!(is_install_id(&second));
    assert_ne!(second, first, "a new id, not the old one");
}

#[test]
fn what_is_no_install_id_is_replaced() {
    for wrong in [
        "",
        " ",
        "my-computer",
        "0b3c6f1e-6a1f-1c4e-9a59-3c1f7f0d2a11",
    ] {
        let mut id = Some(wrong.to_owned());
        assert!(keep_install_id(&mut id, true), "{wrong:?}");
        assert!(is_install_id(id.as_deref().unwrap()), "{wrong:?}");
        let mut id = Some(wrong.to_owned());
        assert!(keep_install_id(&mut id, false), "{wrong:?}");
        assert_eq!(id, None);
    }
}
