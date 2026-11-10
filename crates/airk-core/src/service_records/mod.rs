// SPDX-License-Identifier: Apache-2.0
//! Shared service vocabulary: project registration, request keys, worker
//! identity, job records and maintenance observations. Leaf: it depends only on
//! `identity`. `service` re-exports every item, so `airk_core::service` paths and
//! the wire are unchanged. See `CONTRACT.md` beside this file.
use crate::identity::{Incarnation, Sha256Digest};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

/// Digest of a canonical root/storage/collection registration, never a client path.
pub type ProjectBinding = Sha256Digest;

pub const MAX_PROJECT_REGISTRATION_BYTES: usize = 20_480;

/// Canonical paths must be established by the native host; these bytes are not ownership proof.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectRegistration {
    pub binding: ProjectBinding,
    pub root: String,
    pub data_root: String,
    pub collection: String,
}

impl ProjectRegistration {
    pub fn new(root: String, data_root: String, collection: String) -> Result<Self> {
        let binding = crate::identity::framed_digest(
            b"airk-service-project/1",
            &[root.as_bytes(), data_root.as_bytes(), collection.as_bytes()],
        );
        let registration = Self {
            binding,
            root,
            data_root,
            collection,
        };
        registration.validate()?;
        Ok(registration)
    }
    pub fn validate(&self) -> Result<()> {
        for path in [&self.root, &self.data_root] {
            ensure!(
                !path.is_empty() && path.len() <= 4096 && !path.chars().any(char::is_control),
                "invalid service registration path"
            );
        }
        ensure!(
            !self.collection.is_empty()
                && self.collection.len() <= 128
                && !matches!(self.collection.as_str(), "." | "..")
                && self
                    .collection
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)),
            "invalid service collection name"
        );
        ensure!(
            self.binding
                == crate::identity::framed_digest(
                    b"airk-service-project/1",
                    &[
                        self.root.as_bytes(),
                        self.data_root.as_bytes(),
                        self.collection.as_bytes()
                    ]
                ),
            "service project binding mismatch"
        );
        ensure!(
            serde_json::to_vec(self)?.len() <= MAX_PROJECT_REGISTRATION_BYTES,
            "service registration exceeds budget"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegisteredProject {
    pub registration: ProjectRegistration,
    pub fencing_generation: u64,
    pub paused: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    Inspect,
    PlanInspect,
    Repair,
    Index,
    Reconcile,
    Vectors,
    ModelSetup,
    Configure,
    Verify,
    Rebuild,
    Recover,
    Maintain,
}

impl JobKind {
    pub fn code(self) -> &'static str {
        match self {
            Self::Inspect => "inspect",
            Self::PlanInspect => "plan_inspect",
            Self::Repair => "repair",
            Self::Index => "index",
            Self::Reconcile => "reconcile",
            Self::Vectors => "vectors",
            Self::ModelSetup => "model_setup",
            Self::Configure => "configure",
            Self::Verify => "verify",
            Self::Rebuild => "rebuild",
            Self::Recover => "recover",
            Self::Maintain => "maintain",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Pending,
    Running,
    Succeeded,
    Failed,
    Canceled,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestKey {
    pub namespace: Incarnation,
    pub epoch: u64,
    pub nonce: Incarnation,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum JobOutcome {
    Succeeded {
        revision: Option<crate::identity::RevisionToken>,
    },
    Failed {
        code: String,
    },
    Canceled,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobRecord {
    pub id: Sha256Digest,
    pub project: ProjectBinding,
    pub kind: JobKind,
    pub state: JobState,
    pub fencing_generation: u64,
    pub attempts: u32,
    pub outcome: Option<JobOutcome>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerIdentity {
    pub service: Incarnation,
    pub worker: Incarnation,
    pub project: ProjectBinding,
    pub fencing_generation: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaintenanceObservation {
    pub pending_incarnations: Option<usize>,
    #[serde(default)]
    pub pending_objects: bool,
    #[serde(default)]
    pub objects_checked: bool,
    pub removed_files: u32,
    pub failed: bool,
    pub observed_unix_ms: u64,
}

impl MaintenanceObservation {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.pending_incarnations.is_none_or(|count| count <= 8) && self.removed_files <= 4096,
            "maintenance observation exceeds contract bounds"
        );
        ensure!(
            self.failed || self.pending_incarnations.is_some(),
            "successful maintenance observation lacks pending count"
        );
        Ok(())
    }
}
