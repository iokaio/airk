// SPDX-License-Identifier: Apache-2.0
use super::*;

#[test]
fn software_v2_observer_stops_first_failure_and_retains_it_before_return() -> Result<()> {
    let policy = ResourcePolicy::software_v2();
    let mut series = ResourceSeries::new(100)?;
    let (mut calls, mut retained) = (0, Vec::new());
    let result = observe_series(
        &mut series,
        &policy,
        || {
            calls += 1;
            Ok(ResourceSample {
                elapsed_us: if calls == 2 { 120_000_001 } else { 1 },
                peak_process_rss_bytes: 100,
            })
        },
        |value| {
            retained.push(value.clone());
            Ok(())
        },
    );
    assert!(result.is_err());
    assert_eq!(calls, 2);
    assert_eq!(retained.len(), 2);
    assert_eq!(retained[1].samples[1].elapsed_us, 120_000_001);
    assert_eq!(series.failure, Some(ResourceFailure::DeadlineOverrun));
    assert!(series.nearest_rank_p95_us.is_none());
    assert!(series.maximum_us.is_none());
    Ok(())
}
#[test]
fn software_v2_native_error_has_an_attempt_without_a_fabricated_sample() -> Result<()> {
    let policy = ResourcePolicy::software_v2();
    let mut series = ResourceSeries::new(100)?;
    let mut retained = None;
    assert!(
        observe_series(
            &mut series,
            &policy,
            || Err(anyhow::anyhow!("actual native failure")),
            |value| {
                retained = Some(value.clone());
                Ok(())
            }
        )
        .is_err()
    );
    let retained = retained.context("failure not retained")?;
    assert_eq!(retained.attempted_samples, 1);
    assert!(retained.samples.is_empty());
    assert_eq!(retained.failure, Some(ResourceFailure::NativeFailure));
    assert!(!retained.complete);
    Ok(())
}
#[test]
fn software_v2_complete_twenty_boundary_samples_have_literal_nearest_rank_statistics() -> Result<()>
{
    let policy = ResourcePolicy::software_v2();
    let mut series = ResourceSeries::new(100)?;
    let mut retained = 0;
    observe_series(
        &mut series,
        &policy,
        || {
            Ok(ResourceSample {
                elapsed_us: 120_000_000,
                peak_process_rss_bytes: 100,
            })
        },
        |_| {
            retained += 1;
            Ok(())
        },
    )?;
    assert_eq!(retained, 20);
    assert_eq!(series.samples.len(), 20);
    assert!(series.complete);
    assert_eq!(series.nearest_rank_p95_us, Some(120_000_000));
    assert_eq!(series.maximum_us, Some(120_000_000));
    Ok(())
}
#[test]
fn software_v2_numeric_comparison_refuses_nonfinite_cardinality_and_keeps_argmax() -> Result<()> {
    assert_eq!(comparison(&[1., 2.], &[1., 2.])?, (0., 0., true));
    let (raw, mass, same) = comparison(&[1., 2.], &[2., 1.])?;
    assert_eq!(raw, 1.);
    assert!(mass > 0.4);
    assert!(!same);
    assert!(comparison(&[f32::NAN, 1.], &[1., 2.]).is_err());
    assert!(comparison(&[1.], &[1., 2.]).is_err());
    assert!(comparison(&[], &[]).is_err());
    Ok(())
}

#[test]
fn context_diagnostic_32_binds_all_live_tensors_and_compiler() -> Result<()> {
    let compiler = Sha256Digest::from_bytes([7; 32]);
    let symbols = (32..40).collect::<Vec<_>>();
    let input = context_diagnostic_32(compiler, symbols.clone())?;
    assert!(input.prompt.is_empty());
    assert_eq!(input.input_ids, (1..33).collect::<Vec<_>>());
    assert_eq!(input.attention_mask, [1; 32]);
    assert_eq!(input.position_ids, (0..32).collect::<Vec<_>>());
    assert_eq!(input.decision_position, 31);
    assert_eq!(input.candidate_positions, [32, 33, 34, 35, 36, 37, 38, 39]);
    assert_eq!(input.option_mask, [true; 8]);
    assert_eq!(
        input.input_digest,
        context_diagnostic_32(compiler, symbols.clone())?.input_digest
    );
    assert_ne!(
        input.input_digest,
        context_diagnostic_32(Sha256Digest::from_bytes([8; 32]), symbols.clone())?.input_digest
    );
    let mut changed = symbols;
    changed.swap(0, 1);
    assert_ne!(
        input.input_digest,
        context_diagnostic_32(compiler, changed.clone())?.input_digest
    );
    changed.pop();
    assert!(context_diagnostic_32(compiler, changed).is_err());
    Ok(())
}

fn numeric_fixture(model: ModelChoice) -> Result<(Profile, NumericFixture, ResourcePolicy)> {
    let mut profile: Profile = serde_json::from_slice(&std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/arbiter/profile-downloads.json"),
    )?)?;
    profile.model = model;
    let policy = ResourcePolicy::software_v2();
    let focus = model == ModelChoice::FocusQwenReranker06B;
    let shapes = if focus {
        vec![(512, 2); 12]
    } else {
        vec![(17, 3), (128, 8), (8192, 8)]
    };
    let rows = shapes
        .into_iter()
        .enumerate()
        .map(|(index, (tokens, options))| NumericCase {
            name: format!("independent-{index}"),
            input: CompiledInput {
                prompt: String::new(),
                input_ids: vec![1; tokens],
                attention_mask: vec![1; tokens],
                position_ids: (0..tokens as i64).collect(),
                decision_position: tokens as i64 - 1,
                candidate_positions: (1..=options as i64).collect(),
                option_mask: vec![true; options],
                input_digest: Sha256Digest::from_bytes([7; 32]),
            },
            reference_same_artifact: None,
            reference_original_fp32: None,
        })
        .collect();
    let fixture = NumericFixture {
        schema: "airk.arbiter.native-numeric/2".into(),
        model: profile.model,
        compiler: profile.compiler,
        resource_policy: policy.digest()?,
        reference_producer: None,
        original_reference_assets: vec![],
        rows,
    };
    Ok((profile, fixture, policy))
}
#[test]
fn focus1024_numeric_shape_requires_fourteen_original_rows_and_selected_width() -> Result<()> {
    let (profile, mut fixture, _) = numeric_fixture(ModelChoice::FocusQwenReranker06B)?;
    let policy = ResourcePolicy::focus_static1024();
    fixture.resource_policy = policy.digest()?;
    let template = fixture.rows[0].clone();
    fixture.rows = (0..14)
        .map(|ordinal| {
            let mut row = template.clone();
            row.name = format!("independent-{ordinal}");
            row.input.input_ids = vec![1; 1024];
            row.input.input_ids[0] = 151644;
            row.input.attention_mask = vec![1; 1024];
            row.input.position_ids = (0..1024).collect();
            row.input.decision_position = 1023;
            row.input.candidate_positions = vec![2152, 9693];
            row
        })
        .collect();
    numeric_shape(&fixture, &profile, &policy)?;
    fixture.rows[0].input.input_ids.truncate(512);
    assert!(numeric_shape(&fixture, &profile, &policy).is_err());
    fixture.rows[0].input.input_ids.resize(1024, 1);
    fixture.rows.pop();
    assert!(numeric_shape(&fixture, &profile, &policy).is_err());
    Ok(())
}
#[test]
fn software_v2_numeric_shape_rejects_the_historical2048_resource_row() -> Result<()> {
    let (profile, mut fixture, policy) = numeric_fixture(ModelChoice::ContextQwenCompact)?;
    numeric_shape(&fixture, &profile, &policy)?;
    fixture.rows[2].input.input_ids.truncate(2048);
    assert!(numeric_shape(&fixture, &profile, &policy).is_err());
    Ok(())
}
#[test]
fn software_v2_numeric_context_all_live_rejects_padding_and_tensor_drift_but_focus_keeps_left_padding()
-> Result<()> {
    let (profile, mut fixture, policy) = numeric_fixture(ModelChoice::ContextQwenCompact)?;
    let original = fixture.rows[2].clone();
    for kind in 0..8 {
        fixture.rows[2] = original.clone();
        let input = &mut fixture.rows[2].input;
        match kind {
            0 => input.attention_mask[100] = 0,
            1 => {
                input.attention_mask.pop();
            }
            2 => {
                input.position_ids.pop();
            }
            3 => input.decision_position = 8192,
            4 => {
                input.candidate_positions.pop();
            }
            5 => input.option_mask[0] = false,
            6 => input.position_ids[0] = 1,
            7 => {
                input.input_ids.pop();
            }
            _ => unreachable!(),
        }
        assert!(
            numeric_shape(&fixture, &profile, &policy).is_err(),
            "malformed current numeric tensor case{kind}"
        );
    }
    let (profile, mut fixture, policy) = numeric_fixture(ModelChoice::FocusQwenReranker06B)?;
    for row in &mut fixture.rows {
        row.input.input_ids[..8].fill(151643);
        row.input.input_ids[8] = 151644;
        row.input.attention_mask[..8].fill(0);
        row.input.candidate_positions = vec![2152, 9693];
    }
    numeric_shape(&fixture, &profile, &policy)?;
    Ok(())
}

#[test]
fn software_v2_original_source_roster_refuses_duplicate_foreign_and_cross_model_pins() -> Result<()>
{
    use super::original_sources::select_sources;
    let root = std::env::current_dir()?;
    let mut pins = vec![
        ResourceFilePin {
            path: root.join("model.safetensors").to_str().unwrap().into(),
            sha256: "27cd75a405b9c1b46b59abfd88aaa209e6fed2a1972cde9b70e7659537c5e65b".parse()?,
        },
        ResourceFilePin {
            path: root.join("tokenizer.json").to_str().unwrap().into(),
            sha256: "aeb13307a71acd8fe81861d94ad54ab689df773318809eed3cbe794b4492dae4".parse()?,
        },
        ResourceFilePin {
            path: root.join("config.json").to_str().unwrap().into(),
            sha256: "d479c427a9ca5295218063d4f9aca4f297ab4ac27487cca7af42c84643d51ef0".parse()?,
        },
    ];
    assert_eq!(
        select_sources(ModelChoice::FocusQwenReranker06B, &pins)?.len(),
        3
    );
    let original = pins.clone();
    pins[1] = pins[0].clone();
    assert!(select_sources(ModelChoice::FocusQwenReranker06B, &pins).is_err());
    pins = original.clone();
    pins[0].sha256 = Sha256Digest::from_bytes([91; 32]);
    assert!(select_sources(ModelChoice::FocusQwenReranker06B, &pins).is_err());
    pins = original.clone();
    pins[0].path = root.join("foreign.safetensors").to_str().unwrap().into();
    assert!(select_sources(ModelChoice::FocusQwenReranker06B, &pins).is_err());
    assert!(select_sources(ModelChoice::ContextQwenCompact, &original).is_err());
    assert!(select_sources(ModelChoice::SmallIntern, &original).is_err());
    let role_injection = json!({"schema":"airk.private.original-numeric-source-input/1","model":ModelChoice::FocusQwenReranker06B,"files":original,"role":"compiler"});
    assert!(
        serde_json::from_value::<super::original_sources::OriginalSourceInput>(role_injection)
            .is_err()
    );
    Ok(())
}
#[test]
fn software_v2_original_source_hash_drift_refuses_before_metadata_emission() -> Result<()> {
    let root = tempfile::tempdir()?;
    let path = root.path().join("synthetic-original.bin");
    std::fs::write(&path, b"independent original bytes")?;
    let pin = ResourceFilePin {
        path: path.to_str().unwrap().into(),
        sha256: sha(b"independent original bytes"),
    };
    let actual = super::original_sources::measure_source(&pin)?;
    assert_eq!(actual["bytes"], 26);
    assert_eq!(actual["sha256"], json!(pin.sha256));
    std::fs::write(&path, b"changed original bytes")?;
    assert!(super::original_sources::measure_source(&pin).is_err());
    Ok(())
}

#[test]
fn software_v2_asset_reader_has_exact_capacity_and_preserves_complete_excess_and_eof_checks()
-> Result<()> {
    let root = tempfile::tempdir()?;
    let path = root.path().join("synthetic-asset.bin");
    let expected = vec![0x5a; (12 << 20) + 19];
    std::fs::write(&path, &expected)?;
    let actual = small(&path, 32 << 20)?;
    assert_eq!(actual, expected);
    assert_eq!(
        actual.capacity(),
        expected.len(),
        "actual length reservation must avoid geometric slack/caller maximum allocation"
    );
    let asset = Asset {
        name: "synthetic-asset.bin".into(),
        role: AssetRole::ExternalWeights,
        bytes: expected.len() as u64,
        sha256: sha(&expected),
    };
    let files = Files(BTreeMap::from([(
        asset.name.clone(),
        path.to_str().context("synthetic path")?.into(),
    )]));
    assert_eq!(files.read_asset(&asset)?, expected);
    assert!(small(&path, asset.bytes - 1).is_err());
    std::fs::write(&path, &expected[..expected.len() - 1])?;
    assert!(
        files.read_asset(&asset).is_err(),
        "short EOF remains refused by exact Asset length"
    );
    let mut excess = expected.clone();
    excess.push(0);
    std::fs::write(&path, &excess)?;
    assert!(
        files.read_asset(&asset).is_err(),
        "excess bytes remain refused before read"
    );
    std::fs::write(&path, [])?;
    let empty = small(&path, 32 << 20)?;
    assert!(empty.is_empty());
    assert_eq!(empty.capacity(), 0);
    assert!(
        small(&path, u64::MAX).is_err(),
        "overflow bound remains refused"
    );
    Ok(())
}
