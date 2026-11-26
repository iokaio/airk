// SPDX-License-Identifier: Apache-2.0
//! Shared synthetic fixtures for the Answer /3 loop tests, and the driver's
//! end-to-end tests: canonical replies shaped like the native reads, with an
//! injected owner, clock and reader; no model or transport qualification.
use super::*;
use airk_core::{
    DocumentLocation,
    evidence::RevisionEnvelope,
    identity::{Incarnation, RevisionToken},
    index_catalog::*,
    read_attempt::{ReadAttemptBudget, ReadClock},
    scoped_reads::SourceDescriptor,
    service::{ScopedReadRef, WorkerIdentity},
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};

pub(crate) fn scope() -> ScopeBinding {
    ScopeBinding {
        installation: Sha256Digest::from_bytes([1; 32]),
        catalog: Incarnation::from_bytes([2; 16]),
        index_id: IndexId::from_identity(Incarnation::from_bytes([3; 16])),
        authorization: Sha256Digest::from_bytes([4; 32]),
        configuration_revision: 1,
        publication: RevisionEnvelope::new(
            "fixture".into(),
            RevisionToken {
                incarnation: Incarnation::from_bytes([5; 16]),
                generation: 1,
                revision: Sha256Digest::from_bytes([6; 32]),
            },
        ),
    }
}

/// A hit whose content hash is a real digest, as native Focus requires.
pub(crate) fn hit(path: &str, start: usize, end: usize, text: &str) -> Value {
    let hash = framed_digest(b"fixture-hash", &[path.as_bytes(), text.as_bytes()]);
    json!({"citation":{"path":path,"content_hash":hash.to_string(),"start_line":start,"end_line":end,
        "location":DocumentLocation::Lines {start,end}},"text":text,"truncated":false})
}

pub(crate) fn reply(scope: &ScopeBinding, hits: Vec<Value>) -> Value {
    json!({"publication":scope.publication,"generation":1,"collection":"fixture","truncated":false,"hits":hits})
}

fn digest(byte: u8) -> Sha256Digest {
    Sha256Digest::from_bytes([byte; 32])
}

/// A native owner with no Context model and no Focus.
pub(crate) struct Owner {
    pub(crate) revision: AtomicU64,
    /// Application observations before the configuration revision moves.
    pub(crate) change_after: AtomicU64,
    pub(crate) observed: AtomicU64,
}
impl Owner {
    pub(crate) fn new() -> Self {
        Self {
            revision: AtomicU64::new(1),
            change_after: AtomicU64::new(0),
            observed: AtomicU64::new(0),
        }
    }
}
impl NativeController for Owner {
    fn application(
        &self,
        _: WorkerIdentity,
        authority: ScopedReadRef,
        publication: &airk_core::evidence::RevisionEnvelope,
    ) -> Result<IndexApplication> {
        let seen = self.observed.fetch_add(1, Ordering::AcqRel) + 1;
        let after = self.change_after.load(Ordering::Acquire);
        if after > 0 && seen > after {
            self.revision.store(2, Ordering::Release);
        }
        Ok(IndexApplication {
            scope: ScopeBinding {
                installation: digest(28),
                catalog: Incarnation::from_bytes([29; 16]),
                index_id: authority.index_id,
                authorization: authority.fingerprint,
                configuration_revision: self.revision.load(Ordering::Acquire),
                publication: publication.clone(),
            },
            profile: None,
            availability: Availability::Disabled,
            reason: "Context is disabled in this fixture".into(),
        })
    }
    fn focus_clock(&self) -> Result<Option<(ReadClock, u64)>> {
        Ok(Some((ReadClock::UnixMonotonic, 1000)))
    }
}

/// The token cost of one synthetic Focus pair.
pub(crate) const PAIR_TOKENS: usize = 512;

/// A ready synthetic Focus profile beside an application.
pub(crate) fn focus_ready(
    mut application: IndexApplication,
) -> airk_core::arbiter::FocusApplication {
    let identity = airk_core::arbiter::FocusIdentity {
        profile: digest(30),
        runtime: RUNTIME.into(),
        compiler: digest(31),
        calibration: digest(32),
        task_policy: digest(33),
    };
    application.profile = Some(identity.profile);
    application.availability = Availability::Ready;
    application.reason = "Synthetic Focus owner, not a qualified model".into();
    airk_core::arbiter::FocusApplication {
        application,
        identity: Some(identity),
        limits: Some(airk_core::arbiter::FocusProfileLimits {
            max_candidates: v3::MAX_FOCUS_BATCH_PAIRS,
            pair_input_tokens: PAIR_TOKENS,
            max_input_tokens: 8192,
        }),
    }
}

/// Synthetic batched Focus: every pair is relevant, the first most, and the
/// whole reservation is observed.
pub(crate) fn focus_scores(
    request: airk_core::native_loop::NativeFocusRequestV3,
    identity: airk_core::arbiter::FocusIdentity,
) -> Result<Option<airk_core::native_loop::NativeFocusResponseV3>> {
    use v3::{FOCUS_BATCH_SCHEMA, FocusBatchReceipt, FocusPairScore};
    request.validate(
        request.worker,
        request.authority,
        ReadClock::UnixMonotonic,
        1000,
    )?;
    let pairs: Vec<FocusPairScore> = request
        .candidates
        .iter()
        .enumerate()
        .map(|(index, candidate)| FocusPairScore {
            candidate: index as u16,
            question: candidate.question,
            no_logit: 0.,
            yes_logit: 4.0 - index as f32 * 0.01,
        })
        .collect();
    let tokens = request.remaining_input_tokens;
    Ok(Some(airk_core::native_loop::NativeFocusResponseV3 {
        ranking: FocusBatchReceipt {
            schema: FOCUS_BATCH_SCHEMA.into(),
            scope_digest: v3::scope_digest(&request.scope)?,
            identity: Some(identity),
            availability: Availability::Ready,
            reason: "Synthetic batched scoring".into(),
            applied: true,
            input_digest: request.input_digest()?,
            order_digest: request.order_digest,
            questions_digest: v3::questions_digest(&request.questions)?,
            question_count: request.questions.len() as u8,
            candidate_count: request.candidates.len(),
            ranked: (0..request.candidates.len() as u16).collect(),
            work: Work {
                input_tokens: tokens,
                candidates: pairs.len(),
                ..Work::default()
            },
            pairs,
            reserved_input_tokens: tokens,
            observed_input_tokens: Some(tokens),
            attribution: request.attribution.clone(),
        },
    }))
}

/// The fixture owner with synthetic Focus ready; Context stays disabled.
pub(crate) struct Focused {
    pub(crate) owner: Arc<Owner>,
    pub(crate) batches: AtomicU64,
    /// The configuration moves while the first batch runs.
    pub(crate) stale: bool,
    /// How the synthetic native owner answers a batch.
    pub(crate) shape: FocusShape,
    /// The pairs each batch dispatched, in order.
    pub(crate) sizes: Mutex<Vec<usize>>,
}
/// The shapes a real service's batched Focus reply takes.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum FocusShape {
    /// Every pair scored and the ranking applied.
    Full,
    /// A batch above this many pairs fails before any reservation, as the
    /// service's one-turn `max_candidates` check did (change-log entry 53):
    /// the kernel sees an error and holds the call as unknown work.
    FailAbove(usize),
    /// The lane queued no pair: a known zero-work call that scored nothing.
    ScoredNone,
}
impl NativeController for Focused {
    fn application(
        &self,
        worker: WorkerIdentity,
        authority: ScopedReadRef,
        publication: &RevisionEnvelope,
    ) -> Result<IndexApplication> {
        self.owner.application(worker, authority, publication)
    }
    fn focus_application(
        &self,
        worker: WorkerIdentity,
        authority: ScopedReadRef,
        publication: &RevisionEnvelope,
    ) -> Result<Option<airk_core::arbiter::FocusApplication>> {
        Ok(Some(focus_ready(self.owner.application(
            worker,
            authority,
            publication,
        )?)))
    }
    fn focus_clock(&self) -> Result<Option<(ReadClock, u64)>> {
        Ok(Some((ReadClock::UnixMonotonic, 1000)))
    }
    fn focus_batch(
        &self,
        request: airk_core::native_loop::NativeFocusRequestV3,
        _: Instant,
    ) -> Result<Option<airk_core::native_loop::NativeFocusResponseV3>> {
        self.batches.fetch_add(1, Ordering::AcqRel);
        let identity = focus_ready(self.owner.application(
            request.worker,
            request.authority,
            &request.scope.publication,
        )?)
        .identity
        .context("identity")?;
        if self.stale {
            self.owner.revision.store(2, Ordering::Release);
        }
        let count = request.candidates.len();
        self.sizes.lock().unwrap().push(count);
        match self.shape {
            FocusShape::Full => focus_scores(request, identity),
            FocusShape::FailAbove(limit) => {
                ensure!(
                    count <= limit,
                    "Focus batch exceeds the qualified profile or its reservation"
                );
                focus_scores(request, identity)
            }
            FocusShape::ScoredNone => {
                let mut response = focus_scores(request, identity)?.context("synthetic reply")?;
                let ranking = &mut response.ranking;
                ranking.pairs.clear();
                ranking.applied = false;
                ranking.availability = Availability::Failed;
                ranking.reason = "The native lane queued no pair of this batch".into();
                ranking.observed_input_tokens = Some(0);
                ranking.work = Work::default();
                for charge in &mut ranking.attribution {
                    charge.tokens = 0;
                }
                Ok(Some(response))
            }
        }
    }
}

pub(crate) struct Fixture {
    pub(crate) _temp: tempfile::TempDir,
    pub(crate) kernel: crate::Kernel,
    /// The same native kernel before any read attempt is captured.
    pub(crate) unattempted: crate::Kernel,
    pub(crate) owner: Arc<Owner>,
    /// The native controller the kernel was bound with.
    pub(crate) controller: Arc<dyn NativeController>,
    pub(crate) publication: RevisionEnvelope,
}

pub(crate) fn fixture() -> Result<Fixture> {
    let owner = Arc::new(Owner::new());
    fixture_with(owner.clone(), owner)
}

mod transfer;

pub(crate) fn fixture_with<T: NativeController + 'static>(
    controller: Arc<T>,
    owner: Arc<Owner>,
) -> Result<Fixture> {
    fixture_with_sources(
        controller,
        owner,
        &[("public/guard.md", "# guard\nThe restart guard replays nothing.\n")],
    )
}

fn fixture_with_sources<T: NativeController + 'static>(
    controller: Arc<T>,
    owner: Arc<Owner>,
    files: &[(&str, &str)],
) -> Result<Fixture> {
    let temporary = tempfile::Builder::new().tempdir_in(std::env::temp_dir().canonicalize()?)?;
    let source = temporary.path().join("source");
    for (path, content) in files {
        let output = source.join(path);
        std::fs::create_dir_all(output.parent().context("fixture source parent")?)?;
        std::fs::write(output, content)?;
    }
    let kernel = crate::Kernel::open(&source, Some(temporary.path().join("data")))?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    let project = QueryProjectId::from_identity(Incarnation::from_bytes([1; 16]));
    let index = IndexId::from_identity(Incarnation::from_bytes([2; 16]));
    let source_id = SourceRootId::from_identity(Incarnation::from_bytes([3; 16]));
    let authority = crate::scoped_reads::SelectionReadAuthority {
        schema: 1,
        backing: airk_core::service::ProjectRegistration::new(
            kernel.root().to_str().unwrap().into(),
            kernel.data_dir().to_str().unwrap().into(),
            kernel.collection().into(),
        )?
        .binding,
        index_id: index,
        project_ids: vec![project],
        selection: airk_core::scoped_reads::SelectionScopeRef {
            owner: airk_core::scoped_reads::SelectionOwnerKind::Window,
            grant_nonce: airk_core::identity::Incarnation::from_bytes([10; 16]),
            service_instance: airk_core::identity::Incarnation::from_bytes([11; 16]),
            scope_identity: airk_core::identity::Sha256Digest::from_bytes([12; 32]),
            catalog_revision: airk_core::index_catalog::CatalogRevision::try_from(5_u64).unwrap(),
        },
        atoms: vec![ScopeAtom {
            source_root_id: source_id,
            kind: PathScope::Subtree,
            path: "public".into(),
        }]
        .into_iter()
        .map(|atom| (project, atom))
        .collect(),
        sources: vec![SourceDescriptor {
            source_root_id: source_id,
            original_root: kernel.root().to_str().unwrap().into(),
        }],
        membership_revisions: vec![(project, airk_core::index_catalog::CatalogRevision::INITIAL)],
        source_revision: airk_core::index_catalog::CatalogRevision::INITIAL,
        policy_epoch: 0,
        policy_digest: None,
    };
    let scoped = kernel.with_selection_reads(authority.clone())?;
    let reference = ScopedReadRef {
        schema: 1,
        lease_generation: 1,
        index_id: index,
        selection: airk_core::scoped_reads::SelectionScopeRef {
            owner: airk_core::scoped_reads::SelectionOwnerKind::Window,
            grant_nonce: airk_core::identity::Incarnation::from_bytes([10; 16]),
            service_instance: airk_core::identity::Incarnation::from_bytes([11; 16]),
            scope_identity: airk_core::identity::Sha256Digest::from_bytes([12; 32]),
            catalog_revision: airk_core::index_catalog::CatalogRevision::try_from(5_u64).unwrap(),
        },
        fingerprint: scoped.scoped_read_fingerprint()?.unwrap(),
    };
    let worker = WorkerIdentity {
        service: Incarnation::from_bytes([8; 16]),
        worker: Incarnation::from_bytes([9; 16]),
        project: authority.backing,
        fencing_generation: 1,
    };
    let unattempted = scoped.with_native_arbiter(worker, reference, controller.clone())?;
    let native = unattempted.with_context_attempt(ReadAttemptBudget {
        schema: 1,
        request_id: Incarnation::from_bytes([10; 16]),
        clock: ReadClock::UnixMonotonic,
        expires_monotonic_ns: 100_000_001_000,
        remaining_ns: 100_000_000_000,
        native_retry_expires_monotonic_ns: None,
    })?;
    let publication = native.arbiter_publication()?;
    Ok(Fixture {
        _temp: temporary,
        kernel: native,
        unattempted,
        owner,
        controller,
        publication,
    })
}

/// Canonical replies for every read the loop makes. `variant` changes the
/// excerpt texts, so a second round can add (or not add) new evidence.
pub(crate) fn canned(
    publication: &RevisionEnvelope,
    tool: &str,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let text = |key: &str| {
        arguments
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned()
    };
    let base =
        json!({"publication":publication,"generation":1,"collection":"fixture","truncated":false});
    let with = |key: &str, value: Value| {
        let mut body = base.clone();
        body[key] = value;
        body
    };
    let fact = |name: &str, path: &str, start: usize, end: usize| {
        json!({"text":name,"fact":{"kind":"function"},"citation":{"path":path,"start_line":start,"end_line":end,
            "content_hash":framed_digest(b"fixture-hash", &[path.as_bytes()]).to_string()}})
    };
    Ok(match tool {
        "search" => {
            let query = text("query");
            with(
                "hits",
                json!([
                    hit(
                        "crates/k/src/guard.rs",
                        10,
                        40,
                        &format!("pub fn restart_guard() {{\n    // {query}\n}}\n")
                    ),
                    hit(
                        "crates/k/tests/guard.rs",
                        1,
                        20,
                        &format!("fn restart_guard_test() {{ restart_guard(); }} // {query}")
                    ),
                    hit(
                        "docs/guard.md",
                        1,
                        12,
                        &format!("# Restart guard\nThe guard {query}.")
                    ),
                ]),
            )
        }
        "symbols" => with(
            "facts",
            json!([fact(&text("name"), "crates/k/src/guard.rs", 10, 40)]),
        ),
        "references" => with(
            "facts",
            json!([
                fact(&text("name"), "crates/k/src/caller.rs", 5, 9),
                fact(&text("name"), "crates/k/tests/guard.rs", 1, 20)
            ]),
        ),
        "read_file" => {
            let path = text("path");
            let start = arguments
                .get("start_line")
                .and_then(Value::as_u64)
                .unwrap_or(1) as usize;
            let end = arguments
                .get("end_line")
                .and_then(Value::as_u64)
                .unwrap_or(1) as usize;
            with(
                "hits",
                json!([hit(
                    &path,
                    start,
                    end,
                    &format!("fn restart_guard() {{}} // {path}")
                )]),
            )
        }
        "context" => {
            let path = text("path");
            let line = arguments.get("line").and_then(Value::as_u64).unwrap_or(1) as usize;
            with(
                "hits",
                json!([hit(
                    &path,
                    line,
                    line + 40,
                    &format!("context of {path} at {line}")
                )]),
            )
        }
        "outline" | "imports" => with(
            "facts",
            json!([fact("restart_guard", &text("path"), 10, 40)]),
        ),
        "file_info" => {
            let path = text("path");
            let lines = if path == "crates/k/CONTRACT.md" {
                30
            } else {
                0
            };
            json!({"publication":publication,"path":path,"indexed_lines":lines})
        }
        other => anyhow::bail!("fixture has no {other} read"),
    })
}

pub(crate) fn request(question: &str) -> v3::ContextRequestV3 {
    v3::ContextRequestV3 {
        schema: v3::REQUEST_SCHEMA_V3.into(),
        question: question.into(),
        budget: v3::BudgetV3::default(),
        continuation: None,
    }
}

/// A manual clock: reads advance it by a fixed step; no sleeps.
pub(crate) struct Clock {
    base: Instant,
    pub(crate) offset_ms: AtomicU64,
}
impl Clock {
    pub(crate) fn new() -> Self {
        Self {
            base: Instant::now(),
            offset_ms: AtomicU64::new(0),
        }
    }
    pub(crate) fn now(&self) -> Instant {
        self.base + Duration::from_millis(self.offset_ms.load(Ordering::Acquire))
    }
    pub(crate) fn advance(&self, ms: u64) {
        self.offset_ms.fetch_add(ms, Ordering::AcqRel);
    }
}

/// Rewrites a canned reply before the loop sees it.
pub(crate) type Alter = dyn Fn(&str, &Map<String, Value>, Value) -> Result<Value> + Sync;

/// Run one Answer with a reader built from `canned`, `step_ms` of clock per read.
pub(crate) fn run(
    fixture: &Fixture,
    request: v3::ContextRequestV3,
    step_ms: u64,
    alter: &Alter,
) -> Result<(AnswerOutcome, Vec<AnswerProgress>, Vec<String>)> {
    let clock = Clock::new();
    let reads = Mutex::new(Vec::new());
    let progress = Mutex::new(Vec::new());
    let publication = fixture.publication.clone();
    let outcome = fixture.kernel.answer_with(
        request,
        AnswerEnv {
            deadline: clock.now() + Duration::from_secs(600),
            require_live: &|| Ok(()),
            reader: &|tool, arguments, revision| {
                ensure!(revision == Some(publication.token.to_string().as_str()));
                clock.advance(step_ms);
                reads
                    .lock()
                    .unwrap()
                    .push(format!("{tool} {}", Value::Object(arguments.clone())));
                alter(tool, arguments, canned(&publication, tool, arguments)?)
            },
            clock: &|| clock.now(),
            progress: &|item| progress.lock().unwrap().push(item),
        },
    )?;
    Ok((
        outcome,
        progress.into_inner().unwrap(),
        reads.into_inner().unwrap(),
    ))
}

fn same(_: &str, _: &Map<String, Value>, body: Value) -> Result<Value> {
    Ok(body)
}

#[test]
fn no_model_loop_runs_end_to_end() -> Result<()> {
    let fixture = fixture()?;
    let (outcome, progress, reads) = run(
        &fixture,
        request("Where is `restart_guard` defined and which tests cover it?"),
        10,
        &same,
    )?;
    let response = &outcome.response;
    assert_eq!(response.parts.len(), 1);
    let part = &response.parts[0];
    assert!(!part.sub_needs.is_empty() && part.sub_needs.len() <= 4);
    assert_eq!(part.sub_needs[0].kind, v3::SubNeedKind::Definition);
    assert_eq!(
        part.sub_needs[0].coverage,
        v3::Coverage::Covered,
        "symbols plus its excerpt"
    );
    assert!(
        response
            .degradations
            .iter()
            .any(|item| matches!(item, v3::Degradation::ContextAbsent))
    );
    assert!(
        response
            .degradations
            .iter()
            .any(|item| matches!(item, v3::Degradation::FocusAbsent))
    );
    assert!(
        response
            .records
            .iter()
            .all(|record| record.canonical_result["projection"] == union::PROJECTION_SCHEMA)
    );
    assert_eq!(outcome.custody.records.len(), response.records.len());
    assert_eq!(
        outcome.custody.log.entries.len(),
        response.decision_log.total
    );
    assert!(
        outcome
            .custody
            .log
            .entries
            .iter()
            .all(|entry| !entry.decision.scored_by_model())
    );
    assert!(reads.iter().any(|read| read.starts_with("symbols")));
    assert!(reads.iter().any(|read| read.starts_with("read_file")));
    assert_eq!(
        response.work.calls,
        reads
            .iter()
            .filter(|read| !read.starts_with("file_info"))
            .count()
    );
    assert!(
        progress
            .iter()
            .any(|item| item.stage == v3::LoopStage::Coverage)
    );
    assert!(response.elapsed_ms >= 10 * response.work.calls as u64);
    response.validate_with_log(&outcome.custody.log, &v3::BudgetV3::default())?;
    Ok(())
}

#[test]
fn compound_request_runs_one_part_each_and_shares_reads() -> Result<()> {
    let fixture = fixture()?;
    let question = "Please answer both:\n1. explain how `restart_guard` decides to replay\n2. update the README section on the restart guard";
    let (outcome, _, _) = run(&fixture, request(question), 1, &same)?;
    let response = &outcome.response;
    assert_eq!(response.parts.len(), 2);
    assert!(response.parts[0].span.utf8_end <= response.parts[1].span.utf8_start);
    let split = &outcome.custody.log.entries[0];
    assert_eq!(split.decision.stage, v3::LoopStage::Split);
    assert_eq!(split.cards[usize::from(split.chosen[0])].partition.len(), 2);
    let shared = response
        .records
        .iter()
        .filter(|record| {
            response
                .record_uses
                .iter()
                .filter(|item| item.record == record.id)
                .map(|item| item.part)
                .collect::<BTreeSet<_>>()
                .len()
                > 1
        })
        .count();
    assert!(shared > 0, "both parts used one shared read");
    assert!(response.union.dedup_count >= shared);
    Ok(())
}

#[test]
fn a_single_part_reply_is_its_compose_output() -> Result<()> {
    let fixture = fixture()?;
    let (outcome, _, _) = run(
        &fixture,
        request("Explain the `restart_guard` replay rule"),
        1,
        &same,
    )?;
    let response = &outcome.response;
    assert_eq!(response.parts.len(), 1);
    assert!(response.union.conflicts.is_empty());
    assert!(response.record_uses.iter().all(|item| item.part == 1));
    assert!(
        response
            .paragraphs
            .iter()
            .all(|paragraph| paragraph.text.starts_with("Part 1 sub-need"))
    );
    Ok(())
}

#[test]
fn round_limit_lists_unrun_follow_ups() -> Result<()> {
    let fixture = fixture()?;
    let mut limited = request("Explain the restart guard replay policy and its refusal message");
    limited.budget.rounds = 1;
    // Hits that never name the sub-needs' terms, so nothing is covered.
    let (outcome, _, _) = run(&fixture, limited, 1, &|tool, _, mut body| {
        if tool == "search" {
            body["hits"] = json!([hit(
                "crates/k/src/other.rs",
                1,
                30,
                "pub fn unrelated_helper() {}
let x = OtherType::new();"
            )]);
        }
        Ok(body)
    })?;
    let part = &outcome.response.parts[0];
    assert_eq!(part.stop, v3::PartStop::Rounds);
    assert_eq!(part.rounds.len(), 1);
    let unrun: Vec<_> = part
        .sub_needs
        .iter()
        .flat_map(|need| &need.unrun_reads)
        .collect();
    assert!(!unrun.is_empty());
    assert!(
        unrun
            .iter()
            .all(|read| read.reason == v3::UnrunReason::RoundLimit)
    );
    Ok(())
}

#[test]
fn deadline_returns_partial_with_stage_and_unrun() -> Result<()> {
    let fixture = fixture()?;
    let mut short = request(
        "Explain the replay policy, its configuration flags and the documentation for them",
    );
    short.budget.deadline_ms = 20_000;
    // Each read takes four seconds of the twenty, so the reserve is reached
    // while round one still has queued reads.
    let (outcome, _, _) = run(&fixture, short, 4_000, &|tool, _, mut body| {
        if tool == "search" {
            body["hits"] = json!([hit(
                "crates/k/src/other.rs",
                1,
                30,
                "pub fn unrelated_helper() {}"
            )]);
        }
        Ok(body)
    })?;
    let response = &outcome.response;
    let part = &response.parts[0];
    assert_eq!(part.stop, v3::PartStop::Deadline);
    let partial = response
        .partial
        .as_ref()
        .context("a deadline reply is partial")?;
    assert_eq!(
        (partial.reason, partial.stage),
        (v3::PartialReason::Deadline, v3::LoopStage::Retrieve)
    );
    assert!(
        part.sub_needs
            .iter()
            .flat_map(|need| &need.unrun_reads)
            .any(|read| read.reason == v3::UnrunReason::Deadline),
        "{}",
        serde_json::to_string_pretty(&part)?
    );
    assert!(response.elapsed_ms <= 20_000 + v3::DEADLINE_TOLERANCE_MS);
    Ok(())
}

#[test]
fn budget_stop_returns_partial() -> Result<()> {
    let fixture = fixture()?;
    let mut small = request("Where is `restart_guard` defined and who calls it?");
    small.budget.calls = 4;
    small.budget.candidates = 4;
    let (outcome, _, reads) = run(&fixture, small, 1, &same)?;
    let response = &outcome.response;
    assert_eq!(response.parts[0].stop, v3::PartStop::Budget);
    assert_eq!(
        response.partial.as_ref().map(|partial| partial.reason),
        Some(v3::PartialReason::Budget)
    );
    assert!(
        reads
            .iter()
            .filter(|read| !read.starts_with("file_info"))
            .count()
            <= 4
    );
    Ok(())
}

#[test]
fn stale_publication_stops_all_parts() -> Result<()> {
    let fixture = fixture()?;
    let question = "1. explain how `restart_guard` replays reads\n2. which tests cover the restart guard refusal?";
    let (outcome, _, _) = run(&fixture, request(question), 1, &|tool, _, mut body| {
        if tool == "symbols" {
            body["publication"]["token"] = json!("another-publication");
        }
        Ok(body)
    })?;
    let response = &outcome.response;
    assert!(
        response
            .parts
            .iter()
            .all(|part| part.stop == v3::PartStop::Stale)
    );
    assert_eq!(
        response.partial.as_ref().map(|partial| partial.reason),
        Some(v3::PartialReason::Stale)
    );
    Ok(())
}

#[test]
fn early_stop_no_progress_round() -> Result<()> {
    let fixture = fixture()?;
    // Every read returns the same excerpts, so a follow-up round adds nothing.
    let (outcome, _, _) = run(
        &fixture,
        request("Explain the guard replay policy wording"),
        1,
        &|tool, _, body| {
            if tool == "file_info" {
                return Ok(body);
            }
            let mut body = body;
            body["hits"] = json!([hit("crates/k/src/guard.rs", 10, 11, "fn same() {}")]);
            body.as_object_mut().unwrap().remove("facts");
            Ok(body)
        },
    )?;
    let part = &outcome.response.parts[0];
    assert_eq!(part.stop, v3::PartStop::NoProgress);
    assert_eq!(part.rounds.last().unwrap().new_evidence, 0);
    assert!(part.rounds.len() >= 2);
    Ok(())
}

#[test]
fn configuration_change_marks_decisions_historical() -> Result<()> {
    let fixture = fixture()?;
    fixture.owner.change_after.store(3, Ordering::Release);
    let (outcome, _, _) = run(
        &fixture,
        request("Explain the `restart_guard` replay rule"),
        1,
        &same,
    )?;
    let response = &outcome.response;
    assert!(
        response
            .degradations
            .iter()
            .any(|item| matches!(item, v3::Degradation::ConfigurationChanged))
    );
    assert!(
        outcome.custody.log.entries[..2]
            .iter()
            .all(|entry| entry.decision.applicability
                == v3::AssessmentApplicability::HistoricalAfterConfigurationChange)
    );
    assert_eq!(response.applications[0].scope.configuration_revision, 2);
    Ok(())
}

/// Holds concurrent reads so they complete out of dispatch order, with no
/// timed wait. The wave runner announces each threaded wave's size; a read
/// waits until every read of the wave still to finish is inside the reader
/// (at most four), then the latest arrival goes first and each earlier one
/// goes once a later arrival has completed. Reads on the driver's own
/// thread (single-read waves, probes) are not held.
struct Reversing {
    driver: std::thread::ThreadId,
    board: Mutex<Board>,
    changed: std::sync::Condvar,
}
#[derive(Default)]
struct Board {
    arrivals: u64,
    /// The current wave's size and how many of its reads completed.
    wave: usize,
    wave_done: usize,
    inside: Vec<u64>,
    done: Vec<u64>,
    inverted: u64,
}
impl Reversing {
    fn wave(&self, size: usize) {
        let mut board = self.board.lock().unwrap();
        board.wave = size;
        board.wave_done = 0;
    }
    fn hold(&self) {
        if std::thread::current().id() == self.driver {
            return;
        }
        let mut board = self.board.lock().unwrap();
        let me = board.arrivals;
        board.arrivals += 1;
        board.inside.push(me);
        self.changed.notify_all();
        let fail_at = std::time::Instant::now() + Duration::from_secs(30);
        loop {
            let expected = wave::READS_IN_FLIGHT.min(board.wave - board.wave_done);
            let all_in = board.inside.len() >= expected;
            let latest = board.inside.iter().all(|other| *other <= me);
            let later_done = board.done.iter().any(|done| *done > me);
            if all_in && (latest || later_done) {
                break;
            }
            let now = std::time::Instant::now();
            assert!(now < fail_at, "read {me} was never released");
            board = self.changed.wait_timeout(board, fail_at - now).unwrap().0;
        }
        if board.done.iter().any(|done| *done > me) {
            board.inverted += 1;
        }
        board.inside.retain(|other| *other != me);
        board.done.push(me);
        board.wave_done += 1;
        self.changed.notify_all();
    }
}
/// One Answer on a fresh attempt of the fixture's kernel, so two runs share
/// every identity and differ only in read completion order.
fn answer_by(
    fixture: &Fixture,
    request: v3::ContextRequestV3,
    reversing: Option<&Reversing>,
) -> Result<AnswerOutcome> {
    answer_by_with(fixture, request, reversing, false)
}

/// `answer_by`; with `repeat`, every `read_file` excerpt has one text, so
/// the reply carries it once and the other rows name their carrier.
fn answer_by_with(
    fixture: &Fixture,
    request: v3::ContextRequestV3,
    reversing: Option<&Reversing>,
    repeat: bool,
) -> Result<AnswerOutcome> {
    let context = fixture.kernel.arbiter_context.as_ref().unwrap();
    let kernel = fixture
        .kernel
        .with_native_arbiter(
            context.worker,
            context.authority,
            fixture.controller.clone(),
        )?
        .with_context_attempt(ReadAttemptBudget {
            schema: 1,
            request_id: Incarnation::from_bytes([10; 16]),
            clock: ReadClock::UnixMonotonic,
            expires_monotonic_ns: 100_000_001_000,
            remaining_ns: 100_000_000_000,
            native_retry_expires_monotonic_ns: None,
        })?;
    let clock = Clock::new();
    let publication = fixture.publication.clone();
    let arrivals = std::sync::atomic::AtomicU64::new(0);
    kernel.answer_with(
        request,
        AnswerEnv {
            deadline: clock.now() + Duration::from_secs(600),
            require_live: &|| Ok(()),
            reader: &|tool, arguments, _| {
                if let Some(reversing) = reversing {
                    reversing.hold();
                }
                let mut body = canned(&publication, tool, arguments)?;
                if repeat && tool == "read_file" {
                    for hit in body["hits"].as_array_mut().into_iter().flatten() {
                        hit["text"] = json!("fn restart_guard() {}\n");
                    }
                }
                if reversing.is_some() {
                    // A transport's per-read identity, numbered by arrival,
                    // as a live reader stamps each read (entry 54).
                    let arrival = arrivals.fetch_add(1, Ordering::AcqRel);
                    body["request_id"] = json!(format!("{arrival:032x}"));
                }
                Ok(body)
            },
            clock: &|| clock.now(),
            progress: &|_| {},
        },
    )
}

#[test]
fn decisions_are_independent_of_read_completion_order() -> Result<()> {
    let question = "Please answer both:\n1. where is `restart_guard` defined and who calls it?\n2. which tests and documentation cover the restart guard replay policy?";
    let fixture = fixture()?;
    let in_order = answer_by_with(&fixture, request(question), None, true)?;
    let reversing = Reversing {
        driver: std::thread::current().id(),
        board: Mutex::new(Board::default()),
        changed: std::sync::Condvar::new(),
    };
    let reversing = Arc::new(reversing);
    let reversed = {
        let gate = reversing.clone();
        wave::ON_WAVE.with(|hook| *hook.borrow_mut() = Some(Box::new(move |size| gate.wave(size))));
        let reversed = answer_by_with(&fixture, request(question), Some(&reversing), true);
        wave::ON_WAVE.with(|hook| *hook.borrow_mut() = None);
        reversed?
    };
    let (arrivals, inverted) = {
        let board = reversing.board.lock().unwrap();
        (board.arrivals, board.inverted)
    };
    assert!(arrivals > 4, "the reads ran on wave threads");
    assert!(
        inverted > 0,
        "some reads completed before reads dispatched earlier"
    );
    assert!(in_order.response.parts.len() > 1, "a compound question");
    assert_eq!(
        serde_json::to_value(&in_order.response)?,
        serde_json::to_value(&reversed.response)?
    );
    assert_eq!(
        serde_json::to_value(&in_order.custody.log)?,
        serde_json::to_value(&reversed.custody.log)?
    );
    // A repeated text names its carrier; which record carries it does not
    // follow completion order (the shakedown pair c2-compact a/b).
    assert!(
        serde_json::to_string(&reversed.response.records)?.contains("\"same_as\""),
        "the compound question repeats a text across records"
    );
    // Custody keeps each result as received, the reversed run's per-read
    // request ids included; without them the results are the same.
    let records = |outcome: &AnswerOutcome| -> Vec<(Sha256Digest, String, Value)> {
        outcome
            .custody
            .records
            .iter()
            .map(|record| {
                let mut result = record.canonical_result.clone();
                if let Some(object) = result.as_object_mut() {
                    object.remove("request_id");
                }
                (record.id, record.tool.clone(), result)
            })
            .collect()
    };
    assert_eq!(records(&in_order), records(&reversed));
    assert!(
        reversed
            .custody
            .records
            .iter()
            .all(|record| record.canonical_result.get("request_id").is_some())
    );
    Ok(())
}

#[test]
fn only_an_answer_attempt_may_hold_the_answer_read_budget() -> Result<()> {
    let fixture = fixture()?;
    let base = fixture.kernel.with_native_arbiter(
        fixture.kernel.arbiter_context.as_ref().unwrap().worker,
        fixture.kernel.arbiter_context.as_ref().unwrap().authority,
        fixture.owner.clone(),
    )?;
    let attempt = |seconds: u64| ReadAttemptBudget {
        schema: 1,
        request_id: Incarnation::from_bytes([11; 16]),
        clock: ReadClock::UnixMonotonic,
        expires_monotonic_ns: seconds * 1_000_000_000 + 1_000,
        remaining_ns: seconds * 1_000_000_000,
        native_retry_expires_monotonic_ns: None,
    };
    assert!(
        base.with_context_attempt(attempt(300)).is_err(),
        "other tools keep 125 s"
    );
    assert!(
        base.with_context_attempt_for("airk_search", attempt(300))
            .is_err()
    );
    assert!(
        base.with_context_attempt_for("airk_answer", attempt(300))
            .is_ok()
    );
    assert!(
        base.with_context_attempt_for("answer", attempt(605))
            .is_ok()
    );
    assert!(
        base.with_context_attempt_for("airk_answer", attempt(606))
            .is_err()
    );
    assert!(base.with_context_attempt(attempt(120)).is_ok());
    Ok(())
}

fn focused(stale: bool) -> Result<(Fixture, Arc<Focused>)> {
    focused_as(stale, FocusShape::Full)
}

fn focused_as(stale: bool, shape: FocusShape) -> Result<(Fixture, Arc<Focused>)> {
    let owner = Arc::new(Owner::new());
    let focused = Arc::new(Focused {
        owner: owner.clone(),
        batches: AtomicU64::new(0),
        stale,
        shape,
        sizes: Mutex::new(vec![]),
    });
    Ok((fixture_with(focused.clone(), owner)?, focused))
}

/// Each sub-need's reply evidence and whether it came out Missing.
fn evidence(outcome: &AnswerOutcome) -> Vec<(u8, u8, Vec<Sha256Digest>, bool)> {
    outcome
        .response
        .parts
        .iter()
        .flat_map(|part| {
            part.sub_needs.iter().map(|need| {
                (
                    part.id,
                    need.id,
                    need.evidence.clone(),
                    need.coverage == v3::Coverage::Missing,
                )
            })
        })
        .collect()
}

#[test]
fn focus_short_of_tokens_leaves_spans_unscored_not_dropped() -> Result<()> {
    let (fixture, focused) = focused(false)?;
    let mut wide = request(
        "Explain the restart guard replay policy, its configuration flags and the documentation for them",
    );
    wide.budget.rounds = 1;
    let mut narrow = wide.clone();
    // Half of two pairs: one span a part-round is scored, the rest are not.
    narrow.budget.round_input_tokens = 2 * PAIR_TOKENS;
    let wide = answer_by(&fixture, wide, None)?;
    let narrow = answer_by(&fixture, narrow, None)?;
    assert!(focused.batches.load(Ordering::Acquire) >= 2);
    assert!(!wide.custody.log.focus_receipts.is_empty());
    assert!(!narrow.custody.log.focus_receipts.is_empty());
    assert!(narrow.response.degradations.iter().any(|item| matches!(
        item,
        v3::Degradation::ModelShare {
            stage: v3::LoopStage::Read,
            ..
        }
    )));
    let narrow_evidence = evidence(&narrow);
    assert!(
        narrow_evidence
            .iter()
            .filter(|(_, _, evidence, _)| !evidence.is_empty())
            .count()
            >= 2,
        "{narrow_evidence:?}"
    );
    // Focus only orders spans; the unscored ones keep their evidence.
    assert_eq!(evidence(&wide), narrow_evidence);
    Ok(())
}

#[test]
fn a_focus_batch_gone_stale_is_the_answer_stale_refusal() -> Result<()> {
    let (fixture, focused) = focused(true)?;
    let error = answer_by(
        &fixture,
        request("Explain the restart guard replay policy and its configuration flags"),
        None,
    )
    .unwrap_err();
    assert_eq!(focused.batches.load(Ordering::Acquire), 1, "never replayed");
    let stale = error
        .downcast_ref::<AnswerPostExecutionStale>()
        .context("the Answer's own stale refusal")?;
    assert_eq!(stale.receipt()["error"]["query_replay_safe"], false);
    // The stale batch's receipt stays in custody with its cause.
    assert!(
        stale
            ._cause
            .as_ref()
            .is_some_and(|cause| cause.is::<super::super::focus::FocusBatchStale>())
    );
    assert!(!stale._custody.log.entries.is_empty());
    Ok(())
}

// Change-log entry 53: Focus work as a real request has it. Each search
// returns many distinct spans, so a part-round fills its 32-pair ceiling, two
// parts need two batches a round, and the native owner answers in the shapes
// the live service did.

/// One Answer whose every search returns `hits` distinct spans.
fn answer_wide(
    fixture: &Fixture,
    request: v3::ContextRequestV3,
    hits: usize,
) -> Result<AnswerOutcome> {
    let context = fixture.kernel.arbiter_context.as_ref().unwrap();
    let kernel = fixture
        .kernel
        .with_native_arbiter(
            context.worker,
            context.authority,
            fixture.controller.clone(),
        )?
        .with_context_attempt(ReadAttemptBudget {
            schema: 1,
            request_id: Incarnation::from_bytes([12; 16]),
            clock: ReadClock::UnixMonotonic,
            expires_monotonic_ns: 100_000_001_000,
            remaining_ns: 100_000_000_000,
            native_retry_expires_monotonic_ns: None,
        })?;
    let clock = Clock::new();
    let publication = fixture.publication.clone();
    kernel.answer_with(
        request,
        AnswerEnv {
            deadline: clock.now() + Duration::from_secs(600),
            require_live: &|| Ok(()),
            reader: &|tool, arguments, _| {
                let mut body = canned(&publication, tool, arguments)?;
                if tool == "search" {
                    let query = arguments.get("query").and_then(Value::as_str).unwrap_or("");
                    body["hits"] = Value::Array(
                        (0..hits)
                            .map(|index| {
                                hit(
                                    &format!("crates/k/src/guard_{index}.rs"),
                                    1,
                                    9,
                                    &format!("pub fn restart_guard_{index}() {{}} // {query}"),
                                )
                            })
                            .collect(),
                    );
                }
                Ok(body)
            },
            clock: &|| clock.now(),
            progress: &|_| {},
        },
    )
}

/// Four parts, one sub-need each, one round, at a 4,096-token round
/// allowance: Focus takes half, so each part sends its four spans
/// (`FOCUS_SPANS_PER_NEED`) and one call holds 8 pairs. A round's 16 pairs
/// need two batches, of parts 1 and 2 and of parts 3 and 4.
fn four_part_request() -> v3::ContextRequestV3 {
    let mut request = request(
        "1. where is the restart guard defined?\n2. which tests cover the retry budget refusal?\n3. who calls the restart guard?\n4. which documentation describes the restart guard?",
    );
    request.budget.max_sub_needs = 1;
    request.budget.rounds = 1;
    request.budget.round_input_tokens = 8 * PAIR_TOKENS;
    request
}

/// The reply's Focus figures are exactly the ledger's: one call per
/// receipt, pairs as dispatched, and the batches the owner was sent.
fn assert_focus_figures(outcome: &AnswerOutcome, focused: &Focused) -> (usize, usize) {
    let receipts = &outcome.custody.log.focus_receipts;
    let dispatched: usize = receipts.iter().map(|r| r.candidate_count).sum();
    let summary = &outcome.response.decision_log;
    let work = &outcome.response.model_work;
    assert_eq!(summary.focus_calls, receipts.len());
    assert_eq!(summary.focus_pairs, dispatched);
    assert_eq!(
        (work.focus_calls, work.focus_pairs),
        (summary.focus_calls, summary.focus_pairs)
    );
    let sizes = focused.sizes.lock().unwrap().clone();
    assert_eq!(
        sizes.len(),
        receipts.len(),
        "one receipt per dispatched batch"
    );
    assert_eq!(sizes.iter().sum::<usize>(), dispatched);
    (summary.focus_calls, summary.focus_pairs)
}

fn read_share(outcome: &AnswerOutcome, part: u8) -> bool {
    outcome.response.degradations.iter().any(|item| {
        matches!(item, v3::Degradation::ModelShare { part: p, stage: v3::LoopStage::Read } if *p == part)
    })
}

/// The live failure: the native owner fails every batch above four pairs
/// before reserving anything. The kernel holds that call as unknown work
/// with no scored pair; before entry 53 the reply then failed its own
/// validation ("Answer Focus work exceeds its budget": one call, zero pairs).
#[test]
fn a_focus_call_that_scored_nothing_is_counted_and_disclosed_not_fatal() -> Result<()> {
    let (fixture, focused) = focused_as(false, FocusShape::FailAbove(4))?;
    let mut request = four_part_request();
    request.budget.rounds = 3;
    let outcome = answer_wide(&fixture, request, 40)?;
    assert_eq!(outcome.response.parts.len(), 4);
    // The first batch (8 pairs) failed; Focus stopped there for the Answer.
    assert_eq!(assert_focus_figures(&outcome, &focused), (1, 8));
    let receipt = &outcome.custody.log.focus_receipts[0];
    assert!(receipt.pairs.is_empty() && !receipt.applied);
    assert!(
        outcome
            .response
            .degradations
            .iter()
            .any(|item| matches!(item, v3::Degradation::ModelOutcomeUnknown))
    );
    assert!((1..=4).all(|part| read_share(&outcome, part)));
    Ok(())
}

/// A known zero-work call (the lane queued no pair) still dispatched its
/// pairs; every such call is counted and its parts disclose unscored spans.
#[test]
fn calls_that_queued_no_pair_count_their_dispatched_pairs() -> Result<()> {
    let (fixture, focused) = focused_as(false, FocusShape::ScoredNone)?;
    let outcome = answer_wide(&fixture, four_part_request(), 40)?;
    assert_eq!(assert_focus_figures(&outcome, &focused), (2, 16));
    assert!(
        outcome
            .custody
            .log
            .focus_receipts
            .iter()
            .all(|receipt| receipt.pairs.is_empty())
    );
    assert!((1..=4).all(|part| read_share(&outcome, part)));
    Ok(())
}

/// One round of four full parts needs two 8-pair batches; the budget is
/// reached exactly, exceeded by one batch, or never reached.
#[test]
fn focus_stops_at_its_call_and_pair_budget_and_discloses_the_rest() -> Result<()> {
    // Pairs are shared equally over the parts (remainder to part 1), so a
    // part's share also caps how many of its spans it sends.
    for (calls, pairs, sizes, unscored) in [
        // Never reached: the default budget.
        (24, 192, vec![8, 8], vec![]),
        // Reached exactly by the second batch.
        (2, 16, vec![8, 8], vec![]),
        // Calls exceeded by one batch: parts 3 and 4 are not sent.
        (1, 16, vec![8], vec![3, 4]),
        // One pair short: shares 6, 3, 3, 3 (remainder to part 1), so parts
        // 2 to 4 send 3 of their 4 spans; 4 + 3 is one call, 3 + 3 the next.
        (15, 15, vec![7, 6], vec![2, 3, 4]),
    ] {
        let (fixture, focused) = focused_as(false, FocusShape::Full)?;
        let mut request = four_part_request();
        request.budget.focus_calls = calls;
        request.budget.focus_pairs = pairs;
        let outcome = answer_wide(&fixture, request, 40)?;
        assert_eq!(outcome.response.parts.len(), 4);
        assert_eq!(
            assert_focus_figures(&outcome, &focused),
            (sizes.len(), sizes.iter().sum::<usize>()),
            "{calls} calls, {pairs} pairs"
        );
        assert_eq!(*focused.sizes.lock().unwrap(), sizes);
        assert_eq!(
            outcome
                .response
                .degradations
                .iter()
                .any(|item| matches!(item, v3::Degradation::BatchSplit)),
            sizes.len() == 2,
            "a round needing two batches discloses the split"
        );
        for part in 1..=4u8 {
            assert_eq!(
                read_share(&outcome, part),
                unscored.contains(&part),
                "{calls} calls, {pairs} pairs, part {part}"
            );
        }
    }
    Ok(())
}

/// A canonical read whose owner answered "repeat the same request" is
/// repeated after the owner's delay; any other failure is the read's own.
#[test]
fn only_a_repeat_same_request_failure_is_repeated() {
    let busy = json!({"action":"repeat_same_request","code":"publication_busy",
        "retry_after_ms":25,"retryable":true});
    let envelope = json!({"content":[{"type":"text","text":busy.to_string()}],"isError":true});
    let failed =
        |value: Value| -> Result<Value> { Err(crate::workbench::ReadFailure(value).into()) };
    assert_eq!(
        busy_retry(&failed(envelope)),
        Some(Duration::from_millis(25))
    );
    assert_eq!(
        busy_retry(&failed(
            json!({"structuredContent":busy.clone(),"isError":true})
        )),
        Some(Duration::from_millis(25))
    );
    let mut final_failure = busy.clone();
    final_failure["retryable"] = json!(false);
    assert_eq!(busy_retry(&failed(final_failure)), None);
    let mut long = busy.clone();
    long["retry_after_ms"] = json!(5_000);
    assert_eq!(busy_retry(&failed(long)), None);
    assert_eq!(busy_retry(&Ok(busy)), None);
    assert_eq!(busy_retry(&Err(anyhow::anyhow!("plain"))), None);
}
/// Change-log entry 54: two runs of one request at one publication retain
/// the same records, read sizes, basis and decisions although every read's
/// per-read status differs, and one structural read of the second run lost
/// its readiness observation (the 491-byte flip of the parity pairs).
#[test]
fn runs_differing_only_in_read_status_give_byte_equal_replies() -> Result<()> {
    let mut fixture = fixture()?;
    let question = "Where is `restart_guard` defined and which tests cover it?";
    let status = |run: u64| {
        move |tool: &str, _: &Map<String, Value>, mut body: Value| -> Result<Value> {
            let object = body.as_object_mut().context("canned read is an object")?;
            object.insert("request_id".into(), json!(format!("{run:032x}")));
            object.insert(
                "freshness".into(),
                json!({"state":"idle","observed_unix_ms":1_000 + run}),
            );
            if run == 2 && matches!(tool, "context" | "outline") {
                object.insert(
                    "discovery_observation".into(),
                    json!("unavailable_after_read"),
                );
            } else {
                object.insert(
                    "discovery_readiness".into(),
                    json!({"state":"ready","processed_artifacts":3,"committed_artifacts":3}),
                );
                object.insert(
                    "semantic_readiness".into(),
                    json!({"state":"ready","first_ready_unix_ms":run,"pipeline_digest":"x".repeat(64)}),
                );
            }
            Ok(body)
        }
    };
    let (first, _, first_reads) = run(&fixture, request(question), 10, &status(1))?;
    // A second Answer on the same session, as a transport binds each one.
    fixture.kernel = fixture.kernel.with_fresh_answer_accounting()?;
    let (second, _, mut second_reads) = run(&fixture, request(question), 10, &status(2))?;
    // The reader logs completion order, which concurrency may change; the
    // set of reads is the same.
    let mut first_reads = first_reads;
    first_reads.sort();
    second_reads.sort();
    assert_eq!(first_reads, second_reads);
    assert!(
        first_reads
            .iter()
            .any(|read| read.starts_with("context") || read.starts_with("outline")),
        "the request reads a structural record whose status flips: {first_reads:?}"
    );
    let reply = |outcome: &AnswerOutcome| -> Result<(Value, Value)> {
        let mut value = serde_json::to_value(&outcome.response)?;
        let status = value
            .as_object_mut()
            .context("reply object")?
            .remove("read_status")
            .context("reply carries one read status")?;
        Ok((value, status))
    };
    let (first_reply, first_status) = reply(&first)?;
    let (second_reply, second_status) = reply(&second)?;
    assert_eq!(first_reply, second_reply);
    // The one reply-level status is the first search's own, as received.
    assert_eq!(first_status["freshness"]["observed_unix_ms"], 1_001);
    assert_eq!(second_status["freshness"]["observed_unix_ms"], 1_002);
    assert_eq!(first_status["discovery_readiness"]["state"], "ready");
    assert!(first_status.get("request_id").is_none());
    // No record repeats the status; custody keeps each result as received.
    let records = serde_json::to_string(&first.response.records)?;
    for field in airk_core::arbiter::answer_loop::VOLATILE_READ_FIELDS {
        assert!(!records.contains(&format!("\"{field}\"")), "{field}");
    }
    for (one, two) in first.custody.records.iter().zip(&second.custody.records) {
        assert_eq!(one.id, two.id);
        assert!(one.canonical_result.get("request_id").is_some());
        assert_ne!(
            one.canonical_result["request_id"],
            two.canonical_result["request_id"]
        );
    }
    Ok(())
}

#[test]
fn private_fact_nomination_reopens_a_hash_bound_excerpt() -> Result<()> {
    let fixture = fixture()?;
    let publication = fixture.publication.clone();
    let seen = Mutex::new(Vec::<(String, Map<String, Value>)>::new());
    let hash = "7".repeat(64);
    let clock = Clock::new();
    let outcome = fixture.kernel.answer_with(
        request("Where is `restart_guard` defined?"),
        AnswerEnv {
            deadline: clock.now() + Duration::from_secs(600),
            require_live: &|| Ok(()),
            reader: &|tool, arguments, revision| {
                ensure!(revision == Some(publication.token.to_string().as_str()));
                seen.lock().unwrap().push((tool.into(), arguments.clone()));
                match tool {
                    "search" => {
                        let mut body = canned(&publication, tool, arguments)?;
                        body["hits"][0]["citation"]["content_hash"] = json!(hash);
                        Ok(body)
                    }
                    "code_facts" => Ok(json!({"publication": publication, "items": [{
                        "citation":{"path":"crates/k/src/alternate.rs","start_line":11,
                            "end_line":18,"content_hash":hash}
                    }]})),
                    _ => canned(&publication, tool, arguments),
                }
            },
            clock: &|| clock.now(),
            progress: &|_| {},
        },
    )?;
    let calls = seen.into_inner().unwrap();
    assert_eq!(
        calls
            .iter()
            .filter(|(tool, _)| tool == "code_facts")
            .count(),
        1
    );
    assert!(calls.iter().any(|(tool, arguments)| {
        tool == "read_file"
            && arguments["path"] == "crates/k/src/alternate.rs"
            && arguments["known_hash"] == hash
    }));
    assert_eq!(outcome.custody.nominations.len(), 1);
    assert_eq!(outcome.custody.nominations[0]["tool"], "code_facts");
    assert_eq!(
        outcome.custody.nominations[0]["arguments"]["max_bytes"],
        4096
    );
    assert!(
        !outcome
            .custody
            .records
            .iter()
            .any(|row| row.tool == "code_facts")
    );
    outcome
        .response
        .validate_with_log(&outcome.custody.log, &v3::BudgetV3::default())?;
    Ok(())
}
