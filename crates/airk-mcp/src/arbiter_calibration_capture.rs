// SPDX-License-Identifier: Apache-2.0
//! Private calibration transcript of actual canonical reads; no model admission.
use crate::arbiter_handler_tests::{Disabled, body};
use crate::*;
use airk_core::service::NativeNextReadRequest;
use airk_core::{
    arbiter::*,
    identity::Incarnation,
    service::{ScopedReadRef, WorkerIdentity},
};
use airk_kernel::arbiter::{Gathering, NativeController};
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::json;
use std::{
    collections::BTreeMap,
    path::PathBuf,
    time::{Duration, Instant},
};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    data_root: PathBuf,
    questions: Vec<Question>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Question {
    id: String,
    source_root: PathBuf,
    question: String,
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "explicit private calibration-only source fixture; not final human scoring"]
async fn calibration_choices_retain_actual_before_and_counterfactual_canonical_read_spans()
-> Result<()> {
    capture(false).await
}

#[cfg(all(feature = "local-inference", feature = "test-support"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "actual trained-v2 native hybrid canonical closures; no SDK transport qualification"]
async fn hybrid_calibration_choices_retain_actual_owned_encoder_and_canonical_read_spans()
-> Result<()> {
    capture(true).await
}

async fn capture(hybrid: bool) -> Result<()> {
    let input: Input = serde_json::from_slice(&std::fs::read(
        std::env::var_os("AIRK_ARBITER_CALIBRATION_CAPTURE_INPUT")
            .context("missing capture input")?,
    )?)?;
    let output = std::env::var_os("AIRK_ARBITER_CALIBRATION_CAPTURE_OUTPUT")
        .context("missing capture output")?;
    ensure!(
        !input.questions.is_empty() && input.questions.len() <= 29,
        "only the frozen calibration partition is accepted"
    );
    ensure!(
        input.data_root.is_absolute(),
        "private explicit data root required"
    );
    #[cfg(all(feature = "local-inference", feature = "test-support"))]
    let owner = if hybrid {
        let control = input.data_root.join("hybrid-owner-control");
        std::fs::create_dir_all(&control)?;
        Some(airk_kernel::inference_owner::NativeInferenceOwner::admit_private_fixture(&control)?)
    } else {
        None
    };
    #[cfg(not(all(feature = "local-inference", feature = "test-support")))]
    ensure!(
        !hybrid,
        "native local-inference/test-support features required for hybrid calibration"
    );
    let mut kernels = BTreeMap::new();
    let mut rows = Vec::new();
    let mut ids = std::collections::BTreeSet::new();
    for question in input.questions {
        ensure!(
            ids.insert(question.id.clone()) && question.source_root.is_absolute(),
            "duplicate calibration ID or relative source"
        );
        let root = std::fs::canonicalize(&question.source_root)?;
        let base = if let Some(base) = kernels.get(&root) {
            Kernel::clone(base)
        } else {
            let base = Kernel::open(
                &root,
                Some(
                    (if hybrid {
                        input.data_root.join("hybrid")
                    } else {
                        input.data_root.clone()
                    })
                    .join(format!("index-{}", kernels.len())),
                ),
            )?;
            if hybrid {
                #[cfg(all(feature = "local-inference", feature = "test-support"))]
                {
                    let model = PathBuf::from(
                        std::env::var_os("AIRK_ARBITER_CALIBRATION_CAPTURE_ENCODER")
                            .context("missing actual trained-v2 CPU-selected encoder directory")?,
                    );
                    owner.as_ref().unwrap().run(|| -> Result<()> {
                        base.configure_vectors(true)?;
                        base.install_default_vector_model(&model)?;
                        base.index()?;
                        Ok(())
                    })??;
                }
            } else {
                base.configure_vectors(false)?;
                base.index()?;
            }

            kernels.insert(root.clone(), base.clone());
            base
        };
        let mut authority = crate::scoped_observation_tests::authority(&base)?;
        authority.grant.memberships[0].membership.scope[0]
            .path
            .clear();
        let kernel = base.with_scoped_reads(authority.clone())?;
        let reference = ScopedReadRef {
            schema: 1,
            lease_generation: 1,
            index_id: authority.index_id,
            grant_id: Some(authority.grant.id),
            grant_revision: Some(authority.grant.revision),
            selection: None,
            fingerprint: kernel
                .scoped_read_fingerprint()?
                .context("missing actual fixture scope")?,
        };
        let worker = WorkerIdentity {
            service: Incarnation::from_bytes([8; 16]),
            worker: Incarnation::from_bytes([9; 16]),
            project: authority.backing,
            fencing_generation: 1,
        };
        let mut tools = if hybrid {
            #[cfg(all(feature = "local-inference", feature = "test-support"))]
            {
                CaptureClient::Hybrid {
                    kernel: Box::new(kernel),
                    owner: owner.as_ref().unwrap().clone(),
                    responses: Box::default(),
                }
            }
            #[cfg(not(all(feature = "local-inference", feature = "test-support")))]
            {
                unreachable!("hybrid feature refusal above")
            }
        } else {
            CaptureClient::Sdk(Box::new(local::LocalTools::new(kernel).await?))
        };
        let started = Instant::now();
        let deadline = started + Duration::from_secs(30);
        let mut search_args=json!({"query":question.question,"limit":8,"max_bytes":4096,"full_chunks":true,"response_mode":"full","include_status":true}).as_object().unwrap().clone();
        let initial = tools
            .call("airk_search", std::mem::take(&mut search_args))
            .await?;
        if hybrid {
            ensure!(
                initial["execution"]["served_mode"] == "hybrid"
                    && initial["execution"]["incomplete"] == false,
                "actual trained-v2 hybrid population must not silently fallback"
            );
        }
        let publication = serde_json::from_value(initial["publication"].clone())?;
        let application = Disabled.application(worker, reference, &publication)?;
        let scope = application.scope.clone();
        let mut gathering = Gathering::new(
            ContextRequest {
                schema: REQUEST_SCHEMA.into(),
                question: question.question.clone(),
                budget: Budget::default(),
            },
            scope.clone(),
            application,
            started,
        )?;
        let search = gathering
            .next(started)?
            .context("missing initial canonical dispatch")?;
        ensure!(search.tool == "search", "unexpected initial operation");
        assert_eq!(
            search.arguments,
            json!({"query":question.question,"limit":8,"max_bytes":4096,"full_chunks":true})
                .as_object()
                .unwrap()
                .clone()
        );
        gathering.observe(Ok(initial.clone()), Instant::now())?;
        let choices = gathering.choices();
        let request = NativeNextReadRequest {
            read_attempt: None,
            worker,
            authority: reference,
            scope: scope.clone(),
            question: question.question,
            records: gathering.records().to_vec(),
            choices: choices.clone(),
            remaining_ms: deadline
                .saturating_duration_since(Instant::now())
                .as_millis()
                .min(30_000) as u64,
            remaining_input_tokens: 2048,
        };
        let mut outcomes = Vec::new();
        for choice in choices {
            ensure!(
                Instant::now() < deadline,
                "original capture deadline expired"
            );
            let (tool, args) = if choice.operation == NativeRead::Search {
                let source = literal::SourceLiteral::parse(&choice.target)?
                    .context("literal target provenance missing")?;
                (
                    "airk_search",
                    json!({"query":source.query,"limit":4,"max_bytes":4096,"full_chunks":true,"response_mode":"full","include_status":true}),
                )
            } else {
                let (path, range) = choice
                    .target
                    .rsplit_once(':')
                    .context("missing native target range")?;
                let (start, _end) = range
                    .split_once('-')
                    .context("missing native line bounds")?;
                (
                    "airk_context",
                    json!({"path":path,"line":start.parse::<usize>()?,"before":0,"after":32,"max_bytes":4096,"revision":scope.publication.token.to_string(),"response_mode":"full","include_status":true}),
                )
            };
            let returned = tools.call(tool, args.as_object().unwrap().clone()).await?;
            outcomes.push(json!({"choice":choice,"arguments":args,"canonical_result":returned}));
        }
        tools.close().await?;
        rows.push(json!({"id":question.id,"request":request,"before":initial,"counterfactual_choices":outcomes}));
        std::fs::write(
            &output,
            serde_json::to_vec_pretty(
                &json!({"schema":"airk.arbiter.native-calibration-capture/1","evaluation":true,"release_qualified":false,"read_population":if hybrid {"trained-v2-native-default-hybrid"}else{"lexical"},"scope":if hybrid {"Actual service-lease-owned trained-v2 inference and exact native canonical search/context closures with fixture scoped authority; NOT SDK transport qualification, learned selection, sufficiency or model qualification."}else{"lexical canonical read-only calibration transcripts, fixture scoped authority; no learned selection, sufficiency labels, installed transport or model qualification claim"},"rows":rows}),
            )?,
        )?;
    }
    Ok(())
}

enum CaptureClient {
    Sdk(Box<local::LocalTools>),
    #[cfg(all(feature = "local-inference", feature = "test-support"))]
    Hybrid {
        kernel: Box<Kernel>,
        owner: airk_kernel::inference_owner::NativeInferenceOwner,
        responses: Box<response::Session>,
    },
}
impl CaptureClient {
    async fn call(
        &mut self,
        tool: &str,
        arguments: serde_json::Map<String, serde_json::Value>,
    ) -> Result<serde_json::Value> {
        match self {
            Self::Sdk(tools) => body(&tools.call(tool.into(), arguments).await?),
            #[cfg(all(feature = "local-inference", feature = "test-support"))]
            Self::Hybrid {
                kernel,
                owner,
                responses,
            } => owner.run(|| -> Result<serde_json::Value> {
                kernel
                    .scoped_read_fingerprint()?
                    .context("actual scoped hybrid source missing")?;
                let (result, operation, reply_detail, verification) = match tool {
                    "airk_search" => {
                        let args: SearchArgs =
                            serde_json::from_value(serde_json::Value::Object(arguments))?;
                        ensure!(
                            !args.semantic && args.mode.is_none() && args.full_chunks == Some(true),
                            "capture uses exactly canonical saved hybrid/full-chunk policy"
                        );
                        let reply_detail = args.response.reply_detail;
                        let verification = args.verification.controls().resolve()?;
                        let result =
                            serde_json::to_value(kernel.search_response_with_navigation(
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
                        (result, UsageOperation::Search, reply_detail, verification)
                    }
                    "airk_context" => {
                        let args: ContextArgs =
                            serde_json::from_value(serde_json::Value::Object(arguments))?;
                        let reply_detail = args.response.reply_detail;
                        let verification = args.verification.controls().resolve()?;
                        let result = serde_json::to_value(kernel.context_conditional(
                            &args.path,
                            args.line,
                            args.before,
                            args.after,
                            args.max_bytes,
                            &args.evidence.options()?,
                        )?)?;
                        (result, UsageOperation::Context, reply_detail, verification)
                    }
                    _ => anyhow::bail!("unsupported canonical calibration read"),
                };
                // Same full renderer and default verification/status projection
                // as admitted Workbench canonical reads; only SDK transport is absent.
                let span = kernel.usage_span(operation, airk_core::usage::ClientFamily::Cli);
                let reply = crate::render(
                    kernel,
                    (span, false),
                    Ok(result),
                    responses,
                    (
                        response::ResponseMode::Full,
                        reply_detail,
                        verification,
                        None,
                    ),
                    true,
                    operation,
                );
                let result = body(&reply)?;
                kernel
                    .scoped_read_fingerprint()?
                    .context("actual scoped hybrid source revoked")?;
                Ok(result)
            })?,
        }
    }
    async fn close(self) -> Result<()> {
        match self {
            Self::Sdk(mut tools) => tools.close().await,
            #[cfg(all(feature = "local-inference", feature = "test-support"))]
            Self::Hybrid { .. } => Ok(()), // No SDK/process is created by this native closure fixture.
        }
    }
}
