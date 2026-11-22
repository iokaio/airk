// SPDX-License-Identifier: Apache-2.0
//! Private Answer evaluation only: emit a trained Focus candidate's catalog-only
//! profile and check that the Python example compiler matches the native one.
//!
//! The candidate is the change-set-1 width-16 SoftwareEvidenceV1 profile with
//! new trained weights. Every asset is authenticated from the candidate's
//! private asset folder; only the weights, calibration metadata, identity and
//! name differ from the source profile. Ignored: it needs private inputs.
//!
//! ```text
//! AIRK_FOCUS_CANDIDATE_SOURCE_PROFILE   source width-16 profile JSON
//! AIRK_FOCUS_CANDIDATE_SOURCE_SHA256    its SHA-256
//! AIRK_FOCUS_CANDIDATE_ASSETS           candidate asset folder
//! AIRK_FOCUS_CANDIDATE_ID               new profile id (starts with private-)
//! AIRK_FOCUS_CANDIDATE_PARITY           JSONL rows {query, document, live_ids}
//! AIRK_FOCUS_CANDIDATE_OUTPUT           fresh output folder
//! ```
#![cfg(all(feature = "test-support", feature = "local-inference"))]
use airk_core::{
    arbiter::{AssetRole, ModelChoice, Profile, Task},
    identity::{Sha256Digest, framed_digest},
};
use airk_retrieval::arbiter::compiler::{Candidate, Compiler, focus::experiment};
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

fn sha(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::from_bytes(Sha256::digest(bytes).into())
}

fn env(name: &str) -> Result<String> {
    std::env::var(name).with_context(|| format!("{name} missing"))
}

/// The candidate's identity: its source profile, its name and its content.
fn candidate_manifest(source: Sha256Digest, profile: &Profile) -> Result<Sha256Digest> {
    let mut preimage = profile.clone();
    preimage.manifest = Sha256Digest::from_bytes([0; 32]);
    Ok(framed_digest(
        b"airk.private.answer-focus-candidate-profile/1",
        &[
            source.as_bytes(),
            profile.profile_id.as_bytes(),
            &serde_json::to_vec(&preimage)?,
        ],
    ))
}

#[derive(serde::Deserialize)]
struct ParityRow {
    query: String,
    document: String,
    live_ids: Vec<i64>,
}

#[test]
#[ignore = "private Focus candidate emitter; private assets and a fresh output folder required"]
fn emit_private_focus_candidate_profile() -> Result<()> {
    let source_path = PathBuf::from(env("AIRK_FOCUS_CANDIDATE_SOURCE_PROFILE")?);
    let source_expected: Sha256Digest = env("AIRK_FOCUS_CANDIDATE_SOURCE_SHA256")?.parse()?;
    let assets = PathBuf::from(env("AIRK_FOCUS_CANDIDATE_ASSETS")?).canonicalize()?;
    let profile_id = env("AIRK_FOCUS_CANDIDATE_ID")?;
    let parity = PathBuf::from(env("AIRK_FOCUS_CANDIDATE_PARITY")?);
    let output = PathBuf::from(env("AIRK_FOCUS_CANDIDATE_OUTPUT")?);
    ensure!(
        output.is_absolute() && !output.try_exists()? && profile_id.starts_with("private-"),
        "fresh absolute output and a private profile id required"
    );
    let raw = std::fs::read(&source_path)?;
    ensure!(sha(&raw) == source_expected, "source profile bytes differ");
    let source: Profile = serde_json::from_slice(&raw)?;
    source.validate()?;
    let software = experiment::software_policy_digest()?;
    ensure!(
        source.model == ModelChoice::FocusQwenReranker06B
            && source.max_candidates == 16
            && source.max_tokens == 1024
            && source.policy == software
            && source.task_policies.get(&Task::FocusRank) == Some(&software)
            && source.qualification.is_none()
            && source.device_execution.is_none(),
        "source must be the width-16 SoftwareEvidenceV1 static-1024 CPU Focus profile"
    );
    let mut profile = source.clone();
    let mut changed = Vec::new();
    for asset in &mut profile.assets {
        let bytes = std::fs::read(assets.join(&asset.name))
            .with_context(|| format!("candidate asset {} missing", asset.name))?;
        let digest = sha(&bytes);
        ensure!(
            matches!(
                asset.role,
                AssetRole::ExternalWeights | AssetRole::Calibration
            ) || (digest == asset.sha256 && bytes.len() as u64 == asset.bytes),
            "only the weights and calibration metadata may differ from the source"
        );
        if digest != asset.sha256 {
            changed.push(asset.name.clone());
        }
        asset.bytes = bytes.len() as u64;
        asset.sha256 = digest;
        if asset.role == AssetRole::Calibration {
            profile.calibration = digest;
        }
    }
    ensure!(
        changed.iter().any(|name| name == "model.onnx.data"),
        "candidate weights must differ from the source weights"
    );
    profile.profile_id = profile_id;
    profile.downloads.clear();
    profile.qualified_targets.clear();
    profile.manifest = candidate_manifest(source_expected, &profile)?;
    profile.validate()?;

    // Native compiler parity with the Python example builder.
    let asset = |role: AssetRole| -> Result<Vec<u8>> {
        let found = profile
            .assets
            .iter()
            .find(|asset| asset.role == role)
            .context("profile asset missing")?;
        Ok(std::fs::read(assets.join(&found.name))?)
    };
    let compiler =
        Compiler::from_verified(&asset(AssetRole::Compiler)?, &asset(AssetRole::Tokenizer)?)?;
    let (mut rows, mut equal) = (0_usize, 0_usize);
    for line in std::fs::read_to_string(&parity)?
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let row: ParityRow = serde_json::from_str(line)?;
        let input = experiment::compile_software(
            &compiler,
            "",
            &row.query,
            &[Candidate {
                id: "candidate".into(),
                text: row.document,
            }],
        )?;
        let live: Vec<i64> = input
            .input_ids
            .iter()
            .zip(&input.attention_mask)
            .filter(|(_, mask)| **mask == 1)
            .map(|(id, _)| *id)
            .collect();
        rows += 1;
        equal += usize::from(live == row.live_ids);
    }
    ensure!(rows > 0, "parity sample is empty");

    std::fs::create_dir(&output)?;
    let mut bytes = serde_json::to_vec_pretty(&profile)?;
    bytes.push(b'\n');
    let profile_path = output.join(format!("{}.json", profile.profile_id));
    std::fs::write(&profile_path, &bytes)?;
    let profile_sha = sha(&bytes);
    let fixture = profile
        .assets
        .iter()
        .find(|asset| asset.role == AssetRole::Fixture)
        .context("fixture asset missing")?;
    let inputs = serde_json::json!([{
        "profile": profile_path, "assets": assets, "fixtures": assets.join(&fixture.name),
        "profile_sha256": profile_sha,
    }]);
    let mut inputs_bytes = serde_json::to_vec_pretty(&inputs)?;
    inputs_bytes.push(b'\n');
    std::fs::write(output.join("candidate-inputs.json"), &inputs_bytes)?;
    let report = serde_json::json!({
        "schema": "airk.private.answer-focus-candidate-profile/1",
        "profile": profile_path, "profile_sha256": profile_sha, "manifest": profile.manifest,
        "source_profile": source_path, "source_profile_sha256": source_expected,
        "source_manifest": source.manifest, "changed_assets": changed,
        "inputs_sha256": sha(&inputs_bytes), "parity_rows": rows, "parity_equal": equal,
        "weights_copied": false, "release_qualified": false,
    });
    std::fs::write(
        output.join("emission.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    ensure!(
        equal == rows,
        "native and Python Focus compilers differ on {} of {rows} rows",
        rows - equal
    );
    Ok(())
}
