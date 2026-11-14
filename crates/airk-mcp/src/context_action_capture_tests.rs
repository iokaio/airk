// SPDX-License-Identifier: Apache-2.0
//! Private native publisher TRAIN/CAL request capture; no NN or task admission.
use crate::arbiter_handler_tests::{Disabled, body};
use crate::*;
use airk_core::{
    arbiter::context_action::{ContextActionAdvice, ContextRequestV2, REQUEST_SCHEMA_V2},
    arbiter::{Availability, Budget, IndexApplication},
    identity::{Incarnation, Sha256Digest, framed_digest},
    native_context_action::NativeContextActionRequest,
    read_attempt::{ReadAttemptBudget, ReadClock},
    service::{ScopedReadRef, WorkerIdentity},
};
use airk_kernel::arbiter::NativeController;
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    schema: String,
    facts_sha256: Sha256Digest,
    task_policy: Sha256Digest,
    data_root: PathBuf,
    questions: Vec<Question>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Question {
    id: String,
    question: String,
    split: String,
    source_group: String,
    source: Value,
}
#[derive(Debug)]
struct CaptureStopped;
impl std::fmt::Display for CaptureStopped {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("private request captured; no scoring dispatched")
    }
}
impl std::error::Error for CaptureStopped {}
#[derive(Default)]
struct Capture(Mutex<Vec<NativeContextActionRequest>>);
impl NativeController for Capture {
    fn application(
        &self,
        worker: WorkerIdentity,
        authority: ScopedReadRef,
        publication: &airk_core::evidence::RevisionEnvelope,
    ) -> Result<IndexApplication> {
        let mut application = Disabled.application(worker, authority, publication)?;
        application.availability = Availability::Ready;
        application.profile = Some(Sha256Digest::from_bytes([42; 32]));
        application.reason =
            "Private request-construction fixture; no admitted model or NN execution.".into();
        Ok(application)
    }
    fn context_action(
        &self,
        request: NativeContextActionRequest,
        _: Instant,
    ) -> Result<Option<ContextActionAdvice>> {
        let mut captured = self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("capture lock poisoned"))?;
        ensure!(
            captured.is_empty(),
            "one source episode must not mint another assessment after an unavailable reply"
        );
        captured.push(request);
        Ok(None) // No advice, score, model execution or calibration is fabricated.
    }
}
struct Reader<'a> {
    kernel: &'a Kernel,
    responses: RefCell<response::Session>,
}
impl Reader<'_> {
    fn read(&self, tool: &str, args: &Map<String, Value>, revision: Option<&str>) -> Result<Value> {
        let mut args = args.clone();
        if tool == "context"
            && let Some(revision) = revision
        {
            args.insert("revision".into(), json!(revision));
        }
        args.insert("response_mode".into(), json!("full"));
        args.insert("include_status".into(), json!(true));
        let (result, operation, reply_detail, verification) = match tool {
            "search" => {
                let args: SearchArgs = serde_json::from_value(json!(args))?;
                ensure!(
                    !args.semantic && args.mode.is_none() && args.full_chunks == Some(true),
                    "capture must retain default canonical full-chunk search policy"
                );
                let detail = args.response.reply_detail;
                let verification = args.verification.controls().resolve()?;
                let result = serde_json::to_value(self.kernel.search_response_with_navigation(
                    &airk_core::SearchQuery {
                        text: args.query,
                        path_prefix: args.path_prefix,
                        limit: args.limit,
                        max_bytes: args.max_bytes,
                    },
                    airk_kernel::current::CurrentSearchMode::Default,
                    None,
                    &airk_core::SearchFilters {
                        language: args.language,
                        evidence_kind: args.evidence_kind.map(Into::into),
                        exclude_path_prefix: args.exclude_path_prefix,
                    },
                )?)?;
                (result, UsageOperation::Search, detail, verification)
            }
            "context" => {
                let args: ContextArgs = serde_json::from_value(json!(args))?;
                let detail = args.response.reply_detail;
                let verification = args.verification.controls().resolve()?;
                let result = serde_json::to_value(self.kernel.context_conditional(
                    &args.path,
                    args.line,
                    args.before,
                    args.after,
                    args.max_bytes,
                    &args.evidence.options()?,
                )?)?;
                (result, UsageOperation::Context, detail, verification)
            }
            _ => anyhow::bail!("foreign native capture read"),
        };
        let span = self
            .kernel
            .usage_span(operation, airk_core::usage::ClientFamily::Cli);
        let returned = body(&crate::render(
            self.kernel,
            (span, false),
            Ok(result),
            &self.responses.borrow(),
            (
                response::ResponseMode::Full,
                reply_detail,
                verification,
                None,
            ),
            true,
            operation,
        ))?;
        if let Some(expected) = revision {
            let actual: airk_core::evidence::RevisionEnvelope =
                serde_json::from_value(returned["publication"].clone())?;
            ensure!(
                actual.token.to_string() == expected,
                "canonical counterfactual publication changed"
            );
        }
        Ok(returned)
    }
}
fn path_env(name: &str) -> Result<PathBuf> {
    Ok(std::env::var_os(name)
        .with_context(|| format!("missing {name}"))?
        .into())
}

#[test]
#[ignore = "explicit private TRAIN/CAL canonical capture; no optional NN, DEV or FINAL"]
fn capture_publisher_context_action_native_requests_and_counterfactuals() -> Result<()> {
    let input_bytes = std::fs::read(path_env("AIRK_CONTEXT_ACTION_CAPTURE_INPUT")?)?;
    let input_digest = framed_digest(
        b"airk.context-action.publisher-source-input/1",
        &[&input_bytes],
    );
    ensure!(
        input_digest == std::env::var("AIRK_CONTEXT_ACTION_CAPTURE_INPUT_DIGEST")?.parse()?,
        "publisher source input changed"
    );
    let input: Input = serde_json::from_slice(&input_bytes)?;
    ensure!(
        input.schema == "airk.arbiter.context-action.publisher-sources/1"
            && input.facts_sha256.to_string()
                == "115f9baacef8d9cac1c9602447a854a11787ed759e2f61897eb01009ae6d4254"
            && !input.questions.is_empty()
            && input.questions.len() <= 126,
        "foreign publisher input"
    );
    let mode = std::env::var("AIRK_CONTEXT_ACTION_CAPTURE_MODE")?;
    ensure!(
        matches!(mode.as_str(), "lexical" | "hybrid"),
        "explicit lexical/hybrid capture mode required"
    );
    let output = path_env("AIRK_CONTEXT_ACTION_CAPTURE_OUTPUT")?;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output)?;
    ensure!(
        input.data_root.is_absolute(),
        "absolute fresh private index root required"
    );
    std::fs::create_dir(&input.data_root)?;
    let owner = airk_kernel::inference_owner::NativeInferenceOwner::admit_private_fixture(
        &path_env("AIRK_CONTEXT_ACTION_PRIVATE_CONTROL")?,
    )?;
    let mut bases = BTreeMap::new();
    let mut rows = Vec::new();
    let mut seen = BTreeSet::new();
    for question in input.questions {
        ensure!(
            seen.insert(question.id.clone())
                && matches!(question.split.as_str(), "train" | "calibration"),
            "duplicate/foreign publisher partition"
        );
        let source_root = PathBuf::from(
            question.source["source_root"]
                .as_str()
                .context("missing source root")?,
        );
        ensure!(
            source_root.is_absolute(),
            "absolute publisher source root required"
        );
        let source_before = std::fs::read(source_root.join("source.txt"))?;
        let source_digest = framed_digest(
            b"airk.context-action.publisher-source-bytes/1",
            &[&source_before],
        );
        let base = if let Some(base) = bases.get(&source_root) {
            Kernel::clone(base)
        } else {
            let base = Kernel::open(
                &source_root,
                Some(input.data_root.join(format!("index-{}", bases.len()))),
            )?;
            owner.run(|| -> Result<()> {
                base.configure_vectors(mode == "hybrid")?;
                if mode == "hybrid" {
                    base.install_default_vector_model(&path_env("AIRK_CONTEXT_ACTION_ENCODER")?)?;
                }
                base.index()?;
                Ok(())
            })??;
            bases.insert(source_root.clone(), base.clone());
            base
        };
        let mut authority = crate::scoped_observation_tests::authority(&base)?;
        authority.grant.memberships[0].membership.scope[0]
            .path
            .clear();
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
                .context("missing scoped capture source")?,
        };
        let worker = WorkerIdentity {
            service: Incarnation::from_bytes([8; 16]),
            worker: Incarnation::from_bytes([9; 16]),
            project: authority.backing,
            fencing_generation: 1,
        };
        let capture = Arc::new(Capture::default());
        let attempt_digest = framed_digest(
            b"airk.context-action.publisher-attempt/1",
            &[question.id.as_bytes()],
        );
        let mut attempt_id = [0; 16];
        attempt_id.copy_from_slice(&attempt_digest.as_bytes()[..16]);
        let kernel = scoped
            .with_native_arbiter(worker, reference, capture.clone())?
            .with_context_attempt(ReadAttemptBudget {
                schema: 1,
                request_id: Incarnation::from_bytes(attempt_id),
                clock: if cfg!(windows) {
                    ReadClock::WindowsPerformanceCounter
                } else {
                    ReadClock::UnixMonotonic
                },
                expires_monotonic_ns: 30_000_000_000,
                remaining_ns: 30_000_000_000,
                native_retry_expires_monotonic_ns: None,
            })?;
        let started = Instant::now();
        let deadline = started + Duration::from_secs(30);
        let (packet,counterfactuals) = owner.run(|| -> Result<_> {
            let reader = Reader{kernel:&kernel,responses:RefCell::default()};
            let outcome = kernel.answer_with_reader_v2(
                ContextRequestV2{schema:REQUEST_SCHEMA_V2.into(),question:question.question.clone(),budget:Budget::default()},
                deadline,&||{
                    ensure!(Instant::now()<deadline,"original canonical capture deadline expired");
                    if !capture.0.lock().map_err(|_|anyhow::anyhow!("capture lock poisoned"))?.is_empty() {
                        return Err(CaptureStopped.into());
                    }
                    Ok(())
                },
                &|tool,args,revision|reader.read(tool,args,revision)
            );
            let packet = match outcome {
                Ok(packet) => Some(packet),
                Err(error) if error.is::<CaptureStopped>() => None,
                Err(error) => return Err(error),
            };
            let captured = capture.0.lock().map_err(|_|anyhow::anyhow!("capture lock poisoned"))?;
            let mut outcomes = Vec::new();
            if let Some(request) = captured.first() {
                ensure!(request.roster.policy == input.task_policy,"current native task policy changed after preparation");
                if mode == "hybrid" {
                    ensure!(request.records[0].canonical_result["execution"]["served_mode"] == "hybrid"
                        && request.records[0].canonical_result["execution"]["incomplete"] == false,
                        "native hybrid population cannot silently fall back");
                }
                for choice in request.choices.iter().filter(|choice| request.roster.basis.read_choices_scored.contains(&choice.id)) {
                    ensure!(Instant::now()<deadline,"original counterfactual capture deadline expired");
                    let (tool,args) = if choice.operation == airk_core::arbiter::NativeRead::Search {
                        let literal = airk_core::arbiter::literal::SourceLiteral::parse(&choice.target)?.context("verified literal missing")?;
                        ("search",json!({"query":literal.query,"limit":4,"max_bytes":4096,"full_chunks":true}))
                    } else {
                        ensure!(choice.operation == airk_core::arbiter::NativeRead::Context,"foreign counterfactual operation");
                        let (path,range) = choice.target.rsplit_once(':').context("verified context range missing")?;
                        let (start,_) = range.split_once('-').context("verified context line missing")?;
                        ("context",json!({"path":path,"line":start.parse::<usize>()?,"before":0,"after":32,"max_bytes":4096}))
                    };
                    let result = reader.read(tool,args.as_object().unwrap(),Some(&request.scope.publication.token.to_string()))?;
                    outcomes.push(json!({"choice":choice,"arguments":args,"canonical_result":result}));
                }
            }
            Ok((packet,outcomes))
        })??;
        ensure!(
            source_before == std::fs::read(source_root.join("source.txt"))?,
            "publisher source changed during capture"
        );
        let captured = capture
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("capture lock poisoned"))?;
        let status = if captured.is_empty() {
            "no_native_assessment_request"
        } else {
            "native_request_captured"
        };
        rows.push(json!({"id":question.id,"split":question.split,"source_group":question.source_group,
            "source":question.source,"source_framed_digest":source_digest,"status":status,
            "request":captured.first(),"canonical_packet":packet,"counterfactual_choices":counterfactuals}));
        let document = json!({"schema":"airk.arbiter.context-action.publisher-capture/1","complete":false,
            "facts_sha256":input.facts_sha256,"prepared_input_digest":input_digest,"task_policy":input.task_policy,
            "read_population":mode,"nn_invocations":0,"fixture_scope_and_clock":true,
            "counterfactual_scope":"Only up to five actually offered reads; independent before-state forks, not a learned trajectory. Capture stops before canonical fallback consumes more calls.",
            "limits":"Canonical native fixture-source semantics only; no live service authority, optional-model admission, task qualification or SDK/installed transport claim.","rows":rows});
        std::fs::write(&output, serde_json::to_vec_pretty(&document)?)?;
    }
    let mut final_document: Value = serde_json::from_slice(&std::fs::read(&output)?)?;
    final_document["complete"] = json!(true);
    std::fs::write(output, serde_json::to_vec_pretty(&final_document)?)?;
    Ok(())
}
