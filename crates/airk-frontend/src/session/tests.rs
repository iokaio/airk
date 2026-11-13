// SPDX-License-Identifier: Apache-2.0
//! Session engine regressions over real OS pipes with a fixture invoker. Every
//! wait is for an observed frame or exit under a deadline; no fixed sleeps.
use super::*;
use serde_json::json;
use std::sync::mpsc::{Receiver, Sender};

const DEADLINE: Duration = Duration::from_secs(20);

/// Fixture commands: `block` waits until cancelled or released, `fail` returns
/// an error, `big` overflows the response, `exclusive` records its start and
/// waits for release, `echo` writes its stdin, `help` completes at parse time.
struct Fixture {
    release: Mutex<Option<Receiver<()>>>,
    started: Mutex<Sender<String>>,
}

fn wait_cancel_or(release: Option<Receiver<()>>, cancel: &CancellationToken) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let release = async move {
            match release {
                Some(release) => {
                    let _ = tokio::task::spawn_blocking(move || release.recv()).await;
                }
                None => std::future::pending::<()>().await,
            }
        };
        tokio::select! {
            () = cancel.cancelled() => {}
            () = release => {}
        }
    });
    // Do not wait for a blocked release reader after cancellation.
    runtime.shutdown_background();
}

impl Invoker for Fixture {
    fn prepare(&self, argv: Vec<String>) -> Option<Prepared> {
        let name = argv.first().cloned().unwrap_or_default();
        let started = self.started.lock().unwrap().clone();
        let release = || self.release.lock().unwrap().take();
        let admission = if matches!(name.as_str(), "exclusive" | "stubborn") {
            Admission::Exclusive
        } else {
            Admission::Concurrent
        };
        let job: Job = match name.as_str() {
            "refused" => return None,
            "help" => {
                return Some(Prepared::Complete {
                    success: true,
                    stdout: "usage\n".into(),
                    stderr: String::new(),
                });
            }
            "echo" => Box::new(|io: &CliIo, _: &CancellationToken| -> Result<()> {
                let mut input = String::new();
                io.stdin().read_to_string(&mut input)?;
                io.line(input);
                Ok(())
            }),
            "fail" => Box::new(|_: &CliIo, _: &CancellationToken| -> Result<()> {
                anyhow::bail!("fixture failure")
            }),
            "big" => Box::new(|io: &CliIo, _: &CancellationToken| -> Result<()> {
                io.text("x".repeat(MAX_RESPONSE_FRAME_BYTES));
                Ok(())
            }),
            "large" => Box::new(|io: &CliIo, _: &CancellationToken| -> Result<()> {
                io.text("y".repeat(LARGE));
                Ok(())
            }),
            "block" => Box::new(|_: &CliIo, cancel: &CancellationToken| -> Result<()> {
                wait_cancel_or(None, cancel);
                Ok(())
            }),
            // Exclusive work that ignores cancellation and waits only for release.
            "stubborn" => {
                let held = release();
                Box::new(move |_: &CliIo, _: &CancellationToken| -> Result<()> {
                    let _ = started.send("stubborn".into());
                    if let Some(held) = held {
                        let _ = held.recv();
                    }
                    Ok(())
                })
            }
            "exclusive" => {
                let label = argv.get(1).cloned().unwrap_or_default();
                let held = if label == "first" { release() } else { None };
                Box::new(
                    move |io: &CliIo, cancel: &CancellationToken| -> Result<()> {
                        let _ = started.send(label.clone());
                        if label == "first" {
                            wait_cancel_or(held, cancel);
                        }
                        io.line(&label);
                        Ok(())
                    },
                )
            }
            _ => return None,
        };
        Some(Prepared::Run { admission, job })
    }
    fn report_failure(&self, error: &anyhow::Error, io: &CliIo) {
        io.error_line(format!("Error: {error:#}"));
    }
}

struct Harness {
    input: Option<std::io::PipeWriter>,
    responses: Receiver<Value>,
    exited: Receiver<Result<()>>,
    started: Receiver<String>,
    release: Sender<()>,
    owner_lost: Arc<AtomicBool>,
}

fn id(n: u8) -> Incarnation {
    Incarnation::from_bytes([n; 16])
}

impl Harness {
    fn new() -> Self {
        let (input_reader, input) = std::io::pipe().unwrap();
        let (mut output_reader, output) = std::io::pipe().unwrap();
        let (release, held) = mpsc::channel();
        let (started_sender, started) = mpsc::channel();
        let invoker = Arc::new(Fixture {
            release: Mutex::new(Some(held)),
            started: Mutex::new(started_sender),
        });
        let (done, exited) = mpsc::channel();
        let owner_lost = Arc::new(AtomicBool::new(false));
        let probe = owner_lost.clone();
        std::thread::spawn(move || {
            let identity = Identity {
                version: "0.0.0-fixture".into(),
                executable_sha256: "1".repeat(64),
            };
            let _ = done.send(serve_streams(
                invoker,
                input_reader,
                output,
                identity,
                8 << 20,
                Timing {
                    drain: Duration::from_millis(50),
                    owner_poll: Duration::from_millis(10),
                },
                Some(Box::new(move || probe.load(Ordering::SeqCst))),
            ));
        });
        let (frames, responses) = mpsc::channel();
        std::thread::spawn(move || {
            while let Ok(Some(value)) = service_ipc::read_frame_limited::<_, Value>(
                &mut output_reader,
                MAX_RESPONSE_FRAME_BYTES,
            ) {
                if frames.send(value).is_err() {
                    break;
                }
            }
        });
        Self {
            input: Some(input),
            responses,
            exited,
            started,
            release,
            owner_lost,
        }
    }
    fn exited_before_deadline(&self) -> Result<()> {
        self.exited
            .recv_timeout(DEADLINE)
            .expect("session exits before the deadline")
    }
    fn send(&mut self, value: Value) {
        service_ipc::write_frame_limited(
            self.input.as_mut().unwrap(),
            &value,
            MAX_REQUEST_FRAME_BYTES,
        )
        .unwrap();
    }
    fn next(&self) -> Value {
        self.responses
            .recv_timeout(DEADLINE)
            .expect("session response before the deadline")
    }
    fn hello(&mut self) {
        self.send(json!({"id":id(1),"request":{"hello":{"protocol":1}}}));
        let reply = self.next();
        assert_eq!(reply["outcome"]["status"], "hello", "{reply}");
    }
    fn invoke(&mut self, n: u8, argv: &[&str], deadline_ms: u64) {
        self.send(json!({"id":id(n),"request":{"invoke":{"argv":argv,"deadline_ms":deadline_ms}}}));
    }
    fn close(mut self) -> Result<()> {
        drop(self.input.take());
        self.exited
            .recv_timeout(DEADLINE)
            .expect("session exits after EOF before the deadline")
    }
}

#[test]
fn hello_must_come_first_and_answers_the_session_identity() {
    let mut session = Harness::new();
    session.invoke(2, &["echo"], 1000);
    assert_eq!(
        session.next(),
        json!({"id":id(2),"outcome":{"status":"failed","code":"session_protocol_mismatch","retryable":false}})
    );
    session.send(json!({"id":id(3),"request":{"hello":{"protocol":2}}}));
    assert_eq!(
        session.next()["outcome"]["code"],
        "session_protocol_mismatch"
    );
    session.send(json!({"id":id(1),"request":{"hello":{"protocol":1}}}));
    assert_eq!(
        session.next(),
        json!({"id":id(1),"outcome":{"status":"hello","protocol":1,"version":"0.0.0-fixture","executable_sha256":"1".repeat(64)}})
    );
    session.close().unwrap();
}

#[test]
fn refused_complete_echo_and_failed_commands_answer_their_own_ids() {
    let mut session = Harness::new();
    session.hello();
    session.invoke(2, &["refused"], 1000);
    assert_eq!(
        session.next(),
        json!({"id":id(2),"outcome":{"status":"failed","code":"session_command_refused","retryable":false}})
    );
    session.invoke(3, &["help"], 1000);
    assert_eq!(
        session.next(),
        json!({"id":id(3),"outcome":{"status":"invoked","success":true,"stdout":"usage\n","stderr":""}})
    );
    session.send(json!({"id":id(4),"request":{"invoke":{"argv":["echo"],"stdin":"request input","deadline_ms":5000}}}));
    assert_eq!(
        session.next(),
        json!({"id":id(4),"outcome":{"status":"invoked","success":true,"stdout":"request input\n","stderr":""}})
    );
    session.invoke(5, &["fail"], 5000);
    assert_eq!(
        session.next(),
        json!({"id":id(5),"outcome":{"status":"invoked","success":false,"stdout":"","stderr":"Error: fixture failure\n"}})
    );
    session.close().unwrap();
}

#[test]
fn budgets_and_malformed_requests_are_refused_before_any_work() {
    let mut session = Harness::new();
    session.hello();
    let invalid = |n: u8| json!({"id":id(n),"outcome":{"status":"failed","code":"session_invalid_request","retryable":false}});
    session.invoke(2, &["echo"], 0);
    assert_eq!(session.next(), invalid(2));
    session.invoke(3, &["echo"], MAX_INVOKE_DEADLINE_MS + 1);
    assert_eq!(session.next(), invalid(3));
    let many: Vec<&str> = vec!["echo"; MAX_INVOKE_ARGS + 1];
    session.invoke(4, &many, 1000);
    assert_eq!(session.next(), invalid(4));
    let big = "x".repeat(MAX_INVOKE_STDIN_BYTES + 1);
    session.send(
        json!({"id":id(5),"request":{"invoke":{"argv":["echo"],"stdin":big,"deadline_ms":1000}}}),
    );
    assert_eq!(session.next(), invalid(5));
    session.send(json!({"id":id(6),"request":{"control":{"runtime_dir":"C:\\fixture","runtime_sha256":"not-a-digest","method":"health"}}}));
    assert_eq!(session.next(), invalid(6));
    session.send(json!({"id":id(7),"request":{"spawn":{"argv":[]}}}));
    assert_eq!(session.next(), invalid(7));
    session.close().unwrap();
}

#[test]
fn oversize_output_is_refused_never_truncated() {
    let mut session = Harness::new();
    session.hello();
    // The output fits the capture budget exactly, but the encoded frame does
    // not: oversize is judged on the encoded frame.
    session.invoke(2, &["big"], 60_000);
    assert_eq!(
        session.next(),
        json!({"id":id(2),"outcome":{"status":"failed","code":"session_response_oversize","retryable":false}})
    );
    // Output above the former 4 MiB codec cap is delivered complete.
    session.invoke(3, &["large"], 60_000);
    let reply = session.next();
    assert_eq!(reply["outcome"]["status"], "invoked");
    assert_eq!(
        reply["outcome"]["stdout"].as_str().map(str::len),
        Some(LARGE)
    );
    session.close().unwrap();
}

const LARGE: usize = 6 * 1024 * 1024;

#[test]
fn busy_is_retryable_and_cancel_answers_itself_then_its_stopped_target() {
    let mut session = Harness::new();
    session.hello();
    for n in 0..MAX_IN_FLIGHT as u8 {
        session.invoke(10 + n, &["block"], 60_000);
    }
    session.invoke(9, &["echo"], 1000);
    assert_eq!(
        session.next(),
        json!({"id":id(9),"outcome":{"status":"failed","code":"session_busy","retryable":true}})
    );
    for n in 0..MAX_IN_FLIGHT as u8 {
        // Cancels need no request slot even while every slot is busy.
        session.send(json!({"id":id(100 + n),"request":{"cancel":{"target":id(10 + n)}}}));
        let mut answers = [session.next(), session.next()];
        answers.sort_by_key(|reply| reply["id"].as_str().unwrap().to_owned());
        assert_eq!(
            answers,
            [
                json!({"id":id(10 + n),"outcome":{"status":"cancelled"}}),
                json!({"id":id(100 + n),"outcome":{"status":"cancelled"}}),
            ],
            "the cancel answers itself and its target answers once stopped"
        );
    }
    // A target that is no longer in flight is not a cancellable request.
    session.send(json!({"id":id(99),"request":{"cancel":{"target":id(10)}}}));
    assert_eq!(
        session.next(),
        json!({"id":id(99),"outcome":{"status":"failed","code":"session_invalid_request","retryable":false}})
    );
    session.close().unwrap();
}

#[test]
fn elapsed_deadline_answers_and_cancels_the_command() {
    let mut session = Harness::new();
    session.hello();
    session.invoke(2, &["block"], 50);
    assert_eq!(
        session.next(),
        json!({"id":id(2),"outcome":{"status":"failed","code":"session_deadline_exceeded","retryable":false}})
    );
    // The cancelled command releases its slot; EOF then exits promptly.
    session.close().unwrap();
}

#[test]
fn exclusive_commands_run_one_at_a_time() {
    let mut session = Harness::new();
    session.hello();
    session.invoke(2, &["exclusive", "first"], 60_000);
    assert_eq!(
        session.started.recv_timeout(DEADLINE).unwrap(),
        "first",
        "the first exclusive command starts"
    );
    session.invoke(3, &["exclusive", "second"], 60_000);
    session.invoke(4, &["help"], 1000);
    assert_eq!(
        session.next()["id"],
        json!(id(4)),
        "read-only work is not blocked by exclusive work"
    );
    assert!(
        session.started.try_recv().is_err(),
        "the second exclusive command waits for the first"
    );
    session.release.send(()).unwrap();
    let mut answered = [session.next(), session.next()];
    answered.sort_by_key(|reply| reply["id"].as_str().unwrap().to_owned());
    assert_eq!(answered[0]["outcome"]["stdout"], "first\n");
    assert_eq!(answered[1]["outcome"]["stdout"], "second\n");
    assert_eq!(session.started.recv_timeout(DEADLINE).unwrap(), "second");
    session.close().unwrap();
}

#[test]
fn eof_cancels_read_only_work_and_exits() {
    let mut session = Harness::new();
    session.hello();
    session.invoke(2, &["block"], 600_000);
    // No answer is owed after EOF; the session exits once the work is cancelled.
    session.close().unwrap();
}

#[test]
fn control_relay_without_a_service_reports_it_unavailable() {
    let mut session = Harness::new();
    session.hello();
    let missing = std::env::temp_dir().join(format!("airk-session-missing-{}", std::process::id()));
    session
        .send(json!({"id":id(2),"request":{"control":{"runtime_dir":missing,"method":"health"}}}));
    assert_eq!(
        session.next(),
        json!({"id":id(2),"outcome":{"status":"control","response":{"status":"failure","code":"service_unavailable"}}})
    );
    session.close().unwrap();
}

#[test]
fn a_frame_without_an_identifiable_request_closes_the_session() {
    let mut session = Harness::new();
    session.hello();
    session.send(json!({"request":{"hello":{"protocol":1}}}));
    let exited = session
        .exited
        .recv_timeout(DEADLINE)
        .expect("codec failure closes the session");
    assert!(exited.is_ok());
}

#[test]
fn stderr_tail_starts_on_a_character_boundary() {
    let mut bytes = "é".repeat(MAX_INVOKE_STDERR_BYTES).into_bytes();
    bytes.extend_from_slice(b"end");
    let kept = tail(bytes);
    assert!(kept.len() <= MAX_INVOKE_STDERR_BYTES);
    assert!(kept.ends_with("end") && !kept.contains('\u{fffd}'));
}

#[test]
fn shutdown_drains_only_started_exclusive_work_and_never_past_its_deadline() {
    let now = Instant::now();
    let drain = Duration::from_secs(5);
    assert_eq!(
        drain_until(now, drain, []),
        now + drain,
        "no exclusive work"
    );
    let later = now + Duration::from_secs(300);
    assert_eq!(
        drain_until(now, drain, [now + Duration::from_secs(1), later]),
        later,
        "started exclusive work may finish within its own deadline"
    );
    assert!(may_exit(now, later, 0), "nothing in flight exits at once");
    assert!(
        !may_exit(now, later, 1),
        "started exclusive work is waited for"
    );
    assert!(
        may_exit(later, later, 1),
        "work exceeding its deadline does not hold the session"
    );
}

#[test]
fn owner_loss_cancels_read_only_work_and_exits_with_stdin_still_open() {
    let mut session = Harness::new();
    session.hello();
    session.invoke(2, &["block"], 600_000);
    session.owner_lost.store(true, Ordering::SeqCst);
    session.exited_before_deadline().unwrap();
}

#[test]
fn owner_loss_lets_started_exclusive_work_finish_then_exits() {
    let mut session = Harness::new();
    session.hello();
    session.invoke(2, &["exclusive", "first"], 60_000);
    assert_eq!(session.started.recv_timeout(DEADLINE).unwrap(), "first");
    session.owner_lost.store(true, Ordering::SeqCst);
    session.release.send(()).unwrap();
    // The installer command was neither cancelled nor cut off: it completes.
    assert_eq!(
        session.next(),
        json!({"id":id(2),"outcome":{"status":"invoked","success":true,"stdout":"first\n","stderr":""}})
    );
    session.exited_before_deadline().unwrap();
}

#[test]
fn owner_loss_does_not_wait_past_the_exclusive_deadline() {
    let mut session = Harness::new();
    session.hello();
    session.invoke(2, &["stubborn"], 200);
    assert_eq!(session.started.recv_timeout(DEADLINE).unwrap(), "stubborn");
    session.owner_lost.store(true, Ordering::SeqCst);
    assert_eq!(
        session.next(),
        json!({"id":id(2),"outcome":{"status":"failed","code":"session_deadline_exceeded","retryable":false}})
    );
    // The work ignores cancellation and never stops; the session exits anyway.
    session.exited_before_deadline().unwrap();
}

#[test]
fn eof_lets_started_exclusive_work_finish_then_exits() {
    let mut session = Harness::new();
    session.hello();
    session.invoke(2, &["exclusive", "first"], 60_000);
    assert_eq!(session.started.recv_timeout(DEADLINE).unwrap(), "first");
    drop(session.input.take());
    session.release.send(()).unwrap();
    assert_eq!(
        session.next(),
        json!({"id":id(2),"outcome":{"status":"invoked","success":true,"stdout":"first\n","stderr":""}})
    );
    session.exited_before_deadline().unwrap();
}
