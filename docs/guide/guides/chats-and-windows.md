# Chats, tabs and windows

Arrange separate sessions into split tabs, organize them in the sidebar, and use multiple windows without losing track of which terminal you are controlling.

On Windows, the execution environment selector in **Start a chat** can override a
role's default with Native or an enabled WSL2 distribution and Linux user. The chat
header identifies WSL sessions. Resume and fork keep the saved environment. See
[WSL2 setup](../getting-started/install.md#use-agents-in-wsl2-on-windows).

## Know the three surfaces

| Surface | What it holds |
| --- | --- |
| Window | A real operating-system window |
| Tab | A group of panes, represented by one entry in the chat sidebar |
| Pane | One slot holding a single chat or shell session |

**⌘N** / **Ctrl+N** fills the focused empty pane or starts a chat in a new tab. **⌘T** / **Ctrl+Shift+T** does the same for a shell. A new tab does not start a crew; use a [mission](../concepts/missions.md) when agents need a shared feed.

## Split and navigate

Use **⌘D** / **Ctrl+D** to split right and **⇧⌘D** / **Ctrl+Shift+D** to split down. Choose **New chat** in an empty pane or select a chat from the sidebar. Drag pane headers to reorder the layout; the layout picker offers common arrangements. Splitting is bounded by the available window size.

Use **⌘[** / **Ctrl+[** and **⌘]** / **Ctrl+]** to move focus between panes. **⌘1–9** / **Ctrl+1–9** selects the corresponding visible sidebar tab or mission. **⇧⌘[** / **Ctrl+Shift+[** and **⇧⌘]** / **Ctrl+Shift+]** move through recently viewed pages.

Drag files from Finder or File Explorer onto a chat pane, a drawer terminal or a mission slot to insert their paths at the prompt, quoted as when you paste files. The terminal highlights while the files are over it and takes focus when you drop them; nothing runs until you press Return.

For shell splits, Runner follows the shell's reported working directory. It installs that reporting for zsh and bash on macOS. Windows shells without their own directory-reporting prompt use the original launch directory for a split, even after `cd`.

## Organize the sidebar

Use the sidebar context menus to rename and organize tabs, group them into folders, and pin work you return to often. [Projects](../concepts/projects.md) group work by directory; folders organize sidebar entries. Status markers show activity, attention and stopped sessions. Use them to find the terminal to inspect, rather than treating an idle marker as a completed review.

**⌘K** / **Ctrl+K** opens the command palette to find chats, missions, roles, crews and commands. **⌘S** / **Ctrl+S** toggles the sidebar.

## Use another window

Press **⇧⌘N** / **Ctrl+Shift+N** to open another Runner window. If a chat or mission is already being driven in another window, Runner shows an **Open in another window** overlay. Use its handoff controls to focus or bring the subject here. A terminal has one owning window at a time; the overlay prevents two windows from sending competing input.

Close the window with **⇧⌘W** / **Alt+F4**. Its layout is saved, and sessions can keep running. On Windows, closing the last window follows **Settings → General → When Runner quits**. On macOS, quitting the application is separate from closing a window.

**⌘W** / **Ctrl+Shift+W** acts on the focused drawer, pane or chat tab. It is different from closing the whole window. Use **Stop** when you specifically want to halt an agent, and **Archive** when you want a chat out of active lists.

For alerts while the app is in the background, open **Settings → Notifications**. Chat events are enabled by default; mission events are opt-in. Clicking a notification opens its chat or mission.

See [Keyboard shortcuts](../reference/keyboard-shortcuts.md) for all defaults and customization.
