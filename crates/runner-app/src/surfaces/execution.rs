use crate::NativeRoot;
use gpui::prelude::*;
use gpui::{Context, Entity};
use runner_app::ui::{SelectOption, StyledSelect};
use runner_core::{execution_target::WslIntegration, ExecutionTarget};
use std::rc::Rc;

#[derive(Clone, Copy)]
pub(super) enum WorkKind {
    Chat,
    Mission,
}

fn choices(integrations: &[WslIntegration]) -> Vec<SelectOption> {
    let mut options = vec![
        SelectOption::new("", crate::i18n::message("ui.execution_inherit")),
        SelectOption::new(
            serde_json::to_string(&ExecutionTarget::Native).unwrap(),
            "Native",
        ),
    ];
    options.extend(
        integrations.iter().map(|i| {
            SelectOption::new(serde_json::to_string(&i.target).unwrap(), i.target.label())
        }),
    );
    options
}

impl NativeRoot {
    pub(super) fn new_work_execution_select(
        &self,
        kind: WorkKind,
        cx: &mut Context<Self>,
    ) -> Entity<StyledSelect> {
        let root = cx.entity();
        cx.new(|cx| {
            StyledSelect::new(
                "work-execution-target",
                cx.focus_handle(),
                "",
                choices(&[]),
                Rc::new(move |value, _, cx| {
                    root.update(cx, |root, cx| {
                        root.select_work_execution_target(kind, &value, cx)
                    });
                }),
                cx,
            )
            .full_width(true)
        })
    }

    pub(super) fn load_work_execution_targets(&self, cx: &mut Context<Self>) {
        if !cfg!(windows) {
            return;
        }
        let client = self.core(cx).clone();
        let task = cx.background_spawn(async move { client.wsl_status() });
        cx.spawn(async move |weak, cx| {
            let result = task.await;
            let _ = weak.update(cx, |root, cx| {
                if let Ok(status) = result {
                    let project_target = root
                        .start_chat_modal
                        .as_ref()
                        .and_then(|modal| modal.scope.project_id())
                        .and_then(|id| root.app_store.read(cx).projects.iter().find(|p| p.id == id))
                        .map(|p| p.execution_target.clone());
                    if let Some(modal) = root.start_chat_modal.as_mut() {
                        modal.execution_integrations = status.integrations.clone();
                        let target = modal
                            .execution_target
                            .as_ref()
                            .or(project_target.as_ref())
                            .or_else(|| modal.selected_role().map(|r| &r.execution_target));
                        if let Some(integration) = status
                            .integrations
                            .iter()
                            .find(|i| Some(&i.target) == target)
                        {
                            modal.runtimes = integration
                                .agents
                                .keys()
                                .filter_map(|name| runner_core::Runtime::parse(name))
                                .filter_map(runner_core::protocol::RuntimeCatalogEntry::for_runtime)
                                .map(|mut r| {
                                    r.available = true;
                                    r
                                })
                                .collect();
                            modal.runtime_select.update(cx, |select, cx| {
                                select.set_options(
                                    super::start_chat::runtime_options(&modal.runtimes),
                                    cx,
                                );
                                select.set_disabled(modal.runtimes.is_empty(), cx);
                            });
                            super::start_chat::sync_runtime_controls(modal, cx);
                        }
                        modal.execution_select.update(cx, |select, cx| {
                            select.set_options(choices(&status.integrations), cx)
                        });
                    }
                    if let Some(modal) = root.start_mission_modal.as_mut() {
                        modal.execution_integrations = status.integrations.clone();
                        modal.execution_select.update(cx, |select, cx| {
                            select.set_options(choices(&status.integrations), cx)
                        });
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn select_work_execution_target(
        &mut self,
        kind: WorkKind,
        value: &str,
        cx: &mut Context<Self>,
    ) {
        let target = if value.is_empty() {
            None
        } else {
            let Ok(target) = serde_json::from_str::<ExecutionTarget>(value) else {
                return;
            };
            Some(target)
        };
        let native_cwd = self.settings(cx).default_working_dir.clone();
        let native_catalog =
            super::start_chat::load_selectable_runtimes(self.core(cx), self.settings(cx)).0;
        match kind {
            WorkKind::Chat => {
                let Some(modal) = self.start_chat_modal.as_mut() else {
                    return;
                };
                let integration = modal
                    .execution_integrations
                    .iter()
                    .find(|i| Some(&i.target) == target.as_ref());
                if let Some(integration) = integration {
                    modal
                        .cwd
                        .update(cx, |input, cx| input.reset(integration.home.clone(), cx));
                    modal.runtimes = integration
                        .agents
                        .keys()
                        .filter_map(|name| runner_core::Runtime::parse(name))
                        .filter_map(runner_core::protocol::RuntimeCatalogEntry::for_runtime)
                        .map(|mut r| {
                            r.available = true;
                            r
                        })
                        .collect();
                } else {
                    modal
                        .cwd
                        .update(cx, |input, cx| input.reset(native_cwd, cx));
                    modal.runtimes = native_catalog;
                }
                modal.execution_target = target;
                modal.agents_checking = false;
                modal.agents_error = None;
                if !modal
                    .runtimes
                    .iter()
                    .any(|r| Some(r.name.key()) == modal.runtime_name.as_deref())
                {
                    modal.runtime_name = modal.runtimes.first().map(|r| r.name.to_string());
                }
                let options = super::start_chat::runtime_options(&modal.runtimes);
                modal.runtime_select.update(cx, |select, cx| {
                    select.set_options(options, cx);
                    select.set_disabled(modal.runtimes.is_empty(), cx);
                    select.set_value(modal.runtime_name.clone().unwrap_or_default(), cx);
                });
                super::start_chat::sync_runtime_controls(modal, cx);
            }
            WorkKind::Mission => {
                let Some(modal) = self.start_mission_modal.as_mut() else {
                    return;
                };
                let cwd = modal
                    .execution_integrations
                    .iter()
                    .find(|i| Some(&i.target) == target.as_ref())
                    .map(|i| i.home.clone())
                    .unwrap_or(native_cwd);
                modal.cwd.update(cx, |input, cx| input.reset(cwd, cx));
                modal.execution_target = target;
            }
        }
        cx.notify();
    }
}
