use super::*;
use crate::session::hook_feed::HookFeed;
use crate::session::state::agent::{AdapterFeedback, AgentEvent};
use crate::session::status::TurnOutcome;

const SOURCE: &str = include_str!("tui.mjs");
fn plugin_dir(app_data: &Path) -> PathBuf {
    app_data.join("opencode-hooks")
}
pub(super) fn install(app_data: &Path) -> crate::error::Result<()> {
    let directory = plugin_dir(app_data);
    std::fs::create_dir_all(&directory)?;
    write_config_atomically(
        &directory.join("package.json"),
        br#"{"name":"runner-opencode-status","type":"module","exports":{".":"./index.mjs","./tui":"./tui.mjs"}}"#,
    )?;
    write_config_atomically(&directory.join("index.mjs"), include_bytes!("model.mjs"))?;
    write_config_atomically(&directory.join("tui.mjs"), SOURCE.as_bytes())
}
pub(super) fn model_config(
    object: &mut serde_json::Map<String, Value>,
    app_data: &Path,
    model: &str,
    variant: &str,
) -> crate::error::Result<()> {
    let (provider, id) = model
        .split_once('/')
        .ok_or_else(|| crate::error::Error::msg("OpenCode model must use provider/model format"))?;
    install(app_data)?;
    let directory = plugin_dir(app_data).canonicalize()?;
    let plugins = object
        .entry("plugins")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .ok_or_else(|| crate::error::Error::msg("OpenCode plugins must be an array"))?;
    plugins.push(json!({
        "package": directory.to_string_lossy(),
        "options": {
            "model": {"providerID": provider, "id": id, "variant": variant},
        }
    }));
    Ok(())
}
pub(super) fn launch_config(
    app_data: &Path,
    env: &BTreeMap<String, String>,
) -> crate::error::Result<String> {
    install(app_data)?;
    let env = config::effective_env(env);
    let mut document = config::parse(
        env.get("OPENCODE_CLI_CONFIG_CONTENT")
            .map(String::as_str)
            .unwrap_or("{}"),
    )?;
    let object = document
        .as_object_mut()
        .ok_or_else(|| crate::error::Error::msg("OpenCode CLI config must be an object"))?;
    if !object.contains_key("plugins") {
        if let Some(directory) =
            config::config_dir(&env, runner_core::app_paths::home_dir().as_deref())
        {
            if let Ok(raw) = std::fs::read_to_string(directory.join("cli.json")) {
                let native = config::parse(&raw)?;
                if let Some(plugins) = native.get("plugins") {
                    object.insert("plugins".into(), plugins.clone());
                }
            }
        }
    }
    let plugins = object
        .entry("plugins")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .ok_or_else(|| crate::error::Error::msg("OpenCode CLI plugins must be an array"))?;
    let directory = plugin_dir(app_data).canonicalize()?;
    let entry = json!({"package":directory.to_string_lossy()});
    if !plugins.contains(&entry) {
        plugins.push(entry);
    }
    Ok(document.to_string())
}
pub(super) struct Watcher {
    feed: HookFeed,
    key: Option<String>,
    sequence: u64,
    epoch: u64,
    last_seen: std::time::Instant,
    connected: bool,
}
impl Watcher {
    pub(super) fn new(
        receiver: crate::session::hook_queue::HookReceiver,
        spec: &SpawnSpec,
    ) -> Self {
        let key = spec
            .args
            .windows(2)
            .find(|a| a[0] == "--session")
            .map(|a| a[1].clone());
        Self {
            feed: HookFeed::from_receiver(receiver),
            key,
            sequence: 0,
            epoch: 0,
            last_seen: std::time::Instant::now(),
            connected: false,
        }
    }
}
impl HookWatcher for Watcher {
    fn drain_events(
        &mut self,
        _cancel: u8,
        emit: &mut dyn FnMut(AgentEvent) -> AdapterFeedback,
        session_start: &mut dyn FnMut(String),
    ) -> crate::error::Result<()> {
        let key = &mut self.key;
        let sequence = &mut self.sequence;
        let epoch = &mut self.epoch;
        let last_seen = &mut self.last_seen;
        let connected = &mut self.connected;
        let result = self.feed.drain(|report| {
            let Some(current_epoch) = report["epoch"].as_u64().filter(|value| *value >= *epoch)
            else {
                return;
            };
            let Some(next) = report["sequence"]
                .as_u64()
                .filter(|next| current_epoch > *epoch || *next >= *sequence)
            else {
                return;
            };
            let Some(id) = report["session_id"]
                .as_str()
                .filter(|id| id.is_empty() || valid_key(id))
            else {
                return;
            };
            if report["child"] == true {
                return;
            }
            let event = report["hook_event_name"].as_str().unwrap_or_default();
            if event != "snapshot" {
                return;
            }
            *last_seen = std::time::Instant::now();
            *connected = true;
            if current_epoch == *epoch && next == *sequence {
                return;
            }
            if event == "snapshot" {
                let new = (!id.is_empty()).then(|| id.to_owned());
                if *key != new {
                    *key = new;
                    session_start(id.into());
                    emit(AgentEvent::Batch {
                        runtime: Runtime::OpenCode,
                        events: vec![AgentEvent::ConversationRecovered, AgentEvent::Ready],
                    });
                }
            } else {
                return;
            }
            *sequence = next;
            *epoch = current_epoch;
            if id.is_empty() {
                return;
            }
            let events = match report["state"].as_str().unwrap_or_default() {
                "working" => vec![AgentEvent::Working { detail: None }],
                "ready" => vec![AgentEvent::Ready],
                "idle" => match report["outcome"].as_str() {
                    Some("succeeded") => vec![AgentEvent::TurnEnded {
                        outcome: TurnOutcome::Completed,
                    }],
                    Some("failed") => vec![AgentEvent::TurnEnded {
                        outcome: TurnOutcome::Failed,
                    }],
                    Some("interrupted") => vec![AgentEvent::TurnEnded {
                        outcome: TurnOutcome::Interrupted,
                    }],
                    _ => Vec::new(),
                },
                _ => Vec::new(),
            };
            if !events.is_empty() {
                emit(AgentEvent::Batch {
                    runtime: Runtime::OpenCode,
                    events,
                });
            }
        });
        let expired =
            self.last_seen.elapsed() > Duration::from_secs(if self.connected { 8 } else { 60 });
        // Stop retires the route before killing the CLI; its last verified
        // conversation remains resumable, unlike a failed live bridge.
        if self.feed.is_retired() {
            return result;
        }
        if result.is_err() || expired {
            self.key = None;
            session_start(String::new());
            return Err(crate::error::Error::msg("OpenCode hook bridge unavailable"));
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::hook_queue::TestHookRoute;
    #[test]
    fn stopping_a_hook_route_preserves_the_last_verified_identity() {
        let (route, receiver) = TestHookRoute::new(Runtime::OpenCode, "current".into());
        let mut watcher = Watcher::new(
            receiver,
            &SpawnSpec {
                execution_target: Default::default(),
                args: strings(&["--session", "ses_launch"]),
                ..Default::default()
            },
        );
        drop(route.admit(json!({"hook_event_name":"snapshot","epoch":1,"sequence":1,"session_id":"ses_switched"})).unwrap());
        let mut keys = Vec::new();
        watcher
            .drain_events(0, &mut |_| Default::default(), &mut |key| keys.push(key))
            .unwrap();
        assert_eq!(keys, ["ses_switched"]);
        route.retire();
        assert!(watcher
            .drain_events(0, &mut |_| Default::default(), &mut |key| keys.push(key))
            .is_err());
        assert_eq!(keys, ["ses_switched"]);
    }

    #[test]
    fn opencode_heartbeats_do_not_repeat_outcomes_and_bridge_loss_clears_identity() {
        let (route, receiver) = TestHookRoute::new(Runtime::OpenCode, "current".into());
        let mut watcher = Watcher::new(
            receiver,
            &SpawnSpec {
                execution_target: Default::default(),
                args: strings(&["--session", "ses_owned"]),
                ..Default::default()
            },
        );
        let report = json!({"hook_event_name":"snapshot","epoch":1,"sequence":1,"session_id":"ses_owned","state":"idle","outcome":"succeeded"});
        let mut events = Vec::new();
        let mut keys = Vec::new();
        drop(route.admit(report.clone()).unwrap());
        watcher
            .drain_events(
                0,
                &mut |e| {
                    events.push(e);
                    Default::default()
                },
                &mut |k| keys.push(k),
            )
            .unwrap();
        let count = events.len();
        drop(route.admit(report).unwrap());
        watcher
            .drain_events(
                0,
                &mut |e| {
                    events.push(e);
                    Default::default()
                },
                &mut |k| keys.push(k),
            )
            .unwrap();
        assert_eq!(events.len(), count);
        watcher.last_seen = std::time::Instant::now() - Duration::from_secs(9);
        assert!(watcher
            .drain_events(0, &mut |_| Default::default(), &mut |k| keys.push(k))
            .is_err());
        assert_eq!(keys, [""]);
        route.retire();
    }
}
