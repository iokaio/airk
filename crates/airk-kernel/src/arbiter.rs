// SPDX-License-Identifier: Apache-2.0
//! Finite canonical indexed evidence gathering. No model or transport recursion.
pub mod device_runtime;
use airk_core::service::NativeNextReadRequest;
use airk_core::{
    Hit,
    arbiter::*,
    identity::{Sha256Digest, framed_digest},
};
use anyhow::{Context, Result, ensure};
use serde_json::{Map, Value, json};
use std::{
    collections::{BTreeSet, VecDeque},
    sync::Arc,
    time::{Duration, Instant},
};

/// Service composition supplies the native owner. No public fingerprint or
/// downloaded model descriptor constructs this controller.
pub trait NativeController: Send + Sync {
    fn device_read_status(
        &self,
        _worker: airk_core::service::WorkerIdentity,
        _authority: airk_core::service::ScopedReadRef,
        _publication: &airk_core::evidence::RevisionEnvelope,
        _attempt: airk_core::read_attempt::ReadAttemptBudget,
        _deadline: Instant,
    ) -> Result<Option<device::ScopedDeviceStatus>> {
        Ok(None)
    }

    fn focus_application(
        &self,
        _worker: airk_core::service::WorkerIdentity,
        _authority: airk_core::service::ScopedReadRef,
        _publication: &airk_core::evidence::RevisionEnvelope,
    ) -> Result<Option<airk_core::arbiter::FocusApplication>> {
        Ok(None)
    }
    fn focus_clock(&self) -> Result<Option<(airk_core::read_attempt::ReadClock, u64)>> {
        Ok(None)
    }
    fn focus_rank(
        &self,
        _request: airk_core::native_focus::NativeFocusRequest,
        _deadline: Instant,
    ) -> Result<Option<airk_core::native_focus::NativeFocusResponse>> {
        Ok(None)
    }
    /// Answer /3: the Context model that would score loop card calls now,
    /// with its device and precision. `None` means no card calls are made and
    /// the deterministic order decides.
    fn card_model(
        &self,
        _worker: airk_core::service::WorkerIdentity,
        _authority: airk_core::service::ScopedReadRef,
        _publication: &airk_core::evidence::RevisionEnvelope,
    ) -> Result<Option<LoopModel>> {
        Ok(None)
    }
    /// Answer /3: one batched card call. `None` means not dispatched and
    /// nothing charged; an error after dispatch is an unknown outcome.
    fn score_cards(
        &self,
        _request: airk_core::native_loop::NativeCardRequest,
        _deadline: Instant,
    ) -> Result<Option<airk_core::arbiter::answer_loop::CardAdvice>> {
        Ok(None)
    }
    /// Answer /3: one batched Focus call whose candidates each name their own
    /// sub-need question. `None` means not dispatched and nothing charged.
    fn focus_batch(
        &self,
        _request: airk_core::native_loop::NativeFocusRequestV3,
        _deadline: Instant,
    ) -> Result<Option<airk_core::native_loop::NativeFocusResponseV3>> {
        Ok(None)
    }
    fn application(
        &self,
        worker: airk_core::service::WorkerIdentity,
        authority: airk_core::service::ScopedReadRef,
        publication: &airk_core::evidence::RevisionEnvelope,
    ) -> Result<IndexApplication>;
    /// Same observation, but its native wait must not outlive the ORIGINAL read's
    /// absolute deadline. The default keeps in-process owners unchanged.
    fn application_before(
        &self,
        worker: airk_core::service::WorkerIdentity,
        authority: airk_core::service::ScopedReadRef,
        publication: &airk_core::evidence::RevisionEnvelope,
        _deadline: Instant,
    ) -> Result<IndexApplication> {
        self.application(worker, authority, publication)
    }
    /// Deadline-clipped Focus eligibility observation; see `application_before`.
    fn focus_application_before(
        &self,
        worker: airk_core::service::WorkerIdentity,
        authority: airk_core::service::ScopedReadRef,
        publication: &airk_core::evidence::RevisionEnvelope,
        _deadline: Instant,
    ) -> Result<Option<airk_core::arbiter::FocusApplication>> {
        self.focus_application(worker, authority, publication)
    }
    fn next_read(
        &self,
        _request: NativeNextReadRequest,
        _deadline: Instant,
    ) -> Result<Option<Advice>> {
        Ok(None)
    }
    fn context_action(
        &self,
        _request: airk_core::native_context_action::NativeContextActionRequest,
        _deadline: Instant,
    ) -> Result<Option<airk_core::arbiter::context_action::ContextActionAdvice>> {
        Ok(None)
    }
}
/// The Context model a loop card call runs on, as its decisions record it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoopModel {
    pub model: device::SelectedModel,
    pub device: airk_core::arbiter::answer_loop::ExecutionDevice,
    /// Stated for GPU devices only.
    pub precision: Option<device::DevicePrecision>,
}
pub type CanonicalReader<'a> =
    dyn Fn(&str, &Map<String, Value>, Option<&str>) -> Result<Value> + 'a;
/// The owning read adapter supplies representation/frame headroom. Kernel has
/// no knowledge of SDK, HTTP, SSE or session envelopes.
pub type FocusReplyBudget = Arc<dyn Fn(&airk_core::SearchResponse) -> Result<usize> + Send + Sync>;
#[derive(Clone)]
pub(crate) struct NativeContext {
    worker: airk_core::service::WorkerIdentity,
    authority: airk_core::service::ScopedReadRef,
    owner: Arc<dyn NativeController>,
    attempt: Option<airk_core::read_attempt::ReadAttemptBudget>,
    partition: Option<airk_core::combined_context::PartitionAllowance>,
    /// A combined Answer /3 member's allowance; card and Focus requests carry it.
    answer_partition: Option<airk_core::native_loop::PartitionAllowanceV3>,
    focus: Arc<std::sync::Mutex<focus::Attempt>>,
    focus_reply_budget: Option<FocusReplyBudget>,
    context_reply_reserve: usize,
}
fn observed_application(
    context: &NativeContext,
    publication: &airk_core::evidence::RevisionEnvelope,
    prior: Option<&IndexApplication>,
) -> Result<IndexApplication> {
    let observed = match focus::original_deadline(context) {
        Some(deadline) => context.owner.application_before(
            context.worker,
            context.authority,
            publication,
            deadline,
        ),
        None => context
            .owner
            .application(context.worker, context.authority, publication),
    };
    match observed {
        Ok(application) => {
            application.validate()?;
            Ok(application)
        }
        Err(error) if error.is::<airk_core::scoped_reads::ScopeReadRefused>() => Err(error),
        Err(error) => {
            let Some(prior) = prior else {
                return Err(error);
            };
            let mut failed = prior.clone();
            failed.availability = Availability::Failed;
            failed.reason="Optional Arbiter application was not applied because its native observation is unavailable; canonical evidence continued under its original authority.".into();
            Ok(failed)
        }
    }
}
/// Device observations taken for one diagnostics read while the publication
/// keeps moving under the same authority.
const DEVICE_OBSERVATION_ATTEMPTS: usize = 3;

impl crate::Kernel {
    /// Read-only scoped diagnostics. No native owner/original attempt means no observation.
    pub fn scoped_device_status(
        &self,
        deadline: Option<Instant>,
    ) -> Result<Option<device::ScopedDeviceStatus>> {
        let Some(context) = &self.arbiter_context else {
            return Ok(None);
        };
        let (Some(attempt), Some(deadline)) = (context.attempt, deadline) else {
            return Ok(None);
        };
        ensure!(
            Instant::now() < deadline,
            "original device observation deadline exhausted"
        );
        self.require_source_policy_scope()?;
        let authority = self.scoped_read_fingerprint()?;
        let mut attempts = 0;
        loop {
            attempts += 1;
            let publication = self.arbiter_publication()?;
            let observed = context.owner.device_read_status(
                context.worker,
                context.authority,
                &publication,
                attempt,
                deadline,
            )?;
            self.require_source_policy_scope()?;
            // A changed authority is a refusal: the read is no longer the
            // caller's to make.
            ensure!(
                self.scoped_read_fingerprint()? == authority,
                airk_core::scoped_reads::ScopeReadRefused
            );
            ensure!(
                Instant::now() < deadline,
                "original device observation deadline exhausted"
            );
            // A newer publication under the same authority (an Index's
            // semantic step lands just after its text step) is not a
            // refusal: the observation is taken again against it, and a
            // publication still moving after that is the retryable
            // `publication_busy`, never `scoped_read_unavailable`.
            if self.arbiter_publication()? != publication {
                if attempts < DEVICE_OBSERVATION_ATTEMPTS {
                    continue;
                }
                return Err(anyhow::anyhow!(
                    "the publication changed during each device observation"
                )
                .context(airk_store::current::CurrentGateContention::Applying));
            }
            if let Some(value) = &observed {
                ensure!(
                    value.scope.index_id == context.authority.index_id
                        && value.scope.authorization == context.authority.fingerprint
                        && value.scope.publication == publication,
                    airk_core::scoped_reads::ScopeReadRefused
                );
                value.validate()?;
            }
            return Ok(observed);
        }
    }
    /// The delivery fence of work that already spent model tokens
    /// (change-log entry 67). A changed authority is a refusal. A newer
    /// publication under the same authority is the typed, non-replayable
    /// `publication_advanced`: never `repeat_same_request`, because repeating
    /// would spend the model work again.
    pub(super) fn require_delivery_fence(
        &self,
        authority: airk_core::identity::Sha256Digest,
        publication: &airk_core::evidence::RevisionEnvelope,
    ) -> Result<()> {
        ensure!(
            self.scoped_read_fingerprint()? == Some(authority),
            airk_core::scoped_reads::ScopeReadRefused
        );
        if self.arbiter_publication()? != *publication {
            return Err(airk_core::scoped_reads::PublicationAdvanced.into());
        }
        Ok(())
    }
    fn arbiter_publication(&self) -> Result<airk_core::evidence::RevisionEnvelope> {
        // Published identity only; a partial scope must never request whole
        // index status/counts merely to bind its own evidence reads.
        let read = self.current_reader()?;
        let token = read.token()?;
        Ok(airk_core::evidence::RevisionEnvelope::new(
            read.head().scope.collection.clone(),
            airk_core::identity::RevisionToken {
                incarnation: token.incarnation,
                generation: token.sequence,
                revision: token.digest,
            },
        ))
    }
    /// Called by the native worker only after redeeming the actual connection
    /// grant. The service controller independently checks the exact installed
    /// lease before queue admission and delivery.
    pub fn with_native_arbiter(
        &self,
        worker: airk_core::service::WorkerIdentity,
        authority: airk_core::service::ScopedReadRef,
        owner: Arc<dyn NativeController>,
    ) -> Result<Self> {
        authority.validate()?;
        ensure!(
            self.scoped_read_fingerprint()? == Some(authority.fingerprint)
                && self
                    .scope_observation()
                    .is_some_and(|scope| scope.members.len() == 1
                        && scope.members[0].index_id == authority.index_id),
            "Arbiter authority differs from installed kernel scope"
        );
        let mut kernel = self.clone();
        kernel.arbiter_context = Some(Arc::new(NativeContext {
            worker,
            authority,
            owner,
            attempt: None,
            partition: None,
            answer_partition: None,
            focus: Arc::new(std::sync::Mutex::new(focus::Attempt::default())),
            focus_reply_budget: None,
            context_reply_reserve: 4096,
        }));
        Ok(kernel)
    }
    /// Request-local native metadata after the worker redeemed its actual grant.
    /// An actual parent is preserved; this setter never creates an attempt.
    pub fn with_context_attempt(
        &self,
        attempt: airk_core::read_attempt::ReadAttemptBudget,
    ) -> Result<Self> {
        self.with_context_attempt_for("", attempt)
    }
    /// As `with_context_attempt`, for the actual tool: an Answer attempt may
    /// hold up to the Answer read budget, every other tool the 125 s one.
    pub fn with_context_attempt_for(
        &self,
        tool: &str,
        attempt: airk_core::read_attempt::ReadAttemptBudget,
    ) -> Result<Self> {
        ensure!(
            attempt.schema == 1
                && attempt.remaining_ns > 0
                && attempt.remaining_ns
                    <= airk_core::read_attempt::read_budget_for_tool(tool).as_nanos() as u64
                && attempt
                    .native_retry_expires_monotonic_ns
                    .is_none_or(|expiry| expiry <= attempt.expires_monotonic_ns),
            "invalid native context attempt"
        );
        let mut context = self
            .arbiter_context
            .as_ref()
            .context("context attempt requires a native admitted index")?
            .as_ref()
            .clone();
        ensure!(
            context.attempt.is_none(),
            "native context attempt already captured"
        );
        context.attempt = Some(attempt);
        // A native session is reusable; optional accounting is not. Each actual
        // Host-captured request gets fresh private state under its original ID.
        context.focus = Arc::new(std::sync::Mutex::new(focus::Attempt::default()));
        let mut kernel = self.clone();
        kernel.arbiter_context = Some(Arc::new(context));
        Ok(kernel)
    }
    /// Whole-selection authority is checked by the worker before this local
    /// injection. The installed local index and original attempt still must match.
    pub fn with_context_partition(
        &self,
        allowance: airk_core::combined_context::PartitionAllowance,
        attempt: airk_core::read_attempt::ReadAttemptBudget,
    ) -> Result<Self> {
        self.with_context_partition_for("", allowance, attempt)
    }
    /// As `with_context_partition`, for the actual tool.
    pub fn with_context_partition_for(
        &self,
        tool: &str,
        allowance: airk_core::combined_context::PartitionAllowance,
        attempt: airk_core::read_attempt::ReadAttemptBudget,
    ) -> Result<Self> {
        allowance.validate()?;
        ensure!(
            allowance.attempt == attempt.request_id,
            "context partition attempt mismatch"
        );
        let mut kernel = self.with_context_attempt_for(tool, attempt)?;
        let context = Arc::get_mut(kernel.arbiter_context.as_mut().unwrap())
            .context("new native context unexpectedly shared")?;
        ensure!(
            context.authority.index_id == allowance.index_id
                && (!allowance.is_ordinary_focus()
                    || context.authority.selection.scope_identity == allowance.selection)
                && self.scoped_read_fingerprint()? == Some(context.authority.fingerprint),
            "context partition differs from installed authority"
        );
        context.partition = Some(allowance);
        Ok(kernel)
    }
    /// A combined Answer /3 member: the worker checked whole-selection
    /// authority; the installed index and the original attempt must match.
    /// `answer_with` clamps the request budget to the allowance, and every
    /// card and Focus request carries it.
    pub fn with_answer_partition_for(
        &self,
        tool: &str,
        allowance: airk_core::native_loop::PartitionAllowanceV3,
        attempt: airk_core::read_attempt::ReadAttemptBudget,
    ) -> Result<Self> {
        allowance.validate()?;
        ensure!(
            allowance.attempt == attempt.request_id,
            "context partition attempt mismatch"
        );
        let mut kernel = self.with_context_attempt_for(tool, attempt)?;
        let context = Arc::get_mut(kernel.arbiter_context.as_mut().unwrap())
            .context("new native context unexpectedly shared")?;
        ensure!(
            context.authority.index_id == allowance.index_id
                && self.scoped_read_fingerprint()? == Some(context.authority.fingerprint),
            "context partition differs from installed authority"
        );
        context.answer_partition = Some(allowance);
        Ok(kernel)
    }
    pub fn answer_with_reader(
        &self,
        mut request: ContextRequest,
        deadline: Instant,
        require_live: &dyn Fn() -> Result<()>,
        reader: &CanonicalReader<'_>,
    ) -> Result<ContextResponse> {
        request.validate()?;
        require_live()?;
        self.with_focus_parent_budget(request.budget.clone(), deadline)?;
        let started = Instant::now();
        request.budget.deadline_ms = request.budget.deadline_ms.min(
            deadline
                .saturating_duration_since(started)
                .as_millis()
                .min(u64::MAX as u128) as u64,
        );
        ensure!(
            request.budget.deadline_ms > 0,
            "Arbiter original deadline exhausted before dispatch"
        );
        let context = self
            .arbiter_context
            .as_ref()
            .context("Arbiter context requires a native admitted index connection")?;
        ensure!(
            context.partition.is_none(),
            "partition allocation requires current context /2 handler"
        );
        ensure!(
            self.scoped_read_fingerprint()? == Some(context.authority.fingerprint),
            "Arbiter source authority expired"
        );
        let publication = self.arbiter_publication()?;
        let application = observed_application(context, &publication, None)?;
        self.bind_initial_context_readiness(application.availability == Availability::Ready)?;
        ensure!(
            application.scope.authorization == context.authority.fingerprint
                && application.scope.index_id == context.authority.index_id
                && application.scope.publication == publication,
            "Arbiter owner supplied a foreign binding"
        );
        let scope = application.scope.clone();
        let layers = software::evaluation_layers();
        self.bind_focus_shape(layers)?;
        let mut gathering =
            Gathering::with_layers(request.clone(), scope.clone(), application, started, layers)?;
        loop {
            require_live()?;
            ensure!(
                self.scoped_read_fingerprint()? == Some(context.authority.fingerprint),
                "Arbiter source authority expired"
            );
            let current =
                observed_application(context, &publication, gathering.output.applications.first())?;
            gathering.refresh_application(current.clone())?;
            let choices = gathering.choices();
            let advice = if choices.len() > 1
                && current.availability == Availability::Ready
                && gathering.optional_failure.is_none()
                && !gathering.optional_stopped
                && gathering.output.advice.len() < request.budget.rounds
                && gathering.output.work.input_tokens < request.budget.input_tokens
            {
                let outcome = context.owner.next_read(
                    NativeNextReadRequest {
                        read_attempt: context.attempt,
                        worker: context.worker,
                        authority: context.authority,
                        scope: scope.clone(),
                        question: request.question.clone(),
                        records: gathering.records().to_vec(),
                        choices,
                        remaining_ms: deadline
                            .saturating_duration_since(Instant::now())
                            .as_millis()
                            .min(120_000) as u64,
                        remaining_input_tokens: request
                            .budget
                            .input_tokens
                            .saturating_sub(gathering.output.work.input_tokens),
                    },
                    deadline,
                );
                match outcome {
                    Ok(advice) => advice,
                    Err(error) if error.is::<airk_core::scoped_reads::ScopeReadRefused>() => {
                        return Err(error);
                    }
                    Err(_) => {
                        require_live()?;
                        ensure!(
                            self.scoped_read_fingerprint()? == Some(context.authority.fingerprint),
                            "Arbiter source authority expired"
                        );
                        gathering.optional_failure=Some("Optional next-read was not applied: outcome and token work may be unknown after native dispatch; no replay occurred. Canonical evidence gathering continued.".into());
                        None
                    }
                }
            } else {
                None
            };
            if let Some(advice) = advice {
                let stopped = matches!(
                    advice.outcome,
                    Outcome::Unavailable { .. }
                        | Outcome::Unsupported { .. }
                        | Outcome::InvalidInput { .. }
                );
                gathering.select(advice)?;
                gathering.optional_stopped |= stopped;
            }
            require_live()?;
            gathering.resolve_governing(reader)?;
            let Some(dispatch) = gathering.next(Instant::now())? else {
                break;
            };
            require_live()?;
            let result = reader(dispatch.tool, &dispatch.arguments, Some(&dispatch.revision));
            if let Ok(body) = &result {
                self.observe_optional_source_reply(dispatch.tool, body)?;
            }
            gathering.observe(result, Instant::now())?;
        }
        require_live()?;
        self.require_delivery_fence(context.authority.fingerprint, &publication)?;
        gathering.refresh_application(observed_application(
            context,
            &publication,
            gathering.output.applications.first(),
        )?)?;
        require_live()?;
        self.require_delivery_fence(context.authority.fingerprint, &publication)?;
        let mut response = gathering.finish(Instant::now())?;
        let scope_observation = self
            .scope_observation()
            .map(serde_json::to_value)
            .transpose()?;
        pack_native_response(&mut response, scope_observation.as_ref())?;
        response.validate(&request.budget)?;
        Ok(response)
    }
}

// Account for the same escaped native envelope and reserved frame overhead used
// at delivery. Keep the original question search intact, even when later reads
// must be omitted; it records the actual retrieval population for this packet.
fn native_response_bytes(response: &ContextResponse, scope: Option<&Value>) -> Result<usize> {
    let mut value = serde_json::to_value(response)?;
    if let Some(scope) = scope {
        value["scope_observation"] = scope.clone();
    }
    Ok(serde_json::to_vec(&serde_json::to_string(&value)?)?
        .len()
        .saturating_add(4096))
}

fn pack_native_response(response: &mut ContextResponse, scope: Option<&Value>) -> Result<()> {
    while native_response_bytes(response, scope)? > airk_core::MAX_RESPONSE_BYTES {
        ensure!(
            response.records.len() > 1,
            "Arbiter mandatory initial canonical search or packet metadata exceeds native reply budget"
        );
        // Later search receipts are omitted newest first, then later context
        // receipts newest first. Record zero is the mandatory initial search.
        let remove = response
            .records
            .iter()
            .enumerate()
            .skip(1)
            .rfind(|(_, record)| record.operation == NativeRead::Search)
            .map_or(response.records.len() - 1, |(index, _)| index);
        let omitted = response.records.remove(remove).id;
        response
            .paragraphs
            .retain(|paragraph| !paragraph.evidence.contains(&omitted));
        response.omitted_count += 1;
        response.stop = StopReason::Budget;
        response.advice.clear();
    }
    Ok(())
}

#[derive(Clone)]
pub struct Dispatch {
    pub choice: ReadChoice,
    pub tool: &'static str,
    pub arguments: Map<String, Value>,
    pub revision: String,
    pub round: usize,
    /// Change-set kind; `Initial` and `Literal` under product gathering.
    pub kind: software::FollowUp,
    /// The identifier or document a change-set read is about.
    subject: String,
}

/// Native callers supply this binding from installed authority, never request
/// arguments. The caller rechecks that same authority before dispatch/delivery.
pub struct Gathering {
    optional_failure: Option<String>,
    optional_stopped: bool,
    literal_queries: BTreeSet<String>,
    request: ContextRequest,
    scope: ScopeBinding,
    deadline: Instant,
    queue: VecDeque<Dispatch>,
    pending: Option<Dispatch>,
    seen: BTreeSet<Sha256Digest>,
    excerpts: BTreeSet<Sha256Digest>,
    output: ContextResponse,
    focus_work: Work,
    /// Private evaluation change-set state; all layers off for product reads.
    plan: software::plan::State,
}

fn token(value: &Value) -> Option<&str> {
    value
        .get("publication")
        .and_then(|publication| publication.get("token"))
        .and_then(Value::as_str)
        .or_else(|| {
            value
                .get("read")
                .and_then(|read| read.get("publication"))
                .and_then(|publication| publication.get("token"))
                .and_then(Value::as_str)
        })
        .or_else(|| value.get("evidence").and_then(token))
}

impl Gathering {
    pub fn new(
        request: ContextRequest,
        scope: ScopeBinding,
        application: IndexApplication,
        now: Instant,
    ) -> Result<Self> {
        Self::with_layers(
            request,
            scope,
            application,
            now,
            software::Layers::default(),
        )
    }

    /// Private evaluation change set 1; `Layers::default()` is `new`.
    pub fn with_layers(
        request: ContextRequest,
        scope: ScopeBinding,
        application: IndexApplication,
        now: Instant,
        layers: software::Layers,
    ) -> Result<Self> {
        request.validate()?;
        application.validate()?;
        ensure!(
            scope.index_id == application.scope.index_id
                && scope.authorization == application.scope.authorization
                && scope.publication == application.scope.publication,
            "foreign Arbiter application scope"
        );
        let deadline = now
            .checked_add(Duration::from_millis(request.budget.deadline_ms))
            .context("Arbiter deadline overflow")?;
        let revision = scope.publication.token.to_string();
        let choice = ReadChoice {
            id: framed_digest(
                b"airk.arbiter.initial-search/1",
                &[request.question.as_bytes(), revision.as_bytes()],
            ),
            operation: NativeRead::Search,
            target: request.question.clone(),
        };
        let max_bytes = (request.budget.max_bytes / 8).clamp(1024, 4096);
        let arguments = if layers.pool {
            software::initial_search(&request.question, &request.budget)
        } else {
            json!({"query":request.question,
            "limit":request.budget.fanout.min(8).min(request.budget.candidates),"max_bytes":max_bytes,"full_chunks":true})
        };
        let dispatch = Dispatch {
            choice,
            tool: "search",
            arguments: arguments.as_object().unwrap().clone(),
            revision,
            round: 1,
            kind: software::FollowUp::Initial,
            subject: request.question.clone(),
        };
        let output = ContextResponse {
            schema: CONTEXT_SCHEMA.into(),
            packet_schema: PACKET_SCHEMA.into(),
            question: request.question.clone(),
            records: vec![],
            paragraphs: vec![],
            gaps: vec![],
            stop: StopReason::NoValidRead,
            work: Work::default(),
            candidate_count: 0,
            omitted_count: 0,
            advice: vec![],
            applications: vec![application],
        };
        let literal_queries = BTreeSet::from([request.question.to_lowercase()]);
        Ok(Self {
            optional_failure: None,
            optional_stopped: false,
            literal_queries,
            request,
            scope,
            deadline,
            queue: VecDeque::from([dispatch]),
            pending: None,
            seen: BTreeSet::new(),
            excerpts: BTreeSet::new(),
            output,
            focus_work: Work::default(),
            plan: software::plan::State::new(layers),
        })
    }

    pub fn scope(&self) -> &ScopeBinding {
        &self.scope
    }
    pub fn records(&self) -> &[EvidenceRecord] {
        &self.output.records
    }
    pub fn choices(&self) -> Vec<ReadChoice> {
        self.queue
            .iter()
            .map(|dispatch| dispatch.choice.clone())
            .collect()
    }

    /// Only a separately qualified next-read task may change the next dispatch.
    /// Relevance/role classifications never imply missing-context judgment.
    pub fn select(&mut self, advice: Advice) -> Result<()> {
        advice.validate()?;
        ensure!(
            self.pending.is_none() && self.output.advice.len() < self.request.budget.rounds,
            "Arbiter advice is outside the original round budget"
        );
        ensure!(
            self.output.applications[0].availability == Availability::Ready
                && self.output.applications[0].profile == Some(advice.profile),
            "Arbiter advice profile is not active"
        );
        ensure!(
            advice.task == Task::ContextNextRead
                && advice.scope.index_id == self.scope.index_id
                && advice.scope.installation == self.scope.installation
                && advice.scope.catalog == self.scope.catalog
                && advice.scope.authorization == self.scope.authorization
                && advice.scope.configuration_revision == self.scope.configuration_revision
                && advice.scope.publication == self.scope.publication,
            "foreign Arbiter next-read advice"
        );
        let input_tokens = self
            .output
            .work
            .input_tokens
            .checked_add(advice.work.input_tokens)
            .context("Arbiter token accounting overflow")?;
        ensure!(
            input_tokens <= self.request.budget.input_tokens,
            "Arbiter original token budget exceeded"
        );
        if let Outcome::Accepted {
            decision: Decision::NextRead(id),
        } = &advice.outcome
        {
            let position = self
                .queue
                .iter()
                .position(|dispatch| dispatch.choice.id == *id)
                .context("Arbiter chose an unverified read")?;
            let selected = self.queue.remove(position).unwrap();
            self.queue.push_front(selected);
        }
        self.output.work.input_tokens = input_tokens;
        for (total, addition) in [
            (&mut self.output.work.queue_us, advice.work.queue_us),
            (&mut self.output.work.compile_us, advice.work.compile_us),
            (&mut self.output.work.inference_us, advice.work.inference_us),
            (&mut self.output.work.policy_us, advice.work.policy_us),
        ] {
            if let Some(addition) = addition {
                *total = Some(
                    total
                        .unwrap_or(0)
                        .checked_add(addition)
                        .context("Arbiter timing overflow")?,
                );
            }
        }
        self.output.advice.push(advice);
        Ok(())
    }

    pub fn refresh_application(&mut self, mut application: IndexApplication) -> Result<()> {
        application.validate()?;
        ensure!(
            application.scope.installation == self.scope.installation
                && application.scope.catalog == self.scope.catalog
                && application.scope.index_id == self.scope.index_id
                && application.scope.authorization == self.scope.authorization
                && application.scope.publication == self.scope.publication,
            "Arbiter application authority changed"
        );
        if let Some(reason) = &self.optional_failure {
            application.availability = Availability::Failed;
            application.reason = reason.clone();
        }
        if application.availability != Availability::Ready
            || application.scope.configuration_revision != self.scope.configuration_revision
        {
            // Keep measured costs, suppress stale learned deliveries. Complete
            // authorized native evidence remains usable independently.
            self.output.advice.clear();
        }
        self.output.applications[0] = application;
        Ok(())
    }

    pub fn next(&mut self, now: Instant) -> Result<Option<Dispatch>> {
        ensure!(
            self.pending.is_none(),
            "Arbiter prior read has no observation"
        );
        if now >= self.deadline {
            self.output.stop = StopReason::Deadline;
            return Ok(None);
        }
        if self.output.work.calls >= self.request.budget.calls
            || self.output.candidate_count >= self.request.budget.candidates
        {
            self.output.stop = StopReason::Budget;
            return Ok(None);
        }
        if self.strict_deadline_stops(now) {
            self.output.stop = StopReason::Deadline;
            return Ok(None);
        }
        let Some(dispatch) = self.queue.pop_front() else {
            return Ok(None);
        };
        if dispatch.round > self.request.budget.rounds {
            self.output.stop = StopReason::Budget;
            return Ok(None);
        }
        if !self.seen.insert(dispatch.choice.id) {
            self.output.stop = StopReason::NoProgress;
            return Ok(None);
        }
        self.output.work.calls += 1;
        self.output.work.rounds = self.output.work.rounds.max(dispatch.round);
        self.pending = Some(dispatch.clone());
        Ok(Some(dispatch))
    }

    pub fn observe(&mut self, result: Result<Value>, now: Instant) -> Result<()> {
        let dispatch = self
            .pending
            .take()
            .context("Arbiter observation has no admitted read")?;
        let body = match result {
            Ok(body) => body,
            Err(error) if self.plan.layers.followups && dispatch.round > 1 => {
                // Change set 1: an optional follow-up may fail legitimately (an
                // ambiguous definition, a moved file). Its canonical failure
                // receipt is retained; the other queued reads continue.
                if let Some(failure) = error.downcast_ref::<crate::workbench::ReadFailure>()
                    && self.retain(&dispatch, failure.0.clone(), EvidenceRole::Unknown)?
                {
                    self.plan.record(&self.output.records, dispatch.kind);
                }
                self.plan.failed += 1;
                return Ok(());
            }
            Err(error) => {
                self.output.stop = StopReason::Refused;
                self.queue.clear();
                // Canonical failure receipts remain intact when available.
                if let Some(failure) = error.downcast_ref::<crate::workbench::ReadFailure>() {
                    self.retain(&dispatch, failure.0.clone(), EvidenceRole::Unknown)?;
                }
                return Ok(());
            }
        };
        ensure!(
            token(&body).is_some(),
            "canonical Arbiter read omitted its publication"
        );
        if token(&body) != Some(dispatch.revision.as_str()) {
            self.output.stop = StopReason::Stale;
            self.queue.clear();
            return Ok(());
        }
        let retained = self.retain(&dispatch, body.clone(), EvidenceRole::Unknown)?;
        if !retained {
            self.queue.clear();
            return Ok(());
        }
        self.plan.record(&self.output.records, dispatch.kind);
        let focus = airk_core::arbiter::focus::observed_focus_work(
            &self.output.records,
            &self.request.budget,
        )?;
        self.output.work.input_tokens = self
            .output
            .work
            .input_tokens
            .checked_sub(self.focus_work.input_tokens)
            .and_then(|tokens| tokens.checked_add(focus.work.input_tokens))
            .context("combined Focus/Context token accounting overflow")?;
        for (total, previous, observed) in [
            (
                &mut self.output.work.queue_us,
                self.focus_work.queue_us,
                focus.work.queue_us,
            ),
            (
                &mut self.output.work.compile_us,
                self.focus_work.compile_us,
                focus.work.compile_us,
            ),
            (
                &mut self.output.work.inference_us,
                self.focus_work.inference_us,
                focus.work.inference_us,
            ),
            (
                &mut self.output.work.policy_us,
                self.focus_work.policy_us,
                focus.work.policy_us,
            ),
        ] {
            if observed.is_some() || previous.is_some() {
                *total = Some(
                    total
                        .unwrap_or(0)
                        .checked_sub(previous.unwrap_or(0))
                        .and_then(|value| value.checked_add(observed.unwrap_or(0)))
                        .context("combined Focus/Context timing accounting overflow")?,
                );
            }
        }
        self.output.work.candidates = self.output.work.candidates.max(focus.work.candidates);
        self.focus_work = focus.work;
        if focus.work_unknown {
            self.optional_failure = Some("Focus native work is unsettled; canonical evidence remains available but optional scoring is not replayed.".into());
        }
        if focus.failed_token_overrun {
            self.optional_failure = Some("Focus measured work exceeded its reservation; no later optional scoring was admitted.".into());
            self.output.stop = StopReason::Budget;
            self.queue.clear();
            return Ok(());
        }
        if let Some(source) = literal::SourceLiteral::parse(&dispatch.choice.target)? {
            let result_record = self
                .output
                .records
                .last()
                .context("missing literal result record")?
                .id;
            self.output.paragraphs.push(GroundedParagraph{text:format!("Native literal search {:?} came from returned UTF-8 bytes {}..{} at {}:{}; its complete canonical search result is retained separately.",source.query,source.returned_utf8_start,source.returned_utf8_end,source.path,source.line),evidence:vec![source.record,result_record],interpretation:true});
        }
        let mut progress = false;
        if self.plan.layers.followups {
            // Definitions, references and outlines carry evidence outside
            // top-level hits; each is progress under the same excerpt digest.
            for value in software::plan::evidence_units(&body) {
                let bytes = serde_json::to_vec(value)?;
                progress |= self
                    .excerpts
                    .insert(framed_digest(b"airk.arbiter.source-excerpt/1", &[&bytes]));
            }
        } else if let Some(hits) = body.get("hits").and_then(Value::as_array) {
            for hit in hits {
                let bytes = serde_json::to_vec(hit)?;
                progress |= self
                    .excerpts
                    .insert(framed_digest(b"airk.arbiter.source-excerpt/1", &[&bytes]));
            }
        }
        if !progress {
            if self.queue.is_empty() {
                self.output.stop = StopReason::NoProgress;
            }
            // A duplicate at one target proves nothing about another legal
            // target. Original finite call/round/choice/deadline bounds remain.
            return Ok(());
        }
        if now >= self.deadline {
            self.output.stop = StopReason::Deadline;
            self.queue.clear();
            return Ok(());
        }
        if self.plan.layers.followups {
            return self.plan_followups(&dispatch, &body);
        }
        if matches!(
            dispatch.choice.operation,
            NativeRead::Search | NativeRead::Context | NativeRead::ReadEvidence
        ) {
            let hits = body
                .get("hits")
                .and_then(Value::as_array)
                .context("canonical search hits missing")?;
            let capacity = self
                .request
                .budget
                .fanout
                .min(8)
                .min(self.request.budget.candidates);
            let round = dispatch.round.saturating_add(1);
            if round > self.request.budget.rounds {
                return Ok(());
            }
            let source_record = self
                .output
                .records
                .last()
                .context("missing retained source record")?
                .id;
            let max_bytes = (self.request.budget.max_bytes / 8).clamp(1024, 4096);
            let mut added = 0usize;
            for value in hits.iter().take(capacity) {
                let hit: Hit = serde_json::from_value(value.clone())?;
                let path = &hit.citation.path;
                ensure!(
                    crate::workbench::safe_path(path)
                        && hit.citation.start_line > 0
                        && hit.citation.end_line >= hit.citation.start_line
                        && hit.citation.end_line <= u32::MAX as usize,
                    "unsafe indexed Arbiter target"
                );
                // Full-chunk search can return a match near the end of a long
                // hit. Anchor its bounded context at the exact returned query
                // line when present, rather than losing it behind the first
                // 32 lines. The candidate stays inside the verified citation;
                // nonliteral questions retain the original start-line recipe.
                let start = hit
                    .text
                    .find(&self.request.question)
                    .and_then(|offset| {
                        hit.citation.start_line.checked_add(
                            hit.text[..offset]
                                .bytes()
                                .filter(|byte| *byte == b'\n')
                                .count(),
                        )
                    })
                    .filter(|line| *line <= hit.citation.end_line)
                    .unwrap_or(hit.citation.start_line);
                let end = start.saturating_add(32).min(u32::MAX as usize);
                let target = format!("{path}:{start}-{end}");
                let id = framed_digest(
                    b"airk.arbiter.indexed-expansion/1",
                    &[dispatch.revision.as_bytes(), target.as_bytes()],
                );
                if self.queue.len() < capacity
                    && !self.seen.contains(&id)
                    && !self.queue.iter().any(|choice| choice.choice.id == id)
                {
                    self.queue.push_back(Dispatch{choice:ReadChoice{id,operation:NativeRead::Context,target},tool:"context",arguments:json!({"path":path,"line":start,"before":0,"after":32,"max_bytes":max_bytes}).as_object().unwrap().clone(),revision:dispatch.revision.clone(),round,kind:software::FollowUp::WholeChunk,subject:String::new()});
                    added += 1;
                }
                for (start, end) in literal::source_spans(&hit.text) {
                    if self.queue.len() >= capacity {
                        break;
                    }
                    let query = &hit.text[start..end];
                    if !self.literal_queries.insert(query.to_lowercase()) {
                        continue;
                    }
                    let source = literal::SourceLiteral {
                        record: source_record,
                        path: path.clone(),
                        line: hit.citation.start_line,
                        returned_utf8_start: start,
                        returned_utf8_end: end,
                        query: query.into(),
                    };
                    let target = source.target()?;
                    // The stable closed ID excludes per-request record IDs;
                    // the target still retains their exact byte provenance.
                    let id = framed_digest(
                        b"airk.arbiter.literal-search-choice/1",
                        &[
                            dispatch.revision.as_bytes(),
                            path.as_bytes(),
                            &source.line.to_le_bytes(),
                            &start.to_le_bytes(),
                            &end.to_le_bytes(),
                            query.as_bytes(),
                        ],
                    );
                    if self.seen.contains(&id)
                        || self.queue.iter().any(|choice| choice.choice.id == id)
                    {
                        continue;
                    }
                    self.queue.push_back(Dispatch{choice:ReadChoice{id,operation:NativeRead::Search,target},tool:"search",arguments:json!({"query":query,"limit":4,"max_bytes":max_bytes,"full_chunks":true}).as_object().unwrap().clone(),revision:dispatch.revision.clone(),round,kind:software::FollowUp::Literal,subject:query.into()});
                    added += 1;
                }
                if self.queue.len() >= capacity {
                    break;
                }
            }
            self.output.work.fanout = self.output.work.fanout.max(added);
            self.output.work.candidates = self.output.work.candidates.max(self.queue.len());
            if self.queue.is_empty() {
                self.output.stop = StopReason::NoValidRead;
            }
        }
        Ok(())
    }

    fn retain(&mut self, dispatch: &Dispatch, body: Value, role: EvidenceRole) -> Result<bool> {
        let bytes = serde_json::to_vec(&body)?;
        let record = EvidenceRecord {
            id: framed_digest(
                b"airk.arbiter.canonical-record/1",
                &[
                    dispatch.choice.id.as_bytes(),
                    &serde_json::to_vec(&dispatch.arguments)?,
                    &bytes,
                ],
            ),
            scope: self.scope.clone(),
            operation: dispatch.choice.operation,
            arguments: dispatch.arguments.clone().into_iter().collect(),
            role,
            canonical_result: body,
        };
        let initial = self.output.candidate_count == 0;
        self.output.candidate_count += 1;
        if self
            .output
            .work
            .bytes
            .checked_add(bytes.len())
            .is_none_or(|count| count > self.request.budget.max_bytes / 2)
            || self.output.records.len() >= self.request.budget.candidates
        {
            ensure!(
                !airk_core::arbiter::focus::observed_focus_work(
                    std::slice::from_ref(&record),
                    &self.request.budget,
                )?
                .has_native_receipt,
                "original Answer retention budget cannot retain mandatory Focus native work; optional work is not replayed"
            );
            ensure!(
                !initial,
                "Arbiter mandatory initial canonical search exceeds original retention budget"
            );
            self.output.omitted_count += 1;
            self.output.stop = StopReason::Budget;
            return Ok(false);
        }
        self.output.work.bytes += bytes.len();
        self.output.records.push(record);
        Ok(true)
    }

    pub fn finish(self, now: Instant) -> Result<ContextResponse> {
        self.finish_native(now, true)
    }
    fn finish_native(mut self, now: Instant, validate: bool) -> Result<ContextResponse> {
        ensure!(
            self.pending.is_none(),
            "Arbiter native read is still pending"
        );
        if now >= self.deadline {
            self.output.stop = StopReason::Deadline;
        }
        if self.plan.layers.present {
            self.present()?;
            if validate {
                self.output.validate(&self.request.budget)?;
            }
            return Ok(self.output);
        }
        // Intact excerpts are source facts. Organization and optional learned
        // roles remain separate from the complete canonical read receipts.
        let mut presented = BTreeSet::new();
        for (record_index, record) in self.output.records.iter().enumerate() {
            if let Some(hits) = record
                .canonical_result
                .get("hits")
                .and_then(Value::as_array)
            {
                for value in hits.iter().take(4) {
                    let hit: Hit = serde_json::from_value(value.clone())?;
                    let digest = framed_digest(
                        b"airk.arbiter.presented-excerpt/1",
                        &[&serde_json::to_vec(&hit)?],
                    );
                    if !presented.insert(digest) {
                        continue;
                    }
                    let relation = if record.operation == NativeRead::Search && record_index == 0 {
                        "The initial indexed evidence includes"
                    } else if record.operation == NativeRead::Search {
                        "Additional indexed search evidence states"
                    } else {
                        "Additional indexed context for that evidence states"
                    };
                    self.output.paragraphs.push(GroundedParagraph {
                        text: format!(
                            "{relation} ({}:{}–{}):\n\n{}",
                            hit.citation.path,
                            hit.citation.start_line,
                            hit.citation.end_line,
                            hit.text
                        ),
                        evidence: vec![record.id],
                        interpretation: false,
                    });
                }
            }
        }
        if validate {
            self.output.validate(&self.request.budget)?;
        }
        Ok(self.output)
    }
}

mod answer;
pub(crate) use answer::independent_parts;
mod focus;
mod joint;
pub mod software;
pub use answer::{
    AdmissionPlan, AnswerCustody, AnswerEnv, AnswerOutcome, AnswerPostExecutionStale,
    AnswerProgress, CanonicalReadFn, CustodyRecord,
};
pub use focus::FocusPostExecutionStale;
pub use joint::JointPostExecutionStale;
#[cfg(test)]
mod tests;
