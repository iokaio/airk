// SPDX-License-Identifier: Apache-2.0
//! Committed discovery coverage for progressive current-index scans.
use airk_core::{
    identity::Incarnation,
    readiness::{DiscoveryReadiness, DiscoveryState},
};
use airk_store::{
    FileStore,
    current::{CurrentRead, resources::ResourceChange},
    quota::Reservation,
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

const KEY: &str = "scan-work";

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ScanWork {
    version: u32,
    epoch: Incarnation,
    processed_artifacts: u64,
    committed_artifacts: u64,
    total_artifacts: Option<u64>,
    failure: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    paused: bool,
}

fn is_false(value: &bool) -> bool {
    !value
}

impl ScanWork {
    pub fn complete(&self) -> bool {
        self.total_artifacts.is_some()
    }
    pub fn start() -> Result<Self> {
        let mut bytes = [0; 16];
        getrandom::fill(&mut bytes).map_err(|e| anyhow::anyhow!("allocate scan identity: {e}"))?;
        Ok(Self {
            version: 1,
            epoch: Incarnation::from_bytes(bytes),
            processed_artifacts: 0,
            committed_artifacts: 0,
            total_artifacts: None,
            failure: None,
            paused: false,
        })
    }

    pub fn advanced(&mut self, processed: u64, committed: u64) -> Result<()> {
        ensure!(
            self.total_artifacts.is_none()
                && !self.paused
                && self.failure.is_none()
                && processed >= self.processed_artifacts
                && committed >= self.committed_artifacts
                && committed <= processed,
            "invalid progressive scan advance"
        );
        self.processed_artifacts = processed;
        self.committed_artifacts = committed;
        Ok(())
    }

    pub fn finish(&mut self, total: u64) -> Result<()> {
        ensure!(
            self.failure.is_none() && !self.paused && total >= self.processed_artifacts,
            "invalid progressive scan total"
        );
        self.total_artifacts = Some(total);
        self.processed_artifacts = total;
        self.committed_artifacts = total;
        Ok(())
    }

    /// Reflect explicit additions/removals in an already complete inventory.
    /// This retains its epoch and does not establish a new full source scan.
    pub fn adjust_complete_inventory(&mut self, total: u64) -> Result<()> {
        ensure!(
            self.complete() && self.failure.is_none() && !self.paused,
            "path update requires a complete inventory"
        );
        self.total_artifacts = Some(total);
        self.processed_artifacts = total;
        self.committed_artifacts = total;
        Ok(())
    }

    pub fn fail(&mut self, error: &anyhow::Error) {
        if error.is::<crate::scheduler::ScanPaused>() {
            self.paused = true;
        } else {
            self.failure = Some(format!("{error:#}").chars().take(1024).collect());
        }
    }

    fn validate(&self) -> Result<()> {
        ensure!(
            self.version == 1
                && self.committed_artifacts <= self.processed_artifacts
                && self
                    .total_artifacts
                    .is_none_or(|n| n == self.processed_artifacts)
                && self.failure.as_ref().is_none_or(|s| s.len() <= 4096)
                && !(self.failure.is_some() && self.total_artifacts.is_some()),
            "invalid committed scan coverage"
        );
        Ok(())
    }

    pub fn resource_change(
        &self,
        store: &FileStore,
        reservation: &Reservation,
    ) -> Result<ResourceChange> {
        self.validate()?;
        let bytes = serde_json::to_vec(self)?;
        let reference = store.retain_current_resource(&bytes, reservation)?;
        Ok(ResourceChange {
            key: KEY.into(),
            reference: Some(reference),
        })
    }

    fn readiness(&self) -> DiscoveryReadiness {
        DiscoveryReadiness {
            version: 1,
            state: if self.paused {
                DiscoveryState::Paused
            } else if self.failure.is_some() {
                DiscoveryState::Failed
            } else if self.total_artifacts.is_some() {
                DiscoveryState::Ready
            } else if self.committed_artifacts > 0 {
                DiscoveryState::Partial
            } else {
                DiscoveryState::Pending
            },
            epoch: Some(self.epoch),
            processed_artifacts: self.processed_artifacts,
            committed_artifacts: self.committed_artifacts,
            total_artifacts: self.total_artifacts,
            failure: self.failure.clone(),
        }
    }
}

pub(crate) fn readiness(read: &CurrentRead, store: &FileStore) -> Result<DiscoveryReadiness> {
    if let Some(work) = read_work(read, store)? {
        return Ok(work.readiness());
    }
    let total = read
        .collection_state()?
        .map_or(0, |state| state.counts.files);
    Ok(DiscoveryReadiness {
        version: 1,
        state: DiscoveryState::Ready,
        epoch: None,
        processed_artifacts: total,
        committed_artifacts: total,
        total_artifacts: Some(total),
        failure: None,
    })
}

pub(super) fn read_work(read: &CurrentRead, store: &FileStore) -> Result<Option<ScanWork>> {
    read.resource(store, KEY)?
        .map(|bytes| {
            let work: ScanWork = serde_json::from_slice(&bytes)?;
            work.validate()?;
            ensure!(
                serde_json::to_vec(&work)? == bytes,
                "noncanonical scan work"
            );
            Ok(work)
        })
        .transpose()
}
