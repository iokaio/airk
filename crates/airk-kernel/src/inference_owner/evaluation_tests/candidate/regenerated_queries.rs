// SPDX-License-Identifier: Apache-2.0
//! PRIVATE draft child of existing evaluation_tests::candidate; original capture entry unchanged.
use super::*;
use std::{fs::OpenOptions, io::Write, time::Duration};

const OLD_CAPTURE: &str = "9aec2ccc46b1d278db426da62183078702496879dd826442f5b3fb8f999caeb9";
const REGENERATION: &str = "24d32e696ef6b20d27b5983ec9295d0eb7d23a6c6868f76f68cd12ef1aa13a00";
const PROBE_INPUT: &str = "17917b0cfdee7b0e909edd5ca5227a4ce75366a4683d9935b7aa81048287886f";

fn require_same_query_population(corpus: &Value, mask: &Value, original: &Value) -> Result<()> {
    let tasks = corpus["tasks"]
        .as_array()
        .context("original tasks missing")?;
    let ids: Vec<_> = tasks
        .iter()
        .map(|t| t["id"].as_str().context("original ID missing"))
        .collect::<Result<_>>()?;
    let previous: Vec<_> = original["records"]
        .as_array()
        .context("original records missing")?
        .iter()
        .map(|t| t["id"].as_str().context("original record ID missing"))
        .collect::<Result<_>>()?;
    ensure!(
        ids.len() == 64 && ids == previous && tasks.iter().all(|t| t["split"] == "development"),
        "same ALL64 original DEV query order required"
    );
    validate_dev_ids(&ids.into_iter().map(str::to_owned).collect(), mask)
}

fn validate_all_query_shapes(corpus: &Value) -> Result<()> {
    for task in corpus["tasks"].as_array().context("tasks missing")? {
        airk_core::SearchQuery {
            text: task["question"]
                .as_str()
                .context("original question missing")?
                .into(),
            path_prefix: None,
            limit: 10,
            max_bytes: 32768,
        }
        .validate()?;
    }
    Ok(())
}

fn query_phase(value: Value) -> Result<()> {
    eprintln!("{value}");
    std::io::stderr().flush()?;
    Ok(())
}

fn run_query_only(preflight: bool) -> Result<()> {
    let started = Instant::now();
    let deadline = started + Duration::from_secs(1800);
    let corpus_path = PathBuf::from(
        std::env::var_os("AIRK_NATIVE_DEV64_CORPUS").context("original DEV corpus required")?,
    );
    let question_bytes = sealed(&corpus_path, CORPUS_PIN.parse()?)?;
    let corpus: Value = serde_json::from_slice(&question_bytes)?;
    let mask_path = PathBuf::from(
        std::env::var_os("AIRK_NATIVE_DEV64_MASK").context("original mask required")?,
    );
    let mask: Value = serde_json::from_slice(&sealed(&mask_path, MASK_PIN.parse()?)?)?;
    let old_path = PathBuf::from(
        std::env::var_os("AIRK_NATIVE_DEV64_ORIGINAL_CAPTURE")
            .context("original failed DEV capture required")?,
    );
    let original: Value = serde_json::from_slice(&sealed(&old_path, OLD_CAPTURE.parse()?)?)?;
    require_same_query_population(&corpus, &mask, &original)?;
    validate_all_query_shapes(&corpus)?;
    let regeneration_path = PathBuf::from(
        std::env::var_os("AIRK_NATIVE_DEV64_REGENERATION_RECEIPT")
            .context("actual regeneration receipt required")?,
    );
    let regeneration: Value =
        serde_json::from_slice(&sealed(&regeneration_path, REGENERATION.parse()?)?)?;
    ensure!(
        regeneration["complete"] == true
            && regeneration["query_execution"] == false
            && regeneration["result"]["embedding_actual_forwards"] == 0
            && regeneration["result"]["embedding_inferences"] == 0
            && regeneration["result"]["sources"] == 366
            && regeneration["result"]["vectors"] == 378
            && regeneration["result"]["exact_original_vector_bits_and_order"] == true,
        "actual complete zero-NN reconstruction required"
    );
    let proof = PathBuf::from(
        std::env::var_os("AIRK_NATIVE_DEV64_REGENERATED_INPUT")
            .context("actual native proof input required")?,
    );
    sealed(&proof, PROBE_INPUT.parse()?)?;
    let manifest = PathBuf::from(
        std::env::var_os("AIRK_NATIVE_DEV64_SOURCE_MANIFEST")
            .context("original manifest required")?,
    );
    let manifest_pin: Sha256Digest =
        std::env::var("AIRK_NATIVE_DEV64_SOURCE_MANIFEST_SHA256")?.parse()?;
    let metadata: Value = serde_json::from_slice(&sealed(&manifest, manifest_pin)?)?;
    ensure!(
        metadata["corpus_sha256"] == CORPUS_PIN
            && metadata["quality_mask_sha256"] == MASK_PIN
            && metadata["development_protocol_sha256"] == PROTOCOL_PIN,
        "original source closure required"
    );
    let (source_base, source_rows, source_before) = dev_source_closure(&manifest, manifest_pin)?;
    let source = source_base.join("sources/hotpot").canonicalize()?;
    let data = PathBuf::from(
        regeneration["result"]["new_data_root"]
            .as_str()
            .context("new data root missing")?,
    )
    .canonicalize()?;
    ensure!(
        data.is_dir() && !data.starts_with(&source_base),
        "existing disjoint regenerated data required"
    );
    let kernel =
        crate::Kernel::open_collection_for_diagnostics(&source, Some(data.clone()), "default")?;
    let publication = super::super::ann_self::require_exact_probe_publication(
        &kernel,
        &proof,
        PROBE_INPUT.parse()?,
    )?;
    let indexed_roster = verify_index_source_roster(&kernel, &source_base, &source_rows)?;
    let read = kernel.current_reader()?;
    let expected_head = read.head().clone();
    let model = read
        .collection_state()?
        .context("regenerated configuration missing")?
        .configuration
        .model
        .context("model missing")?;
    ensure!(
        model.pipeline_digest.to_string()
            == original["pipeline_digest"]
                .as_str()
                .context("old pipeline missing")?,
        "original embedding pipeline required"
    );
    drop(read);
    let family = "visible-prefix-mining-r1-u8";
    ensure!(
        original["family"] == family,
        "only exact original U8 family admitted"
    );
    let input = candidate_inputs()?
        .into_iter()
        .find(|i| i.family == family)
        .context("original U8 descriptor missing")?;
    let encoder_path = PathBuf::from(
        std::env::var_os("AIRK_NATIVE_ENCODER_INPUT").context("original encoder input required")?,
    );
    let encoder_input_bytes = sealed(
        &encoder_path,
        "0b58b97ee107c9c2c3e4adfefa7b6bf215a9c2d59a1966167b81e3f88224f718".parse()?,
    )?;
    let manifest_asset = model
        .assets
        .get(&airk_core::model_objects::ModelAssetName::ModelJson)
        .context("published manifest missing")?;
    ensure!(
        manifest_asset.sha256 == input.model_json_sha256
            && input.model_json_sha256.to_string() == original["model_json_sha256"],
        "original exact model descriptor required"
    );
    let output = PathBuf::from(
        std::env::var_os("AIRK_NATIVE_DEV64_OUTPUT").context("fresh query capture required")?,
    );
    let output_parent = output
        .parent()
        .context("output parent missing")?
        .canonicalize()?;
    ensure!(
        output.is_absolute()
            && output_parent == output.parent().unwrap()
            && !output.try_exists()?,
        "fresh canonical capture output required"
    );
    let anchor = PathBuf::from(
        std::env::var_os("AIRK_NATIVE_DEV64_QUERY_ANCHOR")
            .context("explicit private query anchor required")?,
    )
    .canonicalize()?;
    let control = PathBuf::from(
        std::env::var_os("AIRK_NATIVE_DEV64_QUERY_CONTROL")
            .context("explicit private query control required")?,
    )
    .canonicalize()?;
    ensure!(
        output == anchor.join("capture.json")
            && control == anchor.join("control")
            && control.is_dir(),
        "exact disjoint query anchor children required"
    );
    let frozen_roots: [&std::path::Path; 10] = [
        &source_base,
        &data,
        old_path.parent().unwrap(),
        regeneration_path.parent().unwrap(),
        proof.parent().unwrap(),
        &input.directory,
        corpus_path.parent().unwrap(),
        mask_path.parent().unwrap(),
        encoder_path.parent().unwrap(),
        manifest.parent().unwrap(),
    ];
    for frozen in frozen_roots {
        let frozen = frozen.canonicalize()?;
        ensure!(
            !anchor.starts_with(&frozen) && !frozen.starts_with(&anchor),
            "query output overlaps frozen inputs"
        );
    }
    for entry in std::fs::read_dir(&anchor)? {
        ensure!(
            entry?.path() == control,
            "fresh query anchor has unexpected entries"
        );
    }
    if preflight {
        eprintln!(
            "{}",
            json!({"preflight":true,"query_count":64,"quality_denominator":20,"alignment_refusals":44,
            "data_root":data,"publication":publication,"source_roster_count":source_rows.len(),"model":model,
            "indexing":false,"neural_execution":false})
        );
        return Ok(());
    }
    let lease = Arc::new(airk_store::service::ServiceLease::acquire(&control)?);
    let owner = NativeInferenceOwner::admit(lease)?;
    let binary_sha256 =
        Sha256Digest::from_bytes(Sha256::digest(std::fs::read(std::env::current_exe()?)?).into());
    let mut receipt = json!({"schema":"airk.native-development-retrieval-family/1","complete":false,"evaluation":true,
        "release_qualified":false,"family":family,"corpus_sha256":CORPUS_PIN,"quality_mask_sha256":MASK_PIN,
        "development_protocol_sha256":PROTOCOL_PIN,"quality_denominator":20,"alignment_refusals":44,
        "question_input_sha256":CORPUS_PIN,"encoder_input_sha256":format!("{:x}",Sha256::digest(&encoder_input_bytes)),
        "source_manifest_sha256":manifest_pin,"source_roster_sha256":source_before,"source_roster_count":source_rows.len(),
        "source_before_sha256":source_before,"source_after_sha256":null,"binary_sha256":binary_sha256,
        "model_json_sha256":input.model_json_sha256,"pipeline_digest":model.pipeline_digest,
        "runtime":airk_retrieval::model::ONNX_RUNTIME,"query":{"mode":"vector","limit":10,"max_bytes":32768,"full_chunks":true},
        "collections":[{"source_root":source,"publication":publication,"pipeline_digest":model.pipeline_digest,
            "model_json_sha256":input.model_json_sha256,"indexed_source_roster_sha256":indexed_roster,"reused":true}],
        "query_only":true,"regeneration_receipt_sha256":REGENERATION,"regenerated_native_input_sha256":PROBE_INPUT,
        "original_capture_sha256":OLD_CAPTURE,"indexing_performed":false,"records":[]});
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&output)?;
    file.write_all(&serde_json::to_vec_pretty(&receipt)?)?;
    file.sync_all()?;
    drop(file);
    owner.run_encoder_evaluation(input.model_json_sha256,||->Result<()> {
        ensure!(airk_retrieval::model::inspect_object_plan(&input.directory)?.local_set()==model.local_set(),"query model assets differ");
        let mut records=Vec::new();
        for (ordinal,task) in corpus["tasks"].as_array().unwrap().iter().enumerate() {
            ensure!(Instant::now()<deadline,"original whole DEV query deadline exceeded");
            ensure!(kernel.current_reader()?.head()==&expected_head,"regenerated head changed before query");
            let question=task["question"].as_str().context("original question missing")?;
            query_phase(json!({"phase":"query_start","ordinal":ordinal+1,"id":task["id"],
                "whole_elapsed_ms":started.elapsed().as_millis(),"remaining_ms":deadline.saturating_duration_since(Instant::now()).as_millis()}))?;
            let query_started=Instant::now();
            let actual=kernel.semantic_search(&airk_core::SearchQuery{text:question.into(),path_prefix:None,limit:10,max_bytes:32768});
            let result=match actual {
                Ok(result)=>result,
                Err(error)=>{
                    let chain=format!("{error:#}");
                    let failure=json!({"id":task["id"],"source_root":source,"native_error":chain,
                        "query_us":query_started.elapsed().as_micros(),"native_result":null});
                    records.push(failure.clone());receipt["records"]=serde_json::to_value(&records)?;
                    receipt["failed_query"]=failure;
                    std::fs::write(&output,serde_json::to_vec_pretty(&receipt)?)?;
                    query_phase(json!({"phase":"query_failed","ordinal":ordinal+1,"id":task["id"],
                        "query_us":query_started.elapsed().as_micros(),"whole_elapsed_ms":started.elapsed().as_millis(),"native_error":chain}))?;
                    return Err(error);
                }
            };
            records.push(json!({"id":task["id"],"source_root":source,"primary_vector":result,"query_us":query_started.elapsed().as_micros()}));
            receipt["records"]=serde_json::to_value(&records)?;
            std::fs::write(&output,serde_json::to_vec_pretty(&receipt)?)?;
            query_phase(json!({"phase":"query_done","ordinal":ordinal+1,"id":task["id"],
                "query_us":query_started.elapsed().as_micros(),"whole_elapsed_ms":started.elapsed().as_millis(),
                "native_error":null,"hits":result.hits.len(),"truncated":result.truncated,
                "execution":result.execution,"semantic_readiness":result.semantic_readiness}))?;
            ensure!(result.publication==Some(publication.clone()),"query returned another publication; full result retained");
        }
        Ok(())
    })??;
    ensure!(
        super::super::ann_self::require_exact_probe_publication(
            &kernel,
            &proof,
            PROBE_INPUT.parse()?
        )? == publication,
        "regenerated publication changed before capture delivery"
    );
    let (_, after_rows, source_after) = dev_source_closure(&manifest, manifest_pin)?;
    ensure!(
        after_rows == source_rows && source_after == source_before,
        "original source tree changed"
    );
    ensure!(
        sealed(&corpus_path, CORPUS_PIN.parse()?)? == question_bytes,
        "original DEV corpus changed"
    );
    ensure!(
        sealed(
            &encoder_path,
            "0b58b97ee107c9c2c3e4adfefa7b6bf215a9c2d59a1966167b81e3f88224f718".parse()?
        )? == encoder_input_bytes,
        "encoder descriptors changed"
    );
    sealed(&old_path, OLD_CAPTURE.parse()?)?;
    sealed(&regeneration_path, REGENERATION.parse()?)?;
    sealed(&mask_path, MASK_PIN.parse()?)?;
    ensure!(
        Instant::now() < deadline,
        "original whole query deadline exceeded"
    );
    ensure!(
        receipt["records"].as_array().unwrap().len() == 64,
        "all64 query outcomes required"
    );
    receipt["source_after_sha256"] = serde_json::to_value(source_after)?;
    receipt["complete"] = true.into();
    std::fs::write(&output, serde_json::to_vec_pretty(&receipt)?)?;
    Ok(())
}

#[test]
#[ignore = "read-only geometry proof of exact regenerated DEV source/model/head; no NN"]
fn regenerated_dev64_query_only_preflight_binds_exact_existing_geometry_without_nn() -> Result<()> {
    run_query_only(true)
}

#[test]
#[ignore = "actual ALL64 original DEV queries only; preexisting regenerated graph; no indexing; reviewed1800s/8GiB"]
fn regenerated_dev64_queries_use_existing_graph_without_indexing_or_corpus_changes() -> Result<()> {
    run_query_only(false)
}

#[test]
fn query_only_gate_refuses_reorder_missing_refused_or_foreign_dev_tasks() -> Result<()> {
    let tasks: Vec<_> = (0..64)
        .map(|i| json!({"id":format!("hotpot:q{i}"),"split":"development"}))
        .collect();
    let corpus = json!({"tasks":tasks});
    let original = json!({"records":tasks});
    let mask = json!({"quality_question_ids":(0..20).map(|i|format!("q{i}")).collect::<Vec<_>>(),
        "refused_question_ids":(20..64).map(|i|format!("q{i}")).collect::<Vec<_>>()});
    require_same_query_population(&corpus, &mask, &original)?;
    let mut bad = corpus.clone();
    bad["tasks"].as_array_mut().unwrap().swap(0, 1);
    assert!(require_same_query_population(&bad, &mask, &original).is_err());
    let mut bad = corpus.clone();
    bad["tasks"].as_array_mut().unwrap().pop();
    assert!(require_same_query_population(&bad, &mask, &original).is_err());
    let mut bad = corpus.clone();
    bad["tasks"][0]["split"] = "final".into();
    assert!(require_same_query_population(&bad, &mask, &original).is_err());
    Ok(())
}

#[test]
fn query_only_validates_every_native_request_shape_before_any_nn() -> Result<()> {
    let mut corpus = json!({"tasks":[{"question":"valid original question"},{"question":"another original question"}]});
    validate_all_query_shapes(&corpus)?;
    corpus["tasks"][1]["question"] = "".into();
    assert!(validate_all_query_shapes(&corpus).is_err());
    corpus["tasks"][1]["question"] = Value::Null;
    assert!(validate_all_query_shapes(&corpus).is_err());
    corpus["tasks"][1]["question"] = "x".repeat(4097).into();
    assert!(validate_all_query_shapes(&corpus).is_err());
    Ok(())
}
