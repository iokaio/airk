// SPDX-License-Identifier: Apache-2.0
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
    let model_sha256 = asset_sha(&directory.join("model.onnx"))?;
    ensure!(
        model_sha256 == if short == "u8" { U8_PIN } else { S8_PIN },
        "exact selected epoch1 ONNX required"
    );
    let tokenizer_sha256 = asset_sha(&directory.join("tokenizer.json"))?;
    ensure!(
        tokenizer_sha256 == "be50c3628f2bf5bb5e3a7f17b1f74611b2561a3a27eeab05e5aa30f411572037",
        "unchanged tokenizer required"
    );
    Ok(OnnxManifest {
        schema: 2,
        name: "sentence-transformers/all-MiniLM-L6-v2".into(),
        license: "Apache-2.0".into(),
        source: "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2".into(),
        revision: "10dbd2f06a8baf40ad285b037edda610c1b9a57c".into(),
        model_sha256,
        tokenizer_sha256,
        license_sha256: asset_sha(&directory.join("LICENSE"))?,
        dimensions: 384,
        max_tokens: 256,
        pooling: "attention-mask-mean-l2".into(),
        input_ids: "input_ids".into(),
        attention_mask: "attention_mask".into(),
        token_type_ids: Some("token_type_ids".into()),
        output: "last_hidden_state".into(),
    })
}
fn candidate_inputs() -> Result<Vec<EncoderInput>> {
    let path = PathBuf::from(
        std::env::var_os("AIRK_NATIVE_ENCODER_INPUT")
            .context("missing actual Rust-emitted candidate master")?,
    );
    let pin: Sha256Digest = std::env::var("AIRK_CANDIDATE_ENCODER_INPUT_SHA256")
        .context("missing primary-reviewed candidate master SHA")?
        .parse()?;
    let rows: Vec<EncoderInput> = serde_json::from_slice(&sealed(&path, pin)?)?;
    ensure!(
        rows.len() == 2
            && rows
                .iter()
                .map(|r| r.family.as_str())
                .collect::<BTreeSet<_>>()
                == BTreeSet::from(["visible-prefix-mining-r1-u8", "visible-prefix-mining-r1-s8"]),
        "exact separate candidate two-family set required"
    );
    for row in &rows {
        let short = if row.family.ends_with("-u8") {
            "u8"
        } else {
            "s8"
        };
        ensure!(
            row.oracle_sha256 == ORACLE_PIN.parse()?
                && row.oracle_result == format!("mixed-{short}"),
            "exact independent candidate oracle required"
        );
        sealed(&row.oracle, row.oracle_sha256)?;
        let manifest: OnnxManifest = serde_json::from_slice(&sealed(
            &row.directory.join("model.json"),
            row.model_json_sha256,
        )?)?;
        ensure!(
            manifest == typed_manifest(&row.directory, short)?,
            "candidate manifest differs from owning typed contract"
        );
    }
    Ok(rows)
}
#[test]
#[ignore = "explicit selected epoch1 private assets; emits Rust-owned native manifests before consumers"]
fn emit_selected_epoch1_candidate_native_manifests() -> Result<()> {
    let base = PathBuf::from(
        std::env::var_os("AIRK_CANDIDATE_EXPORTS").context("missing exact export root")?,
    )
    .canonicalize()?;
    let oracle =
        PathBuf::from(std::env::var_os("AIRK_CANDIDATE_ORACLE").context("missing exact oracle")?)
            .canonicalize()?;
    let output = PathBuf::from(
        std::env::var_os("AIRK_CANDIDATE_MASTER_OUTPUT")
            .context("missing fresh candidate master output")?,
    );
    let stage = PathBuf::from(
        std::env::var_os("AIRK_CANDIDATE_STAGE_SEAL")
            .context("missing complete export stage seal")?,
    );
    sealed(&stage, STAGE_PIN.parse()?)?;
    sealed(&base.join("receipt.json"), EXPORT_PIN.parse()?)?;
    sealed(&oracle, ORACLE_PIN.parse()?)?;
    ensure!(
        !output.try_exists()?,
        "never overwrite prior candidate master"
    );
    let mut rows = Vec::new();
    for short in ["u8", "s8"] {
        let directory = base.join(short);
        let path = directory.join("model.json");
        ensure!(
            !path.try_exists()?,
            "never overwrite prior native model manifest"
        );
        let manifest = typed_manifest(&directory, short)?;
        // LICENSE must exactly match the independently frozen owning export.
        let exported: Value =
            serde_json::from_slice(&sealed(&base.join("receipt.json"), EXPORT_PIN.parse()?)?)?;
        let profile = exported["profiles"]
            .as_array()
            .context("missing export profiles")?
            .iter()
            .find(|p| p["id"] == format!("visible-prefix-mining-r1-{short}"))
            .context("missing exact candidate profile")?;
        ensure!(
            profile["assets"]
                .as_array()
                .context("missing asset seals")?
                .iter()
                .any(|a| a["name"] == "LICENSE" && a["sha256"] == manifest.license_sha256),
            "LICENSE differs from frozen training attribution"
        );
        std::fs::write(&path, serde_json::to_vec_pretty(&manifest)?)?;
        let model_json_sha256: Sha256Digest = asset_sha(&path)?.parse()?;
        let control = tempfile::tempdir()?;
        let lease = Arc::new(airk_store::service::ServiceLease::acquire(control.path())?);
        let owner = NativeInferenceOwner::admit(lease)?;
        owner.run_encoder_evaluation(model_json_sha256, || {
            airk_retrieval::model::inspect_object_plan(&directory)
        })??;
        ensure!(
            owner.run(|| EmbeddingModel::open(&directory))?.is_err(),
            "product catalog must continue refusing development candidate"
        );
        rows.push(json!({"family":format!("visible-prefix-mining-r1-{short}"),"directory":directory,"model_json_sha256":model_json_sha256,"oracle":oracle,"oracle_sha256":ORACLE_PIN,"oracle_result":format!("mixed-{short}")}));
    }
    std::fs::write(output, serde_json::to_vec_pretty(&rows)?)?;
    Ok(())
}

#[test]
#[ignore = "two exact epoch1 candidate assets and independent padded-input oracles required"]
fn candidate_two_families_match_independent_native_bucket_oracles() -> Result<()> {
    let output =
        std::env::var_os("AIRK_NATIVE_ENCODER_OUTPUT").context("missing native encoder receipt")?;
    let mut results = Vec::new();
    for input in candidate_inputs()? {
        sealed(&input.directory.join("model.json"), input.model_json_sha256)?;
        let oracle_bytes = sealed(&input.oracle, input.oracle_sha256)?;
        let oracle: Value = serde_json::from_slice(&oracle_bytes)?;
        let cases = oracle["cases"]
            .as_array()
            .context("missing independent encoder cases")?;
        ensure!(
            cases.len() == 12
                && cases
                    .iter()
                    .filter_map(|c| c["bucket"].as_u64())
                    .collect::<BTreeSet<_>>()
                    == BTreeSet::from([32, 64, 128, 256]),
            "all original10 plus independent64/128 bucket cases required"
        );
        let root = tempfile::tempdir()?;
        let lease = Arc::new(airk_store::service::ServiceLease::acquire(root.path())?);
        let owner = NativeInferenceOwner::admit(lease)?;
        if input.family.starts_with("original-") {
            ensure!(
                owner
                    .run(|| EmbeddingModel::open(&input.directory))?
                    .is_err(),
                "product catalog must keep refusing original baseline weights"
            );
        }
        let record=owner.run_encoder_evaluation(input.model_json_sha256,||->Result<Value>{
            let started=Instant::now();let model=EmbeddingModel::open(&input.directory)?;let load_us=started.elapsed().as_micros();
            let descriptor=model.cache_descriptor()?;let mut measured=Vec::new();let mut trained_seam=true;
            for case in cases {
                let text=case["text"].as_str().context("missing exact caller text")?;
                let tensors=model.evaluation_inputs(text)?;assert_eq!(tensors,case["inputs"],"native token IDs/masks/types differ from independent padded oracle");
                let expected:Vec<f32>=serde_json::from_value(case["results"][&input.oracle_result]["vector"].clone())?;
                let reference:Vec<f32>=serde_json::from_value(case["reference_vector"].clone())?;
                let started=Instant::now();let actual=model.embed_text(text)?.context("unexpected absent native vector")?;let inference_us=started.elapsed().as_micros();
                let (_,same_graph_l2,error)=distances(&actual,&expected)?;let (cosine,l2,_)=distances(&actual,&reference)?;
                trained_seam &= cosine>=0.995 && l2<=0.1;
                measured.push(json!({"name":case["name"],"inputs":tensors,"native_vector":actual,"same_graph_expected_vector":expected,"original_fp32_reference":reference,"max_component_error":error,"same_graph_l2":same_graph_l2,"candidate_vs_fp32_cosine":cosine,"candidate_vs_fp32_l2":l2,"inference_us":inference_us}));
                std::fs::write(&output,serde_json::to_vec_pretty(&json!({"schema":"airk.native-encoder-oracles/1","complete":false,"evaluation":true,"release_qualified":false,"families":results,"current_family":input.family,"current_cases":measured}))?)?;
                ensure!(error<=1e-5,"native SAMEgraph oracle component tolerance failed; exact receipt retained");
            }
            let texts:Vec<_>=cases.iter().map(|c|c["text"].as_str().unwrap()).collect();let mut stats=EmbeddingCacheStats::default();
            let batch=model.embed_batch(&texts,&mut EmbeddingCacheContext{cache:None,bypass:true,stats:&mut stats,max_workers:8,working_memory_bytes:1<<30})?;
            for (i,vector) in batch.iter().enumerate() {assert_eq!(serde_json::to_value(vector.as_ref().unwrap())?,measured[i]["native_vector"],"mixed-length caller order changed native B1 results");}
            ensure!(stats.workers==1,"native B1 encoder must retain serial per-text forwards");
            if input.family.starts_with("visible-prefix-mining-") {ensure!(trained_seam,"unchanged trained-candidate FP32 admission floor failed");}
            Ok(json!({"family":input.family,"model_json_sha256":input.model_json_sha256,"oracle_sha256":input.oracle_sha256,"runtime":airk_retrieval::model::ONNX_RUNTIME,"pipeline":descriptor,"pipeline_digest":descriptor.identity()?,"load_us":load_us,"candidate_fp32_floor_passed":trained_seam,"baseline_failures_are_not_promoted":input.family.starts_with("original-"),"cases":measured,"mixed_length_execution":stats}))
        })??;
        results.push(record);
        std::fs::write(
            &output,
            serde_json::to_vec_pretty(
                &json!({"schema":"airk.native-encoder-oracles/1","complete":results.len()==2,"evaluation":true,"release_qualified":false,"families":results}),
            )?,
        )?;
    }
    Ok(())
}

// SPDX-License-Identifier: Apache-2.0
// Separate development-only capture. Original205 entry and grader stay unchanged.

const CORPUS_PIN: &str = "f6c6616c1de29948deee9f37c70ccaadbb7a5dc71d0cd02c91d207c9ccbac097";
const MASK_PIN: &str = "76842592dab174fd90bc3918542759adbf9cea615080334a308acafd6924727d";
const PROTOCOL_PIN: &str = "0fa7ffbfe381d0804704710e683f85a897d59a661073ba24e849fe53b3e298e8";

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
fn candidate_identity_gate_retains_all64_including44_refusals() -> Result<()> {
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
#[ignore = "one selected candidate DEV64 native run; no final promotion; explicit reviewed inputs"]
fn candidate_dev64_questions_use_current_kernel_with_fixed20_quality_mask() -> Result<()> {
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
    let input = candidate_inputs()?
        .into_iter()
        .find(|input| input.family == family)
        .context("unknown encoder family")?;
    let encoder_input_bytes =
        std::fs::read(std::env::var_os("AIRK_NATIVE_ENCODER_INPUT").unwrap())?;
    ensure!(
        Sha256Digest::from_bytes(Sha256::digest(&encoder_input_bytes).into())
            == std::env::var("AIRK_CANDIDATE_ENCODER_INPUT_SHA256")
                .context("missing primary-reviewed candidate master SHA")?
                .parse()?,
        "reviewed candidate descriptor inputs changed"
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
