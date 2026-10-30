# Install Runner

Install Runner and sign in to an agent CLI to start working. Runner provides the workspace and terminals; your agent CLI provides the model connection and uses its own account.

## Download the app

Download the latest stable build from [Runner downloads](https://runnersh.dev/downloads/) or [GitHub Releases](https://github.com/yicheng47/runner/releases/latest).

| Platform | Package | Requirements |
| --- | --- | --- |
| macOS | Signed and notarized `.dmg` | macOS 12 or later, Apple Silicon Mac |
| Windows | Signed `Runner-Setup-…-x64.exe` | Windows 10 version 1809 or later, x64 |

Intel Macs, Windows ARM64 and Linux are not supported.

On macOS, open the disk image, drag Runner into Applications, and launch it. On Windows, run the installer and launch Runner from the Start menu. A fresh Windows release may show a SmartScreen warning; after verifying that you downloaded the official installer, **More info → Run anyway** continues installation.

## Set up an agent

1. Install an agent CLI using its provider's instructions. [Supported agents](../reference/supported-agents.md) lists the runtimes and setup links. The default pair-coding crew uses Codex for both roles.
2. Open your usual terminal and run the agent. Complete its sign-in or provider configuration and confirm it can answer a prompt.
3. Open **Settings → Agents** in Runner. Confirm the agent is enabled and that Runner shows its executable and version. Use **Refresh** after installing a CLI; use **Browse** to select its executable if it is outside `PATH`.

On Windows, agents run natively by default. Claude Code and pi's bash tool need Git for Windows; npm-installed CLIs also need Node.js. PowerShell 7 is optional.

Runner does not install the agent CLIs or supply subscriptions. Where supported, **Settings → Agents** offers **Update** when a newer CLI version is available. The update runs the agent's own updater in a terminal.

The dot on the Usage popover's **Agent settings** gear indicates an agent CLI update. Opening Usage or **Settings → Agents** refreshes the installed CLI versions, so the dot clears once every enabled agent is up to date. Updating Runner does not update these separately installed CLIs.

## Use agents in WSL2 on Windows

Install and sign in to your agent CLIs inside a WSL2 distribution. In Windows Runner,
open **Settings → Agents → WSL2**, select a distribution and enable integration.
Leave the Linux user field blank to use the distribution's default user, or enter
another existing Linux user. WSL1 distributions must be upgraded before enabling.

Runner installs its Linux CLI for that distribution and user. If `runner` already
names a different program in `~/.local/bin`, move that command first; Runner will
report the conflict. Add `~/.local/bin` to your Linux shell's `PATH` if needed:

```sh
export PATH="$HOME/.local/bin:$PATH"
runner status
```

Development builds install `runner-dev` instead of `runner`. Windows Runner remains
the owner of your chats, missions and data. WSL interop must be enabled so the Linux
command can reach the Windows application; it does not start a Linux daemon.

Choose the enabled environment in a role, **Start a chat**, or **Start a mission**.
Use a Linux working directory such as `/home/you/src/my-app`; Windows paths and WSL
UNC paths are converted in the selected distribution. Agent sign-ins, configuration
and tools come from the Linux user's environment. The native Windows agent list
continues to describe agents installed on Windows.

A mission uses one environment for all its members, including workers added later.
Saved sessions keep their environment when resumed or forked. Stop that environment's
sessions before disabling integration. After a Runner update, starting WSL work
refreshes the installed CLI; use **Enable / refresh** in Settings if an ordinary
Linux terminal reports a version mismatch or a missing bridge.

## Make the `runner` command available

Open **Settings → General → Command line**. This command is optional for a first chat, but useful when working from a shell or asking another agent to operate Runner.

- **macOS:** Runner automatically installs a link in `~/.local/bin`, or a writable `/usr/local/bin`, when the directory is already on the login `PATH`. If it is not installed, click **Install**. The settings page shows the target and whether an administrator password is needed.
- **Windows:** Runner adds its command directory to your user `PATH`. Open a new terminal so it picks up the change.

Check the command in that terminal:

```sh
runner --version
runner help
```

The **Runner skill for agents** switch in the same settings section installs a small guide for detected agents. It points them to `runner help agents`, which matches the installed CLI version.

## Updates

Runner updates in place and preserves settings, chats and missions. macOS uses Sparkle; Windows offers updates through **Settings → Updates** and the update icon beside Settings. An update restarts running agents so they can resume their saved conversations with the new version.

Next: [start your first chat](first-chat.md).
