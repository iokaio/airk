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
        arbiter::Budget, combined_context::initial_allocations, index_catalog::IndexId,
    };
    let mut request = query(None, "airk_answer");
    validate_request(SessionPurpose::Evidence, &request)?;
    request.context_partition = initial_allocations(
        Incarnation::from_bytes([9; 16]),
        Sha256Digest::from_bytes([8; 32]),
        &[IndexId::from_identity(Incarnation::from_bytes([1; 16]))],
        &Budget::default(),
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
                _ => "context partition requires scoped canonical Answer",
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
        arbiter::{
            Budget,
            context_action::{ContextRequestV2, REQUEST_SCHEMA_V2},
        },
        combined_context::initial_allocations,
        index_catalog::{CatalogRevision, GrantId, IndexId},
        read_attempt::{ReadAttemptBudget, ReadClock},
        service::ScopedReadRef,
    };
    let index = IndexId::from_identity(Incarnation::from_bytes([1; 16]));
    let budget = Budget::default();
    let mut request = query(None, "airk_answer");
    request.arguments = serde_json::to_value(ContextRequestV2 {
        schema: REQUEST_SCHEMA_V2.into(),
        question: "original question".into(),
        budget: budget.clone(),
    })?
    .as_object()
    .unwrap()
    .clone();
    request.context_partition = initial_allocations(
        Incarnation::from_bytes([9; 16]),
        Sha256Digest::from_bytes([8; 32]),
        &[index],
        &budget,
    )?
    .remove(0);
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
        grant_id: Some(GrantId::from_identity(Incarnation::from_bytes([4; 16]))),
        grant_revision: Some(CatalogRevision::INITIAL),
        selection: None,
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
    over.context_partition.as_mut().unwrap().resources.calls = 1;
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
