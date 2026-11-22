//! Integration tests for [`TerminalService`] running against a real isolated `herdr-core` (`hc`) daemon.
//!
//! Fixture layout:
//! ```text
//! <tempdir>/
//! ├── workspace-a/
//! └── workspace-b/
//! Isolated hc daemon: HC_SESSION=lgtm-test-<uuid> (stopped automatically on Drop)
//! ```

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use lgtm_terminal::{
    LgtmTerminalError, TerminalAgentStatus, TerminalHostEvent, TerminalService, TerminalStreamEvent,
};
use tempfile::TempDir;
use uuid::Uuid;

// fixtures --------------------------------------------------------------------

struct IsolatedHcHarness {
    _dir: TempDir,
    workspace_a: std::path::PathBuf,
    workspace_b: std::path::PathBuf,
    service: TerminalService,
    host_events: Arc<Mutex<Vec<TerminalHostEvent>>>,
}

impl IsolatedHcHarness {
    fn new() -> Self {
        let dir = TempDir::new().unwrap();
        let workspace_a = dir.path().join("workspace-a");
        let workspace_b = dir.path().join("workspace-b");
        std::fs::create_dir_all(&workspace_a).unwrap();
        std::fs::create_dir_all(&workspace_b).unwrap();

        let host_events: Arc<Mutex<Vec<TerminalHostEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let sink_events = Arc::clone(&host_events);
        let session_name = format!("lt{}", &Uuid::new_v4().simple().to_string()[..8]);

        let service = TerminalService::new(
            None,
            Some(session_name),
            None,
            Some(Arc::new(move |evt| {
                if let Ok(mut list) = sink_events.lock() {
                    list.push(evt);
                }
            })),
        );

        Self {
            _dir: dir,
            workspace_a,
            workspace_b,
            service,
            host_events,
        }
    }
}

impl Drop for IsolatedHcHarness {
    fn drop(&mut self) {
        let _ = self.service.stop_daemon();
    }
}

fn wait_until(timeout: Duration, mut predicate: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if predicate() {
            return true;
        }
        thread::sleep(Duration::from_millis(25));
    }
    false
}

// tests -----------------------------------------------------------------------

mod given_an_isolated_hc_daemon_with_no_sessions {
    use super::*;

    mod when_creating_a_session_in_an_invalid_directory {
        use super::*;

        #[test]
        fn should_return_an_invalid_workspace_error() {
            // Arrange
            let harness = IsolatedHcHarness::new();
            let missing = harness.workspace_a.join("does-not-exist");

            // Act
            let result = harness.service.create_session(&missing, 80, 24);

            // Assert
            assert!(matches!(
                result,
                Err(LgtmTerminalError::InvalidWorkspace(_))
            ));
        }
    }

    mod when_creating_a_session_in_a_valid_workspace {
        use super::*;

        #[test]
        fn should_spawn_a_live_persistent_hc_terminal_and_list_it_by_workspace() {
            // Arrange
            let harness = IsolatedHcHarness::new();

            // Act
            let created = harness
                .service
                .create_session(&harness.workspace_a, 80, 24)
                .unwrap();
            let listed_a = harness
                .service
                .get_sessions(Some(&harness.workspace_a))
                .unwrap();
            let listed_b = harness
                .service
                .get_sessions(Some(&harness.workspace_b))
                .unwrap();

            // Assert
            assert!(created.alive);
            assert!(!created.id.is_empty());
            assert_eq!(listed_a.len(), 1);
            assert_eq!(listed_a[0].id, created.id);
            assert!(listed_b.is_empty());
        }
    }
}

mod given_a_running_hc_terminal_session {
    use super::*;

    mod when_attaching_writing_input_and_resizing {
        use super::*;

        #[test]
        fn should_stream_coalesced_output_over_the_framed_attach_channel() {
            // Arrange
            let harness = IsolatedHcHarness::new();
            let session = harness
                .service
                .create_session(&harness.workspace_a, 80, 24)
                .unwrap();
            let captured: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
            let sink_buf = Arc::clone(&captured);

            // Act
            let attached = harness
                .service
                .attach(
                    &session.id,
                    90,
                    28,
                    Arc::new(move |evt| match evt {
                        TerminalStreamEvent::Replay { data }
                        | TerminalStreamEvent::Output { data } => {
                            if let Ok(mut buf) = sink_buf.lock() {
                                buf.extend_from_slice(&data);
                            }
                        }
                        _ => {}
                    }),
                )
                .unwrap();
            harness.service.resize(&session.id, 100, 30).unwrap();
            harness
                .service
                .write_input(&session.id, b"echo LGTM_HC_STREAM_OK\r")
                .unwrap();

            let received = wait_until(Duration::from_secs(5), || {
                captured
                    .lock()
                    .map(|b| String::from_utf8_lossy(&b).contains("LGTM_HC_STREAM_OK"))
                    .unwrap_or(false)
            });

            // Assert
            assert_eq!(attached.id, session.id);
            assert!(
                received,
                "expected LGTM_HC_STREAM_OK in streamed output, got: {}",
                String::from_utf8_lossy(&captured.lock().unwrap())
            );
        }
    }

    mod when_detaching_and_reattaching_after_background_output {
        use super::*;

        #[test]
        fn should_keep_session_alive_and_replay_bounded_scrollback_on_reattach() {
            // Arrange
            let harness = IsolatedHcHarness::new();
            let session = harness
                .service
                .create_session(&harness.workspace_a, 80, 24)
                .unwrap();
            let first_buf: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
            let first_sink = Arc::clone(&first_buf);
            harness
                .service
                .attach(
                    &session.id,
                    80,
                    24,
                    Arc::new(move |evt| {
                        if let TerminalStreamEvent::Replay { data }
                        | TerminalStreamEvent::Output { data } = evt
                            && let Ok(mut buf) = first_sink.lock()
                        {
                            buf.extend_from_slice(&data);
                        }
                    }),
                )
                .unwrap();

            // Act: run marker command, detach, verify still alive, then re-attach.
            harness
                .service
                .write_input(&session.id, b"echo LGTM_BG_SURVIVED\r")
                .unwrap();
            assert!(wait_until(Duration::from_secs(5), || {
                first_buf
                    .lock()
                    .map(|b| String::from_utf8_lossy(&b).contains("LGTM_BG_SURVIVED"))
                    .unwrap_or(false)
            }));

            harness.service.detach(&session.id).unwrap();
            let listed_while_detached = harness
                .service
                .get_sessions(Some(&harness.workspace_a))
                .unwrap();

            let replay_bytes: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
            let replay_sink = Arc::clone(&replay_bytes);
            harness
                .service
                .attach(
                    &session.id,
                    80,
                    24,
                    Arc::new(move |evt| {
                        if let TerminalStreamEvent::Replay { data }
                        | TerminalStreamEvent::Output { data } = evt
                            && let Ok(mut buf) = replay_sink.lock()
                        {
                            buf.extend_from_slice(&data);
                        }
                    }),
                )
                .unwrap();

            let hydrated = wait_until(Duration::from_secs(5), || {
                replay_bytes
                    .lock()
                    .map(|b| String::from_utf8_lossy(&b).contains("LGTM_BG_SURVIVED"))
                    .unwrap_or(false)
            });
            let total_replay_len = replay_bytes.lock().unwrap().len();

            // Assert
            assert_eq!(listed_while_detached.len(), 1);
            assert!(listed_while_detached[0].alive);
            assert!(hydrated, "expected re-attached replay to contain marker");
            assert!(total_replay_len <= 256 * 1024);
        }
    }

    mod when_osc_7501_status_is_emitted_in_background {
        use super::*;

        #[test]
        fn should_dynamically_register_agent_and_clear_unseen_done_on_mark_seen() {
            // Arrange
            let harness = IsolatedHcHarness::new();
            let session = harness
                .service
                .create_session(&harness.workspace_a, 80, 24)
                .unwrap();
            harness
                .service
                .attach(&session.id, 80, 24, Arc::new(|_| {}))
                .unwrap();

            // Act: emit OSC 7501 working -> done for a dynamic custom agent while detached.
            harness
                .service
                .write_input(
                    &session.id,
                    b"sleep 0.25 && printf '\\033]7501;state=done:app=custom-bench-agent:msg=All green\\033\\\\'\r",
                )
                .unwrap();
            harness.service.detach(&session.id).unwrap();

            let saw_done = wait_until(Duration::from_secs(5), || {
                harness
                    .service
                    .get_sessions(Some(&harness.workspace_a))
                    .ok()
                    .and_then(|list| list.into_iter().find(|s| s.id == session.id))
                    .is_some_and(|s| {
                        s.status == TerminalAgentStatus::Done
                            && s.agent
                                .as_ref()
                                .is_some_and(|a| a.app_id == "custom-bench-agent" && a.unseen_done)
                    })
            });

            // Assert: registered dynamically with unseen_done = true, then cleared on mark_seen.
            assert!(
                saw_done,
                "expected dynamic OSC 7501 agent registration; host_events={:?}",
                harness.host_events.lock().unwrap()
            );

            let seen = harness
                .service
                .mark_seen(&session.id)
                .unwrap()
                .expect("session tracked");
            assert_eq!(seen.status, TerminalAgentStatus::Idle);
            assert!(seen.agent.as_ref().is_some_and(|a| !a.unseen_done));
        }
    }

    mod when_closing_a_session_vs_detaching_all {
        use super::*;

        #[test]
        fn should_keep_session_alive_on_detach_all_and_terminate_on_close() {
            // Arrange
            let harness = IsolatedHcHarness::new();
            let session = harness
                .service
                .create_session(&harness.workspace_a, 80, 24)
                .unwrap();
            harness
                .service
                .attach(&session.id, 80, 24, Arc::new(|_| {}))
                .unwrap();

            // Act: detach_all (app quit behavior) leaves hc session alive; close_session kills it.
            harness.service.detach_all();
            let after_detach = harness
                .service
                .get_sessions(Some(&harness.workspace_a))
                .unwrap();

            harness.service.close_session(&session.id).unwrap();
            let gone = wait_until(Duration::from_secs(5), || {
                harness
                    .service
                    .get_sessions(Some(&harness.workspace_a))
                    .map(|list| list.iter().all(|s| s.id != session.id))
                    .unwrap_or(false)
            });

            // Assert
            assert_eq!(after_detach.len(), 1);
            assert!(after_detach[0].alive);
            assert!(gone, "expected close_session to terminate the hc terminal");
        }
    }
}
