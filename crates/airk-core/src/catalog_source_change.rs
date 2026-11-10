// SPDX-License-Identifier: Apache-2.0
//! Pure reviewed source-change contracts. Digests describe native observations;
//! they never establish filesystem identity or authorize configuration writes.
use crate::{
    catalog_management::{CatalogOperation, CatalogRequest, MAX_RECEIPT_BYTES},
    identity::{Incarnation, Sha256Digest, framed_digest},
    index_catalog::{MAX_SOURCE_ROOTS, SourceRootId, encoded_size},
    service_configuration::{ConfigurationRequest, ProjectConfiguration},
    service_records::{ProjectRegistration, RequestKey},
    source_policy::{SourceRoot, validate_root_path},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Known pre-insertion rejection: accepted work must settle before disabling
/// this index's source authority. No accepted job is reordered or canceled.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceChangeBusy {
    pub index_id: crate::index_catalog::IndexId,
    pub pending_jobs: u32,
    pub running_jobs: u32,
}
impl std::fmt::Display for SourceChangeBusy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "source change waits for {} queued and {} running jobs; finish or explicitly cancel accepted work before retrying",
            self.pending_jobs, self.running_jobs
        )
    }
}
impl std::error::Error for SourceChangeBusy {}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceConfigurationSnapshot {
    pub primary_root: String,
    pub aliases: BTreeMap<String, String>,
}
impl SourceConfigurationSnapshot {
    pub fn validate(&self) -> Result<()> {
        validate_root_path(&self.primary_root)?;
        ensure!(
            self.primary_root.len() <= 4096 && self.aliases.len() < MAX_SOURCE_ROOTS,
            "source configuration capacity exceeded"
        );
        let mut paths = BTreeSet::from([self.primary_root.as_str()]);
        for (alias, path) in &self.aliases {
            validate_root_path(path)?;
            ensure!(
                !alias.is_empty()
                    && alias.len() <= 64
                    && alias
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(&byte))
                    && path.len() <= 4096
                    && !path.chars().any(char::is_control)
                    && paths.insert(path.as_str()),
                "invalid or duplicate native source alias"
            );
        }
        ensure!(
            !self.primary_root.chars().any(char::is_control),
            "invalid primary source path"
        );
        encoded_size(self, MAX_RECEIPT_BYTES)?;
        Ok(())
    }
    pub fn identity(&self) -> Result<Sha256Digest> {
        self.validate()?;
        Ok(framed_digest(
            b"airk/catalog-source-configuration/1",
            &[&serde_json::to_vec(self)?],
        ))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceChangePlan {
    pub registration: ProjectRegistration,
    /// Stamped by the actual native metadata actor before review, not accepted
    /// as a frontend claim that another executable may run this configuration.
    pub engine_identity: Sha256Digest,
    pub job: Sha256Digest,
    pub configuration: ConfigurationRequest,
    pub before: SourceConfigurationSnapshot,
    pub after: SourceConfigurationSnapshot,
    /// Existing physical paths keep their source IDs. New identities are
    /// allocated by the native owner and frozen with this reviewed request.
    pub sources: Vec<SourceRoot>,
}
impl SourceChangePlan {
    pub fn validate(&self) -> Result<()> {
        self.registration.validate()?;
        self.configuration.validate()?;
        self.before.validate()?;
        self.after.validate()?;
        ensure!(
            self.registration.root == self.before.primary_root
                && self.before.primary_root == self.after.primary_root,
            "source change cannot relink the primary root or backing index"
        );
        let mut expected = self.before.aliases.clone();
        match &self.configuration.configuration {
            ProjectConfiguration::AddRoot { alias, path } => {
                ensure!(
                    !expected.contains_key(alias),
                    "source change alias already exists"
                );
                expected.insert(alias.clone(), path.clone());
            }
            ProjectConfiguration::RemoveRoot { alias } => {
                ensure!(
                    expected.remove(alias).is_some(),
                    "source change alias is unavailable"
                );
            }
            _ => anyhow::bail!("source change requires the exact AddRoot or RemoveRoot request"),
        }
        ensure!(
            expected == self.after.aliases && self.before != self.after,
            "source change desired state differs from reviewed operation"
        );
        ensure!(
            self.sources.len() == self.after.aliases.len() + 1
                && self.sources.len() <= MAX_SOURCE_ROOTS,
            "source change descriptor capacity mismatch"
        );
        let mut ids = BTreeSet::new();
        let mut paths = BTreeSet::new();
        let mut described = BTreeMap::new();
        let mut owners = 0;
        for source in &self.sources {
            source.validate()?;
            ensure!(
                ids.insert(source.id) && paths.insert(&source.canonical_path),
                "duplicate source change descriptor"
            );
            if source.owner {
                ensure!(
                    source.canonical_path == self.after.primary_root && source.aliases.is_empty(),
                    "source change primary descriptor mismatch"
                );
                owners += 1;
            } else {
                ensure!(
                    source.aliases.len() == 1,
                    "source change additional root requires one alias"
                );
                ensure!(
                    described
                        .insert(source.aliases[0].clone(), source.canonical_path.clone())
                        .is_none(),
                    "duplicate source change alias"
                );
            }
        }
        ensure!(
            owners == 1 && described == self.after.aliases,
            "source change descriptors differ from actual desired roots"
        );
        encoded_size(self, MAX_RECEIPT_BYTES)?;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case", deny_unknown_fields)]
pub enum IndexRetirementReason {
    Explicit,
    /// Constructed from the already validated native source observation; this
    /// enum alone is not proof that an external path was removed.
    ConfirmedMissingSource {
        registration_epoch: u64,
        configuration_epoch: u64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppliedSourceChange {
    pub schema: u32,
    pub begin_operation: Incarnation,
    pub begin_request_digest: Sha256Digest,
    pub job: Sha256Digest,
    pub source_revision: crate::index_catalog::CatalogRevision,
    pub actual: SourceConfigurationSnapshot,
    pub digest: Sha256Digest,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceChangeObservation {
    pub begin: crate::catalog_fences::FencedReceipt,
    pub job: Option<crate::service_records::JobRecord>,
    pub configuration_result: Option<serde_json::Value>,
    pub completion: Option<crate::catalog_management::CatalogReceipt>,
}
impl AppliedSourceChange {
    pub fn identity(&self) -> Result<Sha256Digest> {
        self.actual.validate()?;
        Ok(framed_digest(
            b"airk/catalog-applied-source-change/1",
            &[&serde_json::to_vec(&(
                self.schema,
                self.begin_operation,
                self.begin_request_digest,
                self.job,
                self.source_revision,
                &self.actual,
            ))?],
        ))
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == 1 && self.digest == self.identity()?,
            "source effect receipt identity mismatch"
        );
        encoded_size(self, MAX_RECEIPT_BYTES)?;
        Ok(())
    }
}

/// Existing canonical configuration-job identity, not a second work queue.
pub fn source_change_job_id(request: &CatalogRequest) -> Result<Sha256Digest> {
    let CatalogOperation::BeginIndexSourceChange { plan, .. } = &request.operation else {
        anyhow::bail!("source change begin required");
    };
    source_job_id(plan.registration.binding, request.caller, &request.key)
}
pub fn source_job_id(
    binding: Sha256Digest,
    caller: Incarnation,
    key: &RequestKey,
) -> Result<Sha256Digest> {
    ensure!(key.epoch <= i64::MAX as u64, "invalid source job epoch");
    Ok(framed_digest(
        b"airk-service-job/1",
        &[
            binding.as_bytes(),
            caller.as_bytes(),
            key.namespace.as_bytes(),
            &key.epoch.to_le_bytes(),
            key.nonce.as_bytes(),
        ],
    ))
}

/// Native allocation helper for a newly introduced path. Existing paths must
/// reuse their saved source identity, checked again by the Store transaction.
pub fn new_source_id(
    index: crate::index_catalog::IndexId,
    operation: Incarnation,
    path: &str,
) -> Result<SourceRootId> {
    validate_root_path(path)?;
    let digest = framed_digest(
        b"airk/catalog-source-change-root/1",
        &[
            index.to_string().as_bytes(),
            operation.as_bytes(),
            path.as_bytes(),
        ],
    )
    .to_string();
    Ok(SourceRootId::from_identity(digest[..32].parse()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index_catalog::{CatalogRevision, IndexId};
    fn request() -> Result<CatalogRequest> {
        let registration =
            ProjectRegistration::new("/source".into(), "/data".into(), "fixture".into())?;
        let namespace = Incarnation::from_bytes([1; 16]);
        let caller = Incarnation::from_bytes([2; 16]);
        let key = RequestKey {
            namespace,
            epoch: 0,
            nonce: Incarnation::from_bytes([3; 16]),
        };
        let index = IndexId::from_identity(Incarnation::from_bytes([4; 16]));
        let operation = crate::catalog_management::catalog_operation_id(caller, &key)?;
        let before = SourceConfigurationSnapshot {
            primary_root: "/source".into(),
            aliases: BTreeMap::new(),
        };
        let after = SourceConfigurationSnapshot {
            aliases: BTreeMap::from([("extra".into(), "/extra".into())]),
            ..before.clone()
        };
        let plan = SourceChangePlan {
            registration: registration.clone(),
            engine_identity: Sha256Digest::from_bytes([5; 32]),
            job: source_job_id(registration.binding, caller, &key)?,
            configuration: ConfigurationRequest {
                prepare: false,
                incarnation: namespace,
                settings: registration.binding,
                configuration: ProjectConfiguration::AddRoot {
                    alias: "extra".into(),
                    path: "/extra".into(),
                },
            },
            before,
            after,
            sources: vec![
                SourceRoot {
                    id: new_source_id(index, operation, "/source")?,
                    canonical_path: "/source".into(),
                    aliases: vec![],
                    owner: true,
                },
                SourceRoot {
                    id: new_source_id(index, operation, "/extra")?,
                    canonical_path: "/extra".into(),
                    aliases: vec!["extra".into()],
                    owner: false,
                },
            ],
        };
        Ok(CatalogRequest {
            caller,
            key,
            expected_catalog_revision: CatalogRevision::INITIAL,
            operation: CatalogOperation::BeginIndexSourceChange {
                id: index,
                expected_source_revision: CatalogRevision::INITIAL,
                plan: Box::new(plan),
            },
        })
    }
    #[test]
    fn boxed_source_plan_preserves_unboxed_wire_bytes_and_request_digest() -> Result<()> {
        #[derive(Serialize)]
        #[serde(tag = "operation", rename_all = "snake_case")]
        enum UnboxedOperation<'a> {
            BeginIndexSourceChange {
                id: crate::index_catalog::IndexId,
                expected_source_revision: crate::index_catalog::CatalogRevision,
                plan: &'a SourceChangePlan,
            },
        }
        #[derive(Serialize)]
        struct UnboxedRequest<'a> {
            caller: Incarnation,
            key: &'a RequestKey,
            expected_catalog_revision: crate::index_catalog::CatalogRevision,
            operation: UnboxedOperation<'a>,
        }
        let request = request()?;
        let CatalogOperation::BeginIndexSourceChange {
            id,
            expected_source_revision,
            plan,
        } = &request.operation
        else {
            unreachable!()
        };
        let bytes = serde_json::to_vec(&UnboxedRequest {
            caller: request.caller,
            key: &request.key,
            expected_catalog_revision: request.expected_catalog_revision,
            operation: UnboxedOperation::BeginIndexSourceChange {
                id: *id,
                expected_source_revision: *expected_source_revision,
                plan,
            },
        })?;
        assert_eq!(serde_json::to_vec(&request)?, bytes);
        assert_eq!(
            request.identity()?,
            framed_digest(b"airk/catalog-management/1", &[&bytes])
        );
        assert_eq!(serde_json::from_slice::<CatalogRequest>(&bytes)?, request);
        Ok(())
    }
    #[test]
    fn source_snapshot_rejects_alias_to_primary_and_duplicate_physical_root() -> Result<()> {
        let mut snapshot = SourceConfigurationSnapshot {
            primary_root: "/source".into(),
            aliases: BTreeMap::from([("alias".into(), "/source".into())]),
        };
        assert!(snapshot.validate().is_err());
        snapshot.aliases = BTreeMap::from([
            ("first".into(), "/extra".into()),
            ("second".into(), "/extra".into()),
        ]);
        assert!(snapshot.validate().is_err());
        Ok(())
    }
    #[test]
    fn source_review_binds_job_native_observation_and_exact_delta() -> Result<()> {
        let request = request()?;
        request.validate()?;
        for mode in 0..4 {
            let mut invalid = request.clone();
            let CatalogOperation::BeginIndexSourceChange { plan, .. } = &mut invalid.operation
            else {
                unreachable!()
            };
            match mode {
                0 => plan.configuration.incarnation = Incarnation::from_bytes([9; 16]),
                1 => plan.configuration.settings = Sha256Digest::from_bytes([9; 32]),
                2 => plan.job = Sha256Digest::from_bytes([9; 32]),
                _ => {
                    plan.after
                        .aliases
                        .insert("unreviewed".into(), "/other".into());
                }
            }
            if mode < 2 {
                // These are the actual collection incarnation/settings digest,
                // independently reviewed by the native owner. They are sealed
                // into the request, not equal to registry namespace/binding.
                invalid.validate()?;
                assert_ne!(invalid.identity()?, request.identity()?);
            } else {
                assert!(invalid.validate().is_err());
            }
        }
        let mut invalid = request.clone();
        let CatalogOperation::BeginIndexSourceChange { plan, .. } = &mut invalid.operation else {
            unreachable!()
        };
        plan.sources[1].aliases.push("unsupported".into());
        assert!(invalid.validate().is_err());
        assert_eq!(
            request.identity()?,
            serde_json::from_value::<CatalogRequest>(serde_json::to_value(&request)?)?
                .identity()?
        );
        Ok(())
    }
    #[test]
    fn source_native_effect_digest_binds_begin_job_revision_and_actual_config() -> Result<()> {
        let request = request()?;
        let CatalogOperation::BeginIndexSourceChange { plan, .. } = &request.operation else {
            unreachable!()
        };
        let mut effect = AppliedSourceChange {
            schema: 1,
            begin_operation: crate::catalog_management::catalog_operation_id(
                request.caller,
                &request.key,
            )?,
            begin_request_digest: request.identity()?,
            job: plan.job,
            source_revision: CatalogRevision::INITIAL.next()?,
            actual: plan.after.clone(),
            digest: Sha256Digest::from_bytes([0; 32]),
        };
        effect.digest = effect.identity()?;
        effect.validate()?;
        effect.actual.aliases.clear();
        assert!(effect.validate().is_err());
        Ok(())
    }
}
