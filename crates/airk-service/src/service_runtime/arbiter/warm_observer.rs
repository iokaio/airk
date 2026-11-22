// SPDX-License-Identifier: Apache-2.0
//! Private fixture wakeups, never native readiness authority or a public event.
use super::*;
use airk_core::identity::Sha256Digest;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use tokio::sync::mpsc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::service_runtime) enum Outcome {
    Ready,
    Failed,
    Superseded,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::service_runtime) enum ActivationPath {
    NotAdmitted,
    RetainedResident,
    NativeWarmAdmitted,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::service_runtime) struct Record {
    pub schema: String,
    pub observer_id: airk_core::identity::Incarnation,
    pub sequence: u64,
    pub observed_unix_ms: u64,
    pub stage: Stage,
    pub profile_manifest: Sha256Digest,
    pub expected_configuration: Configuration,
    pub activation_path: ActivationPath,
    pub lifecycle_serial: u64,
    pub current_lifecycle_serial: u64,
    pub accepted_configure_revision: u64,
    pub captured_configure_revision: u64,
    pub phase: Availability,
    pub active_profile: Option<Sha256Digest>,
    pub native_configuration_revision: Option<u64>,
    pub canceled: bool,
    pub operation_succeeded: bool,
    pub outcome: Outcome,
    pub error: Option<String>,
    /// The different resident released before this warm reserved (device switch).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub released_prior: Option<Sha256Digest>,
    /// Time from the start of that release to its observed drain.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_ms: Option<u64>,
}
impl Record {
    pub fn classify(&self) -> Outcome {
        if self.canceled
            || self.current_lifecycle_serial != self.lifecycle_serial
            || self.accepted_configure_revision != self.captured_configure_revision
        {
            Outcome::Superseded
        } else if self.activation_path != ActivationPath::NotAdmitted
            && self.operation_succeeded
            && self.phase == Availability::Ready
            && self.expected_configuration.stage(self.stage).enabled
            && self
                .expected_configuration
                .stage(self.stage)
                .selected_profile
                == Some(self.profile_manifest)
            && self.active_profile == Some(self.profile_manifest)
            && self.native_configuration_revision == Some(self.expected_configuration.revision)
        {
            Outcome::Ready
        } else {
            Outcome::Failed
        }
    }
}
struct Inner {
    id: airk_core::identity::Incarnation,
    // No filesystem I/O or native ownership lock is held while taking this
    // short observer-only lock. Queue admission is always try_send, never await.
    sender: Mutex<(u64, mpsc::Sender<Record>)>,
    dropped: AtomicU64,
    active: AtomicUsize,
    sealed: AtomicBool,
    idle: tokio::sync::Notify,
}
#[derive(Clone)]
pub(in crate::service_runtime) struct Observer(Arc<Inner>);
impl Observer {
    pub fn channel() -> Result<(Self, mpsc::Receiver<Record>)> {
        let (sender, receiver) = mpsc::channel(128);
        Ok((
            Self(Arc::new(Inner {
                id: airk_kernel::service_broker::new_instance_id()?,
                sender: Mutex::new((0, sender)),
                dropped: AtomicU64::new(0),
                active: AtomicUsize::new(0),
                sealed: AtomicBool::new(false),
                idle: tokio::sync::Notify::new(),
            })),
            receiver,
        ))
    }
    pub fn id(&self) -> airk_core::identity::Incarnation {
        self.0.id
    }
    pub fn dropped(&self) -> u64 {
        self.0.dropped.load(Ordering::Acquire)
    }
    pub fn lost(&self) {
        self.0.dropped.fetch_add(1, Ordering::Relaxed);
    }
    pub(super) fn begin(&self) -> Option<Guard> {
        if self.0.sealed.load(Ordering::Acquire) {
            return None;
        }
        self.0.active.fetch_add(1, Ordering::AcqRel);
        if self.0.sealed.load(Ordering::Acquire) {
            self.0.active.fetch_sub(1, Ordering::AcqRel);
            self.0.idle.notify_waiters();
            return None;
        }
        Some(Guard {
            observer: self.clone(),
            completed: false,
        })
    }
    pub async fn seal_and_wait(&self) {
        self.0.sealed.store(true, Ordering::Release);
        loop {
            let changed = self.0.idle.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if self.0.active.load(Ordering::Acquire) == 0 {
                return;
            }
            changed.await;
        }
    }
    pub fn emit(&self, mut record: Record) {
        let Ok(mut state) = self.0.sender.try_lock() else {
            self.lost();
            return;
        };
        let Some(sequence) = state.0.checked_add(1) else {
            self.lost();
            return;
        };
        record.sequence = sequence;
        record.observer_id = self.id();
        record.outcome = record.classify();
        state.0 = sequence;
        if state.1.try_send(record).is_err() {
            self.lost();
        }
    }
}
pub(super) struct Guard {
    observer: Observer,
    completed: bool,
}
impl Guard {
    pub fn complete(&mut self) {
        self.completed = true;
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        if !self.completed {
            self.observer.lost();
        }
        self.observer.0.active.fetch_sub(1, Ordering::AcqRel);
        self.observer.0.idle.notify_waiters();
    }
}

#[cfg(test)]
mod tests;
