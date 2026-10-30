#![cfg(target_os = "linux")]

use runner_daemon::session::runtime::SpawnSpec;
use runner_daemon::wsl_session::Identity;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn wait_for(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !path.exists() {
        assert!(Instant::now() < deadline, "missing {}", path.display());
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn start(dir: &Path, script: &str) -> std::process::Child {
    let spec = SpawnSpec {
        command: "/bin/sh".into(),
        args: vec!["-c".into(), script.into()],
        cwd: Some(dir.into()),
        initial_size: Some((80, 24)),
        ..Default::default()
    };
    std::fs::write(dir.join("spec.json"), serde_json::to_vec(&spec).unwrap()).unwrap();
    std::fs::write(dir.join("lease"), "alive").unwrap();
    Command::new(env!("CARGO_BIN_EXE_runner-agent-cli"))
        .arg("--wsl-session")
        .arg(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap()
}

#[test]
fn wsl_stop_checks_identity_and_cleans_tools_in_other_process_groups() {
    let dir = tempfile::tempdir().unwrap();
    let mut launcher = start(
        dir.path(),
        "setsid sh -c 'echo $$ > tool.pid; exec sleep 600' & wait",
    );
    wait_for(&dir.path().join("ready.json"));
    wait_for(&dir.path().join("tool.pid"));
    let mut identity: Identity =
        serde_json::from_slice(&std::fs::read(dir.path().join("ready.json")).unwrap()).unwrap();
    let started = identity.started.clone();
    identity.started = "wrong-generation".into();
    assert!(!runner_daemon::wsl_session::stop_linux(&identity).unwrap());
    assert!(launcher.try_wait().unwrap().is_none());
    identity.started = started;
    assert!(runner_daemon::wsl_session::stop_linux(&identity).unwrap());
    assert!(launcher.wait().unwrap().success());
    let pid = std::fs::read_to_string(dir.path().join("tool.pid")).unwrap();
    assert!(!Path::new(&format!("/proc/{}", pid.trim())).exists());
}

#[test]
fn wsl_missing_host_lease_stops_the_agent_and_preserves_normal_exit_codes() {
    let dir = tempfile::tempdir().unwrap();
    let mut launcher = start(dir.path(), "sleep 600");
    wait_for(&dir.path().join("ready.json"));
    std::fs::remove_file(dir.path().join("lease")).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if launcher.try_wait().unwrap().is_some() {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(20));
    }
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(
        start(dir.path(), "exit 17").wait().unwrap().code(),
        Some(17)
    );
}
