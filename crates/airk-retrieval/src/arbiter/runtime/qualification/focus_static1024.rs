// SPDX-License-Identifier: Apache-2.0
//! Checked private compiler emission for the selected static1024 Focus profile.
use super::*;
use crate::arbiter::compiler::Compiler;
use std::{env, io::Write};

const TOKENIZER: &str = "aeb13307a71acd8fe81861d94ad54ab689df773318809eed3cbe794b4492dae4";
const COMPILER: &str = "2cba18a75ae105352ed9fdc94cb88812364710fcbaa6e2191a3addc524b86133";

fn original_file(root: &Path, name: &str, length: u64, expected: &str) -> Result<Vec<u8>> {
    let mut file = fs::File::open(root.join(name))?;
    ensure!(
        file.metadata()?.is_file() && file.metadata()?.len() == length,
        "original {name} length differs"
    );
    let mut bytes = Vec::new();
    (&mut file).take(length + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 == length && sha(&bytes).to_string() == expected,
        "original {name} bytes differ"
    );
    Ok(bytes)
}

fn write_new(path: PathBuf, bytes: &[u8]) -> Result<()> {
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

#[test]
#[ignore = "requires authenticated original assets and a fresh checked private output"]
fn emit_exact_native_focus_static1024_compiler() -> Result<()> {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .context("repository")?;
    let artifacts = repository.join("artifacts").canonicalize()?;
    let assets =
        PathBuf::from(env::var_os("AIRK_FOCUS1024_ORIGINAL_ASSETS").context("original assets")?)
            .canonicalize()?;
    let output =
        PathBuf::from(env::var_os("AIRK_FOCUS1024_COMPILER_OUTPUT").context("private output")?);
    ensure!(
        assets.starts_with(&artifacts)
            && output.is_absolute()
            && output
                .parent()
                .context("output parent")?
                .canonicalize()?
                .starts_with(&artifacts)
            && output.file_name().is_some()
            && !output.try_exists()?,
        "private input/output location refused"
    );
    let tokenizer = original_file(&assets, "tokenizer.json", 11_422_654, TOKENIZER)?;
    let baseline = original_file(&assets, "compiler.json", 528, COMPILER)?;
    ensure!(
        crate::arbiter::compiler::focus::compiler_profile(&tokenizer)? == baseline,
        "native static512 issuer differs"
    );
    let policy = ResourcePolicy::focus_static1024();
    let compiler = compiler_with_limits(&baseline, true, &policy)?;
    Compiler::from_verified(&compiler, &tokenizer)?;
    let receipt = serde_json::json!({"schema":"airk.private.focus-static1024-compiler/1",
        "width":1024,"original_width":512,"original_compiler":sha(&baseline),"tokenizer":sha(&tokenizer),
        "compiler":sha(&compiler),"resource_policy":policy,"resource_policy_digest":policy.digest()?,
        "native_task_policy":crate::arbiter::compiler::focus::policy_digest()?,
        "score_invocations":0,"release_qualified":false});
    fs::create_dir(&output)?;
    write_new(output.join("compiler.json"), &compiler)?;
    write_new(
        output.join("resource-policy.json"),
        &policy.canonical_bytes()?,
    )?;
    write_new(
        output.join("compiler-receipt.json"),
        &serde_json::to_vec_pretty(&receipt)?,
    )?;
    Ok(())
}

// The selected candidate derives its resource, tokenizer and notice declarations
// from the original descriptor. The graph and fixture come only from the
// independently measured 1024 export. The original static512 graph is rejected.
#[test]
#[ignore = "requires the pinned independent 1024 CPU graph and publisher controls"]
fn emit_focus_static1024_profile_from_independent_export() -> Result<()> {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .context("repository")?;
    let campaign = repository
        .join("artifacts/v190/final-plan/exec-20261116T052942Z")
        .canonicalize()?;
    let old_metadata =
        campaign.join("fresh-cu15-current-cpu-metadata-r1/arbiter-focus-qwen3-reranker-06b");
    let source =
        campaign.join("fp32-paired-answer-r1/arbiter-focus-qwen3-reranker-06b-current-assets");
    let export = campaign.join("focus1024-cpu-export-r2");
    let output = PathBuf::from(
        env::var_os("AIRK_FOCUS1024_PROFILE_OUTPUT").context("private profile output")?,
    );
    ensure!(
        output.is_absolute()
            && output.parent().context("output parent")?.canonicalize()? == campaign
            && !output.try_exists()?,
        "fresh checked candidate directory required"
    );
    let old: Profile = serde_json::from_slice(&original_file(
        &old_metadata,
        "experimental-profile.json",
        3236,
        "488a056f38b018eb3dcb800015ea5a711d2d89b953d6a2cbd25b9096261b7633",
    )?)?;
    old.validate()?;
    ensure!(
        old.model == ModelChoice::FocusQwenReranker06B
            && old.max_tokens == 512
            && old.qualification.is_none(),
        "original Focus descriptor differs"
    );
    let graph_report = original_file(
        &export,
        "graph.private.json",
        770,
        "1a6afa3ac809453ce03510f00dd191bcd840ab3a6636f78c1c1dfe151181e4d7",
    )?;
    let numerical = original_file(
        &export,
        "numeric.private.json",
        603,
        "dd6d22fb2f17bb18b05b396b5e378405da6d53872e5fcff565c46ed08c7feed4",
    )?;
    let owned = original_file(
        &export,
        "owned.json",
        1377,
        "5a4595b76225b46d395e44f6ced5e1d24ebe8007d31e889a0d7e9851e62713c0",
    )?;
    let metrics: serde_json::Value = serde_json::from_slice(&numerical)?;
    let graph: serde_json::Value = serde_json::from_slice(&graph_report)?;
    let settled: serde_json::Value = serde_json::from_slice(&owned)?;
    ensure!(
        metrics["width"] == 1024
            && metrics["reference_rows"] == 14
            && metrics["all_argmax"] == true
            && metrics["overwidth_refusal"] == 1025
            && metrics["publisher_weight8_limits_passed"] == true
            && graph["width"] == 1024
            && graph["graph"] == metrics["graph_sha256"]
            && settled["exit_code"] == 0
            && settled["owned_job_final_active_processes"] == 0,
        "independent 1024 export or native Job closure differs"
    );
    let mut inputs = Vec::new();
    for mut asset in old.assets.iter().cloned() {
        let path = match asset.role {
            AssetRole::Compiler | AssetRole::Calibration => continue,
            AssetRole::Model => {
                asset.bytes = 5_046_780;
                asset.sha256 = serde_json::from_value(metrics["graph_sha256"].clone())?;
                export.join("assets").join(&asset.name)
            }
            AssetRole::Fixture => {
                asset.name = "native-runtime1024-fixture.json".into();
                asset.bytes = 703_130;
                asset.sha256 = serde_json::from_value(metrics["fixture_sha256"].clone())?;
                export.join(&asset.name)
            }
            AssetRole::ExternalWeights => {
                ensure!(
                    serde_json::to_value(asset.sha256)? == graph["external_weights"],
                    "external weights changed"
                );
                export.join("assets").join(&asset.name)
            }
            _ => source.join(&asset.name),
        };
        inputs.push(AssetInput { asset, path });
    }
    let policy = ResourcePolicy::focus_static1024();
    let prepared = prepare_declared(
        ProfileInput {
            model: old.model,
            profile_id: "private-focus-static1024-cpu".into(),
            assets: inputs,
            cpu_fit: old.cpu_fit.context("original CPU fit")?,
            maximum_candidates: old.max_candidates,
            memory_reservation_bytes: old.memory_reservation_bytes,
            peak_disk_bytes: old.peak_disk_bytes,
            calibration: CalibrationInput::NumericRawOnly,
        },
        &output,
        Some(&policy),
    )?;
    ensure!(
        prepared.profile.max_tokens == 1024
            && prepared.profile.manifest != old.manifest
            && prepared.profile.qualification.is_none(),
        "candidate descriptor failed width/identity checks"
    );
    let fixture = prepared
        .files
        .get("native-runtime1024-fixture.json")
        .context("native 1024 fixture")?;
    let request = serde_json::json!({"profile":output.join("experimental-profile.json"),
        "files":prepared.files, "fixtures":fixture});
    write_new(
        output.join("qualification-input.json"),
        &serde_json::to_vec_pretty(&request)?,
    )?;
    let receipt = serde_json::json!({"schema":"airk.private.native-focus1024-bootstrap/1",
        "width":1024,"manifest":prepared.profile.manifest,"compiler":prepared.profile.compiler,
        "resource_policy_digest":policy.digest()?,"independent_numeric_sha256":sha(&numerical),
        "export_graph_receipt_sha256":sha(&graph_report),"export_owner_sha256":sha(&owned),
        "original_manifest":old.manifest,"release_qualified":false});
    write_new(
        output.join("bootstrap-receipt.json"),
        &serde_json::to_vec_pretty(&receipt)?,
    )?;
    Ok(())
}
