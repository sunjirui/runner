use super::{logic::RoleFormKind, RoleFormFields};
use crate::NativeRoot;
use gpui::prelude::*;
use gpui::{px, Context, Entity};
use runner_app::ui::{SelectOption, StyledSelect};
use runner_core::execution_target::WslIntegration;
use runner_core::ExecutionTarget;
use std::rc::Rc;

fn options(current: &ExecutionTarget, integrations: &[WslIntegration]) -> Vec<SelectOption> {
    let mut targets = vec![ExecutionTarget::Native];
    targets.extend(integrations.iter().map(|i| i.target.clone()));
    if !targets.contains(current) {
        targets.push(current.clone());
    }
    targets
        .into_iter()
        .map(|target| SelectOption::new(serde_json::to_string(&target).unwrap(), target.label()))
        .collect()
}

impl NativeRoot {
    pub(super) fn new_role_execution_select(
        &self,
        target: ExecutionTarget,
        kind: RoleFormKind,
        cx: &mut Context<Self>,
    ) -> Entity<StyledSelect> {
        let root = cx.entity();
        cx.new(|select_cx| {
            StyledSelect::new(
                "role-execution-target",
                select_cx.focus_handle(),
                serde_json::to_string(&target).unwrap(),
                options(&target, &[]),
                Rc::new(move |value, _, cx| {
                    root.update(cx, |root, cx| {
                        root.select_role_execution_target(&value, kind, cx)
                    });
                }),
                select_cx,
            )
            .full_width(true)
            .width(px(super::ROLE_COLUMN_WIDTH))
        })
    }

    fn execution_form(&mut self, kind: RoleFormKind) -> Option<&mut RoleFormFields> {
        match kind {
            RoleFormKind::Create => self.role_surfaces.create.as_mut().map(|f| &mut f.fields),
            RoleFormKind::Edit => self.role_surfaces.edit.as_mut().map(|f| &mut f.fields),
        }
    }

    pub(super) fn load_role_execution_targets(&self, cx: &mut Context<Self>) {
        if !cfg!(windows) {
            return;
        }
        let core = self.core(cx).clone();
        let task = cx.background_spawn(async move { core.wsl_status().map_err(|e| e.to_string()) });
        cx.spawn(async move |weak, cx| {
            let result = task.await;
            let _ = weak.update(cx, |this, cx| {
                if let Ok(status) = result {
                    for kind in [RoleFormKind::Create, RoleFormKind::Edit] {
                        if let Some(form) = this.execution_form(kind) {
                            form.execution_integrations = status.integrations.clone();
                            if !form.execution_target.is_native() {
                                form.runtimes = status
                                    .integrations
                                    .iter()
                                    .filter(|i| i.target == form.execution_target)
                                    .flat_map(|i| i.agents.keys())
                                    .filter_map(|name| runner_core::Runtime::parse(name))
                                    .filter_map(
                                        runner_core::protocol::RuntimeCatalogEntry::for_runtime,
                                    )
                                    .map(|mut entry| {
                                        entry.available = true;
                                        entry
                                    })
                                    .collect();
                                form.runtime_select.update(cx, |select, cx| {
                                    select.set_options(
                                        runner_app::ui::runtime_select_options(&form.runtimes),
                                        cx,
                                    );
                                    select.set_disabled(
                                        form.submitting || form.runtimes.is_empty(),
                                        cx,
                                    );
                                });
                            }
                            let choices =
                                options(&form.execution_target, &form.execution_integrations);
                            form.execution_select
                                .update(cx, |select, cx| select.set_options(choices, cx));
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn select_role_execution_target(
        &mut self,
        value: &str,
        kind: RoleFormKind,
        cx: &mut Context<Self>,
    ) {
        let Ok(target) = serde_json::from_str::<ExecutionTarget>(value) else {
            return;
        };
        let native_catalog = if target.is_native() {
            crate::surfaces::start_chat::load_selectable_runtimes(self.core(cx), self.settings(cx))
                .0
        } else {
            Vec::new()
        };
        let native_cwd = self.settings(cx).default_working_dir.clone();
        let Some(form) = self.execution_form(kind) else {
            return;
        };
        if form.submitting {
            return;
        }
        form.execution_target = target.clone();
        let integration = form
            .execution_integrations
            .iter()
            .find(|i| i.target == target);
        let cwd = integration.map(|i| i.home.clone()).unwrap_or(native_cwd);
        form.runtimes = if target.is_native() {
            native_catalog
        } else {
            integration
                .into_iter()
                .flat_map(|i| i.agents.keys())
                .filter_map(|name| runner_core::Runtime::parse(name))
                .filter_map(runner_core::protocol::RuntimeCatalogEntry::for_runtime)
                .map(|mut entry| {
                    entry.available = true;
                    entry
                })
                .collect()
        };
        form.agents_checking = false;
        form.agents_error = None;
        if !form.runtimes.iter().any(|r| r.name.key() == form.runtime) {
            form.runtime = form
                .runtimes
                .first()
                .map(|r| r.name.to_string())
                .unwrap_or_default();
        }
        let runtime = form.runtime.clone();
        let command = form
            .runtimes
            .iter()
            .find(|r| r.name.key() == runtime)
            .map(|r| r.command.clone())
            .unwrap_or_default();
        form.command
            .update(cx, |input, cx| input.reset(command, cx));
        form.working_dir
            .update(cx, |input, cx| input.reset(cwd, cx));
        let disabled = form.runtimes.is_empty();
        let choices = runner_app::ui::runtime_select_options(&form.runtimes);
        form.runtime_select.update(cx, |select, cx| {
            select.set_options(choices, cx);
            select.set_disabled(disabled, cx);
            select.set_value(runtime, cx);
        });
        match kind {
            RoleFormKind::Create => self.sync_create_role_efforts(cx),
            RoleFormKind::Edit => self.sync_role_edit_efforts(cx),
        }
        cx.notify();
    }
}
