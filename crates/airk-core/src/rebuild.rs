// SPDX-License-Identifier: Apache-2.0
//! Native rebuild observations are evidence of an operation, never mutation authority.
use crate::{
    identity::{Incarnation, Sha256Digest},
    service_records::{JobRecord, ProjectBinding, ProjectRegistration},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_REBUILD_INDEXES: usize = 64;
pub const MAX_REBUILD_OBSERVATION_BYTES: usize = 512 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RebuildStage {
    Accepted,
    Preparing,
    Indexing,
    Paused,
    Ready,
    Failed,
    /// The retained operation cannot currently be observed. This is not an
    /// assertion that its native indexing job failed.
    Unavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RebuildReadiness {
    Pending,
    Paused,
    Ready,
    Failed,
    /// For example, the original native job receipt has been retired under the
    /// bounded registry policy. Never infer terminal success from its absence.
    Unavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RebuildProvenance {
    NativeRebuildFromSavedConfiguration,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RebuildFailure {
    pub code: String,
    pub message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RebuildUnavailable {
    pub operation_id: Incarnation,
    pub index_scope: ProjectBinding,
    pub reason: RebuildUnavailableReason,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RebuildUnavailableReason {
    PartialProjectScope,
}

impl RebuildFailure {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.code.is_empty()
                && self.code.len() <= 128
                && self
                    .code
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
                && !self.message.is_empty()
                && self.message.len() <= 4096,
            "invalid native rebuild failure"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RebuildIndexObservation {
    pub original: ProjectRegistration,
    pub replacement: Option<ProjectRegistration>,
    /// Original additional-root aliases, not a discovery of live source folders.
    pub sources: BTreeMap<String, String>,
    pub settings_sha256: Sha256Digest,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings_available: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub discarded_preferences: Vec<String>,
    pub original_paused: bool,
    /// Current native pause observation; before replacement exists, the saved choice.
    pub paused: bool,
    pub watching: bool,
    pub job: Option<JobRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_job: Option<JobRecord>,
    pub readiness: RebuildReadiness,
    pub failure: Option<RebuildFailure>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RebuildMode {
    CleanBreak,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RebuildCleanup {
    pub complete: bool,
    pub scope: String,
    pub state: String,
    pub failure: Option<RebuildFailure>,
    /// Complete native cleanup observation, not a projected summary.
    pub observation: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RebuildObservation {
    pub schema: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<RebuildMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub indexes_complete: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_indexes: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cleanup: Option<RebuildCleanup>,
    pub operation_id: Incarnation,
    pub stage: RebuildStage,
    pub provenance: RebuildProvenance,
    pub candidate: Sha256Digest,
    pub predecessor: Sha256Digest,
    pub original_namespace: Incarnation,
    pub preserved_state: String,
    pub preserved_control: String,
    pub preserved_index_roots: Vec<String>,
    pub successor_state: String,
    pub successor_control: String,
    pub successor_namespace: Option<Incarnation>,
    /// Absent for a rebuild explicitly selected before attempting migration.
    pub original_failure: Option<RebuildFailure>,
    pub failure: Option<RebuildFailure>,
    pub indexes: Vec<RebuildIndexObservation>,
    /// An exact selected index is an explicitly disclosed subset of the operation.
    pub index_scope: Option<ProjectBinding>,
}

fn location(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty() && value.len() <= 4096 && !value.chars().any(char::is_control),
        "invalid native rebuild location"
    );
    Ok(())
}

impl RebuildObservation {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == 1
                && (!self.indexes.is_empty() || self.mode == Some(RebuildMode::CleanBreak))
                && self.indexes.len() <= MAX_REBUILD_INDEXES,
            "invalid native rebuild observation"
        );
        ensure!(
            self.mode.is_some() == self.indexes_complete.is_some()
                && self.mode.is_some() == self.expected_indexes.is_some(),
            "clean-break roster qualification missing"
        );
        if let Some(expected) = self.expected_indexes {
            ensure!(
                expected <= MAX_REBUILD_INDEXES,
                "clean-break complete roster bound exceeded"
            );
            ensure!(
                self.indexes_complete != Some(true)
                    || self.index_scope.is_some()
                    || self.indexes.len() == expected,
                "clean-break complete roster count changed"
            );
            ensure!(
                self.indexes_complete != Some(false)
                    || self.indexes.is_empty() && self.stage != RebuildStage::Ready,
                "incomplete clean-break census is not an empty completed rebuild"
            );
        }
        ensure!(
            self.mode.is_some() == self.cleanup.is_some(),
            "clean-break cleanup observation missing or unqualified"
        );
        if let Some(cleanup) = &self.cleanup {
            ensure!(
                matches!(
                    cleanup.scope.as_str(),
                    "administrative" | "selected_index_summary"
                ) && (cleanup.scope == "administrative" || cleanup.observation.is_none()),
                "cleanup observation crossed administrative scope"
            );
            ensure!(
                matches!(
                    cleanup.state.as_str(),
                    "waiting_for_indexes" | "removing" | "blocked" | "complete"
                ) && cleanup.complete == (cleanup.state == "complete"),
                "invalid clean-break cleanup state"
            );
            if let Some(failure) = &cleanup.failure {
                failure.validate()?;
            }
            ensure!(
                cleanup.state != "blocked" || cleanup.failure.is_some(),
                "blocked cleanup has no diagnostic"
            );
            ensure!(
                !cleanup.complete || cleanup.failure.is_none(),
                "completed cleanup has a failure"
            );
            ensure!(
                !cleanup.complete || self.stage == RebuildStage::Ready,
                "cleanup cannot complete before fresh indexes"
            );
        }
        if self.mode == Some(RebuildMode::CleanBreak) && self.stage == RebuildStage::Ready {
            ensure!(
                self.successor_namespace
                    .is_some_and(|namespace| namespace != self.original_namespace),
                "clean-break readiness requires fresh Genesis"
            );
        }
        for value in [
            &self.preserved_state,
            &self.preserved_control,
            &self.successor_state,
            &self.successor_control,
        ] {
            location(value)?;
        }
        ensure!(
            self.preserved_state != self.successor_state
                && self.preserved_control != self.successor_control,
            "native rebuild reused preserved state"
        );
        let mut roots = BTreeSet::new();
        for root in &self.preserved_index_roots {
            location(root)?;
            ensure!(
                roots.insert(root.as_str()),
                "duplicate preserved rebuild root"
            );
        }
        let mut originals = BTreeSet::new();
        let mut replacements = BTreeSet::new();
        for index in &self.indexes {
            index.original.validate()?;
            ensure!(
                originals.insert(index.original.binding)
                    && roots.contains(index.original.data_root.as_str())
                    && index.sources.len() <= 16,
                "invalid saved rebuild index roster"
            );
            for (alias, source) in &index.sources {
                ensure!(
                    !alias.is_empty()
                        && alias.len() <= 32
                        && alias.bytes().all(|byte| byte.is_ascii_lowercase()
                            || byte.is_ascii_digit()
                            || b"_-".contains(&byte))
                        && alias != "primary",
                    "invalid saved rebuild source alias"
                );
                location(source)?;
            }
            if let Some(replacement) = &index.replacement {
                replacement.validate()?;
                ensure!(
                    replacement.root == index.original.root
                        && replacement.collection == index.original.collection
                        && replacement.data_root != index.original.data_root
                        && replacement.binding != index.original.binding
                        && replacements.insert(replacement.binding),
                    "native rebuild changed saved source or collection identity"
                );
            }
            if let Some(initial) = &index.initial_job {
                ensure!(
                    index
                        .replacement
                        .as_ref()
                        .is_some_and(|replacement| replacement.binding == initial.project)
                        && matches!(
                            initial.state,
                            crate::service_records::JobState::Failed
                                | crate::service_records::JobState::Canceled
                        ),
                    "invalid original clean-break retry receipt"
                );
            }
            if let Some(job) = &index.job {
                ensure!(
                    index
                        .replacement
                        .as_ref()
                        .is_some_and(|replacement| job.project == replacement.binding),
                    "native rebuild job belongs to another index"
                );
            }
            ensure!(
                index.readiness != RebuildReadiness::Ready
                    || index
                        .job
                        .as_ref()
                        .is_some_and(|job| job.state == crate::service_records::JobState::Succeeded),
                "native rebuild readiness has no successful native job"
            );
            if let Some(failure) = &index.failure {
                failure.validate()?;
            }
            ensure!(
                !matches!(
                    index.readiness,
                    RebuildReadiness::Failed | RebuildReadiness::Unavailable
                ) || index.failure.is_some(),
                "native rebuild index failure or unavailability has no retained reason"
            );
            ensure!(
                index.readiness != RebuildReadiness::Paused || index.paused,
                "native rebuild invents a saved pause"
            );
        }
        if let Some(binding) = self.index_scope {
            ensure!(
                self.indexes.len() == 1
                    && self
                        .indexes
                        .iter()
                        .any(|index| index.original.binding == binding
                            || index
                                .replacement
                                .as_ref()
                                .is_some_and(|value| value.binding == binding)),
                "native rebuild observation crossed selected index"
            );
        }
        for failure in [&self.original_failure, &self.failure]
            .into_iter()
            .flatten()
        {
            failure.validate()?;
        }
        ensure!(
            self.stage != RebuildStage::Ready
                || self
                    .indexes
                    .iter()
                    .all(|index| index.readiness == RebuildReadiness::Ready),
            "native rebuild ready stage has unfinished indexes"
        );
        ensure!(
            self.stage != RebuildStage::Failed
                || self.failure.is_some()
                || self
                    .indexes
                    .iter()
                    .any(|index| index.readiness == RebuildReadiness::Failed
                        && index.failure.is_some()),
            "native rebuild failure has no retained reason"
        );
        ensure!(
            self.stage != RebuildStage::Unavailable
                || self.failure.is_some()
                || self
                    .indexes
                    .iter()
                    .any(|index| index.readiness == RebuildReadiness::Unavailable),
            "native rebuild unavailability has no retained reason"
        );
        ensure!(
            serde_json::to_vec(self)?.len() <= MAX_REBUILD_OBSERVATION_BYTES,
            "native rebuild observation exceeds byte bound"
        );
        Ok(())
    }

    pub fn scoped_for(&self, binding: ProjectBinding) -> Result<Option<Self>> {
        self.validate()?;
        if self.index_scope.is_some_and(|scope| scope != binding) {
            return Ok(None);
        }
        let indexes = self
            .indexes
            .iter()
            .filter(|index| {
                index.original.binding == binding
                    || index
                        .replacement
                        .as_ref()
                        .is_some_and(|value| value.binding == binding)
            })
            .cloned()
            .collect::<Vec<_>>();
        if indexes.is_empty() {
            return Ok(None);
        }
        let mut selected = self.clone();
        selected.preserved_index_roots = indexes
            .iter()
            .map(|index| index.original.data_root.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        selected.indexes = indexes;
        selected.index_scope = Some(binding);
        if selected.mode == Some(RebuildMode::CleanBreak) {
            selected.expected_indexes = Some(selected.indexes.len());
            selected.indexes_complete = Some(true);
        }
        if let Some(cleanup) = &mut selected.cleanup {
            cleanup.scope = "selected_index_summary".into();
            cleanup.observation = None;
            if let Some(failure) = &mut cleanup.failure {
                failure.message = "Administrative cleanup details are unavailable in this selected-index observation; inspect Management Service complete runtime diagnostics.".into();
            }
        }
        // The operation-wide failure can mention another index's source or
        // storage. Keep its fixed code while explicitly disclosing that full
        // details belong to the installation's administrative observation.
        for failure in [&mut selected.original_failure, &mut selected.failure]
            .into_iter()
            .flatten()
        {
            failure.message = "Operation-wide failure details are unavailable in this selected-index observation; inspect the installation's administrative rebuild report.".into();
        }
        // Operation-wide owner failures affect every index; sibling job states
        // do not. Recompute the disclosed subset without copying a sibling's
        // terminal reason or implying it makes this selected index ready.
        selected.stage = if selected.failure.is_some() {
            if self.stage == RebuildStage::Unavailable {
                RebuildStage::Unavailable
            } else {
                RebuildStage::Failed
            }
        } else {
            match selected.indexes[0].readiness {
                RebuildReadiness::Ready => RebuildStage::Ready,
                RebuildReadiness::Paused => RebuildStage::Paused,
                RebuildReadiness::Failed => RebuildStage::Failed,
                RebuildReadiness::Unavailable => RebuildStage::Unavailable,
                RebuildReadiness::Pending => match self.stage {
                    RebuildStage::Accepted | RebuildStage::Preparing => self.stage,
                    _ => RebuildStage::Indexing,
                },
            }
        };
        selected.validate()?;
        Ok(Some(selected))
    }
}

#[cfg(test)]
mod tests;
