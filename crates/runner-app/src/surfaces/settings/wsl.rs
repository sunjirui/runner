use crate::theme;
use gpui::prelude::*;
use gpui::{div, Context, Entity, Render, Window};
use runner_app::ui::{Button, ButtonSize, ButtonVariant, SettingsCard, TextField};
use runner_core::protocol::{DaemonClient, WslStatus};

pub(crate) struct WslSettings {
    client: DaemonClient,
    user: Entity<TextField>,
    status: Option<WslStatus>,
    busy: bool,
    error: Option<String>,
}

impl WslSettings {
    pub(crate) fn new(client: DaemonClient, cx: &mut Context<Self>) -> Self {
        let user = cx.new(|cx| {
            TextField::new(
                cx.focus_handle(),
                "",
                crate::i18n::message("ui.wsl_user_default"),
                true,
            )
        });
        Self {
            client,
            user,
            status: None,
            busy: false,
            error: None,
        }
    }

    pub(crate) fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        let client = self.client.clone();
        let task =
            cx.background_spawn(async move { client.wsl_status().map_err(|e| e.to_string()) });
        cx.spawn(async move |weak, cx| {
            let result = task.await;
            let _ = weak.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(status) => {
                        this.status = Some(status);
                        this.error = None;
                    }
                    Err(error) => this.error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn enable(&mut self, distro: String, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        let user = self.user.read(cx).text().trim().to_owned();
        let client = self.client.clone();
        let task = cx.background_spawn(async move {
            client
                .wsl_enable_distribution(distro, (!user.is_empty()).then_some(user))
                .map_err(|e| e.to_string())
        });
        cx.spawn(async move |weak, cx| {
            let result = task.await;
            let _ = weak.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(_) => this.refresh(cx),
                    Err(error) => this.error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn disable(&mut self, target: runner_core::ExecutionTarget, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        let client = self.client.clone();
        let task = cx
            .background_spawn(async move { client.wsl_disable(target).map_err(|e| e.to_string()) });
        cx.spawn(async move |weak, cx| {
            let result = task.await;
            let _ = weak.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(()) => this.refresh(cx),
                    Err(error) => this.error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}

impl Render for WslSettings {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut rows = Vec::new();
        if let Some(status) = &self.status {
            for distro in &status.distributions {
                let name = distro.name.clone();
                let root = cx.entity();
                rows.push(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_3()
                        .child(format!(
                            "{} · WSL{} · {}",
                            distro.name, distro.version, distro.state
                        ))
                        .child(
                            Button::new(
                                gpui::SharedString::from(format!("wsl-enable-{}", distro.name)),
                                crate::i18n::text("ui.wsl_enable_refresh"),
                            )
                            .size(ButtonSize::Sm)
                            .disabled(self.busy || distro.version != 2)
                            .on_press(move |_, cx| {
                                root.update(cx, |this, cx| this.enable(name.clone(), cx));
                            }),
                        )
                        .into_any_element(),
                );
            }
            for integration in &status.integrations {
                let target = integration.target.clone();
                let root = cx.entity();
                rows.push(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_3()
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(integration.target.label())
                                .child(
                                    div()
                                        .text_size(theme::text_meta())
                                        .text_color(theme::muted())
                                        .child(
                                            integration
                                                .agents
                                                .keys()
                                                .cloned()
                                                .collect::<Vec<_>>()
                                                .join(", "),
                                        ),
                                ),
                        )
                        .child(
                            Button::new(
                                gpui::SharedString::from(format!(
                                    "wsl-disable-{}",
                                    serde_json::to_string(&target).unwrap()
                                )),
                                crate::i18n::text("ui.disable"),
                            )
                            .variant(ButtonVariant::Ghost)
                            .size(ButtonSize::Sm)
                            .disabled(self.busy)
                            .on_press(move |_, cx| {
                                root.update(cx, |this, cx| this.disable(target.clone(), cx));
                            }),
                        )
                        .into_any_element(),
                );
            }
        }
        let root = cx.entity();
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .text_size(theme::text_body())
                    .child(crate::i18n::text("ui.wsl_integration")),
            )
            .child(
                div()
                    .text_size(theme::text_ui())
                    .text_color(theme::muted())
                    .child(crate::i18n::text("ui.wsl_integration_hint")),
            )
            .child(self.user.clone())
            .child(
                Button::new("wsl-refresh", crate::i18n::text("ui.refresh"))
                    .size(ButtonSize::Sm)
                    .disabled(self.busy)
                    .on_press(move |_, cx| {
                        root.update(cx, |this, cx| this.refresh(cx));
                    }),
            )
            .child(SettingsCard::new(rows))
            .children(
                self.error
                    .clone()
                    .map(|e| runner_app::ui::notice_banner(e, runner_app::ui::Tone::Danger)),
            )
    }
}
