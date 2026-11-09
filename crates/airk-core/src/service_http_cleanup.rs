// SPDX-License-Identifier: Apache-2.0
//! Historical configuration disposal proof. Never a credential or live grant.
use super::service_http::EndpointDescriptor;
use crate::identity::{Incarnation, Sha256Digest};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, path::Path, str::FromStr};

pub const MAX_HTTP_CLEANUP_OWNERS: usize = 128;
pub const MAX_HTTP_CLEANUP_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HttpCleanupOwnership {
    Active,
    Expired,
    Revoked,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HttpCleanupOwner {
    pub connection_id: Incarnation,
    pub ownership: HttpCleanupOwnership,
    /// SHA-256 of the entire original UTF-8 `airk1.<id>.<secret>` token.
    /// Retired ownership-only markers need not retain this digest.
    pub credential_sha256: Option<Sha256Digest>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HttpCleanupFailure {
    pub code: String,
    pub message: String,
}

/// Complete original private metadata, retained before its namespace is removed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PredecessorHttpSnapshot {
    pub schema: u32,
    pub purpose: String,
    pub predecessor: String,
    pub control_dir: String,
    pub namespace: Option<Incarnation>,
    pub endpoint: Option<EndpointDescriptor>,
    pub ca_certificate_file_sha256: Option<Sha256Digest>,
    pub complete: bool,
    pub owners: Vec<HttpCleanupOwner>,
    pub failure: Option<HttpCleanupFailure>,
}

impl PredecessorHttpSnapshot {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == 1 && self.purpose == "predecessor_cleanup",
            "invalid HTTP cleanup purpose"
        );
        Sha256Digest::from_str(&self.predecessor)?;
        ensure!(
            Path::new(&self.control_dir).is_absolute()
                && self.control_dir.len() <= 4096
                && !self.control_dir.chars().any(char::is_control),
            "invalid HTTP cleanup control"
        );
        ensure!(
            self.owners.len() <= MAX_HTTP_CLEANUP_OWNERS,
            "HTTP cleanup inventory exceeds bound"
        );
        let mut ids = BTreeSet::new();
        ensure!(
            self.owners
                .iter()
                .all(|owner| ids.insert(owner.connection_id)),
            "duplicate HTTP cleanup owner"
        );
        if let Some(endpoint) = &self.endpoint {
            endpoint.validate()?;
            ensure!(
                Some(endpoint.namespace) == self.namespace,
                "HTTP cleanup endpoint namespace changed"
            );
        }
        ensure!(
            self.complete == self.failure.is_none()
                && (!self.complete || self.namespace.is_some() && self.endpoint.is_some())
                && (self.complete || self.owners.is_empty()),
            "incomplete HTTP cleanup inventory cannot imply absence or a selected subset"
        );
        if let Some(failure) = &self.failure {
            ensure!(
                !failure.code.is_empty()
                    && failure.code.len() <= 128
                    && failure.message.len() <= 2048,
                "HTTP cleanup failure exceeds bound"
            );
        }
        crate::index_catalog::encoded_size(self, MAX_HTTP_CLEANUP_BYTES)?;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HttpCleanupQualification {
    pub live_authority: bool,
    pub source: String,
    pub snapshot_complete: bool,
    pub credential_digest_algorithm: String,
    pub current_namespace: Option<Incarnation>,
    pub current_namespace_ownership: String,
    pub current_namespace_failure: Option<HttpCleanupFailure>,
}

/// Administrative setup read. The full original snapshot remains in the DTO;
/// query fields select a single ID without replacing the complete inventory.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PredecessorHttpLookup {
    pub schema: u32,
    pub purpose: String,
    pub active: String,
    pub predecessor: String,
    pub operation_id: Option<Incarnation>,
    pub control_dir: String,
    pub namespace: Option<Incarnation>,
    pub connection_id: Incarnation,
    pub status: String,
    pub ownership: Option<HttpCleanupOwnership>,
    pub endpoint: Option<EndpointDescriptor>,
    pub ca_certificate_file: Option<String>,
    pub credential_file: Option<String>,
    pub credential_sha256: Option<Sha256Digest>,
    pub ca_certificate_file_sha256: Option<Sha256Digest>,
    pub qualification: HttpCleanupQualification,
    pub snapshot: PredecessorHttpSnapshot,
}

impl PredecessorHttpLookup {
    pub fn validate(&self) -> Result<()> {
        self.snapshot.validate()?;
        Sha256Digest::from_str(&self.active)?;
        ensure!(
            self.schema == 1
                && self.purpose == "predecessor_cleanup"
                && self.predecessor == self.snapshot.predecessor
                && self.control_dir == self.snapshot.control_dir
                && self.namespace == self.snapshot.namespace
                && self.endpoint == self.snapshot.endpoint
                && self.ca_certificate_file_sha256 == self.snapshot.ca_certificate_file_sha256
                && !self.qualification.live_authority
                && self.qualification.snapshot_complete == self.snapshot.complete
                && self.qualification.credential_digest_algorithm == "sha256_full_airk1_token",
            "HTTP cleanup lookup crossed original snapshot"
        );
        ensure!(
            match self.qualification.source.as_str() {
                "saved_predecessor_snapshot" => self.operation_id.is_some(),
                "current_predecessor_metadata" =>
                    self.operation_id.is_none() && self.active == self.predecessor,
                _ => false,
            },
            "invalid HTTP cleanup snapshot lineage"
        );
        let owner = self
            .snapshot
            .owners
            .iter()
            .find(|owner| owner.connection_id == self.connection_id);
        let current_clear = match self.qualification.current_namespace_ownership.as_str() {
            "same_predecessor" => {
                ensure!(
                    self.qualification.current_namespace == self.namespace
                        && self.qualification.current_namespace_failure.is_none(),
                    "HTTP cleanup current namespace differs from predecessor"
                );
                true
            }
            "not_owned" => {
                ensure!(
                    self.qualification.current_namespace.is_some()
                        && self.qualification.current_namespace != self.namespace
                        && self.qualification.current_namespace_failure.is_none(),
                    "HTTP cleanup current absence lacks namespace observation"
                );
                true
            }
            "owned" => {
                ensure!(
                    self.qualification.current_namespace.is_some()
                        && self.qualification.current_namespace != self.namespace
                        && self.qualification.current_namespace_failure.is_none(),
                    "HTTP cleanup newer ownership lacks namespace observation"
                );
                false
            }
            "unavailable" => {
                ensure!(
                    self.qualification.current_namespace_failure.is_some(),
                    "HTTP cleanup unknown current namespace lacks failure details"
                );
                false
            }
            _ => anyhow::bail!("invalid HTTP cleanup current namespace ownership"),
        };
        if let Some(failure) = &self.qualification.current_namespace_failure {
            ensure!(
                !failure.code.is_empty()
                    && failure.code.len() <= 128
                    && failure.message.len() <= 2048,
                "HTTP cleanup current failure exceeds bound"
            );
        }
        let private = Path::new(&self.control_dir).join("http-1");
        match self.status.as_str() {
            "owned" => {
                let owner = owner
                    .ok_or_else(|| anyhow::anyhow!("HTTP cleanup ID lacks original ownership"))?;
                ensure!(
                    current_clear
                        && self.snapshot.complete
                        && self.ownership == Some(owner.ownership)
                        && self.credential_sha256 == owner.credential_sha256
                        && self.ca_certificate_file.as_deref().map(Path::new)
                            == Some(private.join("ca.pem").as_path())
                        && self.credential_file.as_deref().map(Path::new)
                            == Some(
                                private
                                    .join(format!("credential-{}.token", self.connection_id))
                                    .as_path()
                            ),
                    "HTTP cleanup owned proof changed"
                );
            }
            "not_owned" | "unavailable" => ensure!(
                self.ownership.is_none()
                    && self.credential_sha256.is_none()
                    && self.ca_certificate_file.is_none()
                    && self.credential_file.is_none()
                    && if self.status == "not_owned" {
                        current_clear && self.snapshot.complete && owner.is_none()
                    } else {
                        !self.snapshot.complete || !current_clear
                    },
                "unobserved HTTP cleanup ownership cannot imply absence"
            ),
            _ => anyhow::bail!("invalid HTTP cleanup ownership status"),
        }
        crate::index_catalog::encoded_size(self, 128 * 1024)?;
        Ok(())
    }
}
