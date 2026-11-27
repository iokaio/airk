// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::{
    identity::{Incarnation, Sha256Digest},
    service::{PROTOCOL_VERSION, WorkerIdentity},
};
use airk_mcp::local::{NativeRead, SettingsView};
use anyhow::Context;

fn identity() -> WorkerIdentity {
    WorkerIdentity {
        service: Incarnation::from_bytes([1; 16]),
        worker: Incarnation::from_bytes([2; 16]),
        project: Sha256Digest::from_bytes([3; 32]),
        fencing_generation: 1,
    }
}
fn grant() -> ConnectionGrant {
    ConnectionGrant {
        protocol: PROTOCOL_VERSION,
        identity: identity(),
        caller: Incarnation::from_bytes([4; 16]),
        token: Sha256Digest::from_bytes([5; 32]),
        ttl_ms: 1000,
        client: airk_core::usage::ClientFamily::Unknown,
        scope: None,
    }
}
fn query(native: Option<NativeRead>, tool: &str) -> protocol::Query {
    protocol::Query {
        context_partition: None,
        answer_partition: None,
        read_attempt: None,
        frontend: None,
        tool: tool.into(),
        arguments: Default::default(),
        native,
        status_receipt: Default::default(),
    }
}

#[test]
fn administrative_purpose_accepts_only_native_reads_without_status_slot_camouflage() -> Result<()> {
    let role = SessionPurpose::NativeAdministrativeRead;
    validate_request(role, &query(Some(NativeRead::UsagePolicy), ""))?;
    assert!(validate_request(role, &query(None, "airk_search")).is_err());
    assert!(validate_request(role, &query(None, "airk_status")).is_err());
    assert!(validate_request(role, &query(Some(NativeRead::UsagePolicy), "airk_status")).is_err());
    let mut mixed = query(Some(NativeRead::UsagePolicy), "");
    mixed
        .arguments
        .insert("query".into(), serde_json::json!("private evidence"));
    assert!(validate_request(role, &mixed).is_err());
    Ok(())
}

#[test]
fn combined_context_partition_is_refused_before_any_backend_admission() -> Result<()> {
    use airk_core::{
        arbiter::{Budget, answer_loop::BudgetV3},
        combined_context::{initial_allocations, initial_allocations_v3},
        index_catalog::IndexId,
    };
    let mut request = query(None, "airk_answer");
    validate_request(SessionPurpose::Evidence, &request)?;
    let member = [IndexId::from_identity(Incarnation::from_bytes([1; 16]))];
    // An Answer /2 member allowance is retired: combined Answer runs /3.
    let mut retired = request.clone();
    retired.context_partition = initial_allocations(
        Incarnation::from_bytes([9; 16]),
        Sha256Digest::from_bytes([8; 32]),
        &member,
        &Budget::default(),
    )?
    .remove(0);
    assert!(validate_request(SessionPurpose::Evidence, &retired).is_err());
    request.answer_partition = initial_allocations_v3(
        Incarnation::from_bytes([9; 16]),
        Sha256Digest::from_bytes([8; 32]),
        &member,
        &BudgetV3::default(),
    )?
    .remove(0);
    let mut backend_entries = 0;
    for purpose in [
        SessionPurpose::Evidence,
        SessionPurpose::NativeAdministrativeRead,
    ] {
        let outcome = validate_request(purpose, &request);
        if outcome.is_ok() {
            backend_entries += 1;
        }
        assert_eq!(
            outcome.unwrap_err().to_string(),
            match purpose {
                SessionPurpose::Evidence => "context partition original attempt missing",
                _ => "an Answer /3 partition requires scoped canonical Answer",
            }
        );
    }
    assert_eq!(backend_entries, 0);
    Ok(())
}

#[test]
fn combined_context_partition_admission_binds_attempt_index_and_actual_nested_budget() -> Result<()>
{
    use airk_core::{
        arbiter::answer_loop::{BudgetV3, ContextRequestV3, REQUEST_SCHEMA_V3},
        combined_context::initial_allocations_v3,
        index_catalog::{CatalogRevision, IndexId},
        read_attempt::{ReadAttemptBudget, ReadClock},
        service::ScopedReadRef,
    };
    let index = IndexId::from_identity(Incarnation::from_bytes([1; 16]));
    let parent = BudgetV3::default();
    let allowance = initial_allocations_v3(
        Incarnation::from_bytes([9; 16]),
        Sha256Digest::from_bytes([8; 32]),
        &[index],
        &parent,
    )?
    .remove(0)
    .unwrap();
    let mut request = query(None, "airk_answer");
    request.arguments = serde_json::to_value(ContextRequestV3 {
        needs: Vec::new(),
        anchors: Vec::new(),
        deliverable: None,
        known: Vec::new(),
        schema: REQUEST_SCHEMA_V3.into(),
        question: "original question".into(),
        budget: allowance.clamp(&parent)?,
        continuation: None,
    })?
    .as_object()
    .unwrap()
    .clone();
    request.answer_partition = Some(allowance);
    request.read_attempt = Some(ReadAttemptBudget {
        schema: 1,
        request_id: Incarnation::from_bytes([9; 16]),
        clock: ReadClock::UnixMonotonic,
        expires_monotonic_ns: 3_000_000_000,
        remaining_ns: 2_000_000_000,
        native_retry_expires_monotonic_ns: None,
    });
    let authority = ScopedReadRef {
        schema: 1,
        lease_generation: 1,
        index_id: index,
        selection: airk_core::scoped_reads::SelectionScopeRef {
            owner: airk_core::scoped_reads::SelectionOwnerKind::Agent,
            grant_nonce: Incarnation::from_bytes([4; 16]),
            service_instance: Incarnation::from_bytes([1; 16]),
            scope_identity: Sha256Digest::from_bytes([6; 32]),
            catalog_revision: CatalogRevision::INITIAL,
        },
        fingerprint: Sha256Digest::from_bytes([5; 32]),
    };
    validate_request(SessionPurpose::Evidence, &request)?;
    validate_context_partition(&request, Some(authority))?;
    assert!(validate_context_partition(&request, None).is_err());
    let mut foreign = authority;
    foreign.index_id = IndexId::from_identity(Incarnation::from_bytes([2; 16]));
    assert!(validate_context_partition(&request, Some(foreign)).is_err());
    let mut wrong = request.clone();
    wrong.read_attempt.as_mut().unwrap().request_id = Incarnation::from_bytes([7; 16]);
    assert!(validate_request(SessionPurpose::Evidence, &wrong).is_err());
    let mut over = request.clone();
    over.answer_partition.as_mut().unwrap().resources.calls = 1;
    assert!(validate_request(SessionPurpose::Evidence, &over).is_err());
    let mut wrong_tool = request;
    wrong_tool.tool = "airk_search".into();
    assert!(validate_request(SessionPurpose::Evidence, &wrong_tool).is_err());
    Ok(())
}

#[test]
fn ordinary_live_scope_none_remains_refused_and_no_wire_role_can_authorize_it() -> Result<()> {
    let mut scopes = scopes::Scopes::new(identity(), true);
    assert!(redeem_scope(&mut scopes, &grant(), SessionPurpose::Evidence).is_err());
    assert!(check_scope(&scopes, None, SessionPurpose::Evidence).is_err());
    assert!(
        redeem_scope(
            &mut scopes,
            &grant(),
            SessionPurpose::NativeAdministrativeRead
        )?
        .is_none()
    );
    check_scope(&scopes, None, SessionPurpose::NativeAdministrativeRead)?;
    let mut wire = serde_json::to_value(grant())?;
    wire["purpose"] = serde_json::json!("native_administrative_read");
    assert!(serde_json::from_value::<ConnectionGrant>(wire).is_err());
    Ok(())
}

#[tokio::test]
async fn administrative_native_reads_retain_actual_complete_handler_pages() -> Result<()> {
    let root = tempfile::tempdir()?;
    let base = root.path().canonicalize()?;
    let source = base.join("source");
    std::fs::create_dir(&source)?;
    let kernel = airk_kernel::Kernel::open(&source, Some(base.join("data")))
        .context("admin fixture: open private native Kernel")?;
    // This fresh data root has no configured model. Native initialize creates
    // the canonical empty publication without scanning any source directory;
    // configuring roots is not a substitute for that storage lifecycle step.
    assert!(!kernel.data_dir().join("models/local").exists());
    let initialized = kernel
        .initialize()
        .context("admin fixture: initialize empty native publication before configuring roots")?;
    assert_eq!(initialized.files, 0);
    assert!(initialized.publication.is_some());
    // Real native Roots metadata exceeds the minimum supported page budget.
    // Build sibling sources outside the primary/data roots, without files or
    // an index scan. Unix backslashes are ordinary filename bytes but escape
    // in JSON, retaining the oversized metadata below macOS PATH_MAX. Windows
    // uses canonical extended paths to exercise its long-path support.
    let (depths, padding) = if cfg!(windows) { (8, "x") } else { (4, "\\") };
    let mut external = base.join("external");
    for depth in 0..depths {
        external = external.join(format!("segment-{depth}-{}", padding.repeat(170)));
    }
    for n in 0..16 {
        let path = external.join(format!("root-{n:02}"));
        std::fs::create_dir_all(&path)?;
        kernel
            .add_root(&format!("metadata-source-{n:02}"), &path)
            .with_context(|| format!("admin fixture: configure native source root {n}"))?;
    }
    let project = airk_core::service::ProjectRegistration::new(
        kernel.root().to_str().unwrap().into(),
        kernel.data_dir().to_str().unwrap().into(),
        kernel.collection().into(),
    )?
    .binding;
    let input_path = root.path().join("quota-input.json");
    std::fs::write(
        &input_path,
        serde_json::to_vec(
            &kernel
                .quota_policy()
                .context("admin fixture: read quota input policy")?,
        )?,
    )?;
    let input = airk_host::service_configuration::input(&input_path, 16384)
        .context("admin fixture: capture native settings input")?;
    let roots_document = serde_json::json!({"project":project,"value":kernel.roots().context("admin fixture: capture complete expected Roots body")?});
    assert!(serde_json::to_vec(&roots_document)?.len() > 20 << 10);
    let mut tools = airk_mcp::local::LocalTools::new(kernel)
        .await
        .context("admin fixture: start canonical LocalTools")?;
    for mut native in [
        NativeRead::Settings {
            view: SettingsView::Roots,
            cursor: None,
            max_bytes: 20 << 10,
        },
        NativeRead::Settings {
            view: SettingsView::QuotaStatus,
            cursor: None,
            max_bytes: 20 << 10,
        },
        NativeRead::UsagePolicy,
        NativeRead::UsageReport {
            days: 1,
            cursor: None,
            max_bytes: 20 << 10,
            format: Default::default(),
            price: None,
        },
        NativeRead::ValidateSettings {
            view: SettingsView::QuotaPolicy,
            input: input.clone(),
            cursor: None,
            max_bytes: 20 << 10,
        },
    ] {
        let roots = matches!(
            &native,
            NativeRead::Settings {
                view: SettingsView::Roots,
                ..
            }
        );
        let mut assembled = serde_json::Value::Null;
        let mut pages = 0;
        let mut expected_digest = None;
        let family = match &native {
            NativeRead::Settings {
                view: SettingsView::Roots,
                ..
            } => "native Roots",
            NativeRead::Settings {
                view: SettingsView::QuotaStatus,
                ..
            } => "native QuotaStatus",
            NativeRead::UsagePolicy => "native UsagePolicy",
            NativeRead::UsageReport { .. } => "native UsageReport",
            NativeRead::ValidateSettings { .. } => "native ValidateSettings",
            _ => "native administrative read",
        };
        loop {
            let request = query(Some(native.clone()), "");
            validate_request(SessionPurpose::NativeAdministrativeRead, &request)?;
            let expected = tools
                .read_native(native.clone(), project)
                .await
                .with_context(|| format!("admin fixture: {family} page {pages}"))?;
            // The actual worker's normal QueryResult path is unchanged; this frame
            // retains the complete canonical NativeRead value, not a report subset.
            let reply = protocol::QueryResult {
                read_admission: None,
                result: expected.clone(),
                status_receipt: tools.status_receipt(),
                rebind: false,
            };
            let mut frame = Vec::new();
            crate::service_ipc::write_frame(&mut frame, &reply)
                .context("admin fixture: serialize complete worker result frame")?;
            let decoded: protocol::QueryResult =
                crate::service_ipc::read_frame(&mut frame.as_slice())
                    .context("admin fixture: decode complete worker result frame")?
                    .unwrap();
            assert_eq!(decoded.result, expected);
            if matches!(&native, NativeRead::UsagePolicy) {
                break;
            }
            let page: airk_kernel::usage_page::Page = serde_json::from_value(decoded.result)?;
            if let Some(digest) = &expected_digest {
                assert_eq!(digest, &page.digest);
            } else {
                expected_digest = Some(page.digest.clone());
            }
            for record in page.items {
                airk_kernel::usage_page::assemble(&mut assembled, record)?;
            }
            pages += 1;
            if let Some(cursor) = page.cursor {
                match &mut native {
                    NativeRead::Settings { cursor: next, .. }
                    | NativeRead::UsageReport { cursor: next, .. }
                    | NativeRead::ValidateSettings { cursor: next, .. } => *next = Some(cursor),
                    _ => anyhow::bail!("unexpected paged native read"),
                }
            } else {
                break;
            }
        }
        if roots {
            assert!(
                pages > 1,
                "real native Roots body did not exercise continuation"
            );
            assert_eq!(assembled, roots_document);
        }
    }
    tools.close().await?;
    Ok(())
}

#[test]
fn private_admin_grant_request_retains_exact_worker_and_nonce() -> Result<()> {
    let nonce = Incarnation::from_bytes([6; 16]);
    let command = protocol::Command::GrantAdmin {
        identity: identity(),
        caller: grant().caller,
        nonce,
    };
    let mut frame = Vec::new();
    crate::service_ipc::write_frame(&mut frame, &command)?;
    let parsed: protocol::Command = crate::service_ipc::read_frame(&mut frame.as_slice())?.unwrap();
    assert!(
        matches!(parsed,protocol::Command::GrantAdmin {identity:worker,nonce:observed,..} if worker==identity() && observed==nonce)
    );
    Ok(())
}

fn focus_query() -> Result<(protocol::Query, airk_core::service::ScopedReadRef)> {
    use airk_core::{
        arbiter::Budget,
        combined_context::{ALLOWANCE_SCHEMA, PartitionAllowance, Resources},
        index_catalog::{CatalogRevision, IndexId},
        scoped_reads::{SelectionOwnerKind, SelectionScopeRef},
        service::ScopedReadRef,
    };
    let attempt_context = airk_mcp::read_attempt::ReadAttemptContext::new(
        std::time::Instant::now() + std::time::Duration::from_secs(30),
        Default::default(),
    )
    .with_request_id(Incarnation::from_bytes([91; 16]))?;
    let attempt =
        airk_host::read_attempt::encode(&attempt_context, attempt_context.request_id().unwrap())?;
    let index = IndexId::from_identity(Incarnation::from_bytes([92; 16]));
    let selection = SelectionScopeRef {
        owner: SelectionOwnerKind::Admin,
        grant_nonce: Incarnation::from_bytes([93; 16]),
        service_instance: identity().service,
        scope_identity: Sha256Digest::from_bytes([94; 32]),
        catalog_revision: CatalogRevision::INITIAL,
    };
    let budget = Budget {
        calls: 1,
        rounds: 1,
        fanout: 1,
        input_tokens: 2048,
        candidates: 4,
        max_bytes: 8192,
        deadline_ms: 1000,
    };
    let allowance = PartitionAllowance {
        schema: ALLOWANCE_SCHEMA.into(),
        attempt: attempt.request_id,
        selection: selection.scope_identity,
        index_id: index,
        resources: Resources::budget(&budget),
        parent_budget: budget,
        assessment_ordinals: vec![],
    };
    let mut request = query(None, "airk_search");
    request.read_attempt = Some(attempt);
    request.context_partition = Some(allowance);
    request.arguments=serde_json::json!({"query":"original singleton source question","limit":16,"max_bytes":8192}).as_object().unwrap().clone();
    let authority = ScopedReadRef {
        schema: 1,
        lease_generation: 1,
        index_id: index,
        selection,
        fingerprint: Sha256Digest::from_bytes([95; 32]),
    };
    Ok((request, authority))
}
#[test]
fn actual_singleton_focus_worker_admission_requires_evidence_search_and_original_attempt()
-> Result<()> {
    let (request, authority) = focus_query()?;
    validate_request(SessionPurpose::Evidence, &request)?;
    validate_context_partition(&request, Some(authority))?;
    for change in 0..5 {
        let mut bad = request.clone();
        match change {
            0 => bad.tool = "airk_answer".into(),
            1 => bad.native = Some(NativeRead::UsagePolicy),
            2 => bad.read_attempt = None,
            3 => bad.read_attempt.as_mut().unwrap().request_id = Incarnation::from_bytes([96; 16]),
            _ => {
                bad.arguments.insert("limit".into(), 1.into());
            }
        }
        assert!(
            validate_request(SessionPurpose::Evidence, &bad).is_err(),
            "variant {change} entered backend"
        );
    }
    assert!(validate_request(SessionPurpose::NativeAdministrativeRead, &request).is_err());
    Ok(())
}
#[test]
fn actual_singleton_focus_worker_admission_binds_installed_selection_index_and_lease() -> Result<()>
{
    let (request, authority) = focus_query()?;
    assert!(validate_context_partition(&request, None).is_err());
    for change in 0..4 {
        let mut bad = authority;
        match change {
            0 => {
                bad.index_id = airk_core::index_catalog::IndexId::from_identity(
                    Incarnation::from_bytes([96; 16]),
                )
            }
            1 => bad.selection.scope_identity = Sha256Digest::from_bytes([96; 32]),
            2 => bad.lease_generation = 0,
            _ => bad.schema = 2,
        }
        assert!(
            validate_context_partition(&request, Some(bad)).is_err(),
            "variant {change} entered backend"
        );
    }
    Ok(())
}
#[test]
fn actual_singleton_focus_worker_refuses_foreign_clock_and_expired_attempt_before_storage()
-> Result<()> {
    let (request, _) = focus_query()?;
    for change in 0..4 {
        let mut bad = request.clone();
        let attempt = bad.read_attempt.as_mut().unwrap();
        match change {
            0 => {
                attempt.clock = match attempt.clock {
                    airk_core::read_attempt::ReadClock::UnixMonotonic => {
                        airk_core::read_attempt::ReadClock::WindowsPerformanceCounter
                    }
                    _ => airk_core::read_attempt::ReadClock::UnixMonotonic,
                }
            }
            1 => attempt.schema = 2,
            2 => attempt.expires_monotonic_ns = 0,
            _ => attempt.remaining_ns = 0,
        };
        assert!(
            validate_request(SessionPurpose::Evidence, &bad).is_err(),
            "variant {change} entered storage"
        );
    }
    Ok(())
}
#[test]
fn malformed_focus_allowances_never_fall_back_to_context_or_unallocated_search() -> Result<()> {
    let (request, _) = focus_query()?;
    for change in 0..8 {
        let mut bad = request.clone();
        let allocation = bad.context_partition.as_mut().unwrap();
        match change {
            0 => allocation.resources.input_tokens = 2049,
            1 => allocation.resources.calls = 2,
            2 => allocation.parent_budget.rounds = 2,
            3 => allocation.parent_budget.fanout = 2,
            4 => allocation.resources.bytes = 65537,
            5 => allocation.resources.candidates = 65,
            6 => allocation.assessment_ordinals.push(1),
            _ => allocation.schema = "foreign".into(),
        };
        assert!(
            validate_request(SessionPurpose::Evidence, &bad).is_err(),
            "variant {change} entered backend"
        );
    }
    Ok(())
}

#[test]
fn actual_rust_emitted_focus_and_context_allowances_cannot_cross_native_worker_tool_purposes()
-> Result<()> {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../fixtures/split/arbiter/focus_only_allowance_cases.json"
    ))?;
    assert_eq!(fixture["schema"], "airk.focus-only-allowance-cases/1");
    assert_eq!(fixture["qualification"], false);
    let (base, mut authority) = focus_query()?;
    for (case, tool, allowed) in [
        ("ordinary_focus", "airk_search", true),
        ("ordinary_focus", "airk_answer", false),
        ("context", "airk_search", false),
        ("zero_optional_work", "airk_search", false),
    ] {
        let mut request = base.clone();
        let mut allocation: airk_core::combined_context::PartitionAllowance =
            serde_json::from_value(fixture[case]["allowance"].clone())?;
        allocation.attempt = request.read_attempt.unwrap().request_id;
        authority.index_id = allocation.index_id;
        authority.selection.scope_identity = allocation.selection;
        request.context_partition = Some(allocation);
        request.tool = tool.into();
        assert_eq!(
            validate_request(SessionPurpose::Evidence, &request).is_ok(),
            allowed,
            "case {case}/{tool}"
        );
        if allowed {
            validate_context_partition(&request, Some(authority))?;
        }
    }
    Ok(())
}
