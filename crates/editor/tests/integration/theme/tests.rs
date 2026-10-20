use birchpad_editor::theme::*;
use birchpad_syntax::Highlight;
use birchpad_theme::Theme;
use gpui_kit::rgba;

#[test]
fn styles_follow_the_theme_and_its_languages() {
    let keyword = Highlight::named("keyword").unwrap();
    let default = syntax_style(Some("python"), keyword).unwrap();
    assert_eq!(default.color, Some(0x0000ffff));
    assert!(default.bold);
    let before = generation();
    set(Theme::from_toml(
            "Mine",
            "[syntax]\nkeyword = '#111111'\n[language.cs]\nkeyword = { color = '#222222', italic = true }\n",
        )
        .unwrap());
    assert_ne!(generation(), before);
    assert_eq!(syntax_style(None, keyword).unwrap().color, Some(0x111111ff));
    assert_eq!(
        syntax_style(Some("python"), keyword).unwrap().color,
        Some(0x111111ff)
    );
    // Found by the Notepad++ name of the language.
    let cs = syntax_style(birchpad_syntax::by_id("cs").map(|l| l.id), keyword).unwrap();
    assert_eq!(cs.color, Some(0x222222ff));
    assert!(cs.italic);
    // A highlight without a style, and a language without styles of its own.
    assert_eq!(
        syntax_style(Some("cs"), Highlight::named("variable").unwrap()),
        None
    );
    assert_eq!(current().name, "Mine");
    set(Theme::default_theme().clone());
    assert_eq!(editor(), Theme::default_theme().editor);
    assert_eq!(ui(), Theme::default_theme().ui);
}

#[test]
fn braces_and_marks() {
    assert_eq!(brace_match().color, Some(0xff0000ff));
    assert!(brace_match().bold && brace_bad().bold);
    assert_eq!(brace_bad().color, Some(0x800000ff));
    assert_eq!(mark_style(0), Paint::Fill(rgba(0x00ffff66)));
    assert_eq!(mark_style(4), Paint::Fill(rgba(0x00800066)));
}
