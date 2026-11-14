// SPDX-License-Identifier: Apache-2.0
//! Private native encoder oracles and actual current Kernel retrieval fixtures.
use super::*;
use airk_core::identity::Sha256Digest;
use airk_retrieval::model::{EmbeddingCacheContext, EmbeddingCacheStats, EmbeddingModel};
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    time::Instant,
};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EncoderInput {
    family: String,
    directory: PathBuf,
    model_json_sha256: Sha256Digest,
    oracle: PathBuf,
    oracle_sha256: Sha256Digest,
    oracle_result: String,
}
fn inputs() -> Result<Vec<EncoderInput>> {
    let inputs: Vec<EncoderInput> = serde_json::from_slice(&std::fs::read(
        std::env::var_os("AIRK_NATIVE_ENCODER_INPUT")
            .context("missing exact native encoder inputs")?,
    )?)?;
    let families: BTreeSet<_> = inputs.iter().map(|i| i.family.as_str()).collect();
    ensure!(
        inputs.len() == 6
            && families
                == BTreeSet::from([
                    "trained-v2-u8",
                    "trained-v2-s8",
                    "original-shipped-u8-ort130-evaluation",
                    "original-shipped-s8-ort130-evaluation",
                    "original-mixed-u8-ort130-evaluation",
                    "original-mixed-s8-ort130-evaluation"
                ]),
        "six independently pinned encoder families required"
    );
    Ok(inputs)
}
fn sealed(path: &std::path::Path, expected: Sha256Digest) -> Result<Vec<u8>> {
    let bytes = std::fs::read(path)?;
    ensure!(
        Sha256Digest::from_bytes(Sha256::digest(&bytes).into()) == expected,
        "actual private fixture bytes changed"
    );
    Ok(bytes)
}
fn distances(actual: &[f32], reference: &[f32]) -> Result<(f64, f64, f64)> {
    ensure!(
        actual.len() == 384
            && reference.len() == 384
            && actual.iter().chain(reference).all(|v| v.is_finite()),
        "native encoder requires384finite components"
    );
    let dot = actual
        .iter()
        .zip(reference)
        .map(|(a, b)| f64::from(*a) * f64::from(*b))
        .sum::<f64>();
    let a = actual
        .iter()
        .map(|v| f64::from(*v).powi(2))
        .sum::<f64>()
        .sqrt();
    let b = reference
        .iter()
        .map(|v| f64::from(*v).powi(2))
        .sum::<f64>()
        .sqrt();
    ensure!(
        a > 0.0 && b > 0.0 && (a - 1.0).abs() <= 1e-5,
        "native encoder unit normalization failed"
    );
    Ok((
        dot / (a * b),
        actual
            .iter()
            .zip(reference)
            .map(|(a, b)| (f64::from(*a) - f64::from(*b)).powi(2))
            .sum::<f64>()
            .sqrt(),
        actual
            .iter()
            .zip(reference)
            .map(|(a, b)| f64::from((*a - *b).abs()))
            .fold(0.0, f64::max),
    ))
}
#[test]
#[ignore = "six exact private native encoder assets and independent padded-input oracles required"]
fn six_encoder_families_match_independent_native_bucket_oracles_without_promoting_baselines()
-> Result<()> {
    let output =
        std::env::var_os("AIRK_NATIVE_ENCODER_OUTPUT").context("missing native encoder receipt")?;
    let mut results = Vec::new();
    for input in inputs()? {
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
            if input.family.starts_with("trained-") {ensure!(trained_seam,"unchanged trained-candidate FP32 admission floor failed");}
            Ok(json!({"family":input.family,"model_json_sha256":input.model_json_sha256,"oracle_sha256":input.oracle_sha256,"runtime":airk_retrieval::model::ONNX_RUNTIME,"pipeline":descriptor,"pipeline_digest":descriptor.identity()?,"load_us":load_us,"candidate_fp32_floor_passed":trained_seam,"baseline_failures_are_not_promoted":input.family.starts_with("original-"),"cases":measured,"mixed_length_execution":stats}))
        })??;
        results.push(record);
        std::fs::write(
            &output,
            serde_json::to_vec_pretty(
                &json!({"schema":"airk.native-encoder-oracles/1","complete":results.len()==6,"evaluation":true,"release_qualified":false,"families":results}),
            )?,
        )?;
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Questions {
    data_root: PathBuf,
    questions: Vec<Question>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Question {
    id: String,
    source_root: PathBuf,
    question: String,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, Deserialize)]
struct SourceRow {
    file: String,
    bytes: u64,
    sha256: Sha256Digest,
}
fn source_closure(
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
        expected.files.len() == 2772
            && expected
                .files
                .windows(2)
                .all(|rows| rows[0].file != rows[1].file),
        "frozen source manifest closed2772roster required"
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
fn verify_index_source_roster(
    kernel: &crate::Kernel,
    base: &std::path::Path,
    rows: &[SourceRow],
) -> Result<Sha256Digest> {
    let expected: BTreeMap<_, _> = rows
        .iter()
        .filter_map(|row| {
            let path = base.join(&row.file);
            path.strip_prefix(kernel.root()).ok().map(|relative| {
                (
                    relative.to_string_lossy().replace('\\', "/"),
                    (row.bytes, row.sha256.to_string()),
                )
            })
        })
        .collect();
    let snapshot = kernel
        .load_optional()?
        .context("native source publication missing")?;
    let actual: BTreeMap<_, _> = snapshot
        .artifacts
        .iter()
        .map(|(path, artifact)| {
            (
                path.clone(),
                (artifact.bytes as u64, artifact.content_hash.clone()),
            )
        })
        .collect();
    ensure!(
        actual == expected,
        "published native index does not match exact approved source roster"
    );
    Ok(Sha256Digest::from_bytes(
        Sha256::digest(serde_json::to_vec(&actual)?).into(),
    ))
}
#[test]
#[ignore = "ONE explicit closedencoderfamily; frozen205query and source manifest; all6required byaggregate"]
fn native_205_questions_use_the_same_current_kernel_collection_chunking_and_diskann_retrieval()
-> Result<()> {
    let question_bytes = std::fs::read(
        std::env::var_os("AIRK_NATIVE_RETRIEVAL_INPUT").context("missing frozen205questions")?,
    )?;
    let questions: Questions = serde_json::from_slice(&question_bytes)?;
    ensure!(
        questions.questions.len() == 205
            && questions
                .questions
                .iter()
                .map(|q| &q.id)
                .collect::<BTreeSet<_>>()
                .len()
                == 205,
        "full frozen205uniquequestions required"
    );
    ensure!(
        questions.data_root.is_absolute(),
        "private explicit data root required"
    );
    let family = std::env::var("AIRK_NATIVE_ENCODER_FAMILY")
        .context("select ONE exact closed encoder family")?;
    let input = inputs()?
        .into_iter()
        .find(|input| input.family == family)
        .context("unknown encoder family")?;
    let encoder_input_bytes =
        std::fs::read(std::env::var_os("AIRK_NATIVE_ENCODER_INPUT").unwrap())?;
    let output = std::env::var_os("AIRK_NATIVE_RETRIEVAL_OUTPUT")
        .context("missing native family receipt")?;
    let manifest = PathBuf::from(
        std::env::var_os("AIRK_NATIVE_RETRIEVAL_SOURCE_MANIFEST")
            .context("missing approved source manifest")?,
    );
    let manifest_pin: Sha256Digest = std::env::var("AIRK_NATIVE_RETRIEVAL_SOURCE_MANIFEST_SHA256")
        .context("missing approved manifest SHA")?
        .parse()?;
    let (source_base, source_rows, source_before) = source_closure(&manifest, manifest_pin)?;
    let binary = std::env::current_exe()?;
    let binary_sha256 = Sha256Digest::from_bytes(Sha256::digest(std::fs::read(&binary)?).into());
    sealed(&input.directory.join("model.json"), input.model_json_sha256)?;
    let control = tempfile::tempdir()?;
    let lease = Arc::new(airk_store::service::ServiceLease::acquire(control.path())?);
    let owner = NativeInferenceOwner::admit(lease)?;
    let expected_model = owner.run_encoder_evaluation(input.model_json_sha256, || {
        airk_retrieval::model::inspect_object_plan(&input.directory)
    })??;
    let mut receipt = json!({"schema":"airk.native-retrieval-family/2","complete":false,"evaluation":true,"release_qualified":false,"family":input.family,"question_input_sha256":format!("{:x}",Sha256::digest(&question_bytes)),"encoder_input_sha256":format!("{:x}",Sha256::digest(&encoder_input_bytes)),"source_manifest_sha256":manifest_pin,"source_roster_sha256":source_before,"source_roster_count":source_rows.len(),"source_before_sha256":source_before,"source_after_sha256":null,"binary_sha256":binary_sha256,"model_json_sha256":input.model_json_sha256,"pipeline_digest":expected_model.pipeline_digest,"runtime":airk_retrieval::model::ONNX_RUNTIME,"query":{"mode":"vector","limit":10,"max_bytes":32768,"full_chunks":true},"collections":[],"records":[]});
    owner.run_encoder_evaluation(input.model_json_sha256,||->Result<()> {
        let mut kernels=BTreeMap::new();let mut collections=Vec::new();let mut records=Vec::new();
        for question in &questions.questions {
            if let Some(stop)=std::env::var_os("AIRK_NATIVE_RETRIEVAL_STOP_FILE") {ensure!(!PathBuf::from(stop).try_exists()?,"owned stop requested at completed-query boundary; partial capture is not a pass");}
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
    let (_, after_rows, source_after) = source_closure(&manifest, manifest_pin)?;
    ensure!(
        source_rows == after_rows && source_before == source_after,
        "approved source closure changed during native capture"
    );
    receipt["source_after_sha256"] = serde_json::to_value(source_after)?;
    receipt["complete"] = true.into();
    std::fs::write(output, serde_json::to_vec_pretty(&receipt)?)?;
    Ok(())
}

#[test]
#[ignore = "exact pinned six-family input metadata required; no native model loaded"]
fn evaluation_setup_keeps_alternate_precision_and_old_baseline_behind_the_actual_owner()
-> Result<()> {
    let selected = airk_retrieval::model_catalog::selected_profile()?;
    let opposite = if selected.id == "minilm-l6-v2-u8-v2" {
        "trained-v2-s8"
    } else {
        "trained-v2-u8"
    };
    for input in inputs()?.into_iter().filter(|input| {
        input.family == opposite || input.family == "original-shipped-u8-ort130-evaluation"
    }) {
        sealed(&input.directory.join("model.json"), input.model_json_sha256)?;
        assert!(
            airk_retrieval::model::inspect_object_plan(&input.directory).is_err(),
            "product CPU default/catalog refusal must survive"
        );
        let fixture = tempfile::tempdir()?;
        let control = fixture.path().join("control");
        std::fs::create_dir(&control)?;
        let lease = Arc::new(airk_store::service::ServiceLease::acquire(&control)?);
        let owner = NativeInferenceOwner::admit(lease)?;
        let source = fixture.path().join("source");
        std::fs::create_dir(&source)?;
        let kernel = crate::Kernel::open(&source, Some(fixture.path().join("data")))?;
        kernel.configure_vectors(true)?;
        owner.run_encoder_evaluation(input.model_json_sha256, || -> Result<()> {
            kernel.install_default_vector_model(&input.directory)?;
            let set = airk_retrieval::model::inspect_object_plan(
                &kernel.data_dir().join("models/local"),
            )?;
            assert_eq!(
                set.assets[&airk_core::model_objects::ModelAssetName::ModelJson].sha256,
                input.model_json_sha256
            );
            Ok(())
        })??;
        assert!(
            airk_retrieval::model::inspect_object_plan(&kernel.data_dir().join("models/local"))
                .is_err(),
            "private admission must not escape its owner scope"
        );
    }
    Ok(())
}

#[test]
#[ignore = "retrospective exact R1 binary/capture/publication proof only; no model execution"]
fn retained_r1_trained_u8_capture_binds_its_original_binary_and_unchanged_published_sources()
-> Result<()> {
    let legacy_path = PathBuf::from(
        std::env::var_os("AIRK_NATIVE_RETRIEVAL_LEGACY_INPUT")
            .context("missing retained R1 capture")?,
    );
    let binary_path = PathBuf::from(
        std::env::var_os("AIRK_NATIVE_RETRIEVAL_LEGACY_BINARY")
            .context("missing original retained R1 executable")?,
    );
    let legacy_pin: Sha256Digest =
        "5fb11c1a6d204bb46c6cc59cfecb5be1a7abe73efd882ef99772cc8fd2999b4b".parse()?;
    let binary_pin: Sha256Digest =
        "2016caf851b93e7e1ec4267e1fa7fb2a965a4d9e0511c355e5e6a1badcf7f72c".parse()?;
    let legacy_bytes = sealed(&legacy_path, legacy_pin)?;
    sealed(&binary_path, binary_pin)?;
    let legacy: Value = serde_json::from_slice(&legacy_bytes)?;
    ensure!(
        legacy["complete"] == false
            && legacy["evaluation"] == true
            && legacy["release_qualified"] == false,
        "retained partial campaign identity changed"
    );
    let families = legacy["families"]
        .as_array()
        .context("retained families missing")?;
    ensure!(
        families.len() == 1 && families[0]["family"] == "trained-v2-u8",
        "exact retained completed first family required"
    );
    let family = &families[0];
    let records = family["records"]
        .as_array()
        .context("retained records missing")?;
    let question_bytes = std::fs::read(
        std::env::var_os("AIRK_NATIVE_RETRIEVAL_INPUT").context("missing frozen questions")?,
    )?;
    let question_pin: Sha256Digest =
        "2e74a873125834e17455c40879c0ba161626053fbd39742bcbbc73077d3dd046".parse()?;
    ensure!(
        Sha256Digest::from_bytes(Sha256::digest(&question_bytes).into()) == question_pin,
        "original question bytes changed"
    );
    let questions: Questions = serde_json::from_slice(&question_bytes)?;
    ensure!(
        records.len() == 205 && questions.questions.len() == 205,
        "all original205 queries required"
    );
    let encoder_bytes = std::fs::read(
        std::env::var_os("AIRK_NATIVE_ENCODER_INPUT").context("missing frozen encoders")?,
    )?;
    let encoder_pin: Sha256Digest =
        "3ead240726ebdba10577db6778bffbf5232eab10c50f2ae128d589e0a961ab81".parse()?;
    ensure!(
        Sha256Digest::from_bytes(Sha256::digest(&encoder_bytes).into()) == encoder_pin,
        "original encoder input bytes changed"
    );
    let input = inputs()?
        .into_iter()
        .find(|input| input.family == "trained-v2-u8")
        .context("original U8 input missing")?;
    ensure!(
        family["model_json_sha256"] == serde_json::to_value(input.model_json_sha256)?,
        "original model descriptor changed"
    );
    let manifest = PathBuf::from(
        std::env::var_os("AIRK_NATIVE_RETRIEVAL_SOURCE_MANIFEST")
            .context("missing approved source manifest")?,
    );
    let manifest_pin: Sha256Digest =
        "2627e54241d8e961cc03c005c02f1655be364462ada16bbafdc0434cf4192a60".parse()?;
    let (base, rows, before) = source_closure(&manifest, manifest_pin)?;
    let control = tempfile::tempdir()?;
    let owner = NativeInferenceOwner::admit(Arc::new(airk_store::service::ServiceLease::acquire(
        control.path(),
    )?))?;
    let model = owner.run_encoder_evaluation(input.model_json_sha256, || {
        airk_retrieval::model::inspect_object_plan(&input.directory)
    })??;
    let mut roots: BTreeMap<PathBuf, airk_core::evidence::RevisionEnvelope> = BTreeMap::new();
    let mut collections = Vec::new();
    let mut ids = BTreeSet::new();
    for (record, question) in records.iter().zip(&questions.questions) {
        ensure!(
            record["id"] == question.id && ids.insert(question.id.clone()),
            "original question roster/order changed"
        );
        let root = std::fs::canonicalize(&question.source_root)?;
        ensure!(
            root.starts_with(base.join("sources"))
                && record["source_root"] == serde_json::to_value(&root)?,
            "original source scope changed"
        );
        let publication = if let Some(publication) = roots.get(&root) {
            publication.clone()
        } else {
            let data = questions
                .data_root
                .join(&input.family)
                .join(format!("index-{}", roots.len()));
            let kernel = crate::Kernel::open(&root, Some(data))?;
            ensure!(
                kernel.current_scope()?.is_some(),
                "original publication no longer exists; fresh capture required"
            );
            let read = kernel.current_reader()?;
            let state = read
                .collection_state()?
                .context("original collection state missing")?;
            ensure!(
                state
                    .configuration
                    .model
                    .as_ref()
                    .is_some_and(|set| set.local_set() == model.local_set()),
                "original published model/pipeline differs"
            );
            let token = read.token()?;
            let publication = airk_core::evidence::RevisionEnvelope::new(
                kernel.collection().into(),
                airk_core::identity::RevisionToken {
                    incarnation: token.incarnation,
                    generation: token.sequence,
                    revision: token.digest,
                },
            );
            drop(read);
            let roster = verify_index_source_roster(&kernel, &base, &rows)?;
            collections.push(json!({"source_root":root,"publication":publication,"pipeline_digest":model.pipeline_digest,"model_json_sha256":input.model_json_sha256,"indexed_source_roster_sha256":roster,"reused":true}));
            roots.insert(root.clone(), publication.clone());
            publication
        };
        ensure!(
            record["primary_vector"]["publication"] == serde_json::to_value(publication)?,
            "retained result publication no longer matches actual immutable source snapshot"
        );
    }
    ensure!(
        roots.len() == 12 && family["collections"] == 12,
        "original twelve source scopes required"
    );
    let (_, after_rows, after) = source_closure(&manifest, manifest_pin)?;
    ensure!(
        rows == after_rows && before == after,
        "source closure changed during retrospective binding"
    );
    let output = PathBuf::from(
        std::env::var_os("AIRK_NATIVE_RETRIEVAL_OUTPUT")
            .context("missing fresh legacy binding output")?,
    );
    ensure!(!output.try_exists()?, "legacy binding output must be fresh");
    let result = json!({"schema":"airk.native-retrieval-family/2","complete":true,"evaluation":true,"release_qualified":false,"family":input.family,"question_input_sha256":question_pin,"encoder_input_sha256":encoder_pin,"source_manifest_sha256":manifest_pin,"source_roster_sha256":before,"source_roster_count":rows.len(),"source_before_sha256":before,"source_after_sha256":after,"binary_sha256":binary_pin,"model_json_sha256":input.model_json_sha256,"pipeline_digest":model.pipeline_digest,"runtime":family["runtime"],"query":{"mode":"vector","limit":10,"max_bytes":32768,"full_chunks":true},"collections":collections,"records":records,"legacy_provenance":{"kind":"retrospective-unchanged-published-source-binding","original_capture_sha256":legacy_pin,"original_binary_sha256":binary_pin,"original_nextest_run":"659c966a-e2fb-4da2-a6fb-cf2635727176","original_campaign_complete":false,"original_family_records":205,"original_source_tree_prerun_seal":null,"original_dirty_source_preseal":null,"original_query_us_semantics":"Retained legacy timing includes its original measurement boundary; do not reinterpret as new vector-only latency or cold performance.","binding_binary_sha256":format!("{:x}",Sha256::digest(std::fs::read(std::env::current_exe()?)?)),"source_closure_timing":"before/after THIS retrospective binding, not before original query execution","diagnostics":"Original lexical/hybrid diagnostic fields retained unchanged; primary vector is the frozen A1 query. This is quality reuse, not cold performance."}});
    std::fs::write(output, serde_json::to_vec_pretty(&result)?)?;
    Ok(())
}

mod development;
mod candidate;
mod fresh_final;
