// SPDX-License-Identifier: Apache-2.0
//! Actual service/RPC workers and scoped evidence transports. Component-format
//! setup is inherited from the existing fixture; this is not installer evidence.
use super::permanent_trim::{
    policy_key as key, policy_revision as revision, policy_rpc_cli as rpc_cli,
};
use super::*;
#[cfg(feature = "embedding-worker")]
use airk_core::{catalog_host::GrantDraftSelection, catalog_management::CatalogOperation};
use airk_core::{
    catalog_split::{
        SplitFinalizationProgress, SplitRecoveryAction, SplitRecoveryOutcome, SplitRecoveryRequest,
        SplitRunState, SplitSnapshot, SplitStage,
    },
    catalog_split_draft::{
        SplitCapacitySelection, SplitDraft, SplitDraftSelection, SplitMembershipSelection,
        SplitProjectSelection,
    },
    index_catalog::{PathScope, ScopeAtom},
};

#[path = "split_metrics.rs"]
mod metrics;

#[cfg(feature = "embedding-worker")]
fn scope_refusal(reply: &Value, session: &CombinedConnectRequest) -> Result<()> {
    ensure!(
        reply["isError"] == true,
        "old authority delivered successful evidence: {reply}"
    );
    let refused = body(reply)?;
    ensure!(
        matches!(
            refused["error"]["code"].as_str(),
            Some("combined_scope_unavailable" | "combined_scope_expired")
        ) && refused["error"]["retryable"] == false
            && refused["error"]["query_replay_safe"] == false
            && refused["requested_scope"]["confirmed"] == false
            && refused["requested_scope"]["selection"] == serde_json::to_value(session)?,
        "old authority failed for an unexpected reason: {refused}"
    );
    Ok(())
}

async fn state(f: &Fixture, operation: Incarnation) -> Result<SplitSnapshot> {
    let response = airk_cli::service_runtime::request(
        &f.control,
        ControlMethod::CatalogSplitStatus {
            operation_id: operation,
        },
    )
    .await?;
    let ControlResponse::CatalogSplitStatus {
        snapshot: Some(snapshot),
        ..
    } = response
    else {
        anyhow::bail!(
            "native split status omitted exact intent: {}",
            serde_json::to_string(&response)?
        );
    };
    snapshot.validate()?;
    Ok(*snapshot)
}

async fn draft(f: &Fixture, nonce: u8) -> Result<SplitDraft> {
    let method = ControlMethod::CatalogSplitSourceState {
        index_id: f.indexes[2],
        expected_catalog_revision: revision(f)?,
    };
    let response = airk_cli::service_runtime::request(&f.control, method.clone()).await?;
    ensure!(
        rpc_cli(f, method).await? == serde_json::to_value(&response)?,
        "CLI split source review reduced canonical result"
    );
    let ControlResponse::CatalogSplitSourceState { state, .. } = response else {
        anyhow::bail!(
            "native split source review unavailable: {}",
            serde_json::to_string(&response)?
        );
    };
    let primary = state
        .policy
        .roots
        .iter()
        .find(|root| root.owner)
        .context("original source absent")?;
    let selection = SplitDraftSelection {
        key: key(f, nonce).await?,
        expected_catalog_revision: state.catalog_revision,
        index_id: f.indexes[2],
        selectors: vec![ScopeAtom {
            source_root_id: primary.id,
            kind: PathScope::Subtree,
            path: "api".into(),
        }],
        destination_name: "Process split destination".into(),
        destination_data_root: f
            .owner
            .path()
            .join(format!("split-data-{nonce}"))
            .to_str()
            .context("UTF8 destination")?
            .into(),
        destination_collection: format!("split{nonce}"),
        projects: state
            .affected_projects
            .iter()
            .map(|project| SplitProjectSelection {
                project_id: project.project_id,
                retain_original: true,
                membership: SplitMembershipSelection::PreserveReviewedScope,
            })
            .collect(),
        capacity: SplitCapacitySelection {
            destination_growth: 256 << 20,
            destination_temporary: 256 << 20,
            original_rewrite: 128 << 20,
            cleanup_allowance: 64 << 20,
        },
        require_semantic_ready: false,
        destination_watch_enabled: false,
    };
    let method = ControlMethod::CatalogSplitDraft {
        selection: Box::new(selection.clone()),
    };
    let response = airk_cli::service_runtime::request(&f.control, method.clone()).await?;
    ensure!(
        rpc_cli(f, method).await? == serde_json::to_value(&response)?,
        "CLI split draft reduced canonical result"
    );
    let ControlResponse::CatalogSplitDraft { draft } = response else {
        anyhow::bail!(
            "native split draft refused: {}",
            serde_json::to_string(&response)?
        );
    };
    draft.validate()?;
    let mut stale = selection.clone();
    stale.expected_catalog_revision = stale.expected_catalog_revision.next()?;
    let response = airk_cli::service_runtime::request(
        &f.control,
        ControlMethod::CatalogSplitDraft {
            selection: Box::new(stale),
        },
    )
    .await?;
    ensure!(
        !matches!(response, ControlResponse::CatalogSplitDraft { .. }),
        "stale review admitted"
    );
    let mut invalid = selection;
    invalid.selectors[0].path = "../escape".into();
    let response = airk_cli::service_runtime::request(
        &f.control,
        ControlMethod::CatalogSplitDraft {
            selection: Box::new(invalid),
        },
    )
    .await?;
    ensure!(
        !matches!(response, ControlResponse::CatalogSplitDraft { .. }),
        "escaping selector admitted"
    );
    ensure!(
        !Path::new(&draft.request.destination_registration.data_root).exists(),
        "read-only review allocated destination"
    );
    Ok(*draft)
}

async fn accept(f: &Fixture, draft: &SplitDraft) -> Result<Incarnation> {
    // Submit mutation once. Observe immutable exact-key receipt on uncertainty.
    let response = airk_cli::service_runtime::request(
        &f.control,
        ControlMethod::CatalogSplitApply {
            draft: Box::new(draft.clone()),
        },
    )
    .await?;
    ensure!(
        matches!(
            response,
            ControlResponse::CatalogSplitAccepted { .. }
                | ControlResponse::CatalogOutcomeUnknown { .. }
        ),
        "native split apply refused: {}",
        serde_json::to_string(&response)?
    );
    let operation = draft.request.operation_id()?;
    let response = airk_cli::service_runtime::request(
        &f.control,
        ControlMethod::CatalogSplitReceipt {
            caller: draft.request.caller,
            key: draft.request.key.clone(),
        },
    )
    .await?;
    ensure!(
        matches!(response, ControlResponse::CatalogSplitReceipt { snapshot: Some(ref snapshot), .. }
        if snapshot.receipt.operation_id == operation && snapshot.request == draft.request),
        "exact acceptance receipt missing"
    );
    Ok(operation)
}

// Bounded read-only native queue/settings evidence, captured before fixture Drop.
// This does not replay a job, recover a journal, or inspect source document bytes.
fn stalled_native_diagnostics(f: &Fixture, snapshot: &SplitSnapshot) -> String {
    let observe = || -> Result<Value> {
        let registry = ServiceRegistry::open_saved(&f.control)?;
        let sql = Connection::open_with_flags(
            f.control.join("service-1.4.sqlite3"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        sql.busy_timeout(Duration::from_secs(2))?;
        let native = registry.catalog_split_state(snapshot.receipt.operation_id)?;
        let mut participants = Vec::new();
        for registration in [
            &snapshot.request.original_registration,
            &snapshot.request.destination_registration,
        ] {
            let mut statement = sql.prepare("SELECT id FROM service_jobs WHERE project=?1 AND state IN('pending','running') ORDER BY seq LIMIT 9")?;
            let ids = statement
                .query_map([registration.binding.to_string()], |row| {
                    row.get::<_, String>(0)
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            let mut jobs = Vec::new();
            for id in ids.iter().take(8) {
                let id: Sha256Digest = id.parse()?;
                let work = registry.configuration_work(id)?;
                jobs.push(json!({"job":registry.job(id)?,
                    "configuration_operation":work.as_ref().map(|work| serde_json::to_value(&work.request.configuration)).transpose()?
                        .and_then(|value| value.get("operation").cloned()),
                    "runs_while_paused":registry.job_runs_while_paused(id)?}));
            }
            participants.push(json!({"binding":registration.binding,"jobs":jobs,"truncated":ids.len()>8,
                "registered":registry.get(registration.binding).map(|row| json!({"paused":row.paused})).unwrap_or_else(|error|json!({"error":format!("{error:#}")})),
                "pending_gate":registry.pending_job_gate(registration.binding).map(|value|json!(value)).unwrap_or_else(|error|json!({"error":format!("{error:#}")}))}));
        }
        let witness_preparation = if snapshot.receipt.stage == SplitStage::DestinationValidated
            && !snapshot.receipt.jobs.iter().any(|job| {
                job.purpose == airk_core::catalog_split::SplitJobPurpose::OriginalWitness
            }) {
            let registration = &snapshot.request.original_registration;
            let prepared = airk_store::service::collection::Collection::existing(
                Path::new(&registration.data_root),
                &registration.root,
                &registration.collection,
            )
            .and_then(|collection| {
                collection.split_work_configuration(
                    &native,
                    airk_core::catalog_split::SplitJobPurpose::OriginalWitness,
                )
            });
            match prepared {
                Ok(work) => {
                    json!({"metadata_preparation":"available","incarnation":work.incarnation,"settings":work.settings})
                }
                Err(error) => {
                    json!({"metadata_preparation":"refused","error":format!("{error:#}")})
                }
            }
        } else {
            Value::Null
        };
        Ok(
            json!({"operation":snapshot.receipt.operation_id,"stage":snapshot.receipt.stage,
            "receipt_digest":snapshot.receipt_digest,"participants":participants,"witness_preparation":witness_preparation}),
        )
    };
    let metadata =
        observe().unwrap_or_else(|error| json!({"diagnostic_error":format!("{error:#}")}));
    let log = (|| -> Result<String> {
        use std::io::{Read, Seek, SeekFrom};
        let mut file = fs::File::open(f.owner.path().join("service.log"))?;
        let bytes = file.metadata()?.len().min(65536);
        file.seek(SeekFrom::End(-(bytes as i64)))?;
        let mut tail = Vec::with_capacity(bytes as usize);
        file.take(bytes).read_to_end(&mut tail)?;
        Ok(String::from_utf8_lossy(&tail).into_owned())
    })()
    .unwrap_or_else(|error| format!("service log unavailable: {error:#}"));
    json!({"native":metadata,"service_log_tail":log}).to_string()
}

async fn terminal(
    f: &Fixture,
    operation: Incarnation,
    canceled: bool,
    capture: &mut Option<metrics::Capture>,
    service: &mut OwnedProcess,
) -> Result<SplitSnapshot> {
    let mut pause_checked_witness = None;
    let deadline = Instant::now() + Duration::from_secs(180);
    loop {
        let snapshot = state(f, operation).await?;
        if let Some(witness) =
            snapshot.receipt.jobs.iter().rev().find(|job| {
                job.purpose == airk_core::catalog_split::SplitJobPurpose::OriginalWitness
            })
            && pause_checked_witness != Some(witness.job)
        {
            let registry = ServiceRegistry::open_saved(&f.control)?;
            ensure!(
                registry.job_runs_while_paused(witness.job)?,
                "actual claimed witness would receive paused=true from native controls"
            );
            let progress = airk_cli::service_runtime::request(
                &f.control,
                ControlMethod::Progress {
                    project: snapshot.request.original_registration.binding,
                },
            )
            .await?;
            ensure!(
                matches!(
                    &progress,
                    ControlResponse::Progress {
                        project_paused: Some(true),
                        ..
                    }
                ),
                "explicit witness altered desired original Pause: {}",
                serde_json::to_string(&progress)?
            );
            if let ControlResponse::Progress {
                pending_job: Some(job),
                pending_paused,
                ..
            } = &progress
                && job.id == witness.job
            {
                ensure!(
                    *pending_paused == Some(false),
                    "native Progress marks exact witness paused: {}",
                    serde_json::to_string(&progress)?
                );
            }
            pause_checked_witness = Some(witness.job);
        }
        if let Some(observer) = capture.as_mut() {
            observer.observe(f, service, &snapshot).await?;
        }
        ensure!(
            !matches!(snapshot.receipt.state, SplitRunState::NeedsAttention { .. })
                && snapshot.receipt.cleanup_attention.is_none()
                && snapshot
                    .finalization
                    .as_ref()
                    .is_none_or(|finalization| finalization.attention.is_none()),
            "native split needs attention: {}; service:{}",
            serde_json::to_string(&snapshot)?,
            fs::read_to_string(f.owner.path().join("service.log")).unwrap_or_default()
        );
        if snapshot.finalization.as_ref().is_some_and(|finalization| {
            finalization.progress == SplitFinalizationProgress::Complete
        }) {
            ensure!(
                if canceled {
                    matches!(
                        snapshot.receipt.state,
                        SplitRunState::CanceledBeforeFence {
                            cleanup_pending: false
                        }
                    )
                } else {
                    snapshot.receipt.stage == SplitStage::Complete
                },
                "native terminal disposition differs"
            );
            return Ok(snapshot);
        }
        if Instant::now() >= deadline {
            let mut progress = Vec::new();
            for registration in [
                &snapshot.request.original_registration,
                &snapshot.request.destination_registration,
            ] {
                progress.push(
                    match airk_cli::service_runtime::request(
                        &f.control,
                        ControlMethod::Progress {
                            project: registration.binding,
                        },
                    )
                    .await
                    {
                        Ok(value) => serde_json::to_value(value)?,
                        Err(error) => json!({"error":format!("{error:#}")}),
                    },
                );
            }
            anyhow::bail!(
                "native split deadline exceeded: {};progress:{};bounded native diagnostics:{}",
                serde_json::to_string(&snapshot)?,
                json!(progress),
                stalled_native_diagnostics(f, &snapshot)
            );
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

#[cfg(feature = "embedding-worker")]
async fn fresh_grant(f: &mut Fixture, snapshot: &SplitSnapshot, nonce: u8) -> Result<()> {
    let response = airk_cli::service_runtime::request(
        &f.control,
        ControlMethod::CatalogGrantDraft {
            context_key: f.session.context_key,
            selection: GrantDraftSelection {
                key: key(f, nonce).await?,
                expected_catalog_revision: revision(f)?,
                project_ids: vec![f.projects[2]],
                index_ids: Some(vec![f.indexes[2], snapshot.request.destination.index_id]),
            },
        },
    )
    .await?;
    let ControlResponse::CatalogGrantDraft { draft } = response else {
        anyhow::bail!(
            "fresh explicit grant refused: {}",
            serde_json::to_string(&response)?
        );
    };
    let CatalogOperation::CreateGrant { grant } = &draft.request.operation else {
        anyhow::bail!("grant review operation changed");
    };
    let grant = grant.clone();
    let response = airk_cli::service_runtime::request(
        &f.control,
        ControlMethod::CatalogApply {
            request: draft.request,
            preview_digest: draft.preview_digest,
        },
    )
    .await?;
    let ControlResponse::CatalogApplied { receipt, .. } = response else {
        anyhow::bail!("grant apply refused: {}", serde_json::to_string(&response)?);
    };
    f.session.grant_id = grant.id;
    f.session.expected_grant_revision = grant.revision;
    f.session.expected_catalog_revision = receipt.catalog_revision;
    f.session.index_ids = Some(vec![f.indexes[2], snapshot.request.destination.index_id]);
    Ok(())
}

#[cfg(feature = "embedding-worker")]
pub(super) async fn pause_quiescent_original(f: &Fixture, index: usize) -> Result<()> {
    let project = f.registrations[index].binding;
    let sql = Connection::open_with_flags(
        f.control.join("service-1.4.sqlite3"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    sql.busy_timeout(Duration::from_secs(2))?;
    // Progress deliberately reports bounded cached work plus pending jobs. This
    // one read-only SQL statement observes actual Pending/Running and pause in
    // the same metadata snapshot, including a job claimed between observations.
    let counts = || -> Result<(bool, u32, u32)> {
        Ok(sql.query_row("SELECT p.paused,coalesce(sum(j.state='pending'),0),coalesce(sum(j.state='running'),0) FROM projects p LEFT JOIN service_jobs j ON j.project=p.binding AND j.state IN('pending','running') WHERE p.binding=?1 GROUP BY p.paused",
            [project.to_string()],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?)))?)
    };
    let deadline = Instant::now() + Duration::from_secs(90);
    let mut observations = Vec::new();
    for attempt in 0..4 {
        ensure!(
            Instant::now() < deadline,
            "shared90s quiescence budget expired before next explicit attempt: {}",
            json!(observations)
        );
        // These are explicit desired-state changes with observed acknowledgments,
        // never replay of a job mutation or cancellation of ordinary native work.
        fixture_pause(f, index, false).await?;
        let starting =
            airk_cli::service_runtime::request(&f.control, ControlMethod::Progress { project })
                .await?;
        eprintln!(
            "split quiescence attempt {} unpaused: {}",
            attempt + 1,
            serde_json::to_string(&starting)?
        );
        observations.push(serde_json::to_value(starting)?);
        loop {
            let (paused, pending, running) = counts()?;
            ensure!(
                !paused,
                "fixture original pause changed unexpectedly during native drain"
            );
            if pending == 0 && running == 0 {
                break;
            }
            ensure!(
                Instant::now() < deadline,
                "native ordinary-work drain exceeded shared90s budget;attempt:{};pending:{};running:{};observations:{};service:{}",
                attempt + 1,
                pending,
                running,
                json!(observations),
                fs::read_to_string(f.owner.path().join("service.log")).unwrap_or_default()
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        let before =
            airk_cli::service_runtime::request(&f.control, ControlMethod::Progress { project })
                .await?;
        fixture_pause(f, index, true).await?;
        let after =
            airk_cli::service_runtime::request(&f.control, ControlMethod::Progress { project })
                .await?;
        let actual = counts()?;
        eprintln!(
            "split quiescence attempt {} pause+recheck atomic={:?};before:{};after:{}",
            attempt + 1,
            actual,
            serde_json::to_string(&before)?,
            serde_json::to_string(&after)?
        );
        observations.push(json!({"atomic":actual,"before":before,"after":after}));
        if actual == (true, 0, 0) && Instant::now() < deadline {
            return Ok(());
        }
        ensure!(
            Instant::now() < deadline,
            "pause raced actual ordinary work and shared90s drain budget expired: {}",
            json!(observations)
        );
        // A real native admission raced Pause. Preserve it, explicitly unpause
        // on the next bounded attempt and let its ordinary owner finish.
    }
    anyhow::bail!(
        "four explicit drain/Pause observations could not establish actual idle ownership: {}",
        json!(observations)
    )
}

#[cfg(feature = "embedding-worker")]
#[tokio::test]
async fn split_actual_service_provider_cutover_retains_complete_rpc_cli_mcp_evidence() -> Result<()>
{
    let mut f = Fixture::with_helper()?;
    f.session.project_ids = vec![f.projects[2]];
    f.session.index_ids = Some(vec![f.indexes[2]]);
    let registration = &f.registrations[2];
    airk_kernel::Kernel::open_service_collection(
        &registration.root,
        Some(registration.data_root.clone().into()),
        &registration.collection,
    )?
    .configure_vectors(true)?;
    let model = f.owner.path().join("split-model");
    fs::create_dir(&model)?;
    fs::write(
        model.join("vocabulary.json"),
        br#"{"boundaryneedle":[1.0,0.0],"original":[0.0,1.0]}"#,
    )?;
    fs::write(
        model.join("model.json"),
        serde_json::to_vec(&json!({"schema":1,"name":"original-process-split-fixture",
        "license":"Apache-2.0","dimensions":2,"sha256":airk_cli::runtime_pair::digest_file(&model.join("vocabulary.json"))?}))?,
    )?;
    let mut _service = f.start().await?;
    fixture_pause(&f, 2, false).await?;
    fixture_install_model(&f, 2, &model).await?;
    fixture_job(&f, 2, airk_core::service::JobKind::Index).await?;
    fixture_job(&f, 2, airk_core::service::JobKind::Vectors).await?;
    pause_quiescent_original(&f, 2).await?;
    let mut capture = metrics::Capture::start(&mut f, &mut _service, "complete").await?;
    let old = airk_cli::combined_frontend::resolve(None, Some(&f.control), &f.session).await?;
    let mut old_mcp = McpSession::start(&f)?;
    let vector = json!({"query":"boundaryneedle","mode":"vector","limit":8,"max_bytes":8192,"verification":"off","response_mode":"full"});
    let warm = native_read(old.as_ref(), "airk_search", &vector).await?;
    ensure!(
        warm["isError"] != true
            && !body(&warm)?["ranking"]
                .as_array()
                .context("vector ranking absent")?
                .is_empty(),
        "actual native provider did not serve original: {warm}"
    );
    let warm_mcp = old_mcp.call(10, "airk_search", vector.clone())?;
    ensure!(
        warm_mcp["result"]["isError"] != true,
        "warm MCP provider refused: {warm_mcp}"
    );
    assert_parity(
        "pre-split actual provider MCP",
        &body(&warm)?,
        &body(&warm_mcp["result"])?,
    )?;
    let reviewed = draft(&f, 131).await?;
    if let Some(observer) = capture.as_mut() {
        observer.destination(&f, &reviewed)?;
    }
    let operation = accept(&f, &reviewed).await?;
    let complete = terminal(&f, operation, false, &mut capture, &mut _service).await?;
    ensure!(
        ServiceRegistry::open_saved(&f.control)?
            .get(f.registrations[2].binding)?
            .paused,
        "native split changed the original user's Pause setting"
    );
    let removal = complete
        .receipt
        .jobs
        .iter()
        .find(|job| job.purpose == airk_core::catalog_split::SplitJobPurpose::OriginalRemoval)
        .context("real original removal child absent")?;
    let response = airk_cli::service_runtime::request(
        &f.control,
        ControlMethod::CatalogReceipt {
            caller: removal.caller,
            key: removal.key.clone(),
        },
    )
    .await?;
    let ControlResponse::CatalogReceipt {
        fence: Some(airk_core::catalog_fences::FenceObservation::Applied(fence)),
        ..
    } = response
    else {
        anyhow::bail!(
            "native original fence receipt absent: {}",
            serde_json::to_string(&response)?
        );
    };
    fence.validate()?;
    let obligation = fence
        .proof
        .worker_obligation
        .as_ref()
        .context("real original worker census absent")?;
    ensure!(
        obligation
            .workers
            .iter()
            .any(|worker| worker.project == f.registrations[2].binding)
        && obligation.providers.iter().any(|provider| matches!(&provider.history,
            airk_core::catalog_fences::WorkerProviderHistory::Helpers { generations } if !generations.is_empty())),
        "real provider-bearing original worker was not captured by native cutover: {obligation:?}"
    );
    ensure!(
        complete.receipt.funding_release_receipts.len() == 2
            && complete
                .finalization
                .as_ref()
                .context("native finalization absent")?
                .funding_acknowledged
                .len()
                == 2,
        "actual native roots were not acknowledged"
    );
    // Old client authority is never renewed across restrictive original cutover.
    let old_reply = old
        .call("airk_search".into(), vector.as_object().unwrap().clone())
        .await
        .context("old native transport failed unexpectedly instead of returning scoped refusal")?;
    scope_refusal(&old_reply, &f.session)?;
    let old_reply = old_mcp
        .call(11, "airk_search", vector)
        .context("old MCP transport failed unexpectedly instead of returning scoped refusal")?;
    scope_refusal(&old_reply["result"], &f.session)?;
    old.close();
    old.shutdown().await?;
    drop(old_mcp);
    for method in [
        ControlMethod::CatalogSplitStatus {
            operation_id: operation,
        },
        ControlMethod::CatalogSplitReceipt {
            caller: reviewed.request.caller,
            key: reviewed.request.key.clone(),
        },
        ControlMethod::CatalogSplitRecoveryReceipt {
            operation_id: operation,
            caller: reviewed.request.caller,
            key: key(&f, 132).await?,
        },
    ] {
        let response = airk_cli::service_runtime::request(&f.control, method.clone()).await?;
        ensure!(
            rpc_cli(&f, method).await? == serde_json::to_value(response)?,
            "actual CLI management result lost fields"
        );
    }
    fresh_grant(&mut f, &complete, 133).await?;
    let backend = airk_cli::combined_frontend::resolve(None, Some(&f.control), &f.session).await?;
    let mut mcp = McpSession::start(&f)?;
    for (tool, args) in [
        (
            "airk_list_files",
            json!({"prefix":"","limit":16,"max_bytes":16384,"response_mode":"full"}),
        ),
        ("airk_status", json!({"response_mode":"full"})),
        (
            "airk_search",
            json!({"query":"boundaryneedle","mode":"lexical","limit":16,"max_bytes":16384,"verification":"off","response_mode":"full"}),
        ),
    ] {
        let native = native_read(backend.as_ref(), tool, &args).await?;
        ensure!(
            native["isError"] != true,
            "post-split evidence refused: {native}"
        );
        let cli = f.cli(tool, &args).await?;
        let mcp_reply = mcp.call(20, tool, args)?;
        ensure!(
            cli["isError"] != true && mcp_reply["result"]["isError"] != true,
            "parity transport returned an error instead of evidence: CLI={cli};MCP={mcp_reply}"
        );
        let native_body = body(&native)?;
        let members = native_body["members"]
            .as_array()
            .context("native member census absent")?;
        let ids = members
            .iter()
            .map(|member| serde_json::from_value::<IndexId>(member["index_id"].clone()))
            .collect::<std::result::Result<std::collections::BTreeSet<_>, _>>()?;
        ensure!(
            members.len() == 2
                && ids
                    == std::collections::BTreeSet::from([
                        f.indexes[2],
                        complete.request.destination.index_id
                    ]),
            "native combined result omitted/added a selected member"
        );
        assert_parity("post-split full CLI", &body(&native)?, &body(&cli)?)?;
        assert_parity(
            "post-split full MCP",
            &body(&native)?,
            &body(&mcp_reply["result"])?,
        )?;
        if tool == "airk_list_files" {
            let body = body(&native)?;
            let mut partition = std::collections::BTreeMap::new();
            for member in body["members"]
                .as_array()
                .context("combined member census absent")?
            {
                let result = member_body(&body, member)?;
                let paths = result["paths"].as_array().context("native paths absent")?;
                let index: IndexId = serde_json::from_value(member["index_id"].clone())?;
                ensure!(
                    result["truncated"] == false,
                    "native partition census truncated"
                );
                partition.insert(
                    index,
                    paths
                        .iter()
                        .map(|path| path.as_str().context("nontext path").map(str::to_owned))
                        .collect::<Result<std::collections::BTreeSet<_>>>()?,
                );
                let destination =
                    result["collection"] == complete.request.destination_registration.collection;
                ensure!(
                    paths.len() == if destination { 1 } else { 3 }
                        && paths.iter().all(|path| path
                            .as_str()
                            .is_some_and(|path| path.starts_with("api/") == destination)),
                    "independent native partition returned wrong files: {result}"
                );
            }
            let original = &partition[&f.indexes[2]];
            let destination = &partition[&complete.request.destination.index_id];
            ensure!(
                original.len() == 3
                    && destination.len() == 1
                    && original.is_disjoint(destination)
                    && original
                        .union(destination)
                        .cloned()
                        .collect::<std::collections::BTreeSet<_>>()
                        == std::collections::BTreeSet::from([
                            "api/policy.md".into(),
                            "impl/policy.md".into(),
                            "private/policy.md".into(),
                            "api-other/policy.md".into()
                        ]),
                "actual partition lost or duplicated an original path"
            );
        }
        if tool == "airk_search" {
            let result = body(&native)?;
            let ranking = result["ranking"]
                .as_array()
                .context("native ranking absent")?;
            ensure!(
                !ranking.is_empty(),
                "native split search returned no evidence"
            );
            for ranked in ranking {
                let origin = airk_core::combined_reads::materialize_ranked_origin(&result, ranked)?;
                let destination = origin.index_id == complete.request.destination.index_id;
                let source = SourceRootId::from_identity(Incarnation::from_bytes([52; 16]));
                let expected = if destination {
                    *complete
                        .request
                        .destination
                        .source_ids
                        .get(&source)
                        .context("destination source remap absent")?
                } else {
                    source
                };
                ensure!(
                    origin.source_root_id == expected
                        && origin.project_ids == vec![f.projects[2]]
                        && origin.source_relative_path.starts_with("api/") == destination,
                    "native split origin changed original location or selected Project"
                );
            }
        }
    }
    for (index, path) in [
        (f.indexes[2], "impl/policy.md"),
        (complete.request.destination.index_id, "api/policy.md"),
    ] {
        let args = json!({"path":path,"project_ids":[f.projects[2]],"index_ids":[index],"verification":"off","response_mode":"full"});
        let native = native_read(backend.as_ref(), "airk_file_info", &args).await?;
        let cli = f.cli("airk_file_info", &args).await?;
        let reply = mcp.call(21, "airk_file_info", args)?;
        ensure!(
            native["isError"] != true
                && cli["isError"] != true
                && reply["result"]["isError"] != true,
            "native source-location proof refused for {index}/{path}: native={native};CLI={cli};MCP={reply}"
        );
        let result = body(&native)?;
        let members = result["members"]
            .as_array()
            .context("file info members absent")?;
        ensure!(
            members.len() == 1 && members[0]["index_id"] == json!(index),
            "file-info scope changed"
        );
        let member = member_body(&result, &members[0])?;
        ensure!(
            member["source"]["root"] == f.registrations[2].root
                && member["source"]["relative_path"] == path
                && member["read"]["original_root"] == f.registrations[2].root
                && member["path"] == path,
            "split changed exported original native source coordinates: {member}"
        );
        assert_parity("split source location CLI", &result, &body(&cli)?)?;
        assert_parity(
            "split source location MCP",
            &result,
            &body(&reply["result"])?,
        )?;
    }
    backend.close();
    backend.shutdown().await?;
    drop(mcp);
    f.assert_sources_unchanged()?;
    if let Some(observer) = capture.take() {
        observer.finish(&f, &mut _service, &complete).await?;
    }
    Ok(())
}

#[tokio::test]
async fn split_actual_service_cancel_has_exact_receipt_without_affecting_healthy_index()
-> Result<()> {
    let mut f = Fixture::new(true)?;
    let mut _service = f.start().await?;
    let mut capture = metrics::Capture::start(&mut f, &mut _service, "cancel").await?;
    let reviewed = draft(&f, 141).await?;
    if let Some(observer) = capture.as_mut() {
        observer.destination(&f, &reviewed)?;
    }
    // This actual exclusive owner exists before the destination registration.
    // Native worker startup acquires this lease before admitting/claiming work.
    // Shared protected setup needs the same real owner, so it remains reserved
    // metadata until this test releases its lease after exact cancellation ACK.
    let held_destination = ProjectLease::acquire(
        &f.control,
        reviewed.request.destination_registration.binding,
    )?;
    let operation = accept(&f, &reviewed).await?;
    let deadline = Instant::now() + Duration::from_secs(90);
    let snapshot = loop {
        let snapshot = state(&f, operation).await?;
        if let Some(observer) = capture.as_mut() {
            observer.observe(&f, &mut _service, &snapshot).await?;
        }
        ensure!(
            !matches!(snapshot.receipt.state, SplitRunState::NeedsAttention { .. }),
            "split preparation needs attention under actual held owner: {}",
            serde_json::to_string(&snapshot)?
        );
        // Shared protected setup also owns this exact ProjectLease. Thus the
        // truthful stable cancellation milestone is Accepted after reservation,
        // before any physical destination, funding or worker job exists.
        if snapshot.receipt.stage == SplitStage::Accepted && snapshot.receipt.revision.get() > 1 {
            ensure!(
                snapshot.receipt.destination_storage.is_none()
                    && snapshot.receipt.capacity_receipt.is_none()
                    && snapshot.receipt.funding_receipts.is_empty()
                    && snapshot.receipt.jobs.is_empty(),
                "held setup owner did not preserve metadata-only cancellation boundary"
            );
            let mut registry = ServiceRegistry::open_saved(&f.control)?;
            let detail = registry.catalog_detail(
                &airk_core::catalog_management::CatalogEntity::Index {
                    id: snapshot.request.destination.index_id,
                },
                None,
            )?;
            ensure!(
                detail.item.is_some(),
                "destination reservation not retained"
            );
            break snapshot;
        }
        ensure!(
            snapshot.receipt.stage == SplitStage::Accepted && Instant::now() < deadline,
            "real held-owner Accepted reservation not reached: {}; bounded native diagnostics:{}",
            serde_json::to_string(&snapshot)?,
            stalled_native_diagnostics(&f, &snapshot)
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    };
    let request = SplitRecoveryRequest {
        caller: reviewed.request.caller,
        key: key(&f, 142).await?,
        operation_id: operation,
        request_digest: snapshot.receipt.request_digest,
        expected_receipt_digest: snapshot.receipt_digest,
        expected_finalization_revision: None,
        action: SplitRecoveryAction::Cancel,
    };
    let response: ControlResponse = serde_json::from_value(
        rpc_cli(
            &f,
            ControlMethod::CatalogSplitRecover {
                request: Box::new(request.clone()),
            },
        )
        .await?,
    )?;
    let ControlResponse::CatalogSplitRecovered { receipt, .. } = response else {
        anyhow::bail!(
            "native cancellation refused: {}",
            serde_json::to_string(&response)?
        );
    };
    ensure!(
        matches!(
            receipt.outcome,
            SplitRecoveryOutcome::CancelBeforeFenceAccepted { .. }
        ),
        "cancel did not retain accepted cleanup"
    );
    drop(held_destination);
    let completed_cancel = terminal(&f, operation, true, &mut capture, &mut _service).await?;
    let response = airk_cli::service_runtime::request(
        &f.control,
        ControlMethod::CatalogSplitRecoveryReceipt {
            operation_id: operation,
            caller: request.caller,
            key: request.key,
        },
    )
    .await?;
    ensure!(
        matches!(response, ControlResponse::CatalogSplitRecoveryReceipt { receipt: Some(ref saved), .. } if **saved == *receipt),
        "native immutable cancellation acknowledgment changed during cleanup"
    );
    let backend = airk_cli::combined_frontend::resolve(None, Some(&f.control), &f.session).await?;
    let healthy = native_read(backend.as_ref(),"airk_search",&json!({"query":"boundaryneedle","mode":"lexical","limit":8,"max_bytes":8192,"verification":"off"})).await?;
    ensure!(
        healthy["isError"] != true,
        "unrelated healthy indexes were poisoned by split cancellation"
    );
    backend.close();
    backend.shutdown().await?;
    f.assert_sources_unchanged()?;
    if let Some(observer) = capture.take() {
        observer
            .finish(&f, &mut _service, &completed_cancel)
            .await?;
    }
    Ok(())
}
