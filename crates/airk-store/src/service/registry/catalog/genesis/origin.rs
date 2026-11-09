// SPDX-License-Identifier: Apache-2.0
//! Immutable identification for a native managed first-pin operation.
//! These bytes identify an operation; they are never a process/containment proof.
use airk_core::identity::{Incarnation, Sha256Digest, framed_digest};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedGenesisOrigin {
    pub schema: u32,
    pub install: PathBuf,
    pub state: PathBuf,
    pub control: PathBuf,
    pub operation: Incarnation,
    pub namespace: Incarnation,
    /// Domain-framed compact native policy identity, not a raw policy-file SHA.
    pub policy_sha256: Sha256Digest,
    pub policy_version: String,
    pub policy_verification_basis: String,
    pub policy_lifecycle: String,
    pub source: PathBuf,
    pub runtime_sha256: Sha256Digest,
    pub main_sha256: Sha256Digest,
    /// Schema 1 binds the historical separate helper. Schema 2 binds the
    /// service-owned provider through the main executable and runtime seal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub helper_sha256: Option<Sha256Digest>,
    pub installer_identity: Sha256Digest,
}

/// Pure encoded-size preflight, before a managed layout reserves an operation.
/// The throwaway template deliberately uses maximum native field widths; it is
/// never validated as evidence, persisted, returned, or used to authorize work.
pub fn preflight_managed_origin(origin: &ManagedGenesisOrigin) -> Result<()> {
    use super::{GenesisReceipt, GenesisStage, activation};
    origin.validate()?;
    let digest = Sha256Digest::from_bytes([0; 32]);
    let candidate = activation::AuthorityV1CandidateObservation {
        schema: 1,
        nonce: origin.operation,
        installer_identity: origin.installer_identity,
        catalog: super::super::CatalogState {
            schema: 12,
            namespace: origin.namespace,
            revision: airk_core::index_catalog::CatalogRevision::INITIAL.next()?,
            activated: false,
            accepted_new_writes: false,
            authority_contract: 1,
            policy_contract: 0,
            upgrade_id: origin.operation,
            legacy_schema_digest: digest,
            backup_digest: digest,
        },
        layout_digest: digest,
        basis: activation::AuthorityV1Basis::FreshGenesis {
            operation: origin.operation,
            legacy_data_digest: digest,
            coordinator_origin_digest: Some(origin.digest()?),
        },
    };
    let template = GenesisReceipt {
        schema: 1,
        root: origin.control.clone(),
        namespace: origin.namespace,
        operation: origin.operation,
        installer_identity: origin.installer_identity,
        legacy_schema_digest: digest,
        legacy_data_digest: digest,
        coordinator_origin: Some(origin.clone()),
        stage: GenesisStage::Activating,
        backups: (0..super::backup::MAX_ATTEMPTS)
            .map(|slot| super::backup::BackupAttempt {
                slot: slot as u8,
                file_identity: Some(super::backup::FileIdentity {
                    platform: "windows".into(),
                    volume: u64::MAX,
                    file: u64::MAX,
                }),
                bytes: Some(u64::MAX),
                sha256: Some(digest),
                complete: false,
                abandoned: false,
            })
            .collect(),
        authority_v1: Some(activation::AuthorityV1ActivationRecord {
            readiness: activation::AuthorityV1ReadinessEvidence {
                candidate,
                required_predecessors: Vec::new(),
                predecessor_refusals: Vec::new(),
            },
            phase: activation::AuthorityV1Phase::Activating,
        }),
    };
    airk_core::index_catalog::encoded_size(&template, super::MAX_RECEIPT_BYTES)?;
    Ok(())
}

impl ManagedGenesisOrigin {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            matches!((self.schema, self.helper_sha256), (1, Some(_)) | (2, None)),
            "unsupported managed genesis executable contract"
        );
        ensure!(
            !self.policy_version.is_empty()
                && self.policy_version.len() <= 64
                && !self.policy_version.chars().any(char::is_control)
                && matches!(
                    self.policy_verification_basis.as_str(),
                    "trusted_distribution" | "organization_digest"
                )
                && matches!(
                    self.policy_lifecycle.as_str(),
                    "user_on_demand" | "external"
                ),
            "invalid managed saved approval"
        );
        ensure!(
            self.operation == self.namespace,
            "managed initial namespace reservation changed"
        );
        for path in [&self.install, &self.state, &self.control, &self.source] {
            ensure!(
                path.is_absolute()
                    && path.to_str().is_some_and(
                        |text| text.len() <= 4096 && !text.chars().any(char::is_control)
                    )
                    && !path
                        .components()
                        .any(|part| matches!(part, Component::CurDir | Component::ParentDir)),
                "invalid managed genesis path"
            );
        }
        ensure!(
            self.control == self.state.join("control")
                && !self.source.starts_with(&self.install)
                && !self.install.starts_with(&self.source)
                && !self.source.starts_with(&self.state)
                && !self.state.starts_with(&self.source),
            "managed genesis payload/state isolation changed"
        );
        Ok(())
    }

    pub fn require_binding(
        &self,
        control: &Path,
        namespace: Incarnation,
        installer: Sha256Digest,
        operation: Incarnation,
    ) -> Result<()> {
        self.validate()?;
        ensure!(
            self.control == control
                && self.namespace == namespace
                && self.installer_identity == installer
                && self.operation == operation,
            "managed genesis operation or namespace changed"
        );
        Ok(())
    }

    pub fn digest(&self) -> Result<Sha256Digest> {
        self.validate()?;
        Ok(framed_digest(
            b"airk/managed-genesis-origin/1",
            &[&serde_json::to_vec(self)?],
        ))
    }
}
