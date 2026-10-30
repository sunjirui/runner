use super::*;
use crate::router::{DeliveryReservation, Router, RouterUiNotifier};
use crate::session::state::agent::AgentEvent;

struct NoUi;
impl RouterUiNotifier for NoUi {
    fn delivery_blocked(&self, _: &crate::router::DeliveryBlockedEvent) {}
}

struct StartupFixture {
    manager: Arc<SessionManager>,
    fake: Arc<FakeRuntime>,
    router: Arc<Router>,
    log: Arc<EventLog>,
    _dir: tempfile::TempDir,
    dispatcher: Mutex<crate::router::dispatch::Dispatcher>,
    now: Mutex<Instant>,
}

impl StartupFixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let log = Arc::new(EventLog::open(dir.path()).unwrap());
        let fake = fake_runtime();
        let manager = mgr_with_fake(None, fake.clone());
        let role = role("fake", &[]);
        let router = Router::new(
            "mission".into(),
            None,
            String::new(),
            &[crate::model::SlotWithRole {
                slot: slot_for(&role),
                role,
            }],
            vec![],
            None,
            log.clone(),
            manager.clone(),
            Arc::new(NoUi),
        )
        .unwrap();
        router.register_pending_sessions(&[("tester".into(), "session".into())]);
        Self {
            manager,
            fake,
            router,
            log,
            _dir: dir,
            dispatcher: Mutex::new(crate::router::dispatch::Dispatcher::new(Instant::now())),
            now: Mutex::new(Instant::now()),
        }
    }

    fn queue(&self, text: &str) {
        let event = self
            .log
            .append(EventDraft::message(
                None,
                "mission",
                "human",
                Some("tester".into()),
                text,
            ))
            .unwrap();
        let offset = self.log.read_from(0).unwrap().last().unwrap().next_offset;
        self.router.handle_event(&event, offset);
        let count = self
            .log
            .read_from(0)
            .unwrap()
            .iter()
            .filter(|e| e.event.kind == runner_core::model::EventKind::Message)
            .count();
        crate::event_bus::BusEmitter::inbox_updated(
            &crate::router::RouterSubscriber(self.router.clone()),
            &crate::event_bus::InboxUpdate {
                mission_id: "mission".into(),
                role_handle: "tester".into(),
                last_id: Some(event.id),
                watermark: None,
                unread_count: count,
            },
        );
        crate::event_bus::BusEmitter::caught_up(&crate::router::RouterSubscriber(
            self.router.clone(),
        ));
        self.drive();
    }

    fn launch(&self, runtime: &str, pending_turn: bool, now: Instant) -> OutputStream {
        let (runtime_session, output) = self
            .fake
            .spawn(SpawnSpec {
                execution_target: Default::default(),
                session_id: "session".into(),
                ..Default::default()
            })
            .unwrap();
        self.manager.install_handle_with_size_persistence(
            "session",
            SessionHandle {
                startup: Some(super::super::startup::StartupDelivery::new(
                    runtime,
                    pending_turn,
                    now,
                )),
                #[cfg(windows)]
                pending_first_turn: None,
                id: "session".into(),
                mission_id: Some("mission".into()),
                role_id: None,
                runtime_session,
                codex_capture: None,
                forwarder: None,
                stop: output.stop_flag(),
            },
            Some(ForwarderEmitCtx {
                crew_id: None,
                mission_id: "mission".into(),
                handle: "tester".into(),
                event_log: self.log.clone(),
            }),
            Some(DEFAULT_PTY_SIZE),
            |_, _| true,
            &Capture::default(),
        );
        output
    }

    fn event(&self, runtime: Runtime, event: AgentEvent) {
        self.manager.publish_agent_event(
            "session",
            AgentEvent::Batch {
                runtime,
                events: vec![event],
            },
            &Capture::default(),
        );
    }

    fn drive(&self) {
        self.router.dispatch_at(
            &mut self.dispatcher.lock().unwrap(),
            *self.now.lock().unwrap(),
        );
    }

    fn held(&self) {
        self.drive();
        assert_eq!(
            self.manager.reserve_delivery("session").unwrap(),
            DeliveryReservation::Starting
        );
        assert!(self.fake.inputs.lock().unwrap().is_empty());
    }

    fn submitted(&self) {
        self.drive();
        *self.now.lock().unwrap() += Duration::from_secs(2);
        self.drive();
        *self.now.lock().unwrap() += Duration::from_secs(2);
        self.drive();
        assert!(self
            .fake
            .inputs
            .lock()
            .unwrap()
            .iter()
            .any(|input| matches!(input, FakeInput::Key { key, .. } if key == "Enter")));
    }
}

impl Drop for StartupFixture {
    fn drop(&mut self) {
        self.manager.kill("session").unwrap();
    }
}

#[test]
fn router_holds_startup_deliveries_until_hook_turn_completion_and_resets_on_resume() {
    for runtime in [
        Runtime::ClaudeCode,
        Runtime::Codex,
        Runtime::Copilot,
        Runtime::Pi,
        Runtime::Antigravity,
    ] {
        let fixture = StartupFixture::new();
        fixture.queue("join broadcast before process launch");
        fixture.queue("task before process launch");
        let now = Instant::now();
        let output = fixture.launch(runtime.key(), true, now);
        fixture.held();
        fixture.event(runtime, AgentEvent::StartupReady);
        fixture.held();
        fixture.manager.startup_output(
            "session",
            &output.stop_flag(),
            b"Starting MCP servers (6/7)",
            now,
        );
        fixture.manager.release_settled_startup(
            "session",
            &output.stop_flag(),
            now + Duration::from_secs(60),
        );
        fixture.held();
        fixture.event(runtime, AgentEvent::TurnStarted);
        fixture.manager.release_settled_startup(
            "session",
            &output.stop_flag(),
            now + Duration::from_secs(3600),
        );
        fixture.held();
        fixture.event(
            runtime,
            AgentEvent::TurnEnded {
                outcome: TurnOutcome::Completed,
            },
        );
        fixture.submitted();
        assert!(fixture
            .fake
            .bytes_writes()
            .iter()
            .any(|(_, bytes)| String::from_utf8_lossy(bytes).contains("2 new messages")));
        fixture.manager.kill("session").unwrap();
        fixture.fake.inputs.lock().unwrap().clear();
        fixture.queue("task before resume");
        let resume = fixture.launch(runtime.key(), false, now);
        fixture.held();
        if runtime == Runtime::Antigravity {
            fixture
                .manager
                .startup_output("session", &resume.stop_flag(), b"resumed prompt", now);
            fixture.manager.release_settled_startup(
                "session",
                &resume.stop_flag(),
                now + Duration::from_secs(10),
            );
        } else {
            fixture.event(runtime, AgentEvent::StartupReady);
        }
        fixture.submitted();
    }
}

#[test]
fn router_output_fallback_requires_output_startup_grace_and_idle_including_bridge_failure() {
    for runtime in [
        Runtime::OpenCode,
        Runtime::Cursor,
        Runtime::Grok,
        Runtime::Trae,
        Runtime::Shell,
        Runtime::Codex,
    ] {
        let fixture = StartupFixture::new();
        fixture.queue("task");
        let now = Instant::now();
        let output = fixture.launch(runtime.key(), true, now);
        let launch = output.stop_flag();
        if runtime == Runtime::Codex {
            fixture
                .manager
                .status_bridge_failed("session", &Capture::default());
        }
        fixture
            .manager
            .release_settled_startup("session", &launch, now + Duration::from_secs(60));
        fixture.held();
        fixture
            .manager
            .startup_output("session", &launch, b"startup screen", now);
        fixture
            .manager
            .release_settled_startup("session", &launch, now + Duration::from_secs(2));
        fixture.held();
        fixture.manager.startup_output(
            "session",
            &launch,
            b"first answer",
            now + Duration::from_secs(11),
        );
        fixture
            .manager
            .release_settled_startup("session", &launch, now + Duration::from_secs(12));
        fixture.held();
        fixture
            .manager
            .release_settled_startup("session", &launch, now + Duration::from_secs(13));
        fixture.submitted();
    }
}

#[test]
fn codex_conversation_recovery_in_first_turn_falls_back_to_output_release() {
    let fixture = StartupFixture::new();
    fixture.queue("answer after recovery");
    let now = Instant::now();
    let output = fixture.launch("codex", true, now);
    let launch = output.stop_flag();
    fixture.event(Runtime::Codex, AgentEvent::TurnStarted);
    fixture
        .manager
        .startup_output("session", &launch, b"first turn", now);
    fixture.event(Runtime::Codex, AgentEvent::ConversationRecovered);
    fixture
        .manager
        .release_settled_startup("session", &launch, now + Duration::from_secs(1));
    fixture.held();
    fixture
        .manager
        .release_settled_startup("session", &launch, now + Duration::from_secs(12));
    fixture.submitted();
}

#[test]
fn codex_interrupted_first_turn_releases_the_startup_hold_once_the_abort_settles() {
    let fixture = StartupFixture::new();
    fixture.queue("task after interrupt");
    let now = Instant::now();
    let _output = fixture.launch("codex", true, now);
    fixture.event(Runtime::Codex, AgentEvent::TurnStarted);
    fixture.event(
        Runtime::Codex,
        AgentEvent::TurnEnded {
            outcome: crate::session::status::TurnOutcome::Interrupted,
        },
    );
    fixture.held();
    fixture.manager.publish_agent_event(
        "session",
        AgentEvent::Transcript {
            runtime: Runtime::Codex,
            events: vec![AgentEvent::AbortSettled],
        },
        &Capture::default(),
    );
    fixture.submitted();
}

#[test]
fn codex_failed_first_turn_releases_the_startup_hold() {
    let fixture = StartupFixture::new();
    fixture.queue("task after failure");
    let now = Instant::now();
    let _output = fixture.launch("codex", true, now);
    fixture.event(Runtime::Codex, AgentEvent::TurnStarted);
    fixture.held();
    fixture.manager.publish_agent_event(
        "session",
        AgentEvent::Transcript {
            runtime: Runtime::Codex,
            events: vec![AgentEvent::TurnEnded {
                outcome: crate::session::status::TurnOutcome::Failed,
            }],
        },
        &Capture::default(),
    );
    fixture.submitted();
}

#[test]
fn startup_exit_preserves_the_backlog_and_delivers_after_resume() {
    let fixture = StartupFixture::new();
    fixture.queue("task before exit");
    let now = Instant::now();
    let _output = fixture.launch("codex", true, now);
    fixture.held();
    fixture.manager.kill("session").unwrap();
    fixture.drive();
    assert!(!fixture.log.read_from(0).unwrap().iter().any(|entry| entry
        .event
        .payload
        .to_string()
        .contains("dropped")));
    let _resume = fixture.launch("codex", false, now);
    fixture.event(Runtime::Codex, AgentEvent::StartupReady);
    fixture.submitted();
}

#[test]
fn completed_startup_keeps_a_human_draft_held_until_input_clears() {
    let fixture = StartupFixture::new();
    fixture.queue("task");
    let now = Instant::now();
    let _output = fixture.launch("codex", true, now);
    fixture.manager.report_input_state(
        "session",
        InputObservation {
            composing: false,
            state: InputState::Drafting,
            since: now,
            composer_visible: true,
        },
    );
    fixture.event(Runtime::Codex, AgentEvent::TurnStarted);
    fixture.event(
        Runtime::Codex,
        AgentEvent::TurnEnded {
            outcome: TurnOutcome::Completed,
        },
    );
    assert_eq!(
        fixture.manager.reserve_delivery("session").unwrap(),
        DeliveryReservation::Drafting {
            composer_visible: true
        }
    );
    assert!(fixture.fake.inputs.lock().unwrap().is_empty());
    fixture.manager.report_input_state(
        "session",
        InputObservation {
            composing: false,
            state: InputState::Idle,
            since: now,
            composer_visible: true,
        },
    );
    fixture.submitted();
}

#[test]
fn readiness_is_owned_by_the_session_manager_before_and_after_attachment() {
    use crate::router::DeliveryReadiness as R;
    let fixture = StartupFixture::new();
    assert_eq!(
        fixture.manager.delivery_readiness("session"),
        R::Unavailable
    );
    fixture
        .manager
        .session_state_or_insert("session")
        .lock()
        .unwrap()
        .pending_spawn = true;
    assert_eq!(fixture.manager.delivery_readiness("session"), R::Pending);
    let _output = fixture.launch("codex", false, Instant::now());
    assert_eq!(fixture.manager.delivery_readiness("session"), R::Starting);
    fixture.event(Runtime::Codex, AgentEvent::StartupReady);
    assert!(matches!(
        fixture.manager.delivery_readiness("session"),
        R::Ready { .. }
    ));
    let token = match fixture.manager.reserve_delivery("session").unwrap() {
        DeliveryReservation::Ready(token) => token,
        other => panic!("{other:?}"),
    };
    assert_eq!(fixture.manager.delivery_readiness("session"), R::InFlight);
    fixture.manager.finish_delivery("session", token);
    fixture.manager.kill("session").unwrap();
    assert_eq!(
        fixture.manager.delivery_readiness("session"),
        R::Unavailable
    );
}

#[test]
fn a_synthetic_busy_publication_cannot_confirm_an_attempt() {
    let fixture = StartupFixture::new();
    let _output = fixture.launch("codex", false, Instant::now());
    fixture.event(Runtime::Codex, AgentEvent::StartupReady);
    let before = fixture.manager.delivery_snapshot("session").unwrap();
    fixture
        .manager
        .synthesize_wake_busy(
            "session",
            EventDraft::signal(
                None,
                "mission",
                "tester",
                SignalType::new("session_status"),
                serde_json::json!({"state":"busy"}),
            ),
        )
        .unwrap();
    assert_eq!(
        fixture
            .manager
            .delivery_snapshot("session")
            .unwrap()
            .turn_started,
        before.turn_started
    );
    fixture.event(Runtime::Codex, AgentEvent::TurnStarted);
    assert_ne!(
        fixture
            .manager
            .delivery_snapshot("session")
            .unwrap()
            .turn_started,
        before.turn_started
    );
}

#[test]
fn approval_response_and_release_are_readiness_events_not_composer_clear_evidence() {
    #[derive(Default)]
    struct Events(Mutex<Vec<crate::router::SessionDeliveryEvent>>);
    impl crate::router::SessionDeliveryListener for Events {
        fn session_delivery_event(&self, _: &str, event: crate::router::SessionDeliveryEvent) {
            self.0.lock().unwrap().push(event);
        }
    }
    let fixture = StartupFixture::new();
    let _output = fixture.launch("claude-code", false, Instant::now());
    fixture.event(Runtime::ClaudeCode, AgentEvent::StartupReady);
    let events = Arc::new(Events::default());
    let listener: Arc<dyn crate::router::SessionDeliveryListener> = events.clone();
    fixture
        .manager
        .register_delivery_listener("session", Arc::downgrade(&listener));
    fixture.event(
        Runtime::ClaudeCode,
        AgentEvent::InteractionOpened {
            reason: crate::session::status::WaitReason::Approval,
            owners: vec!["approval".into()],
        },
    );
    fixture
        .manager
        .inject_direct_stdin("session", b"\r", &Capture::default())
        .unwrap();
    fixture.event(
        Runtime::ClaudeCode,
        AgentEvent::InteractionClosed {
            owner: "approval".into(),
            reason: None,
        },
    );
    for bytes in [b"\r".as_slice(), b"\x03".as_slice()] {
        fixture
            .manager
            .inject_direct_stdin("session", bytes, &Capture::default())
            .unwrap();
    }
    let observed = events.0.lock().unwrap().clone();
    assert!(observed.contains(&crate::router::SessionDeliveryEvent::ReadinessChanged));
    assert!(!observed.contains(&crate::router::SessionDeliveryEvent::InputCleared));
    for state in [InputState::Drafting, InputState::Idle] {
        fixture.manager.report_input_state(
            "session",
            InputObservation {
                state,
                since: Instant::now(),
                composing: false,
                composer_visible: true,
            },
        );
    }
    assert!(events
        .0
        .lock()
        .unwrap()
        .contains(&crate::router::SessionDeliveryEvent::InputCleared));
}

#[test]
fn resized_unsubmitted_input_waits_for_composer_output_after_a_successful_local_enter() {
    use crate::router::DeliveryReadiness;
    use runner_terminal::input_state::InputEvent;
    use runner_terminal::mappings::InputKind;

    let fixture = StartupFixture::new();
    let _output = fixture.launch("claude-code", false, Instant::now());
    fixture.event(Runtime::ClaudeCode, AgentEvent::StartupReady);
    let pool = db::open_pool(&fixture._dir.path().join("terminal.db")).unwrap();
    let events: Arc<dyn SessionEvents> = Arc::new(Capture::default());
    let terminal = fixture
        .manager
        .create_terminal("session", 80, 4, &pool, &events)
        .unwrap();
    fixture
        .manager
        .session_state("session")
        .unwrap()
        .lock()
        .unwrap()
        .terminal = Some(terminal.clone());
    let emit = |bytes: &[u8]| {
        fixture
            .manager
            .ingest_output_chunk("session", Some("mission"), bytes, events.as_ref());
    };
    let advance = |duration| {
        *fixture.now.lock().unwrap() += duration;
        let now = *fixture.now.lock().unwrap();
        crate::session::clock::at(now, 0, || fixture.drive());
    };
    emit(b"> ");
    fixture.drive();
    let revision = terminal.input_reset_guard();
    fixture.queue("first");
    let body = match &fixture.fake.inputs.lock().unwrap()[0] {
        FakeInput::Bytes { bytes, .. } => bytes.clone(),
        other => panic!("{other:?}"),
    };
    emit(&body);
    assert_eq!(terminal.input_reset_guard(), revision);
    advance(Duration::from_millis(300));
    advance(crate::router::dispatch::CONFIRM_WINDOW);
    advance(crate::router::dispatch::CONFIRM_WINDOW);
    let initial_inputs = fixture.fake.inputs.lock().unwrap().clone();
    assert_eq!(initial_inputs.len(), 3, "{initial_inputs:?}");
    terminal.resize(80, 5);
    terminal.observe_input(&InputEvent::Key {
        kind: InputKind::Submit,
    });
    let now = *fixture.now.lock().unwrap();
    crate::session::clock::at(now, 0, || {
        fixture
            .manager
            .inject_direct_stdin("session", b"\r", events.as_ref())
            .unwrap();
        fixture.queue("second");
    });
    advance(Duration::from_secs(120));
    let now = *fixture.now.lock().unwrap();
    assert!(matches!(
        crate::session::clock::at(now, 0, || fixture.manager.delivery_readiness("session")),
        DeliveryReadiness::Ready { .. }
    ));
    assert_eq!(fixture.fake.inputs.lock().unwrap().len(), 4);
    assert!(terminal
        .input_rows()
        .iter()
        .any(|row| row.contains("[inbox]")));
    emit(b"\x1b[2J\x1b[H> ");
    crate::session::clock::at(now, 0, || fixture.drive());
    advance(Duration::from_millis(500));
    let inputs = fixture.fake.inputs.lock().unwrap();
    assert_eq!(inputs.len(), 5);
    assert!(matches!(&inputs[4], FakeInput::Bytes { bytes, .. }
        if String::from_utf8_lossy(bytes).contains("2 new messages")));
    drop(inputs);
    fixture
        .manager
        .session_state("session")
        .unwrap()
        .lock()
        .unwrap()
        .terminal = None;
    let workers = terminal.take_workers();
    drop(terminal);
    for worker in workers {
        worker.join().unwrap();
    }
}

#[test]
fn generated_submissions_accept_rotating_placeholders_and_deliver_later_backlogs() {
    for second_enter in [false, true] {
        let fixture = StartupFixture::new();
        let _output = fixture.launch("claude-code", false, Instant::now());
        fixture.event(Runtime::ClaudeCode, AgentEvent::StartupReady);
        let pool = db::open_pool(&fixture._dir.path().join("terminal.db")).unwrap();
        let events: Arc<dyn SessionEvents> = Arc::new(Capture::default());
        let terminal = fixture
            .manager
            .create_terminal("session", 80, 4, &pool, &events)
            .unwrap();
        fixture
            .manager
            .session_state("session")
            .unwrap()
            .lock()
            .unwrap()
            .terminal = Some(terminal.clone());
        let emit = |bytes: &[u8]| {
            fixture
                .manager
                .ingest_output_chunk("session", Some("mission"), bytes, events.as_ref());
        };
        let advance = |duration| {
            *fixture.now.lock().unwrap() += duration;
            let now = *fixture.now.lock().unwrap();
            crate::session::clock::at(now, 0, || fixture.drive());
        };
        emit("\x1b[2;1H❯ First placeholder".as_bytes());
        fixture.drive();
        let revision = terminal.input_reset_guard();
        fixture.queue("first");
        let body = match &fixture.fake.inputs.lock().unwrap()[0] {
            FakeInput::Bytes { bytes, .. } => String::from_utf8(bytes.clone()).unwrap(),
            other => panic!("{other:?}"),
        };
        emit(format!("\r\x1b[K❯ {body}").as_bytes());
        advance(Duration::from_millis(300));
        if second_enter {
            advance(crate::router::dispatch::CONFIRM_WINDOW);
        }
        emit("\r\x1b[K❯ Different placeholder".as_bytes());
        let now = *fixture.now.lock().unwrap();
        crate::session::clock::at(now, 0, || fixture.drive());
        advance(crate::router::dispatch::CONFIRM_WINDOW);
        let now = *fixture.now.lock().unwrap();
        assert!(matches!(
            crate::session::clock::at(now, 0, || fixture.manager.delivery_readiness("session")),
            crate::router::DeliveryReadiness::Ready { .. }
        ));
        assert_eq!(terminal.input_reset_guard(), revision);
        crate::session::clock::at(now, 0, || fixture.queue("second"));
        advance(Duration::from_millis(300));
        let inputs = fixture.fake.inputs.lock().unwrap().clone();
        assert!(
            matches!(inputs.last(),
            Some(FakeInput::Bytes { bytes, .. }) if String::from_utf8_lossy(bytes).contains("2 new messages")),
            "second_enter={second_enter} inputs={inputs:?}"
        );
        fixture
            .manager
            .session_state("session")
            .unwrap()
            .lock()
            .unwrap()
            .terminal = None;
        let workers = terminal.take_workers();
        drop(terminal);
        for worker in workers {
            worker.join().unwrap();
        }
    }
}

#[test]
fn generated_submit_cannot_publish_over_a_newer_composition_observation() {
    use runner_terminal::delivery_probe::InputProbe;
    use runner_terminal::input_state::InputEvent;
    use runner_terminal::terminal::{HostResult, ModelOptions, TerminalHost, TerminalModel};
    use std::sync::{mpsc, Condvar, Weak};

    struct Host {
        manager: Weak<SessionManager>,
        start: Mutex<Option<mpsc::Sender<()>>>,
        composed: (Mutex<bool>, Condvar),
    }
    impl TerminalHost for Host {
        fn write_input(&self, _: &str, _: &[u8]) -> HostResult<()> {
            Ok(())
        }
        fn write_reply(&self, _: &str, _: &[u8]) -> HostResult<()> {
            Ok(())
        }
        fn report_input_state(&self, id: &str, observation: InputObservation) {
            if observation.state == InputState::Submitted {
                if let Some(start) = self.start.lock().unwrap().take() {
                    start.send(()).unwrap();
                }
                let (composed, _) = self
                    .composed
                    .1
                    .wait_timeout_while(
                        self.composed.0.lock().unwrap(),
                        Duration::from_secs(2),
                        |composed| !*composed,
                    )
                    .unwrap();
                assert!(*composed);
            }
            self.manager
                .upgrade()
                .unwrap()
                .report_input_state(id, observation);
            if observation.composing {
                *self.composed.0.lock().unwrap() = true;
                self.composed.1.notify_all();
            }
        }
        fn set_live_title(&self, _: &str, _: &str) -> HostResult<()> {
            Ok(())
        }
        fn resize(&self, _: &str, _: u16, _: u16) -> HostResult<()> {
            Ok(())
        }
        fn is_local_host(&self, _: &str) -> bool {
            true
        }
    }

    for guarded in [false, true] {
        let fixture = StartupFixture::new();
        let _output = fixture.launch("claude-code", false, Instant::now());
        fixture.event(Runtime::ClaudeCode, AgentEvent::StartupReady);
        let (start, go) = mpsc::channel();
        let (ready, attempted) = mpsc::channel();
        let host = Arc::new(Host {
            manager: Arc::downgrade(&fixture.manager),
            start: Mutex::new(Some(start)),
            composed: (Mutex::new(false), Condvar::new()),
        });
        let terminal = TerminalModel::new(
            host.clone(),
            "session".into(),
            80,
            4,
            ModelOptions::default(),
        )
        .unwrap();
        terminal.publish_input(terminal.feed_output(1, b"> "));
        let text = "[inbox] task";
        let mut probe = InputProbe::new(terminal.input_rows(), text);
        terminal.track_delivery_input(text);
        terminal.publish_input(terminal.feed_output(2, text.as_bytes()));
        probe.locate(&terminal.input_rows());
        let observing = terminal.clone();
        let observer = std::thread::spawn(move || {
            go.recv_timeout(Duration::from_secs(2)).unwrap();
            ready.send(()).unwrap();
            observing.observe_input(&InputEvent::Composing { composing: true });
        });
        if guarded {
            assert!(terminal
                .with_unchanged_input(terminal.input_reset_guard(), &probe, || {
                    host.start.lock().unwrap().take().unwrap().send(()).unwrap();
                    attempted.recv_timeout(Duration::from_secs(2)).unwrap();
                })
                .is_some());
        } else {
            terminal.track_delivery_submit();
            if let Some(start) = host.start.lock().unwrap().take() {
                start.send(()).unwrap();
            }
        }
        observer.join().unwrap();
        crate::session::clock::at(Instant::now() + Duration::from_secs(120), 0, || {
            assert_eq!(
                fixture.manager.delivery_readiness("session"),
                crate::router::DeliveryReadiness::Drafting
            );
            assert!(matches!(
                fixture.manager.reserve_delivery("session").unwrap(),
                DeliveryReservation::Drafting { .. }
            ));
        });
        let workers = terminal.take_workers();
        drop(terminal);
        for worker in workers {
            worker.join().unwrap();
        }
    }
}
