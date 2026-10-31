use super::*;

pub(super) fn start_action_shortcut(
    binding: &str,
    overrides: &keymap::KeymapOverrides,
) -> Option<String> {
    keymap::effective_binding(binding, overrides).map(|combo| keymap::format_combo(&combo))
}
