// SPDX-License-Identifier: Apache-2.0
//! Real helper/process retention migration. Tiny word vectors are not ONNX qualification.
use super::*;
use airk_core::{
    graph_storage::GraphRecordReader,
    retention::RetainedProfile,
    usage::{IndexMetric, IndexSummary, OperationSummary},
};

// Keep positive transport flags and exact scoped members before unwrapping.
fn retained_body(reply: &Value, index: IndexId, label: &str) -> Result<Value> {
    airk_core::index_catalog::encoded_size(reply, 3 * airk_core::MAX_RESPONSE_BYTES + 4096)?;
    ensure!(
        reply.is_object()
            && reply
                .get("isError")
                .is_none_or(|flag| flag == &json!(false)),
        "{label} outer error flag; complete bounded reply: {reply}"
    );
    let value = body(reply)?;
    let members = value["members"]
        .as_array()
        .with_context(|| format!("{label} members absent: {reply}"))?;
    ensure!(
        members.len() == 1
            && members[0]["index_id"] == json!(index)
            && members[0]["outcome"] == "complete"
            && members[0]["is_error"] == false,
        "{label} exact positive member changed: {reply}"
    );
    Ok(value)
}
fn retained_mcp_body(wire: &Value, index: IndexId, label: &str) -> Result<Value> {
    airk_core::index_catalog::encoded_size(wire, 3 * airk_core::MAX_RESPONSE_BYTES + 8192)?;
    ensure!(wire.get("error").is_none(), "{label} RPC error: {wire}");
    let reply = wire
        .get("result")
        .filter(|value| value.is_object())
        .with_context(|| format!("{label} RPC result absent: {wire}"))?;
    retained_body(reply, index, label)
}

fn write_retention_observations(
    name: &str,
    observations: &[serde_json::Value],
    limits: &str,
) -> Result<()> {
    let Some(directory) = std::env::var_os("AIRK_P11B_RESOURCE_OUTPUT_DIR") else {
        return Ok(());
    };
    let directory = std::path::PathBuf::from(directory);
    anyhow::ensure!(
        directory.is_absolute()
            && fs::symlink_metadata(&directory)?.is_dir()
            && !fs::symlink_metadata(&directory)?.file_type().is_symlink(),
        "explicit existing owned report directory required"
    );
    let bytes = serde_json::to_vec_pretty(
        &serde_json::json!({"schema":"airk.p11b.retention-observations/1","observations":observations,"limits":limits}),
    )?;
    anyhow::ensure!(
        bytes.len() <= 1 << 20,
        "bounded retention observations exceeded1MiB"
    );
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(directory.join(name))?;
    file.write_all(&bytes)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}

fn record_helper_retention(
    observations: &mut Vec<Value>,
    phase: &str,
    started: Instant,
    f: &Fixture,
    operation: &OperationSummary,
    snapshot: &Value,
) -> Result<()> {
    if std::env::var_os("AIRK_P11B_RESOURCE_OUTPUT_DIR").is_some() {
        let wall_us = started.elapsed().as_micros();
        let quota =
            airk_store::quota::Quota::open_service(Path::new(&f.registrations[2].data_root))?
                .status()?;
        let records = snapshot["vectors"]
            .as_array()
            .context("positive graph record inventory missing")?;
        let bytes = records.iter().try_fold(0usize, |sum, row| {
            sum.checked_add(
                row["vector_and_adjacency_bytes"]
                    .as_array()
                    .context("graph record bytes missing")?
                    .len(),
            )
            .context("graph byte count overflow")
        })?;
        observations.push(json!({"phase":phase,"wall_us":wall_us,"quota":quota,"native_index_operation_cumulative":operation,"model_and_graph":snapshot,"retained_vector_record_bytes":bytes,"graph_records":records.len()}));
    }
    Ok(())
}

async fn configure(f: &Fixture, name: &str, draft: Value) -> Result<()> {
    let file = f.owner.path().join(format!("retention-{name}.json"));
    fs::write(&file, serde_json::to_vec(&draft)?)?;
    let result = f
        .native_cli(
            2,
            vec![
                "configure".into(),
                "--file".into(),
                file.to_str().context("UTF8 draft")?.into(),
            ],
        )
        .await?;
    let response: ControlResponse = serde_json::from_value(result)?;
    let ControlResponse::Job { job } = response else {
        anyhow::bail!(
            "native retention Configure did not acknowledge exact job: {}",
            serde_json::to_string(&response)?
        );
    };
    let actual = wait_fixture_job(f, job.id).await?;
    ensure!(
        matches!(
            actual.outcome,
            Some(airk_core::service::JobOutcome::Succeeded { .. })
        ),
        "native retention Configure failed: {actual:?}"
    );
    Ok(())
}

async fn counters(f: &Fixture) -> Result<OperationSummary> {
    let mut cursor = None;
    let mut assembled = Value::Null;
    for _ in 0..8 {
        let response = airk_cli::service_runtime::request(
            &f.control,
            ControlMethod::NativeRead {
                project: f.registrations[2].binding,
                usage_client: airk_core::usage::ClientFamily::Unknown,
                request: airk_core::native_reads::NativeRead::UsageReport {
                    days: 1,
                    cursor,
                    max_bytes: 20 << 10,
                    format: airk_core::native_reads::ReportFormat::Json,
                    price: None,
                },
            },
        )
        .await?;
        let ControlResponse::NativeRead { result } = response else {
            anyhow::bail!(
                "native index counters refused: {}",
                serde_json::to_string(&response)?
            );
        };
        ensure!(
            result.get("error").is_none(),
            "native index usage error: {result}"
        );
        let page: airk_kernel::usage_page::Page = serde_json::from_value(result)?;
        for record in page.items {
            airk_kernel::usage_page::assemble(&mut assembled, record)?;
        }
        cursor = page.cursor;
        if cursor.is_none() {
            return Ok(serde_json::from_value(
                assembled
                    .pointer("/value/report/operations/index")
                    .context("native index work counters absent")?
                    .clone(),
            )?);
        }
    }
    anyhow::bail!("native index counter report exceeded explicit eight-page budget")
}
fn metric(summary: &IndexSummary, key: IndexMetric) -> Result<u128> {
    let total = summary
        .metrics
        .get(&key)
        .context("required model/vector counter absent, not zero")?;
    ensure!(total.samples > 0, "model/vector counter has no samples");
    Ok(total.value)
}
/// Capture only bounded authenticated native metadata, dropping every pin before migration.
fn graph(f: &Fixture) -> Result<Value> {
    let registration = &f.registrations[2];
    let selected = airk_store::service::collection::Collection::existing(
        Path::new(&registration.data_root),
        &registration.root,
        &registration.collection,
    )?
    .select()?;
    let scope = selected
        .store
        .current_scope(&registration.root, &registration.collection)?
        .context("actual current absent")?;
    let read = selected.store.current_read(&scope)?;
    let config = read
        .collection_state()?
        .context("native applied config absent")?
        .configuration;
    let graph = &read.head().graph_head;
    ensure!(
        graph.allocated_count <= 128 && graph.live_nodes > 0,
        "fixture vector bound exceeded or not positive"
    );
    let mut vectors = Vec::new();
    for id in 1..u32::try_from(graph.allocated_count)? {
        let record = read.read(id)?;
        if record.live {
            let binding = read.vector_for_graph_id(id)?;
            vectors.push(json!({"embedding_key":binding.embedding_key,"graph_id":binding.graph_id,"vector_and_adjacency_bytes":record.bytes}));
        }
    }
    Ok(
        json!({"model":config.model.context("enabled fixture lacks actual model objects")?,"vectors":vectors,"selection":config.vector_selection,"target":config.retained_profile}),
    )
}

#[tokio::test]
async fn actual_helper_retention_migration_rebinds_evidence_without_new_embedding_inputs()
-> Result<()> {
    let mut observations = Vec::new();
    let mut f = Fixture::with_helper()?;
    f.session.project_ids = vec![f.projects[2]];
    f.session.index_ids = Some(vec![f.indexes[2]]);
    let registration = &f.registrations[2];
    let setup = airk_kernel::Kernel::open_service_collection(
        &registration.root,
        Some(registration.data_root.clone().into()),
        &registration.collection,
    )?;
    setup.configure_vectors(true)?;
    setup.configure_usage(airk_core::usage::RecordingMode::AggregatesOnly)?;
    drop(setup);
    let sources = ["api", "impl", "private", "api-other"]
        .map(|directory| {
            let path = Path::new(&registration.root)
                .join(directory)
                .join("policy.md");
            Ok::<_, anyhow::Error>((path.clone(), fs::read(path)?))
        })
        .into_iter()
        .collect::<Result<Vec<_>>>()?;
    let model = f.owner.path().join("retention-model");
    fs::create_dir(&model)?;
    fs::write(
        model.join("vocabulary.json"),
        br#"{"boundaryneedle":[1.0,0.0],"original":[0.0,1.0]}"#,
    )?;
    fs::write(
        model.join("model.json"),
        serde_json::to_vec(
            &json!({"schema":1,"name":"original-retention-helper-fixture","license":"Apache-2.0","dimensions":2,"sha256":airk_cli::runtime_pair::digest_file(&model.join("vocabulary.json"))?}),
        )?,
    )?;
    let _service = f.start().await?;
    let phase_started = Instant::now();
    fixture_pause(&f, 2, false).await?;
    fixture_install_model(&f, 2, &model).await?;
    fixture_job(&f, 2, airk_core::service::JobKind::Index).await?;
    fixture_job(&f, 2, airk_core::service::JobKind::Vectors).await?;
    split::pause_quiescent_original(&f, 2).await?;
    let helper =
        airk_cli::service_runtime::request(&f.control, ControlMethod::EmbeddingStatus).await?;
    ensure!(
        matches!(
            helper,
            ControlResponse::EmbeddingStatus {
                available: true,
                process_id: Some(_),
                worker: Some(_),
                loads: 1..,
                ..
            }
        ),
        "actual helper was not observed: {}",
        serde_json::to_string(&helper)?
    );
    let original = graph(&f)?;
    let mut before = counters(&f).await?;
    record_helper_retention(
        &mut observations,
        "baseline_model_build",
        phase_started,
        &f,
        &before,
        &original,
    )?;
    ensure!(
        metric(&before.index_work, IndexMetric::EmbeddingInputs)? > 0,
        "warm fixture did not record actual model input work"
    );
    for (name, target) in [
        ("lean", RetainedProfile::Lean),
        ("rich", RetainedProfile::Rich),
    ] {
        let phase_started = Instant::now();
        fixture_pause(&f, 2, false).await?;
        configure(
            &f,
            name,
            json!({"operation":"retained_profile","target":target}),
        )
        .await?;
        // Existing real jobs, not synthetic completed results, finish source and retained semantic work.
        fixture_job(&f, 2, airk_core::service::JobKind::Index).await?;
        fixture_job(&f, 2, airk_core::service::JobKind::Vectors).await?;
        split::pause_quiescent_original(&f, 2).await?;
        let after = graph(&f)?;
        ensure!(
            after["model"] == original["model"] && after["selection"] == original["selection"],
            "retention changed configured model/pipeline or vector eligibility"
        );
        ensure!(
            after["vectors"] == original["vectors"],
            "retention changed exact embedding keys/physical node IDs/vector data"
        );
        ensure!(
            after["target"] == json!(target),
            "native migration was not applied"
        );
        let usage = counters(&f).await?;
        record_helper_retention(&mut observations, name, phase_started, &f, &usage, &after)?;
        ensure!(
            metric(&usage.index_work, IndexMetric::EmbeddingInputs)?
                == metric(&before.index_work, IndexMetric::EmbeddingInputs)?
                && metric(&usage.index_work, IndexMetric::EmbeddingCacheMisses)?
                    == metric(&before.index_work, IndexMetric::EmbeddingCacheMisses)?,
            "retention-only migration sent new inputs to model or lost exact cache identity"
        );
        ensure!(
            metric(&usage.index_work, IndexMetric::VectorReusedInputs)?
                > metric(&before.index_work, IndexMetric::VectorReusedInputs)?,
            "migration did not demonstrate actual graph-input reuse"
        );
        before = usage;
        let backend =
            airk_cli::combined_frontend::resolve(None, Some(&f.control), &f.session).await?;
        let args = json!({"query":"boundaryneedle","mode":"vector","limit":8,"max_bytes":8192,"verification":"off","response_mode":"full"});
        let native = native_read(backend.as_ref(), "airk_search", &args).await?;
        let mut mcp = McpSession::start(&f)?;
        let actual = mcp.call(10, "airk_search", args)?;
        assert_parity(
            "retention actual helper MCP",
            &retained_body(&native, f.indexes[2], "retention native vector")?,
            &retained_mcp_body(&actual, f.indexes[2], "retention MCP vector")?,
        )?;
        // The same real migrated provider must expose the new per-file report
        // through native, actual CLI call and actual stdio MCP, with full data.
        let mut reported: Option<Value> = None;
        for (call_id, reply_detail) in [(11, "lean"), (12, "rich")] {
            let info_args = json!({"path":"api/policy.md","reply_detail":reply_detail,"include_profile_families":true,"project_ids":[f.projects[2]],"index_ids":[f.indexes[2]],"member_availability":"require","verification":"off","response_mode":"full"});
            let info = native_read(backend.as_ref(), "airk_file_info", &info_args).await?;
            let cli = f.cli("airk_file_info", &info_args).await?;
            let wire = mcp.call(call_id, "file_info", info_args)?;
            let complete = retained_body(&info, f.indexes[2], "retention native FileInfo")?;
            assert_parity(
                "selected FileInfo actual CLI",
                &complete,
                &retained_body(&cli, f.indexes[2], "retention CLI FileInfo")?,
            )?;
            assert_parity(
                "selected FileInfo actual MCP",
                &complete,
                &retained_mcp_body(&wire, f.indexes[2], "retention MCP FileInfo")?,
            )?;
            let members = complete["members"]
                .as_array()
                .context("selected FileInfo members absent")?;
            ensure!(
                members.len() == 1
                    && members[0]["index_id"] == json!(f.indexes[2])
                    && members[0]["outcome"] == "complete"
                    && members[0]["is_error"] == false,
                "selected FileInfo returned wrong member"
            );
            let file = member_body(&complete, &members[0])?;
            // ConditionalRead flattens the serialized FileInfo payload. Typed
            // Rust .content remains an Option<T>; this is the actual JSON wire.
            let report = file.get("profile_families").with_context(|| {
                format!("flat selected FileInfo report absent; complete member: {file}")
            })?;
            ensure!(
                report["applied_file"]["applied_retention"]["profile"] == json!(target)
                    && report["applied_file"].get("legacy_detail").is_none(),
                "new retention was omitted or reinterpreted as legacy: {report}"
            );
            ensure!(
                report["families"]
                    .as_array()
                    .is_some_and(|rows| rows.len() == 18)
                    && report["live_source_checked"] == false,
                "selected report lost bounded family/saved-source basis"
            );
            ensure!(
                report["configured_collection_policy"]["revision"]["retained_profile"]
                    == json!(target)
                    && report["published_collection_policy"]["revision"]["retained_profile"]
                        == json!(target),
                "collection and applied selected profiles differ: {report}"
            );
            if let Some(previous) = &reported {
                assert_eq!(
                    report, previous,
                    "reply projection changed effective retained profile metadata"
                );
            } else {
                reported = Some(report.clone());
            }
        }
        drop(mcp);
        backend.close();
        backend.shutdown().await?;
    }
    // A repeated selected profile completes Configure but does not schedule source/model work.
    let phase_started = Instant::now();
    fixture_pause(&f, 2, false).await?;
    configure(
        &f,
        "rich-noop",
        json!({"operation":"retained_profile","target":"rich"}),
    )
    .await?;
    split::pause_quiescent_original(&f, 2).await?;
    let noop = counters(&f).await?;
    if std::env::var_os("AIRK_P11B_RESOURCE_OUTPUT_DIR").is_some() {
        record_helper_retention(
            &mut observations,
            "rich_noop",
            phase_started,
            &f,
            &noop,
            &graph(&f)?,
        )?;
    }
    ensure!(
        metric(&noop.index_work, IndexMetric::EmbeddingInputs)?
            == metric(&before.index_work, IndexMetric::EmbeddingInputs)?
            && metric(&noop.index_work, IndexMetric::VectorReusedInputs)?
                == metric(&before.index_work, IndexMetric::VectorReusedInputs)?,
        "no-op profile selection scheduled native index/model work"
    );
    for (path, bytes) in sources {
        ensure!(
            fs::read(path)? == bytes,
            "retention migration modified source bytes"
        );
    }
    write_retention_observations(
        "helper-retention.json",
        &observations,
        "Native operation counters are cumulative; subtract only observed adjacent phases. Existing native resource counter semantics exclude overlapped intervals and may omit CPU counters; absent is unknown, not zero. Wall time covers existing Configure/Index/Vectors/idle work, excluding quota observation. Logical quota includes retained history, not physical disk or service peak RSS. This actual tiny wordvector helper is not ONNX/default-model qualification; no extra model runs were added.",
    )?;
    Ok(())
}
