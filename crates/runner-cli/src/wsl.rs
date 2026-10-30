use std::io::{self, Read, Write};
use std::process::Stdio;

use serde::{Deserialize, Serialize};

use crate::env::MissionEnv;

#[derive(Serialize, Deserialize)]
pub enum LocalOperation {
    Post {
        text: String,
        to: Vec<String>,
    },
    Read {
        all: bool,
        since: Option<String>,
        from: Option<String>,
    },
    Signal {
        kind: String,
        payload: serde_json::Value,
    },
}

#[derive(Serialize, Deserialize)]
struct LocalRequest {
    mission: MissionEnv,
    operation: LocalOperation,
}

pub fn forward_local(mission: &MissionEnv, operation: LocalOperation) -> i32 {
    let result = (|| -> io::Result<i32> {
        let mut child = runner_core::wsl_bridge::command_for("--wsl-local")?
            .stdin(Stdio::piped())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()?;
        let input = serde_json::to_vec(&LocalRequest {
            mission: mission.clone(),
            operation,
        })?;
        let mut stdin = child.stdin.take().unwrap();
        stdin.write_all(&input)?;
        drop(stdin);
        Ok(child.wait()?.code().unwrap_or(1))
    })();
    match result {
        Ok(code) => code,
        Err(error) => {
            eprintln!("runner: WSL mission bridge: {error}");
            1
        }
    }
}

pub fn forward_daemon(action: &str, json: bool, quiet: bool) -> i32 {
    match runner_core::wsl_bridge::command_for("--wsl-daemon").and_then(|mut command| {
        command
            .args([
                action,
                if json {
                    "json"
                } else if quiet {
                    "quiet"
                } else {
                    "normal"
                },
            ])
            .status()
    }) {
        Ok(status) => status.code().unwrap_or(1),
        Err(error) => {
            eprintln!("runner: Windows daemon bridge: {error}");
            1
        }
    }
}

/// Hidden interop entry points are deliberately handled before ordinary CLI
/// parsing. They cannot select an arbitrary executable or start a Linux daemon.
pub fn entry(args: &[String]) -> Option<i32> {
    let mode = args.get(1)?;
    if !matches!(
        mode.as_str(),
        "--wsl-bridge" | "--wsl-hook" | "--wsl-local" | "--wsl-daemon"
    ) {
        return None;
    }
    Some(match bridge(mode, &args[2..]) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("runner: {error}");
            1
        }
    })
}

fn bridge(mode: &str, args: &[String]) -> Result<i32, Box<dyn std::error::Error>> {
    if !cfg!(windows) {
        return Err("WSL bridge entry points require Windows Runner".into());
    }
    let (args, control) = if mode == "--wsl-daemon" {
        if args.len() < 4 {
            return Err("invalid Windows daemon control".into());
        }
        (
            &args[..args.len() - 2],
            Some((&args[args.len() - 2], &args[args.len() - 1])),
        )
    } else {
        (args, None)
    };
    if !(2..=3).contains(&args.len())
        || args[0] != runner_core::wsl_bridge::VERSION
        || args[1] != runner_core::wsl_bridge::build_identity()
    {
        return Err(
            "WSL CLI and Windows Runner versions differ; refresh WSL integration in Windows Runner"
                .into(),
        );
    }
    let target = args
        .get(2)
        .map(|json| serde_json::from_str::<runner_core::ExecutionTarget>(json))
        .transpose()?;
    if let Some(target) = &target {
        target.validate().map_err(io::Error::other)?;
    }
    if let Some((action, format)) = control {
        use clap::Parser;
        if !matches!(action.as_str(), "status" | "stop") {
            return Err("invalid Windows daemon operation".into());
        }
        let mut args = vec!["runner"];
        match format.as_str() {
            "json" => args.push("--json"),
            "quiet" => args.push("--quiet"),
            "normal" => (),
            _ => return Err("invalid daemon output mode".into()),
        }
        args.extend(["daemon", action]);
        return Ok(crate::command::run(crate::command::Cli::try_parse_from(
            args,
        )?));
    }
    if mode == "--wsl-local" {
        let mut bytes = Vec::new();
        io::stdin()
            .take(8 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 8 * 1024 * 1024 {
            return Err("WSL mission request is too large".into());
        }
        let request: LocalRequest = serde_json::from_slice(&bytes)?;
        let data = runner_core::app_paths::app_data_dir(cfg!(debug_assertions))
            .ok_or("Runner data directory is unavailable")?;
        let expected = runner_core::event_log::mission_dir(
            &data,
            request.mission.crew_id.as_deref(),
            &request.mission.mission_id,
        )
        .join(runner_core::EVENTS_FILENAME);
        if request.mission.event_log != expected {
            return Err("WSL mission log does not belong to Windows Runner".into());
        }
        return Ok(match request.operation {
            LocalOperation::Post { text, to } => crate::msg::post(&request.mission, &text, &to),
            LocalOperation::Read { all, since, from } => {
                crate::msg::read(&request.mission, all, since.as_deref(), from.as_deref())
            }
            LocalOperation::Signal { kind, payload } => {
                let kind = runner_core::model::KnownSignalType::from_name(&kind)
                    .ok_or("unknown Runner signal")?;
                crate::signal::append(&request.mission, kind.as_str(), payload)
            }
        });
    }
    #[cfg(windows)]
    {
        if mode != "--wsl-hook" {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;
            let client = runtime
                .block_on(runner_cli::client::SocketClient::connect())
                .map_err(|error| io::Error::other(error.to_string()))?;
            drop(client);
            runtime.shutdown_background();
        }
        runner_core::protocol::socket::relay_stdio(
            &runner_cli::client::endpoint().ok_or("Runner endpoint is unavailable")?,
            target,
        )?;
    }
    #[cfg(not(windows))]
    let _ = target;
    Ok(0)
}
