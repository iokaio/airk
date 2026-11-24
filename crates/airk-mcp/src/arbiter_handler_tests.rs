// SPDX-License-Identifier: Apache-2.0
//! Canonical handler/SDK transport tests; no installed-service/model claim.
use super::*;
use airk_core::arbiter::answer_loop::{
    self as v3, BudgetV3, CONTEXT_SCHEMA_V3 as CONTEXT_SCHEMA, ContextRequestV3 as ContextRequest,
    REQUEST_SCHEMA_V3 as REQUEST_SCHEMA,
};
use airk_core::native_loop::NativeCardRequest;
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
fn request(question: &str, budget: BudgetV3) -> Result<serde_json::Map<String, Value>> {
    let request = ContextRequest {
        schema: REQUEST_SCHEMA.into(),
        question: question.into(),
        budget,
        continuation: None,
    };
    request.validate()?;
    let mut arguments = serde_json::to_value(&request)?;
    arguments["response_mode"] = json!("full");
    Ok(arguments.as_object().context("Answer arguments")?.clone())
}

struct Scoped {
    kernel: Kernel,
    _fixture: tempfile::TempDir,
}
fn scoped_kernel(
    files: &[(&str, String)],
    controller: Arc<dyn airk_kernel::arbiter::NativeController>,
) -> Result<Scoped> {
    let fixture = tempfile::tempdir()?;
    let root = fixture.path().join("source");
    for (path, text) in files {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().context("fixture parent")?)?;
        std::fs::write(path, text)?;
    }
    let base = Kernel::open(&root, Some(fixture.path().join("data")))?;
    base.configure_vectors(false)?;
    base.index()?;
    let authority = crate::scoped_observation_tests::authority(&base)?;
    let kernel = base.with_selection_reads(authority.clone())?;
    let reference = ScopedReadRef {
        schema: 1,
        lease_generation: 1,
        index_id: authority.index_id,
        selection: airk_core::scoped_reads::SelectionScopeRef {
            owner: airk_core::scoped_reads::SelectionOwnerKind::Window,
            grant_nonce: airk_core::identity::Incarnation::from_bytes([10; 16]),
            service_instance: airk_core::identity::Incarnation::from_bytes([11; 16]),
            scope_identity: airk_core::identity::Sha256Digest::from_bytes([12; 32]),
            catalog_revision: airk_core::index_catalog::CatalogRevision::try_from(5_u64).unwrap(),
        },
        fingerprint: kernel.scoped_read_fingerprint()?.unwrap(),
    };
    let worker = WorkerIdentity {
        service: Incarnation::from_bytes([8; 16]),
        worker: Incarnation::from_bytes([9; 16]),
        project: authority.backing,
        fencing_generation: 1,
    };
    Ok(Scoped {
        kernel: kernel.with_native_arbiter(worker, reference, controller)?,
        _fixture: fixture,
    })
}

/// The span row a `same_as` object names: the row of that record at that
/// pointer.
fn named_row<'a>(packet: &'a Value, name: &Value) -> Option<&'a Value> {
    packet["records"]
        .as_array()?
        .iter()
        .find(|record| record["id"] == name["record"])?["canonical_result"]["spans"]
        .as_array()?
        .iter()
        .find(|row| row["pointer"] == name["pointer"])
}

/// The text a span row stands for, as far as the reply says it: its own
/// text, or the text of the row it names. A row marked `within` is a part
/// of the text returned here.
fn row_text<'a>(packet: &'a Value, row: &'a Value) -> Option<&'a str> {
    match row.get("same_as") {
        Some(name) => named_row(packet, name)?["text"].as_str(),
        None => row["text"].as_str(),
    }
}

/// Every span of a reply's record projection is a hit the same canonical
/// read returns directly: same location and content hash at the same
/// publication, and the hit's text.
/// - A row with `text` and no `overlaps` carries the hit's text, possibly
///   shortened.
/// - A row with `overlaps` carries the hit's text without the bytes other
///   rows carry: its text and those bytes add up to the hit's text.
/// - A row with `same_as` carries no text and names a row of the reply that
///   does. That row's text is the hit's text, or holds it when the row is
///   marked `within`.
async fn assert_projection_matches_direct_read(
    tools: &mut local::LocalTools,
    packet: &Value,
    record: &Value,
) -> Result<()> {
    let operation = record["operation"].as_str().context("record operation")?;
    let tool = format!("airk_{operation}");
    let projection = &record["canonical_result"];
    let mut args = record["arguments"].as_object().unwrap().clone();
    args.insert("response_mode".into(), json!("full"));
    // Answer owns saved-source verification on its complete outer result.
    // Its canonical inner reader explicitly disables per-read verification.
    args.insert("verification".into(), json!("off"));
    if matches!(operation, "read_file" | "context") && !args.contains_key("revision") {
        args.insert("revision".into(), projection["revision"].clone());
    }
    let direct = body(&tools.call(tool.clone(), args).await?)?;
    let spans = projection["spans"].as_array().context("projection spans")?;
    for span in spans {
        // The span names the hit's pointer inside the read body.
        let hit = span["pointer"]
            .as_str()
            .and_then(|pointer| direct.pointer(pointer))
            .cloned()
            .with_context(|| format!("{tool}: span names no hit of its read: {span}"))?;
        let citation = &hit["citation"];
        ensure!(
            citation["path"] == span["path"]
                && citation["start_line"] == span["start_line"]
                && citation["end_line"] == span["end_line"]
                && citation["content_hash"] == span["content_hash"],
            "{tool}: projected span is not at its hit's location: {span}"
        );
        let full = hit["text"].as_str().context("hit text")?;
        let same_text = match (span.get("same_as"), span["text"].as_str()) {
            // A repeat: no text of its own, and a named row that carries it.
            (Some(name), None) => {
                let carrier = named_row(packet, name)
                    .with_context(|| format!("{tool}: a repeat names no row: {span}"))?;
                let carried = carrier["text"]
                    .as_str()
                    .with_context(|| format!("{tool}: a repeat names a row without text"))?;
                let whole = carrier["truncated"] != true && carrier.get("overlaps").is_none();
                match (span["within"] == true, whole) {
                    (false, true) => carried == full,
                    (false, false) => full.contains(carried.trim_end_matches('…')),
                    (true, true) => carried.contains(full),
                    (true, false) => true,
                }
            }
            (None, Some(text)) => match span["overlaps"].as_array() {
                // Trimmed of what other rows carry: the bytes add up.
                Some(overlaps) => {
                    let trimmed: u64 = overlaps
                        .iter()
                        .filter_map(|overlap| overlap["bytes"].as_u64())
                        .sum();
                    full.contains(text.trim_end_matches('…'))
                        && (span["truncated"] == true
                            || text.len() as u64 + trimmed == full.len() as u64)
                }
                None if span["truncated"] == true => full.starts_with(text.trim_end_matches('…')),
                None => full == text,
            },
            _ => false,
        };
        ensure!(
            same_text,
            "{tool}: projected span does not stand for its hit's text: {span}"
        );
    }
    ensure!(
        projection["hits_total"].as_u64() >= Some(spans.len() as u64),
        "{tool}: projection lost its hit count"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn disabled_native_answer_expands_real_scoped_evidence_and_preserves_full_canonical_results()
-> Result<()> {
    let text: String = (1..=800).map(|line| if line == 400 {"amber_marker owns the original finite request deadline.\n".into()} else {format!("Line {line}: the delivery remains bound to its original source authority and unchanged publication.\n")}).collect();
    let fixture = scoped_kernel(
        &[
            ("public/policy.txt", text),
            (
                "private/secret.txt",
                "amber_marker must never expose SECRET_PRIVATE_ARCHIVE\n".into(),
            ),
        ],
        Arc::new(Disabled),
    )?;
    let kernel = fixture.kernel.clone();
    let mut tools = local::LocalTools::new(kernel.clone()).await?;
    let reply = tools
        .call(
            "airk_answer".into(),
            request("amber_marker", BudgetV3::default())?,
        )
        .await?;
    let packet = body(&reply)?;
    assert_eq!(packet["schema"], CONTEXT_SCHEMA);
    assert_eq!(packet["applications"][0]["availability"], "disabled");
    assert!(packet["work"]["calls"].as_u64().unwrap() >= 2, "{packet}");
    let records = packet["records"].as_array().unwrap();
    assert!(records.iter().any(|record| record["operation"] == "search"));
    assert!(packet.to_string().contains("amber_marker owns"), "{packet}");
    assert!(!packet.to_string().contains("SECRET_PRIVATE_ARCHIVE"));
    assert!(!packet.to_string().contains("private/secret.txt"));
    for record in records {
        assert_projection_matches_direct_read(&mut tools, &packet, record).await?;
    }
    // Each text is carried once: no two rows of the reply have the same text.
    let texts: Vec<&str> = records
        .iter()
        .flat_map(|record| {
            record["canonical_result"]["spans"]
                .as_array()
                .into_iter()
                .flatten()
        })
        .filter_map(|row| row["text"].as_str())
        .collect();
    let distinct: std::collections::BTreeSet<&str> = texts.iter().copied().collect();
    assert_eq!(texts.len(), distinct.len(), "{packet}");
    assert!(serde_json::to_vec(&reply)?.len() <= v3::MAX_ANSWER_RESPONSE_BYTES);
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
            request("original authority", BudgetV3::default())?,
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
        airk_core::arbiter::context_action::ContextRequestV2 {
            schema: airk_core::arbiter::context_action::REQUEST_SCHEMA_V2.into(),
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
        let fixture = scoped_kernel(&[("public/eof.txt", text)], Arc::new(Disabled))?;
        let mut tools = local::LocalTools::new(fixture.kernel.clone()).await?;
        let packet = body(
            &tools
                .call("airk_answer".into(), request(query, BudgetV3::default())?)
                .await?,
        )?;
        assert!(
            packet["parts"]
                .as_array()
                .unwrap()
                .iter()
                .all(|part| part["stop"] != "refused"),
            "{name}: EOF must clamp in owning canonical context: {packet}"
        );
        let expansion = packet["records"]
            .as_array()
            .unwrap()
            .iter()
            .find(|record| record["operation"] == "context")
            .context("missing canonical context expansion")?;
        assert_eq!(expansion["arguments"]["before"], 0);
        let line = expansion["arguments"]["line"].as_u64().unwrap();
        assert!(line <= expected_line as u64);
        let span = expansion["canonical_result"]["spans"]
            .as_array()
            .unwrap()
            .iter()
            .find(|span| row_text(&packet, span).is_some_and(|t| t.contains("amber_eof")))
            .context("the expansion keeps the evidence line")?;
        assert_eq!(span["end_line"], expected_line, "{name}: {span}");
        assert_projection_matches_direct_read(&mut tools, &packet, expansion).await?;
        tools.close().await?;
    }
    Ok(())
}

/// The Host-captured original attempt a worker receives with the query.
fn wire_attempt(id: u8, remaining_ns: u64) -> airk_core::read_attempt::ReadAttemptBudget {
    airk_core::read_attempt::ReadAttemptBudget {
        schema: 1,
        request_id: Incarnation::from_bytes([id; 16]),
        clock: wire_clock(),
        expires_monotonic_ns: u64::MAX,
        remaining_ns,
        native_retry_expires_monotonic_ns: None,
    }
}
fn wire_clock() -> airk_core::read_attempt::ReadClock {
    if cfg!(windows) {
        airk_core::read_attempt::ReadClock::WindowsPerformanceCounter
    } else {
        airk_core::read_attempt::ReadClock::UnixMonotonic
    }
}
fn loop_model() -> airk_kernel::arbiter::LoopModel {
    airk_kernel::arbiter::LoopModel {
        model: device::SelectedModel::Arbiter,
        device: v3::ExecutionDevice::Cuda,
        precision: Some(device::DevicePrecision::Float16),
    }
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
    fn focus_clock(&self) -> Result<Option<(airk_core::read_attempt::ReadClock, u64)>> {
        Ok(Some((wire_clock(), 1000)))
    }
    fn card_model(
        &self,
        _: WorkerIdentity,
        _: ScopedReadRef,
        _: &airk_core::evidence::RevisionEnvelope,
    ) -> Result<Option<airk_kernel::arbiter::LoopModel>> {
        Ok(Some(loop_model()))
    }
    fn score_cards(
        &self,
        _: NativeCardRequest,
        deadline: Instant,
    ) -> Result<Option<v3::CardAdvice>> {
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
        let fault = Arc::new(OptionalFault {
            scope_refusal,
            calls: std::sync::atomic::AtomicUsize::new(0),
            observed_deadline: std::sync::Mutex::new(None),
        });
        let files: Vec<_> = (0..3)
            .map(|i| {
                (
                    [
                        "public/policy-0.txt",
                        "public/policy-1.txt",
                        "public/policy-2.txt",
                    ][i],
                    format!("amber_callback owns indexed policy {i}.\n"),
                )
            })
            .collect();
        let fixture = scoped_kernel(&files, fault.clone())?;
        let mut tools = local::LocalTools::new(fixture.kernel.clone()).await?;
        // Long enough to pass the 5 s dispatch reserve and admit a card call.
        let original = Instant::now() + Duration::from_secs(60);
        // The attempt ends no later than the original deadline.
        let remaining = original
            .saturating_duration_since(Instant::now())
            .as_nanos() as u64;
        let arguments = request(
            "1. which policy does amber_callback own\n2. where is amber_callback indexed?",
            BudgetV3 {
                deadline_ms: 120_000,
                ..BudgetV3::default()
            },
        )?;
        let reply = tools
            .call_with_read_context(
                "airk_answer".into(),
                arguments.clone(),
                crate::read_attempt::ReadAttemptContext::new(original, Default::default())
                    .with_wire_attempt(wire_attempt(31, remaining))?
                    .capture_transport_attempt(wire_attempt(31, remaining))?,
            )
            .await?;
        assert!(
            fault
                .observed_deadline
                .lock()
                .unwrap()
                .is_some_and(|observed| observed <= original),
            "canonical Answer must intersect serialized 120 s with original 60 s before the native callback: {:?} {:?}",
            fault.calls,
            reply
        );
        assert_eq!(
            fault.calls.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "an unknown card outcome blocks later model work and is never replayed"
        );
        if scope_refusal {
            assert_eq!(
                reply.is_error,
                Some(true),
                "typed native authority refusal must never become a partial successful reply"
            );
        } else {
            let packet = body(&reply)?;
            assert_eq!(packet["schema"], CONTEXT_SCHEMA);
            assert!(
                packet["degradations"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|item| item["kind"] == "model_outcome_unknown"),
                "{packet}"
            );
            assert!(packet["decision_log"]["work_unknown"].as_u64().unwrap() >= 1);
            assert!(
                packet["decisions"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|decision| decision["work_unknown"] == true
                        && decision["source"]["kind"] == "unavailable")
            );
            assert!(
                packet["records"].as_array().unwrap().iter().any(|record| {
                    record["operation"] == "search"
                        && record["canonical_result"]["spans"]
                            .as_array()
                            .is_some_and(|spans| spans.len() >= 2)
                }),
                "the canonical search evidence is kept: {packet}"
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

/// Accepts the first card call's top cards, then withdraws the policy (a new
/// configuration revision) at the chosen application observation after the
/// call returned. `revoke_after` = 0 never withdraws.
struct RevokingScorer {
    returned: std::sync::atomic::AtomicBool,
    calls: std::sync::atomic::AtomicUsize,
    after_return: std::sync::atomic::AtomicUsize,
    revoke_after: usize,
}
impl airk_kernel::arbiter::NativeController for RevokingScorer {
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
        current.availability = Availability::Ready;
        current.profile = Some(Sha256Digest::from_bytes([33; 32]));
        if self.returned.load(std::sync::atomic::Ordering::SeqCst) {
            let observation = self
                .after_return
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
                + 1;
            if self.revoke_after > 0 && observation >= self.revoke_after {
                current.scope.configuration_revision = 2;
                current.availability = Availability::Disabled;
                current.profile = None;
                current.reason = "Policy was disabled after the native callback.".into();
            }
        }
        Ok(current)
    }
    fn focus_clock(&self) -> Result<Option<(airk_core::read_attempt::ReadClock, u64)>> {
        Ok(Some((wire_clock(), 1000)))
    }
    fn card_model(
        &self,
        _: WorkerIdentity,
        _: ScopedReadRef,
        _: &airk_core::evidence::RevisionEnvelope,
    ) -> Result<Option<airk_kernel::arbiter::LoopModel>> {
        Ok(Some(loop_model()))
    }
    fn score_cards(
        &self,
        request: NativeCardRequest,
        _: Instant,
    ) -> Result<Option<v3::CardAdvice>> {
        if self.returned.load(std::sync::atomic::Ordering::SeqCst) {
            // One model call is enough; later ones are declined, not
            // dispatched and not charged.
            return Ok(None);
        }
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let tokens = 100 * request.attribution.len();
        let segments = request
            .segments
            .iter()
            .map(|segment| {
                let count = segment.cards.len();
                let scores: Vec<f32> = (0..count).map(|index| index as f32).collect();
                let top = count - 1;
                let digest = segment.cards[top].text_digest().unwrap();
                v3::SegmentOutcome {
                    ordinal: segment.ordinal,
                    raw_scores: scores,
                    outcome: Outcome::Accepted {
                        decision: if segment.multiple {
                            Decision::LoopCards(vec![digest])
                        } else {
                            Decision::LoopCard(digest)
                        },
                    },
                    rule: None,
                    margin: Some(if count > 1 { 1.0 } else { 0.0 }),
                }
            })
            .collect();
        let advice = v3::CardAdvice {
            schema: v3::CARD_ADVICE_SCHEMA.into(),
            task: request.task,
            profile: Sha256Digest::from_bytes([33; 32]),
            runtime: format!(
                "onnxruntime-cuda/1.30.0/device-execution/{}",
                "a".repeat(64)
            ),
            compiler: Sha256Digest::from_bytes([34; 32]),
            calibration: Sha256Digest::from_bytes([35; 32]),
            policy: Sha256Digest::from_bytes([36; 32]),
            task_policy: request.task_policy,
            scope: request.scope.clone(),
            input_digest: request.input_digest()?,
            card_slot: request.segments[0].card_slot,
            reserved_input_tokens: request.remaining_input_tokens,
            observed_input_tokens: Some(tokens),
            attribution: request
                .attribution
                .iter()
                .map(|charge| v3::TokenAttribution {
                    tokens: 100,
                    ..*charge
                })
                .collect(),
            work: Work {
                input_tokens: tokens,
                compile_us: Some(0),
                inference_us: Some(0),
                ..Work::default()
            },
            segments,
        };
        // Deterministic event barrier: the policy changes only after this
        // exact native call returned, before the caller applies its result.
        self.returned
            .store(true, std::sync::atomic::Ordering::SeqCst);
        Ok(Some(advice))
    }
}
async fn revoking_answer(revoke_after: usize) -> Result<(CallToolResult, Arc<RevokingScorer>)> {
    let controller = Arc::new(RevokingScorer {
        returned: false.into(),
        calls: 0.into(),
        after_return: 0.into(),
        revoke_after,
    });
    let fixture = scoped_kernel(
        &[
            (
                "public/policy.txt",
                "amber_revoked AMBER_PRIVATE_MODEL_HISTORY owns this source rule.\n".into(),
            ),
            ("private/secret.txt", "SECRET_NOT_GRANTED\n".into()),
        ],
        controller.clone(),
    )?;
    let mut tools = local::LocalTools::new(fixture.kernel.clone()).await?;
    let context = crate::read_attempt::ReadAttemptContext::new(
        Instant::now() + Duration::from_secs(60),
        Default::default(),
    )
    .with_wire_attempt(wire_attempt(32, 60_000_000_000))?
    .capture_transport_attempt(wire_attempt(32, 60_000_000_000))?;
    let reply = tools
        .call_with_read_context(
            "airk_answer".into(),
            request(
                "1. which rule does amber_revoked own\n2. where is amber_revoked defined?",
                BudgetV3::default(),
            )?,
            context,
        )
        .await?;
    if reply.is_error == Some(true) {
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
        ensure!(
            !ordinary["hits"].as_array().unwrap().is_empty(),
            "a refused Answer must not lose the ordinary primitive"
        );
    }
    tools.close().await?;
    Ok((reply, controller))
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn policy_withdrawal_during_joint_wait_refuses_stale_answer_without_replay_or_primitive_loss()
-> Result<()> {
    // Without a withdrawal the accepted card work is delivered; count the
    // application observations after the call returned, so the cuts below
    // are the first one (immediately after the wait) and the last one (at
    // delivery), whatever the loop's shape.
    let (reply, controller) = revoking_answer(0).await?;
    let packet = body(&reply)?;
    assert!(packet["decision_log"]["card_calls"].as_u64().unwrap() >= 1);
    let after = controller
        .after_return
        .load(std::sync::atomic::Ordering::SeqCst);
    assert!(after >= 1, "the delivery fence observes the application");
    for revoke_after in [1, after] {
        let (reply, controller) = revoking_answer(revoke_after).await?;
        assert_eq!(reply.is_error, Some(true), "cut {revoke_after}");
        let refusal: Value = serde_json::from_str(&reply.content[0].as_text().unwrap().text)?;
        assert_eq!(refusal["error"]["code"], "arbiter_context_action_stale");
        assert_eq!(refusal["error"]["retryable"], false);
        assert_eq!(refusal["error"]["query_replay_safe"], false);
        assert!(!serde_json::to_string(&reply)?.contains("AMBER_PRIVATE_MODEL_HISTORY"));
        assert_eq!(
            controller.calls.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "a stale model result is never replayed"
        );
    }
    Ok(())
}

/// Change-log entry 67: the scorer of `RevokingScorer` (never withdrawing),
/// and a newer publication of the same index lands once its first card call
/// has returned, as an Index's next step would.
struct RepublishingScorer {
    scorer: RevokingScorer,
    root: std::sync::OnceLock<std::path::PathBuf>,
}
impl airk_kernel::arbiter::NativeController for RepublishingScorer {
    fn application(
        &self,
        worker: WorkerIdentity,
        authority: ScopedReadRef,
        publication: &airk_core::evidence::RevisionEnvelope,
    ) -> Result<IndexApplication> {
        self.scorer.application(worker, authority, publication)
    }
    fn focus_clock(&self) -> Result<Option<(airk_core::read_attempt::ReadClock, u64)>> {
        self.scorer.focus_clock()
    }
    fn card_model(
        &self,
        worker: WorkerIdentity,
        authority: ScopedReadRef,
        publication: &airk_core::evidence::RevisionEnvelope,
    ) -> Result<Option<airk_kernel::arbiter::LoopModel>> {
        self.scorer.card_model(worker, authority, publication)
    }
    fn score_cards(
        &self,
        request: NativeCardRequest,
        deadline: Instant,
    ) -> Result<Option<v3::CardAdvice>> {
        let first = !self
            .scorer
            .returned
            .load(std::sync::atomic::Ordering::SeqCst);
        let advice = self.scorer.score_cards(request, deadline)?;
        if first {
            let root = self.root.get().context("fixture root not set")?;
            std::fs::write(
                root.join("source/public/later.txt"),
                "amber_moved landed in a newer publication.\n",
            )?;
            Kernel::open(root.join("source"), Some(root.join("data")))?.index()?;
        }
        Ok(advice)
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn publication_landing_after_model_work_is_typed_publication_advanced_without_replay()
-> Result<()> {
    let controller = Arc::new(RepublishingScorer {
        scorer: RevokingScorer {
            returned: false.into(),
            calls: 0.into(),
            after_return: 0.into(),
            revoke_after: 0,
        },
        root: Default::default(),
    });
    let fixture = scoped_kernel(
        &[(
            "public/policy.txt",
            "amber_moved AMBER_PRIVATE_MODEL_HISTORY owns this source rule.\n".into(),
        )],
        controller.clone(),
    )?;
    controller
        .root
        .set(fixture._fixture.path().to_path_buf())
        .unwrap();
    let mut tools = local::LocalTools::new(fixture.kernel.clone()).await?;
    let context = crate::read_attempt::ReadAttemptContext::new(
        Instant::now() + Duration::from_secs(60),
        Default::default(),
    )
    .with_wire_attempt(wire_attempt(33, 60_000_000_000))?
    .capture_transport_attempt(wire_attempt(33, 60_000_000_000))?;
    let reply = tools
        .call_with_read_context(
            "airk_answer".into(),
            request(
                "1. which rule does amber_moved own\n2. where is amber_moved defined?",
                BudgetV3::default(),
            )?,
            context,
        )
        .await?;
    assert_eq!(reply.is_error, Some(true), "{reply:?}");
    let refusal: Value = serde_json::from_str(&reply.content[0].as_text().unwrap().text)?;
    // Distinct from a failure and from a refusal, and never a retry hint.
    assert_eq!(
        refusal["error"]["code"], "publication_advanced",
        "{refusal}"
    );
    assert_eq!(refusal["error"]["retryable"], false);
    assert_eq!(refusal["error"]["query_replay_safe"], false);
    assert!(refusal.get("action").is_none() && refusal["error"].get("action").is_none());
    assert!(!serde_json::to_string(&reply)?.contains("AMBER_PRIVATE_MODEL_HISTORY"));
    assert_eq!(
        controller
            .scorer
            .calls
            .load(std::sync::atomic::Ordering::SeqCst),
        1,
        "model work is never replayed"
    );
    tools.close().await?;
    Ok(())
}

/// Fields of `left` and `right` whose values differ, by JSON pointer, at
/// every depth (an object key on one side only counts as a difference).
fn differing(left: &Value, right: &Value, at: String, out: &mut Vec<String>) {
    match (left, right) {
        (Value::Object(a), Value::Object(b)) => {
            let keys: std::collections::BTreeSet<&String> = a.keys().chain(b.keys()).collect();
            for key in keys {
                match (a.get(key), b.get(key)) {
                    (Some(x), Some(y)) => differing(x, y, format!("{at}/{key}"), out),
                    _ => out.push(format!("{at}/{key}")),
                }
            }
        }
        (Value::Array(a), Value::Array(b)) if a.len() == b.len() => {
            for (index, (x, y)) in a.iter().zip(b).enumerate() {
                differing(x, y, format!("{at}/{index}"), out);
            }
        }
        _ if left != right => out.push(at),
        _ => {}
    }
}

/// Change-log entry 54: two runs of each read the Answer loop dispatches, at
/// one publication, differ only in Core's classified per-read status. A new
/// volatile field fails here until it is classified.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn answer_reads_vary_only_in_classified_status_fields() -> Result<()> {
    let fixture = scoped_kernel(
        &[(
            "public/policy.md",
            "# amber_marker policy\nThe amber_marker keeps its original deadline.\n".into(),
        )],
        Arc::new(Disabled),
    )?;
    let mut tools = local::LocalTools::new(fixture.kernel.clone()).await?;
    let mut observed = 0;
    for (tool, arguments) in [
        (
            "airk_search",
            json!({"query":"amber_marker","limit":8,"max_bytes":6144,"full_chunks":false}),
        ),
        (
            "airk_context",
            json!({"path":"public/policy.md","line":1,"before":0,"after":40,"max_bytes":3072}),
        ),
        (
            "airk_outline",
            json!({"path":"public/policy.md","limit":40,"max_bytes":4096,"compact":false}),
        ),
    ] {
        // The arguments `workbench_read` adds for an Answer's inner read.
        let mut arguments = arguments;
        arguments["response_mode"] = json!("full");
        arguments["include_status"] = json!(true);
        arguments["verification"] = json!("off");
        let arguments = arguments.as_object().context("arguments")?.clone();
        let first = body(&tools.call(tool.into(), arguments.clone()).await?)?;
        let second = body(&tools.call(tool.into(), arguments).await?)?;
        let mut changed = Vec::new();
        differing(&first, &second, String::new(), &mut changed);
        assert!(
            changed.iter().any(|field| field == "/request_id"),
            "{tool}: {changed:?}"
        );
        for field in &changed {
            let top = field
                .trim_start_matches('/')
                .split('/')
                .next()
                .unwrap_or_default();
            assert!(
                v3::VOLATILE_READ_FIELDS.contains(&top),
                "{tool}: unclassified per-read field {field}: classify it in Core VOLATILE_READ_FIELDS"
            );
        }
        observed += 1;
    }
    assert_eq!(observed, 3);
    tools.close().await?;
    Ok(())
}

/// The reply fields that are timing or transport identity, the only
/// normalisation between two runs of one Answer on one session.
fn without_timing(mut packet: Value) -> Value {
    let object = packet.as_object_mut().unwrap();
    object.remove("request_id");
    object.remove("elapsed_ms");
    for key in ["queue_us", "compile_us", "inference_us", "policy_us"] {
        object["work"].as_object_mut().unwrap().remove(key);
    }
    object["decision_log"]
        .as_object_mut()
        .unwrap()
        .remove("timings");
    if let Some(status) = object.get_mut("read_status").and_then(Value::as_object_mut) {
        for field in status.values_mut() {
            if let Some(field) = field.as_object_mut() {
                field.remove("observed_unix_ms");
            }
        }
    }
    if let Some(verification) = object
        .get_mut("source_verification")
        .and_then(Value::as_object_mut)
    {
        verification.remove("observed_from_unix_ms");
        verification.remove("observed_to_unix_ms");
        for source in verification
            .get_mut("sources")
            .and_then(Value::as_array_mut)
            .into_iter()
            .flatten()
        {
            if let Some(source) = source.as_object_mut() {
                source.remove("observed_from_unix_ms");
                source.remove("observed_to_unix_ms");
            }
        }
    }
    for part in object["parts"].as_array_mut().unwrap() {
        // A part's share of the time left before the deadline.
        part.as_object_mut().unwrap().remove("deadline_share_ms");
    }
    // The receipt's selection digest covers the reply, timing included;
    // recompute it over the normalized reply.
    if packet.get("source_verification").is_some() {
        packet["source_verification"]["selection_digest"] =
            serde_json::to_value(airk_kernel::verification::selection_digest(&packet).unwrap())
                .unwrap();
    }
    packet
}

/// Change-log entry 54: one Answer request twice on one session gives the
/// same records (ids, read sizes, digests), basis, decisions and log digest.
/// The reply carries its reads' status once, at reply level, and a terse
/// reply keeps it only with `include_status`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn one_answer_twice_on_one_session_gives_byte_equal_records_and_decisions() -> Result<()> {
    let text: String = (1..=120)
        .map(|line| {
            if line % 30 == 0 {
                format!("amber_marker {line} keeps the original deadline.\n")
            } else {
                format!("Line {line}: the delivery stays bound to its publication.\n")
            }
        })
        .collect();
    let fixture = scoped_kernel(
        &[
            ("public/policy.md", text.clone()),
            ("public/other.md", text.replace("Line", "Other line")),
        ],
        Arc::new(Disabled),
    )?;
    let mut tools = local::LocalTools::new(fixture.kernel.clone()).await?;
    let ask = request("amber_marker", BudgetV3::default())?;
    let first = body(&tools.call("airk_answer".into(), ask.clone()).await?)?;
    let second = body(&tools.call("airk_answer".into(), ask.clone()).await?)?;
    assert!(
        first["records"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["operation"] != "search"),
        "the Answer reads structurally: {first}"
    );
    let status = first["read_status"]
        .as_object()
        .context("full reply carries read_status")?;
    assert!(status.contains_key("freshness"), "{first}");
    let records = first["records"].to_string();
    for field in v3::VOLATILE_READ_FIELDS {
        assert!(
            !records.contains(&format!("\"{field}\"")),
            "{field} in records"
        );
    }
    let (left, right) = (without_timing(first), without_timing(second));
    let mut changed = Vec::new();
    differing(&left, &right, String::new(), &mut changed);
    assert!(changed.is_empty(), "two runs differ at {changed:?}");
    let mut terse = ask.clone();
    terse.insert("response_mode".into(), json!("terse"));
    let reply = body(&tools.call("airk_answer".into(), terse.clone()).await?)?;
    assert!(reply.get("read_status").is_none(), "{reply}");
    terse.insert("include_status".into(), json!(true));
    let reply = body(&tools.call("airk_answer".into(), terse).await?)?;
    assert!(reply["read_status"].is_object(), "{reply}");
    tools.close().await?;
    Ok(())
}
