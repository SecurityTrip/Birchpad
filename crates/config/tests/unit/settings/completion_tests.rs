use super::*;

#[test]
fn pairs_of_auto_close() {
    let mut settings = CompletionSettings::default();
    assert_eq!(
        settings.pairs(),
        [('(', ')'), ('[', ']'), ('{', '}'), ('"', '"'), ('\'', '\'')]
    );
    settings.auto_close = "<>(".into();
    assert_eq!(settings.pairs(), [('<', '>')]);
    settings.auto_close.clear();
    assert!(settings.pairs().is_empty());
    settings.auto_close = "«»".into();
    assert_eq!(settings.pairs(), [('«', '»')]);
}
