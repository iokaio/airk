// SPDX-License-Identifier: Apache-2.0
//! Canonical handler/SDK transport tests; no installed-service/model claim.
use super::*;
use airk_core::arbiter::context_action::{
    CONTEXT_SCHEMA_V2 as CONTEXT_SCHEMA, ContextActionAdvice, ContextRequestV2 as ContextRequest,
    REQUEST_SCHEMA_V2 as REQUEST_SCHEMA,
};
use airk_core::native_context_action::NativeContextActionRequest;
use airk_core::{
    arbiter::*,
    identity::{Incarnation, Sha256Digest},
    service::{ScopedReadRef, WorkerIdentity},
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

pub(super) struct Disabled;
impl airk_kernel::arbiter::NativeController for Disabled {
    fn application(
        &self,
        _: WorkerIdentity,
        authority: ScopedReadRef,
        publication: &airk_core::evidence::RevisionEnvelope,
    ) -> Result<IndexApplication> {
        Ok(IndexApplication {
            scope: ScopeBinding {
                installation: Sha256Digest::from_bytes([7; 32]),
                catalog: Incarnation::from_bytes([8; 16]),
                index_id: authority.index_id,
                authorization: authority.fingerprint,
                configuration_revision: 1,
                publication: publication.clone(),
            },
            profile: None,
            availability: Availability::Disabled,
            reason: "Explicitly disabled test owner".into(),
        })
    }
}
pub(super) fn body(reply: &CallToolResult) -> Result<Value> {
    ensure!(reply.is_error != Some(true), "{reply:?}");
    serde_json::from_str(&reply.content[0].as_text().context("missing JSON")?.text)
        .map_err(Into::into)
}
fn stable(value: &mut Value) {
    match value {
        Value::Object(fields) => {
            for key in [
                "request_id",
                "observed_unix_ms",
                "cache_age_ms",
                "cached",
                "observed_from_unix_ms",
                "observed_to_unix_ms",
            ] {
                fields.remove(key);
            }
            for value in fields.values_mut() {
                stable(value);
            }
        }
        Value::Array(values) => {
            for value in values {
                stable(value);
            }
        }
        _ => {}
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn disabled_native_answer_expands_real_scoped_evidence_and_preserves_full_canonical_results()
-> Result<()> {
    let fixture = tempfile::tempdir()?;
    let root = fixture.path().join("source");
    std::fs::create_dir_all(root.join("public"))?;
    std::fs::create_dir_all(root.join("private"))?;
    let text: String = (1..=800).map(|line| if line == 400 {"amber_marker owns the original finite request deadline.\n".into()} else {format!("Line {line}: the delivery remains bound to its original source authority and unchanged publication.\n")}).collect();
    std::fs::write(root.join("public/policy.txt"), &text)?;
    std::fs::write(
        root.join("private/secret.txt"),
        "amber_marker must never expose SECRET_PRIVATE_ARCHIVE\n",
    )?;
    let base = Kernel::open(&root, Some(fixture.path().join("data")))?;
    base.configure_vectors(false)?;
    base.index()?;
    let authority = super::scoped_observation_tests::authority(&base)?;
    let kernel = base.with_scoped_reads(authority.clone())?;
    let reference = ScopedReadRef {
        schema: 1,
        lease_generation: 1,
        index_id: authority.index_id,
        grant_id: Some(authority.grant.id),
        grant_revision: Some(authority.grant.revision),
        selection: None,
        fingerprint: kernel.scoped_read_fingerprint()?.unwrap(),
    };
    let worker = WorkerIdentity {
        service: Incarnation::from_bytes([8; 16]),
        worker: Incarnation::from_bytes([9; 16]),
        project: authority.backing,
        fencing_generation: 1,
    };
    let kernel = kernel.with_native_arbiter(worker, reference, Arc::new(Disabled))?;
    let mut tools = local::LocalTools::new(kernel.clone()).await?;
    let request = ContextRequest {
        schema: REQUEST_SCHEMA.into(),
        question: "amber_marker".into(),
        budget: Budget::default(),
    };
    let mut arguments = serde_json::to_value(&request)?;
    arguments["response_mode"] = json!("full");
    let reply = tools
        .call("airk_answer".into(), arguments.as_object().unwrap().clone())
        .await?;
    let packet = body(&reply)?;
    assert_eq!(packet["schema"], CONTEXT_SCHEMA);
    assert_eq!(packet["applications"][0]["availability"], "disabled");
    assert!(packet["work"]["calls"].as_u64().unwrap() >= 2, "{packet}");
    let records = packet["records"].as_array().unwrap();
    assert!(
        records
            .iter()
            .any(|record| record["operation"] == "context")
    );
    assert!(!packet.to_string().contains("SECRET_PRIVATE_ARCHIVE"));
    assert!(!packet.to_string().contains("private/secret.txt"));
    for record in records {
        let operation: NativeRead = serde_json::from_value(record["operation"].clone())?;
        let tool = match operation {
            NativeRead::Search => "airk_search",
            NativeRead::ReadEvidence => "airk_read_file",
            NativeRead::Context => "airk_context",
            _ => anyhow::bail!("unexpected recipe operation"),
        };
        let mut args = record["arguments"].as_object().unwrap().clone();
        args.insert("response_mode".into(), json!("full"));
        args.insert("include_status".into(), json!(true));
        if matches!(operation, NativeRead::ReadEvidence | NativeRead::Context) {
            args.insert(
                "revision".into(),
                record["scope"]["publication"]["token"].clone(),
            );
        }
        let mut direct = body(&tools.call(tool.into(), args).await?)?;
        let mut retained = record["canonical_result"].clone();
        stable(&mut direct);
        stable(&mut retained);
        assert_eq!(retained, direct, "full canonical result changed for {tool}");
    }
    assert!(serde_json::to_vec(&reply)?.len() < airk_core::MAX_RESPONSE_BYTES);
    // The actual partial index cannot expose whole-index status as a shortcut.
    assert!(kernel.status().is_err());
    tools.close().await?;
    Ok(())
}

#[tokio::test]
async fn legacy_answer_refuses_without_a_native_lease_binding() -> Result<()> {
    let fixture = tempfile::tempdir()?;
    let kernel = Kernel::open(fixture.path(), Some(fixture.path().join("data")))?;
    let mut tools = local::LocalTools::new(kernel).await?;
    let reply = tools
        .call(
            "airk_answer".into(),
            serde_json::to_value(ContextRequest {
                schema: REQUEST_SCHEMA.into(),
                question: "original authority".into(),
                budget: Budget::default(),
            })?
            .as_object()
            .unwrap()
            .clone(),
        )
        .await?;
    assert_eq!(reply.is_error, Some(true));
    tools.close().await?;
    Ok(())
}

#[test]
fn canonical_answer_retains_original_deadline_before_any_native_read() -> Result<()> {
    let fixture = tempfile::tempdir()?;
    let kernel = Kernel::open(fixture.path(), Some(fixture.path().join("data")))?;
    let calls = std::sync::atomic::AtomicUsize::new(0);
    let result = kernel.answer_with_reader_v2(
        ContextRequest {
            schema: REQUEST_SCHEMA.into(),
            question: "original deadline".into(),
            budget: Budget::default(),
        },
        Instant::now() - Duration::from_secs(1),
        &|| Ok(()),
        &|_, _, _| {
            calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(json!({}))
        },
    );
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("original deadline")
    );
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn canonical_answer_context_handles_one_line_eof_and_late_verified_hits() -> Result<()> {
    for (name, text, query, expected_line) in [
        (
            "one-line",
            "amber_eof is the entire excerpt.\n".to_string(),
            "amber_eof",
            1usize,
        ),
        (
            "late-eof",
            (1..=240)
                .map(|line| {
                    if line == 240 {
                        "amber_eof is the last indexed line.\n".into()
                    } else {
                        format!("line {line} ordinary indexed background.\n")
                    }
                })
                .collect(),
            "amber_eof",
            240usize,
        ),
    ] {
        let fixture = tempfile::tempdir()?;
        let root = fixture.path().join("source");
        std::fs::create_dir_all(root.join("public"))?;
        std::fs::write(root.join("public/eof.txt"), text)?;
        let base = Kernel::open(&root, Some(fixture.path().join("data")))?;
        base.configure_vectors(false)?;
        base.index()?;
        let authority = crate::scoped_observation_tests::authority(&base)?;
        let kernel = base.with_scoped_reads(authority.clone())?;
        let reference = ScopedReadRef {
            schema: 1,
            lease_generation: 1,
            index_id: authority.index_id,
            grant_id: Some(authority.grant.id),
            grant_revision: Some(authority.grant.revision),
            selection: None,
            fingerprint: kernel.scoped_read_fingerprint()?.unwrap(),
        };
        let worker = WorkerIdentity {
            service: Incarnation::from_bytes([8; 16]),
            worker: Incarnation::from_bytes([9; 16]),
            project: authority.backing,
            fencing_generation: 1,
        };
        let kernel = kernel.with_native_arbiter(worker, reference, Arc::new(Disabled))?;
        let mut tools = local::LocalTools::new(kernel).await?;
        let packet=body(&tools.call("airk_answer".into(),json!({"schema":REQUEST_SCHEMA,"question":query,"budget":Budget::default(),"response_mode":"full"}).as_object().unwrap().clone()).await?)?;
        assert_ne!(
            packet["stop"], "refused",
            "{name}: EOF must clamp in owning canonical context: {packet}"
        );
        let expansion = packet["records"]
            .as_array()
            .unwrap()
            .iter()
            .find(|record| record["operation"] == "context")
            .context("missing canonical context expansion")?;
        assert_eq!(expansion["arguments"]["before"], 0);
        assert_eq!(expansion["arguments"]["after"], 32);
        assert_eq!(
            expansion["canonical_result"]["hits"][0]["citation"]["end_line"],
            expected_line
        );
        assert!(
            expansion["canonical_result"]["hits"][0]["text"]
                .as_str()
                .unwrap()
                .contains("amber_eof")
        );
        assert!(expansion["arguments"]["line"].as_u64().unwrap() <= expected_line as u64);
        tools.close().await?;
    }
    Ok(())
}

struct OptionalFault {
    scope_refusal: bool,
    calls: std::sync::atomic::AtomicUsize,
    observed_deadline: std::sync::Mutex<Option<Instant>>,
}
impl airk_kernel::arbiter::NativeController for OptionalFault {
    fn application(
        &self,
        worker: WorkerIdentity,
        authority: ScopedReadRef,
        publication: &airk_core::evidence::RevisionEnvelope,
    ) -> Result<IndexApplication> {
        let mut application = airk_kernel::arbiter::NativeController::application(
            &Disabled,
            worker,
            authority,
            publication,
        )?;
        application.availability = Availability::Ready;
        application.profile = Some(Sha256Digest::from_bytes([33; 32]));
        application.reason =
            "Private callback-failure fixture; no actual model qualification claim.".into();
        Ok(application)
    }
    fn context_action(
        &self,
        _: NativeContextActionRequest,
        deadline: Instant,
    ) -> Result<Option<ContextActionAdvice>> {
        *self.observed_deadline.lock().unwrap() = Some(deadline);
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if self.scope_refusal {
            Err(airk_core::scoped_reads::ScopeReadRefused.into())
        } else {
            anyhow::bail!("optional native resource unavailable")
        }
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn optional_callback_failure_keeps_canonical_evidence_but_typed_scope_refusal_stops_delivery()
-> Result<()> {
    for scope_refusal in [false, true] {
        let fixture = tempfile::tempdir()?;
        let root = fixture.path().join("source");
        std::fs::create_dir_all(root.join("public"))?;
        for i in 0..3 {
            std::fs::write(
                root.join(format!("public/policy-{i}.txt")),
                format!("amber_callback owns indexed policy {i}.\n"),
            )?;
        }
        let base = Kernel::open(&root, Some(fixture.path().join("data")))?;
        base.configure_vectors(false)?;
        base.index()?;
        let authority = crate::scoped_observation_tests::authority(&base)?;
        let kernel = base.with_scoped_reads(authority.clone())?;
        let reference = ScopedReadRef {
            schema: 1,
            lease_generation: 1,
            index_id: authority.index_id,
            grant_id: Some(authority.grant.id),
            grant_revision: Some(authority.grant.revision),
            selection: None,
            fingerprint: kernel.scoped_read_fingerprint()?.unwrap(),
        };
        let worker = WorkerIdentity {
            service: Incarnation::from_bytes([8; 16]),
            worker: Incarnation::from_bytes([9; 16]),
            project: authority.backing,
            fencing_generation: 1,
        };
        let fault = Arc::new(OptionalFault {
            scope_refusal,
            calls: std::sync::atomic::AtomicUsize::new(0),
            observed_deadline: std::sync::Mutex::new(None),
        });
        let kernel = kernel.with_native_arbiter(worker, reference, fault.clone())?;
        let mut tools = local::LocalTools::new(kernel).await?;
        let original = Instant::now() + Duration::from_secs(2);
        let budget = Budget {
            deadline_ms: 30_000,
            ..Budget::default()
        };
        let arguments=json!({"schema":REQUEST_SCHEMA,"question":"amber_callback","budget":budget,"response_mode":"full"}).as_object().unwrap().clone();
        let reply = tools
            .call_with_read_context(
                "airk_answer".into(),
                arguments.clone(),
                crate::read_attempt::ReadAttemptContext::new(original, Default::default())
                    .with_wire_attempt(airk_core::read_attempt::ReadAttemptBudget {
                        schema: 1,
                        request_id: Incarnation::from_bytes([31; 16]),
                        clock: if cfg!(windows) {
                            airk_core::read_attempt::ReadClock::WindowsPerformanceCounter
                        } else {
                            airk_core::read_attempt::ReadClock::UnixMonotonic
                        },
                        expires_monotonic_ns: u64::MAX,
                        remaining_ns: 2_000_000_000,
                        native_retry_expires_monotonic_ns: None,
                    })?,
            )
            .await?;
        assert!(
            fault
                .observed_deadline
                .lock()
                .unwrap()
                .is_some_and(|observed| observed <= original),
            "canonical Answer must intersect serialized30s with original2s before the native callback"
        );
        assert_eq!(
            fault.calls.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "optional failure must not replay native work"
        );
        if scope_refusal {
            assert_eq!(
                reply.is_error,
                Some(true),
                "typed native authority refusal must never become a partial successful packet"
            );
        } else {
            let packet = body(&reply)?;
            assert_eq!(packet["applications"][0]["availability"], "failed");
            assert!(
                packet["applications"][0]["reason"]
                    .as_str()
                    .unwrap()
                    .contains("not applied")
            );
            assert!(packet["records"].as_array().unwrap().len() >= 2);
            assert!(packet.get("advice").is_none());
            assert_eq!(packet["assessment_history"].as_array().unwrap().len(), 1);
            assert_eq!(
                packet["assessment_history"][0]["result"]["status"],
                "unavailable"
            );
            assert_eq!(
                packet["assessment_history"][0]["result"]["work_unknown"],
                true
            );
            assert!(
                packet["records"][0]["canonical_result"]["hits"]
                    .as_array()
                    .unwrap()
                    .len()
                    >= 2
            );
        }
        let expired = tools
            .call_with_read_context(
                "airk_answer".into(),
                arguments,
                crate::read_attempt::ReadAttemptContext::new(Instant::now(), Default::default()),
            )
            .await;
        assert!(
            expired.is_err()
                || expired
                    .as_ref()
                    .is_ok_and(|reply| reply.is_error == Some(true))
        );
        assert_eq!(
            fault.calls.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "expired original parent must never replay optional native work"
        );
        tools.close().await?;
    }
    Ok(())
}

struct RevokingJoint {
    returned: std::sync::atomic::AtomicBool,
    calls: std::sync::atomic::AtomicUsize,
    observations: std::sync::atomic::AtomicUsize,
    revoke_at: usize,
}
impl airk_kernel::arbiter::NativeController for RevokingJoint {
    fn application(
        &self,
        worker: WorkerIdentity,
        authority: ScopedReadRef,
        publication: &airk_core::evidence::RevisionEnvelope,
    ) -> Result<IndexApplication> {
        let mut current = airk_kernel::arbiter::NativeController::application(
            &Disabled,
            worker,
            authority,
            publication,
        )?;
        let observation = self
            .observations
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            + 1;
        if self.returned.load(std::sync::atomic::Ordering::SeqCst) && observation >= self.revoke_at
        {
            current.scope.configuration_revision = 2;
            current.reason = "Policy was disabled during the native callback.".into();
        } else {
            current.availability = Availability::Ready;
            current.profile = Some(Sha256Digest::from_bytes([33; 32]));
        }
        Ok(current)
    }
    fn context_action(
        &self,
        request: NativeContextActionRequest,
        _: Instant,
    ) -> Result<Option<ContextActionAdvice>> {
        use airk_core::arbiter::context_action::{ADVICE_SCHEMA_V2, ContextAction};
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let card = request
            .roster
            .cards
            .iter()
            .find(|card| card.action == ContextAction::Sufficient)
            .unwrap();
        // Deterministic event barrier: config changes while this exact native
        // callback is outstanding, before the caller can apply its return.
        self.returned
            .store(true, std::sync::atomic::Ordering::SeqCst);
        Ok(Some(ContextActionAdvice {
            schema: ADVICE_SCHEMA_V2.into(),
            task: Task::ContextAction,
            profile: Sha256Digest::from_bytes([33; 32]),
            runtime: RUNTIME.into(),
            compiler: Sha256Digest::from_bytes([34; 32]),
            calibration: Sha256Digest::from_bytes([35; 32]),
            policy: Sha256Digest::from_bytes([36; 32]),
            task_policy: request.roster.policy,
            scope: request.scope,
            input_digest: Sha256Digest::from_bytes([37; 32]),
            evidence: request
                .roster
                .basis
                .source_units_presented
                .iter()
                .map(|unit| unit.record)
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect(),
            reserved_input_tokens: 10,
            observed_input_tokens: Some(10),
            work: Work {
                input_tokens: 10,
                compile_us: Some(0),
                inference_us: Some(0),
                ..Work::default()
            },
            outcome: Outcome::Accepted {
                decision: Decision::ContextAction(card.id),
            },
        }))
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn policy_withdrawal_during_joint_wait_refuses_stale_answer_without_replay_or_primitive_loss()
-> Result<()> {
    // Immediate post-wait cut and the final post-packing observation cut.
    for revoke_at in [0, 6] {
        let fixture = tempfile::tempdir()?;
        let root = fixture.path().join("source");
        std::fs::create_dir_all(root.join("public"))?;
        std::fs::create_dir_all(root.join("private"))?;
        std::fs::write(
            root.join("public/policy.txt"),
            "amber_revoked AMBER_PRIVATE_MODEL_HISTORY owns this source rule.\n",
        )?;
        std::fs::write(root.join("private/secret.txt"), "SECRET_NOT_GRANTED\n")?;
        let base = Kernel::open(&root, Some(fixture.path().join("data")))?;
        base.configure_vectors(false)?;
        base.index()?;
        let authority = crate::scoped_observation_tests::authority(&base)?;
        let kernel = base.with_scoped_reads(authority.clone())?;
        let reference = ScopedReadRef {
            schema: 1,
            lease_generation: 1,
            index_id: authority.index_id,
            grant_id: Some(authority.grant.id),
            grant_revision: Some(authority.grant.revision),
            selection: None,
            fingerprint: kernel.scoped_read_fingerprint()?.unwrap(),
        };
        let worker = WorkerIdentity {
            service: Incarnation::from_bytes([8; 16]),
            worker: Incarnation::from_bytes([9; 16]),
            project: authority.backing,
            fencing_generation: 1,
        };
        let controller = Arc::new(RevokingJoint {
            returned: false.into(),
            calls: 0.into(),
            observations: 0.into(),
            revoke_at,
        });
        let kernel = kernel.with_native_arbiter(worker, reference, controller.clone())?;
        let mut tools = local::LocalTools::new(kernel).await?;
        let context = crate::read_attempt::ReadAttemptContext::new(
            Instant::now() + Duration::from_secs(5),
            Default::default(),
        )
        .with_wire_attempt(airk_core::read_attempt::ReadAttemptBudget {
            schema: 1,
            request_id: Incarnation::from_bytes([32; 16]),
            clock: if cfg!(windows) {
                airk_core::read_attempt::ReadClock::WindowsPerformanceCounter
            } else {
                airk_core::read_attempt::ReadClock::UnixMonotonic
            },
            expires_monotonic_ns: u64::MAX,
            remaining_ns: 5_000_000_000,
            native_retry_expires_monotonic_ns: None,
        })?;
        let reply = tools
            .call_with_read_context(
                "airk_answer".into(),
                json!({"schema":REQUEST_SCHEMA,
        "question":"amber_revoked","budget":Budget::default(),"response_mode":"full"})
                .as_object()
                .unwrap()
                .clone(),
                context,
            )
            .await?;
        assert_eq!(reply.is_error, Some(true));
        let refusal: Value = serde_json::from_str(&reply.content[0].as_text().unwrap().text)?;
        assert_eq!(refusal["error"]["code"], "arbiter_context_action_stale");
        assert_eq!(refusal["error"]["retryable"], false);
        assert_eq!(refusal["error"]["query_replay_safe"], false);
        assert!(!serde_json::to_string(&reply)?.contains("AMBER_PRIVATE_MODEL_HISTORY"));
        assert_eq!(
            controller.calls.load(std::sync::atomic::Ordering::SeqCst),
            1
        );
        let ordinary = body(
            &tools
                .call(
                    "airk_search".into(),
                    json!({"query":"amber_revoked","response_mode":"full"})
                        .as_object()
                        .unwrap()
                        .clone(),
                )
                .await?,
        )?;
        assert!(!ordinary["hits"].as_array().unwrap().is_empty());
        assert_eq!(
            controller.calls.load(std::sync::atomic::Ordering::SeqCst),
            1
        );
        tools.close().await?;
    }
    Ok(())
}
