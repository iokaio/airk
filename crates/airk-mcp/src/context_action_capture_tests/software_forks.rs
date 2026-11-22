// SPDX-License-Identifier: Apache-2.0
//! Private oracle leverage collector for the Answer evaluation (Context trainer
//! design, gate 1). Evaluation tooling only; no model, score or calibration.
//!
//! For each prepared first Answer call, the change-set gathering runs in this
//! process through `Kernel::answer_with_reader_v2`, bound to a read-only
//! observer of the live evaluation index (same scope and publication). Every
//! canonical inner read goes to the live evaluation service through the same
//! CLI tool path an agent uses, with the arguments `AirkServer::workbench_read`
//! sets. At each Context decision point the controller either abstains (the
//! queue-order branch) or returns an Accepted advice for one offered Read card
//! (one forced branch per offered Read). Each branch runs to completion under
//! the call's own budget and the delivered packet is rendered as the Answer
//! handler renders it. Rows stream to JSONL; no NN executes.
use crate::arbiter_handler_tests::{Disabled, body};
use crate::*;
use airk_core::{
    arbiter::context_action::{ADVICE_SCHEMA_V2, ContextAction, ContextActionAdvice},
    arbiter::{Availability, Decision, IndexApplication, Outcome, RUNTIME, Task, Work},
    identity::{Incarnation, Sha256Digest, framed_digest},
    native_context_action::NativeContextActionRequest,
    read_attempt::{MAX_READ_BUDGET, ReadAttemptBudget, ReadClock},
    service::{ScopedReadRef, WorkerIdentity},
};
use airk_kernel::arbiter::NativeController;
use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    path::PathBuf,
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

const INPUT_SCHEMA: &str = "airk.arbiter.context-forks.input/1";
const OUTPUT_SCHEMA: &str = "airk.arbiter.context-forks.rows/1";
const INPUT_DOMAIN: &[u8] = b"airk.arbiter.context-forks.input/1";
const ABSTAIN: &str = "oracle collector: queue order, no model";
const FORCED: &str = "oracle collector: forced offered read, no model";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    schema: String,
    /// `Layers::parse` text; Focus must be off (no Focus model here).
    layers: String,
    cli: Vec<String>,
    control: PathBuf,
    environment: BTreeMap<String, String>,
    call_timeout_ms: u64,
    /// Tokens charged per Context assessment (reserved and observed).
    assessment_input_tokens: usize,
    /// Decision points to fork, for example `[1]` or `[1, 2]`.
    fork_ordinals: Vec<u8>,
    /// Refuse a live search reply carrying a Focus receipt (the stage is on).
    #[serde(default)]
    allow_live_focus: bool,
    indexes: BTreeMap<String, IndexEntry>,
    calls: Vec<Call>,
    /// The image the live lifetime runs and the in-process source check
    /// (`prepare_forks.py`); echoed, never interpreted here.
    provenance: Value,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IndexEntry {
    index_id: String,
    root: PathBuf,
    /// The live index's data root, opened only as a read-only observer.
    data_root: PathBuf,
    collection: String,
    /// The service's project binding (`R/fixture/indexes/<index>.state.json`).
    binding: String,
    /// The owner source root of the collection's source policy, when it has one.
    source_root_id: Option<String>,
    publication_token: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Call {
    id: String,
    scenario_id: String,
    repository: String,
    /// The `airk_answer` arguments the agent's wrapper sends (no index).
    arguments: Value,
}

/// One read card as the controller saw it, in roster order.
fn roster_summary(request: &NativeContextActionRequest) -> Value {
    let mut read_slot = 0;
    let cards: Vec<Value> = request
        .roster
        .cards
        .iter()
        .enumerate()
        .map(|(slot, card)| match card.action {
            ContextAction::Read { choice } => {
                read_slot += 1;
                let position = request.choices.iter().position(|c| c.id == choice);
                let offered = position.map(|p| &request.choices[p]);
                json!({"slot":slot,"card":card.id,"action":"read","read_slot":read_slot,
                    "choice":choice,"queue_position":position,
                    "operation":offered.map(|c| &c.operation),"target":offered.map(|c| c.target.clone())})
            }
            ContextAction::Sufficient => json!({"slot":slot,"card":card.id,"action":"sufficient"}),
            ContextAction::Clarify => json!({"slot":slot,"card":card.id,"action":"clarify"}),
            ContextAction::NoJustifiedRead => {
                json!({"slot":slot,"card":card.id,"action":"no_justified_read"})
            }
        })
        .collect();
    json!({"ordinal":request.ordinal,"records":request.records.len(),"queued":request.choices.len(),
        "eligible":request.roster.basis.read_choices_scored,"remaining_input_tokens":request.remaining_input_tokens,
        "remaining_ms":request.remaining_ms,"task_policy":request.roster.policy,"cards":cards})
}

/// The offered roster without card ids: ids are minted per assessment, while
/// the choice, operation, target and slot of each card identify the offer.
fn roster_shape(cards: &Value) -> Value {
    let mut cards = cards.clone();
    for card in cards.as_array_mut().into_iter().flatten() {
        if let Some(card) = card.as_object_mut() {
            card.remove("card");
        }
    }
    cards
}

/// Abstains at every ordinal except `force`, where it accepts the Read card
/// of that offered choice. No score is produced or implied.
struct Forks {
    profile: Sha256Digest,
    tokens: usize,
    force: Option<(u8, Sha256Digest)>,
    seen: Mutex<Vec<Value>>,
    applied: Mutex<Option<bool>>,
}
impl NativeController for Forks {
    fn application(
        &self,
        worker: WorkerIdentity,
        authority: ScopedReadRef,
        publication: &airk_core::evidence::RevisionEnvelope,
    ) -> Result<IndexApplication> {
        let mut application = Disabled.application(worker, authority, publication)?;
        application.availability = Availability::Ready;
        application.profile = Some(self.profile);
        application.reason =
            "Private oracle fork collector; no admitted model or NN execution.".into();
        Ok(application)
    }
    fn context_action(
        &self,
        request: NativeContextActionRequest,
        _: Instant,
    ) -> Result<Option<ContextActionAdvice>> {
        self.seen
            .lock()
            .map_err(|_| anyhow::anyhow!("fork lock poisoned"))?
            .push(roster_summary(&request));
        let forced = match self.force {
            Some((ordinal, choice)) if ordinal == request.ordinal => {
                let card = request
                    .roster
                    .cards
                    .iter()
                    .find(|card| card.action == ContextAction::Read { choice })
                    .map(|card| card.id);
                *self
                    .applied
                    .lock()
                    .map_err(|_| anyhow::anyhow!("fork lock poisoned"))? = Some(card.is_some());
                card
            }
            _ => None,
        };
        let tokens = self.tokens.min(request.remaining_input_tokens);
        ensure!(
            tokens > 0,
            "no Context input tokens remain for an assessment"
        );
        Ok(Some(ContextActionAdvice {
            schema: ADVICE_SCHEMA_V2.into(),
            task: Task::ContextAction,
            profile: self.profile,
            runtime: RUNTIME.into(),
            compiler: Sha256Digest::from_bytes([61; 32]),
            calibration: Sha256Digest::from_bytes([62; 32]),
            policy: Sha256Digest::from_bytes([63; 32]),
            task_policy: request.roster.policy,
            scope: request.scope.clone(),
            input_digest: request.roster.input_digest,
            evidence: request
                .roster
                .basis
                .source_units_presented
                .iter()
                .map(|unit| unit.record)
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect(),
            reserved_input_tokens: tokens,
            observed_input_tokens: Some(tokens),
            work: Work {
                input_tokens: tokens,
                compile_us: Some(0),
                inference_us: Some(0),
                ..Work::default()
            },
            outcome: match forced {
                Some(card) => Outcome::Accepted {
                    decision: Decision::ContextAction(card),
                },
                None => Outcome::Abstained {
                    reason: if self.force.is_some_and(|(o, _)| o == request.ordinal) {
                        FORCED.into()
                    } else {
                        ABSTAIN.into()
                    },
                },
            },
        }))
    }
}

/// Canonical inner reads through the live evaluation service's CLI tool path.
struct LiveReader<'a> {
    input: &'a Input,
    index_id: &'a str,
    /// The prepared live publication; every dispatched revision must equal it.
    publication: &'a str,
    foreign_revision: Mutex<Option<String>>,
    calls: Mutex<Vec<Value>>,
    transport_failure: Mutex<Option<String>>,
    live_focus: Mutex<usize>,
}
impl LiveReader<'_> {
    /// The argument rewrite of `AirkServer::workbench_read` for a standalone Answer.
    fn arguments(tool: &str, args: &Map<String, Value>, revision: Option<&str>) -> Result<Value> {
        let mut arguments = args.clone();
        let catalog = crate::local::tools();
        let definition = catalog
            .iter()
            .find(|entry| entry.name.strip_prefix("airk_") == Some(tool))
            .context("unknown evidence operation")?;
        let properties = definition.input_schema["properties"]
            .as_object()
            .context("native read has no object inputs")?;
        if properties.contains_key("response_mode") {
            arguments.insert("response_mode".into(), json!("full"));
        }
        if properties.contains_key("include_status") {
            arguments.insert("include_status".into(), json!(true));
        }
        if properties.contains_key("verification") {
            arguments.retain(|key, _| !key.starts_with("verification"));
            arguments.insert("verification".into(), json!("off"));
        }
        if matches!(tool, "read_file" | "file_info" | "outline" | "context")
            && let Some(revision) = revision
        {
            arguments
                .entry("revision")
                .or_insert_with(|| json!(revision));
        }
        if matches!(
            tool,
            "symbols" | "imports" | "code_facts" | "document_facts" | "outline"
        ) && arguments.get("reply_detail").is_none_or(Value::is_null)
            && arguments.get("compact").is_none_or(Value::is_null)
        {
            arguments.insert("compact".into(), json!(false));
        }
        Ok(Value::Object(arguments))
    }

    fn invoke(&self, tool: &str, arguments: &Value) -> Result<(Option<i32>, Vec<u8>, Duration)> {
        let (program, prefix) = self.input.cli.split_first().context("empty CLI command")?;
        let mut command = Command::new(program);
        command
            .args(prefix)
            .args(["--output", "json", "--service-runtime-dir"])
            .arg(&self.input.control)
            .args([
                "tools",
                "call",
                &format!("airk_{tool}"),
                "--index",
                self.index_id,
            ])
            .arg("--args")
            .arg(serde_json::to_string(arguments)?)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // The agent's environment: inherited AIRK_* removed, the handle's client
        // environment applied, its response-mode default stripped (arguments set it).
        for (key, _) in std::env::vars_os() {
            if key
                .to_str()
                .is_some_and(|key| key.to_ascii_uppercase().starts_with("AIRK_"))
            {
                command.env_remove(&key);
            }
        }
        for (key, value) in &self.input.environment {
            if key != "AIRK_MCP_RESPONSE_MODE" {
                command.env(key, value);
            }
        }
        let started = Instant::now();
        let mut child = command.spawn().context("start evaluation CLI")?;
        let mut stdout = child.stdout.take().context("CLI stdout")?;
        let mut stderr = child.stderr.take().context("CLI stderr")?;
        let out = std::thread::spawn(move || {
            let mut bytes = Vec::new();
            stdout.read_to_end(&mut bytes).map(|_| bytes)
        });
        let err = std::thread::spawn(move || {
            let mut bytes = Vec::new();
            stderr.read_to_end(&mut bytes).map(|_| bytes)
        });
        let limit = Duration::from_millis(self.input.call_timeout_ms);
        let status = loop {
            if let Some(status) = child.try_wait()? {
                break status;
            }
            if started.elapsed() > limit {
                let _ = child.kill();
                let _ = child.wait();
                bail!("CLI tool call exceeded {} ms", self.input.call_timeout_ms);
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        let stdout = out
            .join()
            .map_err(|_| anyhow::anyhow!("CLI stdout reader panicked"))??;
        let stderr = err
            .join()
            .map_err(|_| anyhow::anyhow!("CLI stderr reader panicked"))??;
        if stdout.is_empty() && !stderr.is_empty() {
            bail!(
                "CLI exit {:?} without JSON: {}",
                status.code(),
                String::from_utf8_lossy(&stderr[..stderr.len().min(2048)])
            );
        }
        Ok((status.code(), stdout, started.elapsed()))
    }

    /// The harness's typed-retry rule (`common.typed_retry`): repeat only a
    /// reply that asks for it and submitted no query.
    fn typed_retry(body: &Value) -> Option<u64> {
        if body["status"] == "retry"
            && body["retryable"] == true
            && body["action"] == "repeat_same_request"
        {
            return Some(body["retry_after_ms"].as_u64().unwrap_or(25));
        }
        let error = &body["error"];
        (error["retryable"] == true
            && matches!(
                error["code"].as_str(),
                Some(
                    "selection_busy"
                        | "publication_busy"
                        | "scoped_observation_unavailable"
                        | "scoped_status_unavailable"
                )
            )
            && error["query_submitted"] != true)
            .then(|| error["retry_after_ms"].as_u64().unwrap_or(250))
    }

    fn envelope_body(envelope: &Value) -> Option<Value> {
        if let Some(structured) = envelope.get("structuredContent") {
            return Some(structured.clone());
        }
        let texts: Vec<&str> = envelope["content"]
            .as_array()?
            .iter()
            .filter(|block| block["type"] == "text")
            .filter_map(|block| block["text"].as_str())
            .collect();
        match texts.as_slice() {
            [text] => serde_json::from_str(text).ok(),
            _ => None,
        }
    }

    fn read(&self, tool: &str, args: &Map<String, Value>, revision: Option<&str>) -> Result<Value> {
        // The observer's publication binds every read; the gathering marks a
        // live reply with any other token stale, so together they are equal.
        if let Some(revision) = revision
            && revision != self.publication
        {
            self.foreign_revision
                .lock()
                .map_err(|_| anyhow::anyhow!("reader lock poisoned"))?
                .get_or_insert_with(|| revision.to_owned());
            bail!("observed publication {revision} differs from the prepared live publication");
        }
        let arguments = Self::arguments(tool, args, revision)?;
        let outcome = (|| -> Result<Value> {
            let mut retries = 0u32;
            loop {
                let (code, stdout, elapsed) = self.invoke(tool, &arguments)?;
                let envelope: Value = serde_json::from_slice(&stdout)
                    .with_context(|| format!("CLI exit {code:?}: stdout is not JSON"))?;
                ensure!(envelope.is_object(), "CLI reply is not a tool result");
                let body = Self::envelope_body(&envelope);
                if let Some(delay) = body.as_ref().and_then(Self::typed_retry)
                    && retries < 8
                {
                    std::thread::sleep(Duration::from_millis((delay.max(25) << retries).min(2000)));
                    retries += 1;
                    continue;
                }
                self.calls
                    .lock()
                    .map_err(|_| anyhow::anyhow!("reader lock poisoned"))?
                    .push(json!({"tool":tool,"arguments":arguments,"exit":code,
                        "elapsed_ms":elapsed.as_millis() as u64,"typed_retries":retries,
                        "is_error":envelope["isError"] == true}));
                if envelope["isError"] == true || code != Some(0) {
                    // The canonical failure receipt, as workbench_read returns it.
                    return Err(airk_kernel::workbench::ReadFailure(envelope).into());
                }
                let body = body.context("CLI tool result has no JSON body")?;
                if tool == "search"
                    && body["focus"].is_object()
                    && !body["focus"]["receipt"].is_null()
                {
                    *self
                        .live_focus
                        .lock()
                        .map_err(|_| anyhow::anyhow!("reader lock poisoned"))? += 1;
                    ensure!(
                        self.input.allow_live_focus,
                        "live search reply carries a Focus receipt: the lifetime's Focus stage is on"
                    );
                }
                return Ok(body);
            }
        })();
        if let Err(error) = &outcome
            && !error.is::<airk_kernel::workbench::ReadFailure>()
        {
            // Never a read result: a transport failure invalidates the branch.
            self.transport_failure
                .lock()
                .map_err(|_| anyhow::anyhow!("reader lock poisoned"))?
                .get_or_insert_with(|| format!("{tool}: {error:#}"));
        }
        outcome
    }
}

struct Branch<'a> {
    input: &'a Input,
    index_id: &'a str,
    publication: &'a str,
    scoped: &'a Kernel,
    reference: ScopedReadRef,
    worker: WorkerIdentity,
    call: &'a Call,
}
impl Branch<'_> {
    fn run(&self, name: &str, force: Option<(u8, Sha256Digest)>) -> Result<Value> {
        let profile = framed_digest(b"airk.context-forks.profile/1", &[b"no-model"]);
        let controller = Arc::new(Forks {
            profile,
            tokens: self.input.assessment_input_tokens,
            force,
            seen: Mutex::default(),
            applied: Mutex::default(),
        });
        let args: AnswerArgs = serde_json::from_value(self.call.arguments.clone())
            .context("prepared airk_answer arguments")?;
        let reserve = args
            .verification
            .controls()
            .resolve()?
            .map(|budget| {
                airk_kernel::verification::verification_reply_metadata_floor(budget)
                    .map(|floor| floor.saturating_mul(2).saturating_add(2048))
            })
            .transpose()?
            .unwrap_or(4096);
        let digest = framed_digest(
            b"airk.context-forks.attempt/1",
            &[self.call.id.as_bytes(), name.as_bytes()],
        );
        let mut attempt_id = [0; 16];
        attempt_id.copy_from_slice(&digest.as_bytes()[..16]);
        let deadline_ms = args.request.budget.deadline_ms.min(120_000);
        let kernel = self
            .scoped
            .with_native_arbiter(self.worker, self.reference, controller.clone())?
            .with_context_attempt(ReadAttemptBudget {
                schema: 1,
                request_id: Incarnation::from_bytes(attempt_id),
                clock: if cfg!(windows) {
                    ReadClock::WindowsPerformanceCounter
                } else {
                    ReadClock::UnixMonotonic
                },
                expires_monotonic_ns: u64::MAX,
                remaining_ns: Duration::from_millis(deadline_ms)
                    .min(MAX_READ_BUDGET)
                    .as_nanos() as u64,
                native_retry_expires_monotonic_ns: None,
            })?
            .with_context_reply_reserve(reserve)?;
        let reader = LiveReader {
            input: self.input,
            index_id: self.index_id,
            publication: self.publication,
            foreign_revision: Mutex::default(),
            calls: Mutex::default(),
            transport_failure: Mutex::default(),
            live_focus: Mutex::default(),
        };
        let started = Instant::now();
        let deadline = started + Duration::from_millis(deadline_ms);
        let outcome = kernel.answer_with_reader_v2(
            args.request,
            deadline,
            &|| Ok(()),
            &|tool, args, revision| reader.read(tool, args, revision),
        );
        let elapsed = started.elapsed();
        if let Some(revision) = reader
            .foreign_revision
            .lock()
            .map_err(|_| anyhow::anyhow!("reader lock poisoned"))?
            .clone()
        {
            bail!(
                "observed index publication {revision} differs from the prepared live publication {} (the live index published after preparation)",
                self.publication
            );
        }
        let native = outcome.as_ref().ok().map(|response| {
            json!({"stop":response.stop,"work":response.work,"candidate_count":response.candidate_count,
                "omitted_count":response.omitted_count,
                "records":response.records.iter().map(|record| json!({"id":record.id,
                    "operation":record.operation,"arguments":record.arguments})).collect::<Vec<_>>(),
                "assessments":response.assessment_history.iter().map(|event| json!({"ordinal":event.ordinal,
                    "applicability":event.applicability,"result":match &event.result {
                        airk_core::arbiter::context_action::AssessmentResult::Selected{card,..} =>
                            json!({"status":"selected","card":card.id,"action":card.action}),
                        airk_core::arbiter::context_action::AssessmentResult::Abstained{..} =>
                            json!({"status":"abstained"}),
                        airk_core::arbiter::context_action::AssessmentResult::Unavailable{reason,..} =>
                            json!({"status":"unavailable","reason":reason}),
                    }})).collect::<Vec<_>>()})
        });
        let error = outcome.as_ref().err().map(|error| format!("{error:#}"));
        let span = kernel.usage_span(UsageOperation::Answer, airk_core::usage::ClientFamily::Cli);
        let responses =
            response::Session::for_scope(kernel.scoped_read_fingerprint().ok().flatten());
        let reply = crate::render(
            &kernel,
            (span, false),
            outcome,
            &responses,
            (
                args.response
                    .response_mode
                    .unwrap_or(response::ResponseMode::Terse),
                args.response.reply_detail,
                args.verification.controls().resolve()?,
                None,
            ),
            args.response.include_status,
            UsageOperation::Answer,
        );
        let packet = match body(&reply) {
            Ok(packet) => packet,
            Err(_) => serde_json::to_value(&reply)?,
        };
        let transport = reader
            .transport_failure
            .lock()
            .map_err(|_| anyhow::anyhow!("reader lock poisoned"))?
            .clone();
        let requests = std::mem::take(
            &mut *controller
                .seen
                .lock()
                .map_err(|_| anyhow::anyhow!("fork lock poisoned"))?,
        );
        let applied = *controller
            .applied
            .lock()
            .map_err(|_| anyhow::anyhow!("fork lock poisoned"))?;
        let reads = std::mem::take(
            &mut *reader
                .calls
                .lock()
                .map_err(|_| anyhow::anyhow!("reader lock poisoned"))?,
        );
        let live_focus = *reader
            .live_focus
            .lock()
            .map_err(|_| anyhow::anyhow!("reader lock poisoned"))?;
        Ok(
            json!({"branch":name,"forced":force.map(|(ordinal,choice)| json!({"ordinal":ordinal,"choice":choice})),
            "forced_applied":applied,"valid":error.is_none() && transport.is_none() && applied != Some(false),
            "error":error,"transport_failure":transport,"elapsed_ms":elapsed.as_millis() as u64,
            "requests":requests,"native":native,"live_reads":reads,"live_focus_receipts":live_focus,
            "reply_is_error":reply.is_error == Some(true),"packet":packet}),
        )
    }
}

/// A whole-index grant on the live index identity and source root, shaped as
/// `scoped_observation_tests::authority`; the collection's permanent source
/// policy binds both, so fixed fixture identities are refused.
fn grant(
    kernel: &Kernel,
    index: &IndexEntry,
) -> Result<airk_kernel::scoped_reads::ScopedReadAuthority> {
    use airk_core::index_catalog::*;
    let project = QueryProjectId::from_identity(Incarnation::from_bytes([1; 16]));
    let index_id: IndexId = serde_json::from_value(json!(index.index_id))?;
    let source: SourceRootId = match &index.source_root_id {
        Some(id) => serde_json::from_value(json!(id))?,
        None => SourceRootId::from_identity(Incarnation::from_bytes([3; 16])),
    };
    Ok(airk_kernel::scoped_reads::ScopedReadAuthority {
        schema: 1,
        backing: airk_core::service::ProjectRegistration::new(
            kernel.root().to_str().context("non-UTF-8 root")?.into(),
            kernel
                .data_dir()
                .to_str()
                .context("non-UTF-8 data root")?
                .into(),
            kernel.collection().into(),
        )?
        .binding,
        index_id,
        project_ids: vec![project],
        provenance: airk_core::grant_provenance::GrantProvenance::UserReviewedProjectGrant,
        grant: ReviewedGrant {
            id: GrantId::from_identity(Incarnation::from_bytes([4; 16])),
            host: Sha256Digest::from_bytes([5; 32]),
            revision: CatalogRevision::INITIAL,
            revoked: false,
            memberships: vec![GrantedMembership {
                membership: Membership {
                    project_id: project,
                    index_id,
                    revision: CatalogRevision::INITIAL,
                    scope: vec![ScopeAtom {
                        source_root_id: source,
                        kind: PathScope::Subtree,
                        path: String::new(),
                    }],
                },
                source_revision: CatalogRevision::INITIAL,
                policy_epoch: 0,
                policy_digest: None,
            }],
        },
        sources: vec![airk_core::scoped_reads::SourceDescriptor {
            source_root_id: source,
            original_root: kernel.root().to_str().context("non-UTF-8 root")?.into(),
        }],
    })
}

fn path_env(name: &str) -> Result<PathBuf> {
    Ok(std::env::var_os(name)
        .with_context(|| format!("missing {name}"))?
        .into())
}

fn write_line(output: &mut std::fs::File, value: &Value) -> Result<()> {
    let mut line = serde_json::to_vec(value)?;
    line.push(b'\n');
    output.write_all(&line)?;
    output.flush()?;
    Ok(())
}

#[test]
#[ignore = "explicit private oracle leverage collection against a live evaluation lifetime; no NN"]
fn collect_context_action_oracle_forks() -> Result<()> {
    let input_path = path_env("AIRK_CONTEXT_FORKS_INPUT")?;
    let input_bytes = std::fs::read(&input_path)?;
    // Framed as identity::framed_digest; prepare_forks.py prints the same value.
    let input_digest = framed_digest(INPUT_DOMAIN, &[&input_bytes]);
    ensure!(
        input_digest == std::env::var("AIRK_CONTEXT_FORKS_INPUT_DIGEST")?.parse()?,
        "prepared collector input changed"
    );
    let input: Input = serde_json::from_slice(&input_bytes)?;
    ensure!(
        input.schema == INPUT_SCHEMA
            && !input.calls.is_empty()
            && !input.cli.is_empty()
            && !input.fork_ordinals.is_empty()
            && input
                .fork_ordinals
                .iter()
                .all(|ordinal| (1..=3).contains(ordinal))
            && (1..=8192).contains(&input.assessment_input_tokens)
            && input.call_timeout_ms > 0,
        "foreign collector input"
    );
    let layers = airk_kernel::arbiter::software::Layers::parse(&input.layers)?;
    ensure!(
        !layers.focus,
        "the collector runs without Focus: Focus kept-hits need the model"
    );
    airk_kernel::arbiter::software::bind_evaluation_layers(layers)?;
    let only: Option<Vec<String>> = std::env::var("AIRK_CONTEXT_FORKS_ONLY")
        .ok()
        .map(|ids| ids.split(',').map(str::to_owned).collect());
    let output_path = path_env("AIRK_CONTEXT_FORKS_OUTPUT")?;
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output_path)?;
    write_line(
        &mut output,
        &json!({"event":"start","schema":OUTPUT_SCHEMA,"input_digest":input_digest,"layers":layers.names(),
            "fork_ordinals":input.fork_ordinals,"assessment_input_tokens":input.assessment_input_tokens,
            "provenance":input.provenance,
            "only":only,"nn_invocations":0,
            "limits":"Forced reads are deterministic canonical reads against one live publication. The Context controller is a fixed oracle harness, not a model; no score, calibration or task qualification is implied."}),
    )?;
    let mut kernels: BTreeMap<String, (Kernel, ScopedReadRef, WorkerIdentity)> = BTreeMap::new();
    let mut rows = 0usize;
    let mut invalid_queue = Vec::new();
    for call in &input.calls {
        if only.as_ref().is_some_and(|only| !only.contains(&call.id)) {
            continue;
        }
        let index = input
            .indexes
            .get(&call.repository)
            .with_context(|| format!("no index entry for {}", call.repository))?;
        if !kernels.contains_key(&call.repository) {
            ensure!(
                index.root.is_absolute() && index.data_root.is_absolute(),
                "absolute source and index data paths required"
            );
            // A service collection binds its owner to its data root, so a copy
            // elsewhere is refused. The observer reads the existing publication
            // without creation, recovery, cleanup or model initialization.
            // Canonical spellings, as the service registered them.
            let registration = airk_core::service::ProjectRegistration::new(
                std::fs::canonicalize(&index.root)?
                    .to_str()
                    .context("non-UTF-8 source root")?
                    .into(),
                std::fs::canonicalize(&index.data_root)?
                    .to_str()
                    .context("non-UTF-8 data root")?
                    .into(),
                index.collection.clone(),
            )?;
            ensure!(
                registration.binding.to_string() == index.binding,
                "observer registration differs from the live project binding"
            );
            let base = Kernel::open_existing_service_observer(&registration)?;
            let mut authority = grant(&base, index)?;
            // The grant names the collection's own permanent source policy, as
            // the service's grant does; reads are refused under any other.
            if let Some(policy) = base.source_policy_observation()? {
                let membership = &mut authority.grant.memberships[0];
                membership.policy_epoch = serde_json::from_value(policy["policy_epoch"].clone())
                    .context("source policy epoch")?;
                membership.policy_digest = Some(
                    serde_json::from_value(policy["policy_digest"].clone())
                        .context("source policy digest")?,
                );
            }
            let scoped = base.with_scoped_reads(authority.clone())?;
            let reference = ScopedReadRef {
                schema: 1,
                lease_generation: 1,
                index_id: authority.index_id,
                grant_id: Some(authority.grant.id),
                grant_revision: Some(authority.grant.revision),
                selection: None,
                fingerprint: scoped
                    .scoped_read_fingerprint()?
                    .context("missing scoped collector source")?,
            };
            let worker = WorkerIdentity {
                service: Incarnation::from_bytes([8; 16]),
                worker: Incarnation::from_bytes([9; 16]),
                project: authority.backing,
                fencing_generation: 1,
            };
            kernels.insert(call.repository.clone(), (scoped, reference, worker));
        }
        let (scoped, reference, worker) = &kernels[&call.repository];
        let branch = Branch {
            input: &input,
            index_id: &index.index_id,
            publication: &index.publication_token,
            scoped,
            reference: *reference,
            worker: *worker,
            call,
        };
        let queue = branch.run("queue", None)?;
        let mut forks = Vec::new();
        for ordinal in &input.fork_ordinals {
            let Some(request) = queue["requests"]
                .as_array()
                .and_then(|requests| requests.iter().find(|r| r["ordinal"] == *ordinal))
            else {
                continue;
            };
            for card in request["cards"].as_array().into_iter().flatten() {
                if card["action"] != "read" {
                    continue;
                }
                let choice: Sha256Digest = serde_json::from_value(card["choice"].clone())?;
                let mut fork = branch.run(
                    &format!("o{ordinal}-r{}", card["read_slot"]),
                    Some((*ordinal, choice)),
                )?;
                // Forced reads replay the queue branch's prefix: same roster offered.
                let offered = fork["requests"]
                    .as_array()
                    .and_then(|requests| requests.iter().find(|r| r["ordinal"] == *ordinal))
                    .map(|r| roster_shape(&r["cards"]));
                fork["same_roster_as_queue"] =
                    json!(offered.as_ref() == Some(&roster_shape(&request["cards"])));
                fork["read_slot"] = card["read_slot"].clone();
                forks.push(fork);
            }
        }
        if queue["valid"] != true {
            invalid_queue.push(call.id.clone());
        }
        write_line(
            &mut output,
            &json!({"event":"call","id":call.id,"scenario_id":call.scenario_id,"repository":call.repository,"index_id":index.index_id,
                "publication_token":index.publication_token,"arguments":call.arguments,
                "queue":queue,"forks":forks}),
        )?;
        rows += 1;
    }
    write_line(
        &mut output,
        &json!({"event":"complete","calls":rows,"invalid_queue_branches":invalid_queue,
            "input_digest":input_digest}),
    )?;
    ensure!(rows > 0, "no prepared call selected");
    // Every row is written; a call without a valid queue branch is still a failure.
    ensure!(
        invalid_queue.is_empty(),
        "queue branch invalid for {invalid_queue:?}; see the rows"
    );
    Ok(())
}

/// Regression for the collector's controller on a small local index (no live
/// service): the queue branch abstains, and a forced offered Read is accepted by
/// the kernel's advice checks and runs next.
#[test]
fn forced_offered_read_is_accepted_and_runs_next() -> Result<()> {
    use airk_core::arbiter::{
        Budget,
        context_action::{
            AssessmentResult, ContextRequestV2, ContextResponseV2, REQUEST_SCHEMA_V2,
        },
    };
    let fixture = tempfile::tempdir()?;
    let root = fixture.path().join("source");
    std::fs::create_dir_all(root.join("public"))?;
    for i in 0..4 {
        let text: String = (1..=80)
            .map(|line| {
                if line == 40 {
                    format!(
                        "amber_fork owns policy {i} of the original request.
"
                    )
                } else {
                    format!(
                        "Line {line} of policy {i}: background text.
"
                    )
                }
            })
            .collect();
        std::fs::write(root.join(format!("public/policy-{i}.txt")), text)?;
    }
    let base = Kernel::open(&root, Some(fixture.path().join("data")))?;
    base.configure_vectors(false)?;
    base.index()?;
    let authority = crate::scoped_observation_tests::authority(&base)?;
    let scoped = base.with_scoped_reads(authority.clone())?;
    let reference = ScopedReadRef {
        schema: 1,
        lease_generation: 1,
        index_id: authority.index_id,
        grant_id: Some(authority.grant.id),
        grant_revision: Some(authority.grant.revision),
        selection: None,
        fingerprint: scoped.scoped_read_fingerprint()?.context("scoped source")?,
    };
    let worker = WorkerIdentity {
        service: Incarnation::from_bytes([8; 16]),
        worker: Incarnation::from_bytes([9; 16]),
        project: authority.backing,
        fencing_generation: 1,
    };
    let run = |force: Option<(u8, Sha256Digest)>| -> Result<(Vec<Value>, Option<bool>, ContextResponseV2)> {
        let controller = Arc::new(Forks {
            profile: Sha256Digest::from_bytes([64; 32]),
            tokens: 100,
            force,
            seen: Mutex::default(),
            applied: Mutex::default(),
        });
        let kernel = scoped
            .with_native_arbiter(worker, reference, controller.clone())?
            .with_context_attempt(ReadAttemptBudget {
                schema: 1,
                request_id: Incarnation::from_bytes([65; 16]),
                clock: if cfg!(windows) {
                    ReadClock::WindowsPerformanceCounter
                } else {
                    ReadClock::UnixMonotonic
                },
                expires_monotonic_ns: u64::MAX,
                remaining_ns: 30_000_000_000,
                native_retry_expires_monotonic_ns: None,
            })?;
        let reader = super::Reader {
            kernel: &kernel,
            responses: std::cell::RefCell::default(),
        };
        let response = kernel.answer_with_reader_v2(
            ContextRequestV2 {
                schema: REQUEST_SCHEMA_V2.into(),
                question: "amber_fork".into(),
                budget: Budget::default(),
            },
            Instant::now() + Duration::from_secs(30),
            &|| Ok(()),
            &|tool, args, revision| reader.read(tool, args, revision),
        )?;
        let seen = controller.seen.lock().unwrap().clone();
        let applied = *controller.applied.lock().unwrap();
        Ok((seen, applied, response))
    };
    let (queue_requests, applied, queue) = run(None)?;
    assert_eq!(applied, None);
    let first = queue_requests
        .first()
        .context("a Context request at ordinal 1")?;
    assert_eq!(first["ordinal"], 1);
    let reads: Vec<&Value> = first["cards"]
        .as_array()
        .context("cards")?
        .iter()
        .filter(|card| card["action"] == "read")
        .collect();
    assert!(reads.len() >= 2, "{first}");
    assert!(
        queue
            .assessment_history
            .iter()
            .all(|event| matches!(event.result, AssessmentResult::Abstained { .. }))
    );
    let shown = first["records"].as_u64().context("records")? as usize;
    let path = |card: &Value| -> String {
        card["target"]
            .as_str()
            .and_then(|target| target.rsplit_once(':'))
            .map(|(path, _)| path.to_owned())
            .unwrap_or_default()
    };
    // Queue order runs the first offered read next.
    assert_eq!(
        queue.records[shown].arguments["path"],
        json!(path(reads[0]))
    );
    let second: Sha256Digest = serde_json::from_value(reads[1]["choice"].clone())?;
    assert_ne!(path(reads[0]), path(reads[1]));
    let (fork_requests, applied, fork) = run(Some((1, second)))?;
    assert_eq!(applied, Some(true));
    assert_eq!(
        roster_shape(&fork_requests[0]["cards"]),
        roster_shape(&first["cards"])
    );
    match &fork.assessment_history[0].result {
        AssessmentResult::Selected { card, .. } => {
            assert_eq!(card.action, ContextAction::Read { choice: second })
        }
        other => panic!("forced read not selected: {other:?}"),
    }
    assert_eq!(fork.records[shown].arguments["path"], json!(path(reads[1])));
    Ok(())
}
