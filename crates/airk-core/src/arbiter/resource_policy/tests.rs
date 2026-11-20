// SPDX-License-Identifier: Apache-2.0
use super::*;

#[test]
fn software_v2_policy_has_literal_limits_and_distinct_canonical_identity() -> Result<()> {
    let policy = ResourcePolicy::software_v2();
    assert_eq!(policy.shared_input_tokens, 8192);
    assert_eq!(policy.context_input_tokens, 8192);
    assert_eq!(policy.focus_pair_input_tokens, 512);
    assert_eq!(policy.maximum_context_candidates, 8);
    assert_eq!(policy.deadline_ms, 120_000);
    assert_eq!(policy.p95_limit_us, 120_000_000);
    assert_eq!(policy.required_warm_samples, 20);
    assert_eq!(policy.quantile, ResourceQuantile::NearestRank95);
    assert!(policy.stop_on_first_overrun);
    let expected = br#"{"schema":"airk.arbiter.resource-policy/2","shared_input_tokens":8192,"context_input_tokens":8192,"focus_pair_input_tokens":512,"maximum_context_candidates":8,"deadline_ms":120000,"p95_limit_us":120000000,"required_warm_samples":20,"quantile":"nearest_rank_95","stop_on_first_overrun":true}"#;
    assert_eq!(policy.canonical_bytes()?, expected);
    assert_eq!(
        policy.digest()?,
        framed_digest(b"airk.arbiter.resource-policy/2", &[expected])
    );
    Ok(())
}

#[test]
fn static1024_policy_keeps_original_budget_and_has_distinct_framed_identity() -> Result<()> {
    let policy = ResourcePolicy::focus_static1024();
    let expected = br#"{"schema":"airk.arbiter.resource-policy/3","shared_input_tokens":8192,"context_input_tokens":8192,"focus_pair_input_tokens":1024,"maximum_context_candidates":8,"deadline_ms":120000,"p95_limit_us":120000000,"required_warm_samples":20,"quantile":"nearest_rank_95","stop_on_first_overrun":true}"#;
    assert_eq!(policy.canonical_bytes()?, expected);
    assert_eq!(
        policy.digest()?,
        framed_digest(b"airk.arbiter.resource-policy/3", &[expected])
    );
    assert_ne!(policy.digest()?, ResourcePolicy::software_v2().digest()?);
    for mut crossed in [ResourcePolicy::software_v2(), policy] {
        crossed.focus_pair_input_tokens = if crossed.focus_pair_input_tokens == 512 {
            1024
        } else {
            512
        };
        assert!(crossed.digest().is_err());
    }
    Ok(())
}

#[test]
fn changed_resource_limits_and_historical_policy_are_not_software_v2() -> Result<()> {
    for (field, replacement) in [
        (
            "schema",
            serde_json::json!("airk.arbiter.resource-policy/1"),
        ),
        ("shared_input_tokens", serde_json::json!(2048)),
        ("context_input_tokens", serde_json::json!(8191)),
        ("focus_pair_input_tokens", serde_json::json!(2048)),
        ("maximum_context_candidates", serde_json::json!(7)),
        ("deadline_ms", serde_json::json!(30_000)),
        ("p95_limit_us", serde_json::json!(120_000_001)),
        ("required_warm_samples", serde_json::json!(19)),
        ("stop_on_first_overrun", serde_json::json!(false)),
    ] {
        let mut value = serde_json::to_value(ResourcePolicy::software_v2())?;
        value[field] = replacement;
        let changed: ResourcePolicy = serde_json::from_value(value)?;
        assert!(changed.validate().is_err(), "{field}");
        assert!(changed.digest().is_err(), "{field}");
    }
    Ok(())
}

#[test]
fn resource_series_only_emits_nearest_rank_p95_after_twenty_valid_observations() -> Result<()> {
    let policy = ResourcePolicy::software_v2();
    let mut series = ResourceSeries::new(100)?;
    for index in 1..=20 {
        series.begin_sample(&policy)?;
        series.finish_sample(
            ResourceSample {
                elapsed_us: index * 1_000_000,
                peak_process_rss_bytes: 100,
            },
            &policy,
        )?;
        if index < 20 {
            assert!(!series.complete);
            assert!(series.nearest_rank_p95_us.is_none());
            assert!(series.maximum_us.is_none());
            assert!(series.require_complete(&policy).is_err());
        }
    }
    series.require_complete(&policy)?;
    assert_eq!(series.attempted_samples, 20);
    assert_eq!(series.nearest_rank_p95_us, Some(19_000_000));
    assert_eq!(series.maximum_us, Some(20_000_000));
    let original = series.clone();
    assert!(series.begin_sample(&policy).is_err());
    assert_eq!(series, original);
    series.nearest_rank_p95_us = Some(18_000_000);
    assert!(series.validate(&policy).is_err());
    Ok(())
}

#[test]
fn first_deadline_overrun_is_preserved_without_partial_quantiles_or_further_admission() -> Result<()>
{
    let policy = ResourcePolicy::software_v2();
    let mut series = ResourceSeries::new(100)?;
    series.begin_sample(&policy)?;
    series.finish_sample(
        ResourceSample {
            elapsed_us: 120_000_000,
            peak_process_rss_bytes: 99,
        },
        &policy,
    )?;
    series.begin_sample(&policy)?;
    assert!(
        series
            .finish_sample(
                ResourceSample {
                    elapsed_us: 120_000_001,
                    peak_process_rss_bytes: 99
                },
                &policy
            )
            .is_err()
    );
    series.validate(&policy)?;
    assert_eq!(series.failure, Some(ResourceFailure::DeadlineOverrun));
    assert_eq!(series.attempted_samples, 2);
    assert_eq!(series.samples.len(), 2);
    assert_eq!(series.samples[1].elapsed_us, 120_000_001);
    assert!(!series.complete);
    assert!(series.nearest_rank_p95_us.is_none());
    assert!(series.maximum_us.is_none());
    let original = series.clone();
    assert!(series.begin_sample(&policy).is_err());
    assert_eq!(series, original);
    // A forged later sample cannot hide the first failure.
    series.samples.push(ResourceSample {
        elapsed_us: 1,
        peak_process_rss_bytes: 1,
    });
    series.attempted_samples += 1;
    assert!(series.validate(&policy).is_err());
    Ok(())
}

#[test]
fn rss_overrun_and_unknown_native_failure_have_different_observation_evidence() -> Result<()> {
    let policy = ResourcePolicy::software_v2();
    let mut rss = ResourceSeries::new(100)?;
    rss.begin_sample(&policy)?;
    assert!(
        rss.finish_sample(
            ResourceSample {
                elapsed_us: 1,
                peak_process_rss_bytes: 101
            },
            &policy
        )
        .is_err()
    );
    rss.validate(&policy)?;
    assert_eq!(rss.failure, Some(ResourceFailure::RssOverrun));
    assert_eq!(rss.samples.len(), 1);
    let mut unknown = ResourceSeries::new(100)?;
    unknown.begin_sample(&policy)?;
    unknown.fail_native(&policy)?;
    assert_eq!(unknown.failure, Some(ResourceFailure::NativeFailure));
    assert_eq!(unknown.attempted_samples, 1);
    assert!(unknown.samples.is_empty());
    assert!(unknown.nearest_rank_p95_us.is_none());
    assert!(unknown.require_complete(&policy).is_err());
    unknown.samples.push(ResourceSample {
        elapsed_us: 0,
        peak_process_rss_bytes: 0,
    });
    assert!(unknown.validate(&policy).is_err());
    Ok(())
}

#[test]
fn unadmitted_or_repeated_native_completion_cannot_create_measurements() -> Result<()> {
    let policy = ResourcePolicy::software_v2();
    let mut series = ResourceSeries::new(100)?;
    assert!(
        series
            .finish_sample(
                ResourceSample {
                    elapsed_us: 1,
                    peak_process_rss_bytes: 1
                },
                &policy
            )
            .is_err()
    );
    assert!(series.fail_native(&policy).is_err());
    series.begin_sample(&policy)?;
    assert!(series.begin_sample(&policy).is_err());
    series.finish_sample(
        ResourceSample {
            elapsed_us: 1,
            peak_process_rss_bytes: 1,
        },
        &policy,
    )?;
    assert!(
        series
            .finish_sample(
                ResourceSample {
                    elapsed_us: 1,
                    peak_process_rss_bytes: 1
                },
                &policy
            )
            .is_err()
    );
    assert_eq!(series.attempted_samples, 1);
    assert_eq!(series.samples.len(), 1);
    Ok(())
}

#[test]
fn resource_request_reuses_native_policy_and_requires_exact_pins_and_bounded_paths() -> Result<()> {
    let root = std::env::current_dir()?;
    let pin = ResourceFilePin {
        path: root.join("input.json").to_string_lossy().into_owned(),
        sha256: Sha256Digest::from_bytes([1; 32]),
    };
    let policy = ResourcePolicy::software_v2();
    let request = ResourceRunInput {
        schema: "airk.arbiter.resource-run/2".into(),
        policy_digest: policy.digest()?,
        policy,
        source_manifest: pin.clone(),
        profile: pin.clone(),
        files: BTreeMap::from([("model.onnx".into(), pin.path.clone())]),
        numeric_fixture: None,
    };
    request.validate()?;
    let mut changed = request.clone();
    changed.policy_digest = Sha256Digest::from_bytes([0; 32]);
    assert!(changed.validate().is_err());
    let mut changed = request.clone();
    changed.profile.path = "relative/profile.json".into();
    assert!(changed.validate().is_err());
    let mut changed = request.clone();
    changed.files.clear();
    assert!(changed.validate().is_err());
    let mut changed = request;
    changed.numeric_fixture = Some(ResourceFilePin {
        path: "relative.json".into(),
        ..pin
    });
    assert!(changed.validate().is_err());
    Ok(())
}
