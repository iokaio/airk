// SPDX-License-Identifier: Apache-2.0
use super::*;

#[test]
fn session_ingress_lane_bounds_queued_frames_and_preserves_completion_progress() {
    let (sender, receiver) = event_channel();
    let raw = "null";
    for _ in 0..16 {
        assert!(
            sender
                .try_send(Event::Frame {
                    frame: Ok(Some(
                        serde_json::value::RawValue::from_string(raw.into()).unwrap()
                    )),
                    received: Received { ms: 0, ns: Some(0) },
                })
                .is_ok()
        );
    }
    // A stalled actor cannot cause unlimited frame decoding or block a clock
    // callback. Freeing one slot admits the actual blocked producer again.
    assert!(matches!(
        sender.try_send(Event::Wake),
        Err(mpsc::TrySendError::Full(_))
    ));
    let (attempt, attempted) = channel();
    let producer = std::thread::spawn(move || {
        attempt.send(()).unwrap();
        assert!(
            sender
                .send(Event::Finished {
                    id: id(10).parse().unwrap(),
                    outcome: Box::new(SessionOutcome::Cancelled),
                    effects: Box::new(Effects::default()),
                })
                .is_ok()
        );
    });
    attempted.recv_timeout(OBSERVE).unwrap();
    assert!(matches!(
        receiver.recv_timeout(OBSERVE).unwrap(),
        Event::Frame { .. }
    ));
    producer.join().unwrap();
    for _ in 0..15 {
        assert!(matches!(
            receiver.recv_timeout(OBSERVE).unwrap(),
            Event::Frame { .. }
        ));
    }
    assert!(matches!(
        receiver.recv_timeout(OBSERVE).unwrap(),
        Event::Finished { .. }
    ));
}

struct QueuedService {
    opening: Sender<()>,
    gate: Gate,
    effects: Arc<AtomicU64>,
}
impl Service for QueuedService {
    fn instance(&self) -> Result<Incarnation, ErrorLine> {
        Ok(E1.parse().unwrap())
    }
    fn request(
        &self,
        method: ControlMethod,
        expected: Incarnation,
    ) -> Result<ControlResponse, ErrorLine> {
        Ok(match method {
            ControlMethod::WindowOpen { .. } => {
                self.opening.send(()).unwrap();
                self.gate.pass();
                ControlResponse::WindowOpened {
                    window_id: id(2).parse().unwrap(),
                    service_instance: expected,
                    lease: WindowLease::from_bytes([3; 32]),
                    ttl_ms: 30000,
                }
            }
            ControlMethod::WindowClose { .. } => ControlResponse::WindowClosed {},
            ControlMethod::AdminNativeOpen { .. } => {
                self.effects.fetch_add(1, Ordering::SeqCst);
                ControlResponse::Failure {
                    code: "unexpected_expired_open".into(),
                }
            }
            _ => panic!("unexpected queued-deadline service request"),
        })
    }
}

#[test]
fn session_frame_receipt_deadline_survives_blocked_hello_admission() {
    for admin in [false, true] {
        let clock = Arc::new(ManualClock::default());
        let (opening, opened) = channel();
        let gate = Gate::default();
        gate.set(true);
        let effects = Arc::new(AtomicU64::new(0));
        let service = Arc::new(QueuedService {
            opening,
            gate: gate.clone(),
            effects: effects.clone(),
        });
        let invoked = effects.clone();
        let mut settings = options(&clock, None, move |_, _| {
            invoked.fetch_add(1, Ordering::SeqCst);
            0
        });
        settings.connect = Some(Arc::new(move || Some(service.clone() as Arc<dyn Service>)));
        let mut client = Client::start(settings);
        client.read_next.recv_timeout(OBSERVE).unwrap();
        client.send(&hello());
        opened.recv_timeout(OBSERVE).unwrap();
        client.read_next.recv_timeout(OBSERVE).unwrap();
        client.send(&frame(
            10,
            if admin {
                json!({"admin_call":{"operation":"usage.document","body":{},"deadline_ms":5}})
            } else {
                json!({"invoke":{"argv":["version"],"deadline_ms":5}})
            },
        ));
        // Reader has queued the complete request and begun its next read.
        client.read_next.recv_timeout(OBSERVE).unwrap();
        clock.advance(6);
        gate.set(false);
        assert_eq!(client.next()["outcome"]["status"], "hello");
        let expired = client.next();
        assert_eq!(expired["outcome"]["code"], "session_deadline_exceeded");
        let (_, exit, _) = client.eof();
        assert_eq!(exit, 0);
        assert_eq!(
            effects.load(Ordering::SeqCst),
            0,
            "expired queued request reached first effect"
        );
    }
}

#[test]
fn session_blocked_writer_bounds_reply_lane_without_blocking_actor() {
    struct Blocked {
        observed: Sender<()>,
        gate: Gate,
    }
    impl Write for Blocked {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.observed.send(()).unwrap();
            self.gate.pass();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let outbox = Arc::new(Outbox::default());
    let gate = Gate::default();
    gate.set(true);
    let (observed, writing) = channel();
    let (sender, _receiver) = std::sync::mpsc::sync_channel(MAX_IN_FLIGHT * 2);
    outbox.reply(vec![1]);
    let queue = outbox.clone();
    let writer_gate = gate.clone();
    let writer = std::thread::spawn(move || {
        write_loop(
            &queue,
            Blocked {
                observed,
                gate: writer_gate,
            },
            &sender,
        )
    });
    writing.recv_timeout(OBSERVE).unwrap();
    // Actor-facing admission must remain nonblocking while the OS writer waits.
    for _ in 0..=MAX_IN_FLIGHT {
        outbox.reply(vec![2]);
    }
    let pending = outbox
        .lock()
        .frames
        .iter()
        .filter(|(_, event)| !*event)
        .count();
    outbox.close();
    gate.set(false);
    writer.join().unwrap();
    assert!(
        pending <= MAX_IN_FLIGHT,
        "blocked writer retained {pending} reply frames"
    );
}
