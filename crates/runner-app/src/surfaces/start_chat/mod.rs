mod controls;
mod launch;
#[cfg(test)]
mod launcher_model;
mod modal;
mod render;
mod terminal;

use controls::*;
#[cfg(test)]
use launch::*;
use render::*;

mod add_role;

mod role_setup;

#[cfg(test)]
use runner_core::protocol::model::Runtime;

use std::fs;

use std::path::{Path, PathBuf};

use std::rc::Rc;

use crate::surfaces::front_page::model::{RoleChoice, RuntimeChoice};

use anyhow::bail;

use gpui::prelude::*;

use gpui::{
    div, px, rems, svg, AnyElement, Context, Div, FontWeight, KeyDownEvent, PathPromptOptions,
    ScrollHandle, Window,
};

use runner_core::protocol::model::CodexSpeed;

#[cfg(test)]
use runner_core::protocol::model::Role;

use runner_core::protocol::project::ProjectScope;

use runner_core::protocol::runtime::{
    filter_selectable_runtime_catalog, RuntimeCatalogEntry, RuntimeCatalogOption,
};

use runner_app::ui::{
    effective_working_dir, working_dir_placeholder, working_dir_text_field, Button, ButtonVariant,
    Field, IconButton, Modal, ModelField, OverlayWidth, RoleAvatar, Scrollbar, SelectHandler,
    SelectLeading, SelectOption, StyledSelect, TextField, WorkingDirField,
};

use super::profile_page::{column_text, section_label, text_action};

use super::project_field::{ProjectChoice, ProjectField};

use super::*;

use crate::chat_icon::runtime_mark;

use crate::*;

const START_CHAT_RECENT_RUNTIMES_FILE: &str = "start-chat-recent-runtimes";

const MODAL_WIDTH: f32 = 560.;

const CARD_PADDING: f32 = 12.;

const COLUMN_GAP: f32 = 12.;

const EFFORT_COLUMN_WIDTH: f32 = 160.;

const SPEED_COLUMN_WIDTH: f32 = 104.;

const ROLE_HINT: &str = "ui.starts_with_the_role_s_settings_change_any_of_them_for_this_chat_only";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ChatMode {
    Role,
    Runtime,
}

#[derive(Clone)]
pub(super) enum ChatTarget {
    NewTab,
    Pane { tab_id: String, pane_id: String },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NewTerminalTarget {
    MissionDrawer,
    Tab,
}

fn new_terminal_target(route: &AppRoute, mission_drawer_available: bool) -> NewTerminalTarget {
    if matches!(route, AppRoute::Mission(_)) && mission_drawer_available {
        NewTerminalTarget::MissionDrawer
    } else {
        NewTerminalTarget::Tab
    }
}

fn new_terminal_empty_pane(layout: &PaneLayout) -> Option<String> {
    let leaves = layout.root.leaves();
    leaves
        .iter()
        .find(|leaf| leaf.id == layout.focused_pane_id && leaf.session_id.is_none())
        .or_else(|| leaves.iter().find(|leaf| leaf.session_id.is_none()))
        .map(|leaf| leaf.id.clone())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StartChatSelection {
    Project,
    Role,
    RoleRuntime,
    Runtime,
    Effort,
    Speed,
}

pub(crate) struct StartChatModal {
    mission_role: Option<add_role::MissionRoleForm>,
    front_page: crate::surfaces::front_page::FrontPageClient,
    folder_picker: Option<Entity<crate::surfaces::front_page::folder_picker::FolderPicker>>,
    remote_started: Option<String>,
    remote_renamed: bool,
    host_account: Option<String>,
    _folder_subscription: Option<Subscription>,
    _folder_choice: Option<Subscription>,
    _cwd_blur: Option<Subscription>,
    _cwd_follow: Option<Subscription>,
    target: ChatTarget,
    project: ProjectField,
    mode: ChatMode,
    roles: Vec<RoleChoice>,
    runtimes: Vec<RuntimeChoice>,
    /// The agents Direct chats last started on, most recent first.
    recent_runtimes: Vec<String>,
    role_id: Option<String>,
    runtime_name: Option<String>,
    /// The agent a role chat runs on when it is not the role's own; picking
    /// the role's own agent clears it.
    role_runtime_override: Option<String>,
    /// The Effort and Speed the controls show; `baseline_effort` and
    /// `baseline_speed` are what leaves them untouched.
    effort: String,
    speed: String,
    title: Entity<TextField>,
    cwd: Entity<TextField>,
    model: Entity<TextField>,
    model_field: Entity<ModelField>,
    role_select: Entity<StyledSelect>,
    role_runtime_select: Entity<StyledSelect>,
    runtime_select: Entity<StyledSelect>,
    effort_select: Entity<StyledSelect>,
    speed_select: Entity<StyledSelect>,
    scroll_handle: ScrollHandle,
    scrollbar: Entity<Scrollbar>,
    role_mode_focus: FocusHandle,
    direct_mode_focus: FocusHandle,
    /// One Reset per role control, in `ResetKind` order.
    reset_focus: [FocusHandle; 4],
    browse_focus: FocusHandle,
    close_focus: FocusHandle,
    cancel_focus: FocusHandle,
    submit_focus: FocusHandle,
    agents_checking: bool,
    agents_error: Option<String>,
    submitting: bool,
    error: Option<crate::i18n::Text>,
    _model_subscription: gpui::Subscription,
}

impl StartChatModal {
    #[cfg(test)]
    pub(crate) fn test_folder_picker(
        &self,
    ) -> Option<Entity<crate::surfaces::front_page::folder_picker::FolderPicker>> {
        self.folder_picker.clone()
    }

    #[cfg(test)]
    pub(crate) fn test_project(&self) -> &ProjectField {
        &self.project
    }

    #[cfg(test)]
    pub(crate) fn test_inputs(&self) -> (Entity<TextField>, Entity<TextField>) {
        (self.title.clone(), self.cwd.clone())
    }

    fn mode_focus(&self) -> FocusHandle {
        match self.mode {
            ChatMode::Role => self.role_mode_focus.clone(),
            ChatMode::Runtime => self.direct_mode_focus.clone(),
        }
    }

    fn picker_focus(&self, cx: &App) -> FocusHandle {
        match self.mode {
            ChatMode::Role if !self.roles.is_empty() => self.role_select.read(cx).focus_handle(),
            ChatMode::Runtime if !self.runtimes.is_empty() => {
                self.runtime_select.read(cx).focus_handle()
            }
            _ => self.title.read(cx).focus_handle(),
        }
    }

    fn selected_role(&self) -> Option<&RoleChoice> {
        self.role_id
            .as_deref()
            .and_then(|role_id| self.roles.iter().find(|role| role.id == role_id))
    }

    fn selected_runtime(&self) -> Option<&RuntimeChoice> {
        self.runtime_name
            .as_deref()
            .and_then(|name| self.runtime_entry(name))
    }

    fn runtime_entry(&self, name: &str) -> Option<&RuntimeChoice> {
        self.runtimes
            .iter()
            .find(|runtime| runtime.name.key() == name)
    }

    /// The agent a role chat runs on: the override, else the role's own.
    fn role_agent(&self) -> Option<&str> {
        self.role_runtime_override
            .as_deref()
            .or_else(|| self.selected_role().map(|role| role.runtime.as_str()))
    }

    /// Whether a role chat runs on the role's own agent, where the controls
    /// start from the role's model, effort and speed.
    fn on_own_agent(&self) -> bool {
        self.role_runtime_override.is_none()
    }

    fn active_runtime(&self) -> Option<&RuntimeChoice> {
        match self.mode {
            ChatMode::Role => self.role_agent().and_then(|name| self.runtime_entry(name)),
            ChatMode::Runtime => self.selected_runtime(),
        }
    }

    fn runtime_has_speed(&self, key: &str) -> bool {
        self.runtimes
            .iter()
            .find(|runtime| runtime.name.key() == key)
            .map(|runtime| runtime.speed)
            .unwrap_or_else(|| {
                !self.front_page.is_remote()
                    && crate::runtime_ui::catalog_capabilities(&[], key).codex_speed
            })
    }

    fn codex_speed_visible(&self) -> bool {
        let key = match self.mode {
            ChatMode::Role => self.role_agent(),
            ChatMode::Runtime => self.runtime_name.as_deref(),
        };
        key.is_some_and(|key| self.runtime_has_speed(key))
    }

    fn effective_speed(&self) -> Option<CodexSpeed> {
        self.codex_speed_visible()
            .then(|| parse_speed(&self.speed))
            .flatten()
    }

    /// The Effort that leaves the control untouched: the role's on its own
    /// agent, else the agent's default, else none known.
    fn baseline_effort(&self) -> String {
        let runtime = self.active_runtime();
        inheriting_role(self)
            .and_then(|role| trimmed(role.effort.as_deref()))
            .map(str::to_owned)
            .or_else(|| {
                let runtime = runtime?;
                runtime
                    .default_effort
                    .clone()
                    .filter(|effort| runtime.efforts.iter().any(|option| option.value == *effort))
            })
            .unwrap_or_default()
    }

    /// The Speed that leaves the control untouched: the role's on its own
    /// Codex agent, else Inherit.
    fn baseline_speed(&self) -> String {
        match inheriting_role(self)
            .filter(|role| self.runtime_has_speed(&role.runtime))
            .and_then(|role| role.codex_speed)
        {
            Some(CodexSpeed::Standard) => "standard",
            Some(CodexSpeed::Fast) => "fast",
            None => "inherit",
        }
        .into()
    }

    /// The model the request carries: the one typed, unless it is the role's
    /// own, which leaves the control untouched.
    fn model_override(&self, cx: &App) -> Option<String> {
        let role_model = inheriting_role(self).and_then(|role| trimmed(role.model.as_deref()));
        normalized_value(self.model.read(cx).text())
            .filter(|text| Some(text.as_str()) != role_model)
    }

    fn effort_override(&self) -> Option<String> {
        (!self.effort.is_empty() && self.effort != self.baseline_effort())
            .then(|| self.effort.clone())
    }

    fn speed_override(&self) -> Option<CodexSpeed> {
        (self.speed != self.baseline_speed())
            .then(|| self.effective_speed())
            .flatten()
    }

    /// The role controls that differ from the role's own setup. On the role's
    /// own agent that is the model, effort and speed changed; on another agent
    /// it is the agent alone, whose defaults the rest start from.
    fn role_overrides(&self, cx: &App) -> RoleOverrides {
        let inheriting = inheriting_role(self).is_some();
        RoleOverrides {
            runtime: self.mode == ChatMode::Role
                && self.selected_role().is_some()
                && !self.on_own_agent(),
            model: inheriting && self.model_override(cx).is_some(),
            effort: inheriting && self.effort_override().is_some(),
            speed: inheriting && self.speed_override().is_some(),
        }
    }

    fn can_submit(&self) -> bool {
        !self.submitting
            && self.front_page.supports(&["startChat"])
            && (!self.front_page.is_remote()
                || self
                    .active_runtime()
                    .is_some_and(|runtime| runtime.available))
            && match self.mode {
                ChatMode::Role => self.selected_role().is_some(),
                ChatMode::Runtime => self.selected_runtime().is_some(),
            }
    }

    fn is_composing(&self, cx: &App) -> bool {
        self.title.read(cx).is_composing()
            || self.cwd.read(cx).is_composing()
            || self.model.read(cx).is_composing()
    }
}

/// Which role controls carry an override, each with its amber dot and Reset.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct RoleOverrides {
    runtime: bool,
    model: bool,
    effort: bool,
    speed: bool,
}

/// A role control's Reset, in `StartChatModal::reset_focus` order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ResetKind {
    Runtime,
    Model,
    Effort,
    Speed,
}

#[derive(Debug, Eq, PartialEq)]
enum StartRequest {
    Role {
        role_id: String,
        runtime: Option<String>,
        model: Option<String>,
        effort: Option<String>,
        speed: Option<CodexSpeed>,
        cwd: Option<String>,
    },
    Runtime {
        runtime: String,
        model: Option<String>,
        effort: Option<String>,
        speed: Option<CodexSpeed>,
        cwd: Option<String>,
    },
}

impl NativeRoot {
    pub(crate) fn create_modal_open(&self) -> bool {
        self.start_chat_modal.is_some() || self.start_mission_modal.is_some()
    }

    fn confirm_start_chat(
        &mut self,
        _: &ConfirmStartChat,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(modal) = self
            .start_chat_modal
            .as_ref()
            .filter(|modal| modal.can_submit() && !modal.is_composing(cx))
        else {
            return;
        };
        for select in [
            &modal.role_select,
            &modal.role_runtime_select,
            &modal.runtime_select,
            &modal.effort_select,
            &modal.speed_select,
        ] {
            select.update(cx, |select, cx| select.close(cx));
        }
        modal
            .project
            .select()
            .update(cx, |select, cx| select.close(cx));
        modal.model_field.update(cx, |field, cx| field.close(cx));
        self.submit_start_chat(window, cx);
    }

    fn mouse_switch_start_chat_mode(
        &mut self,
        mode: ChatMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let focused = window.focused(cx);
        self.set_start_chat_mode(mode, cx);
        if let Some(modal) = &self.start_chat_modal {
            let survives = focused.as_ref().is_some_and(|focus| {
                *focus == modal.direct_mode_focus
                    || *focus == modal.role_mode_focus
                    || start_chat_focus_order(modal, cx).contains(focus)
            });
            if !survives {
                modal.picker_focus(cx).focus(window, cx);
            }
        }
    }

    pub(super) fn switch_start_chat_mode(
        &mut self,
        mode: ChatMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .start_chat_modal
            .as_ref()
            .is_none_or(|modal| modal.submitting)
        {
            return;
        }
        self.set_start_chat_mode(mode, cx);
        if let Some(modal) = &self.start_chat_modal {
            modal.picker_focus(cx).focus(window, cx);
        }
    }
}

impl NativeRoot {
    /// The cwd a shell started from `session_id` opens in: see
    /// `shell_start_cwd`. Only shell sessions scan for OSC 7, so a chat's
    /// cwd never moves.
    pub(crate) fn session_start_cwd(&self, session_id: &str, cx: &App) -> Option<String> {
        let live = self
            .app_store
            .read(cx)
            .bridge
            .session(session_id)
            .and_then(|terminal| terminal.live_cwd());
        shell_start_cwd(
            live,
            self.session_entry(session_id, cx)
                .and_then(|entry| entry.cwd.as_deref()),
        )
    }
}

/// Where a new shell opens when it is started from another session (#575):
/// the directory that session's shell last reported through OSC 7, which
/// `live_cwd` only offers while it exists, else the cwd it was spawned in.
/// `None` continues down `terminal_working_dir`'s chain.
pub(crate) fn shell_start_cwd(live: Option<PathBuf>, spawn: Option<&str>) -> Option<String> {
    live.and_then(|cwd| cwd.into_os_string().into_string().ok())
        .or_else(|| spawn.map(str::to_owned))
}

pub(crate) fn terminal_working_dir(
    sibling_cwd: Option<&str>,
    project_cwd: Option<&str>,
    default_cwd: &str,
    home: Option<&str>,
) -> Option<String> {
    [sibling_cwd, project_cwd, Some(default_cwd), home]
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|path| !path.is_empty())
        .map(str::to_owned)
}

pub(crate) fn load_selectable_runtimes(
    core: &DaemonClient,
    settings: &AppSettings,
) -> (Vec<RuntimeCatalogEntry>, bool, Option<String>) {
    let checking = core.discovery_snapshot().is_ok_and(|state| state.checking);
    match core.runtime_catalog() {
        Ok(catalog) => {
            let enabled = catalog
                .iter()
                .filter(|runtime| settings.is_agent_enabled(runtime.name, runtime.default_enabled))
                .map(|runtime| runtime.name.to_string())
                .collect::<Vec<_>>();
            (
                filter_selectable_runtime_catalog(catalog, Some(&enabled)),
                checking,
                None,
            )
        }
        Err(error) => (Vec::new(), checking, Some(error.to_string())),
    }
}

fn runtime_display_name(runtimes: &[RuntimeChoice], name: &str) -> String {
    runtimes
        .iter()
        .find(|runtime| runtime.name.key() == name)
        .map(|runtime| runtime.display_name.clone())
        .or_else(|| {
            runner_core::protocol::runtime_metadata::runtime_list()
                .into_iter()
                .find(|runtime| runtime.name.key() == name)
                .map(|runtime| runtime.display_name)
        })
        .unwrap_or_else(|| name.to_owned())
}

fn default_title_for_role(handle: &str) -> String {
    format!("@{handle}")
}

fn default_title_for_runtime(label: &str) -> String {
    label.to_owned()
}

fn user_chat_title(input: &TextField) -> Option<String> {
    input
        .edited()
        .then(|| normalized_value(input.text()))
        .flatten()
}

fn update_auto_title(title: &Entity<TextField>, derived: String, cx: &mut gpui::App) {
    title.update(cx, |input, input_cx| {
        input.set_placeholder(derived, input_cx);
    });
}

fn cwd_placeholder(mode: ChatMode, role: Option<&RoleChoice>, default_path: &str) -> String {
    match mode {
        ChatMode::Role => working_dir_placeholder(
            role.and_then(|role| role.working_dir.as_deref()),
            default_path,
        ),
        ChatMode::Runtime => working_dir_placeholder(None, default_path),
    }
}

#[cfg(test)]
fn effort_options_for_runtime<'a>(
    runtimes: &'a [RuntimeCatalogEntry],
    name: &str,
) -> &'a [RuntimeCatalogOption] {
    runtimes
        .iter()
        .find(|runtime| runtime.name.key() == name)
        .map(|runtime| runtime.efforts.as_slice())
        .unwrap_or_default()
}

fn normalized_value(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

/// Direct's agents, the most recently started first; the ones never started
/// follow in catalog order.
fn direct_runtime_order<'a>(
    runtimes: &'a [RuntimeChoice],
    recent: &[String],
) -> Vec<&'a RuntimeChoice> {
    let mut ordered = runtimes.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|runtime| {
        recent
            .iter()
            .position(|name| name == runtime.name.key())
            .unwrap_or(usize::MAX)
    });
    ordered
}

/// The agent Direct opens on: the most recently started one, otherwise the
/// default from Settings, otherwise the first.
fn initial_direct_runtime(
    runtimes: &[RuntimeChoice],
    recent: &[String],
    default_runtime: &str,
) -> Option<String> {
    recent
        .iter()
        .map(String::as_str)
        .chain([default_runtime])
        .find_map(|name| runtimes.iter().find(|runtime| runtime.name.key() == name))
        .or_else(|| runtimes.first())
        .map(|runtime| runtime.name.to_string())
}

fn recent_runtimes_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join(START_CHAT_RECENT_RUNTIMES_FILE)
}

fn read_recent_runtimes(app_data_dir: &Path) -> Vec<String> {
    fs::read_to_string(recent_runtimes_path(app_data_dir))
        .map(|text| {
            text.lines()
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn record_recent_runtime(app_data_dir: &Path, runtime: &str) -> std::io::Result<()> {
    let mut recent = read_recent_runtimes(app_data_dir);
    recent.retain(|name| name != runtime);
    recent.insert(0, runtime.to_owned());
    fs::write(recent_runtimes_path(app_data_dir), recent.join("\n"))
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    struct ModalHost(Entity<NativeRoot>);
    impl Render for ModalHost {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            self.0.update(cx, |root, cx| {
                if root.route == AppRoute::Settings {
                    return root.render_app_shell(window, cx);
                }
                let pane = root
                    .tabs
                    .active()
                    .cloned()
                    .filter(|layout| {
                        layout
                            .root
                            .leaves()
                            .iter()
                            .any(|leaf| leaf.session_id.is_some())
                    })
                    .map(|layout| root.render_pane_node(&layout.root, &layout, cx));
                div()
                    .size_full()
                    .children(pane)
                    .key_context("Terminal")
                    .track_focus(&root.root_focus)
                    .on_action(cx.listener(NativeRoot::open_new_tab_modal))
                    .on_action(cx.listener(NativeRoot::new_terminal_action))
                    .on_action(cx.listener(NativeRoot::new_mission_action))
                    .on_action(cx.listener(|root, _: &SelectTab1, _, cx| {
                        root.tabs.activate("01M3VD00000000000000000001");
                        cx.notify();
                    }))
                    .on_action(cx.listener(|root, _: &SelectTab2, _, cx| {
                        root.tabs.activate("01M3VD00000000000000000002");
                        cx.notify();
                    }))
                    .when(root.start_chat_modal.is_some(), |host| {
                        host.child(root.render_start_chat_modal(window, cx))
                    })
                    .when(root.start_mission_modal.is_some(), |host| {
                        host.child(root.render_start_mission_modal(cx))
                    })
                    .into_any_element()
            })
        }
    }

    pub(in crate::surfaces) struct ModalHarness {
        pub(in crate::surfaces) visual: VisualTestContext,
        host: WindowHandle<ModalHost>,
        _cx: TestAppContext,
        _temp: tempfile::TempDir,
        _theme: crate::theme_snapshot::ThemeGuard,
    }

    /// A modal window that opens already holding `roles` and `runtimes`, so no
    /// frame is drawn from the machine's own catalog: a test frame keeps every
    /// debug selector it ever drew.
    pub(in crate::surfaces) fn modal_harness(
        width: f32,
        height: f32,
        roles: Vec<Role>,
        runtimes: Vec<RuntimeCatalogEntry>,
        mode: ChatMode,
    ) -> ModalHarness {
        let theme = crate::theme_snapshot::ThemeGuard::new();
        let temp = tempfile::tempdir().unwrap();
        let runtime_shell_env = Arc::new(RwLock::new(shell_path::LoginShellEnv::default()));
        let runtime_discovery =
            Arc::new(RwLock::new(shell_path::DiscoveryState::startup(None, None)));
        let core = crate::test_support::core(
            Arc::new(db::open_fixture_pool(&temp.path().join("runner.db")).unwrap()),
            temp.path().to_owned(),
            session::SessionManager::new(
                runtime_shell_env.clone(),
                runtime_discovery.clone(),
                Arc::new(session::pty_runtime::PtyRuntime::new()),
            ),
            runtime_shell_env,
            runtime_discovery,
        );
        let mut cx = TestAppContext::single();
        cx.update(|cx| keymap::install_bindings(cx, &keymap::KeymapOverrides::new(), false));
        #[cfg(target_os = "macos")]
        let updater = cx.new(|cx| Updater::new(false, cx));
        #[cfg(any(windows, target_os = "linux"))]
        let updater = cx.new(|cx| Updater::new(false, temp.path().join("updates"), cx));
        cx.set_global(GlobalUpdater(updater));
        let store = cx.new(|cx| {
            AppStore::new(
                core,
                None,
                None,
                temp.path().join("settings.json"),
                AppSettings::default(),
                None,
                cx,
            )
        });
        cx.set_global(GlobalAppStore(store.clone()));
        cx.set_global(WindowLayoutCheckpoint::default());
        let host = cx.add_window(|window, cx| {
            let root = cx.new(|cx| {
                NativeRoot::new(
                    "test-chat-modal".into(),
                    temp.path().to_owned(),
                    Some("chats".into()),
                    None,
                    store,
                    window,
                    cx,
                )
            });
            root.update(cx, |root, cx| {
                root.root_focus.focus(window, cx);
                root.open_new_tab_modal(&NewTab, window, cx);
                seed_modal(
                    root.start_chat_modal.as_mut().unwrap(),
                    roles,
                    runtimes,
                    mode,
                    cx,
                );
                root.start_chat_modal
                    .as_ref()
                    .unwrap()
                    .picker_focus(cx)
                    .focus(window, cx);
            });
            ModalHost(root)
        });
        cx.run_until_parked();
        let visual = VisualTestContext::from_window(host.into(), &cx);
        visual.simulate_resize(size(px(width), px(height)));
        visual.run_until_parked();
        ModalHarness {
            visual,
            host,
            _cx: cx,
            _temp: temp,
            _theme: theme,
        }
    }

    /// Puts roles and agents in the modal as a loaded catalog would.
    fn seed_modal(
        modal: &mut StartChatModal,
        roles: Vec<Role>,
        runtimes: Vec<RuntimeCatalogEntry>,
        mode: ChatMode,
        cx: &mut Context<NativeRoot>,
    ) {
        modal.roles = roles.into_iter().map(Into::into).collect();
        modal.runtimes = runtimes.into_iter().map(Into::into).collect();
        modal.role_id = modal.roles.first().map(|role| role.id.clone());
        modal.runtime_name = modal
            .runtimes
            .first()
            .map(|runtime| runtime.name.to_string());
        modal.mode = mode;
        modal.role_select.update(cx, |select, select_cx| {
            select.set_options(role_options(&modal.roles), select_cx);
            select.set_value(modal.role_id.clone().unwrap_or_default(), select_cx);
            select.set_disabled(modal.roles.is_empty(), select_cx);
        });
        modal.runtime_select.update(cx, |select, select_cx| {
            select.set_options(runtime_options(&modal.runtimes), select_cx);
            select.set_value(modal.runtime_name.clone().unwrap_or_default(), select_cx);
            select.set_disabled(modal.runtimes.is_empty(), select_cx);
        });
        sync_role_runtime_select(modal, cx);
        restore_baseline(modal, cx);
    }

    impl ModalHarness {
        pub(in crate::surfaces) fn act(
            &mut self,
            f: impl FnOnce(&mut NativeRoot, &mut Window, &mut Context<NativeRoot>),
        ) {
            self.host
                .update(&mut self.visual, |host, window, cx| {
                    host.0.update(cx, |root, cx| f(root, window, cx));
                    cx.notify();
                })
                .unwrap();
            self.visual.run_until_parked();
        }

        fn edit(&mut self, f: impl FnOnce(&mut StartChatModal, &mut Context<NativeRoot>)) {
            self.act(|root, _, cx| {
                f(root.start_chat_modal.as_mut().unwrap(), cx);
                cx.notify();
            });
        }

        fn read<R>(&self, f: impl FnOnce(&StartChatModal, &App) -> R) -> R {
            self.host
                .read_with(&self.visual, |host, cx| {
                    f(host.0.read(cx).start_chat_modal.as_ref().unwrap(), cx)
                })
                .unwrap()
        }

        fn bounds(&mut self, selector: &str) -> Option<gpui::Bounds<gpui::Pixels>> {
            self.visual
                .debug_bounds(Box::leak(selector.to_owned().into_boxed_str()))
        }

        /// Opens the focused select with the keyboard and picks by key, the
        /// way a person does: through `StyledSelect::choose`, which holds the
        /// select while it calls back.
        fn pick(&mut self, select: fn(&StartChatModal, &App) -> FocusHandle, keys: &str) {
            let handle = self.read(select);
            self.act(move |_, window, cx| handle.focus(window, cx));
            self.visual.simulate_keystrokes(keys);
            self.visual.run_until_parked();
        }

        /// Picks as the select would, minus the keyboard: it holds the choice
        /// itself before it calls back.
        fn choose(&mut self, selection: StartChatSelection, value: &str) {
            let value = value.to_owned();
            self.act(move |root, _, cx| {
                let modal = root.start_chat_modal.as_ref().unwrap();
                let select = match selection {
                    StartChatSelection::Project => modal.project.select(),
                    StartChatSelection::Role => modal.role_select.clone(),
                    StartChatSelection::RoleRuntime => modal.role_runtime_select.clone(),
                    StartChatSelection::Runtime => modal.runtime_select.clone(),
                    StartChatSelection::Effort => modal.effort_select.clone(),
                    StartChatSelection::Speed => modal.speed_select.clone(),
                };
                select.update(cx, |select, cx| select.set_value(value.clone(), cx));
                root.select_start_chat_choice(selection, &value, cx);
            });
        }

        fn type_model(&mut self, text: &'static str) {
            self.edit(|modal, cx| modal.model.update(cx, |input, cx| input.set_text(text, cx)));
        }

        /// Clicks a control's Reset, as a person does.
        fn reset(&mut self, kind: ResetKind) {
            let bounds = self
                .bounds(&format!("START_CHAT_RESET {kind:?}"))
                .expect("the control shows its Reset");
            let content = self.bounds("MODAL_CONTENT").expect("modal content");
            assert!(content.contains(&bounds.origin) && content.contains(&bounds.bottom_right()));
            self.visual
                .simulate_click(bounds.center(), gpui::Modifiers::default());
            self.visual.run_until_parked();
        }

        /// The label the Effort control shows, and whether it carries the dot.
        fn effort_choice(&self) -> (String, bool) {
            self.read(|modal, cx| {
                let option = modal.effort_select.read(cx).selected().unwrap();
                (option.label.to_string(), option.marked)
            })
        }

        fn effort_disabled(&self) -> bool {
            self.read(|modal, cx| modal.effort_select.read(cx).is_disabled())
        }

        fn overrides(&self) -> RoleOverrides {
            self.read(|modal, cx| modal.role_overrides(cx))
        }
    }

    use gpui::{size, Render, TestAppContext, VisualTestContext, WindowHandle};
    use runner_daemon::{db, session, shell_path};
    use std::sync::RwLock;

    fn test_role(handle: &str, runtime: &str) -> Role {
        Role {
            id: format!("role-{handle}"),
            handle: handle.into(),
            display_name: format!("Role {handle}"),
            runtime: runtime.into(),
            command: runtime.into(),
            args: Vec::new(),
            working_dir: None,
            system_prompt: None,
            env: Default::default(),
            model: None,
            effort: None,
            codex_speed: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    /// Claude Code and Codex as a detected catalog offers them.
    fn agents() -> Vec<RuntimeCatalogEntry> {
        let mut claude = runtime("claude-code", &["", "low", "high", "xhigh"]);
        claude.display_name = "Claude Code".into();
        claude.command = "claude".into();
        claude.default_effort = Some("high".into());
        let mut codex = runtime("codex", &["", "low", "medium"]);
        codex.display_name = "Codex".into();
        codex.default_model = Some("gpt-5.5".into());
        codex.default_effort = Some("medium".into());
        vec![claude, codex]
    }

    /// Antigravity as the catalog lists it: a default model, and levels per model.
    fn antigravity() -> RuntimeCatalogEntry {
        let mut agy = runtime("antigravity", &["", "low", "medium", "high"]);
        agy.display_name = "Antigravity CLI".into();
        agy.command = "agy".into();
        agy.default_model = Some("gemini-3.1-pro".into());
        agy.models = vec![RuntimeCatalogOption {
            value: "gemini-3.1-pro".into(),
            label: "Gemini 3.1 Pro".into(),
            description: None,
            supported_efforts: Some(vec!["low".into(), "high".into()]),
        }];
        agy
    }

    fn claude_role() -> Role {
        Role {
            model: Some("opus[1m]".into()),
            effort: Some("xhigh".into()),
            ..test_role("impl-claude", "claude-code")
        }
    }

    fn codex_role() -> Role {
        Role {
            codex_speed: Some(CodexSpeed::Fast),
            ..test_role("impl-codex", "codex")
        }
    }

    #[test]
    fn mode_switch_and_direct_speed_follow_the_selected_agent() {
        let mut modal = modal_harness(1200., 1000., Vec::new(), agents(), ChatMode::Runtime);
        for mode in [ChatMode::Runtime, ChatMode::Role, ChatMode::Runtime] {
            modal.act(move |root, _, cx| root.set_start_chat_mode(mode, cx));
            let form = modal.bounds("START_CHAT_FORM").unwrap();
            let modes = modal.bounds("START_CHAT_MODES").unwrap();
            let role = modal.bounds("START_CHAT_ROLE_MODE").unwrap();
            let direct = modal.bounds("START_CHAT_DIRECT_MODE").unwrap();
            assert!(direct.origin.x < role.origin.x);
            assert!(modes.size.width >= px(480.), "{mode:?}: {modes:?}");
            assert_eq!(modes.size.width, form.size.width, "{mode:?}: {form:?}");
            assert!((role.size.width - direct.size.width).abs() <= px(1.));
            assert!(role.size.width > px(200.));
        }
        assert!(!modal.read(|modal, _| modal.codex_speed_visible()));

        modal.choose(StartChatSelection::Runtime, "codex");
        assert!(modal.bounds("START_CHAT_SPEED_FIELD").is_some());
        assert!(modal.bounds("START_CHAT_SPEED_NOTE").is_none());
        modal.choose(StartChatSelection::Speed, "fast");
        assert!(modal.bounds("START_CHAT_SPEED_NOTE").is_some());

        modal.choose(StartChatSelection::Runtime, "claude-code");
        assert_eq!(
            modal.read(|modal, _| (modal.codex_speed_visible(), modal.speed.clone())),
            (false, "inherit".into())
        );
    }

    #[test]
    fn a_role_chat_with_untouched_overrides_sends_none() {
        let role = Some(("coder", "claude-code"));
        assert_eq!(
            build_start_request(
                ChatMode::Role,
                role,
                Some("codex"),
                None,
                None,
                None,
                None,
                Some("/repo".into()),
            ),
            Some(StartRequest::Role {
                role_id: "coder".into(),
                runtime: None,
                model: None,
                effort: None,
                speed: None,
                cwd: Some("/repo".into()),
            })
        );
        // Speed alone does not pin the role's agent.
        assert_eq!(
            build_start_request(
                ChatMode::Role,
                role,
                None,
                None,
                None,
                None,
                Some(CodexSpeed::Fast),
                None
            ),
            Some(StartRequest::Role {
                role_id: "coder".into(),
                runtime: None,
                model: None,
                effort: None,
                speed: Some(CodexSpeed::Fast),
                cwd: None,
            })
        );
    }

    #[test]
    fn a_model_or_effort_changed_on_the_roles_agent_sends_that_agent_with_the_change() {
        let role = Some(("coder", "claude-code"));
        for (model, effort) in [
            (Some("sonnet"), None),
            (None, Some("low")),
            (Some("sonnet"), Some("low")),
        ] {
            assert_eq!(
                build_start_request(
                    ChatMode::Role,
                    role,
                    None,
                    None,
                    model.map(Into::into),
                    effort.map(Into::into),
                    None,
                    None,
                ),
                Some(StartRequest::Role {
                    role_id: "coder".into(),
                    runtime: Some("claude-code".into()),
                    model: model.map(Into::into),
                    effort: effort.map(Into::into),
                    speed: None,
                    cwd: None,
                })
            );
        }
    }

    #[test]
    fn another_agent_goes_with_its_own_defaults_and_direct_chats_start_as_before() {
        assert_eq!(
            build_start_request(
                ChatMode::Role,
                Some(("coder", "claude-code")),
                None,
                Some("codex"),
                None,
                None,
                None,
                None,
            ),
            Some(StartRequest::Role {
                role_id: "coder".into(),
                runtime: Some("codex".into()),
                model: None,
                effort: None,
                speed: None,
                cwd: None,
            })
        );
        assert_eq!(
            build_start_request(
                ChatMode::Runtime,
                Some(("coder", "claude-code")),
                Some("codex"),
                Some("claude-code"),
                Some("gpt-5.6-sol".into()),
                Some("high".into()),
                Some(CodexSpeed::Standard),
                None,
            ),
            Some(StartRequest::Runtime {
                runtime: "codex".into(),
                model: Some("gpt-5.6-sol".into()),
                effort: Some("high".into()),
                speed: Some(CodexSpeed::Standard),
                cwd: None,
            })
        );
        assert_eq!(
            build_start_request(
                ChatMode::Runtime,
                None,
                Some("codex"),
                None,
                None,
                None,
                None,
                None,
            ),
            Some(StartRequest::Runtime {
                runtime: "codex".into(),
                model: None,
                effort: None,
                speed: None,
                cwd: None,
            })
        );
        assert_eq!(
            build_start_request(ChatMode::Role, None, None, None, None, None, None, None),
            None
        );
    }

    #[test]
    fn an_untouched_role_chat_shows_the_roles_values_and_sends_no_override() {
        let mut modal = modal_harness(1200., 1000., vec![claude_role()], agents(), ChatMode::Role);
        assert!(modal.bounds("START_CHAT_SETUP").is_some());
        assert_eq!(modal.overrides(), RoleOverrides::default());
        modal.read(|modal, cx| {
            assert_eq!(modal.model_override(cx), None);
            assert_eq!(modal.effort_override(), None);
            assert_eq!(modal.speed_override(), None);
            assert_eq!(modal.role_runtime_override, None);
            assert_eq!(modal.effort, "xhigh");
            assert_eq!(
                model_placeholder(inheriting_role(modal)).resolve(),
                "opus[1m]"
            );
            assert!(modal.model.read(cx).placeholder_uses_value_style());
        });
        assert_eq!(modal.effort_choice(), ("xhigh".into(), false));

        // A control still showing the role's value is untouched, however it got there.
        modal.type_model("opus[1m]");
        modal.choose(StartChatSelection::Effort, "low");
        modal.choose(StartChatSelection::Effort, "xhigh");
        assert_eq!(modal.overrides(), RoleOverrides::default());
        modal.read(|modal, cx| {
            assert_eq!(modal.model_override(cx), None);
            assert_eq!(modal.effort_override(), None);
        });
    }

    #[test]
    fn a_changed_value_carries_the_dot_and_its_reset_restores_only_that_value() {
        let mut modal = modal_harness(1200., 1000., vec![claude_role()], agents(), ChatMode::Role);
        modal.type_model("sonnet");
        modal.choose(StartChatSelection::Effort, "low");
        assert_eq!(
            modal.overrides(),
            RoleOverrides {
                runtime: false,
                model: true,
                effort: true,
                speed: false
            }
        );
        assert!(modal.bounds("MODEL_FIELD_MARK").is_some());
        assert_eq!(modal.effort_choice(), ("low".into(), true));
        assert!(modal
            .bounds("START_CHAT_NOTE Model role: opus[1m]")
            .is_some());
        assert!(modal.bounds("START_CHAT_NOTE Effort role: xhigh").is_some());

        modal.reset(ResetKind::Model);
        modal.read(|modal, cx| {
            assert_eq!(modal.model.read(cx).text(), "");
            assert_eq!(modal.effort, "low", "Reset restores one value only");
        });
        assert_eq!(
            modal.overrides(),
            RoleOverrides {
                effort: true,
                ..RoleOverrides::default()
            }
        );

        modal.reset(ResetKind::Effort);
        assert_eq!(modal.read(|modal, _| modal.effort.clone()), "xhigh");
        assert_eq!(modal.effort_choice(), ("xhigh".into(), false));
        assert_eq!(modal.overrides(), RoleOverrides::default());
    }

    #[test]
    fn choosing_the_roles_own_agent_is_not_an_override() {
        let mut modal = modal_harness(1200., 1000., vec![claude_role()], agents(), ChatMode::Role);
        modal.choose(StartChatSelection::RoleRuntime, "codex");
        assert_eq!(
            modal.read(|modal, _| modal.role_runtime_override.clone()),
            Some("codex".into())
        );
        modal.choose(StartChatSelection::RoleRuntime, "claude-code");
        modal.read(|modal, cx| {
            assert_eq!(modal.role_runtime_override, None);
            assert_eq!(modal.role_runtime_select.read(cx).value(), "claude-code");
        });
        assert_eq!(modal.overrides(), RoleOverrides::default());
    }

    #[test]
    fn another_role_starts_from_its_own_setup() {
        let other = Role {
            model: Some("gpt-5.5".into()),
            ..test_role("other", "codex")
        };
        let mut modal = modal_harness(
            1200.,
            1000.,
            vec![claude_role(), other.clone()],
            agents(),
            ChatMode::Role,
        );
        modal.choose(StartChatSelection::RoleRuntime, "codex");
        modal.type_model("gpt-5.6-sol");
        modal.choose(StartChatSelection::Role, &other.id);
        modal.read(|modal, cx| {
            assert_eq!(modal.role_runtime_override, None);
            assert_eq!(modal.model.read(cx).text(), "");
            assert_eq!(modal.effort, "medium", "the codex agent's own default");
        });
        assert_eq!(modal.overrides(), RoleOverrides::default());
    }

    #[test]
    fn another_agent_shows_its_own_defaults_at_full_contrast_and_only_runtime_carries_the_dot() {
        let mut modal = modal_harness(1200., 1000., vec![claude_role()], agents(), ChatMode::Role);
        modal.choose(StartChatSelection::RoleRuntime, "codex");
        assert_eq!(
            modal.overrides(),
            RoleOverrides {
                runtime: true,
                ..RoleOverrides::default()
            }
        );
        assert!(modal.read(|modal, cx| {
            modal
                .role_runtime_select
                .read(cx)
                .selected()
                .unwrap()
                .marked
        }));
        assert!(modal
            .bounds("START_CHAT_NOTE Runtime role: Claude Code")
            .is_some());
        assert!(modal.bounds("START_CHAT_RESET Runtime").is_some());
        modal.read(|modal, cx| {
            assert!(inheriting_role(modal).is_none());
            // The agent's defaults, not the role's model and effort. The
            // model stays generic because no concrete value reaches launch.
            assert_eq!(
                model_placeholder(inheriting_role(modal)).resolve(),
                "default"
            );
            assert_eq!(modal.effort, "medium");
            assert_eq!(modal.model_override(cx), None);
            assert_eq!(modal.effort_override(), None);
        });
        assert_eq!(modal.effort_choice(), ("medium".into(), false));

        // Changing the agent's model is not the role's to override: no dot.
        modal.type_model("gpt-5.6-sol");
        assert!(modal.bounds("MODEL_FIELD_MARK").is_none());
        modal.read(|modal, cx| {
            assert_eq!(modal.model_override(cx), Some("gpt-5.6-sol".into()));
        });

        modal.reset(ResetKind::Runtime);
        modal.read(|modal, cx| {
            assert_eq!(modal.role_runtime_override, None);
            assert_eq!(modal.model.read(cx).text(), "");
            assert_eq!(modal.effort, "xhigh");
            assert_eq!(modal.role_runtime_select.read(cx).value(), "claude-code");
        });
        assert_eq!(modal.overrides(), RoleOverrides::default());
    }

    #[test]
    fn a_role_without_a_model_or_effort_starts_from_its_agents_defaults() {
        let modal = modal_harness(
            1200.,
            1000.,
            vec![test_role("plain", "claude-code")],
            agents(),
            ChatMode::Role,
        );
        modal.read(|modal, cx| {
            assert_eq!(
                model_placeholder(inheriting_role(modal)).resolve(),
                "default",
                "the role does not pin a model"
            );
            assert_eq!(modal.effort, "high", "the agent's default effort");
            assert_eq!(modal.effort_override(), None);
            assert_eq!(modal.model_override(cx), None);
            assert!(modal.model.read(cx).placeholder_uses_value_style());
        });
        assert_eq!(modal.effort_choice(), ("high".into(), false));
        drop(modal);

        // No default effort known: the control reads `default`, never a blank.
        let mut claude = agents().remove(0);
        claude.default_effort = None;
        let modal = modal_harness(
            1200.,
            1000.,
            vec![test_role("plain", "claude-code")],
            vec![claude],
            ChatMode::Role,
        );
        assert_eq!(modal.effort_choice(), ("default".into(), false));
        modal.read(|modal, _| assert_eq!(modal.effort, ""));
    }

    #[test]
    fn direct_model_placeholder_does_not_expose_the_cached_default() {
        let modal = modal_harness(
            1200.,
            1000.,
            Vec::new(),
            vec![agents().remove(1)],
            ChatMode::Runtime,
        );
        modal.read(|modal, cx| {
            assert_eq!(
                modal.active_runtime().unwrap().default_model.as_deref(),
                Some("gpt-5.5")
            );
            assert_eq!(
                model_placeholder(inheriting_role(modal)).resolve(),
                "default"
            );
            assert!(modal.model.read(cx).placeholder_uses_value_style());
        });
    }

    #[test]
    fn speed_options_mark_values_that_override_the_role() {
        let values = |options: Vec<SelectOption>| {
            options
                .iter()
                .map(|option| (option.value.clone(), option.marked))
                .collect::<Vec<_>>()
        };
        // A role that sets a speed cannot be told to inherit: no such choice.
        assert_eq!(
            values(speed_options(Some("fast"), true)),
            [("standard".to_owned(), true), ("fast".to_owned(), false)]
        );
        assert_eq!(
            values(speed_options(Some("inherit"), false)),
            [
                ("inherit".to_owned(), false),
                ("standard".to_owned(), true),
                ("fast".to_owned(), true)
            ]
        );
        assert_eq!(
            values(speed_options(None, false)),
            [
                ("inherit".to_owned(), false),
                ("standard".to_owned(), false),
                ("fast".to_owned(), false)
            ]
        );
    }

    #[test]
    fn effort_is_changeable_without_typing_a_model() {
        // Direct on an agent whose default model is not known.
        let mut modal = modal_harness(1200., 1000., Vec::new(), agents(), ChatMode::Runtime);
        assert_eq!(modal.effort_choice(), ("high".into(), false));
        modal.pick(
            |modal, cx| modal.effort_select.read(cx).focus_handle(),
            "enter down enter",
        );
        modal.read(|modal, _| {
            assert_eq!(modal.effort, "xhigh");
            assert_eq!(modal.effort_override(), Some("xhigh".into()));
        });
        drop(modal);

        // And on a role that has no model of its own.
        let mut modal = modal_harness(
            1200.,
            1000.,
            vec![test_role("plain", "claude-code")],
            agents(),
            ChatMode::Role,
        );
        modal.pick(
            |modal, cx| modal.effort_select.read(cx).focus_handle(),
            "enter up enter",
        );
        modal.read(|modal, _| {
            assert_eq!(modal.effort, "low");
            assert_eq!(modal.effort_override(), Some("low".into()));
        });
        assert!(modal.overrides().effort);
    }

    /// The launch gives Antigravity `--effort` only beside `--model`, so an
    /// effort with no model that reaches the launch would be shown and dropped.
    /// The `default` placeholder does not reach it.
    #[test]
    fn antigravity_effort_waits_for_a_model_that_reaches_the_launch() {
        let effort_picker: fn(&StartChatModal, &App) -> FocusHandle =
            |modal, cx| modal.effort_select.read(cx).focus_handle();

        // Direct: the runtime default is not pinned by the placeholder.
        let mut modal = modal_harness(
            1200.,
            1000.,
            Vec::new(),
            vec![antigravity()],
            ChatMode::Runtime,
        );
        modal.read(|modal, _| {
            assert_eq!(
                model_placeholder(inheriting_role(modal)).resolve(),
                "default"
            );
        });
        assert_eq!(modal.effort_choice(), ("default".into(), false));
        // No key is sent to it: Enter on a disabled select would submit the form.
        assert!(modal.effort_disabled());
        modal.read(|modal, _| {
            assert_eq!(modal.effort, "");
            assert_eq!(modal.effort_override(), None);
        });

        // A typed model is a pair the launch keeps.
        modal.type_model("gemini-3.1-pro");
        assert!(!modal.effort_disabled());
        modal.pick(effort_picker, "enter down enter");
        modal.read(|modal, cx| {
            assert_eq!(modal.effort_override(), Some("low".into()));
            assert_eq!(modal.model_override(cx), Some("gemini-3.1-pro".into()));
        });
        // Its levels are the model's own: no medium.
        modal.pick(effort_picker, "enter down enter");
        assert_eq!(modal.read(|modal, _| modal.effort.clone()), "high");

        // Clearing the model takes the effort with it.
        modal.type_model("");
        assert!(modal.effort_disabled());
        modal.read(|modal, _| {
            assert_eq!(modal.effort, "");
            assert_eq!(modal.effort_override(), None);
        });
        drop(modal);

        // A role with no model of its own has nothing for an effort to ride on.
        let modal = modal_harness(
            1200.,
            1000.,
            vec![test_role("plain", "antigravity")],
            vec![antigravity()],
            ChatMode::Role,
        );
        assert!(modal.effort_disabled());
        modal.read(|modal, _| assert_eq!(modal.effort_override(), None));
        assert!(!modal.overrides().effort);
        drop(modal);

        // A role's model reaches the launch, so its effort can change.
        let mut modal = modal_harness(
            1200.,
            1000.,
            vec![Role {
                model: Some("gemini-3.1-pro".into()),
                ..test_role("pro", "antigravity")
            }],
            vec![antigravity()],
            ChatMode::Role,
        );
        modal.pick(effort_picker, "enter down enter");
        modal.read(|modal, cx| {
            assert_eq!(modal.effort_override(), Some("low".into()));
            assert_eq!(modal.model_override(cx), None, "the role's model is kept");
        });
        drop(modal);

        // Another role sent to Antigravity has no model until one is typed.
        let mut modal = modal_harness(
            1200.,
            1000.,
            vec![claude_role()],
            [agents(), vec![antigravity()]].concat(),
            ChatMode::Role,
        );
        modal.choose(StartChatSelection::RoleRuntime, "antigravity");
        assert!(modal.effort_disabled());
        modal.read(|modal, _| assert_eq!(modal.effort_override(), None));
        modal.type_model("gemini-3.1-pro");
        assert!(!modal.effort_disabled());
        modal.pick(effort_picker, "enter down enter");
        modal.read(|modal, _| assert_eq!(modal.effort_override(), Some("low".into())));
    }

    #[test]
    fn role_speed_remains_visible_when_its_agent_is_not_selectable() {
        let mut modal = modal_harness(
            1200.,
            1000.,
            vec![codex_role()],
            vec![agents().remove(0)],
            ChatMode::Role,
        );
        assert!(modal.read(|modal, _| modal.codex_speed_visible()));
        assert!(modal.bounds("START_CHAT_SPEED_FIELD").is_some());
        modal.read(|modal, _| assert_eq!(modal.baseline_speed(), "fast"));
    }

    #[test]
    fn speed_shows_for_codex_roles_codex_overrides_and_direct_codex() {
        // A Codex role's speed is a control from the start.
        let mut modal = modal_harness(1200., 1000., vec![codex_role()], agents(), ChatMode::Role);
        assert!(modal.read(|modal, _| modal.codex_speed_visible()));
        assert!(modal.bounds("START_CHAT_SPEED_FIELD").is_some());
        assert!(modal.bounds("START_CHAT_SPEED_NOTE").is_some());
        modal.read(|modal, cx| {
            let option = modal.speed_select.read(cx).selected().unwrap();
            assert_eq!((option.label.as_ref(), option.marked), ("Fast", false));
            assert_eq!(modal.speed_override(), None);
        });
        modal.choose(StartChatSelection::Speed, "standard");
        assert_eq!(
            modal.overrides(),
            RoleOverrides {
                speed: true,
                ..RoleOverrides::default()
            }
        );
        assert!(modal.bounds("START_CHAT_NOTE Speed role: Fast").is_some());
        modal.reset(ResetKind::Speed);
        modal.read(|modal, _| {
            assert_eq!(modal.speed, "fast");
            assert_eq!(modal.speed_override(), None);
        });
        drop(modal);

        // A Claude role gains it by choosing Codex.
        let mut modal = modal_harness(1200., 1000., vec![claude_role()], agents(), ChatMode::Role);
        assert!(!modal.read(|modal, _| modal.codex_speed_visible()));
        modal.choose(StartChatSelection::RoleRuntime, "codex");
        assert!(modal.read(|modal, _| modal.codex_speed_visible()));
        assert!(modal.bounds("START_CHAT_SPEED_FIELD").is_some());
        modal.read(|modal, cx| {
            let option = modal.speed_select.read(cx).selected().unwrap();
            assert_eq!((option.label.as_ref(), option.marked), ("Inherit", false));
        });
        drop(modal);

        // Direct chats show it for Codex only.
        let mut modal = modal_harness(1200., 1000., Vec::new(), agents(), ChatMode::Runtime);
        assert!(!modal.read(|modal, _| modal.codex_speed_visible()));
        modal.choose(StartChatSelection::Runtime, "codex");
        assert!(modal.read(|modal, _| modal.codex_speed_visible()));
        assert!(modal.bounds("START_CHAT_SPEED_FIELD").is_some());
    }

    /// The pick goes through `StyledSelect::choose`, which holds the select
    /// while it calls back: a callback that touches that select again panics.
    #[test]
    fn every_select_takes_a_pick_made_from_its_open_menu() {
        let other = test_role("other", "codex");
        let mut modal = modal_harness(
            1200.,
            1000.,
            vec![claude_role(), other.clone()],
            agents(),
            ChatMode::Role,
        );
        let role_picker: fn(&StartChatModal, &App) -> FocusHandle =
            |modal, cx| modal.role_select.read(cx).focus_handle();
        modal.pick(role_picker, "enter down enter");
        assert_eq!(
            modal.read(|modal, _| modal.role_id.clone()),
            Some(other.id.clone())
        );
        modal.pick(role_picker, "enter up enter");
        assert_eq!(
            modal.read(|modal, _| modal.role_id.clone()),
            Some(claude_role().id)
        );

        let agent: fn(&StartChatModal, &App) -> FocusHandle =
            |modal, cx| modal.role_runtime_select.read(cx).focus_handle();
        // The role's own agent again is no override; another is.
        modal.pick(agent, "enter enter");
        assert_eq!(
            modal.read(|modal, _| modal.role_runtime_override.clone()),
            None
        );
        modal.pick(agent, "enter down enter");
        assert_eq!(
            modal.read(|modal, _| modal.role_runtime_override.clone()),
            Some("codex".into())
        );
        modal.read(|modal, cx| {
            assert_eq!(modal.role_runtime_select.read(cx).value(), "codex");
        });
        modal.pick(agent, "enter up enter");
        modal.read(|modal, cx| {
            assert_eq!(modal.role_runtime_override, None);
            assert_eq!(modal.role_runtime_select.read(cx).value(), "claude-code");
        });

        // xhigh is the role's; up two levels is low.
        modal.pick(
            |modal, cx| modal.effort_select.read(cx).focus_handle(),
            "enter up up enter",
        );
        assert_eq!(modal.read(|modal, _| modal.effort.clone()), "low");
        assert_eq!(modal.effort_choice(), ("low".into(), true));
        drop(modal);

        let mut modal = modal_harness(1200., 1000., vec![codex_role()], agents(), ChatMode::Role);
        modal.pick(
            |modal, cx| modal.speed_select.read(cx).focus_handle(),
            "enter up enter",
        );
        assert_eq!(modal.read(|modal, _| modal.speed.clone()), "standard");
        drop(modal);

        let mut modal = modal_harness(1200., 1000., Vec::new(), agents(), ChatMode::Runtime);
        modal.pick(
            |modal, cx| modal.runtime_select.read(cx).focus_handle(),
            "enter down enter",
        );
        assert_eq!(
            modal.read(|modal, _| modal.runtime_name.clone()),
            Some("codex".into())
        );
    }

    /// The override dot has a slot of its own in a control, so a long value
    /// ends before it rather than running underneath.
    #[test]
    fn a_long_overridden_value_ends_before_the_dot() {
        let mut claude = agents().remove(0);
        claude.efforts.push(RuntimeCatalogOption {
            value: "an-effort-level-far-too-long-for-its-narrow-column".into(),
            label: "an-effort-level-far-too-long-for-its-narrow-column".into(),
            description: None,
            supported_efforts: None,
        });
        let mut modal = modal_harness(
            640.,
            480.,
            vec![claude_role()],
            vec![claude],
            ChatMode::Role,
        );
        modal.choose(
            StartChatSelection::Effort,
            "an-effort-level-far-too-long-for-its-narrow-column",
        );
        let text = modal
            .bounds("STYLED_SELECT_VALUE_start-chat-effort")
            .unwrap();
        let mark = modal.bounds("STYLED_SELECT_MARK").unwrap();
        let trigger = modal.bounds("STYLED_SELECT_start-chat-effort").unwrap();
        assert!(text.right() <= mark.left(), "{text:?} {mark:?}");
        assert!(mark.right() <= trigger.right(), "{mark:?} {trigger:?}");

        // A long model reserves the same strip at the text field's right.
        modal.type_model("a-model-name-that-is-far-too-long-for-its-narrow-column-to-hold");
        let mark = modal.bounds("MODEL_FIELD_MARK").unwrap();
        let field = modal.bounds("START_CHAT_FIELD Model").unwrap();
        assert!(
            mark.left() >= field.left() && mark.right() <= field.right(),
            "{mark:?} in {field:?}"
        );
    }

    #[test]
    fn confirm_binding_precedes_every_control_and_preserves_open_menu_values() {
        for index in 0..15 {
            let mut modal = modal_harness(
                1200.,
                1000.,
                vec![codex_role(), claude_role()],
                agents(),
                ChatMode::Role,
            );
            modal.type_model("custom-model");
            modal.choose(StartChatSelection::Effort, "low");
            modal.choose(StartChatSelection::Speed, "standard");
            let focus = modal.read(|modal, cx| start_chat_focus_order(modal, cx)[index].clone());
            modal.act(|_, window, cx| focus.focus(window, cx));
            modal
                .visual
                .simulate_keystrokes(&keymap::platform_default("cmd-enter"));
            modal.read(|modal, cx| {
                assert!(
                    modal.error.is_some(),
                    "confirm did not submit at control {index}"
                );
                assert_eq!(modal.model.read(cx).text(), "custom-model");
                assert_eq!(modal.effort, "low");
                assert_eq!(modal.speed, "standard");
            });
        }
        for picker in 0..5 {
            let mut modal = modal_harness(
                1200.,
                1000.,
                vec![codex_role(), claude_role()],
                agents(),
                ChatMode::Role,
            );
            if picker == 4 {
                let missing = modal
                    ._temp
                    .path()
                    .join("missing")
                    .to_string_lossy()
                    .into_owned();
                modal.edit(|form, cx| form.cwd.update(cx, |field, cx| field.set_text(missing, cx)));
                modal.act(|root, window, cx| {
                    root.switch_start_chat_mode(ChatMode::Runtime, window, cx)
                });
            }
            let select = modal.read(|modal, _| match picker {
                0 => modal.role_select.clone(),
                1 => modal.role_runtime_select.clone(),
                2 => modal.effort_select.clone(),
                3 => modal.speed_select.clone(),
                _ => modal.runtime_select.clone(),
            });
            let original = select.read_with(&modal.visual, |select, _| select.value().to_owned());
            let focus = select.read_with(&modal.visual, |select, _| select.focus_handle());
            modal.act(|_, window, cx| focus.focus(window, cx));
            modal.visual.simulate_keystrokes("enter down");
            modal
                .visual
                .simulate_keystrokes(&keymap::platform_default("cmd-enter"));
            assert_eq!(
                select.read_with(&modal.visual, |select, _| select.value().to_owned()),
                original
            );
            modal.read(|modal, _| assert!(modal.error.is_some()));
        }
    }

    #[test]
    fn enter_opens_and_chooses_a_model_without_submitting_the_chat() {
        for mode in [ChatMode::Role, ChatMode::Runtime] {
            let mut catalog = agents();
            catalog
                .iter_mut()
                .find(|runtime| runtime.name == Runtime::Codex)
                .unwrap()
                .models = ["first-model", "second-model"]
                .into_iter()
                .map(|value| RuntimeCatalogOption {
                    value: value.into(),
                    label: value.into(),
                    description: None,
                    supported_efforts: None,
                })
                .collect();
            let mut modal = modal_harness(1200., 1000., vec![codex_role()], catalog, mode);
            if mode == ChatMode::Runtime {
                modal.choose(StartChatSelection::Runtime, "codex");
            }
            let missing = modal
                ._temp
                .path()
                .join("missing-model-key-test")
                .to_string_lossy()
                .into_owned();
            modal.edit(|form, cx| form.cwd.update(cx, |field, cx| field.set_text(missing, cx)));
            modal.pick(|form, cx| form.model.read(cx).focus_handle(), "enter");
            modal.read(|form, _| {
                assert!(form.error.is_none(), "Enter must open the model chooser");
                assert!(!form.submitting);
            });
            modal.visual.simulate_keystrokes("down enter");
            modal.read(|form, cx| {
                assert_eq!(form.model.read(cx).text(), "second-model");
                assert!(form.error.is_none(), "Choosing a model must not submit");
                assert!(!form.submitting);
            });
            modal
                .visual
                .simulate_keystrokes(&keymap::platform_default("cmd-enter"));
            modal.read(|form, cx| {
                assert!(form.error.is_some(), "The confirm shortcut must submit");
                assert_eq!(form.model.read(cx).text(), "second-model");
            });
        }
    }

    #[test]
    fn confirm_from_model_menu_and_runtime_reset_preserves_the_shown_values() {
        let mut modal = modal_harness(1200., 1000., vec![codex_role()], agents(), ChatMode::Role);
        let mut catalog = agents();
        catalog[1].models = vec![RuntimeCatalogOption {
            value: "suggested".into(),
            label: "Suggested".into(),
            description: None,
            supported_efforts: None,
        }];
        modal.edit(|form, cx| seed_modal(form, vec![codex_role()], catalog, ChatMode::Role, cx));
        modal.type_model("custom");
        modal.pick(|form, cx| form.model.read(cx).focus_handle(), "down");
        modal
            .visual
            .simulate_keystrokes(&keymap::platform_default("cmd-enter"));
        modal.read(|form, cx| {
            assert!(form.error.is_some());
            assert_eq!(form.model.read(cx).text(), "custom");
        });
        modal.choose(StartChatSelection::RoleRuntime, "claude-code");
        let focus = modal.read(|form, _| form.reset_focus[ResetKind::Runtime as usize].clone());
        modal.act(|_, window, cx| focus.focus(window, cx));
        modal
            .visual
            .simulate_keystrokes(&keymap::platform_default("cmd-enter"));
        modal.read(|form, _| {
            assert!(form.error.is_some());
            assert_eq!(form.role_runtime_override.as_deref(), Some("claude-code"));
        });
    }

    #[test]
    fn new_chats_default_to_direct_with_empty_modes_focusing_the_title() {
        let mut modal = modal_harness(1200., 1000., Vec::new(), Vec::new(), ChatMode::Runtime);
        modal.act(|root, window, cx| {
            let input =
                serde_json::from_value(serde_json::to_value(test_role("saved", "codex")).unwrap())
                    .unwrap();
            let role = root.core(cx).role_create(input).unwrap();
            fs::write(
                root.app_store.read(cx).app_data_dir.join("start-chat-mode"),
                "role",
            )
            .unwrap();
            root.set_start_chat_mode(ChatMode::Role, cx);
            root.close_start_chat_modal(window, cx);
            root.open_new_tab_modal(&NewTab, window, cx);
            let form = root.start_chat_modal.as_ref().unwrap();
            assert_eq!(form.mode, ChatMode::Runtime);
            assert!(form.picker_focus(cx).is_focused(window));
            root.set_start_chat_mode(ChatMode::Role, cx);
            root.close_start_chat_modal(window, cx);
            let mut layout = PaneLayout::single(None, &[]);
            layout.id = "01M3VD00000000000000000004".into();
            let pane_id = layout.focused_pane_id.clone();
            root.core(cx)
                .node_tab_upsert(layout.upsert_input().unwrap())
                .unwrap();
            root.reload_tabs(cx).unwrap();
            root.last_focused_role_id = Some(role.id.clone());
            root.open_pane_chat_modal(&pane_id, window, cx);
            let form = root.start_chat_modal.as_ref().unwrap();
            assert_eq!(form.mode, ChatMode::Runtime);
            assert!(form.picker_focus(cx).is_focused(window));
            assert_eq!(form.role_id.as_deref(), Some(role.id.as_str()));
            root.set_start_chat_mode(ChatMode::Role, cx);
            assert_eq!(root.start_chat_modal.as_ref().unwrap().mode, ChatMode::Role);
        });
        for mode in [ChatMode::Runtime, ChatMode::Role] {
            modal.edit(|form, cx| seed_modal(form, Vec::new(), Vec::new(), mode, cx));
            modal.act(|root, window, cx| {
                let form = root.start_chat_modal.as_ref().unwrap();
                form.picker_focus(cx).focus(window, cx);
                assert!(form.title.read(cx).focus_handle().is_focused(window));
            });
        }
        modal.edit(|form, cx| seed_modal(form, Vec::new(), agents(), ChatMode::Runtime, cx));
        modal.act(|root, window, cx| {
            let form = root.start_chat_modal.as_ref().unwrap();
            form.picker_focus(cx).focus(window, cx);
            assert!(form
                .runtime_select
                .read(cx)
                .focus_handle()
                .is_focused(window));
        });
    }

    #[test]
    #[cfg(unix)]
    fn terminal_key_fills_only_the_focused_empty_pane_and_otherwise_opens_a_tab() {
        let mut modal = modal_harness(1200., 1000., Vec::new(), Vec::new(), ChatMode::Runtime);
        let cwd = modal._temp.path().to_string_lossy().into_owned();
        modal.act(|root, window, cx| {
            root.close_start_chat_modal(window, cx);
            root.app_store.update(cx, |store, cx| {
                store.settings.default_working_dir = cwd.clone();
                cx.notify();
            });
            root.new_terminal_action(&NewTerminal, window, cx);
        });
        let mut first_tab = String::new();
        let mut empty = String::new();
        let mut extra = String::new();
        modal.act(|root, _, cx| {
            let layout = root.tabs.active_mut().unwrap();
            first_tab = layout.id.clone();
            empty = layout
                .split(&layout.focused_pane_id.clone(), SplitOrientation::Row)
                .unwrap();
            let input = layout.upsert_input().unwrap();
            root.core(cx).node_tab_upsert(input).unwrap();
        });
        modal.act(|root, window, cx| {
            root.app_store.update(cx, |store, cx| {
                store.settings.default_working_dir = "/tmp".into();
                cx.notify();
            });
            assert_eq!(root.focused_empty_chat_pane(), Some(empty.clone()));
            root.new_terminal_action(&NewTerminal, window, cx);
        });
        modal.act(|root, window, cx| {
            let layout = root.tabs.active().unwrap();
            assert_eq!(layout.id, first_tab);
            assert_eq!(layout.root.leaves().len(), 2);
            assert_eq!(layout.focused_pane_id, empty);
            let session = root
                .session_entry(layout.focused_session_id().unwrap(), cx)
                .unwrap();
            assert_eq!(session.cwd.as_deref(), Some(cwd.as_str()));
            assert_eq!(root.tabs.tabs().len(), 1);
            root.new_terminal_action(&NewTerminal, window, cx);
        });
        modal.act(|root, _, _| {
            assert_eq!(root.tabs.tabs().len(), 2);
        });
        modal.act(|root, window, cx| {
            root.tabs.activate(&first_tab);
            let layout = root.tabs.active_mut().unwrap();
            extra = layout
                .split(&layout.focused_pane_id.clone(), SplitOrientation::Row)
                .unwrap();
            let occupied = layout
                .root
                .leaves()
                .into_iter()
                .find(|leaf| leaf.session_id.is_some())
                .unwrap()
                .id
                .clone();
            layout.focus_pane(&occupied);
            let input = layout.upsert_input().unwrap();
            root.core(cx).node_tab_upsert(input).unwrap();
            assert_eq!(root.focused_empty_chat_pane(), None);
            root.new_terminal_action(&NewTerminal, window, cx);
        });
        modal.act(|root, window, cx| {
            assert_eq!(root.tabs.tabs().len(), 3);
            assert!(root
                .tabs
                .tabs()
                .iter()
                .find(|tab| tab.id == first_tab)
                .unwrap()
                .root
                .leaves()
                .into_iter()
                .find(|leaf| leaf.id == extra)
                .unwrap()
                .session_id
                .is_none());
            root.set_route(AppRoute::Crews, cx);
            root.new_terminal_action(&NewTerminal, window, cx);
        });
        modal.act(|root, _, cx| {
            assert_eq!(root.route, AppRoute::Chat);
            assert_eq!(root.tabs.tabs().len(), 4);
            for session in root.app_store.read(cx).sessions.clone() {
                root.core(cx).session_close(&session.session_id).unwrap();
            }
        });
    }

    #[test]
    #[cfg(unix)]
    fn terminal_key_from_a_mission_route_opens_a_chat_tab_in_the_active_project() {
        let mut modal = modal_harness(1200., 1000., Vec::new(), Vec::new(), ChatMode::Runtime);
        let cwd = modal._temp.path().to_string_lossy().into_owned();
        let mut project_id = String::new();
        modal.act(|root, window, cx| {
            root.close_start_chat_modal(window, cx);
            let project = root
                .core(cx)
                .project_create("Terminal project".into(), cwd.clone())
                .unwrap();
            project_id = project.id.clone();
            let node = runner_daemon::repo::node::ensure_project_node(
                &root.app_store.read(cx).test_core.db.get().unwrap(),
                &project.id,
            )
            .unwrap();
            let mut layout = PaneLayout::single(None, &[]);
            layout.id = "01M3VD00000000000000000005".into();
            layout.parent_id = Some(node.id);
            root.core(cx)
                .node_tab_upsert(layout.upsert_input().unwrap())
                .unwrap();
            root.app_store.update(cx, |store, cx| {
                store.projects = vec![project];
                cx.notify();
            });
            root.reload_tabs(cx).unwrap();
            root.set_route(AppRoute::Mission("test-mission".into()), cx);
        });
        modal.act(|root, window, cx| root.root_focus.focus(window, cx));
        modal
            .visual
            .simulate_keystrokes(&keymap::platform_default("cmd-t"));
        modal.act(|root, _, cx| {
            assert_eq!(root.route, AppRoute::Chat);
            assert_eq!(root.tabs.tabs().len(), 2);
            let session_id = root.tabs.active().unwrap().focused_session_id().unwrap();
            let session = root.session_entry(session_id, cx).unwrap();
            assert_eq!(session.project_id.as_deref(), Some(project_id.as_str()));
            assert_eq!(session.cwd.as_deref(), Some(cwd.as_str()));
            root.core(cx).session_close(session_id).unwrap();
        });
    }

    #[test]
    fn confirm_does_nothing_when_disabled_starting_or_composing() {
        use gpui::EntityInputHandler;
        let mut modal = modal_harness(1200., 1000., Vec::new(), Vec::new(), ChatMode::Runtime);
        modal
            .visual
            .simulate_keystrokes(&keymap::platform_default("cmd-enter"));
        modal.read(|modal, _| assert!(modal.error.is_none()));
        drop(modal);
        let mut modal = modal_harness(1200., 1000., vec![codex_role()], agents(), ChatMode::Role);
        modal.edit(|modal, cx| {
            modal.submitting = true;
            set_start_chat_controls_disabled(modal, true, cx);
        });
        modal
            .visual
            .simulate_keystrokes(&keymap::platform_default("cmd-enter"));
        modal.read(|modal, _| {
            assert!(modal.submitting);
            assert!(modal.error.is_none());
        });
        modal.edit(|modal, cx| {
            modal.submitting = false;
            set_start_chat_controls_disabled(modal, false, cx);
        });
        modal.act(|root, window, cx| {
            let title = root.start_chat_modal.as_ref().unwrap().title.clone();
            title.read(cx).focus_handle().focus(window, cx);
            title.update(cx, |title, cx| {
                title.replace_and_mark_text_in_range(None, "ni", Some(2..2), window, cx)
            });
        });
        modal
            .visual
            .simulate_keystrokes(&keymap::platform_default("cmd-enter"));
        modal.read(|modal, cx| {
            assert!(modal.title.read(cx).is_composing());
            assert!(modal.error.is_none());
        });
    }

    #[test]
    fn mode_context_beats_global_tab_keys_and_arrows_keep_the_switch_focused() {
        let mut modal = modal_harness(
            1200.,
            1000.,
            vec![codex_role()],
            agents(),
            ChatMode::Runtime,
        );
        modal.act(|root, _, cx| {
            for id in ["01M3VD00000000000000000001", "01M3VD00000000000000000002"] {
                let mut layout = PaneLayout::single(None, &[]);
                layout.id = id.into();
                root.core(cx)
                    .node_tab_upsert(layout.upsert_input().unwrap())
                    .unwrap();
            }
            root.reload_tabs(cx).unwrap();
            root.tabs.activate("01M3VD00000000000000000001");
        });
        modal.act(|root, window, cx| {
            root.start_chat_modal
                .as_ref()
                .unwrap()
                .picker_focus(cx)
                .focus(window, cx)
        });
        modal
            .visual
            .simulate_keystrokes(&keymap::platform_default("cmd-2"));
        modal.act(|root, window, cx| {
            assert_eq!(
                root.tabs.active_tab_id(),
                Some("01M3VD00000000000000000001")
            );
            let form = root.start_chat_modal.as_ref().unwrap();
            assert_eq!(form.mode, ChatMode::Role);
            assert!(form.role_select.read(cx).focus_handle().is_focused(window));
        });
        modal
            .visual
            .simulate_keystrokes(&keymap::platform_default("cmd-1"));
        modal.act(|root, window, cx| {
            let form = root.start_chat_modal.as_ref().unwrap();
            assert_eq!(form.mode, ChatMode::Runtime);
            assert!(form
                .runtime_select
                .read(cx)
                .focus_handle()
                .is_focused(window));
        });
        let name_focus = modal.read(|form, cx| form.title.read(cx).focus_handle());
        modal.act(|_, window, cx| name_focus.focus(window, cx));
        modal
            .visual
            .simulate_keystrokes(&keymap::platform_default("cmd-2"));
        modal.act(|root, window, cx| {
            let form = root.start_chat_modal.as_ref().unwrap();
            assert_eq!(
                root.tabs.active_tab_id(),
                Some("01M3VD00000000000000000001")
            );
            assert_eq!(form.mode, ChatMode::Role);
            assert!(form.role_select.read(cx).focus_handle().is_focused(window));
        });
        modal.visual.simulate_keystrokes("enter");
        modal
            .visual
            .simulate_keystrokes(&keymap::platform_default("cmd-1"));
        modal.act(|root, window, cx| {
            assert_eq!(
                root.tabs.active_tab_id(),
                Some("01M3VD00000000000000000001")
            );
            root.start_chat_modal
                .as_ref()
                .unwrap()
                .direct_mode_focus
                .focus(window, cx);
        });
        modal.visual.simulate_keystrokes("right");
        modal.act(|root, window, _cx| {
            let form = root.start_chat_modal.as_ref().unwrap();
            assert_eq!(form.mode, ChatMode::Role);
            assert!(form.role_mode_focus.is_focused(window));
        });
        modal.visual.simulate_keystrokes("left space enter");
        modal.read(|form, _| assert_eq!(form.mode, ChatMode::Runtime));
        let bounds = modal.bounds("START_CHAT_ROLE_MODE").unwrap();
        modal
            .visual
            .simulate_click(bounds.center(), gpui::Modifiers::default());
        modal.act(|root, window, _cx| {
            assert!(root
                .start_chat_modal
                .as_ref()
                .unwrap()
                .direct_mode_focus
                .is_focused(window))
        });
        modal.visual.simulate_keystrokes("escape");
        modal.act(|root, window, cx| root.root_focus.focus(window, cx));
        modal
            .visual
            .simulate_keystrokes(&keymap::platform_default("cmd-2"));
        modal.act(|root, _, _| {
            assert_eq!(
                root.tabs.active_tab_id(),
                Some("01M3VD00000000000000000002")
            )
        });
        modal
            .visual
            .simulate_keystrokes(&keymap::platform_default("cmd-1"));
        modal.act(|root, _, _| {
            assert_eq!(
                root.tabs.active_tab_id(),
                Some("01M3VD00000000000000000001")
            )
        });
    }

    #[test]
    fn new_tab_from_outside_focus_closes_with_escape_and_name_enter_submits() {
        let mut modal = modal_harness(1200., 1000., Vec::new(), Vec::new(), ChatMode::Runtime);
        modal.visual.simulate_keystrokes("escape");
        modal.act(|root, window, cx| {
            assert!(root.start_chat_modal.is_none());
            root.root_focus.focus(window, cx);
        });
        modal
            .visual
            .simulate_keystrokes(&keymap::platform_default("cmd-n"));
        modal.act(|root, window, cx| {
            let form = root.start_chat_modal.as_ref().unwrap();
            assert!(form.picker_focus(cx).is_focused(window));
        });
        modal.visual.simulate_keystrokes("escape");
        modal.act(|root, window, cx| {
            assert!(root.start_chat_modal.is_none());
            root.root_focus.focus(window, cx);
        });
        modal
            .visual
            .simulate_keystrokes(&keymap::platform_default("cmd-n"));
        modal.edit(|form, cx| seed_modal(form, vec![codex_role()], agents(), ChatMode::Role, cx));
        let name = modal.read(|form, cx| form.title.read(cx).focus_handle());
        modal.act(|_, window, cx| name.focus(window, cx));
        modal.visual.simulate_keystrokes("enter");
        modal.read(|form, _| assert!(form.error.is_some()));
    }

    #[test]
    #[cfg(unix)]
    fn new_tab_from_a_focused_terminal_pane_handles_escape_and_name_enter() {
        let mut modal = modal_harness(1200., 1000., Vec::new(), Vec::new(), ChatMode::Runtime);
        let cwd = modal._temp.path().to_string_lossy().into_owned();
        let mut session_id = String::new();
        modal.act(|root, window, cx| {
            root.close_start_chat_modal(window, cx);
            root.app_store.update(cx, |store, cx| {
                store.settings.default_working_dir = cwd;
                cx.notify();
            });
            root.new_terminal_tab(ProjectScope::Root, window, cx);
        });
        modal.act(|root, _, _| {
            session_id = root
                .tabs
                .active()
                .unwrap()
                .focused_session_id()
                .unwrap()
                .to_owned();
        });
        modal.act(|root, window, cx| {
            root.chat_transitions.clear();
            root.focus_active_terminal(window, cx);
            assert!(root
                .attached
                .get(&session_id)
                .unwrap()
                .terminal_focus
                .is_focused(window));
        });
        modal
            .visual
            .simulate_keystrokes(&keymap::platform_default("cmd-n"));
        modal.act(|root, window, cx| {
            assert!(root
                .start_chat_modal
                .as_ref()
                .unwrap()
                .picker_focus(cx)
                .is_focused(window));
        });
        modal.visual.simulate_keystrokes("escape");
        modal.act(|root, window, _cx| {
            assert!(root.start_chat_modal.is_none());
            assert!(root
                .attached
                .get(&session_id)
                .unwrap()
                .terminal_focus
                .is_focused(window));
        });
        modal
            .visual
            .simulate_keystrokes(&keymap::platform_default("cmd-n"));
        modal.edit(|form, cx| seed_modal(form, vec![codex_role()], agents(), ChatMode::Role, cx));
        let name = modal.read(|form, cx| form.title.read(cx).focus_handle());
        modal.act(|_, window, cx| name.focus(window, cx));
        modal.visual.simulate_keystrokes("enter");
        modal.read(|form, _| assert!(form.error.is_some()));
        modal.act(|root, _, cx| {
            root.core(cx).session_close(&session_id).unwrap();
        });
    }

    #[test]
    fn mouse_mode_change_from_the_initial_picker_keeps_modal_keys_live() {
        for mode in [ChatMode::Runtime, ChatMode::Role] {
            for confirm in [false, true] {
                let mut modal = modal_harness(1200., 1000., vec![codex_role()], agents(), mode);
                let missing = modal
                    ._temp
                    .path()
                    .join("missing")
                    .to_string_lossy()
                    .into_owned();
                modal.edit(|form, cx| form.cwd.update(cx, |field, cx| field.set_text(missing, cx)));
                let selector = match mode {
                    ChatMode::Runtime => "START_CHAT_ROLE_MODE",
                    ChatMode::Role => "START_CHAT_DIRECT_MODE",
                };
                let bounds = modal.bounds(selector).unwrap();
                modal
                    .visual
                    .simulate_click(bounds.center(), gpui::Modifiers::default());
                modal.act(|root, window, cx| {
                    let form = root.start_chat_modal.as_ref().unwrap();
                    assert_ne!(form.mode, mode);
                    assert!(form.picker_focus(cx).is_focused(window));
                });
                if confirm {
                    modal.visual.simulate_keystrokes("tab");
                    modal
                        .visual
                        .simulate_keystrokes(&keymap::platform_default("cmd-enter"));
                    modal.read(|form, _| assert!(form.error.is_some()));
                } else {
                    modal.visual.simulate_keystrokes("escape");
                    modal.act(|root, _, _| assert!(root.start_chat_modal.is_none()));
                }
            }
        }
    }

    #[test]
    fn mouse_mode_change_preserves_focus_in_a_persistent_text_field() {
        let mut modal = modal_harness(
            1200.,
            1000.,
            vec![codex_role()],
            agents(),
            ChatMode::Runtime,
        );
        let focus = modal.read(|form, cx| form.title.read(cx).focus_handle());
        modal.act(|_, window, cx| focus.focus(window, cx));
        let bounds = modal.bounds("START_CHAT_ROLE_MODE").unwrap();
        modal
            .visual
            .simulate_click(bounds.center(), gpui::Modifiers::default());
        modal.act(|root, window, _| {
            assert_eq!(root.start_chat_modal.as_ref().unwrap().mode, ChatMode::Role);
            assert!(focus.is_focused(window));
        });
    }

    #[test]
    fn user_confirm_collision_cannot_shadow_chat_confirm_from_a_picker_or_text_field() {
        for text_field in [false, true] {
            let mut modal =
                modal_harness(1200., 1000., vec![codex_role()], agents(), ChatMode::Role);
            let key = gpui::Keystroke::parse(&keymap::platform_default("cmd-enter")).unwrap();
            let overrides = keymap::KeymapOverrides::from([(
                "new-terminal".into(),
                keymap::combo_from_keystroke(&key),
            )]);
            modal.act(|root, window, cx| {
                keymap::install_bindings(cx, &overrides, false);
                if text_field {
                    root.start_chat_modal
                        .as_ref()
                        .unwrap()
                        .title
                        .read(cx)
                        .focus_handle()
                        .focus(window, cx);
                }
            });
            modal
                .visual
                .simulate_keystrokes(&keymap::platform_default("cmd-enter"));
            modal.read(|form, _| assert!(form.error.is_some()));
        }
    }

    pub(in crate::surfaces) fn project_fixture(
        harness: &mut ModalHarness,
    ) -> Vec<runner_core::protocol::project::ProjectRow> {
        let base = harness._temp.path().join("projects");
        let mut projects = Vec::new();
        harness.act(|root, _, cx| {
            for name in ["alpha", "beta"] {
                let path = base.join(name);
                fs::create_dir_all(path.join(".worktrees/feature")).unwrap();
                projects.push(
                    root.core(cx)
                        .project_create(name.into(), path.to_string_lossy().into_owned())
                        .unwrap(),
                );
            }
            root.app_store.update(cx, |store, cx| {
                store.projects = projects.clone();
                cx.notify();
            });
        });
        projects
    }

    #[test]
    fn project_choice_defaults_to_the_entry_point_and_locks_a_pane() {
        let mut harness = modal_harness(1200., 1000., Vec::new(), agents(), ChatMode::Runtime);
        let projects = project_fixture(&mut harness);
        harness.act(|root, window, cx| {
            root.close_start_chat_modal(window, cx);
            root.front_page_sidebar().update(cx, |sidebar, cx| {
                sidebar.test_set_active_project(Some(projects[0].id.clone()), cx)
            });
            root.open_new_tab_modal(&NewTab, window, cx);
            let modal = root.start_chat_modal.as_ref().unwrap();
            assert_eq!(
                modal.project.scope(),
                ProjectScope::Project(projects[0].id.clone())
            );
            assert_eq!(modal.cwd.read(cx).text(), projects[0].cwd);
            root.close_start_chat_modal(window, cx);
            root.open_sidebar_chat_modal(ProjectScope::Project(projects[1].id.clone()), window, cx);
            assert_eq!(
                root.start_chat_modal.as_ref().unwrap().project.scope(),
                ProjectScope::Project(projects[1].id.clone())
            );
            root.close_start_chat_modal(window, cx);
            root.front_page_sidebar()
                .update(cx, |sidebar, cx| sidebar.test_set_active_project(None, cx));
            root.open_new_tab_modal(&NewTab, window, cx);
            let modal = root.start_chat_modal.as_ref().unwrap();
            assert_eq!(modal.project.scope(), ProjectScope::Root);
            assert_eq!(modal.cwd.read(cx).text(), "");
            root.close_start_chat_modal(window, cx);
            let node = runner_daemon::repo::node::ensure_project_node(
                &root.app_store.read(cx).test_core.db.get().unwrap(),
                &projects[0].id,
            )
            .unwrap();
            let mut layout = PaneLayout::single(None, &[]);
            layout.id = "01M3VD00000000000000000006".into();
            layout.parent_id = Some(node.id);
            let pane = layout.focused_pane_id.clone();
            root.core(cx)
                .node_tab_upsert(layout.upsert_input().unwrap())
                .unwrap();
            root.reload_tabs(cx).unwrap();
            root.open_pane_chat_modal(&pane, window, cx);
            let modal = root.start_chat_modal.as_ref().unwrap();
            assert!(modal.project.locked());
            assert_eq!(
                modal.project.scope(),
                ProjectScope::Project(projects[0].id.clone())
            );
            assert!(modal.project.select().read(cx).is_disabled());
            assert!(!start_chat_focus_order(modal, cx).contains(&modal.project.focus_handle(cx)));
            modal
                .cwd
                .update(cx, |input, cx| input.set_text(projects[1].cwd.clone(), cx));
            root.follow_start_chat_cwd(cx);
            let modal = root.start_chat_modal.as_ref().unwrap();
            assert_eq!(
                modal.project.scope(),
                ProjectScope::Project(projects[0].id.clone())
            );
            assert_eq!(
                modal.project.directory_hint(modal.cwd.read(cx).text()),
                Some(super::super::project_field::DirectoryHint::Outside(
                    "alpha".into()
                ))
            );
        });
    }

    #[test]
    fn project_follows_typed_directory_when_tab_moves_to_browse_on_the_app_shell() {
        let mut harness = modal_harness(1200., 1000., Vec::new(), agents(), ChatMode::Runtime);
        let projects = project_fixture(&mut harness);
        harness.act(|root, window, cx| {
            root.close_start_chat_modal(window, cx);
            root.set_route(AppRoute::Settings, cx);
            root.open_sidebar_chat_modal(ProjectScope::Project(projects[1].id.clone()), window, cx);
            let modal = root.start_chat_modal.as_ref().unwrap();
            modal.cwd.read(cx).focus_handle().focus(window, cx);
            window.activate_window();
        });
        harness
            .visual
            .simulate_keystrokes(&keymap::platform_default("cmd-a"));
        harness.visual.simulate_input(&projects[0].cwd);
        harness.visual.run_until_parked();
        harness.visual.simulate_keystrokes("tab");
        harness.visual.run_until_parked();
        assert!(
            harness
                .visual
                .debug_bounds("FIELD_SUBTITLE Inside alpha, so it is filed there.")
                .is_some(),
            "the followed project's hint must repaint without another input event"
        );
        harness.act(|root, window, cx| {
            let modal = root.start_chat_modal.as_ref().unwrap();
            assert!(modal.browse_focus.is_focused(window));
            assert_eq!(modal.cwd.read(cx).text(), projects[0].cwd);
            assert_eq!(
                modal.project.scope(),
                ProjectScope::Project(projects[0].id.clone())
            );
        });
        harness.choose(StartChatSelection::Project, "");
        harness.act(|root, window, cx| {
            root.start_chat_modal
                .as_ref()
                .unwrap()
                .cwd
                .read(cx)
                .focus_handle()
                .focus(window, cx)
        });
        harness.visual.simulate_keystrokes("tab");
        harness.visual.run_until_parked();
        harness.read(|modal, _| assert_eq!(modal.project.scope(), ProjectScope::Root));
    }

    #[test]
    fn project_choice_rewrites_or_keeps_the_directory_and_follows_blur_and_browse() {
        let mut harness = modal_harness(1200., 1000., Vec::new(), agents(), ChatMode::Runtime);
        let projects = project_fixture(&mut harness);
        harness.act(|root, window, cx| {
            root.close_start_chat_modal(window, cx);
            root.open_sidebar_chat_modal(ProjectScope::Project(projects[0].id.clone()), window, cx);
        });
        harness.choose(StartChatSelection::Project, &projects[1].id);
        harness.read(|modal, cx| assert_eq!(modal.cwd.read(cx).text(), projects[1].cwd));
        let worktree = format!("{}/.worktrees/feature", projects[1].cwd);
        harness.edit(|modal, cx| {
            modal
                .cwd
                .update(cx, |input, cx| input.set_text(worktree.clone(), cx))
        });
        harness.choose(StartChatSelection::Project, &projects[1].id);
        harness.read(|modal, cx| assert_eq!(modal.cwd.read(cx).text(), worktree));
        harness.choose(StartChatSelection::Project, "");
        harness.act(|root, _, cx| root.follow_start_chat_cwd(cx));
        harness.read(|modal, cx| {
            assert_eq!(modal.project.scope(), ProjectScope::Root);
            assert_eq!(modal.cwd.read(cx).text(), worktree);
        });
        harness.act(|root, window, cx| {
            let modal = root.start_chat_modal.as_ref().unwrap();
            modal
                .cwd
                .update(cx, |input, cx| input.set_text(projects[0].cwd.clone(), cx));
            modal.cwd.read(cx).focus_handle().focus(window, cx);
            window.activate_window();
        });
        harness.visual.simulate_keystrokes("tab");
        harness.visual.run_until_parked();
        harness.read(|modal, _| {
            assert_eq!(
                modal.project.scope(),
                ProjectScope::Project(projects[0].id.clone())
            )
        });
        harness.act(|root, _, cx| root.browse_start_chat_cwd(cx));
        let browsed = PathBuf::from(&projects[1].cwd);
        harness
            .visual
            .simulate_path_prompt_response(move |_| Some(vec![browsed]));
        harness.visual.run_until_parked();
        harness.read(|modal, cx| {
            assert_eq!(
                modal.project.scope(),
                ProjectScope::Project(projects[1].id.clone())
            );
            assert_eq!(modal.cwd.read(cx).text(), projects[1].cwd);
        });
        harness.choose(StartChatSelection::Project, "");
        harness.act(|root, _, cx| root.browse_start_chat_cwd(cx));
        let same = PathBuf::from(&projects[1].cwd);
        harness
            .visual
            .simulate_path_prompt_response(move |_| Some(vec![same]));
        harness.visual.run_until_parked();
        harness.read(|modal, _| {
            assert_eq!(
                modal.project.scope(),
                ProjectScope::Project(projects[1].id.clone())
            )
        });
        harness.act(|root, _, cx| {
            let modal = root.start_chat_modal.as_ref().unwrap();
            modal
                .cwd
                .update(cx, |input, cx| input.set_text("/outside-projects", cx));
            root.follow_start_chat_cwd(cx);
        });
        harness.read(|modal, cx| {
            assert_eq!(
                modal.project.scope(),
                ProjectScope::Project(projects[1].id.clone())
            );
            assert_eq!(
                modal.project.directory_hint(modal.cwd.read(cx).text()),
                Some(super::super::project_field::DirectoryHint::Outside(
                    "beta".into()
                ))
            );
        });
    }

    #[test]
    fn create_keys_are_inert_in_start_chat() {
        let mut modal = modal_harness(1200., 1000., vec![codex_role()], agents(), ChatMode::Role);
        let title = modal.read(|form, _| form.title.clone());
        for key in ["cmd-n", "cmd-t", "shift-cmd-m"] {
            modal
                .visual
                .simulate_keystrokes(&keymap::platform_default(key));
            modal.read(|form, _| assert_eq!(form.title, title));
            modal.act(|root, _, _| {
                assert!(root.start_mission_modal.is_none());
                assert!(root.tabs.tabs().is_empty());
            });
        }
    }

    #[test]
    fn tab_order_runs_from_the_picker_through_the_controls_and_resets() {
        let mut modal = modal_harness(1200., 1000., vec![codex_role()], agents(), ChatMode::Role);
        let order = |modal: &ModalHarness| {
            modal.read(|modal, cx| {
                let handles = start_chat_focus_order(modal, cx);
                let resets = |kind: ResetKind| modal.reset_focus[kind as usize].clone();
                let names = [
                    ("close", modal.close_focus.clone()),
                    ("direct", modal.direct_mode_focus.clone()),
                    ("role", modal.role_mode_focus.clone()),
                    ("role picker", modal.role_select.read(cx).focus_handle()),
                    ("runtime", modal.role_runtime_select.read(cx).focus_handle()),
                    ("model", modal.model.read(cx).focus_handle()),
                    ("effort", modal.effort_select.read(cx).focus_handle()),
                    ("speed", modal.speed_select.read(cx).focus_handle()),
                    ("reset runtime", resets(ResetKind::Runtime)),
                    ("reset model", resets(ResetKind::Model)),
                    ("reset effort", resets(ResetKind::Effort)),
                    ("reset speed", resets(ResetKind::Speed)),
                    ("agent picker", modal.runtime_select.read(cx).focus_handle()),
                    ("name", modal.title.read(cx).focus_handle()),
                    ("project", modal.project.focus_handle(cx)),
                    ("directory", modal.cwd.read(cx).focus_handle()),
                    ("browse", modal.browse_focus.clone()),
                    ("cancel", modal.cancel_focus.clone()),
                    ("start", modal.submit_focus.clone()),
                ];
                handles
                    .iter()
                    .map(|handle| {
                        names
                            .iter()
                            .find(|(_, named)| named == handle)
                            .map_or("?", |(name, _)| *name)
                    })
                    .collect::<Vec<_>>()
            })
        };
        let head = ["close", "role", "role picker"];
        let tail = ["name", "project", "directory", "browse", "cancel", "start"];
        assert_eq!(
            order(&modal),
            [
                &head[..],
                &["runtime", "model", "effort", "speed"],
                &tail[..]
            ]
            .concat()
        );

        // A Reset joins the order after every control, in control order.
        modal.type_model("gpt-5.6-sol");
        modal.choose(StartChatSelection::Speed, "standard");
        assert_eq!(
            order(&modal),
            [
                &head[..],
                &[
                    "runtime",
                    "model",
                    "effort",
                    "speed",
                    "reset model",
                    "reset speed"
                ],
                &tail[..]
            ]
            .concat()
        );

        modal.act(|root, _, cx| root.set_start_chat_mode(ChatMode::Runtime, cx));
        assert_eq!(
            order(&modal),
            [
                &["close", "direct", "agent picker", "model", "effort"][..],
                &tail[..]
            ]
            .concat()
        );
    }

    #[test]
    fn tab_traversal_reaches_only_the_active_mode_segment() {
        let mut modal = modal_harness(1200., 1000., vec![codex_role()], agents(), ChatMode::Role);
        let order = modal.read(start_chat_focus_order);
        let close = order[0].clone();
        modal.act(|_, window, cx| close.focus(window, cx));
        for focus in order.iter().skip(1).chain(order.iter().take(1)) {
            modal.visual.simulate_keystrokes("tab");
            modal.act(|_, window, _| assert!(focus.is_focused(window)));
        }
        modal.visual.simulate_keystrokes("shift-tab");
        modal.act(|_, window, _| assert!(order.last().unwrap().is_focused(window)));
    }

    #[test]
    fn empty_states_draw_the_picker_alone() {
        // No roles yet.
        let mut modal = modal_harness(1200., 1000., Vec::new(), agents(), ChatMode::Role);
        assert!(modal.bounds("START_CHAT_CARD").is_some());
        assert!(modal.bounds("START_CHAT_SETUP").is_none());
        modal.read(|modal, _| assert!(!modal.can_submit()));
        drop(modal);

        // No enabled agents.
        let mut modal = modal_harness(1200., 1000., Vec::new(), Vec::new(), ChatMode::Runtime);
        assert!(modal.bounds("START_CHAT_CARD").is_some());
        assert!(modal.bounds("START_CHAT_SETUP").is_none());
        modal.read(|modal, cx| {
            assert!(!modal.can_submit());
            assert!(start_chat_focus_order(modal, cx)
                .iter()
                .all(|handle| *handle != modal.runtime_select.read(cx).focus_handle()));
        });
    }

    #[test]
    fn resets_are_inert_while_starting() {
        let mut modal = modal_harness(1200., 1000., vec![claude_role()], agents(), ChatMode::Role);
        modal.type_model("sonnet");
        modal.edit(|modal, cx| {
            modal.submitting = true;
            set_start_chat_controls_disabled(modal, true, cx);
        });
        modal.act(|root, window, cx| root.reset_start_chat_control(ResetKind::Model, window, cx));
        modal.read(|modal, cx| assert_eq!(modal.model.read(cx).text(), "sonnet"));
    }

    #[test]
    fn the_working_directory_hint_names_where_a_blank_field_starts() {
        let mut with_dir = test_role("dir", "claude-code");
        with_dir.working_dir = Some("/repo".into());
        let without = test_role("none", "claude-code");
        assert_eq!(
            working_dir_hint(ChatMode::Role, Some(&with_dir.clone().into())),
            "Blank starts in the role's directory."
        );
        assert_eq!(
            working_dir_hint(ChatMode::Role, Some(&without.clone().into())),
            "Blank starts in your default directory."
        );
        assert_eq!(
            working_dir_hint(ChatMode::Runtime, Some(&with_dir.clone().into())),
            "Blank starts in your default directory."
        );
    }

    #[test]
    fn the_agent_note_says_which_role_values_do_not_carry_over() {
        assert_eq!(
            agent_note("Codex", "Claude Code", Some("opus[1m]"), Some("xhigh")),
            "Codex starts from its own model and effort. The role's opus[1m] and xhigh belong to Claude Code and don't carry over."
        );
        assert_eq!(
            agent_note("Codex", "Claude Code", None, Some("xhigh")),
            "Codex starts from its own model and effort. The role's xhigh belongs to Claude Code and doesn't carry over."
        );
        assert_eq!(
            agent_note("Codex", "Claude Code", None, None),
            "Codex starts from its own model and effort."
        );
    }

    #[cfg(debug_assertions)]
    #[test]
    fn chinese_inheritance_and_runtime_notes_translate_effort_only() {
        crate::i18n::testing::with_locale(crate::i18n::Locale::SimplifiedChinese, || {
            let mut role = claude_role();
            role.model = Some("xhigh".into());
            let mut modal = modal_harness(1200., 1000., vec![role], agents(), ChatMode::Role);
            modal.type_model("sonnet");
            modal.choose(StartChatSelection::Effort, "low");
            assert!(modal
                .bounds("START_CHAT_NOTE 思考强度 角色：极高")
                .is_some());
            assert!(modal.bounds("START_CHAT_NOTE 模型 角色：xhigh").is_some());
            assert_eq!(agent_note("Codex", "Claude Code", Some("xhigh"), Some("xhigh")), "Codex 从其自身的模型和思考强度启动。 角色的 xhigh 和 极高 属于 Claude Code，不会继承。");
            assert_eq!(
                agent_note("Codex", "Claude Code", None, Some("medium")),
                "Codex 从其自身的模型和思考强度启动。 角色的 中 属于 Claude Code，不会继承。"
            );
            assert_eq!(
                agent_note("Codex", "Claude Code", Some("high"), None),
                "Codex 从其自身的模型和思考强度启动。 角色的 high 属于 Claude Code，不会继承。"
            );
        });
    }

    #[test]
    fn card_columns_fit_the_width_the_modal_leaves_the_form() {
        let layout = CardLayout::new(510.);
        assert_eq!(layout.content, 484.);
        assert!(3. * layout.third() + 2. * COLUMN_GAP <= layout.content);
        assert_eq!(layout.model(false, false), layout.content);
        assert_eq!(
            layout.model(true, true) + EFFORT_COLUMN_WIDTH + SPEED_COLUMN_WIDTH + 2. * COLUMN_GAP,
            layout.content
        );
    }

    /// The modal keeps its width in the smallest window, scrolls, and lets
    /// nothing run past the card or the panel.
    #[test]
    fn the_modal_fits_a_640_by_480_window_without_clipping_sideways() {
        let long = Role {
            display_name: "A role whose display name is far too long to fit a picker".repeat(3),
            handle: "a-handle-that-goes-on-and-on-and-on-and-on-and-on-and-on".into(),
            model: Some("a-model-name-that-is-longer-than-its-column-can-hold".into()),
            ..codex_role()
        };
        let mut modal = modal_harness(640., 480., vec![long], agents(), ChatMode::Role);
        let fits = |modal: &mut ModalHarness, fields: &[&str]| {
            let panel = modal.bounds("MODAL_PANEL").unwrap();
            assert_eq!(panel.size.width, px(560.));
            assert!(
                panel.left() >= px(0.) && panel.right() <= px(640.),
                "{panel:?}"
            );
            assert!(panel.size.height <= px(480. * 0.85 + 1.), "{panel:?}");
            let card = modal.bounds("START_CHAT_CARD").unwrap();
            assert!(
                card.left() >= panel.left() + px(24.),
                "{card:?} in {panel:?}"
            );
            assert!(
                card.right() <= panel.right() - px(24.),
                "{card:?} in {panel:?}"
            );
            for field in fields {
                let bounds = modal.bounds(&format!("START_CHAT_FIELD {field}")).unwrap();
                assert!(
                    bounds.left() >= card.left() && bounds.right() <= card.right(),
                    "{field}: {bounds:?} in {card:?}"
                );
            }
        };
        fits(&mut modal, &["Runtime", "Model", "Effort", "Speed"]);
        let picker = modal.bounds("STYLED_SELECT_start-chat-role").unwrap();
        let card = modal.bounds("START_CHAT_CARD").unwrap();
        assert!(
            picker.left() >= card.left() && picker.right() <= card.right(),
            "a long name truncates inside the picker: {picker:?} in {card:?}"
        );
        assert!(
            modal.read(|modal, _| modal.scroll_handle.max_offset().y) > px(0.),
            "the tall form scrolls"
        );

        // Overridden: notes and Resets sit inside their columns too.
        modal.type_model("a-model-name-that-is-far-too-long-for-its-narrow-column-to-hold");
        modal.choose(StartChatSelection::Speed, "standard");
        fits(&mut modal, &["Runtime", "Model", "Effort", "Speed"]);
        for reset in [ResetKind::Model, ResetKind::Speed] {
            let reset = modal
                .bounds(&format!("START_CHAT_RESET {reset:?}"))
                .unwrap();
            let card = modal.bounds("START_CHAT_CARD").unwrap();
            assert!(reset.right() <= card.right(), "{reset:?} in {card:?}");
        }

        modal.act(|root, _, cx| root.set_start_chat_mode(ChatMode::Runtime, cx));
        modal.choose(StartChatSelection::Runtime, "codex");
        fits(&mut modal, &["Model", "Effort", "Speed"]);
    }

    #[test]
    fn new_terminal_targets_an_available_mission_drawer_and_otherwise_tab_panes() {
        let mission = AppRoute::Mission("mission".into());
        assert_eq!(
            new_terminal_target(&mission, true),
            NewTerminalTarget::MissionDrawer
        );
        for route in [
            AppRoute::Chat,
            AppRoute::Roles,
            AppRoute::RoleDetail("role".into()),
            AppRoute::Crews,
            AppRoute::CrewEditor("crew".into()),
            AppRoute::Settings,
            AppRoute::ArchivedChat,
            mission,
        ] {
            assert_eq!(
                new_terminal_target(&route, false),
                NewTerminalTarget::Tab,
                "{route:?}"
            );
        }
    }

    #[test]
    fn new_terminal_prefers_the_focused_empty_pane_then_any_empty_pane() {
        let mut layout = PaneLayout::single(Some("shell"), &[]);
        assert_eq!(new_terminal_empty_pane(&layout), None);

        let first = layout.split("p1", SplitOrientation::Row).unwrap();
        let second = layout.split(&first, SplitOrientation::Column).unwrap();
        assert_eq!(new_terminal_empty_pane(&layout), Some(second.clone()));

        layout.focus_session("shell");
        assert_eq!(new_terminal_empty_pane(&layout), Some(first.clone()));

        layout.assign_session(&first, "shell-2").unwrap();
        assert_eq!(new_terminal_empty_pane(&layout), Some(second.clone()));

        layout.assign_session(&second, "shell-3").unwrap();
        assert_eq!(new_terminal_empty_pane(&layout), None);
    }

    fn runtime(name: &str, efforts: &[&str]) -> RuntimeCatalogEntry {
        RuntimeCatalogEntry {
            name: Runtime::parse(name).unwrap(),
            capabilities: runner_core::protocol::runtime::RuntimeCatalogEntry::for_runtime(
                Runtime::parse(name).unwrap(),
            )
            .map(|entry| entry.capabilities)
            .unwrap_or_default(),
            display_name: name.into(),
            command: name.into(),
            native_fork: matches!(name, "codex" | "claude-code" | "pi"),
            description: name.into(),
            install_url: String::new(),
            default_enabled: true,
            available: true,
            default_model: None,
            default_effort: None,
            models: Vec::new(),
            efforts: efforts
                .iter()
                .map(|value| RuntimeCatalogOption {
                    value: (*value).into(),
                    label: (*value).into(),
                    description: None,
                    supported_efforts: None,
                })
                .collect(),
        }
    }

    #[test]
    fn new_chat_only_saves_names_the_user_edited() {
        let mut cx = gpui::TestAppContext::single();
        let title = cx.new(|cx| TextField::new(cx.focus_handle(), "", "Codex", false));
        title.update(&mut cx, |input, cx| {
            assert_eq!(input.text(), "");
            assert!(!input.edited());
            assert_eq!(user_chat_title(input), None);
            input.set_placeholder("@coder", cx);
            assert_eq!(user_chat_title(input), None);
            input.set_text("  Cars  ", cx);
            assert_eq!(user_chat_title(input).as_deref(), Some("Cars"));
            input.set_text("Codex", cx);
            assert_eq!(user_chat_title(input).as_deref(), Some("Codex"));
            input.set_text("", cx);
            assert_eq!(user_chat_title(input), None);
            input.set_text("   ", cx);
            assert_eq!(user_chat_title(input), None);
        });
    }

    #[test]
    fn title_selection_changes_preserve_blank_and_user_edited_text() {
        let mut cx = gpui::TestAppContext::single();
        let title = cx.new(|cx| {
            TextField::new(
                cx.focus_handle(),
                "",
                default_title_for_role("coder"),
                false,
            )
        });
        cx.update(|cx| {
            update_auto_title(&title, default_title_for_role("reviewer"), cx);
            assert_eq!(title.read(cx).text(), "");
            assert!(!title.read(cx).edited());
            assert_eq!(user_chat_title(title.read(cx)), None);

            title.update(cx, |input, cx| input.set_text("my chat", cx));
            update_auto_title(&title, default_title_for_runtime("Codex"), cx);
            assert_eq!(user_chat_title(title.read(cx)).as_deref(), Some("my chat"));

            title.update(cx, |input, cx| input.set_text("", cx));
            update_auto_title(&title, default_title_for_role("coder"), cx);
            assert_eq!(title.read(cx).text(), "");
            assert_eq!(user_chat_title(title.read(cx)), None);
            update_auto_title(&title, default_title_for_runtime("Claude Code"), cx);
            assert_eq!(title.read(cx).text(), "");
            assert_eq!(user_chat_title(title.read(cx)), None);
        });
    }

    #[test]
    fn effort_options_follow_the_selected_runtime_catalog() {
        let runtimes = [runtime("codex", &["", "low", "max"]), runtime("trae", &[])];
        assert_eq!(
            effort_options_for_runtime(&runtimes, "codex")
                .iter()
                .map(|option| option.value.as_str())
                .collect::<Vec<_>>(),
            ["", "low", "max"]
        );
        assert!(effort_options_for_runtime(&runtimes, "trae").is_empty());
        assert!(effort_options_for_runtime(&runtimes, "missing").is_empty());
    }

    #[test]
    fn direct_lists_recently_started_agents_first_then_the_catalog_order() {
        let runtimes = ["claude-code", "codex", "pi", "trae"]
            .map(|name| RuntimeChoice::from(runtime(name, &[])));
        let recent = ["pi", "gone", "codex"].map(String::from);
        assert_eq!(
            runtime_options(direct_runtime_order(&runtimes, &recent))
                .iter()
                .map(|option| option.value.as_str())
                .collect::<Vec<_>>(),
            ["pi", "codex", "claude-code", "trae"]
        );
        assert_eq!(
            direct_runtime_order(&runtimes, &[])
                .iter()
                .map(|runtime| runtime.name.key())
                .collect::<Vec<_>>(),
            ["claude-code", "codex", "pi", "trae"]
        );
    }

    #[test]
    fn direct_opens_on_the_most_recent_agent_then_the_settings_default() {
        let runtimes = ["claude-code", "codex", "pi", "trae"]
            .map(|name| RuntimeChoice::from(runtime(name, &[])));
        let recent = ["gone", "pi", "codex"].map(String::from);
        assert_eq!(
            initial_direct_runtime(&runtimes, &recent, "trae").as_deref(),
            Some("pi")
        );
        assert_eq!(
            initial_direct_runtime(&runtimes, &["gone".into()], "trae").as_deref(),
            Some("trae")
        );
        assert_eq!(
            initial_direct_runtime(&runtimes, &[], "trae").as_deref(),
            Some("trae")
        );
        assert_eq!(
            initial_direct_runtime(&runtimes, &[], "copilot").as_deref(),
            Some("claude-code")
        );
        assert_eq!(
            initial_direct_runtime(&runtimes, &[], "").as_deref(),
            Some("claude-code")
        );
        assert_eq!(initial_direct_runtime(&[], &recent, "trae"), None);
    }

    #[test]
    fn recording_a_direct_start_moves_its_agent_to_the_front() {
        let temp = tempfile::tempdir().unwrap();
        assert!(read_recent_runtimes(temp.path()).is_empty());

        record_recent_runtime(temp.path(), "codex").unwrap();
        record_recent_runtime(temp.path(), "claude-code").unwrap();
        record_recent_runtime(temp.path(), "pi").unwrap();
        record_recent_runtime(temp.path(), "codex").unwrap();
        assert_eq!(
            read_recent_runtimes(temp.path()),
            ["codex", "pi", "claude-code"]
        );
        assert_eq!(
            fs::read_to_string(recent_runtimes_path(temp.path())).unwrap(),
            "codex\npi\nclaude-code"
        );

        fs::write(recent_runtimes_path(temp.path()), " trae \n\ncodex\n").unwrap();
        assert_eq!(read_recent_runtimes(temp.path()), ["trae", "codex"]);
    }

    #[test]
    fn a_shell_started_from_a_session_prefers_its_live_cwd_then_its_spawn_cwd() {
        let live = Some(PathBuf::from("/repo/crates/app"));
        assert_eq!(
            shell_start_cwd(live.clone(), Some("/repo")).as_deref(),
            Some("/repo/crates/app")
        );
        assert_eq!(
            shell_start_cwd(live, None).as_deref(),
            Some("/repo/crates/app")
        );
        assert_eq!(
            shell_start_cwd(None, Some("/repo")).as_deref(),
            Some("/repo")
        );
        assert_eq!(shell_start_cwd(None, None), None);
        assert_eq!(
            terminal_working_dir(
                shell_start_cwd(None, None).as_deref(),
                Some("/project"),
                "/settings",
                Some("/home")
            )
            .as_deref(),
            Some("/project")
        );
    }

    #[test]
    fn terminal_cwd_prefers_the_focused_sibling_then_project_settings_and_home() {
        assert_eq!(
            terminal_working_dir(
                Some("/sibling"),
                Some("/project"),
                "/settings",
                Some("/home")
            )
            .as_deref(),
            Some("/sibling")
        );
        assert_eq!(
            terminal_working_dir(None, Some("/project"), "/settings", Some("/home")).as_deref(),
            Some("/project")
        );
        assert_eq!(
            terminal_working_dir(None, None, "/settings", Some("/home")).as_deref(),
            Some("/settings")
        );
        assert_eq!(
            terminal_working_dir(None, None, " ", Some("/home")).as_deref(),
            Some("/home")
        );
    }
}
