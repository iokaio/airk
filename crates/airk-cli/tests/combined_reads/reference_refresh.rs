// SPDX-License-Identifier: Apache-2.0
//! Actual native refresh, receipts and transports. Tiny vectors are not ONNX qualification.
use super::*;
use airk_core::{
    catalog_refresh::{RefreshMember, RefreshMode, RefreshSelection, RefreshSource},
    retention::RetainedProfile,
    service::{JobKind, JobOutcome, RequestKey},
    service_configuration::{ConfigurationRequest, ProjectConfiguration},
};
use sha2::Digest;
fn digest(value: &Value) -> Result<Sha256Digest> {
    Ok(Sha256Digest::from_bytes(
        sha2::Sha256::digest(serde_json::to_vec(value)?).into(),
    ))
}
fn positive(reply: &Value, index: IndexId, label: &str) -> Result<Value> {
    airk_core::index_catalog::encoded_size(reply, 3 * airk_core::MAX_RESPONSE_BYTES + 8192)?;
    ensure!(
        reply.is_object()
            && reply
                .get("isError")
                .is_none_or(|flag| flag == &json!(false)),
        "{label} outer error: {reply}"
    );
    let value = body(reply)?;
    let members = value["members"]
        .as_array()
        .context("member census absent")?;
    ensure!(
        members.len() == 1
            && members[0]["index_id"] == json!(index)
            && members[0]["outcome"] == "complete"
            && members[0]["is_error"] == false,
        "{label} wrong/failed selected member: {reply}"
    );
    Ok(value)
}
async fn parity(f: &Fixture, tool: &str, args: Value) -> Result<Value> {
    let backend = airk_cli::combined_frontend::resolve(None, Some(&f.control), &f.session).await?;
    // Poll native work independently of this long journey's debug assertion
    // frame, using the standard test stack and the same canonical backend.
    let native_backend = backend.clone();
    let native_tool = tool.to_owned();
    let native_args = args.clone();
    let native = tokio::spawn(async move {
        native_read(native_backend.as_ref(), &native_tool, &native_args).await
    })
    .await??;
    let cli = f.cli(tool, &args).await?;
    let mut mcp = McpSession::start(f)?;
    let wire = mcp.call(4, tool, args)?;
    airk_core::index_catalog::encoded_size(&wire, 3 * airk_core::MAX_RESPONSE_BYTES + 8192)?;
    ensure!(
        wire.get("error").is_none() && wire.get("result").is_some_and(Value::is_object),
        "actual MCP failure: {wire}"
    );
    let native = positive(&native, f.indexes[2], tool)?;
    assert_parity(
        "refresh complete CLI",
        &native,
        &positive(&cli, f.indexes[2], tool)?,
    )?;
    assert_parity(
        "refresh complete MCP",
        &native,
        &positive(&wire["result"], f.indexes[2], tool)?,
    )?;
    backend.shutdown().await?;
    Ok(native)
}
async fn file(f: &Fixture, path: &str) -> Result<Value> {
    let value=parity(f,"airk_file_info",json!({"path":path,"include_profile_families":true,"reply_detail":"rich","project_ids":[f.projects[2]],"index_ids":[f.indexes[2]],"member_availability":"require","verification":"off","response_mode":"full"})).await?;
    member_body(&value, &value["members"][0])
}
fn jobs(f: &Fixture) -> Result<u64> {
    let sql = Connection::open_with_flags(
        f.control.join("service-1.4.sqlite3"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    sql.busy_timeout(Duration::from_secs(2))?;
    let count: i64 = sql.query_row(
        "SELECT count(*) FROM service_jobs WHERE project=?1",
        [f.registrations[2].binding.to_string()],
        |row| row.get(0),
    )?;
    Ok(count.try_into()?)
}
async fn receipt_observation(f: &Fixture, caller: Incarnation, key: &RequestKey) -> Result<Value> {
    let method = ControlMethod::ReferenceRefreshReceipt {
        caller,
        key: key.clone(),
    };
    let native =
        serde_json::to_value(airk_cli::service_runtime::request(&f.control, method).await?)?;
    ensure!(
        native["status"] == "reference_refresh_receipt",
        "receipt lookup failed: {native}"
    );
    Ok(native)
}
async fn receipt(f: &Fixture, caller: Incarnation, key: &RequestKey) -> Result<Value> {
    let native = receipt_observation(f, caller, key).await?;
    let cli = super::permanent_trim::policy_rpc_cli(
        f,
        ControlMethod::ReferenceRefreshReceipt {
            caller,
            key: key.clone(),
        },
    )
    .await?;
    ensure!(
        native == cli,
        "CLI omitted complete receipt fields: native={native};CLI={cli}"
    );
    Ok(native)
}
async fn request(
    f: &Fixture,
    nonce: u8,
    mode: RefreshMode,
    query: &Value,
) -> Result<ConfigurationRequest> {
    // Explicit fixture Inspect work, never an automatic host refresh side effect.
    fixture_pause(f, 2, false).await?;
    fixture_job(f, 2, JobKind::Inspect).await?;
    super::split::pause_quiescent_original(f, 2).await?;
    let response = airk_cli::service_runtime::request(
        &f.control,
        ControlMethod::Inspection {
            project: f.registrations[2].binding,
        },
    )
    .await?;
    let ControlResponse::Inspection {
        observation: Some(observation),
        ..
    } = response
    else {
        anyhow::bail!("inspection absent: {}", serde_json::to_string(&response)?);
    };
    let snapshot = observation.snapshot;
    snapshot.validate()?;
    let response = airk_cli::service_runtime::request(
        &f.control,
        ControlMethod::CatalogSplitSourceState {
            index_id: f.indexes[2],
            expected_catalog_revision: super::permanent_trim::policy_revision(f)?,
        },
    )
    .await?;
    let ControlResponse::CatalogSplitSourceState {
        state,
        capabilities,
    } = response
    else {
        anyhow::bail!(
            "source identities absent: {}",
            serde_json::to_string(&response)?
        );
    };
    ensure!(
        capabilities.reference_refresh_contract == Some(1),
        "actual native host does not advertise reference-refresh contract1"
    );
    let root = state
        .policy
        .roots
        .iter()
        .find(|root| root.owner)
        .context("native owner source absent")?;
    let original = parity(f, "airk_search", query.clone()).await?;
    ensure!(
        original["ranking"]
            .as_array()
            .is_some_and(|rows| !rows.is_empty()),
        "actual original query has no references: {original}"
    );
    let current = file(f, "api/policy.md").await?;
    let publication = serde_json::from_value(current["publication"]["token"].clone())?;
    ensure!(
        snapshot.publication == Some(publication),
        "inspection changed before review"
    );
    let members = vec![RefreshMember {
        index_id: f.indexes[2],
        original_publication: publication,
        sources: vec![RefreshSource {
            source_root_id: root.id,
            source_relative_path: "api/policy.md".into(),
            canonical_root: root.canonical_path.clone(),
            indexed_hash: current["content_hash"]
                .as_str()
                .context("source hash absent")?
                .parse()?,
        }],
    }];
    let selection = RefreshSelection {
        schema: 1,
        caller: airk_cli::local_ipc::caller_identity()?,
        key: super::permanent_trim::policy_key(f, nonce).await?,
        original_result_digest: digest(&original)?,
        original_evidence_selection_digest: digest(&serde_json::to_value(&members)?)?,
        original_query_digest: digest(query)?,
        selection: f.session.clone(),
        mode,
        members,
    };
    let request = ConfigurationRequest {
        prepare: false,
        incarnation: snapshot.incarnation,
        settings: snapshot.settings_digest.context("settings absent")?,
        configuration: ProjectConfiguration::RefreshReferences {
            selection: Box::new(selection),
        },
    };
    request.validate()?;
    Ok(request)
}
fn selected(request: &ConfigurationRequest) -> &RefreshSelection {
    let ProjectConfiguration::RefreshReferences { selection } = &request.configuration else {
        unreachable!("fixture constructs refresh")
    };
    selection
}
async fn submit_once(f: &Fixture, request: &ConfigurationRequest) -> Result<ControlResponse> {
    let s = selected(request);
    airk_cli::service_runtime::request(
        &f.control,
        ControlMethod::ConfigureProject {
            project: f.registrations[2].binding,
            caller: s.caller,
            key: s.key.clone(),
            request: Box::new(request.clone()),
        },
    )
    .await
}
async fn submit_without_reply(f: &Fixture, request: &ConfigurationRequest) -> Result<()> {
    let s = selected(request);
    let address = airk_cli::local_ipc::endpoint(&f.control, "control")?;
    let mut stream = tokio::time::timeout(
        Duration::from_secs(5),
        airk_cli::local_ipc::connect(&address),
    )
    .await??;
    airk_cli::local_ipc::write_message(
        &mut stream,
        &airk_core::service::ControlRequest {
            protocol: airk_core::service::PROTOCOL_VERSION,
            expected_instance: None,
            method: ControlMethod::ConfigureProject {
                project: f.registrations[2].binding,
                caller: s.caller,
                key: s.key.clone(),
                request: Box::new(request.clone()),
            },
        },
        Duration::from_secs(5),
    )
    .await?;
    // Close the actual native transport without reading the acknowledgment.
    // A write/close error is unknown and never authorizes retransmission.
    drop(stream);
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let observed = receipt_observation(f, s.caller, &s.key).await?;
        if !observed["job"].is_null() {
            return Ok(());
        }
        ensure!(
            Instant::now() < deadline,
            "lost-reply exact receipt remains unknown; no resubmission: {observed}"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}
async fn finish(f: &Fixture, request: &ConfigurationRequest) -> Result<Value> {
    let s = selected(request);
    // Admission/execution may advance between reads. Poll the actual native
    // observation; compare complete transport responses once work is settled.
    let saved = receipt_observation(f, s.caller, &s.key).await?;
    let id = serde_json::from_value(saved["job"]["id"].clone())?;
    let job = wait_fixture_job(f, id).await?;
    ensure!(
        matches!(job.outcome, Some(JobOutcome::Succeeded { .. })),
        "actual refresh failed: {job:?}"
    );
    super::split::pause_quiescent_original(f, 2).await?;
    let saved = receipt(f, s.caller, &s.key).await?;
    ensure!(
        saved["configuration"]["request"] == serde_json::to_value(request)?
            && saved["result"]["selection_digest"] == json!(s.identity()?)
            && saved["result"]["original_query_digest"] == json!(s.original_query_digest),
        "receipt crossed original intent: {saved}"
    );
    ensure!(
        saved["result"]["schema"] == "airk.reference-refresh-result/1"
            && saved["result"]["effective_mode"] == json!(s.mode)
            && saved["result"]["before_publication"] == json!(s.members[0].original_publication)
            && saved["result"]["requery_executed"] == false,
        "incomplete native result: {saved}"
    );
    let after: airk_core::identity::RevisionToken =
        serde_json::from_value(saved["result"]["after_publication"].clone())?;
    ensure!(
        after.incarnation == s.members[0].original_publication.incarnation
            && after.generation > s.members[0].original_publication.generation,
        "no successor publication: {saved}"
    );
    Ok(saved)
}
async fn configure_retention(f: &Fixture, target: RetainedProfile) -> Result<()> {
    let path = f
        .owner
        .path()
        .join(format!("refresh-retention-{target:?}.json"));
    fs::write(
        &path,
        serde_json::to_vec(&json!({"operation":"retained_profile","target":target}))?,
    )?;
    fixture_pause(f, 2, false).await?;
    let response: ControlResponse = serde_json::from_value(
        f.native_cli(
            2,
            vec![
                "configure".into(),
                "--file".into(),
                path.to_str().context("UTF8 draft")?.into(),
            ],
        )
        .await?,
    )?;
    let ControlResponse::Job { job } = response else {
        anyhow::bail!(
            "retention admission failed: {}",
            serde_json::to_string(&response)?
        );
    };
    let job = wait_fixture_job(f, job.id).await?;
    ensure!(
        matches!(job.outcome, Some(JobOutcome::Succeeded { .. })),
        "migration configure failed: {job:?}"
    );
    fixture_job(f, 2, JobKind::Index).await?;
    fixture_job(f, 2, JobKind::Vectors).await?;
    super::split::pause_quiescent_original(f, 2).await
}
async fn stop(f: &Fixture, service: &mut OwnedProcess) -> Result<()> {
    let response = airk_cli::service_runtime::request(&f.control, ControlMethod::Shutdown).await?;
    ensure!(
        matches!(response, ControlResponse::Stopping),
        "shutdown refused: {}",
        serde_json::to_string(&response)?
    );
    let deadline = Instant::now() + Duration::from_secs(20);
    while service.child.try_wait()?.is_none() {
        ensure!(Instant::now() < deadline, "owned service did not stop");
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    Ok(())
}
fn same_publication(left: &Value, right: &Value) -> bool {
    left["publication"]["token"] == right["publication"]["token"]
        && left["content_hash"] == right["content_hash"]
}
async fn assert_paused_pending(
    f: &Fixture,
    request: &ConfigurationRequest,
    before: &Value,
) -> Result<()> {
    let s = selected(request);
    let saved = receipt(f, s.caller, &s.key).await?;
    ensure!(
        saved["job"]["state"] == "pending" && saved["job"]["outcome"].is_null(),
        "paused refresh was claimed: {saved}"
    );
    let id = serde_json::from_value(saved["job"]["id"].clone())?;
    ensure!(
        !ServiceRegistry::open_saved(&f.control)?.job_runs_while_paused(id)?,
        "refresh crossed canonical pause classification"
    );
    let progress = airk_cli::service_runtime::request(
        &f.control,
        ControlMethod::Progress {
            project: f.registrations[2].binding,
        },
    )
    .await?;
    ensure!(
        matches!(
            progress,
            ControlResponse::Progress {
                project_paused: Some(true),
                pending_paused: Some(true),
                ..
            }
        ),
        "actual paused queue observation changed: {}",
        serde_json::to_string(&progress)?
    );
    ensure!(
        same_publication(&file(f, "api/policy.md").await?, before),
        "paused pending refresh changed source publication"
    );
    Ok(())
}
#[tokio::test]
async fn actual_native_refresh_keeps_profile_scope_shared_model_and_exact_receipts_across_restart()
-> Result<()> {
    let mut f = Fixture::with_inference()?;
    f.session.project_ids = vec![f.projects[2]];
    f.session.index_ids = Some(vec![f.indexes[2]]);
    let root = PathBuf::from(&f.registrations[2].root);
    let original_sources = f
        .registrations
        .iter()
        .map(|r| {
            ["api", "impl", "private", "api-other"]
                .into_iter()
                .map(|directory| {
                    let path = Path::new(&r.root).join(directory).join("policy.md");
                    Ok((path.clone(), fs::read(path)?))
                })
                .collect::<Result<Vec<_>>>()
        })
        .collect::<Result<Vec<_>>>()?;
    let setup = airk_kernel::Kernel::open_service_collection(
        &root,
        Some(f.registrations[2].data_root.clone().into()),
        &f.registrations[2].collection,
    )?;
    setup.configure_vectors(true)?;
    drop(setup);
    let model = f.owner.path().join("refresh-tiny-model");
    fs::create_dir(&model)?;
    fs::write(
        model.join("vocabulary.json"),
        br#"{"boundaryneedle":[1.0,0.0],"original":[0.0,1.0],"selected":[0.5,0.5]}"#,
    )?;
    fs::write(
        model.join("model.json"),
        serde_json::to_vec(
            &json!({"schema":1,"name":"original-reference-refresh-process-fixture","license":"Apache-2.0","dimensions":2,"sha256":airk_cli::runtime_pair::digest_file(&model.join("vocabulary.json"))?}),
        )?,
    )?;
    let plan = airk_retrieval::model::inspect_object_plan(&model)?;
    let query = json!({"query":"boundaryneedle","mode":"lexical","limit":8,"max_bytes":8192,"verification":"off","response_mode":"full","project_ids":[f.projects[2]],"index_ids":[f.indexes[2]],"member_availability":"require"});
    let query_digest = digest(&query)?;
    let mut service = f.start().await?;
    let watch = airk_cli::service_runtime::request(
        &f.control,
        ControlMethod::ConfigureWatch {
            project: f.registrations[2].binding,
            enabled: false,
        },
    )
    .await?;
    ensure!(
        matches!(watch,ControlResponse::Watch{observation,..} if !observation.enabled),
        "explicit manual fixture watch setup refused"
    );
    fixture_pause(&f, 2, false).await?;
    fixture_install_model(&f, 2, &model).await?;
    fixture_job(&f, 2, JobKind::Index).await?;
    fixture_job(&f, 2, JobKind::Vectors).await?;
    super::split::pause_quiescent_original(&f, 2).await?;
    let mut expected = original_sources[2]
        .iter()
        .cloned()
        .collect::<std::collections::BTreeMap<_, _>>();
    for (phase, target) in [(0u8, RetainedProfile::Lean), (1u8, RetainedProfile::Rich)] {
        configure_retention(&f, target).await?;
        let declined = request(&f, 180 + phase * 10, RefreshMode::SelectedOnly, &query).await?;
        let before = file(&f, "api/policy.md").await?;
        let before_jobs = jobs(&f)?;
        let refusal = submit_once(&f, &declined).await?;
        ensure!(
            matches!(&refusal,ControlResponse::ReferenceRefreshReviewRequired{review,..} if review.reasons.contains(&airk_core::catalog_refresh::RefreshFallbackReason::UnqualifiedControlProvenance)),
            "unqualified selection silently accepted/widened: {}",
            serde_json::to_string(&refusal)?
        );
        let missing = receipt(&f, selected(&declined).caller, &selected(&declined).key).await?;
        ensure!(
            missing["job"].is_null()
                && missing["configuration"].is_null()
                && missing["result"].is_null()
                && missing["expired"] == false,
            "refusal invented acceptance: {missing}"
        );
        // Decline the disclosed broader pass: send no full-pass mutation.
        ensure!(
            same_publication(&file(&f, "api/policy.md").await?, &before)
                && jobs(&f)? == before_jobs,
            "declined fallback changed publication/jobs"
        );
        let full = request(
            &f,
            181 + phase * 10,
            RefreshMode::FullReconcileReviewed,
            &query,
        )
        .await?;
        let before = file(&f, "api/policy.md").await?;
        let response = submit_once(&f, &full).await?;
        ensure!(
            matches!(response, ControlResponse::Job { .. }),
            "explicit full review not admitted: {}",
            serde_json::to_string(&response)?
        );
        // Configure refresh remains source work: accepting explicit intent does
        // not unpause the Project or authorize a paused source pass.
        assert_paused_pending(&f, &full, &before).await?;
        fixture_pause(&f, 2, false).await?;
        let full_saved = finish(&f, &full).await?;
        ensure!(
            full_saved["result"]["effective_scope"] == "whole_index_explicitly_reviewed"
                && full_saved["result"]["report"]["path_scoped"] == false,
            "full scope disclosure absent: {full_saved}"
        );
        let selected_request =
            request(&f, 182 + phase * 10, RefreshMode::SelectedOnly, &query).await?;
        let untouched = file(&f, "impl/policy.md").await?;
        let stale_request =
            request(&f, 183 + phase * 10, RefreshMode::SelectedOnly, &query).await?;
        let selected_bytes =
            format!("# boundaryneedle selected {target:?}\nOriginal selected source refresh.\n")
                .into_bytes();
        let unselected_bytes = format!(
            "# boundaryneedle unselected {target:?}\nMust remain outside selected refresh.\n"
        )
        .into_bytes();
        fs::write(root.join("api/policy.md"), &selected_bytes)?;
        fs::write(root.join("impl/policy.md"), &unselected_bytes)?;
        expected.insert(root.join("api/policy.md"), selected_bytes);
        expected.insert(root.join("impl/policy.md"), unselected_bytes);
        let before = file(&f, "api/policy.md").await?;
        // Discard the client acknowledgment; reconcile by exact saved key only.
        // The mutation is sent exactly once, including across the restart.
        submit_without_reply(&f, &selected_request).await?;
        assert_paused_pending(&f, &selected_request, &before).await?;
        fixture_pause(&f, 2, false).await?;
        let saved = finish(&f, &selected_request).await?;
        ensure!(
            saved["result"]["effective_scope"] == "selected_sources"
                && saved["result"]["report"]["path_scoped"] == true
                && saved["result"]["original_query_digest"] == json!(query_digest),
            "selected mode/query reference changed: {saved}"
        );
        let after = file(&f, "api/policy.md").await?;
        ensure!(
            after["content_hash"]
                == json!(airk_cli::runtime_pair::digest_file(
                    &root.join("api/policy.md")
                )?)
                && after["profile_families"]["applied_file"]["applied_retention"]["profile"]
                    == json!(target),
            "source/profile continuity lost: {after}"
        );
        // A separate, explicit replay of the exact original query obtains new
        // evidence under the same grant/Project/index. The source result and
        // historical receipt continue to state requery_executed=false.
        ensure!(
            after["profile_families"]["families"]
                .as_array()
                .is_some_and(|rows| rows.len() == 18),
            "actual retained report omitted family rows: {after}"
        );
        let requeried = parity(&f, "airk_search", query.clone()).await?;
        let refreshed_member = member_body(&requeried, &requeried["members"][0])?;
        ensure!(
            digest(&query)? == query_digest
                && refreshed_member["publication"]["token"] == after["publication"]["token"],
            "explicit original-query replay did not select the actual new publication: {requeried}"
        );
        ensure!(
            receipt(
                &f,
                selected(&selected_request).caller,
                &selected(&selected_request).key
            )
            .await?
                == saved,
            "explicit requery mutated historical source-operation receipt"
        );
        let unchanged = file(&f, "impl/policy.md").await?;
        ensure!(
            unchanged["content_hash"] == untouched["content_hash"]
                && unchanged["profile_families"]["applied_file"]["effective_profile_digest"]
                    == untouched["profile_families"]["applied_file"]["effective_profile_digest"],
            "selected refresh changed unselected retained evidence"
        );
        let before_jobs = jobs(&f)?;
        let stale = submit_once(&f, &stale_request).await?;
        ensure!(
            matches!(stale,ControlResponse::Failure{ref code} if code=="invalid_registry_request"),
            "newer publication did not reject stale fresh review: {}",
            serde_json::to_string(&stale)?
        );
        ensure!(
            receipt(
                &f,
                selected(&stale_request).caller,
                &selected(&stale_request).key
            )
            .await?["job"]
                .is_null(),
            "stale review acquired acceptance"
        );
        ensure!(
            same_publication(&file(&f, "api/policy.md").await?, &after) && jobs(&f)? == before_jobs,
            "stale refusal changed publication/jobs"
        );
        let unknown = super::permanent_trim::policy_key(&f, 184 + phase * 10).await?;
        let missing = receipt(&f, selected(&selected_request).caller, &unknown).await?;
        ensure!(
            missing["job"].is_null()
                && missing["configuration"].is_null()
                && missing["result"].is_null()
                && missing["expired"] == false
                && jobs(&f)? == before_jobs,
            "unknown lookup invented success or replayed: {missing}"
        );
        if phase == 0 {
            stop(&f, &mut service).await?;
            drop(service);
            service = f.start().await?;
            let historical = receipt(
                &f,
                selected(&selected_request).caller,
                &selected(&selected_request).key,
            )
            .await?;
            ensure!(
                historical == saved && jobs(&f)? == before_jobs,
                "restart altered receipt or duplicated work"
            );
            ensure!(
                same_publication(&file(&f, "api/policy.md").await?, &after),
                "restart/receipt reran publication"
            );
        }
        // Explicit semantic continuation, then a fresh explicit query. Neither
        // is synthesized by receipt lookup or attributed to the source result.
        fixture_pause(&f, 2, false).await?;
        fixture_job(&f, 2, JobKind::Vectors).await?;
        super::split::pause_quiescent_original(&f, 2).await?;
        parity(&f, "airk_status", json!({"response_mode":"full"})).await?;
        let vectors=parity(&f,"airk_search",json!({"query":"boundaryneedle","mode":"vector","limit":8,"max_bytes":8192,"verification":"off","response_mode":"full","project_ids":[f.projects[2]],"index_ids":[f.indexes[2]],"member_availability":"require"})).await?;
        ensure!(
            vectors["ranking"]
                .as_array()
                .is_some_and(|rows| !rows.is_empty()),
            "actual in-process provider produced no evidence: {vectors}"
        );
        let helper =
            airk_cli::service_runtime::request(&f.control, ControlMethod::EmbeddingStatus).await?;
        ensure!(
            matches!(
                helper,
                ControlResponse::EmbeddingStatus {
                    available: true,
                    process_id: None,
                    provider: Some(airk_core::inference_provider::ProviderStatus {
                        kind: airk_core::inference_provider::ProviderKind::InProcess,
                        ..
                    }),
                    worker: Some(_),
                    loads: 1..,
                    ..
                }
            ),
            "in-process provider was not observed: {}",
            serde_json::to_string(&helper)?
        );
        let watch = airk_cli::service_runtime::request(
            &f.control,
            ControlMethod::Watch {
                project: f.registrations[2].binding,
            },
        )
        .await?;
        ensure!(
            matches!(watch,ControlResponse::Watch{observation,..} if !observation.enabled),
            "refresh changed explicit manual watch preference"
        );
        let selected_store = airk_store::service::collection::Collection::existing(
            Path::new(&f.registrations[2].data_root),
            &f.registrations[2].root,
            &f.registrations[2].collection,
        )?
        .select()?;
        let scope = selected_store
            .store
            .current_scope(&f.registrations[2].root, &f.registrations[2].collection)?
            .context("current scope absent")?;
        let read = selected_store.store.current_read(&scope)?;
        let config = read
            .collection_state()?
            .context("actual applied config absent")?
            .configuration;
        ensure!(
            config
                .model
                .as_ref()
                .is_some_and(|model| model.pipeline_digest == plan.pipeline_digest),
            "refresh changed immutable shared model pipeline"
        );
        drop(read);
        for (path, bytes) in &expected {
            ensure!(
                fs::read(path)? == *bytes,
                "native modified original source: {}",
                path.display()
            );
        }
        for set in &original_sources[..2] {
            for (path, bytes) in set {
                ensure!(
                    fs::read(path)? == *bytes,
                    "native touched unrelated source: {}",
                    path.display()
                );
            }
        }
    }
    stop(&f, &mut service).await?;
    Ok(())
}
