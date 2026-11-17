# SPDX-License-Identifier: Apache-2.0
"""Prepare native candidate test sibling; does not edit frozen Rust tree."""
from pathlib import Path
import sys
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/"support"))
from recipe_paths import private_run_root, fresh_output
ROOT=next(p for p in Path(__file__).resolve().parents if (p/"Cargo.toml").is_file() and (p/"crates/airk-core").is_dir())
SOURCE_DIR=Path(__file__).resolve().parent;HERE=private_run_root(ROOT)
fresh_output(HERE,HERE/'candidate.rs')
parent=(ROOT/'crates/airk-kernel/src/inference_owner/evaluation_tests.rs').read_text()
numeric=parent[parent.index('#[test]\n#[ignore = "six exact private'):parent.index('\n#[derive(Deserialize)]\n#[serde(deny_unknown_fields)]\nstruct Questions')]
numeric=numeric.replace('six_encoder_families_match_independent_native_bucket_oracles_without_promoting_baselines','candidate_two_families_match_independent_native_bucket_oracles')
numeric=numeric.replace('for input in inputs()? {','for input in candidate_inputs()? {').replace('if input.family.starts_with("trained-")','if input.family.starts_with("visible-prefix-mining-")').replace('results.len()==6','results.len()==2')
dev=(ROOT/'crates/airk-kernel/src/inference_owner/evaluation_tests/development.rs').read_text()
dev=dev.replace('use super::*;','')
dev=dev.replace('const ENCODERS_PIN: &str = "3ead240726ebdba10577db6778bffbf5232eab10c50f2ae128d589e0a961ab81";','')
dev=dev.replace('development_identity_gate_retains_all64_including44_refusals','candidate_identity_gate_retains_all64_including44_refusals')
dev=dev.replace('native_dev64_questions_use_current_kernel_with_fixed20_quality_mask','candidate_dev64_questions_use_current_kernel_with_fixed20_quality_mask')
dev=dev.replace('let input = inputs()?','let input = candidate_inputs()?')
dev=dev.replace('== ENCODERS_PIN.parse()?', '== std::env::var("AIRK_CANDIDATE_ENCODER_INPUT_SHA256").context("missing primary-reviewed candidate master SHA")?.parse()?')
dev=dev.replace('"original six family descriptor inputs changed"','"reviewed candidate descriptor inputs changed"')
header='''// SPDX-License-Identifier: Apache-2.0
//! Separate exact epoch1 candidate admission; closed old six remain unchanged.
use super::*;
use airk_retrieval::model::OnnxManifest;
const ORACLE_PIN: &str = "7e2f6112c521cdcdb3f892c7338818f85f4edfa7c9578a0cfeff58ab8fac9300";
const EXPORT_PIN: &str = "4f5a1e29dd8deea509450a2300ba4baaa8dc6df38ce357eade21f012989de272";
const STAGE_PIN: &str = "10cb03745f8170fa4a0cb2062ce5d079cab1b6a6b1fa811c1c857db611804799";
const U8_PIN: &str = "1f47822317026b7151e9b3279f2c507b7cc49d97d8d08d713b732e001da97872";
const S8_PIN: &str = "58507b49baf6684021f0541c46328d9db37dedb85ca43e763c30e34190daa514";
fn asset_sha(path: &std::path::Path) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(std::fs::read(path)?)))
}
fn typed_manifest(directory: &std::path::Path, short: &str) -> Result<OnnxManifest> {
    let model_sha256=asset_sha(&directory.join("model.onnx"))?;
    ensure!(model_sha256==if short=="u8" {U8_PIN} else {S8_PIN},"exact selected epoch1 ONNX required");
    let tokenizer_sha256=asset_sha(&directory.join("tokenizer.json"))?;
    ensure!(tokenizer_sha256=="be50c3628f2bf5bb5e3a7f17b1f74611b2561a3a27eeab05e5aa30f411572037","unchanged tokenizer required");
    Ok(OnnxManifest { schema:2,name:"sentence-transformers/all-MiniLM-L6-v2".into(),license:"Apache-2.0".into(),source:"https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2".into(),revision:"10dbd2f06a8baf40ad285b037edda610c1b9a57c".into(),model_sha256,tokenizer_sha256,license_sha256:asset_sha(&directory.join("LICENSE"))?,dimensions:384,max_tokens:256,pooling:"attention-mask-mean-l2".into(),input_ids:"input_ids".into(),attention_mask:"attention_mask".into(),token_type_ids:Some("token_type_ids".into()),output:"last_hidden_state".into() })
}
fn candidate_inputs() -> Result<Vec<EncoderInput>> {
    let path=PathBuf::from(std::env::var_os("AIRK_NATIVE_ENCODER_INPUT").context("missing actual Rust-emitted candidate master")?);
    let pin:Sha256Digest=std::env::var("AIRK_CANDIDATE_ENCODER_INPUT_SHA256").context("missing primary-reviewed candidate master SHA")?.parse()?;
    let rows:Vec<EncoderInput>=serde_json::from_slice(&sealed(&path,pin)?)?;
    ensure!(rows.len()==2 && rows.iter().map(|r|r.family.as_str()).collect::<BTreeSet<_>>()==BTreeSet::from(["visible-prefix-mining-r1-u8","visible-prefix-mining-r1-s8"]),"exact separate candidate two-family set required");
    for row in &rows {
        let short=if row.family.ends_with("-u8") {"u8"} else {"s8"};
        ensure!(row.oracle_sha256==ORACLE_PIN.parse()? && row.oracle_result==format!("mixed-{short}"),"exact independent candidate oracle required");
        sealed(&row.oracle,row.oracle_sha256)?;
        let manifest:OnnxManifest=serde_json::from_slice(&sealed(&row.directory.join("model.json"),row.model_json_sha256)?)?;
        ensure!(manifest==typed_manifest(&row.directory,short)?,"candidate manifest differs from owning typed contract");
    }
    Ok(rows)
}
#[test]
#[ignore="explicit selected epoch1 private assets; emits Rust-owned native manifests before consumers"]
fn emit_selected_epoch1_candidate_native_manifests() -> Result<()> {
    let base=PathBuf::from(std::env::var_os("AIRK_CANDIDATE_EXPORTS").context("missing exact export root")?).canonicalize()?;
    let oracle=PathBuf::from(std::env::var_os("AIRK_CANDIDATE_ORACLE").context("missing exact oracle")?).canonicalize()?;
    let output=PathBuf::from(std::env::var_os("AIRK_CANDIDATE_MASTER_OUTPUT").context("missing fresh candidate master output")?);
    let stage=PathBuf::from(std::env::var_os("AIRK_CANDIDATE_STAGE_SEAL").context("missing complete export stage seal")?);
    sealed(&stage,STAGE_PIN.parse()?)?;sealed(&base.join("receipt.json"),EXPORT_PIN.parse()?)?;sealed(&oracle,ORACLE_PIN.parse()?)?;
    ensure!(!output.try_exists()?,"never overwrite prior candidate master");
    let mut rows=Vec::new();
    for short in ["u8","s8"] {
        let directory=base.join(short);let path=directory.join("model.json");
        ensure!(!path.try_exists()?,"never overwrite prior native model manifest");
        let manifest=typed_manifest(&directory,short)?;
        // LICENSE must exactly match the independently frozen owning export.
        let exported:Value=serde_json::from_slice(&sealed(&base.join("receipt.json"),EXPORT_PIN.parse()?)?)?;
        let profile=exported["profiles"].as_array().context("missing export profiles")?.iter().find(|p|p["id"]==format!("visible-prefix-mining-r1-{short}")).context("missing exact candidate profile")?;
        ensure!(profile["assets"].as_array().context("missing asset seals")?.iter().any(|a|a["name"]=="LICENSE" && a["sha256"]==manifest.license_sha256),"LICENSE differs from frozen training attribution");
        std::fs::write(&path,serde_json::to_vec_pretty(&manifest)?)?;
        let model_json_sha256:Sha256Digest=asset_sha(&path)?.parse()?;
        let control=tempfile::tempdir()?;let lease=Arc::new(airk_store::service::ServiceLease::acquire(control.path())?);let owner=NativeInferenceOwner::admit(lease)?;
        owner.run_encoder_evaluation(model_json_sha256,||airk_retrieval::model::inspect_object_plan(&directory))??;
        ensure!(owner.run(||EmbeddingModel::open(&directory))?.is_err(),"product catalog must continue refusing development candidate");
        rows.push(json!({"family":format!("visible-prefix-mining-r1-{short}"),"directory":directory,"model_json_sha256":model_json_sha256,"oracle":oracle,"oracle_sha256":ORACLE_PIN,"oracle_result":format!("mixed-{short}")}));
    }
    std::fs::write(output,serde_json::to_vec_pretty(&rows)?)?;
    Ok(())
}
'''
(HERE/'candidate.rs').write_text(header+'\n'+numeric+'\n'+dev)
print('Prepared private candidate.rs; old native sources not edited')
