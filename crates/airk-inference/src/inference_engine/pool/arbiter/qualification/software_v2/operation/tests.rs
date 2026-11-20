// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::arbiter::resource_policy::{
    ResourceFailure, ResourcePolicy, ResourceSample, ResourceSeries,
};
use std::cell::Cell;
fn identity() -> Identity {
    Identity {
        input_sha256: Sha256Digest::from_bytes([1; 32]),
        profile: Sha256Digest::from_bytes([2; 32]),
        process_id: 7,
        native_binary_sha256: Sha256Digest::from_bytes([3; 32]),
    }
}
fn attempt(identity: &Identity, kind: Kind, ordinal: usize) -> Attempt<'_> {
    Attempt {
        identity,
        max_numeric_ordinal: 12,
        kind,
        ordinal,
        input: (kind != Kind::Load).then_some(Sha256Digest::from_bytes([4; 32])),
        deadline_ms: 10,
        absolute_expires_ns: None,
    }
}
#[test]
fn native_operation_starts_before_call_with_one_original_expiry() -> Result<()> {
    let identity = identity();
    let mut sequence = 0;
    let mut times = [1000, 2000, 7_000_001, 8_000_001].into_iter();
    let mut events = Vec::new();
    let called = Cell::new(false);
    let observed = observe(
        attempt(&identity, Kind::Sample, 1),
        &mut sequence,
        &mut || Ok(times.next().unwrap()),
        &mut |event| {
            assert!(!called.get() || event.phase == Phase::Returned);
            events.push(event.clone());
            Ok(())
        },
        || {
            called.set(true);
            Ok(17)
        },
    )?;
    assert_eq!(observed.value?, 17);
    assert_eq!(observed.elapsed_us, 8000);
    assert_eq!(observed.native_call_elapsed_us, 6998);
    assert!(observed.within_deadline);
    assert_eq!(events.len(), 2);
    assert_eq!(sequence, 2);
    assert_eq!(events[0].sequence, 1);
    assert_eq!(events[0].phase, Phase::Started);
    assert_eq!(events[1].sequence, 2);
    assert_eq!(events[1].phase, Phase::Returned);
    assert_eq!(events[0].started_monotonic_ns, 1000);
    assert_eq!(events[0].expires_monotonic_ns, 10_001_000);
    assert_eq!(events[1].expires_monotonic_ns, 10_001_000);
    assert_eq!(events[1].native_success, Some(true));
    if let Some(path) = std::env::var_os("AIRK_SOFTWARE_V2_OPERATION_CASES_OUTPUT") {
        let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
        serde_json::to_writer_pretty(&mut file, &events)?;
        file.write_all(b"\n")?;
        file.sync_data()?;
    }
    Ok(())
}
#[test]
fn native_operation_start_observer_failure_never_dispatches() {
    let identity = identity();
    let called = Cell::new(false);
    let mut sequence = 0;
    let result = observe(
        attempt(&identity, Kind::Load, 0),
        &mut sequence,
        &mut || Ok(100),
        &mut |_| anyhow::bail!("actual start observer refused"),
        || {
            called.set(true);
            Ok(())
        },
    );
    assert!(result.is_err());
    assert!(!called.get());
    assert_eq!(sequence, 1);
}
#[test]
fn native_operation_startup_expiry_is_retained_without_dispatch() {
    let identity = identity();
    let called = Cell::new(false);
    let mut sequence = 0;
    let mut times = [100, 10_000_100].into_iter();
    let mut events = Vec::new();
    let result = observe(
        attempt(&identity, Kind::Load, 0),
        &mut sequence,
        &mut || Ok(times.next().unwrap()),
        &mut |event| {
            events.push(event.clone());
            Ok(())
        },
        || {
            called.set(true);
            Ok(())
        },
    );
    assert!(result.is_err());
    assert!(!called.get());
    assert_eq!(events.len(), 2);
    assert_eq!(events[1].phase, Phase::NotDispatched);
    assert_eq!(events[1].native_success, None);
    assert_eq!(events[1].expires_monotonic_ns, 10_000_100);
}
#[test]
fn native_operation_known_error_retains_actual_end_without_renewal() -> Result<()> {
    let identity = identity();
    let mut sequence = 0;
    let mut times = [100, 200, 300, 400].into_iter();
    let mut events = Vec::new();
    let result = observe(
        attempt(&identity, Kind::Numeric, 3),
        &mut sequence,
        &mut || Ok(times.next().unwrap()),
        &mut |event| {
            events.push(event.clone());
            Ok(())
        },
        || anyhow::bail!("actual score returned error"),
    )?;
    let error: Result<()> = result.value;
    assert!(error.is_err());
    assert_eq!(events.len(), 2);
    assert_eq!(events[1].phase, Phase::Returned);
    assert_eq!(events[1].native_success, Some(false));
    assert_eq!(events[1].expires_monotonic_ns, 10_000_100);
    assert_eq!(result.elapsed_us, 1);
    Ok(())
}
#[test]
fn native_operation_first_expired_return_stops_core_series_with_exact_partial_sample() -> Result<()>
{
    let identity = identity();
    let policy = ResourcePolicy::software_v2();
    let mut series = ResourceSeries::new(100)?;
    let mut sequence = 0;
    let mut times = [100, 200, 120_000_000_099, 120_000_000_101].into_iter();
    let mut events = Vec::new();
    let calls = Cell::new(0);
    let mut retained = None;
    let result = super::super::observe_series(
        &mut series,
        &policy,
        || {
            let mut original = attempt(&identity, Kind::Sample, 1);
            original.deadline_ms = policy.deadline_ms;
            let observed = observe(
                original,
                &mut sequence,
                &mut || Ok(times.next().unwrap()),
                &mut |event| {
                    events.push(event.clone());
                    Ok(())
                },
                || {
                    calls.set(calls.get() + 1);
                    Ok(())
                },
            )?;
            observed.value?;
            assert!(!observed.within_deadline);
            Ok(ResourceSample {
                elapsed_us: observed.elapsed_us,
                peak_process_rss_bytes: 10,
            })
        },
        |series| {
            retained = Some(series.clone());
            Ok(())
        },
    );
    assert!(result.is_err());
    assert_eq!(calls.get(), 1);
    assert_eq!(events.len(), 2);
    let retained = retained.unwrap();
    assert_eq!(retained.attempted_samples, 1);
    assert_eq!(retained.samples.len(), 1);
    assert_eq!(retained.samples[0].elapsed_us, 120_000_001);
    assert_eq!(retained.failure, Some(ResourceFailure::DeadlineOverrun));
    assert!(!retained.complete);
    assert_eq!(retained.nearest_rank_p95_us, None);
    assert_eq!(retained.maximum_us, None);
    Ok(())
}
#[test]
fn native_operation_exact_boundary_end_succeeds_and_end_observer_failure_refuses() -> Result<()> {
    let identity = identity();
    let mut sequence = 0;
    let mut times = [100, 200, 10_000_099, 10_000_100].into_iter();
    let value = observe(
        attempt(&identity, Kind::Warmup, 0),
        &mut sequence,
        &mut || Ok(times.next().unwrap()),
        &mut |_| Ok(()),
        || Ok(()),
    )?;
    value.value?;
    assert!(value.within_deadline);
    assert_eq!(value.elapsed_us, 10000);
    let called = Cell::new(0);
    let mut times = [100, 200, 300].into_iter();
    let result = observe(
        attempt(&identity, Kind::Numeric, 12),
        &mut sequence,
        &mut || Ok(times.next().unwrap()),
        &mut |event| {
            if event.phase == Phase::Returned {
                anyhow::bail!("end observer refused")
            }
            Ok(())
        },
        || {
            called.set(called.get() + 1);
            Ok(())
        },
    );
    assert!(result.is_err());
    assert_eq!(called.get(), 1);
    Ok(())
}
#[test]
fn native_numeric_journal_limits_each_original_roster_before_dispatch() {
    let identity = identity();
    for (maximum, ordinal, accepted) in [
        (3, 4, false),
        (12, 12, true),
        (12, 13, false),
        (14, 13, true),
        (14, 14, true),
        (14, 15, false),
    ] {
        let attempted = Attempt {
            max_numeric_ordinal: maximum,
            ..attempt(&identity, Kind::Numeric, ordinal)
        };
        let called = Cell::new(false);
        let mut sequence = 0;
        let result = observe(
            attempted,
            &mut sequence,
            &mut || Ok(100),
            &mut |_| Ok(()),
            || {
                called.set(true);
                Ok(())
            },
        );
        assert_eq!(result.is_ok(), accepted);
        assert_eq!(called.get(), accepted);
    }
}
#[test]
fn native_operation_wrong_ordinals_and_backwards_clocks_never_gain_time() {
    let identity = identity();
    for (kind, ordinal) in [
        (Kind::Load, 1),
        (Kind::Warmup, 1),
        (Kind::Sample, 0),
        (Kind::Sample, 21),
        (Kind::Numeric, 0),
        (Kind::Numeric, 13),
    ] {
        let mut sequence = 0;
        let called = Cell::new(false);
        assert!(
            observe(
                attempt(&identity, kind, ordinal),
                &mut sequence,
                &mut || Ok(100),
                &mut |_| Ok(()),
                || {
                    called.set(true);
                    Ok(())
                }
            )
            .is_err()
        );
        assert!(!called.get());
        assert_eq!(sequence, 0);
    }
    let mut sequence = 0;
    let mut times = [100, 99].into_iter();
    let called = Cell::new(false);
    assert!(
        observe(
            attempt(&identity, Kind::Load, 0),
            &mut sequence,
            &mut || Ok(times.next().unwrap()),
            &mut |_| Ok(()),
            || {
                called.set(true);
                Ok(())
            }
        )
        .is_err()
    );
    assert!(!called.get());
}

#[test]
fn native_operation_retains_original_native_error_when_any_after_call_observer_fails() {
    #[derive(Debug)]
    struct ActualNativeFailure;
    impl std::fmt::Display for ActualNativeFailure {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("actual native failure marker")
        }
    }
    impl std::error::Error for ActualNativeFailure {}
    let cases = [
        (
            vec![Some(100), Some(200), None],
            false,
            0,
            "injected clock read failure",
        ),
        (
            vec![Some(100), Some(200), Some(199)],
            false,
            0,
            "return clock moved backwards",
        ),
        (
            vec![Some(100), Some(200), Some(300)],
            true,
            0,
            "injected end retention failure",
        ),
        (
            vec![Some(100), Some(200), Some(300), None],
            false,
            0,
            "injected clock read failure",
        ),
        (
            vec![Some(100), Some(200), Some(300), Some(299)],
            false,
            0,
            "retention clock moved backwards",
        ),
        (
            vec![Some(100), Some(200), Some(300)],
            false,
            u64::MAX - 1,
            "operation sequence overflow",
        ),
    ];
    for (times, end_fails, mut sequence, expected_observer_error) in cases {
        let identity = identity();
        let mut times = times.into_iter();
        let called = Cell::new(0);
        let result: Result<Returned<()>> = observe(
            attempt(&identity, Kind::Numeric, 1),
            &mut sequence,
            &mut || {
                times
                    .next()
                    .unwrap()
                    .ok_or_else(|| anyhow::anyhow!("injected clock read failure"))
            },
            &mut |event| {
                if end_fails && event.phase == Phase::Returned {
                    anyhow::bail!("injected end retention failure");
                }
                Ok(())
            },
            || {
                called.set(called.get() + 1);
                Err(ActualNativeFailure.into())
            },
        );
        let error = result
            .err()
            .expect("after-call observation must fail closed");
        assert!(
            error.downcast_ref::<ActualNativeFailure>().is_some(),
            "lost original typed native error"
        );
        let display = format!("{error:#}");
        assert!(display.contains("actual native failure marker"));
        assert!(display.contains(expected_observer_error), "{display}");
        assert_eq!(
            called.get(),
            1,
            "observer failure admitted another native call"
        );
    }
}

#[test]
fn absolute_component_deadline_is_not_renewed_by_later_operation_start() -> Result<()> {
    let identity = identity();
    let mut sequence = 0;
    let mut times = [9_000_000, 9_000_001, 10_000_001, 10_000_002].into_iter();
    let mut events = Vec::new();
    let mut request = attempt(&identity, Kind::Numeric, 1);
    request.absolute_expires_ns = Some(10_000_000);
    let result = observe(
        request,
        &mut sequence,
        &mut || Ok(times.next().unwrap()),
        &mut |event| {
            events.push(event.clone());
            Ok(())
        },
        || Ok(1),
    )?;
    assert!(!result.within_deadline);
    assert_eq!(result.value?, 1);
    assert!(events.iter().all(|v| v.expires_monotonic_ns == 10_000_000));
    Ok(())
}
