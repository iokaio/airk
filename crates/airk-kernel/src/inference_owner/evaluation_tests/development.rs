// SPDX-License-Identifier: Apache-2.0
//! Separate development-only capture. Original205 entry and grader stay unchanged.
use super::*;
const CORPUS_PIN: &str = "f6c6616c1de29948deee9f37c70ccaadbb7a5dc71d0cd02c91d207c9ccbac097";
const MASK_PIN: &str = "76842592dab174fd90bc3918542759adbf9cea615080334a308acafd6924727d";
const PROTOCOL_PIN: &str = "0fa7ffbfe381d0804704710e683f85a897d59a661073ba24e849fe53b3e298e8";
const ENCODERS_PIN: &str = "3ead240726ebdba10577db6778bffbf5232eab10c50f2ae128d589e0a961ab81";
fn validate_dev_ids(ids: &BTreeSet<String>, mask: &Value) -> Result<()> {
    let read = |name: &str| -> Result<BTreeSet<String>> {
        mask[name]
            .as_array()
            .context("missing quality/refusal mask")?
            .iter()
            .map(|v| {
                Ok(format!(
                    "hotpot:{}",
                    v.as_str().context("nonstring mask ID")?
                ))
            })
            .collect()
    };
    let quality = read("quality_question_ids")?;
    let refused = read("refused_question_ids")?;
    ensure!(
        quality.len() == 20 && refused.len() == 44 && quality.is_disjoint(&refused),
        "fixed disjoint20/44 mask required"
    );
    ensure!(
        ids.len() == 64 && *ids == quality.union(&refused).cloned().collect(),
        "exact64 original DEV IDs, including all44 refusals, required"
    );
    Ok(())
}

#[test]
fn development_identity_gate_retains_all64_including44_refusals() -> Result<()> {
    let mask = json!({
        "quality_question_ids": (0..20).map(|i| format!("q{i}")).collect::<Vec<_>>(),
        "refused_question_ids": (20..64).map(|i| format!("q{i}")).collect::<Vec<_>>()
    });
    let ids: BTreeSet<_> = (0..64).map(|i| format!("hotpot:q{i}")).collect();
    validate_dev_ids(&ids, &mask)?;
    let mut missing = ids.clone();
    missing.remove("hotpot:q63");
    assert!(validate_dev_ids(&missing, &mask).is_err());
    let mut overlapping = mask.clone();
    overlapping["refused_question_ids"][0] = "q0".into();
    assert!(validate_dev_ids(&ids, &overlapping).is_err());
    let mut empty = mask;
    empty["quality_question_ids"] = json!([]);
    assert!(validate_dev_ids(&ids, &empty).is_err());
    Ok(())
}
fn dev_source_closure(
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
        expected.files.len() == 366
            && expected
                .files
                .windows(2)
                .all(|rows| rows[0].file != rows[1].file),
        "frozen source manifest closed366DEVroster required"
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
#[test]
#[ignore = "one matched six-family DEV64 native run; no final promotion; explicit reviewed inputs"]
fn native_dev64_questions_use_current_kernel_with_fixed20_quality_mask() -> Result<()> {
    let corpus_path = PathBuf::from(
        std::env::var_os("AIRK_NATIVE_DEV64_CORPUS").context("missing fixed DEV corpus")?,
    );
    let question_bytes = sealed(&corpus_path, CORPUS_PIN.parse()?)?;
    let corpus: Value = serde_json::from_slice(&question_bytes)?;
    let mask_path = PathBuf::from(
        std::env::var_os("AIRK_NATIVE_DEV64_MASK").context("missing fixed DEV mask")?,
    );
    let mask: Value = serde_json::from_slice(&sealed(&mask_path, MASK_PIN.parse()?)?)?;
    let manifest = PathBuf::from(
        std::env::var_os("AIRK_NATIVE_DEV64_SOURCE_MANIFEST")
            .context("missing DEV source manifest")?,
    );
    let manifest_pin: Sha256Digest = std::env::var("AIRK_NATIVE_DEV64_SOURCE_MANIFEST_SHA256")
        .context("missing reviewed DEV manifest SHA")?
        .parse()?;
    let metadata: Value = serde_json::from_slice(&sealed(&manifest, manifest_pin)?)?;
    ensure!(
        metadata["corpus_sha256"] == CORPUS_PIN
            && metadata["quality_mask_sha256"] == MASK_PIN
            && metadata["development_protocol_sha256"] == PROTOCOL_PIN,
        "foreign DEV source closure"
    );
    let (source_base, source_rows, source_before) = dev_source_closure(&manifest, manifest_pin)?;
    let source_root = source_base.join("sources/hotpot").canonicalize()?;
    let data_root = PathBuf::from(
        std::env::var_os("AIRK_NATIVE_DEV64_DATA").context("missing explicit DEV data root")?,
    );
    ensure!(
        data_root.is_absolute() && !data_root.starts_with(&source_base),
        "DEV index data must be absolute and outside read-only fixture"
    );
    let data_root = if data_root.try_exists()? {
        data_root.canonicalize()?
    } else {
        data_root
            .parent()
            .context("DEV data parent missing")?
            .canonicalize()?
            .join(data_root.file_name().context("DEV data name missing")?)
    };
    ensure!(
        !data_root.starts_with(&source_base),
        "resolved DEV data points inside read-only fixture"
    );
    let tasks = corpus["tasks"].as_array().context("missing DEV tasks")?;
    let mut questions = Vec::new();
    for task in tasks {
        ensure!(task["split"] == "development", "non-DEV task refused");
        questions.push(Question {
            id: task["id"].as_str().context("missing original ID")?.into(),
            source_root: source_root.clone(),
            question: task["question"]
                .as_str()
                .context("missing original question")?
                .into(),
        });
    }
    validate_dev_ids(&questions.iter().map(|q| q.id.clone()).collect(), &mask)?;
    ensure!(
        questions.len() == 64,
        "all fixed64 original questions required"
    );
    let questions = Questions {
        data_root,
        questions,
    };
    let family = std::env::var("AIRK_NATIVE_ENCODER_FAMILY")
        .context("select ONE exact closed encoder family")?;
    let input = inputs()?
        .into_iter()
        .find(|input| input.family == family)
        .context("unknown encoder family")?;
    let encoder_input_bytes =
        std::fs::read(std::env::var_os("AIRK_NATIVE_ENCODER_INPUT").unwrap())?;
    ensure!(
        Sha256Digest::from_bytes(Sha256::digest(&encoder_input_bytes).into())
            == ENCODERS_PIN.parse()?,
        "original six family descriptor inputs changed"
    );
    let output =
        std::env::var_os("AIRK_NATIVE_DEV64_OUTPUT").context("missing native family receipt")?;
    ensure!(
        !PathBuf::from(&output).try_exists()?,
        "fresh DEV output required; never overwrite previous trial"
    );
    let output_parent = PathBuf::from(&output)
        .parent()
        .context("DEV output parent missing")?
        .canonicalize()?;
    ensure!(
        !output_parent.starts_with(&source_base),
        "DEV receipt output inside read-only fixture refused"
    );
    let binary = std::env::current_exe()?;
    let binary_sha256 = Sha256Digest::from_bytes(Sha256::digest(std::fs::read(&binary)?).into());
    sealed(&input.directory.join("model.json"), input.model_json_sha256)?;
    let control = tempfile::tempdir()?;
    let lease = Arc::new(airk_store::service::ServiceLease::acquire(control.path())?);
    let owner = NativeInferenceOwner::admit(lease)?;
    let expected_model = owner.run_encoder_evaluation(input.model_json_sha256, || {
        airk_retrieval::model::inspect_object_plan(&input.directory)
    })??;
    let mut receipt = json!({"schema":"airk.native-development-retrieval-family/1","corpus_sha256":CORPUS_PIN,"quality_mask_sha256":MASK_PIN,"development_protocol_sha256":PROTOCOL_PIN,"quality_denominator":20,"alignment_refusals":44,"complete":false,"evaluation":true,"release_qualified":false,"family":input.family,"question_input_sha256":format!("{:x}",Sha256::digest(&question_bytes)),"encoder_input_sha256":format!("{:x}",Sha256::digest(&encoder_input_bytes)),"source_manifest_sha256":manifest_pin,"source_roster_sha256":source_before,"source_roster_count":source_rows.len(),"source_before_sha256":source_before,"source_after_sha256":null,"binary_sha256":binary_sha256,"model_json_sha256":input.model_json_sha256,"pipeline_digest":expected_model.pipeline_digest,"runtime":airk_retrieval::model::ONNX_RUNTIME,"query":{"mode":"vector","limit":10,"max_bytes":32768,"full_chunks":true},"collections":[],"records":[]});
    owner.run_encoder_evaluation(input.model_json_sha256,||->Result<()> {
        let mut kernels=BTreeMap::new();let mut collections=Vec::new();let mut records=Vec::new();
        for question in &questions.questions {
            if let Some(stop)=std::env::var_os("AIRK_NATIVE_DEV64_STOP_FILE") {ensure!(!PathBuf::from(stop).try_exists()?,"owned stop requested at completed-query boundary; partial capture is not a pass");}
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
    let (_, after_rows, source_after) = dev_source_closure(&manifest, manifest_pin)?;
    ensure!(
        source_rows == after_rows && source_before == source_after,
        "approved source closure changed during native capture"
    );
    ensure!(
        sealed(&corpus_path, CORPUS_PIN.parse()?)? == question_bytes,
        "DEV corpus changed during native run"
    );
    sealed(&mask_path, MASK_PIN.parse()?)?;
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
