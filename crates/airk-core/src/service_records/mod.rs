// SPDX-License-Identifier: Apache-2.0
//! Shared service vocabulary: project registration, request keys, worker
//! identity, connection endpoints and maintenance observations. It depends
//! only on `identity`. The service modules re-export the public types.
//! See `CONTRACT.md` beside this file.
use crate::identity::{Incarnation, Sha256Digest};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

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

/// The largest M3 request frame (contracts §2.8): arguments, selection and
/// envelope with room for re-encoding.
pub const MAX_TOOLS_CALL_REQUEST_BYTES: usize = 128 * 1024;

/// Exactly one canonical IPv4 loopback MCP URL. No DNS, redirects or userinfo.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct LoopbackMcpUrl(u16);

impl LoopbackMcpUrl {
    pub fn new(port: u16) -> Result<Self> {
        ensure!(
            port != 0,
            "an endpoint needs an actually bound nonzero port"
        );
        Ok(Self(port))
    }

    pub fn port(self) -> u16 {
        self.0
    }

    pub fn origin(self) -> String {
        format!("https://127.0.0.1:{}", self.0)
    }
}

impl fmt::Display for LoopbackMcpUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "https://127.0.0.1:{}/mcp", self.0)
    }
}

impl FromStr for LoopbackMcpUrl {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        Self::new(canonical_port(value, "https://127.0.0.1:")?)
    }
}

/// The port of `<prefix><port>/mcp` under the canonical rules: bounded, no
/// leading zero, digits only, nonzero.
fn canonical_port(value: &str, prefix: &str) -> Result<u16> {
    ensure!(value.len() <= 32, "loopback URL exceeds its bound");
    let port = value
        .strip_prefix(prefix)
        .and_then(|value| value.strip_suffix("/mcp"))
        .ok_or_else(|| anyhow::anyhow!("expected a canonical loopback MCP URL"))?;
    ensure!(
        !port.is_empty()
            && !port.starts_with('0')
            && port.bytes().all(|byte| byte.is_ascii_digit()),
        "noncanonical loopback port"
    );
    let port: u16 = port.parse()?;
    ensure!(
        port != 0,
        "an endpoint needs an actually bound nonzero port"
    );
    Ok(port)
}

impl LoopbackMcpUrl {
    /// Local trust plan C: the port of an `https://127.0.0.1:<port>/mcp` URL
    /// that a build serving loopback over HTTPS wrote (its endpoint record,
    /// agent configs). The same canonical rules; such a URL is never a
    /// current endpoint, only a record to migrate.
    pub fn legacy_https_port(value: &str) -> Option<u16> {
        canonical_port(value, "https://127.0.0.1:").ok()
    }
}

impl TryFrom<String> for LoopbackMcpUrl {
    type Error = anyhow::Error;

    fn try_from(value: String) -> Result<Self> {
        value.parse()
    }
}

impl From<LoopbackMcpUrl> for String {
    fn from(value: LoopbackMcpUrl) -> Self {
        value.to_string()
    }
}
