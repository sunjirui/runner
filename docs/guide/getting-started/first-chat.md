# Your first chat

Start a chat to work with one agent in its own terminal. You can choose the agent directly or use a saved role with its own instructions and settings.

## Start and finish a small task

1. [Install and sign in to an agent](install.md), then open Runner.
2. Press **⌘N** / **Ctrl+N** to open **Start a chat**. If an empty pane has focus, the chat will fill it; otherwise it starts in a new tab.
3. Choose **Direct** and select your agent, or choose **Role** and select a saved role such as **Coder**. The seeded Coder role requires Codex. Model and effort choices depend on the agent; leaving them at their defaults uses the agent or role configuration.
4. Choose **Project**, or **No project** to list the chat under Recents. A project fills **Working directory** with its folder; check or change the path before starting. A directory inside another project updates the selection when you leave the field.
5. Click **Start chat**, or press **⌘Enter** / **Ctrl+Enter**.
6. In the terminal, send a small prompt such as: “List the top-level files in this folder and explain what each is for. Do not change any files.” Complete any sign-in, trust or approval prompts in the agent's own terminal.
7. Wait for the agent's reply. Its terminal remains available for follow-up questions. When finished, click the **Stop chat** icon at the right of the pane’s header, before ⋯, or **⇧⌘X** / **Ctrl+Shift+X** with that session focused.

A stopped chat remains in Runner. Its centered **Chat stopped** card has a **Resume** button with the current shortcut; use × to archive. Short panes hide the badge and reason first so Resume stays reachable. The **Resume chat** icon beside its status, or **⇧⌘R** / **Ctrl+Shift+R**, continues its saved conversation when the runtime supports it. [Sessions](../concepts/sessions.md) explains the difference between stopping, resuming and closing.

## Add another pane

Press **⌘D** / **Ctrl+D** to split right, or **⇧⌘D** / **Ctrl+Shift+D** to split down. Use **New chat** in the empty pane to select another agent or role. Each pane holds a separate session; splitting does not copy the first agent's conversation or create a mission.

## Open a shell

Press **⌘T** / **Ctrl+Shift+T** to open a terminal. Split a chat pane first to place a shell beside the agent. Chat and terminal tabs have no terminal drawer; **⌥F12** / **Alt+F12** remains a mission-workspace shortcut. A terminal has no Stop or Restart: use Ctrl+C to interrupt a command, and × or `exit` to close its pane. From a chat tab, **New terminal** in the command palette fills an available empty pane or opens a terminal tab; in a mission workspace it adds a drawer shell.

## Close and return later

Runner's background service owns the sessions. In **Settings → General → When Runner quits**, choose whether to ask, keep sessions running, or stop them. With **Keep running**, reopening Runner reconnects to the live terminals. **Resume running agents on launch**, in the same pane, controls recovery of agents that were live when Runner stopped them; it does not automatically restart every chat you stopped manually.

For a team working together on one task, continue to [your first mission](first-mission.md).
