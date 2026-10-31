# 1092 — One launcher for starting chats and terminals

> Tracking issue: [#1092](https://github.com/yicheng47/runner/issues/1092).
> Status: in development since 2026-10-31.
> Design: `runner/desktop/specs/1092-chat-terminal-launcher.pen` in `yeelinks_design` at `21be219`: the launcher in a pane, the default page, the default page while typing, and a click table.

## Motivation

Starting work takes two steps per pane. A split leaves an empty pane that asks “chat or terminal?” (**Open in this pane**), and **New chat** then opens the **Start a chat** form (Direct/Role, agent, model, effort, speed, name, project and working directory) for every chat, including one in a split, where the project is locked and the directory is usually the neighbouring pane's. The default page (**Start something new**) is a third surface with the same choices. Chat and terminal are separate actions with separate rows and code paths, although in a pane both are a process in a directory.

From the sidebar, a chat with a different agent takes four clicks and a second agent beside the first takes four to six. With the launcher they take two and three.

## Scope

### One surface

- The launcher replaces the empty-pane picker (`panes/pane.rs`), the default page (`render_start_surface` in `panes/start.rs`, shown when `start_surface_visible`: a window with no tab, or a tab of one empty pane) and the Start a chat form as the way to start chats. It is drawn inside the pane under the island header, whose title is **New** with a plus mark. Nothing opens as an overlay.
- **New chat** (⌘N), the sidebar's New chat row, a project row's New chat item and the command palette's New chat open a new tab whose only pane shows the launcher. When the focused pane is empty, ⌘N focuses its launcher instead, as ⌘N fills a focused empty pane today.
- A split (⌘D, ⇧⌘D or the split menu) leaves an empty pane showing the launcher. In a tab with terminals and no chat panes, a split starts another shell as today, even if that tab also has an empty pane (Jason, 2026-10-31).
- ⌘T still starts a terminal directly, in the focused empty pane or a new tab, and ⇧⌘M still opens Start mission.

### Layout

Match `runner/desktop/specs/1092-chat-terminal-launcher.pen`: node `XlBTt` for the default page and `g8HlmM` for a split pane, using the existing theme tokens. A centred column, 400 px wide in a pane and 440 px on the default page, with 8 px gaps in a pane and 12 px on the default page, holds, with no title on either page:

1. A 32 px search field, focused when the launcher appears, with the placeholder **Search agents and roles**.
2. The start location. On the default page it is a line under the search field, inline without a box: a folder icon, the project picker (`ProjectField`), the project's path in the mono font and **Change**, so the project's name and its folder read together. **Change** opens the existing directory picker: the OS folder picker locally, the host's `FolderPicker` remotely. A split pane has no start location: its chat files with the tab's project, which stays locked as today, and starts in the folder the existing `terminal_start_location` rule gives, normally a neighbouring session's. There is no note naming the neighbouring pane.
3. A list card with no section label: up to four ranked rows, a divider, then pinned rows: **Terminal**, and **Mission** on the default page only. There is no hint line.

A split pane measures itself and drops ranked rows from the bottom, down to one, until the card fits, then drops **Terminal** (⌘T still starts one), then the last row, leaving the search field alone; a pane too short for even that loses the vertical padding. Dragging a divider (15–85 %) or shrinking the window can make a pane shorter than the 160 px a split needs. The 1–4 keys cover only the rows shown; typing still searches every agent and role.

A 30 px row has a key badge (1–4 for ranked rows; the effective shortcut for pinned rows, ⌘T and ⇧⌘M by default, in chips 40 px wide so their marks line up), a 20 px mark (the runtime mark on a raised tile, the role's `RoleAvatar`, or the terminal or users icon), the name, a detail and, on the selected row, a ↵ glyph. A role's detail is **Role · {runtime}**, followed by the role's model and effort when it sets them. An agent's detail is **Agent**: the PNGs' model and effort on agent rows are illustrative, since a chat started without a form uses the agent's own configuration. Search results also show the last use at the right (**2h ago**, **Yesterday**, a date, or **Never used**).

### Order and search

- With an empty search, the ranked rows are the four agents and roles most recently started, mixed and newest first. When fewer than four have been used, fill with the Settings default agent, then the remaining selectable agents in catalog order, then roles by name.
- Typing searches every selectable agent (`filter_selectable_runtime_catalog`) and every role, by display name, runtime key (such as `claude`) and role handle, case-insensitively. Rank a prefix match of the name or handle first, then a match at a word start, then any other substring; break ties by last use (never used last), then by name. Highlight the matched characters in the accent colour.
- Pinned rows are not ranked and stay visible while searching.
- Keys: ↑ and ↓ move through ranked and pinned rows; ↵ starts the selected row; 1–4 start a ranked row only while the search is empty, and otherwise go into the search; Esc clears a non-empty search. A click starts a row.
- With no selectable agents and no roles, the list shows the existing “no agents” message with a way to Settings → Agents, and Terminal stays pinned.
- A role whose runtime is unavailable is listed, dimmed, with the existing reason, and does not start.

### Recent use

Recency is one list for the whole app, kept per computer (local, or each remote host), not per project. It replaces `start-chat-recent-runtimes`, which holds Direct agents only: migrate its entries as agent entries, then store agent and role entries in one app-data file, newest first. Record an entry whenever a chat starts from the launcher or from a role page. Skip entries whose role was deleted or whose agent is no longer selectable. Nothing is stored in the daemon or the database.

### Starting

- There is no form. An agent row starts a Direct chat with no model, effort or speed override. A role row starts a role chat with the role's settings. Both use the project and the folder shown, except that a role with its own working directory starts there unless the person chose a folder with **Change**.
- **Terminal** starts a shell in the folder shown, in this pane, as ⌘T does for a focused empty pane today.
- **Mission** opens Start mission with the project preselected.
- While a start is pending, its row shows a spinner and further starts are ignored. A failure shows its error above the list and leaves the launcher in place.
- A chat starts unnamed; people rename it from the pane menu, and terminal-provided titles apply as today.

### Remote

In a remote window the launcher lists the host's selectable agents and roles, uses the host's projects and folder browser, and starts through the existing client calls. Claude Code rows show the host's active account, as the form does today. Rows whose capability the host lacks (`catalog`, `startChat`, `startShell`, `layout`, `startMission`) are hidden. The remote role list's **Chat** starts a role chat directly, like the local role page.

### Removed

- The empty-pane picker, the default page's three rows, and Start a chat as a way to start chats, with ⌘1 and ⌘2 (Direct and Role). `StartChatModal` stays only for adding a role to a mission (`start_chat/add_role.rs`); delete code that only the chat form used.

### Out of scope

A per-chat terminal button that opens a shell in the chat's folder; starting a managed agent by typing its command in a shell; the add-role-to-mission form; Start mission's form.

## Implementation phases

1. **Launcher model:** query, selection, ranking, fill rules, key handling and the recency store with its migration, as pure logic with unit tests.
2. **Launcher surface:** the view in empty panes and as the default page; the entry points; starting agents, roles, terminals and missions; the remote window.
3. **Removal and docs:** remove the chat form's start path, the old picker and the start page's rows; update the guide, architecture and regression rows.

## Verification

- Unit tests: ranking (prefix, word start, substring, recency ties, never used), the empty-search fill, the recency migration and per-computer lists, digits with an empty and a non-empty search, ↑↓ across pinned rows, Esc, every entry point (⌘N with and without a focused empty pane, the sidebar, a project row, the palette, a split), start requests for agents, roles, terminals and missions (no overrides, the folder rules, a role's own directory), an unavailable role, remote capability hiding and the remote role-list start.
- Live checks in the development app: the default page with no tab; ⌘N, then starting a bounded test role found by typing; a split, then **Terminal** in the neighbour's folder; **Change** and the project picker on the default page; the Recent order after starting two different entries. Remote windows and Windows/Linux native checks are left to Jason.

## Affected documentation

[First chat](../guide/getting-started/first-chat.md), [Chats and windows](../guide/guides/chats-and-windows.md), [Keyboard shortcuts](../guide/reference/keyboard-shortcuts.md) (New chat's description, and the ⌘1/⌘2 rows), [Projects](../guide/concepts/projects.md), [Roles](../guide/concepts/roles.md), [Remote access](../guide/guides/remote-access.md) (New chat's Browse), and their zh-CN copies; [runtime integration](../arch/runtime-integration.md) where it names New chat; the [workspace](../tests/regression/workspace.md) rows WS-CREATE-01, -03, -06, -07 and -09 and the [remote](../tests/regression/remote.md) rows REMOTE-CHAT-01 and -05.
