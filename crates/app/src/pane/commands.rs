//! The commands of tabs: moving, pinning and coloring them, and the tab's context menu run on
//! its document.

use birchpad_commands::Invocation;

use gpui_kit::{Context, Entity, Window};

use crate::commands::{CommandRegistry, Register as _};
use crate::editor::EditorView;
use crate::workspace::Workspace;

use birchpad_editor::{TAB_COLORS, TabMarks};

pub fn register_commands(registry: &mut CommandRegistry) {
    registry.workspace("view.go-to-tab", |this, args: IndexArgs, window, cx| {
        if args.index == 0 {
            anyhow::bail!("{}", t!("pane.tabs-are-counted-from-1"));
        }
        this.active_pane()
            .update(cx, |pane, cx| pane.activate(args.index - 1, window, cx));
        Ok(())
    });
    registry.workspace("view.last-tab", |this, (), window, cx| {
        this.active_pane().update(cx, |pane, cx| {
            if let Some(last) = pane.items.len().checked_sub(1) {
                pane.activate(last, window, cx);
            }
        });
        Ok(())
    });
    for (id, step) in [
        ("view.move-tab-forward", Step::Forward),
        ("view.move-tab-backward", Step::Backward),
        ("view.move-tab-to-start", Step::Start),
        ("view.move-tab-to-end", Step::End),
    ] {
        registry.workspace(id, move |this, (), _, cx| {
            this.active_pane()
                .update(cx, |pane, cx| pane.move_active(step, cx));
            Ok(())
        });
    }
    registry.workspace("view.pin-tab", |this, (), _, cx| {
        let pane = this.active_pane().clone();
        if let Some(view) = this.active_view(cx) {
            let pinned = !view.read(cx).tab.pinned;
            pane.update(cx, |pane, cx| pane.set_pinned(&view, pinned, cx));
        }
        Ok(())
    });
    // Preferences > Tab Bar > Enable pin tab feature.
    registry.enabled_when("view.pin-tab", |cx| {
        crate::app_state::AppState::global(cx).settings.tabs.pinning
    });
    registry.workspace("view.tab-color", |this, args: ColorArgs, _, cx| {
        if !(1..=TAB_COLORS).contains(&args.color) {
            anyhow::bail!(
                "{}",
                t!(
                    "pane.there-is-no-tab-color",
                    color = args.color,
                    last = TAB_COLORS
                )
            );
        }
        let pane = this.active_pane().clone();
        if let Some(view) = this.active_view(cx) {
            pane.update(cx, |pane, cx| pane.set_color(&view, Some(args.color), cx));
        }
        Ok(())
    });
    registry.workspace("view.remove-tab-color", |this, (), _, cx| {
        let pane = this.active_pane().clone();
        if let Some(view) = this.active_view(cx) {
            pane.update(cx, |pane, cx| pane.set_color(&view, None, cx));
        }
        Ok(())
    });
    registry.workspace(
        "view.switch-document",
        |this, args: SwitchArgs, window, cx| {
            this.active_pane()
                .update(cx, |pane, cx| pane.switch(args.backward, window, cx));
            Ok(())
        },
    );
}

#[derive(serde::Deserialize)]
pub(super) struct IndexArgs {
    index: usize,
}

#[derive(serde::Deserialize)]
pub(super) struct ColorArgs {
    color: u8,
}

#[derive(serde::Deserialize, Default)]
#[serde(default)]
pub(super) struct SwitchArgs {
    backward: bool,
}

/// Where View > Tab > Move Tab moves the active tab, within its group (pinned or not).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Forward,
    Backward,
    Start,
    End,
}

/// Whether View > Tab's switches are checked for the active tab.
pub fn is_checked(invocation: &Invocation, marks: Option<TabMarks>) -> Option<bool> {
    let marks = marks.unwrap_or_default();
    match invocation.command.as_str() {
        "view.pin-tab" => Some(marks.pinned),
        "view.tab-color" => Some(
            invocation
                .args
                .get("color")
                .and_then(|color| color.as_u64())
                .is_some_and(|color| marks.color.map(u64::from) == Some(color)),
        ),
        _ => None,
    }
}

impl Workspace {
    /// Runs a command of a tab's context menu on its document.
    pub fn run_tab_command(
        &mut self,
        view: Option<&Entity<EditorView>>,
        invocation: &Invocation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(view) = view {
            self.activate_view(view, window, cx);
        }
        if let Err(error) = self.dispatch(invocation, window, cx) {
            crate::workspace::report_error(&error, window, cx);
        }
    }
}
