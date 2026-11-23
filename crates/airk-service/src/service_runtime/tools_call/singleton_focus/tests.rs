// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::{
    identity::Incarnation, index_catalog::CatalogRevision, service::WindowScopeSelection,
};
fn limits() -> FocusProfileLimits {
    FocusProfileLimits {
        max_candidates: 64,
        pair_input_tokens: 512,
        max_input_tokens: 2048,
    }
}
fn parent() -> Result<ReadAttemptContext> {
    let context = ReadAttemptContext::new(
        std::time::Instant::now() + std::time::Duration::from_secs(90),
        Default::default(),
    )
    .with_request_id(Incarnation::from_bytes([11; 16]))?;
    let attempt = airk_host::read_attempt::encode(&context, context.request_id().unwrap())?;
    context.capture_transport_attempt(attempt)
}
fn indexes(count: u8) -> Vec<IndexId> {
    (1..=count)
        .map(|n| IndexId::from_identity(Incarnation::from_bytes([n; 16])))
        .collect()
}
fn selection() -> Sha256Digest {
    Sha256Digest::from_bytes([13; 32])
}
fn arguments() -> Map<String, Value> {
    serde_json::json!({"query":"original question","limit":16,"max_bytes":8192})
        .as_object()
        .unwrap()
        .clone()
}
#[test]
fn actual_singleton_focus_capture_preserves_original_attempt_expiry_and_shared_cancellation()
-> Result<()> {
    let parent = parent()?;
    let original = parent.captured_transport_attempt().unwrap();
    let captured = capture(
        parent.clone(),
        selection(),
        indexes(1)[0],
        16,
        8192,
        &limits(),
    )?;
    let wire = captured
        .captured_transport_attempt()
        .context("capture absent")?;
    assert_eq!(wire.request_id, original.request_id);
    assert_eq!(wire.clock, original.clock);
    assert_eq!(wire.expires_monotonic_ns, original.expires_monotonic_ns);
    assert!(wire.remaining_ns <= original.remaining_ns);
    assert!(captured.deadline() < parent.deadline());
    let allocation = captured.context_partition().context("allocation absent")?;
    allocation.validate_binding(&wire, selection(), indexes(1)[0])?;
    assert!(allocation.is_ordinary_focus());
    assert!(allocation.assessment_ordinals.is_empty());
    assert_eq!(allocation.parent_budget.calls, 1);
    assert_eq!(allocation.parent_budget.rounds, 1);
    assert_eq!(allocation.parent_budget.fanout, 1);
    assert_eq!(allocation.resources.calls, 1);
    assert_eq!(allocation.resources.input_tokens, 2048);
    assert_eq!(allocation.resources.candidates, 4);
    assert_eq!(allocation.resources.bytes, 8192);
    parent.cancellation().cancel();
    assert!(captured.cancellation().is_cancelled());
    assert!(capture(captured, selection(), indexes(1)[0], 16, 8192, &limits()).is_err());
    Ok(())
}
#[test]
fn actual_resolved_singleton_roster_captures_but_multi_roster_never_infers_a_local_allocation()
-> Result<()> {
    for count in [0, 1, 2, 32] {
        let resolved = WindowScopeSelection {
            namespace: Incarnation::from_bytes([21; 16]),
            expected_catalog_revision: CatalogRevision::INITIAL,
            index_ids: indexes(count),
            project_ids: Vec::new(),
        };
        let mut observed = None;
        let before = arguments();
        let context = admit_resolved(
            "airk_search",
            &before,
            parent()?,
            selection(),
            &resolved.index_ids,
            |index| {
                observed = Some(index);
                Ok(Some(limits()))
            },
        )?;
        assert_eq!(before, arguments());
        assert_eq!(context.context_partition().is_some(), count == 1);
        assert_eq!(
            observed,
            if count == 1 {
                Some(resolved.index_ids[0])
            } else {
                None
            }
        );
    }
    Ok(())
}
#[test]
fn singleton_native_aliases_preserve_inputs_and_explicit_subset_controls_never_allocate()
-> Result<()> {
    for tool in [
        "search",
        "airk_search",
        "answer",
        "airk_answer",
        "airk_file_info",
    ] {
        let context = admit_resolved(
            tool,
            &arguments(),
            parent()?,
            selection(),
            &indexes(1),
            |_| Ok(Some(limits())),
        )?;
        assert_eq!(
            context.context_partition().is_some(),
            matches!(tool, "search" | "airk_search")
        );
    }
    for key in ["project_ids", "index_ids", "member_availability"] {
        let mut args = arguments();
        args.insert(key.into(), Value::Null);
        let context = admit_resolved(
            "airk_search",
            &args,
            parent()?,
            selection(),
            &indexes(1),
            |_| panic!("subset must not request model allowance"),
        )?;
        assert!(context.context_partition().is_none());
    }
    Ok(())
}
#[test]
fn singleton_allocation_respects_actual_frame_allowance_and_refuses_invalid_canonical_arguments()
-> Result<()> {
    for (key, value) in [
        ("limit", 0),
        ("limit", 101),
        ("max_bytes", 0),
        ("max_bytes", 65537),
    ] {
        let mut args = arguments();
        args.insert(key.into(), value.into());
        let context = admit_resolved(
            "airk_search",
            &args,
            parent()?,
            selection(),
            &indexes(1),
            |_| panic!("invalid canonical read must not request model allowance"),
        )?;
        assert!(context.context_partition().is_none());
    }
    for key in [
        "unknown_field",
        "context_partition",
        "singleton",
        "focus_allocation",
    ] {
        let mut malformed = arguments();
        malformed.insert(key.into(), true.into());
        assert!(
            admit_resolved(
                "search",
                &malformed,
                parent()?,
                selection(),
                &indexes(1),
                |_| panic!("public argument cannot create an allocation")
            )?
            .context_partition()
            .is_none()
        );
    }
    for cap in [1, 1023, 1024, 4096, 65536] {
        let context = capture(parent()?, selection(), indexes(1)[0], 16, cap, &limits())?;
        if cap < 1024 {
            assert!(context.context_partition().is_none());
        } else {
            assert_eq!(context.context_partition().unwrap().resources.bytes, cap);
        }
    }
    // Search max_bytes remains TEXT-only, not receipt metadata capacity.
    let mut args = arguments();
    args.insert("max_bytes".into(), 1.into());
    let context = admit_resolved("search", &args, parent()?, selection(), &indexes(1), |_| {
        Ok(Some(limits()))
    })?;
    assert_eq!(
        context.context_partition().unwrap().resources.bytes,
        airk_core::service::MAX_FRAME_BYTES
    );
    Ok(())
}
#[test]
fn singleton_capture_never_replaces_partition_or_refreshes_an_earlier_native_deadline() -> Result<()>
{
    let original = parent()?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
    let context = original.narrowed(deadline);
    let captured = capture(context, selection(), indexes(1)[0], 16, 8192, &limits())?;
    assert!(captured.deadline() <= deadline);
    let allocation = captured.context_partition().unwrap().clone();
    let captured = admit_resolved(
        "search",
        &arguments(),
        captured,
        selection(),
        &indexes(1),
        |_| panic!("existing allocation must not be replaced"),
    )?;
    assert_eq!(
        serde_json::to_value(captured.context_partition())?,
        serde_json::to_value(Some(allocation))?
    );
    assert!(
        capture(
            original.narrowed(std::time::Instant::now()),
            selection(),
            indexes(1)[0],
            16,
            8192,
            &limits()
        )
        .is_err()
    );
    Ok(())
}
#[test]
fn absent_unqualified_or_failed_profile_observation_leaves_complete_ordinary_read_unallocated()
-> Result<()> {
    for fail in [false, true] {
        let context = admit_resolved(
            "search",
            &arguments(),
            parent()?,
            selection(),
            &indexes(1),
            |_| {
                if fail {
                    anyhow::bail!("native profile observation unavailable")
                } else {
                    Ok(None)
                }
            },
        )?;
        assert!(context.context_partition().is_none());
        assert_eq!(
            context.captured_transport_attempt().unwrap().request_id,
            Incarnation::from_bytes([11; 16])
        );
    }
    Ok(())
}

#[test]
fn actual_focus_callbacks_and_singleton_tools_call_cannot_enter_prebaseline_metadata_relay()
-> Result<()> {
    use airk_core::service::ControlMethod;
    let request: airk_core::native_focus::NativeFocusRequest = serde_json::from_str(include_str!(
        "../../../../../../fixtures/split/arbiter/native_focus_request.json"
    ))?;
    for method in [
        ControlMethod::ArbiterFocusApplication {
            worker: request.worker,
            authority: request.authority,
            publication: request.scope.publication.clone(),
        },
        ControlMethod::ArbiterFocusRank {
            request: Box::new(request),
        },
    ] {
        assert_eq!(
            method.metadata_relay_refusal(),
            Some("console_private_setup_forbidden")
        );
        assert!(!airk_installer::control_client::prebaseline_metadata_method(&method)?);
    }
    let call = ControlMethod::ToolsCall {
        tool: "airk_search".into(),
        arguments: arguments(),
        selection: airk_core::service::ToolSelection::Admin,
        response_mode: airk_core::service::ToolResponseMode::Full,
        usage_client: airk_core::usage::ClientFamily::Unknown,
        read_attempt: parent()?.captured_transport_attempt(),
    };
    assert!(!airk_installer::control_client::prebaseline_metadata_method(&call)?);
    Ok(())
}

#[test]
fn actual_open_resolution_and_registered_lease_own_singleton_vs_mixed_focus_allocation()
-> Result<()> {
    use super::super::super::agent_plane::{AgentPlane, open_scope_identity};
    use airk_catalog_control::catalog_admission::NativeCatalogAdmission;
    use airk_core::{
        agent_scope::{self, CatalogListing},
        index_catalog::{IndexDefinition, SourceImportState},
        service::ProjectRegistration,
    };
    let service = Incarnation::from_bytes([51; 16]);
    let namespace = Incarnation::from_bytes([52; 16]);
    let revision = CatalogRevision::INITIAL;
    let listing = CatalogListing {
        namespace,
        revision: Some(revision),
        indexes: (1..=32)
            .map(|n| {
                Ok(IndexDefinition {
                    id: indexes(n)[n as usize - 1],
                    name: format!("fixture-{n}"),
                    backing: ProjectRegistration::new(
                        format!("/fixture/source-{n}"),
                        format!("/fixture/data-{n}"),
                        format!("fixture-{n}"),
                    )?,
                    definition_revision: revision,
                    source_revision: revision,
                    policy: None,
                    tombstone: false,
                    source_import: SourceImportState::Complete,
                    description: None,
                })
            })
            .collect::<Result<Vec<_>>>()?,
        projects: vec![],
        memberships: vec![],
        sources: vec![],
    };
    for count in [1, 2, 32] {
        let names: Vec<_> = (1..=count).map(|n| format!("fixture-{n}")).collect();
        let requested = agent_scope::requested(&names, &[]).map_err(anyhow::Error::msg)?;
        let resolved = agent_scope::resolve(&listing, requested).map_err(anyhow::Error::msg)?;
        assert_eq!(resolved.len(), count as usize);
        let identity = open_scope_identity(namespace, revision, &resolved)?;
        let gate = NativeCatalogAdmission::new(service, namespace, revision, 1)?;
        let registrar = gate.clone();
        let mut agents = AgentPlane::new(
            service,
            Box::new(|| 1),
            Box::new(|| Incarnation::from_bytes([53; 16])),
            Box::new(move |selection| registrar.register_selection(selection).map_err(|_| ())),
        );
        let (leased, dropped) = agents.lease(Incarnation::from_bytes([54; 16]), identity, revision);
        assert!(dropped.into_nonces().is_empty());
        let leased = leased.map_err(|e| anyhow::anyhow!("native lease refused: {e:?}"))?;
        let issue = gate.begin_issue(namespace, revision)?;
        gate.with_live_selection(issue, &leased, || ())?;
        let context = admit_resolved(
            "search",
            &arguments(),
            parent()?,
            leased.scope_identity,
            &resolved,
            |_| Ok(Some(limits())),
        )?;
        if count == 1 {
            let allocation = context
                .context_partition()
                .context("actual singleton allocation absent")?;
            allocation.validate_binding(
                &context.captured_transport_attempt().unwrap(),
                leased.scope_identity,
                resolved[0],
            )?;
        } else {
            assert!(context.context_partition().is_none());
        }
        assert!(gate.revoke_selection(&[leased.grant_nonce])?.is_empty());
        assert!(gate.with_live_selection(issue, &leased, || ()).is_err());
    }
    Ok(())
}
