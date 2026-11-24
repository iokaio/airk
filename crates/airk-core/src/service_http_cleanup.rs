// SPDX-License-Identifier: Apache-2.0
//! Historical configuration disposal proof. Never a credential or live grant.
//!
//! Since local trust plan B (23 November 2026) no installation keeps a
//! credential ledger: the service deletes the retired ledger and its token
//! files on upgrade. A predecessor's agent-config entries are therefore
//! recognised by their known shapes (the `Bearer`, `headersHelper`,
//! `http_headers_helper` and `bearer_token_env_var` forms, which name the
//! connection id), not by a ledger record. This proof identifies the
//! predecessor installation, its endpoint and whether a newer installation
//! could own that endpoint, so a cleanup removes only an AIRK-shaped entry at
//! the predecessor's exact endpoint URL and never one it does not recognise.
use super::service_http::EndpointDescriptor;
use crate::identity::{Incarnation, Sha256Digest};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{path::Path, str::FromStr};

pub const MAX_HTTP_CLEANUP_BYTES: usize = 64 * 1024;

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
        if let Some(endpoint) = &self.endpoint {
            endpoint.validate()?;
            ensure!(
                Some(endpoint.namespace) == self.namespace,
                "HTTP cleanup endpoint namespace changed"
            );
        }
        ensure!(
            self.complete == self.failure.is_none()
                && (!self.complete || self.namespace.is_some() && self.endpoint.is_some()),
            "incomplete HTTP cleanup metadata cannot identify the predecessor"
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
    pub current_namespace: Option<Incarnation>,
    /// `same_predecessor`, `not_owned` (a newer installation, which issues no
    /// connection) or `unavailable`.
    pub current_namespace_ownership: String,
    pub current_namespace_failure: Option<HttpCleanupFailure>,
}

/// Administrative setup read for one connection id taken from an AIRK-shaped
/// agent-config entry. `owned` means the predecessor is identified and no
/// newer installation can claim its endpoint, so such an entry whose URL is
/// `endpoint.url` is the predecessor's; `unavailable` means it is not known.
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
    pub endpoint: Option<EndpointDescriptor>,
    pub ca_certificate_file: Option<String>,
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
                && self.qualification.snapshot_complete == self.snapshot.complete,
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
            "owned" => ensure!(
                current_clear
                    && self.snapshot.complete
                    && self.ca_certificate_file.as_deref().map(Path::new)
                        == Some(private.join("ca.pem").as_path()),
                "HTTP cleanup owned proof changed"
            ),
            "unavailable" => ensure!(
                self.ca_certificate_file.is_none() && (!self.snapshot.complete || !current_clear),
                "unobserved HTTP cleanup ownership cannot imply ownership"
            ),
            _ => anyhow::bail!("invalid HTTP cleanup ownership status"),
        }
        crate::index_catalog::encoded_size(self, 128 * 1024)?;
        Ok(())
    }
}
