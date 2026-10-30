#![cfg(unix)]

use std::process::Command;

#[test]
fn missing_wsl_bridge_fails_without_starting_a_linux_daemon() {
    let temp = tempfile::tempdir().unwrap();
    let bridge = temp.path().join("missing-windows-runner.exe");
    let output = Command::new(env!("CARGO_BIN_EXE_runner-agent-cli"))
        .args(["--json", "project", "list"])
        .env("RUNNER_WSL_BRIDGE", &bridge)
        .env(
            "RUNNER_WSL_TARGET",
            r#"{"kind":"wsl","distro":"Ubuntu","user":"tester"}"#,
        )
        .env("HOME", temp.path())
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!temp
        .path()
        .join(".local/share/com.wycstudios.runner-dev/runner.db")
        .exists());
    assert!(!temp
        .path()
        .join(".local/share/com.wycstudios.runner/runner.db")
        .exists());
    assert!(!bridge.exists());
}

#[cfg(feature = "wsl-client")]
#[test]
fn distributable_wsl_client_requires_the_managed_wrapper() {
    let temp = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_runner-agent-cli"))
        .args(["project", "list"])
        .env_remove("RUNNER_WSL_BRIDGE")
        .env_remove("RUNNER_WSL_TARGET")
        .env("HOME", temp.path())
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("managed runner wrapper"));
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
}
