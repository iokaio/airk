// SPDX-License-Identifier: Apache-2.0
//! Exact original publisher bytes, current provider implementation, native drafts only.
use super::*;
use crate::model::{MAX_MODEL_BYTES, MAX_TOKENIZER_BYTES};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

fn asset(path: &Path, limit: u64) -> Result<(u64, String)> {
    for ancestor in path.ancestors() {
        let metadata = fs::symlink_metadata(ancestor)?;
        ensure!(
            !metadata.file_type().is_symlink(),
            "baseline asset alias refused"
        );
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            ensure!(
                metadata.file_attributes() & 0x400 == 0,
                "baseline asset reparse refused"
            );
        }
    }
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file() && metadata.len() <= limit,
        "baseline asset shape/size differs"
    );
    let mut source = fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut count = 0u64;
    let mut buffer = [0u8; 65536];
    loop {
        let read = source.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        count = count
            .checked_add(read as u64)
            .context("baseline length overflow")?;
        ensure!(count <= limit, "baseline asset grew");
        hash.update(&buffer[..read]);
    }
    ensure!(
        count == metadata.len() && source.metadata()?.len() == count,
        "baseline asset length changed"
    );
    Ok((count, format!("{:x}", hash.finalize())))
}
fn write(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)?;
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    file.write_all(&bytes)?;
    file.sync_all()?;
    Ok(())
}

#[test]
#[ignore = "Root-recorded original MiniLM restoration; pure typed catalog/bundle draft; no inference"]
fn emit_original_baseline_current_runtime_catalog_and_bundle_draft() -> Result<()> {
    let root = PathBuf::from(
        std::env::var_os("AIRK_ORIGINAL_MINILM_ASSETS")
            .context("actual original MiniLM assets missing")?,
    );
    let output = PathBuf::from(
        std::env::var_os("AIRK_ORIGINAL_MINILM_DRAFT_OUTPUT")
            .context("fresh baseline draft output missing")?,
    );
    ensure!(
        root.is_absolute() && output.is_absolute() && !output.try_exists()?,
        "absolute inputs/fresh native output required"
    );
    let private = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../artifacts")
        .canonicalize()?;
    ensure!(
        output
            .parent()
            .context("draft parent missing")?
            .canonicalize()?
            .starts_with(&private),
        "draft leaves private artifacts"
    );
    // This retained source has a fixed independently checked digest. Its typed
    // manifests describe the original publisher weights; only catalog ID,
    // current provider field, download policy and candidacy scope change.
    let mut catalog = legacy_catalog()?;
    catalog.schema = 2;
    catalog.default_profile = "minilm-l6-v2-u8-v3".into();
    let mut pins = std::collections::BTreeMap::new();
    for profile in &mut catalog.profiles {
        let short = match profile.precision.as_str() {
            "uint8" => "u8",
            "int8" => "s8",
            _ => anyhow::bail!("unexpected original precision"),
        };
        profile.id = format!("minilm-l6-v2-{short}-v3");
        profile.runtime = ONNX_RUNTIME.into();
        profile.quantization = None;
        profile.qualification=Qualification {verified_platforms:Vec::new(),evidence:"docs/split/codex-plan.md#9-arbiter-and-local-models".into(),scope:"Exact original publisher graph under current ORT1.30 provider. Qualification remains empty until actual task, signed-profile numerical/platform, resource, regeneration, installed/package/offline gates complete. Historical provider name is not training provenance.".into()};
        let directory = root.join(short);
        let definition = directory.join("model.json");
        let (_, definition_sha) = asset(&definition, 16384)?;
        let mut bytes = Vec::new();
        fs::File::open(&definition)?
            .take(16385)
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() <= 16384 && format!("{:x}", Sha256::digest(&bytes)) == definition_sha,
            "original definition changed during bounded read"
        );
        let manifest: OnnxManifest = serde_json::from_slice(&bytes)?;
        ensure!(
            manifest == profile.manifest,
            "original publisher manifest/ABI changed"
        );
        pins.insert(definition, definition_sha);
        for entry in &mut profile.assets {
            let path = directory.join(&entry.name);
            let (length, digest) = asset(&path, entry.bytes)?;
            ensure!(
                length == entry.bytes && digest == entry.sha256,
                "original publisher asset changed"
            );
            pins.insert(path, digest);
            entry.url = None;
        }
    }
    catalog.validate()?;
    ensure!(
        catalog
            .profiles
            .iter()
            .all(|profile| profile.quantization.is_none()
                && profile.qualification.verified_platforms.is_empty()),
        "draft must not inherit trained policy or qualification"
    );
    fs::create_dir(&output)?;
    write(&output.join("model_catalog.json"), &catalog)?;
    let policy = serde_json::json!({"schema":"airk.minilm.native-policy/2","catalog":catalog,"scope":"candidate build policy; human quality and installed platform qualification are not inferred"});
    write(&output.join("model-policy.json"), &policy)?;
    // Rust emits each exact existing OnnxManifest for the default bundle. No
    // handwritten Python/native mirror is introduced by the materializer.
    for profile in &catalog.profiles {
        let path = output.join(&profile.id);
        fs::create_dir(&path)?;
        write(&path.join("model.json"), &profile.manifest)?;
    }
    for (path, pin) in &pins {
        ensure!(
            asset(path, MAX_MODEL_BYTES.max(MAX_TOKENIZER_BYTES))?.1 == *pin,
            "original source drift during draft emission"
        );
    }
    write(
        &output.join("draft-receipt.json"),
        &serde_json::json!({"schema":"airk.private-original-minilm-native-draft/1","source_pins":pins,"runtime":ONNX_RUNTIME,"graph_bytes_changed":false,"NN_invocations":0,"product_activated":false,"qualification_inferred":false}),
    )?;
    Ok(())
}
