// SPDX-License-Identifier: Apache-2.0
//! Actual optional next-read scoring under the installed native worker scope.
use super::*;

impl Owner {
    pub(in crate::service_runtime) async fn next_read(
        &self,
        request: NativeNextReadRequest,
        supervisor: &airk_supervisor::project_supervisor::Supervisor,
    ) -> Result<Option<Advice>> {
        let started = Instant::now();
        let attempt = request
            .read_attempt
            .context("native Arbiter attempt budget required")?;
        let clock = if cfg!(windows) {
            airk_core::read_attempt::ReadClock::WindowsPerformanceCounter
        } else {
            airk_core::read_attempt::ReadClock::UnixMonotonic
        };
        let remaining = attempt
            .remaining_at(clock, airk_process::monotonic::now_ns()?)?
            .min(Duration::from_millis(request.remaining_ms.min(30_000)));
        ensure!(
            !remaining.is_zero(),
            "Arbiter original deadline expired before admission"
        );
        let deadline = started
            .checked_add(remaining)
            .context("Arbiter deadline overflow")?;
        self.validate_scope(request.worker, request.authority)?;
        ensure!(
            request.remaining_ms > 0
                && request.remaining_ms <= 30_000
                && request.remaining_input_tokens > 0
                && request.remaining_input_tokens <= 2048
                && request.question.len() <= 16384
                && request.records.len() <= 32
                && !request.choices.is_empty()
                && request.choices.len() <= 8
                && serde_json::to_vec(&request)?.len() <= 524288,
            "Arbiter next-read bounds exceeded"
        );
        ensure!(
            request
                .records
                .iter()
                .all(|record| record.scope == request.scope),
            "foreign Arbiter next-read evidence"
        );
        let application = self.application(
            request.worker,
            request.authority,
            request.scope.publication.clone(),
        )?;
        ensure!(
            application.scope.installation == request.scope.installation
                && application.scope.catalog == request.scope.catalog
                && application.scope.index_id == request.scope.index_id
                && application.scope.authorization == request.scope.authorization,
            "Arbiter next-read source identity changed"
        );
        let configuration = self.configuration()?;
        if configuration.revision != request.scope.configuration_revision
            || !configuration.eligible(request.scope.index_id, Task::ContextNextRead)
            || application.availability != Availability::Ready
        {
            return Ok(None);
        }
        #[cfg(not(feature = "local-inference"))]
        {
            let _ = (supervisor, deadline);
            Ok(None)
        }
        #[cfg(feature = "local-inference")]
        {
            use airk_core::identity::framed_digest;
            use airk_inference::inference_engine::pool::arbiter::{CompiledReceipt, ScoreRequest};
            use airk_retrieval::arbiter::tasks;
            let profile = self.execution_profile(
                configuration
                    .selected_profile
                    .context("Arbiter selected profile missing")?,
            )?;
            let profile = profile.description().clone();
            if !profile.tasks.contains(&Task::ContextNextRead) {
                return Ok(None);
            }
            let Ok(prompt) = tasks::context_next_read(&request) else {
                return Ok(None);
            };
            let planned_digest = framed_digest(
                b"airk.arbiter.planned-task-input/1",
                &[&serde_json::to_vec(&prompt)?],
            );
            let mut advice=Advice {schema:ADVICE_SCHEMA.into(),task:Task::ContextNextRead,profile:profile.manifest,runtime:profile.runtime.clone(),compiler:profile.compiler,calibration:profile.calibration,policy:profile.policy,scope:request.scope.clone(),input_digest:planned_digest,evidence:request.records.iter().map(|record|record.id).collect(),work:Work::default(),outcome:Outcome::Unavailable {reason:"The optional native call did not complete; canonical gathering remains available.".into()}};
            let gate = self.gate.clone();
            let store = self.configuration.clone();
            let worker = request.worker;
            let authority = request.authority;
            let manifest = profile.manifest;
            let live = Arc::new(move |scope: &ScopeBinding| {
                store.load().is_ok_and(|current| {
                    current.revision == scope.configuration_revision
                        && current.installation == scope.installation
                        && current.catalog == scope.catalog
                        && current.selected_profile == Some(manifest)
                        && current.eligible(scope.index_id, Task::ContextNextRead)
                }) && gate.catalog_clock().is_some_and(|clock| {
                    clock.namespace == scope.catalog
                        && gate
                            .capture_current_issue(clock.namespace, clock.revision)
                            .is_ok_and(|ticket| {
                                gate.check_scope_issue(ticket, worker, authority).is_ok()
                            })
                })
            });
            let pending = async {
                let handle = supervisor.arbiter().await?;
                // The warmed session already owns the qualified maximum peak
                // charge. Recheck that exact charge and fresh OS headroom;
                // serialized scoring never allocates a second model session.
                let resident = handle.resident(profile.manifest)?;
                let admission = supervisor.reuse_optional(resident.reservation()).await?;
                handle.score(ScoreRequest {
                    worker: request.worker.worker,
                    scope: request.scope.clone(),
                    task: Task::ContextNextRead,
                    deadline,
                    authority: live,
                    state: prompt.state,
                    question: prompt.question,
                    candidates: prompt.candidates,
                    resident,
                    admission,
                    max_input_tokens: request.remaining_input_tokens,
                })
            };
            let mut pending =
                match tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), pending)
                    .await
                {
                    Ok(Ok(pending)) => pending,
                    _ => return Ok(Some(advice)),
                };
            let compiled: Option<CompiledReceipt> = match tokio::time::timeout_at(
                tokio::time::Instant::from_std(deadline),
                &mut pending.compiled,
            )
            .await
            {
                Ok(Ok(receipt)) => Some(receipt),
                _ => {
                    pending.compiled.close();
                    pending.compiled.try_recv().ok()
                }
            };
            if let Some(receipt) = compiled {
                advice.input_digest = receipt.input_digest;
                advice.work.input_tokens = receipt.input_tokens;
                advice.work.queue_us = Some(receipt.queue_us);
                advice.work.compile_us = Some(receipt.compile_us);
            }
            if let Ok(Ok(Ok(scored))) = tokio::time::timeout_at(
                tokio::time::Instant::from_std(deadline),
                &mut pending.completed,
            )
            .await
            {
                advice.input_digest = scored.input_digest;
                advice.work.input_tokens = scored.input_tokens;
                advice.work.queue_us = Some(scored.queue_us);
                advice.work.compile_us = Some(scored.compile_us);
                advice.work.inference_us = Some(scored.inference_us);
                let started = Instant::now();
                advice.outcome = match tasks::selected_read(&scored.scores, &request.choices)? {
                    Some(id) => Outcome::Accepted {
                        decision: Decision::NextRead(id),
                    },
                    None => Outcome::Abstained {
                        reason: "The trained readout did not distinguish a unique next read."
                            .into(),
                    },
                };
                advice.work.policy_us =
                    Some(started.elapsed().as_micros().min(u64::MAX as u128) as u64);
            }
            self.validate_scope(request.worker, request.authority)?;
            let current = self.configuration()?;
            if current.revision != request.scope.configuration_revision
                || !current.eligible(request.scope.index_id, Task::ContextNextRead)
            {
                advice.outcome=Outcome::Unavailable {reason:"Optional configuration changed; the learned choice was discarded after actual native work.".into()};
            }
            advice.validate()?;
            Ok(Some(advice))
        }
    }
}
