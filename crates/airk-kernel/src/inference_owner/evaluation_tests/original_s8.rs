// SPDX-License-Identifier: Apache-2.0
//! Exact retained original S8 numeric qualification, separate from six-family/DEV/FINAL.
use super::*;
const FAMILY: &str = "original-shipped-s8-ort130-evaluation";
const RESULT: &str = "original-shipped-s8-ort130-eval";
const GRAPH: &str = "4278337fd0ff3c68bfb6291042cad8ab363e1d9fbc43dcb499fe91c871902474";
const MANIFEST: &str = "333b6babaaae8bda229e99c533a9ad57524704566eadb902fe52f37f38c78ce5";
const ORACLE: &str = "97ec355d47ca8b8da028fa512fa3a7127cfd66748b1a7bfe80c8c1e97725552d";
const DECISION: &str = "cef9af82caf81a6bd755161ffb8a00943d954cf7110eeb822ba775789a4433c2";
fn selected(rows: &[EncoderInput]) -> Result<&EncoderInput> {
    ensure!(
        rows.len() == 1,
        "exactly one retained original-S8 family required"
    );
    let row = &rows[0];
    ensure!(
        row.family == FAMILY
            && row.oracle_result == RESULT
            && row.model_json_sha256 == MANIFEST.parse()?
            && row.oracle_sha256 == ORACLE.parse()?,
        "foreign or unpinned encoder family refused before model admission"
    );
    Ok(row)
}
#[test]
fn original_s8_selector_refuses_foreign_duplicate_and_unpinned_inputs_without_model_load()
-> Result<()> {
    fn row(family: &str) -> Result<EncoderInput> {
        Ok(EncoderInput {
            family: family.into(),
            directory: PathBuf::from("unused-model"),
            model_json_sha256: MANIFEST.parse()?,
            oracle: PathBuf::from("unused-oracle"),
            oracle_sha256: ORACLE.parse()?,
            oracle_result: RESULT.into(),
        })
    }
    assert!(selected(&[]).is_err());
    assert!(selected(&[row(FAMILY)?, row(FAMILY)?]).is_err());
    for foreign in [
        "original-shipped-u8-ort130-evaluation",
        "trained-v2-s8",
        "native-parent-bag-r1-s8",
    ] {
        assert!(selected(&[row(foreign)?]).is_err());
    }
    let mut wrong = row(FAMILY)?;
    wrong.oracle_result = "mixed-s8".into();
    assert!(selected(&[wrong]).is_err());
    let mut wrong = row(FAMILY)?;
    wrong.model_json_sha256 = Sha256Digest::from_bytes([0; 32]);
    assert!(selected(&[wrong]).is_err());
    assert!(selected(&[row(FAMILY)?]).is_ok());
    Ok(())
}
#[test]
#[ignore = "Root-reviewed original S8 alone, exact12 independent padded oracle cases and actual native owned resource window required"]
fn original_shipped_s8_matches_twelve_exact_native_oracles_and_serial_b1() -> Result<()> {
    native_parent_bag::private_base(std::env::var_os("AIRK_PARENT_BAG_PRIVATE_BASE").as_deref())?;
    let input_path = PathBuf::from(
        std::env::var_os("AIRK_ORIGINAL_S8_NATIVE_INPUT")
            .context("missing one-family original S8 descriptor")?,
    );
    let input_pin: Sha256Digest = std::env::var("AIRK_ORIGINAL_S8_NATIVE_INPUT_SHA256")?.parse()?;
    let input_bytes = sealed(&input_path, input_pin)?;
    let rows: Vec<EncoderInput> = serde_json::from_slice(&input_bytes)?;
    let input = selected(&rows)?;
    let decision = PathBuf::from(
        std::env::var_os("AIRK_ORIGINAL_S8_RETENTION_DECISION")
            .context("missing Root retention decision")?,
    );
    sealed(&decision, DECISION.parse()?)?;
    let manifest_bytes = sealed(&input.directory.join("model.json"), input.model_json_sha256)?;
    let manifest: airk_retrieval::model::OnnxManifest = serde_json::from_slice(&manifest_bytes)?;
    ensure!(
        manifest.model_sha256 == GRAPH
            && manifest.tokenizer_sha256
                == "be50c3628f2bf5bb5e3a7f17b1f74611b2561a3a27eeab05e5aa30f411572037"
            && manifest.license_sha256
                == "cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30",
        "exact original S8 assets required"
    );
    for (name, pin) in [
        ("model.onnx", manifest.model_sha256.as_str()),
        ("tokenizer.json", manifest.tokenizer_sha256.as_str()),
        ("LICENSE", manifest.license_sha256.as_str()),
    ] {
        sealed(&input.directory.join(name), pin.parse()?)?;
    }
    ensure!(
        std::fs::metadata(input.directory.join("model.onnx"))?.len() == 23_026_053,
        "exact original S8 graph length"
    );
    let oracle_bytes = sealed(&input.oracle, input.oracle_sha256)?;
    let oracle: Value = serde_json::from_slice(&oracle_bytes)?;
    let cases = oracle["cases"]
        .as_array()
        .context("independent12 padded oracle cases")?;
    let expected_names = BTreeSet::from([
        "fixed-0",
        "fixed-1",
        "fixed-2",
        "fixed-3",
        "fixed-4",
        "fixed-5",
        "fixed-6",
        "fixed-7",
        "mixed-long",
        "mixed-repeat-short",
        "independent-bucket-64",
        "independent-bucket-128",
    ]);
    ensure!(
        cases.len() == 12
            && cases
                .iter()
                .filter_map(|x| x["name"].as_str())
                .collect::<BTreeSet<_>>()
                == expected_names
            && cases
                .iter()
                .filter_map(|x| x["bucket"].as_u64())
                .collect::<BTreeSet<_>>()
                == BTreeSet::from([32, 64, 128, 256]),
        "exact original12 names/buckets without subset/refill required"
    );
    let output = PathBuf::from(
        std::env::var_os("AIRK_ORIGINAL_S8_NATIVE_OUTPUT")
            .context("fresh numeric receipt required")?,
    );
    ensure!(!output.try_exists()?, "numeric output must be fresh");
    let control = native_parent_bag::private_control()?;
    let lease = Arc::new(airk_store::service::ServiceLease::acquire(control.path())?);
    let owner = NativeInferenceOwner::admit(lease)?;
    let mut receipt = json!({"schema":"airk.native-original-s8-oracles/1","complete":false,"evaluation":true,"release_qualified":false,"family":FAMILY,"decision_sha256":DECISION,"input_sha256":input_pin,"model_json_sha256":input.model_json_sha256,"graph_sha256":GRAPH,"oracle_sha256":input.oracle_sha256,"runtime":airk_retrieval::model::ONNX_RUNTIME,"binary_sha256":format!("{:x}",Sha256::digest(std::fs::read(std::env::current_exe()?)?)),"cases":[],"trained_candidate_FP32_floor_applicable":false,"scope":"Exact original S8 SAMEgraph numeric and serialB1 proof only. Original FP32 deltas reported honestly; no trained-candidate floor, task quality, platform/resource or release claim."});
    let result=owner.run_encoder_evaluation(input.model_json_sha256,||->Result<()>{
        let started=Instant::now();let model=EmbeddingModel::open(&input.directory)?;receipt["load_us"]=json!(started.elapsed().as_micros());let descriptor=model.cache_descriptor()?;receipt["pipeline_digest"]=json!(descriptor.identity()?);receipt["pipeline"]=serde_json::to_value(descriptor)?;
        for case in cases {
            let text=case["text"].as_str().context("original caller text")?;let tensors=model.evaluation_inputs(text)?;ensure!(tensors==case["inputs"],"native IDs/masks/types differ from exact independent padded oracle");
            let expected:Vec<f32>=serde_json::from_value(case["results"][RESULT]["vector"].clone())?;let reference:Vec<f32>=serde_json::from_value(case["reference_vector"].clone())?;let started=Instant::now();let actual=model.embed_text(text)?.context("original S8 vector missing")?;let elapsed=started.elapsed().as_micros();let (_,same_l2,error)=distances(&actual,&expected)?;let (cosine,l2,fp32_error)=distances(&actual,&reference)?;
            receipt["cases"].as_array_mut().context("partial cases")?.push(json!({"name":case["name"],"bucket":case["bucket"],"inputs":tensors,"native_vector":actual,"same_graph_expected_vector":expected,"original_FP32_reference":reference,"max_component_error":error,"same_graph_l2":same_l2,"original_FP32_cosine":cosine,"original_FP32_l2":l2,"original_FP32_max_component_error":fp32_error,"inference_us":elapsed}));std::fs::write(&output,serde_json::to_vec_pretty(&receipt)?)?;ensure!(error<=1e-5,"fixed SAMEgraph1e-5 component tolerance failed");
        }
        let texts=cases.iter().map(|x|x["text"].as_str().context("original exact text")).collect::<Result<Vec<_>>>()?;let mut stats=EmbeddingCacheStats::default();let batch=model.embed_batch(&texts,&mut EmbeddingCacheContext{cache:None,bypass:true,stats:&mut stats,max_workers:8,working_memory_bytes:1<<30})?;
        ensure!(batch.len()==12,"all12 serialB1 outputs required");for (index,vector) in batch.iter().enumerate(){ensure!(serde_json::to_value(vector.as_ref().context("batch vector missing")?)?==receipt["cases"][index]["native_vector"],"mixed-length caller order or serialB1 vector changed");}ensure!(stats.workers==1,"actual B1 encoder must retain serial per-text forwards");receipt["mixed_length_execution"]=serde_json::to_value(stats)?;
        sealed(&input.directory.join("model.json"),input.model_json_sha256)?;sealed(&input.oracle,input.oracle_sha256)?;receipt["complete"]=json!(true);Ok(())
    })?;
    if let Err(error) = &result {
        receipt["error"] = json!(error.to_string());
    }
    std::fs::write(output, serde_json::to_vec_pretty(&receipt)?)?;
    result
}
