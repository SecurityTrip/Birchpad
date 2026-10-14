use birchpad_commands::Invocation;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};

use super::system::{FAKE, Fake};
use super::*;
use crate::workspace::Workspace;
use crate::workspace::tests::open_workspace;

fn start(fake: Fake, cx: &mut TestAppContext) -> (Entity<Workspace>, &mut VisualTestContext) {
    FAKE.with(|current| *current.borrow_mut() = fake);
    open_workspace(cx)
}

fn offer_now(cx: &mut VisualTestContext) {
    cx.update(offer);
    cx.run_until_parked();
}

fn registered() -> usize {
    FAKE.with(|fake| fake.borrow().registered)
}

fn opened() -> Vec<String> {
    FAKE.with(|fake| fake.borrow().opened.clone())
}

#[gpui_kit::test]
fn set_up_from_the_offer(cx: &mut TestAppContext) {
    let (_, cx) = start(Fake::default(), cx);
    offer_now(cx);
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Set Up");
    cx.run_until_parked();
    assert_eq!(registered(), 1);
    // Windows' Default apps, where the user picks Birchpad; elsewhere it is done.
    let page = birchpad_cli::associations::settings_page().map(str::to_owned);
    assert_eq!(opened(), page.into_iter().collect::<Vec<_>>());
}

#[gpui_kit::test]
fn not_now_and_do_not_ask_again(cx: &mut TestAppContext) {
    let (_, cx) = start(Fake::default(), cx);
    offer_now(cx);
    cx.simulate_prompt_answer("Not Now");
    cx.run_until_parked();
    assert_eq!(registered(), 0);
    // Asked again next time.
    offer_now(cx);
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Don't Ask Again");
    cx.run_until_parked();
    assert_eq!(registered(), 0);
    assert!(cx.update(|_, cx| AppState::global(cx).state.default_app_declined));
    offer_now(cx);
    assert!(!cx.has_pending_prompt(), "not asked again");
}

#[gpui_kit::test]
fn nothing_is_offered_when_there_is_nothing_to_offer(cx: &mut TestAppContext) {
    // Already the default.
    let (_, cx) = start(
        Fake {
            default: true,
            ..Fake::default()
        },
        cx,
    );
    offer_now(cx);
    assert!(!cx.has_pending_prompt());
    // Nothing to register on this system (macOS: Finder chooses).
    FAKE.with(|fake| {
        *fake.borrow_mut() = Fake {
            unsupported: true,
            ..Fake::default()
        }
    });
    offer_now(cx);
    assert!(!cx.has_pending_prompt());
    // The setting is off.
    FAKE.with(|fake| *fake.borrow_mut() = Fake::default());
    cx.update(|_, cx| {
        cx.global_mut::<AppState>().settings.files.offer_default_app = false;
    });
    offer_now(cx);
    assert!(!cx.has_pending_prompt());
}

#[gpui_kit::test]
fn the_settings_menu_sets_it_up_at_any_time(cx: &mut TestAppContext) {
    let (workspace, cx) = start(Fake::default(), cx);
    let run = |cx: &mut VisualTestContext| {
        let result = workspace.update_in(cx, |workspace, window, cx| {
            workspace.dispatch(&Invocation::new("settings.default-app"), window, cx)
        });
        cx.run_until_parked();
        result
    };
    run(cx).unwrap();
    run(cx).unwrap();
    assert_eq!(registered(), 2, "again whenever asked");

    // Refused by the system: an error that says so.
    FAKE.with(|fake| fake.borrow_mut().refuse = true);
    let error = run(cx).unwrap_err();
    assert!(error.to_string().contains("access denied"), "{error}");

    // Where there is nothing to register, the command is off.
    FAKE.with(|fake| {
        *fake.borrow_mut() = Fake {
            unsupported: true,
            ..Fake::default()
        }
    });
    assert!(run(cx).is_err());
    assert_eq!(registered(), 0);
}
