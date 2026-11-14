// SPDX-License-Identifier: Apache-2.0
//! Opaque fresh FINAL capture; no developer question/gold inspection.
use super::*;
const CORPUS_PIN: &str = "7c579edd13c85b0bdadfe8308bca2a427c5364bc9abab6d1e753b313e9769d5c";
const PROTOCOL_PIN: &str = "1feb3036243249ccfd18c7d75af0adae4b9b908301ed17a12639ac36fc8927f5";
fn final_family(input_bytes: &[u8], freeze: &Value, family: &str) -> Result<EncoderInput> {
    ensure!(
        freeze["schema"] == "airk.private-final-candidate-export-cal-freeze/1"
            && freeze["status"] == "FROZEN_BEFORE_ANY_FRESH_FINAL_MODEL_OUTCOME",
        "unfrozen FINAL candidate barrier"
    );
    for name in [
        "candidate_selection_sha256",
        "calibration_sha256",
        "native_binary_sha256",
    ] {
        let _: Sha256Digest = freeze[name]
            .as_str()
            .context("missing frozen selection/CAL/native identity")?
            .parse()?;
    }
    let roles = freeze["families"]
        .as_array()
        .context("missing frozen six families")?;
    ensure!(
        roles.len() == 6
            && roles
                .iter()
                .filter_map(|r| r["role"].as_str())
                .collect::<BTreeSet<_>>()
                == BTreeSet::from([
                    "candidate-u8",
                    "candidate-s8",
                    "control-shipped-u8",
                    "control-mixed-u8",
                    "diagnostic-shipped-s8",
                    "diagnostic-mixed-s8"
                ]),
        "exact frozen six-role experiment required"
    );
    let rows: Vec<EncoderInput> = serde_json::from_slice(input_bytes)?;
    ensure!(
        rows.len() == 6
            && rows
                .iter()
                .map(|r| r.family.as_str())
                .collect::<BTreeSet<_>>()
                == roles
                    .iter()
                    .filter_map(|r| r["family"].as_str())
                    .collect::<BTreeSet<_>>()
            && rows
                .iter()
                .map(|r| r.family.as_str())
                .collect::<BTreeSet<_>>()
                .len()
                == 6,
        "native descriptors differ from exact frozen six families"
    );
    let digest = format!("{:x}", Sha256::digest(input_bytes));
    for row in &rows {
        let expected = roles
            .iter()
            .find(|r| r["family"] == row.family)
            .context("foreign final family")?;
        ensure!(
            expected["encoder_input_sha256"] == digest
                && expected["model_json_sha256"] == serde_json::to_value(row.model_json_sha256)?
                && expected["runtime"] == airk_retrieval::model::ONNX_RUNTIME,
            "native input differs from candidate/export/CAL freeze"
        );
        sealed(&row.directory.join("model.json"), row.model_json_sha256)?;
        sealed(&row.oracle, row.oracle_sha256)?;
    }
    rows.into_iter()
        .find(|r| r.family == family)
        .context("unknown frozen family")
}
fn final_source_closure(
    manifest: &std::path::Path,
    pin: Sha256Digest,
) -> Result<(PathBuf, Vec<SourceRow>, Sha256Digest)> {
    #[derive(Deserialize)]
    struct Manifest {
        files: Vec<SourceRow>,
    }
    let bytes = sealed(manifest, pin)?;
    let mut expected: Manifest = serde_json::from_slice(&bytes)?;
    expected.files.sort_by(|a, b| a.file.cmp(&b.file));
    ensure!(
        !expected.files.is_empty()
            && expected
                .files
                .windows(2)
                .all(|rows| rows[0].file != rows[1].file),
        "frozen source manifest exact frozen FINAL source roster required"
    );
    let base = manifest
        .parent()
        .context("source manifest parent missing")?
        .canonicalize()?;
    fn walk(base: &std::path::Path, path: &std::path::Path, files: &mut Vec<String>) -> Result<()> {
        let metadata = std::fs::symlink_metadata(path)?;
        ensure!(
            !metadata.file_type().is_symlink(),
            "source fixture alias refused"
        );
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            ensure!(
                metadata.file_attributes() & 0x400 == 0,
                "source fixture reparse point refused"
            );
        }
        if metadata.is_dir() {
            for entry in std::fs::read_dir(path)? {
                walk(base, &entry?.path(), files)?;
            }
        } else {
            ensure!(metadata.is_file(), "source fixture special file refused");
            files.push(
                path.strip_prefix(base)?
                    .to_str()
                    .context("source path not UTF8")?
                    .replace('\\', "/"),
            );
        }
        Ok(())
    }
    let mut files = Vec::new();
    walk(&base, &base.join("sources"), &mut files)?;
    files.sort();
    ensure!(
        files
            == expected
                .files
                .iter()
                .map(|row| row.file.clone())
                .collect::<Vec<_>>(),
        "source fixture has extra or missing files"
    );
    for row in &expected.files {
        airk_core::index_catalog::relative_path(&row.file, false)?;
        let path = base.join(&row.file);
        ensure!(
            std::fs::metadata(&path)?.len() == row.bytes,
            "source length changed"
        );
        sealed(&path, row.sha256)?;
    }
    let hash =
        Sha256Digest::from_bytes(Sha256::digest(serde_json::to_vec(&expected.files)?).into());
    Ok((base, expected.files, hash))
}
fn validate_population(tasks: &[Value]) -> Result<()> {
    ensure!(
        tasks.len() == 205
            && tasks.iter().filter(|t| t["dataset"] == "HotpotQA").count() == 171
            && tasks.iter().filter(|t| t["dataset"] == "QASPER").count() == 34,
        "fixed 171Hotpot/34QASPER population required"
    );
    let mut ids = BTreeSet::new();
    for task in tasks {
        ensure!(
            task["split"] == "test" && task["experiment_partition"] == "FINAL",
            "non-FINAL task refused"
        );
        let id = task["id"].as_str().context("missing original task ID")?;
        ensure!(
            !id.is_empty() && ids.insert(id),
            "duplicate/empty FINAL task ID"
        );
    }
    Ok(())
}
#[test]
fn fresh_final_population_preserves205_and_refuses_missing_duplicate_foreign_partition()
-> Result<()> {
    let tasks:Vec<_>=(0..205).map(|i|json!({"id":format!("q{i}"),"dataset":if i<171 {"HotpotQA"}else{"QASPER"},"split":"test","experiment_partition":"FINAL"})).collect();
    validate_population(&tasks)?;
    assert!(validate_population(&tasks[..204]).is_err());
    let mut duplicate = tasks.clone();
    duplicate[204]["id"] = duplicate[0]["id"].clone();
    assert!(validate_population(&duplicate).is_err());
    let mut foreign = tasks.clone();
    foreign[0]["experiment_partition"] = "DEV".into();
    assert!(validate_population(&foreign).is_err());
    let mut composition = tasks;
    composition[0]["dataset"] = "QASPER".into();
    assert!(validate_population(&composition).is_err());
    Ok(())
}
#[test]
#[ignore = "opaque fresh FINAL205; immutable candidate/export/CAL freeze and Root execution release required"]
fn native_fresh_final205_questions_use_current_kernel_after_candidate_freeze() -> Result<()> {
    let corpus_path = PathBuf::from(
        std::env::var_os("AIRK_NATIVE_FINAL205_CORPUS").context("missing fixed FINAL corpus")?,
    );
    let question_bytes = sealed(&corpus_path, CORPUS_PIN.parse()?)?;
    let corpus: Value = serde_json::from_slice(&question_bytes)?;
    let freeze_path = PathBuf::from(
        std::env::var_os("AIRK_NATIVE_FINAL205_CANDIDATE_FREEZE")
            .context("missing immutable candidate/export/CAL barrier")?,
    );
    let freeze_pin: Sha256Digest = std::env::var("AIRK_NATIVE_FINAL205_CANDIDATE_FREEZE_SHA256")
        .context("missing primary-reviewed frozen barrier SHA")?
        .parse()?;
    let freeze_bytes = sealed(&freeze_path, freeze_pin)?;
    let freeze: Value = serde_json::from_slice(&freeze_bytes)?;
    let manifest = PathBuf::from(
        std::env::var_os("AIRK_NATIVE_FINAL205_SOURCE_MANIFEST")
            .context("missing FINAL source manifest")?,
    );
    let manifest_pin: Sha256Digest = std::env::var("AIRK_NATIVE_FINAL205_SOURCE_MANIFEST_SHA256")
        .context("missing reviewed FINAL manifest SHA")?
        .parse()?;
    let metadata: Value = serde_json::from_slice(&sealed(&manifest, manifest_pin)?)?;
    ensure!(
        metadata["corpus_sha256"] == CORPUS_PIN
            && metadata["fresh_final_protocol_sha256"] == PROTOCOL_PIN
            && metadata["task_count"] == 205
            && metadata["original_paragraph_count"] == 1685,
        "foreign FINAL source closure"
    );
    let (source_base, source_rows, source_before) = final_source_closure(&manifest, manifest_pin)?;

    let data_root = PathBuf::from(
        std::env::var_os("AIRK_NATIVE_FINAL205_DATA")
            .context("missing explicit FINAL data root")?,
    );
    ensure!(
        data_root.is_absolute() && !data_root.starts_with(&source_base),
        "FINAL index data must be absolute and outside read-only fixture"
    );
    let data_root = if data_root.try_exists()? {
        data_root.canonicalize()?
    } else {
        data_root
            .parent()
            .context("FINAL data parent missing")?
            .canonicalize()?
            .join(data_root.file_name().context("FINAL data name missing")?)
    };
    ensure!(
        !data_root.starts_with(&source_base),
        "resolved FINAL data points inside read-only fixture"
    );
    let tasks = corpus["tasks"]
        .as_array()
        .context("missing opaque FINAL tasks")?;
    validate_population(tasks)?;
    ensure!(
        tasks.len() == 205
            && tasks.iter().filter(|t| t["dataset"] == "HotpotQA").count() == 171
            && tasks.iter().filter(|t| t["dataset"] == "QASPER").count() == 34,
        "fixed 171Hotpot/34QASPER population required"
    );
    let mut questions = Vec::new();
    let mut ids = BTreeSet::new();
    for task in tasks {
        ensure!(
            task["split"] == "test" && task["experiment_partition"] == "FINAL",
            "non-FINAL task refused"
        );
        let id = task["id"].as_str().context("missing original task ID")?;
        ensure!(ids.insert(id.to_string()), "duplicate FINAL question");
        let source_root = if task["dataset"] == "HotpotQA" {
            source_base.join("sources/hotpot")
        } else {
            let group = task["source_group"]
                .as_str()
                .context("missing QASPER family")?;
            let paper = group
                .strip_prefix("qasper:")
                .context("foreign QASPER scope")?;
            ensure!(
                !paper.is_empty() && !paper.contains(['/', '\\']),
                "invalid paper family"
            );
            source_base.join("sources/qasper").join(paper)
        }
        .canonicalize()?;
        questions.push(Question {
            id: id.into(),
            source_root,
            question: task["question"]
                .as_str()
                .context("missing original question")?
                .into(),
        });
    }
    let questions = Questions {
        data_root,
        questions,
    };
    let family = std::env::var("AIRK_NATIVE_ENCODER_FAMILY")
        .context("select ONE exact closed encoder family")?;
    let encoder_input_bytes = std::fs::read(
        std::env::var_os("AIRK_NATIVE_ENCODER_INPUT")
            .context("missing exact frozen native descriptors")?,
    )?;
    let input = final_family(&encoder_input_bytes, &freeze, &family)?;
    let output =
        std::env::var_os("AIRK_NATIVE_FINAL205_OUTPUT").context("missing native family receipt")?;
    ensure!(
        !PathBuf::from(&output).try_exists()?,
        "fresh FINAL output required; never overwrite previous trial"
    );
    let output_parent = PathBuf::from(&output)
        .parent()
        .context("FINAL output parent missing")?
        .canonicalize()?;
    ensure!(
        !output_parent.starts_with(&source_base),
        "FINAL receipt output inside read-only fixture refused"
    );
    let binary = std::env::current_exe()?;
    let binary_sha256 = Sha256Digest::from_bytes(Sha256::digest(std::fs::read(&binary)?).into());
    sealed(&input.directory.join("model.json"), input.model_json_sha256)?;
    ensure!(
        freeze["native_binary_sha256"] == serde_json::to_value(binary_sha256)?,
        "FINAL native binary differs from frozen barrier"
    );
    let control = tempfile::tempdir()?;
    let lease = Arc::new(airk_store::service::ServiceLease::acquire(control.path())?);
    let owner = NativeInferenceOwner::admit(lease)?;
    let expected_model = owner.run_encoder_evaluation(input.model_json_sha256, || {
        airk_retrieval::model::inspect_object_plan(&input.directory)
    })??;
    let expected = freeze["families"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["family"] == input.family)
        .context("missing frozen family")?;
    ensure!(
        expected["pipeline_digest"] == serde_json::to_value(expected_model.pipeline_digest)?,
        "actual native pipeline differs from frozen barrier"
    );
    let mut receipt = json!({"schema":"airk.native-fresh-final-retrieval-family/1","corpus_sha256":CORPUS_PIN,"fresh_final_protocol_sha256":PROTOCOL_PIN,"candidate_freeze_sha256":freeze_pin,"complete":false,"evaluation":true,"release_qualified":false,"family":input.family,"question_input_sha256":format!("{:x}",Sha256::digest(&question_bytes)),"encoder_input_sha256":format!("{:x}",Sha256::digest(&encoder_input_bytes)),"source_manifest_sha256":manifest_pin,"source_roster_sha256":source_before,"source_roster_count":source_rows.len(),"source_before_sha256":source_before,"source_after_sha256":null,"binary_sha256":binary_sha256,"model_json_sha256":input.model_json_sha256,"pipeline_digest":expected_model.pipeline_digest,"runtime":airk_retrieval::model::ONNX_RUNTIME,"query":{"mode":"vector","limit":10,"max_bytes":32768,"full_chunks":true},"collections":[],"records":[]});
    owner.run_encoder_evaluation(input.model_json_sha256,||->Result<()> {
        let mut kernels=BTreeMap::new();let mut collections=Vec::new();let mut records=Vec::new();
        for question in &questions.questions {
            if let Some(stop)=std::env::var_os("AIRK_NATIVE_FINAL205_STOP_FILE") {ensure!(!PathBuf::from(stop).try_exists()?,"owned stop requested at completed-query boundary; partial capture is not a pass");}
            let root=std::fs::canonicalize(&question.source_root)?;
            ensure!(root.starts_with(source_base.join("sources")),"question scope outside approved source fixture");
            let kernel=if let Some(kernel)=kernels.get(&root){crate::Kernel::clone(kernel)}else{
                let data=questions.data_root.join(&input.family).join(format!("index-{}",kernels.len()));let kernel=crate::Kernel::open(&root,Some(data))?;
                let reused=kernel.current_scope()?.is_some();
                if !reused {kernel.configure_vectors(true)?;kernel.install_default_vector_model(&input.directory)?;kernel.index()?;}
                let read=kernel.current_reader()?;let state=read.collection_state()?.context("native collection state missing")?;
                ensure!(state.configuration.model.as_ref().is_some_and(|model|model.local_set()==expected_model.local_set()),"reused source/model/pipeline publication differs");
                let token=read.token()?;
                let publication=airk_core::evidence::RevisionEnvelope::new(kernel.collection().into(),airk_core::identity::RevisionToken{incarnation:token.incarnation,generation:token.sequence,revision:token.digest});drop(read);
                let indexed_roster=verify_index_source_roster(&kernel,&source_base,&source_rows)?;
                collections.push(json!({"source_root":root,"publication":publication,"pipeline_digest":expected_model.pipeline_digest,"model_json_sha256":input.model_json_sha256,"indexed_source_roster_sha256":indexed_roster,"reused":reused}));
                kernels.insert(root.clone(),kernel.clone());kernel
            };
            let started=Instant::now();let vector=kernel.semantic_search(&airk_core::SearchQuery{text:question.question.clone(),path_prefix:None,limit:10,max_bytes:32768})?;
            ensure!(collections.iter().any(|collection|collection["source_root"]==serde_json::to_value(&root).unwrap() && collection["publication"]==serde_json::to_value(&vector.publication).unwrap()),"query publication differs from bound native collection");
            records.push(json!({"id":question.id,"source_root":root,"primary_vector":vector,"query_us":started.elapsed().as_micros()}));
            receipt["collections"]=serde_json::to_value(&collections)?;receipt["records"]=serde_json::to_value(&records)?;
            std::fs::write(&output,serde_json::to_vec_pretty(&receipt)?)?;
        }
        Ok(())
    })??;
    let (_, after_rows, source_after) = final_source_closure(&manifest, manifest_pin)?;
    ensure!(
        source_rows == after_rows && source_before == source_after,
        "approved source closure changed during native capture"
    );
    ensure!(
        sealed(&corpus_path, CORPUS_PIN.parse()?)? == question_bytes,
        "FINAL corpus changed during native run"
    );
    ensure!(
        sealed(&freeze_path, freeze_pin)? == freeze_bytes,
        "frozen candidate barrier changed during FINAL capture"
    );
    let encoder_after = std::fs::read(
        std::env::var_os("AIRK_NATIVE_ENCODER_INPUT").context("missing encoder input")?,
    )?;
    ensure!(
        encoder_after == encoder_input_bytes,
        "encoder descriptor inputs changed during native run"
    );
    receipt["source_after_sha256"] = serde_json::to_value(source_after)?;
    receipt["complete"] = true.into();
    std::fs::write(output, serde_json::to_vec_pretty(&receipt)?)?;
    Ok(())
}
