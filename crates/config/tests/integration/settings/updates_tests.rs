//! The settings of Preferences > Updates: install statistics (ADR 0064).

use birchpad_config::*;
use toml::Table;

fn table(toml: &str) -> Option<Table> {
    Some(toml::from_str(toml).unwrap())
}

#[test]
fn statistics_are_on_unless_turned_off() {
    assert!(UpdateSettings::default().statistics);
    assert!(Settings::default().updates.statistics);
    // Other update settings given, statistics not: still on.
    let settings: Settings = toml::from_str("[updates]\nmode = \"notify\"\n").unwrap();
    assert!(settings.updates.statistics);
    let settings: Settings = toml::from_str("[updates]\nstatistics = false\n").unwrap();
    assert!(!settings.updates.statistics);
    check_setting("updates.statistics", &toml::Value::Boolean(false)).unwrap();
    // Not a boolean: refused, and the default stays.
    for wrong in [toml::Value::String("false".into()), toml::Value::Integer(0)] {
        assert!(
            check_setting("updates.statistics", &wrong).is_err(),
            "{wrong}"
        );
    }
    let resolved = resolve(Sources {
        user: table("updates.statistics = 'no'"),
        ..Sources::default()
    });
    assert!(resolved.settings.updates.statistics);
}

#[test]
fn the_statistics_policy_wins_over_the_setting() {
    let resolved = resolve(Sources {
        machine: table("updates.statistics = true"),
        user: table("updates.statistics = true"),
        policy: table("updates.statistics = false"),
        ..Sources::default()
    });
    assert!(!resolved.settings.updates.statistics);
    assert!(resolved.is_locked("updates.statistics"));
    // Without the policy the user's setting holds, and is not locked.
    let resolved = resolve(Sources {
        user: table("updates.statistics = false"),
        ..Sources::default()
    });
    assert!(!resolved.settings.updates.statistics);
    assert!(!resolved.is_locked("updates.statistics"));
    // The policy may turn them on too, over a user who turned them off.
    let resolved = resolve(Sources {
        user: table("updates.statistics = false"),
        policy: table("updates.statistics = true"),
        ..Sources::default()
    });
    assert!(resolved.settings.updates.statistics);
}

#[test]
fn the_statistics_policy_is_a_dword_of_updates_statistics() {
    let policy = POLICIES
        .iter()
        .find(|policy| policy.name == "UpdateStatistics")
        .expect("listed, so that templates have it");
    assert_eq!(policy.key, "updates.statistics");
    assert_eq!(policy.kind, PolicyKind::Bool);
    // Names and keys stay unique.
    for (index, policy) in POLICIES.iter().enumerate() {
        assert!(
            POLICIES[index + 1..]
                .iter()
                .all(|other| other.name != policy.name && other.key != policy.key),
            "{}",
            policy.name
        );
    }
}
