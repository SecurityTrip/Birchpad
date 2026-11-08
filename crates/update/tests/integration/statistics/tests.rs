use birchpad_config::UpdateChannel;
use birchpad_update::statistics::*;

fn statistics(plantuml: PlantUml) -> Statistics {
    Statistics {
        install_id: "0b3c6f1e-6a1f-4c4e-9a59-3c1f7f0d2a11".into(),
        version: "0.8.0-nightly.20261110.5".into(),
        channel: UpdateChannel::Nightly,
        platform: Some("linux-x64".into()),
        plantuml,
    }
}

fn body(statistics: Option<&Statistics>) -> String {
    String::from_utf8(request_body(statistics)).unwrap()
}

/// The keys of the JSON object `body`.
fn keys(body: &str) -> Vec<String> {
    let json: serde_json::Value = serde_json::from_str(body).unwrap();
    json.as_object().unwrap().keys().cloned().collect()
}

#[test]
fn without_statistics_the_body_is_an_empty_object() {
    assert_eq!(request_body(None), b"{}");
}

#[test]
fn the_body_has_the_contract_s_fields_and_their_names() {
    let component = PlantUml::Component {
        version: "1.2026.9-3".into(),
    };
    assert_eq!(
        body(Some(&statistics(component))),
        concat!(
            r#"{"installId":"0b3c6f1e-6a1f-4c4e-9a59-3c1f7f0d2a11","#,
            r#""version":"0.8.0-nightly.20261110.5","channel":"nightly","platform":"linux-x64","#,
            r#""plantuml":"component","plantumlVersion":"1.2026.9-3"}"#
        )
    );
    // Only the component has a version.
    for (plantuml, name) in [(PlantUml::Custom, "custom"), (PlantUml::None, "none")] {
        let json: serde_json::Value =
            serde_json::from_str(&body(Some(&statistics(plantuml)))).unwrap();
        assert_eq!(json["plantuml"], name);
        assert!(json.get("plantumlVersion").is_none(), "{name}");
    }
    for (channel, name) in [
        (UpdateChannel::Stable, "stable"),
        (UpdateChannel::Beta, "beta"),
        (UpdateChannel::Nightly, "nightly"),
    ] {
        let mut stats = statistics(PlantUml::None);
        stats.channel = channel;
        let json: serde_json::Value = serde_json::from_str(&body(Some(&stats))).unwrap();
        assert_eq!(json["channel"], name);
    }
}

#[test]
fn empty_fields_are_left_out() {
    let stats = Statistics {
        install_id: String::new(),
        version: " ".into(),
        channel: UpdateChannel::Stable,
        platform: None,
        plantuml: PlantUml::Component {
            version: String::new(),
        },
    };
    assert_eq!(
        body(Some(&stats)),
        r#"{"channel":"stable","plantuml":"component"}"#
    );
    let mut stats = statistics(PlantUml::None);
    stats.platform = Some(String::new());
    assert!(!keys(&body(Some(&stats))).contains(&"platform".to_owned()));
    // Every key is one of the contract's.
    let contract = [
        "installId",
        "version",
        "channel",
        "platform",
        "plantuml",
        "plantumlVersion",
    ];
    let full = statistics(PlantUml::Component {
        version: "1.2026.9-3".into(),
    });
    let keys = keys(&body(Some(&full)));
    assert_eq!(keys.len(), contract.len());
    assert!(
        keys.iter().all(|key| contract.contains(&key.as_str())),
        "{keys:?}"
    );
}

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
