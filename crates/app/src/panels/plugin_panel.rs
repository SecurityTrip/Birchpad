//! The plugin panel: lines a plugin or a script shows (ADR 0036), or a plugin's controls
//! (ADR 0046). A click on a line, or a control used, tells the plugin that filled it.

use birchpad_plugin::Control;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::Button;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, FocusHandle, Focusable, FontWeight,
    SharedString, Subscription, WeakEntity, Window, div, prelude::*, px,
};

use crate::extensions::PanelOwner;
use crate::workspace::{Workspace, report_error};

pub struct PluginPanel {
    workspace: WeakEntity<Workspace>,
    pub owner: Option<PanelOwner>,
    pub title: String,
    pub lines: Vec<String>,
    /// The controls, when the plugin filled the panel with them rather than lines.
    pub controls: Vec<Control>,
    /// The text fields of the controls, by the control's index.
    pub fields: Vec<(usize, Entity<InputState>)>,
    field_subscriptions: Vec<Subscription>,
    focus_handle: FocusHandle,
}

impl Focusable for PluginPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl PluginPanel {
    pub fn new(workspace: WeakEntity<Workspace>, _: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            workspace,
            owner: None,
            title: String::new(),
            lines: Vec::new(),
            controls: Vec::new(),
            fields: Vec::new(),
            field_subscriptions: Vec::new(),
            focus_handle: cx.focus_handle(),
        }
    }

    pub fn fill(
        &mut self,
        owner: PanelOwner,
        title: String,
        lines: Vec<String>,
        cx: &mut Context<Self>,
    ) {
        self.owner = Some(owner);
        self.title = title;
        self.lines = lines;
        self.controls.clear();
        self.fields.clear();
        self.field_subscriptions.clear();
        cx.notify();
    }

    /// Fills the panel with `controls` instead of lines.
    pub fn fill_controls(
        &mut self,
        owner: PanelOwner,
        title: String,
        controls: Vec<Control>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.owner = Some(owner);
        self.title = title;
        self.lines.clear();
        self.fields.clear();
        self.field_subscriptions.clear();
        for (index, control) in controls.iter().enumerate() {
            let Control::TextField { id, value } = control else {
                continue;
            };
            let field = cx.new(|cx| InputState::new(window, cx).default_value(value.clone()));
            let id = id.clone();
            let subscription = cx.subscribe_in(
                &field,
                window,
                move |this, field, event: &InputEvent, window, cx| {
                    if let InputEvent::PressEnter { .. } = event {
                        let value = field.read(cx).value().to_string();
                        this.used(&id, &value, window, cx);
                    }
                },
            );
            self.fields.push((index, field));
            self.field_subscriptions.push(subscription);
        }
        self.controls = controls;
        cx.notify();
    }

    /// Tells the plugin that filled the panel that its control `id` was used.
    pub fn used(&mut self, id: &str, value: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(owner) = self.owner.clone() else {
            return;
        };
        self.workspace
            .update(cx, |workspace, cx| {
                if let Err(error) = workspace.plugin_control_used(&owner, id, value, window, cx) {
                    report_error(&error, window, cx);
                }
            })
            .ok();
    }

    fn render_control(
        &self,
        index: usize,
        control: &Control,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let row = || div().px_2().py_1().flex().items_center();
        match control {
            Control::Label(text) => row().child(SharedString::from(text.clone())).into_any(),
            Control::Separator => div()
                .my_1()
                .h(px(1.))
                .bg(crate::theme::paint(crate::theme::ui().border))
                .into_any(),
            Control::Button { id, title } => {
                let id = id.clone();
                row()
                    .child(
                        Button::new(("plugin-control", index))
                            .debug_selector(move || format!("plugin-control-{index}"))
                            .small()
                            .label(title.clone())
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.used(&id, "", window, cx);
                            })),
                    )
                    .into_any()
            }
            Control::TextField { .. } => {
                let field = self
                    .fields
                    .iter()
                    .find(|(at, _)| *at == index)
                    .map(|(_, field)| field.clone());
                row()
                    .debug_selector(move || format!("plugin-control-{index}"))
                    .children(field.map(|field| div().flex_1().child(Input::new(&field))))
                    .into_any()
            }
            Control::Checkbox { id, title, checked } => {
                let (id, checked) = (id.clone(), *checked);
                row()
                    .child(
                        Checkbox::new(("plugin-control", index))
                            .debug_selector(move || format!("plugin-control-{index}"))
                            .label(title.clone())
                            .checked(checked)
                            .on_click(cx.listener(move |this, checked: &bool, window, cx| {
                                // The box shows what was chosen until the plugin fills the
                                // panel again.
                                if let Some(Control::Checkbox { checked: shown, .. }) =
                                    this.controls.get_mut(index)
                                {
                                    *shown = *checked;
                                }
                                this.used(&id, if *checked { "true" } else { "false" }, window, cx);
                                cx.notify();
                            })),
                    )
                    .into_any()
            }
        }
    }
}

impl Render for PluginPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let rows = self.lines.iter().enumerate().map(|(index, line)| {
            let workspace = self.workspace.clone();
            let owner = self.owner.clone();
            let text: SharedString = line.clone().into();
            div()
                .id(("plugin-panel-row", index))
                .debug_selector(move || format!("plugin-panel-{index}"))
                .h(px(22.))
                .px_2()
                .flex()
                .items_center()
                .whitespace_nowrap()
                .overflow_hidden()
                .cursor_pointer()
                .hover(|row| row.bg(crate::theme::paint(crate::theme::ui().hovered)))
                .child(text)
                .on_click(move |_, window, cx| {
                    let Some(owner) = owner.clone() else {
                        return;
                    };
                    workspace
                        .update(cx, |workspace, cx| {
                            if let Err(error) =
                                workspace.plugin_panel_clicked(&owner, index, window, cx)
                            {
                                report_error(&error, window, cx);
                            }
                        })
                        .ok();
                })
        });
        let controls: Vec<AnyElement> = self
            .controls
            .iter()
            .enumerate()
            .map(|(index, control)| self.render_control(index, control, cx))
            .collect();
        div()
            .id("plugin-panel")
            .track_focus(&self.focus_handle)
            .size_full()
            .flex()
            .flex_col()
            .when(!self.title.is_empty(), |panel| {
                panel.child(
                    div()
                        .flex_none()
                        .h(px(24.))
                        .px_2()
                        .flex()
                        .items_center()
                        .font_weight(FontWeight::SEMIBOLD)
                        .border_b_1()
                        .border_color(crate::theme::paint(crate::theme::ui().border))
                        .child(self.title.clone()),
                )
            })
            .child(
                div()
                    .id("plugin-panel-rows")
                    .flex_1()
                    .overflow_y_scroll()
                    .children(rows)
                    .children(controls),
            )
    }
}
