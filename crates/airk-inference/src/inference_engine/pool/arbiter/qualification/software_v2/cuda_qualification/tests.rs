// SPDX-License-Identifier: Apache-2.0
use super::*;
fn bounds() -> GpuProbeReservation {
    GpuProbeReservation {
        schema: "airk.arbiter.gpu-probe-reservation/1".into(),
        host_limit_bytes: 16 << 30,
        device_limit_bytes: 24 << 30,
        deadline_ms: 120000,
    }
}
#[test]
fn unqualified_component_bounds_do_not_replace_observed_capacity_or_host_charge() -> Result<()> {
    let b = bounds();
    let plan = bounds_validate(&b, 12 << 30, 1 << 30, 2 << 30, 24 << 30, 16 << 30)?;
    assert_eq!(plan.cold_peak_bytes(), 12 << 30);
    assert!(bounds_validate(&b, 24 << 30, 1 << 30, 2 << 30, 24 << 30, 16 << 30).is_err());
    assert!(bounds_validate(&b, 12 << 30, 13 << 30, 2 << 30, 24 << 30, 16 << 30).is_err());
    assert!(bounds_validate(&b, 12 << 30, 1 << 30, 2 << 30, 25 << 30, 16 << 30).is_err());
    let mut bad = b;
    bad.deadline_ms = 120001;
    assert!(bounds_validate(&bad, 12 << 30, 1 << 30, 2 << 30, 24 << 30, 16 << 30).is_err());
    Ok(())
}
#[test]
fn native_component_requests_deny_unknown_fields_and_do_not_deserialize_runtime_witnesses() {
    let raw = r#"{"kind":"assembly","schema":"airk.private.cuda-component-probe-request/1","assembly":{"directory":"C:/private","producer_name":"receipt.json","producer_sha256":"2222222222222222222222222222222222222222222222222222222222222222","module_names":[],"load_order":[],"system_imports":[]},"deadline_ms":120000}"#;
    assert!(serde_json::from_str::<Request>(raw).is_ok());
    assert!(
        serde_json::from_str::<Request>(&raw.replace(
            "\"deadline_ms\":120000",
            "\"deadline_ms\":120000,\"qualified\":true"
        ))
        .is_err()
    );
}

fn fixture(profile: &Profile, widths: &[usize]) -> NumericFixture {
    NumericFixture {
        schema: "airk.arbiter.native-numeric/2".into(),
        model: profile.model,
        compiler: profile.compiler,
        resource_policy: Sha256Digest::from_bytes([1; 32]),
        reference_producer: None,
        original_reference_assets: vec![],
        rows: widths
            .iter()
            .enumerate()
            .map(|(index, &tokens)| NumericCase {
                name: format!("literal-{index}"),
                input: CompiledInput {
                    prompt: String::new(),
                    input_ids: vec![0; tokens],
                    attention_mask: vec![1; tokens],
                    position_ids: (0..tokens as i64).collect(),
                    decision_position: tokens as i64 - 1,
                    candidate_positions: vec![32, 33],
                    option_mask: vec![true; 2],
                    input_digest: Sha256Digest::from_bytes([2; 32]),
                },
                reference_same_artifact: None,
                reference_original_fp32: None,
            })
            .collect(),
    }
}
#[test]
fn component_selector_keeps_complete_original_denominator_and_literal_widths() -> Result<()> {
    let profiles = airk_retrieval::arbiter::runtime::catalog_profiles()?;
    let focus = profiles
        .iter()
        .find(|v| v.model == ModelChoice::FocusQwenReranker06B)
        .unwrap();
    let context = profiles
        .iter()
        .find(|v| v.model == ModelChoice::ContextQwenCompact)
        .unwrap();
    let rows = fixture(context, &[17, 128, 8192]);
    assert_eq!(selected_rows(CaseKind::Context17, context, &rows)?, [0]);
    assert_eq!(selected_rows(CaseKind::Context128, context, &rows)?, [1]);
    assert!(selected_rows(CaseKind::Focus512, context, &rows).is_err());
    let rows = fixture(focus, &[512; 12]);
    assert_eq!(
        selected_rows(CaseKind::Focus512, focus, &rows)?,
        (0..12).collect::<Vec<_>>()
    );
    assert!(selected_rows(CaseKind::Context17, focus, &rows).is_err());
    assert!(selected_rows(CaseKind::Focus512, focus, &fixture(focus, &[512; 11])).is_err());
    assert!(
        selected_rows(
            CaseKind::Context128,
            context,
            &fixture(context, &[17, 127, 8192])
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn final_host_peak_refuses_late_growth_and_missing_persistent_observation() -> Result<()> {
    assert_eq!(final_host_peak(Some(7), 10)?, 7);
    assert!(
        final_host_peak(Some(11), 10).is_err(),
        "a passing early row must not hide late posthash growth"
    );
    assert!(final_host_peak(None, 10).is_err());
    assert!(final_host_peak(Some(0), 10).is_err());
    assert_eq!(final_host_peak(Some(10), 10)?, 10);
    Ok(())
}
