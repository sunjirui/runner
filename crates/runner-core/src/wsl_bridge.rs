//! The integrated Linux CLI uses the native Windows daemon through WSL interop.
//! A missing bridge must never trigger a Linux daemon or a second database.

use std::io;
use std::path::PathBuf;

pub const EXECUTABLE_ENV: &str = "RUNNER_WSL_BRIDGE";
pub const TARGET_ENV: &str = "RUNNER_WSL_TARGET";
pub const VERSION: &str = "1";

pub fn executable() -> Option<PathBuf> {
    std::env::var_os(EXECUTABLE_ENV).map(PathBuf::from)
}

pub fn integrated() -> bool {
    std::env::var_os(EXECUTABLE_ENV).is_some()
}

pub fn build_identity() -> String {
    format!(
        "{}:{}:{}",
        env!("CARGO_PKG_VERSION"),
        option_env!("RUNNER_BUILD_SHA").unwrap_or("dev"),
        option_env!("RUNNER_BUILD_STAMP").unwrap_or("dev")
    )
}

pub fn command() -> io::Result<std::process::Command> {
    command_for("--wsl-bridge")
}

pub fn command_for(mode: &str) -> io::Result<std::process::Command> {
    let exe = executable().ok_or_else(|| io::Error::other("WSL integration is not configured"))?;
    if !exe.is_absolute() || !exe.is_file() {
        return Err(io::Error::other("Windows Runner bridge is unavailable; refresh this distribution's WSL integration in Windows Runner"));
    }
    let mut command = std::process::Command::new(exe);
    command.args([mode, VERSION, &build_identity()]);
    if let Ok(target) = std::env::var(TARGET_ENV) {
        command.arg(target);
    }
    command.env_remove(EXECUTABLE_ENV).env_remove(TARGET_ENV);
    Ok(command)
}

#[cfg(unix)]
pub fn open() -> io::Result<std::os::unix::net::UnixStream> {
    open_for("--wsl-bridge")
}

#[cfg(unix)]
pub fn open_for(mode: &str) -> io::Result<std::os::unix::net::UnixStream> {
    use std::os::fd::OwnedFd;
    use std::os::unix::net::UnixStream;
    use std::process::Stdio;
    let (client, child_socket) = UnixStream::pair()?;
    let mut child = command_for(mode)?
        .stdin(Stdio::from(OwnedFd::from(child_socket.try_clone()?)))
        .stdout(Stdio::from(OwnedFd::from(child_socket)))
        .stderr(Stdio::inherit())
        .spawn()?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(client)
}
