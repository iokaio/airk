# SPDX-License-Identifier: Apache-2.0
"""Prepare opaque FINAL native entry from unchanged DEV canonical engine path."""
from pathlib import Path
import sys
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/"support"))
from recipe_paths import private_run_root, fresh_output
ROOT=next(p for p in Path(__file__).resolve().parents if (p/"Cargo.toml").is_file() and (p/"crates/airk-core").is_dir())
SOURCE_DIR=Path(__file__).resolve().parent;HERE=private_run_root(ROOT)
fresh_output(HERE,HERE/'fresh_final.rs')
text=(ROOT/'crates/airk-kernel/src/inference_owner/evaluation_tests/development.rs').read_text()
text=text.replace('//! Separate development-only capture. Original205 entry and grader stay unchanged.','//! Opaque fresh FINAL capture; no developer question/gold inspection.')
text=text.replace('f6c6616c1de29948deee9f37c70ccaadbb7a5dc71d0cd02c91d207c9ccbac097','7c579edd13c85b0bdadfe8308bca2a427c5364bc9abab6d1e753b313e9769d5c')
start=text.index('const MASK_PIN:')
end=text.index('fn dev_source_closure(')
text=text[:start]+'''const PROTOCOL_PIN: &str = "2fcde2b8428ed1e2301960a3e44e2b8aba7b224aef2aca756b1e21b929b8030e";
fn final_family(input_bytes:&[u8],freeze:&Value,family:&str)->Result<EncoderInput> {
    ensure!(freeze["schema"]=="airk.private-final-candidate-export-cal-freeze/1" && freeze["status"]=="FROZEN_BEFORE_ANY_FRESH_FINAL_MODEL_OUTCOME","unfrozen FINAL candidate barrier");
    let roles=freeze["families"].as_array().context("missing frozen six families")?;
    ensure!(roles.len()==6 && roles.iter().filter_map(|r|r["role"].as_str()).collect::<BTreeSet<_>>()==BTreeSet::from(["candidate-u8","candidate-s8","control-shipped-u8","control-mixed-u8","diagnostic-shipped-s8","diagnostic-mixed-s8"]),"exact frozen six-role experiment required");
    let rows:Vec<EncoderInput>=serde_json::from_slice(input_bytes)?;
    ensure!(rows.len()==6 && rows.iter().map(|r|r.family.as_str()).collect::<BTreeSet<_>>()==roles.iter().filter_map(|r|r["family"].as_str()).collect::<BTreeSet<_>>() && rows.iter().map(|r|r.family.as_str()).collect::<BTreeSet<_>>().len()==6,"native descriptors differ from exact frozen six families");
    let digest=format!("{:x}",Sha256::digest(input_bytes));
    for row in &rows {let expected=roles.iter().find(|r|r["family"]==row.family).context("foreign final family")?;ensure!(expected["encoder_input_sha256"]==digest && expected["model_json_sha256"]==serde_json::to_value(row.model_json_sha256)? && expected["runtime"]==airk_retrieval::model::ONNX_RUNTIME,"native input differs from candidate/export/CAL freeze");sealed(&row.directory.join("model.json"),row.model_json_sha256)?;sealed(&row.oracle,row.oracle_sha256)?;}
    rows.into_iter().find(|r|r.family==family).context("unknown frozen family")
}
''' + text[end:]
text=text.replace('dev_source_closure','final_source_closure')
text=text.replace('expected.files.len() == 366','!expected.files.is_empty()')
text=text.replace('native_dev64_questions_use_current_kernel_with_fixed20_quality_mask','native_fresh_final205_questions_use_current_kernel_after_candidate_freeze')
text=text.replace('AIRK_NATIVE_DEV64','AIRK_NATIVE_FINAL205')
start=text.index('    let mask_path =')
end=text.index('    let manifest =',start)
text=text[:start]+'''    let freeze_path=PathBuf::from(std::env::var_os("AIRK_NATIVE_FINAL205_CANDIDATE_FREEZE").context("missing immutable candidate/export/CAL barrier")?);
    let freeze_pin:Sha256Digest=std::env::var("AIRK_NATIVE_FINAL205_CANDIDATE_FREEZE_SHA256").context("missing primary-reviewed frozen barrier SHA")?.parse()?;
    let freeze_bytes=sealed(&freeze_path,freeze_pin)?;let freeze:Value=serde_json::from_slice(&freeze_bytes)?;
''' + text[end:]
text=text.replace('            && metadata["quality_mask_sha256"] == MASK_PIN\n            && metadata["development_protocol_sha256"] == PROTOCOL_PIN,','            && metadata["fresh_final_protocol_sha256"] == PROTOCOL_PIN\n            && metadata["task_count"] == 205 && metadata["original_paragraph_count"] == 1685,')
text=text.replace('    let source_root = source_base.join("sources/hotpot").canonicalize()?;','')
start=text.index('    let tasks =')
end=text.index('    let questions = Questions {',start)
text=text[:start]+'''    let tasks=corpus["tasks"].as_array().context("missing opaque FINAL tasks")?;
    ensure!(tasks.len()==205 && tasks.iter().filter(|t|t["dataset"]=="HotpotQA").count()==171 && tasks.iter().filter(|t|t["dataset"]=="QASPER").count()==34,"fixed 171Hotpot/34QASPER population required");
    let mut questions=Vec::new();let mut ids=BTreeSet::new();
    for task in tasks {
        ensure!(task["split"]=="test" && task["experiment_partition"]=="FINAL","non-FINAL task refused");
        let id=task["id"].as_str().context("missing original task ID")?;ensure!(ids.insert(id.to_string()),"duplicate FINAL question");
        let source_root=if task["dataset"]=="HotpotQA" {source_base.join("sources/hotpot")} else {let group=task["source_group"].as_str().context("missing QASPER family")?;let paper=group.strip_prefix("qasper:").context("foreign QASPER scope")?;ensure!(!paper.is_empty() && !paper.contains(['/', '\\\\']),"invalid paper family");source_base.join("sources/qasper").join(paper)}.canonicalize()?;
        questions.push(Question{id:id.into(),source_root,question:task["question"].as_str().context("missing original question")?.into()});
    }
''' + text[end:]
start=text.index('    let input = inputs()?')
end=text.index('    let output =',start)
text=text[:start]+'''    let encoder_input_bytes=std::fs::read(std::env::var_os("AIRK_NATIVE_ENCODER_INPUT").context("missing exact frozen native descriptors")?)?;
    let input=final_family(&encoder_input_bytes,&freeze,&family)?;
''' + text[end:]
text=text.replace('    let control = tempfile::tempdir()?;','    ensure!(freeze["native_binary_sha256"]==serde_json::to_value(binary_sha256)?,"FINAL native binary differs from frozen barrier");\n    let control = tempfile::tempdir()?;')
start=text.index('    let mut receipt = json!(')
end=text.index('\n    owner.run_encoder_evaluation(',start)
text=text[:start]+'''    let expected=freeze["families"].as_array().unwrap().iter().find(|r|r["family"]==input.family).context("missing frozen family")?;
    ensure!(expected["pipeline_digest"]==serde_json::to_value(expected_model.pipeline_digest)?,"actual native pipeline differs from frozen barrier");
    let mut receipt=json!({"schema":"airk.native-fresh-final-retrieval-family/1","corpus_sha256":CORPUS_PIN,"fresh_final_protocol_sha256":PROTOCOL_PIN,"candidate_freeze_sha256":freeze_pin,"complete":false,"evaluation":true,"release_qualified":false,"family":input.family,"question_input_sha256":format!("{:x}",Sha256::digest(&question_bytes)),"encoder_input_sha256":format!("{:x}",Sha256::digest(&encoder_input_bytes)),"source_manifest_sha256":manifest_pin,"source_roster_sha256":source_before,"source_roster_count":source_rows.len(),"source_before_sha256":source_before,"source_after_sha256":null,"binary_sha256":binary_sha256,"model_json_sha256":input.model_json_sha256,"pipeline_digest":expected_model.pipeline_digest,"runtime":airk_retrieval::model::ONNX_RUNTIME,"query":{"mode":"vector","limit":10,"max_bytes":32768,"full_chunks":true},"collections":[],"records":[]});''' + text[end:]
text=text.replace('    sealed(&mask_path, MASK_PIN.parse()?)?;','    ensure!(sealed(&freeze_path,freeze_pin)?==freeze_bytes,"frozen candidate barrier changed during FINAL capture");')
text=text.replace('one matched six-family DEV64 native run; no final promotion; explicit reviewed inputs','opaque fresh FINAL205; immutable candidate/export/CAL freeze and Root execution release required')
text=text.replace('closed366DEVroster','exact frozen FINAL source roster')
text=text.replace('DEV','FINAL')
(HERE/'fresh_final.rs').write_text(text)
print('Prepared private opaque FINAL native source; no payload opened or native source mounted')
