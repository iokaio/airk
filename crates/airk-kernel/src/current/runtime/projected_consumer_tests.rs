// SPDX-License-Identifier: Apache-2.0
//! Public strict readers and actual sealed native consumers preserve values.
use super::*;

#[test]
fn ordinary_readers_refuse_private_ranges_and_keep_exact_strict_values() -> Result<()> {
    let root = crate::unlinked_tempdir()?;
    let data = crate::unlinked_tempdir()?;
    let source = format!(
        "first a\u{e9}\u{754c}\u{1f980}\n{}\nlast",
        "body\n".repeat(30_000)
    );
    for path in ["source.html", "document.txt"] {
        std::fs::write(root.path().join(path), &source)?;
    }
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    let read = kernel.current_reader()?;
    assert!(!read.private_query_eligible()?);
    for path in ["source.html", "document.txt"] {
        let native = read.native_line_projection(&kernel.store, path)?;
        let text = read.text_range_projection(&kernel.store, path)?;
        for budget in [1, 7, 100] {
            let expected = kernel.current_read_lines(
                &read,
                path,
                LineSelection::Range { start: 1, end: 1 },
                budget,
            )?;
            if let Some(native) = &native {
                assert!(native.read_private(1, 1, budget).is_err());
                let selected = native.read_strict(1, 1, budget)?;
                assert_eq!(selected.text, expected.hits[0].text);
                assert_eq!(selected.truncated, expected.truncated);
            } else {
                let text = text.as_ref().context("retained text range missing")?;
                assert!(text.read_private(1, 1, budget).is_err());
                let selected = text.read_strict(1, 1, budget)?;
                assert_eq!(selected.text, expected.hits[0].text);
                assert_eq!(selected.location, expected.hits[0].citation.location);
                assert_eq!(selected.truncated, expected.truncated);
            }
        }
    }
    Ok(())
}

/// Controlled current-user ACL setup belongs solely to this disposable fixture.
/// It does not establish untouched native-installation eligibility.
#[cfg(windows)]
fn controlled_private_directory(path: &std::path::Path) -> Result<()> {
    use std::{
        os::windows::process::CommandExt,
        process::{Command, Stdio},
    };
    let system =
        std::path::PathBuf::from(std::env::var_os("SystemRoot").context("Windows root missing")?)
            .join("System32");
    let output = Command::new(system.join("whoami.exe"))
        .args(["/user", "/fo", "csv", "/nh"])
        .creation_flags(0x08000000)
        .output()?;
    ensure!(
        output.status.success(),
        "fixture current-user SID lookup failed"
    );
    let output = String::from_utf8(output.stdout)?;
    let sid = output
        .split('"')
        .find(|field| field.starts_with("S-1-"))
        .context("fixture user SID absent")?;
    std::fs::create_dir(path)?;
    ensure!(
        Command::new(system.join("icacls.exe"))
            .arg(path)
            .args(["/inheritance:r", "/grant:r"])
            .arg(format!("*{sid}:(OI)(CI)F"))
            .creation_flags(0x08000000)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?
            .success(),
        "fixture private ACL setup failed"
    );
    Ok(())
}

#[cfg(windows)]
#[test]
fn sealed_native_kernel_preserves_read_context_validators_file_info_search_and_relationships()
-> Result<()> {
    use airk_core::{
        RelationshipQuery, SearchQuery, evidence::EvidenceReadOptions, identity::Incarnation,
    };
    use airk_store::service::{ProjectLease, data_scope, registry::ServiceRegistry};
    let temporary = crate::unlinked_tempdir()?;
    let source = temporary.path().join("source");
    let data = temporary.path().join("data");
    let control = temporary.path().join("control");
    std::fs::create_dir(&source)?;
    std::fs::create_dir(&control)?;
    controlled_private_directory(&data)?;
    let source = source.canonicalize()?;
    let data = data.canonicalize()?;
    let text = format!(
        "first a\u{e9}\u{754c}\u{1f980}\r\n{}last",
        "body copper\n".repeat(7000)
    );
    for path in ["source.html", "document.txt"] {
        std::fs::write(source.join(path), &text)?;
    }
    std::fs::write(
        source.join("code.rs"),
        "fn leaf() {}\nfn middle() { leaf(); }\nfn outer() { middle(); }\n",
    )?;
    let namespace = Incarnation::from_bytes([181; 16]);
    let registration = airk_core::service::ProjectRegistration::new(
        source.to_str().unwrap().into(),
        data.to_str().unwrap().into(),
        "fixture".into(),
    )?;
    let mut registry = ServiceRegistry::open(&control, namespace)?;
    registry.register(&registration)?;
    let bound = data_scope::bind(&data, &control, namespace)?;
    let kernel = Kernel::open_service_collection(&source, Some(data), "fixture")?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    let cases = [
        ("source.html", 1, 1, 1),
        ("source.html", 1, 7002, 65536),
        ("source.html", 7002, 7002, 7),
        ("document.txt", 1, 1, 7),
        ("document.txt", 7000, 7002, 65536),
    ];
    let expected_reads = cases
        .iter()
        .map(|(path, start, end, budget)| {
            kernel.read_file_conditional(path, *start, *end, *budget, &Default::default())
        })
        .collect::<Result<Vec<_>>>()?;
    let expected_contexts = ["source.html", "document.txt"]
        .into_iter()
        .map(|path| kernel.context_conditional(path, 7001, 1, 1, 7, &Default::default()))
        .collect::<Result<Vec<_>>>()?;
    let expected_infos = ["source.html", "document.txt"]
        .into_iter()
        .map(|path| kernel.file_info_conditional(path, &Default::default()))
        .collect::<Result<Vec<_>>>()?;
    let query = SearchQuery {
        text: "copper".into(),
        path_prefix: None,
        limit: 10,
        max_bytes: 4096,
    };
    let expected_search = kernel.search(&query)?;
    assert!(!expected_search.hits.is_empty());
    let facts = kernel.outline("code.rs", 100, 65536)?;
    let symbols = facts
        .facts
        .iter()
        .filter_map(|fact| fact.code.as_ref())
        .flat_map(|code| code.owner_id.iter().chain(code.target.iter()))
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    assert!(
        !symbols.is_empty(),
        "syntax fixture produced no relationships"
    );
    let mut relationship_queries = Vec::new();
    let mut expected_relationships = Vec::new();
    for symbol in symbols.iter().take(3) {
        for direction in ["incoming", "outgoing"] {
            for hops in [1, 3] {
                let query = RelationshipQuery {
                    symbol: symbol.clone(),
                    direction: direction.into(),
                    kind: None,
                    path_prefix: "code.rs".into(),
                    hops,
                    limit: 17,
                    max_bytes: 65536,
                };
                expected_relationships.push(kernel.relationships(&query)?);
                relationship_queries.push(query);
            }
        }
    }
    assert!(
        expected_relationships
            .iter()
            .any(|response| !response.relationships.is_empty())
    );
    let mut lease = ProjectLease::acquire(&control, registration.binding)?;
    registry.admit_worker(&mut lease)?;
    let identity = airk_core::service::WorkerIdentity {
        service: namespace,
        worker: Incarnation::from_bytes([182; 16]),
        project: registration.binding,
        fencing_generation: lease.identity().1.context("fixture epoch missing")?,
    };
    let observed = kernel.current_reader()?;
    registry.observe_retained_audit_index(&lease, &observed, 0, 512 << 20)?;
    drop(observed);
    kernel.arm_native_owner(&mut registry, std::sync::Arc::new(lease), &bound, identity)?;
    assert!(kernel.current_reader()?.private_query_eligible()?);
    for ((path, start, end, budget), expected) in cases.iter().zip(expected_reads) {
        let actual =
            kernel.read_file_conditional(path, *start, *end, *budget, &Default::default())?;
        assert_eq!(
            serde_json::to_value(&actual)?,
            serde_json::to_value(&expected)?,
            "{path}:{start}:{end}:{budget}"
        );
        let unchanged = kernel.read_file_conditional(
            path,
            *start,
            *end,
            *budget,
            &EvidenceReadOptions {
                if_evidence: actual.read.validator,
                ..Default::default()
            },
        )?;
        assert!(unchanged.content.is_none());
    }
    for ((path, context), info) in ["source.html", "document.txt"]
        .into_iter()
        .zip(expected_contexts)
        .zip(expected_infos)
    {
        assert_eq!(
            serde_json::to_value(kernel.context_conditional(
                path,
                7001,
                1,
                1,
                7,
                &Default::default()
            )?)?,
            serde_json::to_value(context)?
        );
        assert_eq!(
            serde_json::to_value(kernel.file_info_conditional(path, &Default::default())?)?,
            serde_json::to_value(info)?
        );
    }
    assert_eq!(
        serde_json::to_value(kernel.search(&query)?)?,
        serde_json::to_value(expected_search)?
    );
    for (query, expected) in relationship_queries.iter().zip(expected_relationships) {
        assert_eq!(
            serde_json::to_value(kernel.relationships(query)?)?,
            serde_json::to_value(expected)?
        );
    }
    kernel.settle_native_owner_after_owned_drain(&mut registry)?;
    assert!(!kernel.current_reader()?.private_query_eligible()?);
    Ok(())
}

#[cfg(windows)]
#[test]
fn sealed_native_rebuild_restores_strict_reads_after_private_damage() -> Result<()> {
    use airk_core::{
        identity::Incarnation,
        service::{JobKind, JobOutcome, ProjectRegistration, WorkerIdentity},
    };
    use airk_store::service::{ProjectLease, data_scope, registry::ServiceRegistry};
    let temporary = crate::unlinked_tempdir()?;
    let source = temporary.path().join("source");
    let data = temporary.path().join("data");
    let control = temporary.path().join("control");
    std::fs::create_dir(&source)?;
    std::fs::create_dir(&control)?;
    controlled_private_directory(&data)?;
    let source = source.canonicalize()?;
    let data = data.canonicalize()?;
    let text = format!("first copper\n{}last", "body copper\n".repeat(7000));
    std::fs::write(source.join("source.html"), &text)?;
    let namespace = Incarnation::from_bytes([191; 16]);
    let registration = ProjectRegistration::new(
        source.to_str().unwrap().into(),
        data.to_str().unwrap().into(),
        "fixture".into(),
    )?;
    let mut registry = ServiceRegistry::open(&control, namespace)?;
    registry.register(&registration)?;
    let bound = data_scope::bind(&data, &control, namespace)?;
    let kernel = Kernel::open_service_collection(&source, Some(data), "fixture")?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    let mut lease = ProjectLease::acquire(&control, registration.binding)?;
    registry.admit_worker(&mut lease)?;
    let lease = Arc::new(lease);
    let identity = WorkerIdentity {
        service: namespace,
        worker: Incarnation::from_bytes([192; 16]),
        project: registration.binding,
        fencing_generation: lease.identity().1.context("worker epoch missing")?,
    };
    let observed = kernel.current_reader()?;
    registry.observe_retained_audit_index(&lease, &observed, 0, 512 << 20)?;
    drop(observed);
    kernel.arm_native_owner(&mut registry, lease.clone(), &bound, identity)?;
    let first = kernel.current_reader()?.token()?;
    // A healthy owner must not be attached to the unpublished replacement's
    // strict verification reads before its manifest switch.
    let job = registry.enqueue_internal(
        registration.binding,
        namespace,
        JobKind::Rebuild,
        Incarnation::from_bytes([193; 16]),
    )?;
    registry
        .claim_job_id(&lease, job.id)?
        .context("healthy rebuild not claimed")?;
    let healthy_token = kernel.rebuild_service(job.id)?;
    assert_ne!(healthy_token.incarnation, first.incarnation);
    registry.complete_job(
        &lease,
        job.id,
        JobOutcome::Succeeded {
            revision: Some(healthy_token),
        },
    )?;
    let healthy = kernel.refresh_service_snapshot()?;
    let admitted = healthy.current_reader()?;
    assert!(admitted.private_query_eligible()?);
    let compact = admitted
        .artifact("source.html")?
        .context("selected source missing")?;
    let path = healthy
        .store
        .directory()
        .join("objects/sources")
        .join(&compact.content_hash);
    let mut bytes = std::fs::read(&path)?;
    bytes[0] ^= 1;
    std::fs::write(path, bytes)?;
    assert!(healthy.read_file("source.html", 1, 1, 4096).is_err());
    assert!(admitted.token().is_err());
    let damage = registry.recorded_integrity_damage(&lease)?;
    assert_eq!(damage.len(), 1);
    assert_eq!(damage[0].incarnation, healthy_token.incarnation);
    // Original source files remain authoritative and unchanged. The explicit
    // accepted rebuild restores a new incarnation without erasing old damage.
    assert_eq!(std::fs::read_to_string(source.join("source.html"))?, text);
    let job = registry.enqueue_internal(
        registration.binding,
        namespace,
        JobKind::Rebuild,
        Incarnation::from_bytes([194; 16]),
    )?;
    registry
        .claim_job_id(&lease, job.id)?
        .context("restoring rebuild not claimed")?;
    let restored_token = healthy.rebuild_service(job.id)?;
    registry.complete_job(
        &lease,
        job.id,
        JobOutcome::Succeeded {
            revision: Some(restored_token),
        },
    )?;
    assert_ne!(restored_token.incarnation, healthy_token.incarnation);
    let restored = kernel.refresh_service_snapshot()?;
    assert!(!restored.current_reader()?.private_query_eligible()?);
    assert!(
        restored.read_file("source.html", 1, 1, 4096)?.hits[0]
            .text
            .contains("first copper")
    );
    assert_eq!(registry.recorded_integrity_damage(&lease)?, damage);
    assert!(admitted.token().is_err());
    restored.settle_native_owner_after_owned_drain(&mut registry)?;
    let db = rusqlite::Connection::open(control.join("service-1.4.sqlite3"))?;
    let state: String = db.query_row(
        "SELECT engine FROM service_recovery_budget WHERE scope=?1",
        [format!("owner_session:{}", registration.binding)],
        |row| row.get(0),
    )?;
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&state)?["state"],
        "active"
    );
    assert!(
        restored
            .try_arm_native_owner(&mut registry, lease, &bound, identity)
            .is_err(),
        "retiring failure must not manufacture a new private grant"
    );
    Ok(())
}

#[cfg(windows)]
#[test]
fn fresh_physical_verify_retires_uncertain_owner_and_preserves_old_reader_revocation() -> Result<()>
{
    use airk_core::{
        identity::{Incarnation, RevisionToken},
        service::{JobKind, JobOutcome, ProjectRegistration, WorkerIdentity},
    };
    use airk_store::service::{
        ProjectLease, data_scope,
        registry::{AuditEvidence, ServiceRegistry},
    };
    let temporary = crate::unlinked_tempdir()?;
    let source = temporary.path().join("source");
    let data = temporary.path().join("data");
    let control = temporary.path().join("control");
    std::fs::create_dir(&source)?;
    std::fs::create_dir(&control)?;
    controlled_private_directory(&data)?;
    let source = source.canonicalize()?;
    let data = data.canonicalize()?;
    std::fs::write(source.join("source.html"), "first copper\nsecond line")?;
    let namespace = Incarnation::from_bytes([201; 16]);
    let registration = ProjectRegistration::new(
        source.to_str().unwrap().into(),
        data.to_str().unwrap().into(),
        "fixture".into(),
    )?;
    let mut registry = ServiceRegistry::open(&control, namespace)?;
    registry.register(&registration)?;
    let bound = data_scope::bind(&data, &control, namespace)?;
    let kernel = Kernel::open_service_collection(&source, Some(data), "fixture")?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    let mut lease = ProjectLease::acquire(&control, registration.binding)?;
    registry.admit_worker(&mut lease)?;
    let lease = Arc::new(lease);
    let identity = WorkerIdentity {
        service: namespace,
        worker: Incarnation::from_bytes([202; 16]),
        project: registration.binding,
        fencing_generation: lease.identity().1.context("worker epoch missing")?,
    };
    let observed = kernel.current_reader()?;
    registry.observe_retained_audit_index(&lease, &observed, 0, 512 << 20)?;
    drop(observed);
    kernel.arm_native_owner(&mut registry, lease.clone(), &bound, identity)?;
    let old = kernel.current_reader()?;
    let token = old.token()?;
    assert!(
        old.observe_private_integrity::<()>(Err(std::io::Error::other(
            "controlled uncertain selected read"
        )
        .into()))
            .is_err()
    );
    assert!(kernel.native_owner_requires_recovery()?);
    assert!(kernel.read_file("source.html", 1, 1, 4096).is_err());
    assert!(registry.recorded_integrity_damage(&lease)?.is_empty());
    let job = registry.enqueue_internal(
        registration.binding,
        namespace,
        JobKind::Verify,
        Incarnation::from_bytes([203; 16]),
    )?;
    registry
        .claim_job_id(&lease, job.id)?
        .context("fresh recovery Verify not claimed")?;
    let epoch =
        kernel.begin_service_native_recovery(&registry, &lease, &bound, identity, job.id)?;
    let mut completed = None;
    for _ in 0..16 {
        let turn = kernel.verify_native_recovery_service_turn(
            &registry,
            &lease,
            &bound,
            &epoch,
            512 << 20,
        )?;
        if turn.recovery_completed.is_some() {
            completed = Some(turn);
            break;
        }
    }
    let turn = completed.context("bounded fresh physical recovery did not finish")?;
    kernel.recover_service_native_owner(
        &mut registry,
        &lease,
        &bound,
        identity,
        job.id,
        turn.recovery_completed
            .as_ref()
            .context("sealed recovery proof missing")?,
    )?;
    assert!(!kernel.native_owner_requires_recovery()?);
    assert!(
        old.token().is_err(),
        "physical recovery must not revive an old capability"
    );
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis()
        .try_into()?;
    let (outcome, evidence) = if let Some(proof) = turn.completed.as_ref() {
        (
            JobOutcome::Succeeded {
                revision: Some(RevisionToken {
                    incarnation: token.incarnation,
                    generation: token.sequence,
                    revision: token.digest,
                }),
            },
            AuditEvidence::Complete(proof),
        )
    } else {
        (
            JobOutcome::Failed {
                code: "integrity_coverage_incomplete".into(),
            },
            AuditEvidence::Drained(
                turn.drained
                    .as_ref()
                    .context("actual physical drain missing")?,
            ),
        )
    };
    registry.complete_job_with_maintenance_and_audit_at(
        &lease,
        job.id,
        outcome,
        now,
        None,
        Some(evidence),
    )?;
    assert!(kernel.try_arm_native_owner(&mut registry, lease.clone(), &bound, identity)?);
    assert!(kernel.current_reader()?.private_query_eligible()?);
    assert!(
        kernel.read_file("source.html", 1, 1, 4096)?.hits[0]
            .text
            .contains("first copper")
    );
    assert!(old.private_query_eligible().is_err());
    assert!(registry.recorded_integrity_damage(&lease)?.is_empty());
    kernel.settle_native_owner_after_owned_drain(&mut registry)?;
    Ok(())
}
