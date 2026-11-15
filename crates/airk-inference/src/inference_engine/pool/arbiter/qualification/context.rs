// SPDX-License-Identifier: Apache-2.0
//! Explicit selected Context numeric/resource qualification; never release admission.
use super::{peak_rss, private_control};
use airk_core::{
    arbiter::{Asset, AssetRole, ModelChoice, Profile, Stage, Task},
    identity::{Sha256Digest, framed_digest},
};
use airk_retrieval::arbiter::{
    compiler::{Candidate, CompiledInput},
    runtime::{AssetSource, ExecutionProfile, Scorer},
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ContextInput {
    profile: PathBuf,
    profile_sha256: Sha256Digest,
    files: BTreeMap<String, PathBuf>,
    fixtures: PathBuf,
}
struct ContextFiles(BTreeMap<String, PathBuf>);
impl AssetSource for ContextFiles {
    fn read_asset(&self, asset: &Asset) -> Result<Vec<u8>> {
        let path = self.0.get(&asset.name).context("unmapped native asset")?;
        let file = std::fs::File::open(path)?;
        ensure!(
            file.metadata()?.len() == asset.bytes,
            "actual native asset length differs"
        );
        let mut bytes = Vec::new();
        file.take(asset.bytes + 1).read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() as u64 == asset.bytes,
            "native asset changed during bounded read"
        );
        Ok(bytes)
    }
}
fn small(path: &Path, maximum: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(maximum + 1)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() as u64 <= maximum, "private metadata bound");
    Ok(bytes)
}
fn sha(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::from_bytes(Sha256::digest(bytes).into())
}
fn verify_files(source: &ContextFiles, profile: &Profile) -> Result<()> {
    for asset in &profile.assets {
        let path = source.0.get(&asset.name).context("complete asset map")?;
        let metadata = std::fs::symlink_metadata(path)?;
        ensure!(
            metadata.is_file()
                && !metadata.file_type().is_symlink()
                && metadata.len() == asset.bytes,
            "actual asset type/length differs"
        );
        let mut reader = std::fs::File::open(path)?.take(asset.bytes + 1);
        let mut buffer = vec![0_u8; 4 << 20];
        let mut total = 0_u64;
        let mut h = Sha256::new();
        loop {
            let n = reader.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            total += n as u64;
            h.update(&buffer[..n]);
        }
        ensure!(
            total == asset.bytes && Sha256Digest::from_bytes(h.finalize().into()) == asset.sha256,
            "actual native asset bytes differ"
        );
    }
    Ok(())
}
fn diagnostic(row: &Value, fixture: Sha256Digest) -> Result<CompiledInput> {
    let input = &row["input"];
    Ok(CompiledInput {
        prompt: String::new(),
        input_ids: serde_json::from_value(input["input_ids"].clone())?,
        attention_mask: serde_json::from_value(input["attention_mask"].clone())?,
        position_ids: serde_json::from_value(input["position_ids"].clone())?,
        decision_position: input["decision_position"].as_i64().context("decision")?,
        candidate_positions: serde_json::from_value(input["candidate_positions"].clone())?,
        option_mask: serde_json::from_value(input["option_mask"].clone())?,
        input_digest: framed_digest(
            b"airk.private-context-numeric-probe/1",
            &[fixture.as_bytes(), &serde_json::to_vec(row)?],
        ),
    })
}
fn winner(values: &[f32]) -> usize {
    values.iter().enumerate().fold(
        0,
        |best, (index, value)| if *value > values[best] { index } else { best },
    )
}
fn maximum_native(scorer: &Scorer) -> Result<CompiledInput> {
    let options = (0..8)
        .map(|i| Candidate {
            id: format!("synthetic-card-{i}"),
            text: format!("Synthetic closed option {i}; no task truth or source authority."),
        })
        .collect::<Vec<_>>();
    let mut low = 0;
    let mut high = 4096;
    let mut largest = None;
    while low <= high {
        let mid = low + (high - low) / 2;
        match scorer.compile(
            &" source".repeat(mid),
            "Select one complete synthetic option.",
            &options,
        ) {
            Ok(input) => {
                largest = Some(input);
                low = mid + 1
            }
            Err(_) => {
                if mid == 0 {
                    break;
                }
                high = mid - 1;
            }
        }
    }
    let input = largest.context("maximum native compiler probe")?;
    ensure!(
        input.input_ids.len() == 2048
            && input.candidate_positions.len() == 8
            && input.option_mask == vec![true; 8],
        "actual nativeT2048/N8/all-live required; no shorter substitution"
    );
    Ok(input)
}
/// Record every observation before refusing the first known resource failure.
/// The injected observer is the actual forward+clock+RSS boundary in production.
fn measured_resource_samples(
    resource: &mut Value,
    reservation: u64,
    mut observe: impl FnMut() -> Result<(u128, u64)>,
) -> Result<()> {
    let mut samples = Vec::new();
    resource["measurement_complete"] = json!(false);
    resource["required_samples"] = json!(20);
    resource["candidate_numeric_resource_passed"] = json!(false);
    for index in 0..20 {
        resource["attempted_samples"] = json!(index + 1);
        let (elapsed, peak) = observe()?;
        samples.push(elapsed);
        resource["samples_us"] = json!(samples);
        resource["completed_samples"] = json!(samples.len());
        resource["peak_process_lifetime_rss_bytes"] = json!(peak);
        resource["candidate_reservation_bytes"] = json!(reservation);
        resource["original_product_deadline_us"] = json!(30_000_000_u128);
        ensure!(
            elapsed <= 30_000_000,
            "first measured call exceeded original30s deadline; partial samples retained"
        );
        ensure!(
            peak <= reservation,
            "first observed RSS exceeded candidate reservation; partial samples retained"
        );
    }
    let mut sorted = samples;
    sorted.sort_unstable();
    resource["nearest_rank_p95_us"] = json!(sorted[18]);
    resource["maximum_us"] = json!(sorted[19]);
    resource["remaining_product_budget_after_context_p95_us"] =
        json!(30_000_000_u128.saturating_sub(sorted[18]));
    resource["measurement_complete"] = json!(true);
    resource["candidate_numeric_resource_passed"] = json!(true);
    Ok(())
}

#[test]
fn context_resource_observer_stops_at_first_overrun_with_truthful_partial_samples() {
    let mut calls = 0;
    let mut receipt = json!({});
    let result = measured_resource_samples(&mut receipt, 100, || {
        calls += 1;
        Ok((if calls == 2 { 30_000_001 } else { 1 }, 50))
    });
    assert!(result.is_err());
    assert_eq!(calls, 2);
    assert_eq!(receipt["samples_us"], json!([1, 30_000_001]));
    assert_eq!(receipt["completed_samples"], 2);
    assert_eq!(receipt["measurement_complete"], false);
    assert_eq!(receipt["candidate_numeric_resource_passed"], false);
    assert!(receipt.get("nearest_rank_p95_us").is_none());
    assert!(receipt.get("maximum_us").is_none());
}

#[test]
fn context_resource_observer_stops_at_first_rss_failure_and_completes_only_twenty_valid_samples()
-> Result<()> {
    let mut calls = 0;
    let mut failed = json!({});
    assert!(
        measured_resource_samples(&mut failed, 100, || {
            calls += 1;
            Ok((1, 101))
        })
        .is_err()
    );
    assert_eq!(calls, 1);
    assert_eq!(failed["completed_samples"], 1);
    assert_eq!(failed["measurement_complete"], false);
    let mut complete = json!({});
    measured_resource_samples(&mut complete, 100, || Ok((30_000_000, 100)))?;
    assert_eq!(complete["completed_samples"], 20);
    assert_eq!(complete["measurement_complete"], true);
    assert_eq!(complete["candidate_numeric_resource_passed"], true);
    assert_eq!(complete["nearest_rank_p95_us"], 30_000_000);
    assert_eq!(complete["maximum_us"], 30_000_000);
    Ok(())
}

#[test]
#[ignore = "actual byte-identical packed Context FP32 graph, reviewed same-model all-live numeric probes and Root-owned NN resource window required"]
fn selected_context_fp32_native_numeric_and_resource_candidate() -> Result<()> {
    let input_path = PathBuf::from(std::env::var("AIRK_CONTEXT_NATIVE_INPUT")?);
    let expected: Sha256Digest = std::env::var("AIRK_CONTEXT_NATIVE_INPUT_SHA256")?.parse()?;
    let input_bytes = small(&input_path, 1 << 20)?;
    ensure!(
        sha(&input_bytes) == expected,
        "private input descriptor changed"
    );
    let input: ContextInput = serde_json::from_slice(&input_bytes)?;
    let profile_bytes = small(&input.profile, 1 << 20)?;
    ensure!(
        sha(&profile_bytes) == input.profile_sha256,
        "Rust-emitted profile bytes changed"
    );
    let profile: Profile = serde_json::from_slice(&profile_bytes)?;
    profile.validate()?;
    let (tier, reservation) = match profile.model {
        ModelChoice::ContextQwenCompact => (4, 16_u64 << 30),
        ModelChoice::ContextQwenLarge => (16, 40_u64 << 30),
        _ => anyhow::bail!("only selected Context models"),
    };
    let fit = profile
        .cpu_fit
        .as_ref()
        .context("explicit candidateCpuFit")?;
    fit.validate()?;
    ensure!(
        fit.minimum_physical_cores == tier
            && fit.intra_threads == tier
            && profile.memory_reservation_bytes == reservation
            && profile.stage == Some(Stage::Context)
            && profile.tasks == BTreeSet::from([Task::ContextAction])
            && profile.max_tokens == 2048
            && profile.max_candidates == 8
            && profile.qualification.is_none()
            && profile.qualified_targets.is_empty()
            && profile.downloads.is_empty(),
        "exact declared unqualified candidate required"
    );
    ensure!(
        input.files.keys().collect::<BTreeSet<_>>()
            == profile
                .assets
                .iter()
                .map(|asset| &asset.name)
                .collect::<BTreeSet<_>>(),
        "exact native asset overlay required"
    );
    let fixture_bytes = small(&input.fixtures, 1 << 20)?;
    let fixture_hash = sha(&fixture_bytes);
    ensure!(
        profile
            .assets
            .iter()
            .any(|asset| asset.role == AssetRole::Fixture
                && asset.bytes == fixture_bytes.len() as u64
                && asset.sha256 == fixture_hash),
        "numeric fixture is not authenticated"
    );
    let fixture: Value = serde_json::from_slice(&fixture_bytes)?;
    let rows = fixture["rows"].as_array().context("all-live probes")?;
    ensure!(rows.len() == 3, "all three numeric shapes required");
    let output = PathBuf::from(std::env::var("AIRK_CONTEXT_NATIVE_OUTPUT")?);
    ensure!(!output.exists(), "fresh private native output required");
    let source = ContextFiles(input.files);
    let (_parent, control) = private_control()?;
    let lease = Arc::new(airk_store::service::ServiceLease::acquire(&control)?);
    let owner = airk_kernel::inference_owner::NativeInferenceOwner::admit(lease)?;
    owner.run(||->Result<()>{
  verify_files(&source,&profile)?;let mut receipt=json!({"schema":"airk.private-context-native-numeric-resource/1","complete":false,"profile":profile.manifest,"profile_source_sha256":input.profile_sha256,"fixture_sha256":fixture_hash,"model":profile.model,"candidate_cpu_fit":fit,"candidate_memory_reservation_bytes":reservation,"numeric_rows":[],"semantic_or_CAL_or_publication_qualified":false,"combined_service_admission_verified":false,"scope":"Actual service-lease compute owner and nativeORT numeric/resource candidate only. No semantic labels or fitted actions. Actual composed service admission and original30s budget remain required beforefit."});
  let run=(||->Result<()>{let start=Instant::now();let scorer=Scorer::open(ExecutionProfile::for_qualification(profile.clone())?,&source)?;receipt["load_us"]=json!(start.elapsed().as_micros());receipt["execution_identity"]=serde_json::to_value(scorer.execution_identity()?)?;
   for (index,row) in rows.iter().enumerate(){let compiled=diagnostic(row,fixture_hash)?;let (tokens,options)=[(17,3),(128,8),(2048,8)][index];ensure!(compiled.input_ids.len()==tokens&&compiled.candidate_positions.len()==options&&compiled.option_mask==vec![true;options],"actual all-live shape differs");let expected:Vec<f32>=serde_json::from_value(row["python_fp32_known_live"].clone())?;let original:Vec<f32>=serde_json::from_value(row["original_fp32_all_live"].clone())?;
    let start=Instant::now();let actual=scorer.score(&compiled)?;let us=start.elapsed().as_micros();ensure!(actual.len()==expected.len()&&actual.len()==original.len(),"native numeric output cardinality");let known_mask:Vec<bool>=serde_json::from_value(row["known_live_mask"].clone())?;ensure!(known_mask.len()==actual.len()&&known_mask.iter().any(|x|*x),"known-live reference mask");let actual_known=actual.iter().zip(&known_mask).filter_map(|(v,live)|live.then_some(*v)).collect::<Vec<_>>();let expected_known=expected.iter().zip(&known_mask).filter_map(|(v,live)|live.then_some(*v)).collect::<Vec<_>>();let err=actual_known.iter().zip(&expected_known).map(|(a,b)|(a-b).abs()).fold(0_f32,f32::max);let original_err=actual.iter().zip(&original).map(|(a,b)|(a-b).abs()).fold(0_f32,f32::max);let same_winner=winner(&actual_known)==winner(&expected_known)&&winner(&actual)==winner(&original);
    receipt["numeric_rows"].as_array_mut().context("receipt rows")?.push(json!({"tokens":tokens,"options":options,"all_live":true,"native_python_fp32_max_error":err,"native_original_fp32_max_error":original_err,"inference_us":us,"input_digest":compiled.input_digest,"argmax_matches":same_winner,"raw_scores":actual}));ensure!(err<=0.001&&original_err<=0.001&&same_winner,"frozen native numeric tolerance exceeded");
   }
   let compiled=maximum_native(&scorer)?;let start=Instant::now();scorer.score(&compiled)?;receipt["warmup_us"]=json!(start.elapsed().as_micros());
   receipt["resource"]=json!({"input":compiled,"scope":"Actual nativecompiledT2048/N8/all-live measurements; stop immediately after first measured30s/RSS failure, retain partial samples; p95/max only after all20valid calls. Not combinedFocus/service-path fit or producthardware floor."});
   measured_resource_samples(&mut receipt["resource"],reservation,||{let start=Instant::now();scorer.score(&compiled)?;Ok((start.elapsed().as_micros(),peak_rss().context("actual process peakRSS unavailable")?))})?;
   verify_files(&source,&profile)?;receipt["complete"]=json!(true);Ok(())})();
  if let Err(error)=&run{receipt["error"]=json!(error.to_string());}std::fs::write(&output,serde_json::to_vec_pretty(&receipt)?)?;run
 })??;
    Ok(())
}
