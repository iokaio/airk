// SPDX-License-Identifier: Apache-2.0
//! Private, ignored emitter for the 1.8.x unqualified local-test catalog.
//!
//! It never constructs identities by hand: `qualification::prepare` (the Rust
//! owner of compiler/calibration/policy digests) regenerates every descriptor,
//! and only the local-test profile id, GitHub release downloads and the catalog
//! manifest are added afterwards. Weights, graphs and tokenizers stay
//! byte-identical (they are re-authenticated by `prepare`).
//!
//! `AIRK_LOCAL_TEST_CATALOG_INPUT` names a JSON file:
//! `{"phase":"prepare"|"catalog","output":"<fresh dir>","tag":"models-1.8.201",
//!   "models":[{"raw_dir":"..","profile_id":"unqualified-local-test-.."}],
//!   "parts":"<parts-manifest.json>","catalog":"<release-catalog.json>"}`.
use super::{qualification::*, *};
use airk_core::identity::framed_digest;
use anyhow::Context;
use std::{fs, path::PathBuf};

fn text<'a>(value: &'a serde_json::Value, key: &str) -> Result<&'a str> {
    value[key].as_str().with_context(|| key.to_string())
}

fn prepare_phase(input: &serde_json::Value) -> Result<()> {
    let output = PathBuf::from(text(input, "output")?);
    fs::create_dir_all(&output)?;
    for model in input["models"].as_array().context("models")? {
        let raw_dir = PathBuf::from(text(model, "raw_dir")?);
        let raw: Profile =
            serde_json::from_slice(&fs::read(raw_dir.join("experimental-profile.json"))?)?;
        let files: BTreeMap<String, PathBuf> =
            serde_json::from_slice(&fs::read(raw_dir.join("asset-files.private.json"))?)?;
        let assets = raw
            .assets
            .iter()
            .filter(|asset| !matches!(asset.role, AssetRole::Compiler | AssetRole::Calibration))
            .map(|asset| {
                Ok(AssetInput {
                    asset: asset.clone(),
                    path: files
                        .get(&asset.name)
                        .with_context(|| format!("missing file for {}", asset.name))?
                        .clone(),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let prepared = prepare(
            ProfileInput {
                model: raw.model,
                profile_id: raw.profile_id.clone(),
                assets,
                cpu_fit: raw.cpu_fit.clone().context("raw cpu fit")?,
                memory_reservation_bytes: raw.memory_reservation_bytes,
                peak_disk_bytes: raw.peak_disk_bytes,
                maximum_candidates: raw.max_candidates,
                calibration: CalibrationInput::NumericRawOnly,
            },
            &output.join(model_dir(raw.model)),
        )?;
        assert_eq!(prepared.profile.max_tokens, raw.max_tokens);
        assert_eq!(
            prepared.profile.memory_reservation_bytes,
            raw.memory_reservation_bytes
        );
    }
    Ok(())
}

fn model_dir(model: ModelChoice) -> &'static str {
    match model {
        ModelChoice::FocusQwenReranker06B => "arbiter-focus-qwen3-reranker-06b",
        ModelChoice::ContextQwenCompact => "arbiter-context-qwen3-17b",
        ModelChoice::ContextQwenLarge => "arbiter-context-qwen3-4b",
        _ => unreachable!("only the three selected models"),
    }
}

fn catalog_phase(input: &serde_json::Value) -> Result<()> {
    let output = PathBuf::from(text(input, "output")?);
    let tag = text(input, "tag")?;
    let parts: serde_json::Value = serde_json::from_slice(&fs::read(text(input, "parts")?)?)?;
    let mut profiles = Vec::new();
    for model in input["models"].as_array().context("models")? {
        let raw_dir = PathBuf::from(text(model, "raw_dir")?);
        let raw: Profile =
            serde_json::from_slice(&fs::read(raw_dir.join("experimental-profile.json"))?)?;
        let directory = output.join(model_dir(raw.model));
        let mut profile: Profile =
            serde_json::from_slice(&fs::read(directory.join("experimental-profile.json"))?)?;
        profile.profile_id = text(model, "profile_id")?.to_string();
        profile.qualification = None;
        profile.qualified_targets = Vec::new();
        let rows = &parts[model_dir(profile.model)];
        profile.downloads = profile
            .assets
            .iter()
            .map(|asset| {
                let pieces = rows[&asset.name].as_array().context("asset parts")?;
                Ok(AssetDownload {
                    asset_name: asset.name.clone(),
                    parts: pieces
                        .iter()
                        .map(|piece| {
                            Ok(AssetPart {
                                url: format!(
                                    "https://github.com/iokaio/airk/releases/download/{tag}/{}",
                                    text(piece, "file")?
                                ),
                                backup_urls: Vec::new(),
                                bytes: piece["bytes"].as_u64().context("bytes")?,
                                sha256: text(piece, "sha256")?.parse()?,
                            })
                        })
                        .collect::<Result<Vec<_>>>()?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        profile.manifest = Sha256Digest::from_bytes([0; 32]);
        profile.manifest = framed_digest(
            b"airk.private.local-test-release-profile/1",
            &[&serde_json::to_vec(&profile)?],
        );
        profile.validate()?;
        profile.validate_release_downloads()?;
        validate_selected_policies(&profile)?;
        release_qualification_gate(&profile, "1.8.201")?;
        assert!(release_qualification_gate(&profile, "1.9.0").is_err());
        profiles.push(profile);
    }
    let catalog = serde_json::json!({"schema": CATALOG_SCHEMA, "profiles": profiles});
    fs::write(
        text(input, "catalog")?,
        serde_json::to_vec_pretty(&catalog)?,
    )?;
    Ok(())
}

#[test]
#[ignore = "private emitter: needs AIRK_LOCAL_TEST_CATALOG_INPUT and the private exports"]
fn emit_local_test_catalog() -> Result<()> {
    let path = std::env::var_os("AIRK_LOCAL_TEST_CATALOG_INPUT").context("input path")?;
    let input: serde_json::Value = serde_json::from_slice(&fs::read(path)?)?;
    match text(&input, "phase")? {
        "prepare" => prepare_phase(&input),
        "catalog" => catalog_phase(&input),
        _ => anyhow::bail!("unknown phase"),
    }
}
