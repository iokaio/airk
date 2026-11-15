// SPDX-License-Identifier: Apache-2.0
//! Private test-support observations, never native admission or an inside-FFI claim.
use super::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    sync::{Weak, atomic::AtomicU64},
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::sync::mpsc::{self, Receiver, Sender};
pub const CAPACITY: usize = 4096;
const MAX_CHARGES: usize = 64;
#[derive(Clone, Serialize, Deserialize)]
pub struct Binding {
    pub observer_id: Incarnation,
    pub dispatch_identity: u64,
    pub profile_manifest: Sha256Digest,
    pub task: Task,
    pub input_digest: Sha256Digest,
    pub question_utf8_sha256: Sha256Digest,
    pub original_deadline_unix_ms: u64,
    pub tokens: usize,
    pub charge_identity: String,
    pub charge_bytes: u64,
    pub scope: ScopeBinding,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    DispatchAdmitted,
    ScoreReturned,
    JobSettled,
    ProfileReleased,
    ChargeReleased,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Success,
    Error,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecorderSummary {
    pub schema: String,
    pub complete: bool,
    pub evaluation: bool,
    pub release_qualified: bool,
    pub dropped: u64,
    pub writer_failed: usize,
    pub records: usize,
    pub bytes: u64,
    pub last_sequence: u64,
    pub error: Option<String>,
    pub proof: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub schema: String,
    pub sequence: u64,
    pub unix_ms: u64,
    pub kind: Kind,
    #[serde(flatten)]
    pub binding: Binding,
    pub outcome: Option<Outcome>,
    pub finite_raw_scores: Option<bool>,
    pub raw_scores: Option<Vec<Option<f32>>>,
    pub raw_score_bits: Option<Vec<u32>>,
    pub work: Option<ScoreReceipt>,
    pub error: Option<String>,
    pub return_observed: bool,
    pub proof: Option<String>,
}
pub struct Event {
    pub record: Record,
    pub release: Option<ReleaseProof>,
}
/// Opaque actual charge completion, constructed only at native dispatch.
pub struct ReleaseProof {
    observer: Observer,
    binding: Binding,
    release: airk_kernel::service_resources::OptionalRelease,
}
pub struct ReleasedCharge {
    observer: Observer,
    binding: Binding,
}
impl ReleaseProof {
    pub async fn wait(self) -> Result<ReleasedCharge> {
        self.release.wait().await?;
        Ok(ReleasedCharge {
            observer: self.observer,
            binding: self.binding,
        })
    }
}
impl ReleasedCharge {
    pub fn emit_for(&self, bindings: &[Binding]) -> Result<()> {
        for binding in bindings {
            ensure!(
                binding.observer_id == self.binding.observer_id
                    && binding.charge_identity == self.binding.charge_identity
                    && binding.profile_manifest == self.binding.profile_manifest
                    && binding.charge_bytes == self.binding.charge_bytes,
                "crossed actual native charge release evidence"
            );
        }
        for binding in bindings {
            for kind in [Kind::ProfileReleased, Kind::ChargeReleased] {
                let mut record = Observer::record(binding.clone(), kind);
                record.proof = Some(
                    "actual_final_reservation_drop_after_native_scorer_and_source_teardown".into(),
                );
                self.observer.emit(record, None);
            }
        }
        Ok(())
    }
}
struct State {
    sender: Option<Sender<Event>>,
    sequence: u64,
    next_dispatch: u64,
    charges: BTreeMap<usize, (Weak<Reservation>, u64)>,
}
struct Inner {
    id: Incarnation,
    state: Mutex<State>,
    dropped: AtomicU64,
}
#[derive(Clone)]
pub struct Observer(Arc<Inner>);
impl Observer {
    pub fn channel(capacity: usize) -> Result<(Self, Receiver<Event>)> {
        ensure!(
            (1..=CAPACITY).contains(&capacity),
            "private observer capacity refused"
        );
        let (sender, receiver) = mpsc::channel(capacity);
        Ok((
            Self(Arc::new(Inner {
                id: airk_kernel::service_broker::new_instance_id()?,
                state: Mutex::new(State {
                    sender: Some(sender),
                    sequence: 0,
                    next_dispatch: 0,
                    charges: BTreeMap::new(),
                }),
                dropped: AtomicU64::new(0),
            })),
            receiver,
        ))
    }
    fn lost(&self) {
        let _ = self
            .0
            .dropped
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                Some(value.saturating_add(1))
            });
    }
    pub fn dropped(&self) -> u64 {
        self.0.dropped.load(Ordering::Acquire)
    }
    /// Only after actual fixture service/native owners have settled. No runtime path calls this.
    pub fn close(&self) {
        if let Ok(mut state) = self.0.state.lock() {
            state.sender = None;
        } else {
            self.lost();
        }
    }
    fn timestamp() -> Option<u64> {
        u64::try_from(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .ok()?
                .as_millis(),
        )
        .ok()
    }
    fn emit(&self, mut record: Record, release: Option<ReleaseProof>) {
        let Ok(mut state) = self.0.state.try_lock() else {
            self.lost();
            return;
        };
        let Some(next) = state.sequence.checked_add(1) else {
            self.lost();
            return;
        };
        let Some(unix_ms) = Self::timestamp() else {
            self.lost();
            return;
        };
        state.sequence = next;
        record.sequence = next;
        record.unix_ms = unix_ms;
        if state
            .sender
            .as_ref()
            .is_none_or(|sender| sender.try_send(Event { record, release }).is_err())
        {
            self.lost();
        }
    }
    fn record(binding: Binding, kind: Kind) -> Record {
        Record {
            schema: "airk.private.native-dispatch/1".into(),
            sequence: 0,
            unix_ms: 0,
            kind,
            binding,
            outcome: None,
            finite_raw_scores: None,
            raw_scores: None,
            raw_score_bits: None,
            work: None,
            error: None,
            return_observed: false,
            proof: None,
        }
    }
    pub(super) fn prepare(&self, job: &Job, compiled: &CompiledReceipt) -> Option<Prepared> {
        let key = Arc::as_ptr(&job.resident.reservation) as usize;
        let (charge, dispatch_identity) = {
            let Ok(mut state) = self.0.state.try_lock() else {
                self.lost();
                return None;
            };
            let Some(dispatch) = state.next_dispatch.checked_add(1) else {
                self.lost();
                return None;
            };
            state.next_dispatch = dispatch;
            let charge = if let Some((_, id)) = state.charges.get(&key) {
                *id
            } else {
                if state.charges.len() >= MAX_CHARGES {
                    self.lost();
                    return None;
                }
                let id = state.charges.len() as u64 + 1;
                state
                    .charges
                    .insert(key, (Arc::downgrade(&job.resident.reservation), id));
                id
            };
            (charge, dispatch)
        };
        let Some(unix_ms) = Self::timestamp() else {
            self.lost();
            return None;
        };
        let Some(deadline) = unix_ms.checked_add(
            u64::try_from(
                job.deadline
                    .saturating_duration_since(Instant::now())
                    .as_millis(),
            )
            .unwrap_or(u64::MAX),
        ) else {
            self.lost();
            return None;
        };
        let binding = Binding {
            observer_id: self.0.id,
            dispatch_identity,
            profile_manifest: job.resident.profile,
            task: job.task,
            input_digest: compiled.input_digest,
            question_utf8_sha256: Sha256Digest::from_bytes(
                Sha256::digest(job.question.as_bytes()).into(),
            ),
            original_deadline_unix_ms: deadline,
            tokens: compiled.input_tokens,
            charge_identity: format!("{}:{charge}", self.0.id),
            charge_bytes: job.resident.reservation.bytes(),
            scope: job.scope.clone(),
        };
        Some(Prepared {
            observer: self.clone(),
            binding,
            release: job.resident.reservation.release_observer(),
        })
    }
}
pub(super) struct Prepared {
    observer: Observer,
    binding: Binding,
    release: airk_kernel::service_resources::OptionalRelease,
}
impl Prepared {
    /// Called only inside the already-passed final native dispatch guard.
    pub(super) fn admitted(self) -> Witness {
        self.observer.emit(
            Observer::record(self.binding.clone(), Kind::DispatchAdmitted),
            Some(ReleaseProof {
                observer: self.observer.clone(),
                binding: self.binding.clone(),
                release: self.release,
            }),
        );
        Witness {
            observer: self.observer,
            binding: self.binding,
            returned: false,
        }
    }
}
pub(super) struct Witness {
    observer: Observer,
    binding: Binding,
    returned: bool,
}
impl Witness {
    pub(super) fn returned(&mut self, result: &Result<Scored>) {
        self.returned = true;
        let mut record = Observer::record(self.binding.clone(), Kind::ScoreReturned);
        record.return_observed = true;
        match result {
            Ok(scored) => {
                record.outcome = Some(Outcome::Success);
                record.finite_raw_scores = Some(
                    !scored.scores.is_empty()
                        && scored.scores.iter().all(|value| value.is_finite()),
                );
                if scored.scores.len() <= 64 {
                    record.raw_scores = Some(
                        scored
                            .scores
                            .iter()
                            .map(|value| value.is_finite().then_some(*value))
                            .collect(),
                    );
                    record.raw_score_bits =
                        Some(scored.scores.iter().map(|value| value.to_bits()).collect());
                } else {
                    self.observer.lost();
                    record.finite_raw_scores = Some(false);
                    record.error=Some("Native score evidence exceeded the private observation bound; full result was not observed".into());
                }
                record.work = Some(ScoreReceipt {
                    input_digest: scored.input_digest,
                    input_tokens: scored.input_tokens,
                    queue_us: scored.queue_us,
                    compile_us: scored.compile_us,
                    inference_us: scored.inference_us,
                });
            }
            Err(error) => {
                record.outcome = Some(Outcome::Error);
                record.error = Some(format!("{error:#}").chars().take(1024).collect());
                record.work = error
                    .downcast_ref::<KnownScoreFailure>()
                    .map(|failure| failure.receipt.clone());
            }
        }
        self.observer.emit(record, None);
    }
}
impl Drop for Witness {
    fn drop(&mut self) {
        let mut record = Observer::record(self.binding.clone(), Kind::JobSettled);
        record.return_observed = self.returned;
        record.proof =
            Some("actual_job_compute_delivery_and_per_job_resident_permit_dropped".into());
        self.observer.emit(record, None);
    }
}

impl Record {
    /// Decode conformance only; serialized observations never mint native custody.
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == "airk.private.native-dispatch/1"
                && self.sequence > 0
                && self.unix_ms > 0,
            "private native event schema/sequence/time refused"
        );
        ensure!(
            (1..=8192).contains(&self.binding.tokens)
                && self.binding.charge_bytes > 0
                && !self.binding.charge_identity.is_empty()
                && self.binding.charge_identity.len() <= 128
                && self.binding.dispatch_identity > 0,
            "private native event binding bounds refused"
        );
        if self.kind == Kind::ScoreReturned {
            ensure!(
                self.return_observed && self.outcome.is_some(),
                "native return observation missing"
            );
            let work = self
                .work
                .as_ref()
                .context("actual native returned work missing")?;
            ensure!(
                work.input_digest == self.binding.input_digest
                    && work.input_tokens == self.binding.tokens,
                "actual return differs from original compiled input"
            );
            if self.outcome == Some(Outcome::Success) {
                let scores = self
                    .raw_scores
                    .as_ref()
                    .context("actual returned scores missing")?;
                let bits = self
                    .raw_score_bits
                    .as_ref()
                    .context("actual score bits missing")?;
                ensure!(
                    !scores.is_empty() && scores.len() <= 64 && scores.len() == bits.len(),
                    "actual score evidence bound differs"
                );
                let finite = bits.iter().all(|bits| f32::from_bits(*bits).is_finite());
                ensure!(
                    self.finite_raw_scores == Some(finite),
                    "native score finiteness claim differs"
                );
                ensure!(
                    scores.iter().zip(bits).all(|(score, bits)| {
                        let value = f32::from_bits(*bits);
                        if value.is_finite() {
                            score.is_some_and(|score| score.to_bits() == *bits)
                        } else {
                            score.is_none()
                        }
                    }),
                    "native score evidence differs from actual bit pattern"
                );
            } else {
                ensure!(
                    self.raw_scores.is_none()
                        && self.raw_score_bits.is_none()
                        && self.error.is_some(),
                    "failed native return invented scores or omitted cause"
                );
            }
        } else {
            ensure!(
                self.outcome.is_none()
                    && self.work.is_none()
                    && self.raw_scores.is_none()
                    && self.raw_score_bits.is_none(),
                "non-return observation invented completed work"
            );
        }
        if matches!(self.kind, Kind::ProfileReleased | Kind::ChargeReleased) {
            ensure!(
                self.proof.as_deref()
                    == Some(
                        "actual_final_reservation_drop_after_native_scorer_and_source_teardown"
                    ),
                "native release proof kind differs"
            );
        }
        ensure!(
            self.error.as_ref().is_none_or(|value| value.len() <= 4096)
                && serde_json::to_vec(self)?.len() <= 65536,
            "native event byte bound exceeded"
        );
        Ok(())
    }
}
impl RecorderSummary {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == "airk.private.native-dispatch-recorder/1"
                && self.evaluation
                && !self.release_qualified,
            "private native recorder scope/bounds differ"
        );
        ensure!(
            !self.complete
                || (self.dropped == 0
                    && self.writer_failed == 0
                    && self.error.is_none()
                    && self.last_sequence == self.records as u64
                    && self.records <= 8192
                    && self.bytes <= 64 << 20),
            "incomplete observer evidence claimed complete"
        );
        Ok(())
    }
}
