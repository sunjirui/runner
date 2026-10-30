//! One WSL launcher owns one Linux process tree. Windows owns its lease and PTY.

use crate::error::{Error, Result};
use crate::session::runtime::SpawnSpec;
use runner_core::ExecutionTarget;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Identity {
    pub pid: u32,
    pub started: String,
    pub boot_id: String,
    pub cwd: String,
}

pub struct HostSession {
    target: ExecutionTarget,
    dir: PathBuf,
    identity: Option<Identity>,
    heartbeat: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl HostSession {
    pub fn prepare(spec: &SpawnSpec, app_data: &Path) -> Result<(Self, Vec<String>)> {
        crate::wsl::require_windows_target(&spec.execution_target)?;
        let dir = app_data
            .join("wsl/launches")
            .join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join("spec.json"), serde_json::to_vec(spec)?)?;
        std::fs::write(dir.join("lease"), b"alive")?;
        let linux_dir = crate::wsl::linux_path(&spec.execution_target, &dir)?;
        let mut args = spec
            .execution_target
            .wsl_args(spec.cwd.as_deref().and_then(Path::to_str))
            .map_err(Error::msg)?;
        args.extend([
            "/bin/sh".into(),
            "-c".into(),
            r#"exec "$HOME/$1/bin/runner" --wsl-session "$2""#.into(),
            "runner".into(),
            crate::wsl::integration_dir(),
            linux_dir,
        ]);
        Ok((
            Self {
                target: spec.execution_target.clone(),
                dir,
                identity: None,
                heartbeat: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            },
            args,
        ))
    }

    pub fn acknowledge(&mut self) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            match std::fs::read(self.dir.join("ready.json")) {
                Ok(bytes) => {
                    self.identity = Some(serde_json::from_slice(&bytes)?);
                    break;
                }
                Err(error)
                    if error.kind() == std::io::ErrorKind::NotFound
                        && Instant::now() < deadline =>
                {
                    if let Ok(error) = std::fs::read_to_string(self.dir.join("error")) {
                        return Err(Error::msg(error));
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(error) => {
                    return Err(Error::msg(format!(
                        "WSL agent did not acknowledge startup: {error}"
                    )))
                }
            }
        }
        let lease = self.dir.join("lease");
        let stop = self.heartbeat.clone();
        std::thread::spawn(move || {
            while !stop.load(std::sync::atomic::Ordering::Acquire) {
                if std::fs::write(&lease, b"alive").is_err() {
                    break;
                }
                if stop.load(std::sync::atomic::Ordering::Acquire) {
                    let _ = std::fs::remove_file(&lease);
                    break;
                }
                std::thread::sleep(Duration::from_secs(2));
            }
        });
        Ok(())
    }

    pub fn launch_dir(&self) -> &Path {
        &self.dir
    }

    pub fn finished(&self) {
        self.heartbeat
            .store(true, std::sync::atomic::Ordering::Release);
        let _ = std::fs::remove_file(self.dir.join("lease"));
    }

    pub fn stop(&self) -> Result<()> {
        if let Some(identity) = &self.identity {
            let _: bool = crate::wsl::helper(
                &self.target,
                &crate::wsl::HelperRequest::Stop {
                    identity: identity.clone(),
                },
            )?;
        }
        self.heartbeat
            .store(true, std::sync::atomic::Ordering::Release);
        let _ = std::fs::remove_file(self.dir.join("lease"));
        Ok(())
    }
}

impl Drop for HostSession {
    fn drop(&mut self) {
        self.heartbeat
            .store(true, std::sync::atomic::Ordering::Release);
        // A missing lease also cleans Linux children if startup acknowledgement failed.
        let _ = std::fs::remove_file(self.dir.join("lease"));
        for file in [
            "spec.json",
            "ready.json",
            "ready.tmp",
            "error",
            "conversation.json",
            "conversation.tmp",
        ] {
            let _ = std::fs::remove_file(self.dir.join(file));
        }
        let _ = std::fs::remove_dir(&self.dir);
    }
}

#[cfg(target_os = "linux")]
fn process_start(pid: u32) -> Option<String> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let fields = stat
        .rsplit_once(')')?
        .1
        .split_whitespace()
        .collect::<Vec<_>>();
    if fields.first() == Some(&"Z") {
        return None;
    }
    fields.get(19).map(|value| value.to_string())
}

#[cfg(target_os = "linux")]
pub fn stop_linux(identity: &Identity) -> Result<bool> {
    if std::fs::read_to_string("/proc/sys/kernel/random/boot_id")?.trim() != identity.boot_id
        || process_start(identity.pid).as_deref() != Some(&identity.started)
    {
        return Ok(false);
    }
    // The launcher is a subreaper: tools that fork into a new session remain
    // attributable to it when their immediate parent exits.
    unsafe {
        libc::kill(identity.pid as i32, libc::SIGUSR1);
    }
    let deadline = Instant::now() + Duration::from_secs(8);
    while process_start(identity.pid).as_deref() == Some(&identity.started) {
        if Instant::now() >= deadline {
            return Err(Error::msg(
                "WSL launcher did not finish stopping its Linux children",
            ));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    Ok(true)
}

#[cfg(not(target_os = "linux"))]
pub fn stop_linux(_identity: &Identity) -> Result<bool> {
    Err(Error::msg(
        "Linux process cleanup requires the Linux helper",
    ))
}

#[cfg(target_os = "linux")]
static STOP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[cfg(target_os = "linux")]
extern "C" fn request_stop(_: libc::c_int) {
    STOP.store(true, std::sync::atomic::Ordering::Release);
}

#[cfg(target_os = "linux")]
fn descendants(root: u32) -> Vec<(u32, String)> {
    let mut rows = Vec::new();
    if let Ok(entries) = std::fs::read_dir("/proc") {
        for entry in entries.flatten() {
            let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
                continue;
            };
            let Ok(stat) = std::fs::read_to_string(entry.path().join("stat")) else {
                continue;
            };
            let Some((_, fields)) = stat.rsplit_once(')') else {
                continue;
            };
            let fields = fields.split_whitespace().collect::<Vec<_>>();
            if let (Some(ppid), Some(started)) = (
                fields.get(1).and_then(|s| s.parse::<u32>().ok()),
                fields.get(19),
            ) {
                rows.push((pid, ppid, started.to_string()));
            }
        }
    }
    let mut found = vec![root];
    loop {
        let additional = rows
            .iter()
            .filter(|(pid, ppid, _)| found.contains(ppid) && !found.contains(pid))
            .map(|(pid, _, _)| *pid)
            .collect::<Vec<_>>();
        if additional.is_empty() {
            break;
        }
        found.extend(additional);
    }
    rows.into_iter()
        .filter(|(pid, _, _)| *pid != root && found.contains(pid))
        .map(|(pid, _, start)| (pid, start))
        .collect()
}

#[cfg(target_os = "linux")]
struct LinuxChildren;

#[cfg(target_os = "linux")]
impl Drop for LinuxChildren {
    fn drop(&mut self) {
        let grace = Instant::now() + Duration::from_secs(6);
        let mut signaled = std::collections::HashSet::new();
        loop {
            let children = descendants(std::process::id());
            if children.is_empty() {
                break;
            }
            let killing = Instant::now() >= grace;
            for (pid, started) in children {
                if process_start(pid).as_deref() == Some(&started)
                    && (killing || signaled.insert((pid, started)))
                {
                    unsafe {
                        libc::kill(
                            pid as i32,
                            if killing { libc::SIGKILL } else { libc::SIGHUP },
                        );
                    }
                }
            }
            while unsafe { libc::waitpid(-1, std::ptr::null_mut(), libc::WNOHANG) } > 0 {}
            if Instant::now() > grace + Duration::from_secs(1) {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

#[cfg(target_os = "linux")]
pub fn run_linux(dir: &Path) -> Result<i32> {
    use std::os::unix::process::CommandExt;
    let result = (|| -> Result<i32> {
        let spec: SpawnSpec = serde_json::from_slice(&std::fs::read(dir.join("spec.json"))?)?;
        unsafe {
            if libc::prctl(libc::PR_SET_CHILD_SUBREAPER, 1, 0, 0, 0) != 0 {
                return Err(std::io::Error::last_os_error().into());
            }
            for signal in [libc::SIGUSR1, libc::SIGHUP, libc::SIGTERM, libc::SIGINT] {
                libc::signal(signal, request_stop as *const () as usize);
            }
            libc::signal(libc::SIGTTOU, libc::SIG_IGN);
        }
        let path = crate::session::launch::compose_path(
            spec.shim_dir.as_deref(),
            spec.bundled_bin_dir.as_deref(),
            spec.shell_path.as_deref(),
            runner_core::app_paths::home_dir().as_deref(),
            std::env::var("PATH").ok().as_deref(),
        );
        let mut command = std::process::Command::new(&spec.command);
        command.args(&spec.args).envs(&spec.env).env("PATH", path);
        if let Some(cwd) = &spec.cwd {
            command.current_dir(cwd);
        }
        if let Some((cols, rows)) = spec.initial_size {
            command
                .env("COLUMNS", cols.to_string())
                .env("LINES", rows.to_string());
        }
        unsafe {
            command.pre_exec(|| {
                if libc::setpgid(0, 0) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::isatty(libc::STDIN_FILENO) == 1
                    && libc::tcsetpgrp(libc::STDIN_FILENO, libc::getpid()) != 0
                {
                    return Err(std::io::Error::last_os_error());
                }
                libc::signal(libc::SIGTTOU, libc::SIG_DFL);
                Ok(())
            });
        }
        let mut child = command.spawn()?;
        let cleanup = LinuxChildren;
        let identity = Identity {
            boot_id: std::fs::read_to_string("/proc/sys/kernel/random/boot_id")?
                .trim()
                .into(),
            pid: std::process::id(),
            started: process_start(std::process::id())
                .ok_or_else(|| Error::msg("Linux process identity is unavailable"))?,
            cwd: std::env::current_dir()?.to_string_lossy().into_owned(),
        };
        std::fs::write(dir.join("ready.tmp"), serde_json::to_vec(&identity)?)?;
        std::fs::rename(dir.join("ready.tmp"), dir.join("ready.json"))?;
        let mut cursor = (spec.agent_runtime == Some(crate::model::Runtime::Cursor))
            .then(|| crate::runtimes::cursor_agent::conversation::Watcher::new(&spec))
            .flatten();
        let mut last_cursor_scan = Instant::now() - Duration::from_secs(1);
        let code = loop {
            if last_cursor_scan.elapsed() >= Duration::from_millis(500) {
                last_cursor_scan = Instant::now();
                if let Some(watcher) = &mut cursor {
                    if let Some(key) = watcher
                        .scan_processes(descendants(identity.pid).into_iter().map(|(pid, _)| pid))
                    {
                        let temp = dir.join("conversation.tmp");
                        if let Ok(bytes) = serde_json::to_vec(&key) {
                            if std::fs::write(&temp, bytes).is_ok() {
                                let _ = std::fs::rename(temp, dir.join("conversation.json"));
                            }
                        }
                    }
                }
            }
            if let Some(status) = child.try_wait()? {
                break status.code().unwrap_or(1);
            }
            let lease_alive = std::fs::metadata(dir.join("lease"))
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.elapsed().ok())
                .is_some_and(|age| age < Duration::from_secs(30));
            if STOP.load(std::sync::atomic::Ordering::Acquire) || !lease_alive {
                break 0;
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        drop(cleanup);
        let _ = child.wait();
        Ok(code)
    })();
    if let Err(error) = &result {
        let _ = std::fs::write(dir.join("error"), error.to_string());
    }
    result
}

#[cfg(not(target_os = "linux"))]
pub fn run_linux(_dir: &Path) -> Result<i32> {
    Err(Error::msg("WSL session launcher requires Linux"))
}
