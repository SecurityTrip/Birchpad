use super::*;

#[test]
fn up_to_the_limit_passes() {
    within("nothing", Duration::ZERO, Duration::ZERO);
    within(
        "exactly",
        Duration::from_millis(5),
        Duration::from_millis(5),
    );
}

#[test]
#[should_panic(expected = "more than its limit")]
fn past_the_limit_fails() {
    within(
        "too slow",
        Duration::from_millis(6),
        Duration::from_millis(5),
    );
}
