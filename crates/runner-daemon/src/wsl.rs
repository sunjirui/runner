//! Windows-side WSL integration and a bounded, typed Linux helper interface.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use runner_core::execution_target::{parse_distributions, WslDistribution};
use runner_core::ExecutionTarget;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::session::runtime::SpawnSpec;

const TIMEOUT: Duration = Duration::from_secs(20);
const MAX_OUTPUT: u64 = 8 * 1024 * 1024;
pub const LINUX_BINARY: &str = "runner-agent-cli-linux";

static INTEGRATIONS: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn load_integrations(
    app_data: &Path,
) -> Result<Vec<runner_core::execution_target::WslIntegration>> {
    Ok(
        match std::fs::read(app_data.join("wsl/integrations.json")) {
            Ok(bytes) => serde_json::from_slice(&bytes)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(error.into()),
        },
    )
}

pub fn status(core: &crate::AppCore) -> Result<runner_core::execution_target::WslStatus> {
    Ok(runner_core::execution_target::WslStatus {
        distributions: distributions()?,
        integrations: load_integrations(&core.app_data_dir)?,
    })
}

fn save_integrations(
    app_data: &Path,
    integrations: &[runner_core::execution_target::WslIntegration],
) -> Result<()> {
    let dir = app_data.join("wsl");
    std::fs::create_dir_all(&dir)?;
    let mut file = tempfile::NamedTempFile::new_in(&dir)?;
    serde_json::to_writer(&mut file, integrations)?;
    file.flush()?;
    file.persist(dir.join("integrations.json"))
        .map_err(|e| Error::Io(e.error))?;
    Ok(())
}

pub fn enable(
    core: &crate::AppCore,
    target: ExecutionTarget,
) -> Result<runner_core::execution_target::WslIntegration> {
    let _lock = INTEGRATIONS.lock().unwrap_or_else(|e| e.into_inner());
    enable_at(&core.app_data_dir, target)
}

pub fn ensure_current(app_data: &Path, target: &ExecutionTarget) -> Result<()> {
    let _lock = INTEGRATIONS.lock().unwrap_or_else(|e| e.into_inner());
    let integration = load_integrations(app_data)?.into_iter().find(|i| &i.target == target)
        .ok_or_else(|| Error::msg("Enable this distribution and Linux user in Settings → Agents → WSL2 before starting an agent"))?;
    if integration.build != runner_core::wsl_bridge::build_identity() {
        enable_at(app_data, target.clone())?;
    }
    Ok(())
}

fn enable_at(
    app_data: &Path,
    target: ExecutionTarget,
) -> Result<runner_core::execution_target::WslIntegration> {
    ensure_wsl2(&target)?;
    if target.is_native() {
        return Err(Error::msg(
            "WSL integration requires a distribution and Linux user",
        ));
    }
    let source = app_data.join("bin").join(LINUX_BINARY);
    if !source.is_file() {
        return Err(Error::msg(
            "Bundled Linux runner CLI is missing; rebuild or reinstall the Windows Runner package",
        ));
    }
    let source = linux_path(&target, &source)?;
    let bridge = linux_path(&target, &app_data.join("bin/runner.exe"))?;
    let target_json = serde_json::to_string(&target)?;
    let quote = crate::session::launch::shell_quote;
    let script = format!(
        "#!/bin/sh\n# runner-managed-wsl-v1\nexport {}={}\nexport {}={}\nexport RUNNER_WSL_INTEGRATION_DIR={}\nexec \"$HOME\"/{} \"$@\"\n",
        runner_core::wsl_bridge::EXECUTABLE_ENV, quote(&bridge),
        runner_core::wsl_bridge::TARGET_ENV, quote(&target_json),
        quote(&integration_dir()),
        quote(&format!("{}/bin/runner-cli", integration_dir())),
    );
    let mut cmd = command(&target, None)?;
    cmd.args([
        "/bin/sh",
        "-c",
        INSTALL_SCRIPT,
        "runner",
        &integration_dir(),
        &source,
        if cfg!(debug_assertions) {
            "runner-dev"
        } else {
            "runner"
        },
    ]);
    output(cmd, Some(script.into_bytes()))?;
    let probe: Probe = helper(&target, &HelperRequest::Probe)?;
    let mut bridge_probe = command(&target, None)?;
    bridge_probe.args([
        "/bin/sh",
        "-c",
        r#"exec "$HOME/$1/bin/runner" status --json"#,
        "runner",
        &integration_dir(),
    ]);
    let _: serde_json::Value = serde_json::from_slice(&output(bridge_probe, None)?)?;
    let integration = runner_core::execution_target::WslIntegration {
        target,
        home: probe.home,
        path: probe.path,
        agents: probe.agents,
        build: probe.build,
    };
    let mut integrations = load_integrations(app_data)?;
    integrations.retain(|i| i.target != integration.target);
    integrations.push(integration.clone());
    save_integrations(app_data, &integrations)?;
    Ok(integration)
}

pub fn enable_distribution(
    core: &crate::AppCore,
    distro: String,
    user: Option<String>,
) -> Result<runner_core::execution_target::WslIntegration> {
    let default_user = user.as_ref().is_none_or(|u| u.trim().is_empty());
    let mut target = ExecutionTarget::Wsl {
        distro: distro.clone(),
        user: user
            .filter(|u| !u.trim().is_empty())
            .unwrap_or_else(|| "root".into()),
    };
    ensure_wsl2(&target)?;
    if default_user {
        let mut cmd = Command::new("wsl.exe");
        cmd.args(["--distribution", &distro, "--exec", "id", "-un"]);
        let value = String::from_utf8(output(cmd, None)?).map_err(|e| Error::msg(e.to_string()))?;
        target = ExecutionTarget::Wsl {
            distro,
            user: value.trim().into(),
        };
    }
    enable(core, target)
}

const INSTALL_SCRIPT: &str = r#"set -eu
umask 077
root="$HOME/$1"
source=$2
name=$3
public="$HOME/.local/bin/$name"
if [ -e "$public" ] || [ -L "$public" ]; then
  [ -L "$public" ] && [ "$(readlink "$public")" = "$root/bin/runner" ] || { echo "An existing $public is not managed by Runner; move it before enabling integration" >&2; exit 1; }
fi
if [ -e "$root/bin/runner" ]; then
  grep -q "^# runner-managed-wsl-v1$" "$root/bin/runner" || { echo "Existing integration wrapper is not managed by Runner" >&2; exit 1; }
fi
mkdir -p "$root/bin" "$HOME/.local/bin"
tmp="$root/bin/runner-cli.$$"
trap 'rm -f "$tmp" "$root/bin/runner.$$"' EXIT
cp -- "$source" "$tmp"
chmod 700 "$tmp"
mv -f -- "$tmp" "$root/bin/runner-cli"
cat > "$root/bin/runner.$$"
chmod 700 "$root/bin/runner.$$"
mv -f -- "$root/bin/runner.$$" "$root/bin/runner"
ln -sfn -- "$root/bin/runner" "$public"
"#;

pub fn disable(core: &crate::AppCore, target: ExecutionTarget) -> Result<()> {
    require_windows_target(&target)?;
    let _lock = INTEGRATIONS.lock().unwrap_or_else(|e| e.into_inner());
    for id in core.sessions.live_session_ids() {
        if crate::repo::session::get_row(&*core.db.get()?, &id)?
            .and_then(|r| r.launch_definition)
            .is_some_and(|d| d.execution_target == target)
        {
            return Err(Error::msg(
                "Stop this environment's sessions before disabling WSL integration",
            ));
        }
    }
    let mut integrations = load_integrations(&core.app_data_dir)?;
    if !integrations.iter().any(|i| i.target == target) {
        return Ok(());
    }
    let mut cmd = command(&target, None)?;
    cmd.args(["/bin/sh", "-c", r#"set -eu
root="$HOME/$1"
public="$HOME/.local/bin/$2"
if [ -f "$root/bin/runner" ]; then
  grep -q "^# runner-managed-wsl-v1$" "$root/bin/runner" || { echo "Integration wrapper is no longer managed by Runner" >&2; exit 1; }
fi
if [ -L "$public" ] && [ "$(readlink "$public")" = "$root/bin/runner" ]; then rm -- "$public"; fi
for name in runner runner-cli; do
  if [ -f "$root/bin/$name" ]; then rm -- "$root/bin/$name"; fi
done"#, "runner", &integration_dir(), if cfg!(debug_assertions) { "runner-dev" } else { "runner" }]);
    output(cmd, None)?;
    integrations.retain(|i| i.target != target);
    save_integrations(&core.app_data_dir, &integrations)
}

pub fn require_windows_target(target: &ExecutionTarget) -> Result<()> {
    target.validate().map_err(Error::msg)?;
    if !target.is_native() && !cfg!(windows) {
        return Err(Error::msg(
            "WSL execution targets require Windows Runner and WSL2",
        ));
    }
    Ok(())
}

fn output(command: Command, input: Option<Vec<u8>>) -> Result<Vec<u8>> {
    output_with_timeout(command, input, TIMEOUT)
}

fn output_with_timeout(
    mut command: Command,
    input: Option<Vec<u8>>,
    timeout: Duration,
) -> Result<Vec<u8>> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut stdin = child.stdin.take().unwrap();
    let input_writer = std::thread::spawn(move || -> std::io::Result<()> {
        if let Some(input) = input {
            stdin.write_all(&input)?;
        }
        Ok(())
    });
    let capture = |stream: Box<dyn Read + Send>| {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let result = stream
                .take(MAX_OUTPUT + 1)
                .read_to_end(&mut bytes)
                .map(|_| bytes);
            let _ = tx.send(result);
        });
        rx
    };
    let stdout = capture(Box::new(child.stdout.take().unwrap()));
    let stderr = capture(Box::new(child.stderr.take().unwrap()));
    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Error::msg(
                "WSL command timed out; check the distribution and WSL interop",
            ));
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let capture_result =
        |rx: std::sync::mpsc::Receiver<std::io::Result<Vec<u8>>>| -> Result<Vec<u8>> {
            let bytes = rx
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .map_err(|_| Error::msg("WSL output did not close"))??;
            if bytes.len() as u64 > MAX_OUTPUT {
                return Err(Error::msg("WSL response is too large"));
            }
            Ok(bytes)
        };
    let stdout = capture_result(stdout)?;
    let stderr = capture_result(stderr)?;
    input_writer
        .join()
        .map_err(|_| Error::msg("WSL input writer failed"))??;
    if !status.success() {
        return Err(Error::msg(format!(
            "WSL command failed ({status}): {}",
            String::from_utf8_lossy(&stderr).trim()
        )));
    }
    Ok(stdout)
}

pub fn distributions() -> Result<Vec<WslDistribution>> {
    if !cfg!(windows) {
        return Ok(Vec::new());
    }
    let mut cmd = Command::new("wsl.exe");
    cmd.args(["--list", "--verbose"]);
    parse_distributions(&output(cmd, None)?).map_err(Error::msg)
}

fn command(target: &ExecutionTarget, cwd: Option<&str>) -> Result<Command> {
    require_windows_target(target)?;
    let mut command = Command::new("wsl.exe");
    command.args(target.wsl_args(cwd).map_err(Error::msg)?);
    Ok(command)
}

fn ensure_wsl2(target: &ExecutionTarget) -> Result<()> {
    require_windows_target(target)?;
    let ExecutionTarget::Wsl { distro, .. } = target else {
        return Ok(());
    };
    let row = distributions()?
        .into_iter()
        .find(|row| &row.name == distro)
        .ok_or_else(|| Error::msg(format!("WSL distribution {distro:?} is not installed")))?;
    if row.version != 2 {
        return Err(Error::msg(format!(
            "{distro} uses WSL{}; Runner requires WSL2 (wsl --set-version {distro} 2)",
            row.version
        )));
    }
    Ok(())
}

pub fn linux_path(target: &ExecutionTarget, path: &Path) -> Result<String> {
    let path = path.to_string_lossy();
    if let Some(unc) = path.strip_prefix(r"\\") {
        let mut parts = unc.splitn(3, '\\');
        if parts.next().is_some_and(|server| {
            server.eq_ignore_ascii_case("wsl.localhost") || server.eq_ignore_ascii_case("wsl$")
        }) {
            let distro = parts.next().unwrap_or_default();
            let ExecutionTarget::Wsl {
                distro: selected, ..
            } = target
            else {
                return Err(Error::msg("WSL UNC paths require a WSL execution target"));
            };
            if !distro.eq_ignore_ascii_case(selected) {
                return Err(Error::msg(
                    "The WSL directory belongs to a different distribution",
                ));
            }
            let path = format!("/{}", parts.next().unwrap_or_default().replace('\\', "/"));
            runner_core::execution_target::validate_linux_path(&path).map_err(Error::msg)?;
            return Ok(path);
        }
    }
    if path.starts_with('/') {
        runner_core::execution_target::validate_linux_path(&path).map_err(Error::msg)?;
        return Ok(path.into_owned());
    }
    let mut cmd = command(target, None)?;
    cmd.args(["wslpath", "-a", "-u", &path]);
    let path = String::from_utf8(output(cmd, None)?).map_err(|e| Error::msg(e.to_string()))?;
    let path = path.trim_end_matches(['\r', '\n']);
    runner_core::execution_target::validate_linux_path(path).map_err(Error::msg)?;
    Ok(path.into())
}

pub fn resolve_cwd(
    target: &ExecutionTarget,
    explicit: Option<&str>,
    role_default: Option<&str>,
) -> Result<Option<String>> {
    if target.is_native() {
        return Ok([explicit, role_default]
            .into_iter()
            .flatten()
            .find(|s| !s.trim().is_empty())
            .map(str::to_owned)
            .or_else(|| {
                runner_core::app_paths::home_dir().map(|p| p.to_string_lossy().into_owned())
            }));
    }
    ensure_wsl2(target)?;
    let cwd = [explicit, role_default]
        .into_iter()
        .flatten()
        .find(|s| !s.trim().is_empty());
    let cwd = cwd.map(|p| linux_path(target, Path::new(p))).transpose()?;
    let mut cmd = command(target, cwd.as_deref())?;
    // A real directory check in the selected user's namespace, including symlinks.
    if cwd.is_some() {
        cmd.args(["pwd", "-P"]);
    } else {
        cmd.args(["/bin/sh", "-c", r#"cd "$HOME" && pwd -P"#]);
    }
    let value = String::from_utf8(output(cmd, None)?).map_err(|e| Error::msg(e.to_string()))?;
    let value = value.trim_end_matches(['\r', '\n']);
    runner_core::execution_target::validate_linux_path(value).map_err(Error::msg)?;
    Ok(Some(value.into()))
}

pub fn integration_dir() -> String {
    #[cfg(target_os = "linux")]
    if let Ok(root) = std::env::var("RUNNER_WSL_INTEGRATION_DIR") {
        if matches!(
            root.as_str(),
            ".local/share/com.wycstudios.runner/wsl" | ".local/share/com.wycstudios.runner-dev/wsl"
        ) {
            return root;
        }
    }
    format!(
        ".local/share/{}/wsl",
        runner_core::app_paths::app_identifier(cfg!(debug_assertions))
    )
}

fn helper_command(target: &ExecutionTarget) -> Result<Command> {
    let mut cmd = command(target, None)?;
    cmd.args([
        "/bin/sh",
        "-c",
        "exec \"$HOME/$1/bin/runner\" --wsl-helper",
        "runner",
        &integration_dir(),
    ]);
    Ok(cmd)
}

pub fn helper<T: serde::de::DeserializeOwned>(
    target: &ExecutionTarget,
    request: &HelperRequest,
) -> Result<T> {
    let timeout = if matches!(request, HelperRequest::Fork { .. }) {
        Duration::from_secs(140)
    } else {
        TIMEOUT
    };
    let request = serde_json::to_vec(&HelperEnvelope {
        version: runner_core::wsl_bridge::VERSION.into(),
        build: runner_core::wsl_bridge::build_identity(),
        request: request.clone(),
    })?;
    serde_json::from_slice(&output_with_timeout(
        helper_command(target)?,
        Some(request),
        timeout,
    )?)
    .map_err(Into::into)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HelperEnvelope {
    version: String,
    build: String,
    request: HelperRequest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HelperRequest {
    Fork {
        spec: Box<SpawnSpec>,
        args: Vec<String>,
        source_key: String,
        timeout_seconds: u64,
    },
    Stop {
        identity: crate::wsl_session::Identity,
    },
    Probe,
    Prepare {
        spec: Box<SpawnSpec>,
        role: Box<crate::model::Role>,
        plan: crate::router::runtime::ResumePlan,
        system_prompt: Option<String>,
        first_turn: Option<String>,
        mission_dir: Option<String>,
    },
    ConversationExists {
        runtime: crate::model::Runtime,
        cwd: Option<String>,
        key: String,
        env: std::collections::HashMap<String, String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Probe {
    pub user: String,
    pub home: String,
    pub path: String,
    pub agents: BTreeMap<String, String>,
    pub build: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prepared {
    pub spec: SpawnSpec,
    pub first_turn_in_argv: bool,
}

/// Runs in the Linux CLI process. It opens no database, daemon or listener.
pub fn run_helper() -> Result<()> {
    if !cfg!(target_os = "linux") {
        return Err(Error::msg("WSL helper requires the bundled Linux CLI"));
    }
    let mut bytes = Vec::new();
    std::io::stdin()
        .take(MAX_OUTPUT + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_OUTPUT {
        return Err(Error::msg("WSL helper request is too large"));
    }
    let envelope: HelperEnvelope = serde_json::from_slice(&bytes)?;
    if envelope.version != runner_core::wsl_bridge::VERSION
        || envelope.build != runner_core::wsl_bridge::build_identity()
    {
        return Err(Error::msg(
            "WSL helper version differs; refresh WSL integration",
        ));
    }
    let response = match envelope.request {
        HelperRequest::Fork {
            mut spec,
            args,
            source_key,
            timeout_seconds,
        } => {
            spec.execution_target = ExecutionTarget::Native;
            let shell = crate::shell_path::resolve_login_shell_env().env;
            let mut env = shell.vars;
            env.extend(spec.env.clone());
            spec.env = env;
            spec.shell_path = shell.path;
            let plan = crate::router::runtime::ForkPlan::Headless { args, source_key };
            serde_json::to_value(crate::session::manager::run_headless_fork(
                &spec,
                &plan,
                Duration::from_secs(timeout_seconds.min(120)),
            )?)?
        }
        HelperRequest::Stop { identity } => {
            serde_json::to_value(crate::wsl_session::stop_linux(&identity)?)?
        }
        HelperRequest::Probe => {
            let env = crate::shell_path::resolve_login_shell_env().env;
            let path = env
                .path
                .unwrap_or_else(|| std::env::var("PATH").unwrap_or_default());
            let agents = crate::runtimes::catalogs()
                .into_iter()
                .filter_map(|runtime| {
                    std::env::split_paths(&path)
                        .map(|dir| dir.join(runtime.command))
                        .find(|p| p.is_file())
                        .map(|p| (runtime.name.to_string(), p.to_string_lossy().into_owned()))
                })
                .collect();
            serde_json::to_value(Probe {
                user: String::from_utf8(Command::new("id").arg("-un").output()?.stdout)
                    .map_err(|e| Error::msg(e.to_string()))?
                    .trim()
                    .into(),
                home: runner_core::app_paths::home_dir()
                    .ok_or_else(|| Error::msg("Linux home is unavailable"))?
                    .to_string_lossy()
                    .into_owned(),
                path,
                agents,
                build: runner_core::wsl_bridge::build_identity(),
            })?
        }
        HelperRequest::ConversationExists {
            runtime,
            cwd,
            key,
            env,
        } => {
            let mut effective = crate::shell_path::resolve_login_shell_env()
                .env
                .vars
                .into_iter()
                .collect::<std::collections::HashMap<_, _>>();
            effective.extend(env);
            serde_json::to_value(crate::runtimes::adapter(runtime).conversation_exists(
                &key,
                &crate::runtimes::ProbeContext {
                    cwd: cwd.as_deref(),
                    role_env: &effective,
                },
            ))?
        }
        HelperRequest::Prepare {
            mut spec,
            role,
            plan,
            system_prompt,
            first_turn,
            mission_dir,
        } => {
            let runtime = spec
                .agent_runtime
                .ok_or_else(|| Error::msg("WSL runtime is missing"))?;
            let adapter = crate::runtimes::adapter(runtime);
            let shell = crate::shell_path::resolve_login_shell_env().env;
            let mut env = shell.vars;
            env.extend(spec.env.clone());
            spec.env = env;
            spec.shell_path = shell.path;
            spec.shim_dir = None;
            let home = runner_core::app_paths::home_dir()
                .ok_or_else(|| Error::msg("Linux home is unavailable"))?;
            if runtime == crate::model::Runtime::Codex && !spec.env.contains_key("CODEX_HOME") {
                spec.env.insert(
                    "CODEX_HOME".into(),
                    home.join(".codex").to_string_lossy().into_owned(),
                );
            }
            if !spec.env.contains_key("CURSOR_CONFIG_DIR") {
                let cursor = spec
                    .env
                    .get("XDG_CONFIG_HOME")
                    .map(|p| std::path::PathBuf::from(p).join("cursor"))
                    .unwrap_or_else(|| home.join(".cursor"));
                spec.env.insert(
                    "CURSOR_CONFIG_DIR".into(),
                    cursor.to_string_lossy().into_owned(),
                );
            }
            for (key, relative) in [
                ("COPILOT_HOME", ".copilot"),
                ("GROK_HOME", ".grok"),
                ("XDG_DATA_HOME", ".local/share"),
                ("XDG_CONFIG_HOME", ".config"),
            ] {
                spec.env.entry(key.into()).or_insert_with(|| {
                    std::env::var(key)
                        .unwrap_or_else(|_| home.join(relative).to_string_lossy().into_owned())
                });
            }
            for key in [
                "CODEX_HOME",
                "CLAUDE_CONFIG_DIR",
                "COPILOT_HOME",
                "GROK_HOME",
                "CURSOR_CONFIG_DIR",
                "XDG_DATA_HOME",
                "XDG_CONFIG_HOME",
            ] {
                if let Some(value) = spec.env.get_mut(key) {
                    let path = Path::new(value);
                    if !path.is_absolute() {
                        *value = spec
                            .cwd
                            .as_deref()
                            .unwrap_or(&home)
                            .join(path)
                            .to_string_lossy()
                            .into_owned();
                    }
                }
            }
            let app_data = runner_core::app_paths::home_dir()
                .ok_or_else(|| Error::msg("Linux home is unavailable"))?
                .join(integration_dir());
            spec.bundled_bin_dir = Some(app_data.join("bin"));
            spec.env.insert(
                "RUNNER_WSL_APP_DATA".into(),
                app_data.to_string_lossy().into_owned(),
            );
            if spec.mission {
                let mission = spec
                    .env
                    .get("RUNNER_MISSION_ID")
                    .ok_or_else(|| Error::msg("WSL mission identity is missing"))?;
                let handle = spec
                    .env
                    .get("RUNNER_HANDLE")
                    .ok_or_else(|| Error::msg("WSL slot identity is missing"))?;
                let log = spec
                    .env
                    .get("RUNNER_EVENT_LOG")
                    .ok_or_else(|| Error::msg("WSL mission log identity is missing"))?;
                spec.shim_dir = Some(crate::cli_install::install_session_runner_shim(
                    &app_data,
                    spec.env.get("RUNNER_CREW_ID").map(String::as_str),
                    mission,
                    handle,
                    Path::new(log),
                    spec.cwd.as_deref().and_then(Path::to_str),
                )?);
            }
            adapter.seed_trust(
                &spec.session_id,
                spec.cwd.as_deref(),
                spec.env.get("COPILOT_HOME").map(String::as_str),
            )?;
            spec.env.extend(
                adapter
                    .launch_env()
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string())),
            );
            spec.pending_turn = adapter
                .status_hooks()
                .is_some_and(|h| h.tracks_pending_turn())
                .then_some(!plan.resuming && first_turn.is_some());
            if let Some(hooks) = adapter.status_hooks() {
                hooks.install(&app_data);
                spec.env
                    .extend(hooks.env(&role.args, &plan, &app_data, &spec));
            }
            if matches!(adapter.key_capture(), crate::runtimes::KeyCapture::LogTail) {
                crate::runtimes::antigravity::agy_capture::prepare_log(&app_data, &spec.session_id);
            }
            let ctx = crate::runtimes::LaunchContext {
                default_model: None,
                env: &spec.env,
                cwd: spec.cwd.as_deref(),
                role_args: &role.args,
                app_data_dir: &app_data,
                session_id: &spec.session_id,
                resuming: plan.resuming,
                mission: spec.mission,
                model: role.model.as_deref(),
                effort: role.effort.as_deref(),
                codex_speed: role.codex_speed,
                system_prompt: system_prompt.as_deref(),
                first_turn: first_turn.as_deref(),
            };
            let context_env = adapter.launch_context_env(&ctx);
            let launch_args = adapter.launch_args(&ctx);
            spec.env.extend(context_env);
            let mut args = adapter.launch_role_args(&role.args);
            if plan.prepend {
                let mut prefix = plan.args;
                prefix.append(&mut args);
                args = prefix;
            } else {
                args.extend(plan.args);
            }
            args.extend(adapter.mission_dir_args(mission_dir.as_deref().map(Path::new)));
            args.extend(launch_args);
            let in_argv = !adapter.first_turn_argv(first_turn.as_deref()).is_empty();
            spec.args = args;
            serde_json::to_value(Prepared {
                spec: *spec,
                first_turn_in_argv: in_argv,
            })?
        }
    };
    serde_json::to_writer(std::io::stdout().lock(), &response)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linux_and_unc_paths_keep_case_and_reject_other_distributions() {
        let target = ExecutionTarget::Wsl {
            distro: "Ubuntu".into(),
            user: "me".into(),
        };
        assert_eq!(
            linux_path(&target, Path::new("/home/me/项目 A")).unwrap(),
            "/home/me/项目 A"
        );
        assert_eq!(
            linux_path(&target, Path::new(r"\\wsl.localhost\Ubuntu\home\me\Repo")).unwrap(),
            "/home/me/Repo"
        );
        assert_eq!(
            linux_path(&target, Path::new(r"\\wsl$\Ubuntu\home\me\repo")).unwrap(),
            "/home/me/repo"
        );
        assert!(linux_path(&target, Path::new(r"\\wsl.localhost\Debian\home\me")).is_err());
        assert!(linux_path(&target, Path::new("/home/../root")).is_err());
    }
}
