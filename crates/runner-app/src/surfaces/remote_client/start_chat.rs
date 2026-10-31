use super::*;

pub(crate) struct ChatLaunchAction {
    client: crate::surfaces::front_page::FrontPageClient,
    epoch: u64,
    before: HashMap<String, PaneLayout>,
    navigation: u64,
    key: Option<String>,
}
impl RemoteClient {
    pub(crate) fn prepare_chat_launch(
        &mut self,
        client: crate::surfaces::front_page::FrontPageClient,
        placement: Option<&(PaneLayout, String)>,
    ) -> Option<ChatLaunchAction> {
        if placement.is_some_and(|(layout, _)| self.layout_pending(&layout.id)) {
            return None;
        }
        let remote = self.workspace.as_mut()?;
        let key = placement.map(|(layout, _)| format!("layout:{}", layout.id));
        if let Some(key) = &key {
            if !remote.actions_pending.insert(key.clone()) {
                return None;
            }
        }
        Some(ChatLaunchAction {
            client,
            epoch: self.ui.epoch,
            before: remote.layouts.clone(),
            navigation: remote.navigation_generation,
            key,
        })
    }
    pub(crate) fn settle_chat_launch(&mut self, action: &ChatLaunchAction) -> bool {
        if self.ui.epoch != action.epoch
            || !action.client.valid()
            || self
                .workspace
                .as_ref()
                .is_none_or(|remote| remote.front_page.target != action.client.target)
        {
            return false;
        }
        if let Some(key) = &action.key {
            self.finish_named_action(key);
        }
        true
    }
    pub(crate) fn complete_chat_launch(
        &mut self,
        action: ChatLaunchAction,
        session: String,
        layout: Option<PaneLayout>,
        cx: &mut Context<NativeRoot>,
    ) {
        self.apply_remote_completion(
            crate::surfaces::front_page::actions::Completion {
                session: Some(session),
                layout,
                ..Default::default()
            },
            &action.before,
            action.navigation,
            cx,
        );
    }
}

impl RemoteClient {
    pub(crate) fn begin_mission_form(
        &mut self,
    ) -> Option<(crate::surfaces::front_page::FrontPageClient, u64, u64)> {
        let remote = self.workspace.as_mut()?;
        if !remote.front_page.supports(&["catalog", "startMission"]) {
            return None;
        }
        remote.form_generation = remote.form_generation.wrapping_add(1);
        Some((
            remote.front_page.clone(),
            remote.navigation_generation,
            remote.form_generation,
        ))
    }
}
