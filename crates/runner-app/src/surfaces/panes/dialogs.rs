use super::*;

impl NativeRoot {
    pub(crate) fn render_terminal_close_confirm(&self, cx: &mut Context<Self>) -> AnyElement {
        let confirm = self
            .terminal_close_confirm
            .as_ref()
            .expect("archive confirm is open");
        let (title, body, confirm_label, pending_label, icon) = match &confirm.target {
            TerminalCloseTarget::ArchiveAll {
                confirmation_body, ..
            } => (
                crate::i18n::text("ui.archive_tab"),
                confirmation_body.resolve().to_string(),
                crate::i18n::text("ui.archive"),
                crate::i18n::text("ui.archiving"),
                "trash.svg",
            ),
            TerminalCloseTarget::ArchiveChatPane { session_id, .. } => {
                let entry = self.front_page_session(session_id, cx);
                let label = entry
                    .as_ref()
                    .map(|entry| entry.label(None))
                    .unwrap_or_else(|| crate::i18n::text("ui.this_chat").into());
                let running = entry
                    .as_ref()
                    .is_some_and(|entry| entry.status == SessionStatus::Running);
                (
                    crate::i18n::text("ui.archive_chat"),
                    archive_chat_confirm_body(&label, running),
                    crate::i18n::text("ui.archive_chat_action"),
                    crate::i18n::text("ui.archiving"),
                    "archive.svg",
                )
            }
        };
        let root = cx.entity();
        let confirm_root = root.clone();
        ConfirmDialog::new(
            title,
            body,
            confirm_label,
            pending_label,
            false,
            Rc::new(move |window, cx| {
                confirm_root.update(cx, |this, cx| this.confirm_terminal_close(window, cx));
            }),
            Rc::new(move |window, cx| {
                root.update(cx, |this, cx| this.cancel_terminal_close(window, cx));
            }),
        )
        .focus_handle(confirm.focus.clone())
        .icon(icon)
        .into_any_element()
    }

    pub(crate) fn render_fork_confirm(&self, cx: &mut Context<Self>) -> AnyElement {
        let confirm = self.fork_confirm.as_ref().expect("fork confirm is open");
        let label = self
            .front_page_session(&confirm.session_id, cx)
            .map(|entry| entry.label(None))
            .unwrap_or_else(|| crate::i18n::text("ui.this_chat").into());
        let body = crate::i18n::tf(
            "format.start_a_new_chat_from_with_its_full_conversation_history_the_fork_opens_in_a_n",
            &[("label", (label).to_string())],
        );
        let busy = confirm.pending;
        let root = cx.entity();
        let confirm_root = root.clone();
        ConfirmDialog::new(
            crate::i18n::text("ui.fork_chat"),
            body,
            crate::i18n::text("ui.fork"),
            crate::i18n::text("ui.forking"),
            busy,
            Rc::new(move |window, cx| {
                confirm_root.update(cx, |this, cx| this.confirm_fork_chat(window, cx));
            }),
            Rc::new(move |_, cx| {
                root.update(cx, |this, cx| this.cancel_fork_chat(cx));
            }),
        )
        .icon("git-fork.svg")
        .variant(ButtonVariant::Primary)
        .into_any_element()
    }

    pub(crate) fn render_chat_rename_modal(&self, cx: &mut Context<Self>) -> AnyElement {
        let modal = self.chat_rename_modal.as_ref().expect("chat rename modal");
        let is_shell = match &modal.target {
            ChatRenameTarget::Session { session_id, .. } => self
                .front_page_session(session_id, cx)
                .is_some_and(|entry| {
                    Runtime::parse(&entry.agent_runtime).is_some_and(Runtime::is_shell)
                }),
        };
        let submitting = modal.submitting;
        let root = cx.entity();
        let close_root = root.clone();
        let cancel_root = root.clone();
        let submit_root = root.clone();
        let dismiss_root = root;
        let title = div()
            .flex()
            .items_center()
            .justify_between()
            .gap_4()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(rems(2. / 16.))
                    .child(
                        div()
                            .text_size(theme::text_heading())
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(if is_shell {
                                crate::i18n::text("ui.rename_terminal")
                            } else {
                                crate::i18n::text("ui.rename_chat")
                            }),
                    )
                    .child(
                        div()
                            .text_size(theme::text_ui())
                            .font_weight(FontWeight::NORMAL)
                            .text_color(theme::muted())
                            .child(crate::i18n::text(
                                "ui.leave_blank_to_restore_the_default_name",
                            )),
                    ),
            )
            .child(
                IconButton::new("close-chat-rename", "close.svg")
                    .focus_handle(modal.close_focus.clone())
                    .tooltip(crate::i18n::text("ui.close_rename"))
                    .disabled(submitting)
                    .on_press(move |window, cx| {
                        close_root.update(cx, |this, cx| this.close_chat_rename(window, cx));
                    }),
            );
        let body = div()
            .flex()
            .flex_col()
            .gap_3()
            .on_key_down(cx.listener(Self::on_chat_rename_key_down))
            .children(
                modal
                    .error
                    .clone()
                    .filter(|_| !self.app_store.read(cx).daemon_disconnected)
                    .map(|error| {
                        runner_app::ui::notice_banner(error, runner_app::ui::Tone::Danger)
                    }),
            )
            .child(
                runner_app::ui::Field::new(
                    "chat-rename-name",
                    crate::i18n::text("ui.name"),
                    modal.input.clone(),
                )
                .focus_target(modal.input.read(cx).focus_handle())
                .emphasized(true),
            );
        let footer = div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                Button::new("cancel-chat-rename", crate::i18n::text("ui.cancel"))
                    .focus_handle(modal.cancel_focus.clone())
                    .disabled(submitting)
                    .on_press(move |window, cx| {
                        cancel_root.update(cx, |this, cx| this.close_chat_rename(window, cx));
                    }),
            )
            .child(
                Button::new(
                    "submit-chat-rename",
                    if submitting {
                        crate::i18n::text("ui.saving")
                    } else {
                        crate::i18n::text("ui.save")
                    },
                )
                .focus_handle(modal.submit_focus.clone())
                .variant(ButtonVariant::Primary)
                .disabled(submitting)
                .on_press(move |window, cx| {
                    submit_root.update(cx, |this, cx| this.submit_chat_rename(window, cx));
                }),
            );
        Modal::new(
            title,
            body,
            Rc::new(move |window, cx| {
                dismiss_root.update(cx, |this, cx| this.close_chat_rename(window, cx));
            }),
        )
        .width(OverlayWidth::Custom(448.))
        .busy(submitting)
        .focus_order(if submitting {
            Vec::new()
        } else {
            vec![
                modal.input.read(cx).focus_handle(),
                modal.cancel_focus.clone(),
                modal.submit_focus.clone(),
                modal.close_focus.clone(),
            ]
        })
        .footer(footer)
        .into_any_element()
    }
}
