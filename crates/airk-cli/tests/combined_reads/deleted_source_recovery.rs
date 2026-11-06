// SPDX-License-Identifier: Apache-2.0
//! Existing leaf deletion already passed on source 224ee433. These cases extend
//! the native fixture to removed ancestors and missing storage; they do not
//! assert that production currently poisons unrelated indexes.
use super::*;

async fn deletion_read_grant(
    f: &Fixture,
    project: usize,
    index: usize,
    nonce: u8,
) -> Result<CombinedConnectRequest> {
    let response = airk_cli::service_runtime::request(
        &f.control,
        ControlMethod::CatalogGrantDraft {
            context_key: f.session.context_key,
            selection: GrantDraftSelection {
                key: policy_key(f, nonce).await?,
                expected_catalog_revision: policy_revision(f)?,
                project_ids: vec![f.projects[project]],
                index_ids: Some(vec![f.indexes[index]]),
            },
        },
    )
    .await?;
    let ControlResponse::CatalogGrantDraft { draft } = response else {
        anyhow::bail!(
            "dedicated deletion grant review refused: {}",
            serde_json::to_string(&response)?
        );
    };
    let CatalogOperation::CreateGrant { grant } = &draft.request.operation else {
        anyhow::bail!("native grant operation missing");
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
        anyhow::bail!(
            "dedicated deletion grant admission refused: {}",
            serde_json::to_string(&response)?
        );
    };
    let mut selected = f.session.clone();
    selected.project_ids = vec![f.projects[project]];
    selected.index_ids = Some(vec![f.indexes[index]]);
    selected.grant_id = grant.id;
    selected.expected_grant_revision = grant.revision;
    selected.expected_catalog_revision = receipt.catalog_revision;
    Ok(selected)
}

fn deletion_query() -> Value {
    json!({"query":"boundaryneedle","mode":"lexical","limit":12,"max_bytes":8192,
        "verification":"off","response_mode":"full"})
}

async fn healthy_deletion_read(f: &Fixture, mcp: &mut McpSession, id: u64) -> Result<Value> {
    let query = deletion_query();
    let cli = f.cli("airk_search", &query).await?;
    let wire = mcp.call(id, "airk_search", query)?;
    ensure!(
        cli["isError"] != true && wire["result"]["isError"] != true,
        "healthy selected member refused: CLI={cli}; MCP={wire}"
    );
    policy_parity(
        "healthy index across unrelated source loss",
        &cli,
        &wire["result"],
    )?;
    let result = body(&cli)?;
    let members = result["members"]
        .as_array()
        .context("healthy member inventory missing")?;
    let expected = f
        .session
        .index_ids
        .as_ref()
        .filter(|ids| ids.len() == 1)
        .context("healthy grant must select exactly one explicit index")?[0];
    ensure!(
        members.len() == 1 && members[0]["index_id"] == expected.to_string(),
        "healthy read broadened or crossed index scope: {result}"
    );
    let member = member_body(&result, &members[0])?;
    ensure!(
        member["hits"]
            .as_array()
            .is_some_and(|hits| !hits.is_empty()),
        "healthy member evidence missing: {member}"
    );
    // Only the exact retained publication is compared across independent dated reads.
    Ok(
        result["publication_vector"]["members"][0]["publication"]["token"]
            .as_str()
            .context("healthy publication token missing")?
            .into(),
    )
}

// Missing storage is a failed member under Report, not an RPC or whole-union
// failure. Explicit Require still refuses while retaining that same complete A
// outcome. Exercise all three actual read paths without replaying a mutation.
async fn missing_storage_reads(f: &Fixture, mcp: &mut McpSession, first_id: u64) -> Result<()> {
    for (offset, required) in [false, true].into_iter().enumerate() {
        let mut args = deletion_query();
        args["project_ids"] = json!(f.session.project_ids);
        args["member_availability"] = json!(if required { "require" } else { "report" });
        let backend =
            airk_cli::combined_frontend::resolve(None, Some(&f.control), &f.session).await?;
        let native = native_read(backend.as_ref(), "airk_search", &args).await;
        backend.close();
        backend.shutdown().await?;
        let native = native?;
        let cli = f.cli("airk_search", &args).await?;
        let wire = mcp.call(first_id + offset as u64, "airk_search", args)?;
        ensure!(
            wire.get("error").is_none() && wire["result"].is_object(),
            "missing storage produced an RPC failure instead of a member result: {wire}"
        );
        policy_parity("missing storage native/CLI", &native, &cli)?;
        policy_parity("missing storage native/MCP", &native, &wire["result"])?;
        for reply in [&native, &cli, &wire["result"]] {
            ensure!(
                reply.get("isError").is_none_or(Value::is_boolean)
                    && (reply["isError"] == true) == required,
                "missing storage Report/Require outer flag differs: {reply}"
            );
        }
        let value = body(&native)?;
        ensure!(
            value["schema"] == "airk.combined-read/1"
                && value["partial"] == true
                && value["ranking"].as_array().is_some_and(Vec::is_empty),
            "missing storage invented complete or ranked evidence: {value}"
        );
        let members = value["members"]
            .as_array()
            .context("missing storage member inventory absent")?;
        ensure!(
            members.len() == 1
                && members[0]["index_id"] == json!(f.indexes[2])
                && members[0]["outcome"] == "failed"
                && members[0]["is_error"] == true,
            "missing storage omitted or changed selected A: {value}"
        );
        let child = member_body(&value, &members[0])?;
        ensure!(
            child["error"]["code"] == "scoped_connection_unavailable"
                && child["error"]["query_submitted"] == false
                && child["error"]["retryable"] == false,
            "missing storage failed for another reason: {child}"
        );
        let publications = value["publication_vector"]["members"]
            .as_array()
            .context("missing storage publication inventory absent")?;
        ensure!(
            publications.len() == 1
                && publications[0]["index_id"] == json!(f.indexes[2])
                && publications[0]["publication"].is_null(),
            "missing storage invented an A publication: {value}"
        );
        ensure!(
            if required {
                value["error"]["code"] == "member_unavailable"
            } else {
                value["error"].is_null()
            },
            "explicit member availability contract changed: {value}"
        );
    }
    Ok(())
}

async fn stop_deletion_service(f: &Fixture, service: &mut Option<OwnedProcess>) -> Result<()> {
    let response = airk_cli::service_runtime::request(&f.control, ControlMethod::Shutdown).await?;
    ensure!(
        !matches!(response, ControlResponse::Failure { .. }),
        "owned service shutdown refused: {}",
        serde_json::to_string(&response)?
    );
    drop(service.take());
    Ok(())
}

fn delete_owned_source_parent(f: &Fixture, data_lost: bool) -> Result<()> {
    let owner = f.owner.path().canonicalize()?;
    let root = Path::new(&f.registrations[2].root);
    let removed = root.parent().context("nested source parent missing")?;
    ensure!(
        removed.starts_with(&owner)
            && removed != owner
            && !f.control.starts_with(removed)
            && !f.candidate.starts_with(removed),
        "deletion target escaped its isolated fixture"
    );
    ensure!(
        Path::new(&f.registrations[2].data_root).starts_with(removed) == data_lost,
        "fixture storage loss mode differs from planned deletion"
    );
    fs::remove_dir_all(removed)?;
    ensure!(!root.exists(), "fixture did not remove source");
    Ok(())
}

async fn deleted_ancestor_case(data_lost: bool, policy_bearing: bool) -> Result<()> {
    // The existing fixture has three indexes. A=2 and B=1 are the two measured
    // independent indexes; index 0 remains an untouched isolation control.
    let mut f = Fixture::with_runtime_layout(true, 3, false, Some(data_lost))?;
    let mut service = Some(f.start().await?);
    let expected_policy = if policy_bearing {
        let trim = policy_draft(&f, 141, trim_selector()).await?;
        apply_policy(&f, &trim).await?;
        Some(wait_policy(&f).await?)
    } else {
        None
    };
    let healthy_session = deletion_read_grant(&f, 0, 1, 142).await?;
    let affected_session = deletion_read_grant(&f, 2, 2, 143).await?;
    f.session = affected_session.clone();
    let mut affected = McpSession::start(&f)?;
    let warm = affected.call(10, "airk_search", deletion_query())?;
    ensure!(
        warm["result"]["isError"] != true,
        "affected baseline failed: {warm}"
    );
    f.session = healthy_session.clone();
    let mut healthy = McpSession::start(&f)?;
    let baseline = healthy_deletion_read(&f, &mut healthy, 20).await?;

    if data_lost {
        // Windows may deny deleting an open SQLite file. Stop exact owned
        // clients/owners first; do not weaken sharing or use ACL workarounds.
        drop(affected);
        drop(healthy);
        stop_deletion_service(&f, &mut service).await?;
        delete_owned_source_parent(&f, true)?;
    } else {
        delete_owned_source_parent(&f, false)?;
        ensure!(
            healthy_deletion_read(&f, &mut healthy, 30).await? == baseline,
            "warm healthy publication changed after A ancestor removal"
        );
        drop(affected);
        drop(healthy);
        stop_deletion_service(&f, &mut service).await?;
    }

    // Cold startup must preserve B even when A cannot start an evidence worker.
    service = Some(f.start().await?);
    f.session = healthy_session.clone();
    let mut healthy = McpSession::start(&f)?;
    ensure!(
        healthy_deletion_read(&f, &mut healthy, 40).await? == baseline,
        "cold healthy publication changed after A loss"
    );
    if data_lost {
        f.session = affected_session.clone();
        let mut unavailable = McpSession::start(&f)?;
        missing_storage_reads(&f, &mut unavailable, 50).await?;
        drop(unavailable);
        let registry = ServiceRegistry::open_saved(&f.control)?;
        let budget = registry.recovery_budget(
            &f.registrations[2].binding.to_string(),
            airk_cli::runtime_pair::engine_identity_at(&f.candidate)?,
        )?;
        ensure!(
            budget.failures <= 3,
            "missing storage escaped its durable startup circuit: {budget:?}"
        );
        drop(registry);
        f.session = healthy_session.clone();
        ensure!(
            healthy_deletion_read(&f, &mut healthy, 60).await? == baseline,
            "healthy member failed after an actual missing-storage admission"
        );
    }

    // A missing ancestor is not the existing positively confirmed missing leaf.
    // Retire deliberately through the metadata owner, without inspecting A's
    // missing Kernel state or manufacturing a worker completion.
    let registration = f.registrations[2].clone();
    ensure!(
        ServiceRegistry::open_saved(&f.control)?.is_registered(registration.binding)?,
        "unproven ancestor absence silently retired the index"
    );
    let retirement = policy_draft(&f, 144, SourceMutationSelection::RetireIndex).await?;
    apply_policy(&f, &retirement).await?;
    let evidence_method = ControlMethod::CatalogIndexRetirementEvidence {
        index_id: f.indexes[2],
    };
    let native = airk_cli::service_runtime::request(&f.control, evidence_method.clone()).await?;
    ensure!(
        policy_rpc_cli(&f, evidence_method).await? == serde_json::to_value(&native)?,
        "explicit retirement transport omitted native evidence"
    );
    let ControlResponse::CatalogIndexRetirementEvidence {
        evidence: Some(evidence),
        ..
    } = native
    else {
        anyhow::bail!("exact native retirement evidence missing");
    };
    evidence.validate()?;
    ensure!(
        evidence.index.tombstone && evidence.index.backing == registration,
        "retirement changed the selected backing"
    );
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        let registry = ServiceRegistry::open_saved(&f.control)?;
        let settled = registry.project_was_removed(registration.binding)?;
        ensure!(
            !registry.is_registered(registration.binding)?,
            "retired source became registered again"
        );
        if settled {
            break;
        }
        ensure!(
            Instant::now() < deadline,
            "explicit absent-storage cleanup did not settle; policy={policy_bearing}; inspect owned receipts"
        );
        drop(registry);
        // Continuously exercise B while A's asynchronous cleanup is observed.
        ensure!(
            healthy_deletion_read(&f, &mut healthy, 70).await? == baseline,
            "healthy member became unusable during A cleanup"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let registry = ServiceRegistry::open_saved(&f.control)?;
    ensure!(
        !registry
            .pending_project_removals(None, 32)?
            .iter()
            .any(|project| project.binding == registration.binding),
        "completed absent-storage cleanup remained in the error-poll selection"
    );
    if let Some(expected) = expected_policy {
        let current = registry
            .catalog_policy_status(f.indexes[2])?
            .context("absent storage erased permanent policy")?;
        ensure!(
            current.policy == expected.policy && current.job == expected.job,
            "absent-storage cleanup changed policy/job lineage"
        );
    }
    drop(registry);
    let worker = airk_cli::service_runtime::request(
        &f.control,
        ControlMethod::Worker {
            project: registration.binding,
        },
    )
    .await?;
    ensure!(
        matches!(worker, ControlResponse::Worker { identity: None, .. }),
        "retirement left or restarted A's evidence worker"
    );
    ensure!(
        !Path::new(&registration.root).exists(),
        "cleanup recreated the removed source ancestor"
    );
    if data_lost {
        ensure!(
            !Path::new(&registration.data_root).exists(),
            "cleanup recreated removed storage to open its quota or capsule"
        );
    }
    ensure!(
        healthy_deletion_read(&f, &mut healthy, 80).await? == baseline,
        "healthy final publication changed"
    );
    drop(healthy);
    stop_deletion_service(&f, &mut service).await?;
    service = Some(f.start().await?);
    f.session = healthy_session;
    let mut healthy = McpSession::start(&f)?;
    ensure!(
        healthy_deletion_read(&f, &mut healthy, 90).await? == baseline,
        "healthy publication failed after retired-A cold restart"
    );
    let worker = airk_cli::service_runtime::request(
        &f.control,
        ControlMethod::Worker {
            project: registration.binding,
        },
    )
    .await?;
    ensure!(
        matches!(worker, ControlResponse::Worker { identity: None, .. }),
        "retired missing source restarted after service restart"
    );
    drop(healthy);
    stop_deletion_service(&f, &mut service).await?;
    Ok(())
}

#[tokio::test]
async fn post_p14_removed_source_ancestor_keeps_healthy_cli_mcp_warm_and_cold() -> Result<()> {
    deleted_ancestor_case(false, false).await
}

#[tokio::test]
async fn post_p14_source_and_storage_loss_supports_explicit_retirement() -> Result<()> {
    deleted_ancestor_case(true, false).await
}

#[tokio::test]
async fn post_p14_policy_bearing_missing_storage_settles_without_losing_policy() -> Result<()> {
    deleted_ancestor_case(true, true).await
}

// Uses the real native Index job, never a forged publication or watch event.
#[cfg(feature = "embedding-worker")]
#[tokio::test]
async fn post_p14_individual_file_removal_keeps_healthy_index_and_reconciles_retained_paths()
-> Result<()> {
    let mut f = Fixture::new(true)?;
    let mut service = Some(f.start().await?);
    let healthy_session = deletion_read_grant(&f, 0, 1, 151).await?;
    let affected_session = deletion_read_grant(&f, 2, 2, 152).await?;
    f.session = healthy_session.clone();
    let mut healthy = McpSession::start(&f)?;
    let before = healthy_deletion_read(&f, &mut healthy, 100).await?;
    let selected = Path::new(&f.registrations[2].root).join("api/policy.md");
    let preserved = Path::new(&f.registrations[2].root).join("impl/policy.md");
    let preserved_bytes = fs::read(&preserved)?;
    fs::remove_file(&selected)?;
    ensure!(
        healthy_deletion_read(&f, &mut healthy, 101).await? == before,
        "warm healthy member changed after A file removal"
    );
    // This shared fixture deliberately starts paused. Ordinary Index work must
    // be explicitly admitted, then drained before restoring that user choice.
    fixture_pause(&f, 2, false).await?;
    fixture_job(&f, 2, airk_core::service::JobKind::Index).await?;
    super::super::split::pause_quiescent_original(&f, 2).await?;
    f.session = affected_session.clone();
    let mut affected = McpSession::start(&f)?;
    let args = json!({"prefix":"api/","limit":16,"max_bytes":8192,"response_mode":"full"});
    let cli = f.cli("airk_list_files", &args).await?;
    let wire = affected.call(102, "airk_list_files", args)?;
    ensure!(
        cli["isError"] != true && wire["result"]["isError"] != true,
        "surviving A index refused after ordinary reconciliation: {cli}; {wire}"
    );
    policy_parity(
        "removed-file native retained inventory",
        &cli,
        &wire["result"],
    )?;
    ensure!(
        !serde_json::to_string(&body(&cli)?)?.contains("api/policy.md"),
        "reconciled file inventory still contains removed path"
    );
    ensure!(
        ServiceRegistry::open_saved(&f.control)?.is_registered(f.registrations[2].binding)?,
        "a single missing file retired its whole index"
    );
    ensure!(
        fs::read(&preserved)? == preserved_bytes && !selected.exists(),
        "reconciliation changed surviving source bytes or recreated removed file"
    );
    drop(affected);
    f.session = healthy_session.clone();
    ensure!(
        healthy_deletion_read(&f, &mut healthy, 103).await? == before,
        "healthy member changed during A reconciliation"
    );
    drop(healthy);
    stop_deletion_service(&f, &mut service).await?;
    service = Some(f.start().await?);
    let mut healthy = McpSession::start(&f)?;
    ensure!(
        healthy_deletion_read(&f, &mut healthy, 104).await? == before,
        "healthy member changed after cold restart"
    );
    f.session = affected_session;
    let retirement = policy_draft(&f, 153, SourceMutationSelection::RetireIndex).await?;
    apply_policy(&f, &retirement).await?;
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        if ServiceRegistry::open_saved(&f.control)?
            .project_was_removed(f.registrations[2].binding)?
        {
            break;
        }
        ensure!(
            Instant::now() < deadline,
            "explicit A retirement did not settle"
        );
        f.session = healthy_session.clone();
        ensure!(
            healthy_deletion_read(&f, &mut healthy, 105).await? == before,
            "healthy member changed during explicit A retirement"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    ensure!(
        fs::read(&preserved)? == preserved_bytes,
        "retirement removed surviving source"
    );
    drop(healthy);
    stop_deletion_service(&f, &mut service).await?;
    Ok(())
}

#[tokio::test]
async fn post_p14_missing_collection_settles_ordinary_credit_without_poisoning_healthy_shared_root()
-> Result<()> {
    let mut f = Fixture::with_shared_runtime()?;
    let mut service = Some(f.start().await?);
    let healthy_session = deletion_read_grant(&f, 0, 1, 161).await?;
    f.session = healthy_session.clone();
    let mut healthy = McpSession::start(&f)?;
    let before = healthy_deletion_read(&f, &mut healthy, 110).await?;
    drop(healthy);
    stop_deletion_service(&f, &mut service).await?;
    let registration = f.registrations[2].clone();
    let data = Path::new(&registration.data_root);
    ensure!(
        data == Path::new(&f.registrations[1].data_root),
        "fixture did not share quota root"
    );
    let grants = data.join(".quota-1.4/current-grants");
    let inventory = |collection: &str| -> Result<Vec<(PathBuf, Vec<u8>)>> {
        let mut rows = Vec::new();
        for entry in fs::read_dir(&grants)? {
            let path = entry?.path();
            let bytes = fs::read(&path)?;
            let body: Value = serde_json::from_slice(&bytes)?;
            if body["scope"]["collection"] == collection {
                rows.push((path, bytes));
            }
        }
        rows.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(rows)
    };
    let a_rows = inventory(&registration.collection)?;
    ensure!(
        !a_rows.is_empty(),
        "fixture did not leave real native durable Current allowance; do not claim grant recovery without a paid native fixture"
    );
    let b_rows = inventory(&f.registrations[1].collection)?;
    ensure!(
        !b_rows.is_empty(),
        "healthy native Current allowance fixture is vacuous"
    );
    let quota = airk_store::quota::Quota::open_service(data)?;
    let previous_policy = quota.policy()?;
    let capacity = quota.status()?;
    let mut constrained = previous_policy.clone();
    constrained.root_bytes = capacity.used_bytes + capacity.reserved_bytes + 128;
    quota.configure_if_current(constrained, Some(&previous_policy))?;
    ensure!(
        quota.reserve(&f.registrations[1].collection, 4096).is_err(),
        "ordinary writer consumed unavailable prepaid allowance"
    );
    let logical = data.join("collections-1.4").join(&registration.collection);
    let owner = f.owner.path().canonicalize()?;
    ensure!(
        logical.starts_with(&owner) && logical != data && !f.control.starts_with(&logical),
        "selected collection deletion escaped fixture"
    );
    fs::remove_dir_all(&logical)?;
    service = Some(f.start().await?);
    let mut healthy = McpSession::start(&f)?;
    ensure!(
        healthy_deletion_read(&f, &mut healthy, 111).await? == before,
        "cold shared-root healthy member changed after A storage loss"
    );
    let retirement = policy_draft(&f, 162, SourceMutationSelection::RetireIndex).await?;
    apply_policy(&f, &retirement).await?;
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        let registry = ServiceRegistry::open_saved(&f.control)?;
        if registry.project_was_removed(registration.binding)? {
            ensure!(
                !registry
                    .pending_project_removals(None, 32)?
                    .iter()
                    .any(|p| p.binding == registration.binding),
                "settled shared-root removal remained selected for cleanup"
            );
            break;
        }
        ensure!(
            Instant::now() < deadline,
            "shared-root credit settlement did not finish"
        );
        drop(registry);
        ensure!(
            healthy_deletion_read(&f, &mut healthy, 112).await? == before,
            "healthy B failed while exact A credit settled"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    ensure!(
        inventory(&registration.collection)?.is_empty(),
        "disposed A durable grants leaked"
    );
    ensure!(
        inventory(&f.registrations[1].collection)? == b_rows,
        "A cleanup changed B grant identity/bytes"
    );
    let paid_b = quota.reserve(&f.registrations[1].collection, 4096)?;
    paid_b.charge(4096)?;
    drop(paid_b);
    ensure!(
        !logical.exists(),
        "cleanup recreated missing logical collection"
    );
    f.assert_sources_unchanged()?;
    drop(healthy);
    stop_deletion_service(&f, &mut service).await?;
    service = Some(f.start().await?);
    f.session = healthy_session;
    let mut healthy = McpSession::start(&f)?;
    ensure!(
        healthy_deletion_read(&f, &mut healthy, 113).await? == before,
        "healthy B failed after settled A restart"
    );
    let worker = airk_cli::service_runtime::request(
        &f.control,
        ControlMethod::Worker {
            project: registration.binding,
        },
    )
    .await?;
    ensure!(
        matches!(worker, ControlResponse::Worker { identity: None, .. }),
        "retired A evidence worker restarted"
    );
    drop(healthy);
    stop_deletion_service(&f, &mut service).await?;
    Ok(())
}

// Called ONLY after a real shared-root provider cutover and native two-participant
// finalization/ack. No complete/release/current_owned bit is constructed by tests.
#[cfg(feature = "embedding-worker")]
pub(crate) async fn retire_completed_destination(
    f: &mut Fixture,
    service: &mut Option<OwnedProcess>,
    complete: &airk_core::catalog_split::SplitSnapshot,
    interrupt_retirement: bool,
) -> Result<()> {
    use airk_store::quota::funding::FundingChildKind;
    let destination = complete.request.destination_registration.clone();
    ensure!(
        destination.data_root == complete.request.original_registration.data_root,
        "terminal Current preservation regression requires a genuinely shared root"
    );
    let quota = airk_store::quota::Quota::open_service(Path::new(&destination.data_root))?;
    let operation = complete.receipt.operation_id;
    let before = quota.observe_funding(operation)?;
    ensure!(
        before.complete && before.release_acknowledged,
        "actual native cutover did not acknowledge ordinary Current ownership"
    );
    let selected: Vec<_> = before
        .children
        .iter()
        .filter(|child| child.index_id == complete.request.destination.index_id)
        .cloned()
        .collect();
    let original: Vec<_> = before
        .children
        .iter()
        .filter(|child| child.index_id == complete.request.original_index)
        .cloned()
        .collect();
    ensure!(
        selected.len() == 1
            && original.is_empty()
            && before.children == selected
            && selected.iter().all(|child| child.current_owned
                && matches!(child.kind, FundingChildKind::Current { .. })),
        "real native terminal WAL children missing; selected={}, original={}; complete bounded actual funding={}",
        selected.len(),
        original.len(),
        {
            airk_core::index_catalog::encoded_size(&before, airk_core::MAX_RESPONSE_BYTES)?;
            serde_json::to_string(&before)?
        }
    );
    let healthy = deletion_read_grant(f, 2, 2, 181).await?;
    f.session = healthy;
    let mut mcp = McpSession::start(f)?;
    let token = healthy_deletion_read(f, &mut mcp, 120).await?;
    drop(mcp);
    stop_deletion_service(f, service).await?;
    // The original already owns a standalone native WAL allowance. The split
    // reuses it; only the newly created destination consumes funded Current.
    let original_registration = &complete.request.original_registration;
    let ordinary_inventory = || -> Result<Vec<(PathBuf, Vec<u8>)>> {
        let mut rows = Vec::new();
        for entry in
            fs::read_dir(Path::new(&destination.data_root).join(".quota-1.4/current-grants"))?
        {
            let path = entry?.path();
            let metadata = fs::symlink_metadata(&path)?;
            ensure!(
                metadata.is_file() && !metadata.file_type().is_symlink() && metadata.len() <= 65536,
                "invalid ordinary Current fixture record"
            );
            let bytes = fs::read(&path)?;
            let record: Value = serde_json::from_slice(&bytes)?;
            if record["scope"]["collection"] == original_registration.collection {
                rows.push((path, bytes));
            }
        }
        rows.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(rows)
    };
    let ordinary_before = ordinary_inventory()?;
    ensure!(
        !ordinary_before.is_empty(),
        "original native standalone WAL credit missing"
    );
    #[cfg(windows)]
    let ordinary_scope: airk_store::quota::CurrentGrantScope = serde_json::from_value(
        serde_json::from_slice::<Value>(&ordinary_before[0].1)?["scope"].clone(),
    )?;
    let parent_path = Path::new(&destination.data_root)
        .join(".quota-1.4/split-allocations")
        .join(format!("f-{operation}.json"));
    let scratch = parent_path
        .parent()
        .context("funding parent directory missing")?
        .join(format!("funding-retire-{}", before.identity()?));
    ensure!(
        !scratch.try_exists()?,
        "native fixture unexpectedly has retirement scratch"
    );
    let logical = Path::new(&destination.data_root)
        .join("collections-1.4")
        .join(&destination.collection);
    let owner = f.owner.path().canonicalize()?;
    ensure!(
        logical.starts_with(&owner)
            && logical != Path::new(&destination.data_root)
            && !f.control.starts_with(&logical),
        "terminal selected deletion escaped fixture"
    );
    fs::remove_dir_all(&logical)?;
    *service = Some(f.start().await?);
    let mut mcp = McpSession::start(f)?;
    ensure!(
        healthy_deletion_read(f, &mut mcp, 121).await? == token,
        "surviving original changed after terminal destination loss"
    );
    drop(mcp);
    let response = airk_cli::service_runtime::request(
        &f.control,
        ControlMethod::CatalogSourceMutationDraft {
            selection: SourceMutationDraftSelection {
                key: policy_key(f, 182).await?,
                expected_catalog_revision: policy_revision(f)?,
                index_id: complete.request.destination.index_id,
                change: SourceMutationSelection::RetireIndex,
            },
        },
    )
    .await?;
    let ControlResponse::CatalogSourceMutationDraft { draft } = response else {
        anyhow::bail!(
            "completed split destination retirement review refused: {}",
            serde_json::to_string(&response)?
        );
    };
    // A test-only OS share denial interrupts the REAL native final-parent unlink,
    // not a fabricated completed funding row or a production fault environment.
    #[cfg(windows)]
    let interrupted_parent = if interrupt_retirement {
        use std::os::windows::fs::OpenOptionsExt;
        Some(
            std::fs::OpenOptions::new()
                .read(true)
                .share_mode(1 | 2)
                .open(
                    Path::new(&destination.data_root)
                        .join(".quota-1.4/split-allocations")
                        .join(format!("f-{operation}.json")),
                )?,
        )
    } else {
        None
    };
    #[cfg(not(windows))]
    ensure!(
        !interrupt_retirement,
        "OS share-denial regression is Windows-only"
    );
    apply_policy(f, &draft).await?;
    #[cfg(windows)]
    if let Some(parent) = interrupted_parent {
        // The destination is the sole funded child, so native cleanup unlinks
        // its parent directly. No replacement scratch belongs to this branch.
        stop_deletion_service(f, service).await?;
        ensure!(
            !ServiceRegistry::open_saved(&f.control)?.project_was_removed(destination.binding)?,
            "share denial did not interrupt native disposal"
        );
        ensure!(service.is_none(), "owned native service was not reaped");
        // OwnedProcess::drop drains this exact Job, waits for every captured
        // descendant HANDLE and ActiveProcesses==0, then reaps the root child.
        // Now force the owning cleanup attempt to finish synchronously: merely
        // stopping a worker alone cannot prove the anchored unlink was denied.
        let saved = ServiceRegistry::open_saved(&f.control)?;
        let incarnation = saved.incarnation()?;
        drop(saved);
        let mut native = ServiceRegistry::open(&f.control, incarnation)?;
        ensure!(
            native
                .pending_project_removals(None, 32)?
                .contains(&destination),
            "exact A retirement was not pending before synchronous retry"
        );
        let funding_before_denial = quota.observe_funding(operation)?;
        ensure!(
            funding_before_denial == before,
            "native interruption changed full funding parent before denied retry"
        );
        let denial = native
            .purge_removed_project_turn(destination.binding)
            .err()
            .context("no-DELETE HANDLE failed to deny actual native final-parent unlink")?;
        let os = denial
            .downcast_ref::<std::io::Error>()
            .context("native metadata denial lost its typed OS error")?;
        let expected_context = match os.raw_os_error() {
            Some(32) => "NtCreateFile relative child: NTSTATUS 0xc0000043",
            Some(5) => "NtCreateFile relative child: NTSTATUS 0xc0000022",
            other => anyhow::bail!("unexpected native unlink OS error {other:?}: {denial:#}"),
        };
        ensure!(
            denial
                .chain()
                .any(|cause| cause.to_string() == expected_context),
            "denial was not the exact anchored final-parent unlink open: {denial:#}"
        );
        ensure!(
            !native.project_was_removed(destination.binding)?
                && native
                    .pending_project_removals(None, 32)?
                    .contains(&destination)
                && native.catalog_split_status(operation)?.as_ref() == Some(complete)
                && quota.observe_funding(operation)? == funding_before_denial
                && !scratch.try_exists()?,
            "denied native retry changed exact retirement/funding/terminal receipt or created replacement scratch"
        );
        drop(native);
        drop(parent);
        let interrupted = quota.observe_funding(operation)?;
        ensure!(
            interrupted == before && !scratch.try_exists()?,
            "interruption prematurely released funded Current ownership"
        );
        let collection = airk_store::service::collection::Collection::existing(
            Path::new(&original_registration.data_root),
            &original_registration.root,
            &original_registration.collection,
        )?;
        let store = collection.selected_store()?;
        let writer = store.lock()?;
        let mut grants = quota.current_grants(&writer)?;
        let grant = grants
            .iter_mut()
            .find(|grant| grant.scope() == &ordinary_scope)
            .context("exact healthy original standalone Current grant not found")?;
        let capacity = grant.capacity();
        ensure!(capacity > 1, "real Current fixture lacks resize headroom");
        grant.resize(&writer, capacity - 1)?;
        ensure!(
            quota
                .current_grant(&writer, &ordinary_scope)?
                .context("resized grant missing")?
                .capacity()
                == capacity - 1
                && ordinary_inventory()? != ordinary_before,
            "ordinary Current resize did not persist real progress"
        );
        grant.resize(&writer, capacity)?;
        drop(grants);
        drop(writer);
        drop(store);
        drop(collection);
        ensure!(
            quota.observe_funding(operation)? == interrupted
                && ordinary_inventory()? == ordinary_before
                && !scratch.try_exists()?,
            "ordinary B progress changed A funding lineage/revision or failed exact restoration"
        );
        *service = Some(f.start().await?);
    }
    // This is an explicit new grant review after restrictive native retirement;
    // never silently renew the old Project authority or relax its errors.
    f.session = deletion_read_grant(f, 2, 2, 183).await?;
    let mut mcp = McpSession::start(f)?;
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        if ServiceRegistry::open_saved(&f.control)?.project_was_removed(destination.binding)? {
            break;
        }
        ensure!(
            Instant::now() < deadline,
            "acknowledged Current-owned destination cleanup did not settle"
        );
        ensure!(
            healthy_deletion_read(f, &mut mcp, 122).await? == token,
            "terminal A disposal changed surviving B publication"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    ensure!(
        matches!(fs::symlink_metadata(&parent_path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound)
            && !scratch.try_exists()?
            && ordinary_inventory()? == ordinary_before,
        "final funded parent was not removed or original ordinary Current credit changed"
    );
    let saved = ServiceRegistry::open_saved(&f.control)?
        .catalog_split_status(operation)?
        .context("completed split receipt missing")?;
    ensure!(
        saved == *complete,
        "ordinary Current disposal changed the exact completed native receipt"
    );
    ensure!(
        !logical.exists(),
        "terminal disposal recreated the missing destination"
    );
    f.assert_sources_unchanged()?;
    drop(mcp);
    stop_deletion_service(f, service).await?;
    *service = Some(f.start().await?);
    let mut mcp = McpSession::start(f)?;
    ensure!(
        healthy_deletion_read(f, &mut mcp, 123).await? == token,
        "surviving original failed after retired terminal destination restart"
    );
    let worker = airk_cli::service_runtime::request(
        &f.control,
        ControlMethod::Worker {
            project: destination.binding,
        },
    )
    .await?;
    ensure!(
        matches!(worker, ControlResponse::Worker { identity: None, .. }),
        "retired terminal destination worker restarted"
    );
    drop(mcp);
    stop_deletion_service(f, service).await?;
    Ok(())
}

// Same-Project isolation: keep the unavailable index inside the reviewed union.
async fn union_apply(
    f: &Fixture,
    nonce: u8,
    operation: airk_core::catalog_management::CatalogOperation,
) -> Result<()> {
    let request = airk_core::catalog_management::CatalogRequest {
        caller: Incarnation::from_bytes([219; 16]),
        key: policy_key(f, nonce).await?,
        expected_catalog_revision: policy_revision(f)?,
        operation,
    };
    let response = airk_cli::service_runtime::request(
        &f.control,
        ControlMethod::CatalogPreview {
            request: request.clone(),
        },
    )
    .await?;
    let ControlResponse::CatalogPreview {
        preview,
        preview_digest,
    } = response
    else {
        anyhow::bail!(
            "union metadata preview refused: {}",
            serde_json::to_string(&response)?
        );
    };
    ensure!(
        preview.identity()? == preview_digest,
        "union preview identity differs"
    );
    let response = airk_cli::service_runtime::request(
        &f.control,
        ControlMethod::CatalogApply {
            request,
            preview_digest,
        },
    )
    .await?;
    ensure!(
        matches!(response, ControlResponse::CatalogApplied { .. }),
        "union metadata apply refused: {}",
        serde_json::to_string(&response)?
    );
    Ok(())
}

async fn deletion_default_union(f: &mut Fixture) -> Result<()> {
    use airk_core::catalog_management::{CatalogEntity, CatalogItem};
    // Project2 already owns A. Add B with its real existing api-only scope;
    // Project0/index0 never becomes part of this default reviewed union.
    let mut registry = ServiceRegistry::open_saved(&f.control)?;
    let Some(CatalogItem::Membership(mut membership)) = registry
        .catalog_detail(
            &CatalogEntity::Membership {
                project_id: f.projects[0],
                index_id: f.indexes[1],
            },
            Some(policy_revision(f)?),
        )?
        .item
    else {
        anyhow::bail!("existing B membership absent");
    };
    let Some(CatalogItem::Project(project)) = registry
        .catalog_detail(
            &CatalogEntity::Project { id: f.projects[2] },
            Some(policy_revision(f)?),
        )?
        .item
    else {
        anyhow::bail!("affected Project absent");
    };
    drop(registry);
    membership.project_id = f.projects[2];
    membership.revision = CatalogRevision::INITIAL;
    union_apply(
        f,
        220,
        CatalogOperation::SetMembership {
            membership,
            expected_scope_revision: project.scope_revision,
            expected_membership_revision: None,
        },
    )
    .await?;
    union_apply(
        f,
        221,
        CatalogOperation::SetProjectDefaults {
            project_id: project.id,
            expected_defaults_revision: project.defaults_revision,
            index_ids: vec![f.indexes[1], f.indexes[2]],
        },
    )
    .await?;
    let response = airk_cli::service_runtime::request(
        &f.control,
        ControlMethod::CatalogGrantDraft {
            context_key: f.session.context_key,
            selection: GrantDraftSelection {
                key: policy_key(f, 222).await?,
                expected_catalog_revision: policy_revision(f)?,
                project_ids: vec![project.id],
                index_ids: None,
            },
        },
    )
    .await?;
    let ControlResponse::CatalogGrantDraft { draft } = response else {
        anyhow::bail!(
            "default union grant review refused: {}",
            serde_json::to_string(&response)?
        );
    };
    let CatalogOperation::CreateGrant { grant } = &draft.request.operation else {
        anyhow::bail!("default union grant review changed operation");
    };
    let grant = grant.clone();
    let selected: std::collections::BTreeSet<_> = grant
        .memberships
        .iter()
        .map(|member| member.membership.index_id)
        .collect();
    ensure!(
        selected == [f.indexes[1], f.indexes[2]].into_iter().collect(),
        "default grant broadened or omitted a member: {grant:?}"
    );
    let response = airk_cli::service_runtime::request(
        &f.control,
        ControlMethod::CatalogApply {
            request: draft.request,
            preview_digest: draft.preview_digest,
        },
    )
    .await?;
    let ControlResponse::CatalogApplied { receipt, .. } = response else {
        anyhow::bail!(
            "default union grant apply refused: {}",
            serde_json::to_string(&response)?
        );
    };
    f.session.project_ids = vec![project.id];
    f.session.index_ids = None;
    f.session.grant_id = grant.id;
    f.session.expected_grant_revision = grant.revision;
    f.session.expected_catalog_revision = receipt.catalog_revision;
    Ok(())
}

fn union_member(value: &Value, index: IndexId) -> Result<&Value> {
    value["members"]
        .as_array()
        .context("union members absent")?
        .iter()
        .find(|member| member["index_id"] == json!(index))
        .context("selected union member omitted")
}

fn union_healthy_identity(value: &Value, index: IndexId) -> Result<Value> {
    let member = union_member(value, index)?;
    ensure!(
        member["outcome"] == "complete" && member["is_error"] == false,
        "healthy member did not complete: {value}"
    );
    let materialized = member_body(value, member)?;
    let hits = materialized["hits"]
        .as_array()
        .context("healthy hits absent")?;
    ensure!(
        !hits.is_empty(),
        "healthy union returned no evidence: {value}"
    );
    let publications = value["publication_vector"]["members"]
        .as_array()
        .context("union publication inventory absent")?;
    let publication = publications
        .iter()
        .find(|entry| entry["index_id"] == json!(index))
        .context("healthy publication omitted")?;
    ensure!(
        publication["publication"]["token"].is_string(),
        "healthy publication token absent"
    );
    // Verification receipts are dated observations. Retained hits and the exact
    // publication are literal across modes; full replies are compared separately.
    Ok(json!({"token":publication["publication"]["token"], "hits":hits}))
}

async fn union_read(
    f: &Fixture,
    mcp: &mut McpSession,
    id: u64,
    mode: &str,
    require_members: bool,
) -> Result<Value> {
    let mut args = json!({"query":"boundaryneedle","mode":"lexical","limit":12,
        "max_bytes":16384,"response_mode":"full","verification":mode});
    // Off forbids all verification budgets in the canonical native catalog.
    // Report/Require retain the originally bounded verification workload.
    if mode != "off" {
        args["verification_max_sources"] = json!(8);
        args["verification_max_bytes"] = json!(32768);
        args["verification_deadline_ms"] = json!(4000);
    }
    if require_members {
        args["member_availability"] = json!("require");
    }
    // This selector enables the public availability input. The combined session
    // still freezes precisely this one Project and leaves index_ids absent.
    args["project_ids"] = json!(f.session.project_ids);
    let backend = airk_cli::combined_frontend::resolve(None, Some(&f.control), &f.session).await?;
    let native = native_read(backend.as_ref(), "airk_search", &args).await;
    backend.close();
    backend.shutdown().await?;
    let native = native?;
    let cli = f.cli("airk_search", &args).await?;
    let wire = mcp.call(id, "airk_search", args)?;
    ensure!(
        wire.get("error").is_none() && wire["result"].is_object(),
        "union MCP transport failed: {wire}"
    );
    for response in [&native, &cli, &wire["result"]] {
        ensure!(
            response.get("isError").is_none_or(Value::is_boolean),
            "malformed union transport error flag: {response}"
        );
    }
    ensure!(
        native.get("isError") == cli.get("isError")
            && cli.get("isError") == wire["result"].get("isError"),
        "union error flags differ: native={native};CLI={cli};MCP={wire}"
    );
    policy_parity("default union native/CLI", &native, &cli)?;
    policy_parity("default union native/MCP", &native, &wire["result"])?;
    let value = body(&native)?;
    let members = value["members"]
        .as_array()
        .with_context(|| format!("union members absent; complete bounded native body: {value}"))?;
    let ids: std::collections::BTreeSet<_> = members
        .iter()
        .map(|member| serde_json::from_value::<IndexId>(member["index_id"].clone()))
        .collect::<std::result::Result<_, _>>()?;
    ensure!(
        members.len() == 2 && ids == [f.indexes[1], f.indexes[2]].into_iter().collect(),
        "default selected union broadened or silently omitted A: {value}"
    );
    Ok(
        json!({"is_error": native.get("isError").cloned().unwrap_or(json!(false)),
        "value":value}),
    )
}

async fn union_loss_observations(
    f: &Fixture,
    mcp: &mut McpSession,
    expected_b: &Value,
    data_lost: bool,
    start_id: u64,
) -> Result<()> {
    for (offset, (mode, require_members)) in [
        ("off", false),
        ("report", false),
        ("require", false),
        ("off", true),
    ]
    .into_iter()
    .enumerate()
    {
        let observed = union_read(f, mcp, start_id + offset as u64, mode, require_members).await?;
        let value = &observed["value"];
        ensure!(
            union_healthy_identity(value, f.indexes[1])? == *expected_b,
            "healthy B publication/hits changed within default union: {value}"
        );
        let a = union_member(value, f.indexes[2])?;
        let a_body = member_body(value, a)?;
        if data_lost {
            ensure!(
                a["outcome"] == "failed" && a["is_error"] == true,
                "missing-storage A was omitted, unqueried or invented as complete: {value}"
            );
            ensure!(
                value["partial"] == true,
                "missing A failure not disclosed: {value}"
            );
            if require_members {
                ensure!(
                    observed["is_error"] == true && value["error"]["code"] == "member_unavailable",
                    "explicit strict member availability hid failure: {value}"
                );
            } else if mode != "require" {
                ensure!(
                    observed["is_error"] == false,
                    "default Report discarded healthy union evidence: {value}"
                );
            }
        } else if mode == "off" {
            ensure!(
                a["outcome"] == "complete" && observed["is_error"] == false,
                "source-only loss should preserve retained evidence with verification Off: {value}"
            );
        } else {
            ensure!(
                a_body["source_verification"].is_object()
                    && a_body["source_verification"]["requirements_satisfied"] != true
                    && a_body["source_verification"]["status"] != "verified",
                "removed ancestor was claimed verified-current or uncertainty omitted: {a_body}"
            );
            if mode == "require" {
                ensure!(
                    observed["is_error"] == true,
                    "Require accepted unavailable A source: {value}"
                );
            } else {
                ensure!(
                    observed["is_error"] == false,
                    "source Report discarded healthy retained evidence: {value}"
                );
            }
        }
    }
    Ok(())
}

async fn deleted_default_union_case(data_lost: bool) -> Result<()> {
    let mut f = Fixture::with_runtime_layout(true, 3, false, Some(data_lost))?;
    let mut service = Some(f.start().await?);
    deletion_default_union(&mut f).await?;
    let frozen_session = serde_json::to_value(&f.session)?;
    let b_source = Path::new(&f.registrations[1].root).join("api/policy.md");
    let b_bytes = fs::read(&b_source)?;
    let mut mcp = McpSession::start(&f)?;
    let before = union_read(&f, &mut mcp, 200, "off", false).await?;
    ensure!(
        before["is_error"] == false,
        "default union baseline failed: {before}"
    );
    let expected_b = union_healthy_identity(&before["value"], f.indexes[1])?;
    ensure!(
        union_member(&before["value"], f.indexes[2])?["outcome"] == "complete",
        "affected A never had real baseline evidence: {before}"
    );
    if data_lost {
        drop(mcp);
        stop_deletion_service(&f, &mut service).await?;
        delete_owned_source_parent(&f, true)?;
    } else {
        delete_owned_source_parent(&f, false)?;
        union_loss_observations(&f, &mut mcp, &expected_b, false, 210).await?;
        drop(mcp);
        stop_deletion_service(&f, &mut service).await?;
    }
    service = Some(f.start().await?);
    let mut mcp = McpSession::start(&f)?;
    union_loss_observations(&f, &mut mcp, &expected_b, data_lost, 220).await?;
    ensure!(
        serde_json::to_value(&f.session)? == frozen_session && f.session.index_ids.is_none(),
        "deletion test changed or narrowed the reviewed default union"
    );
    ensure!(
        policy_revision(&f)? == f.session.expected_catalog_revision,
        "deletion changed catalog authority without explicit review"
    );
    ensure!(
        fs::read(&b_source)? == b_bytes,
        "union read changed healthy source bytes"
    );
    ensure!(
        ServiceRegistry::open_saved(&f.control)?.is_registered(f.registrations[2].binding)?,
        "source absence implicitly retired A"
    );
    retire_default_union_and_review_same_project(&mut f, &mut mcp, &expected_b).await?;
    drop(mcp);
    stop_deletion_service(&f, &mut service).await?;
    Ok(())
}

#[tokio::test]
async fn post_p14_default_project_union_preserves_healthy_member_after_source_ancestor_loss()
-> Result<()> {
    deleted_default_union_case(false).await
}

#[tokio::test]
async fn post_p14_default_project_union_preserves_healthy_member_after_source_and_storage_loss()
-> Result<()> {
    deleted_default_union_case(true).await
}

fn retired_union_refusal(reply: &Value, session: &CombinedConnectRequest) -> Result<()> {
    ensure!(
        reply["isError"] == true,
        "old default union unexpectedly succeeded: {reply}"
    );
    let value = body(reply)?;
    ensure!(
        matches!(
            value["error"]["code"].as_str(),
            Some("combined_scope_unavailable" | "combined_scope_expired")
        ) && value["error"]["retryable"] == false
            && value["error"]["query_replay_safe"] == false
            && value["requested_scope"]["confirmed"] == false
            && value["requested_scope"]["selection"] == serde_json::to_value(session)?,
        "old default union failed for an unexpected reason: {value}"
    );
    Ok(())
}

async fn retire_default_union_and_review_same_project(
    f: &mut Fixture,
    old_mcp: &mut McpSession,
    expected_b: &Value,
) -> Result<()> {
    use airk_core::catalog_management::{CatalogEntity, CatalogItem};
    let old_session = f.session.clone();
    let old_native =
        airk_cli::combined_frontend::resolve(None, Some(&f.control), &old_session).await?;
    let mut registry = ServiceRegistry::open_saved(&f.control)?;
    let frozen = registry
        .catalog_detail(
            &CatalogEntity::Grant {
                id: old_session.grant_id,
            },
            Some(policy_revision(f)?),
        )?
        .item
        .context("original grant history absent")?;
    let project_before = registry
        .catalog_detail(
            &CatalogEntity::Project { id: f.projects[2] },
            Some(policy_revision(f)?),
        )?
        .item
        .context("default Project absent")?;
    drop(registry);
    let CatalogItem::Project(project_before) = project_before else {
        anyhow::bail!("default Project kind changed");
    };
    let b_path = Path::new(&f.registrations[1].root).join("api/policy.md");
    let b_source = fs::read(&b_path)?;
    let retirement = policy_draft(f, 223, SourceMutationSelection::RetireIndex).await?;
    let expected_scope_revision = project_before.scope_revision.next()?;
    let expected_defaults_revision = project_before.defaults_revision.next()?;
    ensure!(retirement.preview.before.iter().any(|item|matches!(item,CatalogItem::Membership(value) if value.project_id==f.projects[2] && value.index_id==f.indexes[2])),
        "Retire review omitted the current A membership");
    ensure!(
        retirement
            .preview
            .after
            .iter()
            .any(|item| matches!(item,CatalogItem::Project(value)
        if value.id==f.projects[2] && value.default_indexes==vec![f.indexes[1]]
        && value.scope_revision==expected_scope_revision
        && value.defaults_revision==expected_defaults_revision)),
        "Retire review omitted the resulting same-Project default scope"
    );
    apply_policy(f, &retirement).await?;
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        let registry = ServiceRegistry::open_saved(&f.control)?;
        if registry.project_was_removed(f.registrations[2].binding)? {
            break;
        }
        ensure!(
            Instant::now() < deadline,
            "default-union A physical cleanup did not settle"
        );
        drop(registry);
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    // A restrictive catalog change expires the OLD exact grant/selection. Do not
    // silently shrink it, reuse its transport, or convert this into a healthy read.
    let args = deletion_query();
    let native = old_native
        .call("airk_search".into(), args.as_object().unwrap().clone())
        .await?;
    retired_union_refusal(&native, &old_session)?;
    let wire = old_mcp.call(240, "airk_search", args.clone())?;
    ensure!(
        wire.get("error").is_none() && wire["result"].is_object(),
        "old MCP scope transport failed unexpectedly: {wire}"
    );
    retired_union_refusal(&wire["result"], &old_session)?;
    old_native.close();
    old_native.shutdown().await?;
    // A fresh CLI cannot even construct the old scoped backend. Its documented
    // setup refusal is a terminal code on stderr, not a reduced evidence reply.
    let mut command = Command::new(&f.candidate);
    command
        .arg("--combined-session")
        .arg(serde_json::to_string(&old_session)?)
        .arg("--service-runtime-dir")
        .arg(&f.control)
        .args(["tools", "call", "airk_search", "--args"])
        .arg(args.to_string());
    let output = tokio::task::spawn_blocking(move || {
        read_retry::once_until(&command, Instant::now() + Duration::from_secs(60))
    })
    .await??;
    ensure!(
        output.status.code() == Some(1)
            && output.stdout.is_empty()
            && String::from_utf8_lossy(&output.stderr).contains("scoped_read_unavailable:"),
        "old CLI scope failed unexpectedly: exit={:?};stdout={};stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let mut registry = ServiceRegistry::open_saved(&f.control)?;
    ensure!(
        registry
            .catalog_detail(
                &CatalogEntity::Grant {
                    id: old_session.grant_id
                },
                None
            )?
            .item
            == Some(frozen),
        "Retire/physical cleanup rewrote immutable old grant history"
    );
    let detail = registry.catalog_detail(&CatalogEntity::Project { id: f.projects[2] }, None)?;
    ensure!(
        matches!(detail.item,Some(CatalogItem::Project(value)) if value.default_indexes==vec![f.indexes[1]]),
        "A survived in current same-Project defaults after cleanup"
    );
    ensure!(
        registry
            .catalog_detail(
                &CatalogEntity::Membership {
                    project_id: f.projects[2],
                    index_id: f.indexes[2]
                },
                None
            )?
            .item
            .is_none(),
        "A survived in current Project membership after cleanup"
    );
    drop(registry);
    // Explicitly review a NEW grant using the SAME default Project selector.
    // No index_ids=B convenience subset or implicit grant rewrite is permitted.
    let response = airk_cli::service_runtime::request(
        &f.control,
        ControlMethod::CatalogGrantDraft {
            context_key: old_session.context_key,
            selection: GrantDraftSelection {
                key: policy_key(f, 224).await?,
                expected_catalog_revision: policy_revision(f)?,
                project_ids: old_session.project_ids.clone(),
                index_ids: None,
            },
        },
    )
    .await?;
    let ControlResponse::CatalogGrantDraft { draft } = response else {
        anyhow::bail!(
            "fresh same-default Project review refused: {}",
            serde_json::to_string(&response)?
        );
    };
    let CatalogOperation::CreateGrant { grant } = &draft.request.operation else {
        anyhow::bail!("fresh default grant operation changed");
    };
    let grant = grant.clone();
    ensure!(
        grant.memberships.len() == 1
            && grant.memberships[0].membership.index_id == f.indexes[1]
            && grant.memberships[0].membership.project_id == f.projects[2],
        "fresh default review retained A or lost B: {grant:?}"
    );
    let response = airk_cli::service_runtime::request(
        &f.control,
        ControlMethod::CatalogApply {
            request: draft.request,
            preview_digest: draft.preview_digest,
        },
    )
    .await?;
    let ControlResponse::CatalogApplied { receipt, .. } = response else {
        anyhow::bail!(
            "fresh same-default grant apply refused: {}",
            serde_json::to_string(&response)?
        );
    };
    f.session.grant_id = grant.id;
    f.session.expected_grant_revision = grant.revision;
    f.session.expected_catalog_revision = receipt.catalog_revision;
    ensure!(
        f.session.project_ids == old_session.project_ids && f.session.index_ids.is_none(),
        "cleanup test narrowed the default Project selection"
    );
    let backend = airk_cli::combined_frontend::resolve(None, Some(&f.control), &f.session).await?;
    let native = native_read(backend.as_ref(), "airk_search", &args).await;
    backend.close();
    backend.shutdown().await?;
    let native = native?;
    let cli = f.cli("airk_search", &args).await?;
    let mut fresh = McpSession::start(f)?;
    let wire = fresh.call(241, "airk_search", args)?;
    ensure!(
        wire.get("error").is_none() && wire["result"].is_object(),
        "fresh default MCP transport failed: {wire}"
    );
    for value in [&native, &cli, &wire["result"]] {
        ensure!(
            value
                .get("isError")
                .is_none_or(|flag| flag == &json!(false)),
            "fresh default evidence failed: {value}"
        );
    }
    policy_parity(
        "fresh same-default Project native/CLI after A cleanup",
        &native,
        &cli,
    )?;
    policy_parity(
        "fresh same-default Project native/MCP after A cleanup",
        &native,
        &wire["result"],
    )?;
    let value = body(&native)?;
    let members = value["members"]
        .as_array()
        .context("fresh default member inventory absent")?;
    ensure!(
        members.len() == 1 && members[0]["index_id"] == json!(f.indexes[1]),
        "fresh default Project retained A: {value}"
    );
    ensure!(
        union_healthy_identity(&value, f.indexes[1])? == *expected_b,
        "A cleanup changed B publication/hits: {value}"
    );
    ensure!(
        fs::read(b_path)? == b_source,
        "A retirement/cleanup changed healthy B source bytes"
    );
    Ok(())
}
