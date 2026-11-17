// SPDX-License-Identifier: Apache-2.0
use super::*;
#[test]
fn selected_log_prefixes_are_literal_and_exclude_verbose_noise() {
    for message in [
        "ORT optimization- Force fallback to CPU execution for node: node0",
        "CUDA kernel not found in registries for Op type: Foo",
        "CUDA kernel not supported. Fallback to CPU execution provider for Op type: Bar",
        "This session contains graph nodes that are assigned to the default CPU EP",
        "Could not find an implementation for Foo",
        "CUDA_EP Halting assignment due to capacity threshold",
    ] {
        assert!(selected(message));
    }
    assert!(!selected("Node placements"));
    assert!(!selected("unrelated verbose initialization"));
}
#[test]
fn log_selection_counts_utf8_and_retains_failure_observations() {
    let mut sink = Capture::default();
    sink.push("ignored", "unrelated");
    let message = "Could not find an implementation for \u{e9}";
    sink.push("\u{3bb}", message);
    assert_eq!(
        (
            sink.logs.seen,
            sink.logs.selected,
            sink.logs.retained,
            sink.logs.omitted
        ),
        (2, 1, 1, 0)
    );
    assert_eq!(sink.logs.bytes, 2 + 2 + message.len());
    assert_eq!(sink.logs.rows[0].message, message);
    assert!(!sink.logs.truncated);
}
#[test]
fn oversized_and_total_log_overflow_are_explicit() {
    let mut sink = Capture::default();
    sink.push(
        "",
        &format!("Could not find an implementation for {}", "x".repeat(4096)),
    );
    assert_eq!(
        (sink.logs.selected, sink.logs.retained, sink.logs.omitted),
        (1, 0, 1)
    );
    assert!(sink.logs.truncated);
    for _ in 0..2000 {
        sink.push("", "Could not find an implementation for x");
    }
    assert!(sink.logs.bytes <= 65536);
    assert_eq!(sink.logs.selected, sink.logs.retained + sink.logs.omitted);
}
#[test]
fn log_counter_saturation_is_never_complete() {
    let mut sink = Capture::default();
    sink.logs.seen = u64::MAX;
    sink.push("", "Could not find an implementation for x");
    assert!(sink.logs.truncated);
}
#[cfg(feature = "test-support")]
#[test]
fn diagnostic_authority_refuses_each_non_scientific_input() {
    assert!(authority(true, false, true, true, true, true).is_ok());
    for row in [
        (false, false, true, true, true, true),
        (true, true, true, true, true, true),
        (true, false, false, true, true, true),
        (true, false, true, false, true, true),
        (true, false, true, true, false, true),
        (true, false, true, true, true, false),
    ] {
        assert!(authority(row.0, row.1, row.2, row.3, row.4, row.5).is_err());
    }
}
#[cfg(feature = "test-support")]
fn placement() -> PlacementObservation {
    PlacementObservation {
        scope: "onnxruntime-final-session-state-ep-placement",
        complete: true,
        shim_status: 0,
        ort_error_code: 0,
        reported_nodes: 1,
        reported_string_bytes: 26,
        nodes: vec![PlacedNode {
            subgraph_index: 0,
            name: "n".into(),
            domain: "".into(),
            op: "Shape".into(),
            ep: "CPUExecutionProvider".into(),
        }],
    }
}
#[cfg(feature = "test-support")]
#[test]
fn complete_cpu_observation_never_satisfies_strict_qualification() {
    let mut value = placement();
    assert!(complete_placement(&value).is_ok());
    assert!(value.validate_strict().is_err());
    value.complete = false;
    assert!(complete_placement(&value).is_err());
    value.complete = true;
    value.reported_nodes = 2;
    assert!(complete_placement(&value).is_err());
}
#[cfg(feature = "test-support")]
#[test]
fn teardown_observes_pre_drop_post_without_returning_session() {
    use std::cell::RefCell;
    let events = RefCell::new(Vec::new());
    struct Session<'a>(&'a RefCell<Vec<&'static str>>);
    impl Drop for Session<'_> {
        fn drop(&mut self) {
            self.0.borrow_mut().push("drop");
        }
    }
    let observed = settle_with(
        Session(&events),
        |_| {
            events.borrow_mut().push("pre");
            Ok(())
        },
        || panic!("successful Drop must not retain custody"),
        || {
            events.borrow_mut().push("post");
            (Ok(()), Some(0), false)
        },
    );
    assert_eq!(*events.borrow(), ["pre", "drop", "post"]);
    assert!(observed.validate().is_ok());
}
#[cfg(feature = "test-support")]
#[test]
fn teardown_preserves_failed_sync_unknown_claims_and_poison() {
    let value = settle_with(
        (),
        |_| Err(anyhow::anyhow!("pre failed")),
        || panic!("a returned sync error is not a Drop panic"),
        || (Err(anyhow::anyhow!("post failed")), None, true),
    );
    assert!(value.session_drop_returned);
    assert_eq!(value.errors.len(), 2);
    assert!(value.validate().is_err());
    for claims in [None, Some(1)] {
        assert!(
            settle_with((), |_| Ok(()), || {}, || (Ok(()), claims, false))
                .validate()
                .is_err()
        );
    }
}
#[cfg(feature = "test-support")]
#[test]
fn teardown_drop_panic_triggers_retention_before_post_observation() {
    use std::cell::{Cell, RefCell};
    struct BadDrop<'a>(&'a RefCell<Vec<&'static str>>);
    impl Drop for BadDrop<'_> {
        fn drop(&mut self) {
            self.0.borrow_mut().push("drop panic");
            panic!("injected drop failure");
        }
    }
    let events = RefCell::new(Vec::new());
    let poisoned = Cell::new(false);
    let epoch = Cell::new(7u64);
    let retained = RefCell::new(None);
    let custody = Arc::new(());
    let value = settle_with(
        BadDrop(&events),
        |_| Ok(()),
        || {
            events.borrow_mut().push("poison and retain");
            poisoned.set(true);
            epoch.set(epoch.get() + 1);
            *retained.borrow_mut() = Some(custody.clone());
        },
        || {
            events.borrow_mut().push("post sync");
            assert!(poisoned.get());
            assert_eq!(epoch.get(), 8);
            assert_eq!(Arc::strong_count(&custody), 2);
            (Ok(()), Some(0), poisoned.get())
        },
    );
    assert_eq!(
        *events.borrow(),
        ["drop panic", "poison and retain", "post sync"]
    );
    assert!(!value.session_drop_returned);
    assert!(value.poisoned);
    assert!(retained.borrow().is_some());
    assert!(value.validate().is_err());
}

#[cfg(feature = "test-support")]
#[test]
fn snapshot_rejects_missing_teardown_or_truncated_logs() {
    let mut value = LoadSnapshot {
        session_created: true,
        placement: Some(placement()),
        graph_metadata: None,
        logs: LogSnapshot::default(),
        teardown: None,
    };
    assert!(value.validate().is_err());
    value.teardown = Some(settle_with(
        (),
        |_| Ok(()),
        || {},
        || (Ok(()), Some(0), false),
    ));
    assert!(value.validate().is_ok());
    value.logs.truncated = true;
    assert!(value.validate().is_err());
}

#[cfg(feature = "test-support")]
#[test]
fn graph_mode_permits_truncation_only_with_truthful_log_disclosure() {
    let logs = LogSnapshot {
        seen: 1,
        selected: 1,
        retained: 0,
        omitted: 1,
        bytes: 0,
        truncated: true,
        rows: vec![],
    };
    assert!(logs.validate_disclosure().is_ok());
    for mutation in 0..4 {
        let mut broken = logs.clone();
        match mutation {
            0 => broken.truncated = false,
            1 => broken.retained = 1,
            2 => broken.bytes = 1,
            _ => broken.seen = 0,
        }
        assert!(broken.validate_disclosure().is_err());
    }
}

#[cfg(feature = "test-support")]
#[test]
fn poisoned_capture_never_bypasses_settlement_or_unknown_release_retention() {
    use std::cell::{Cell, RefCell};
    let capture = Mutex::new(());
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = capture.lock().unwrap();
        panic!("injected capture poison");
    }));
    for fail_drop in [false, true] {
        let events = RefCell::new(Vec::new());
        let poisoned = Cell::new(false);
        struct Owned<'a> {
            events: &'a RefCell<Vec<&'static str>>,
            fail: bool,
        }
        impl Drop for Owned<'_> {
            fn drop(&mut self) {
                self.events.borrow_mut().push("drop");
                assert!(!self.fail, "injected unknown drop");
            }
        }
        let (recorded, teardown) = capture_then_settle(
            Owned {
                events: &events,
                fail: fail_drop,
            },
            || {
                events.borrow_mut().push("capture");
                let _guard = capture
                    .lock()
                    .map_err(|_| anyhow::anyhow!("capture poisoned"))?;
                Ok(())
            },
            |_| {
                events.borrow_mut().push("pre");
                Ok(())
            },
            || {
                events.borrow_mut().push("retain");
                poisoned.set(true);
            },
            || {
                events.borrow_mut().push("post");
                (Ok(()), Some(0), poisoned.get())
            },
        );
        assert!(recorded.is_err());
        assert_eq!(teardown.session_drop_returned, !fail_drop);
        assert_eq!(teardown.poisoned, fail_drop);
        if fail_drop {
            assert_eq!(
                *events.borrow(),
                ["capture", "pre", "drop", "retain", "post"]
            );
        } else {
            assert_eq!(*events.borrow(), ["capture", "pre", "drop", "post"]);
            assert!(teardown.validate().is_ok());
        }
    }
}
#[cfg(feature = "test-support")]
#[test]
fn capture_panic_still_performs_actual_settlement() {
    use std::cell::Cell;
    let post_seen = Cell::new(false);
    let (captured, teardown) = capture_then_settle(
        (),
        || panic!("injected capture panic"),
        |_| Ok(()),
        || panic!("known drop must not retain"),
        || {
            post_seen.set(true);
            (Ok(()), Some(0), false)
        },
    );
    assert!(captured.is_err());
    assert!(post_seen.get());
    assert!(teardown.validate().is_ok());
}
