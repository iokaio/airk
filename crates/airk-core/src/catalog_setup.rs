// SPDX-License-Identifier: Apache-2.0
//! Complete bounded index-setup wire contracts. These describe native metadata
//! operations; syntax, IDs and reported digests never establish filesystem proof.
use crate::{
    catalog_management::{MAX_RECEIPT_BYTES, catalog_operation_id},
    identity::{Incarnation, Sha256Digest, framed_digest},
    index_catalog::{
        CatalogRevision, IndexId, MAX_SOURCE_ROOTS, SourceRootId, encoded_size, label,
    },
    service::{ProjectRegistration, RequestKey},
    source_policy::validate_root_path,
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// User choices for native review. Canonical bindings and index/source IDs are
/// assigned by the native service, never inferred by an Explorer form.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndexSetupSelection {
    pub key: RequestKey,
    pub expected_catalog_revision: CatalogRevision,
    pub name: String,
    pub primary_root: String,
    pub data_root: String,
    pub collection: String,
    pub additional_roots: Vec<SetupAdditionalRoot>,
    pub setup_watch: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetupAdditionalRoot {
    pub alias: String,
    pub path: String,
}

impl IndexSetupSelection {
    pub fn validate(&self) -> Result<()> {
        label(&self.name)?;
        ensure!(
            self.additional_roots.len() < MAX_SOURCE_ROOTS,
            "too many setup roots"
        );
        for path in std::iter::once(&self.primary_root)
            .chain(std::iter::once(&self.data_root))
            .chain(self.additional_roots.iter().map(|root| &root.path))
        {
            validate_root_path(path)?;
            ensure!(
                path.len() <= 4096 && !path.chars().any(char::is_control),
                "invalid setup path"
            );
        }
        ensure!(
            !self.collection.is_empty() && self.collection.len() <= 255,
            "invalid setup collection"
        );
        encoded_size(self, MAX_RECEIPT_BYTES)?;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SetupStorageAction {
    CreateMissingLeaf,
    ClaimExistingEmptyStorage,
    ReuseOwnedStorage,
    ResumeReservedStorageClaim,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndexSetupDraft {
    pub schema: u32,
    pub request: IndexSetupRequest,
    pub storage_action: SetupStorageAction,
    pub review_digest: Sha256Digest,
}
impl IndexSetupDraft {
    pub fn new(
        request: IndexSetupRequest,
        storage_action: SetupStorageAction,
        review_digest: Sha256Digest,
    ) -> Result<Self> {
        request.validate()?;
        let draft = Self {
            schema: 1,
            request,
            storage_action,
            review_digest,
        };
        encoded_size(&draft, MAX_RECEIPT_BYTES)?;
        Ok(draft)
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(self.schema == 1, "unsupported setup review schema");
        self.request.validate()?;
        encoded_size(self, MAX_RECEIPT_BYTES)?;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetupSource {
    pub canonical_path: String,
    pub aliases: Vec<String>,
    pub owner: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndexSetupRequest {
    pub caller: Incarnation,
    pub key: RequestKey,
    pub expected_catalog_revision: CatalogRevision,
    pub name: String,
    pub registration: ProjectRegistration,
    /// Planned canonical metadata, not evidence of filesystem preparation.
    pub sources: Vec<SetupSource>,
    pub setup_watch: bool,
}

pub fn validate_setup_sources(
    registration: &ProjectRegistration,
    sources: &[SetupSource],
) -> Result<()> {
    ensure!(
        !sources.is_empty() && sources.len() <= MAX_SOURCE_ROOTS,
        "invalid setup source capacity"
    );
    let mut paths = BTreeSet::new();
    let mut aliases = BTreeSet::new();
    let mut owners = 0;
    for source in sources {
        validate_root_path(&source.canonical_path)?;
        ensure!(
            source.canonical_path.len() <= 4096
                && !source.canonical_path.chars().any(char::is_control)
                && paths.insert(&source.canonical_path),
            "invalid or duplicate setup source path"
        );
        ensure!(
            source.owner == (source.canonical_path == registration.root),
            "setup source owner disagrees with registration"
        );
        owners += usize::from(source.owner);
        ensure!(
            if source.owner {
                source.aliases.is_empty()
            } else {
                source.aliases.len() == 1
            },
            "native setup requires no primary alias and exactly one alias per additional root"
        );
        for alias in &source.aliases {
            ensure!(
                !alias.is_empty()
                    && alias.len() <= 64
                    && alias
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(&byte))
                    && aliases.insert(alias),
                "invalid or duplicate setup source alias"
            );
        }
    }
    ensure!(
        owners == 1 && aliases.len() <= 16,
        "setup requires one primary root and bounded aliases"
    );
    Ok(())
}

pub fn canonical_setup_sources(sources: &[SetupSource]) -> Result<Vec<SetupSource>> {
    ensure!(
        sources.len() <= MAX_SOURCE_ROOTS
            && sources.iter().all(|source| source.aliases.len() <= 16),
        "setup source canonicalization capacity exceeded"
    );
    encoded_size(&sources, MAX_RECEIPT_BYTES)?;
    let mut sources = sources.to_vec();
    for source in &mut sources {
        source.aliases.sort();
    }
    sources.sort_by(|a, b| a.canonical_path.cmp(&b.canonical_path));
    Ok(sources)
}

impl IndexSetupRequest {
    pub fn validate(&self) -> Result<()> {
        self.registration.validate()?;
        validate_root_path(&self.registration.root)?;
        validate_root_path(&self.registration.data_root)?;
        label(&self.name)?;
        ensure!(
            self.key.epoch <= i64::MAX as u64,
            "invalid setup request epoch"
        );
        validate_setup_sources(&self.registration, &self.sources)?;
        encoded_size(self, MAX_RECEIPT_BYTES)?;
        Ok(())
    }
    pub fn identity(&self) -> Result<Sha256Digest> {
        self.validate()?;
        Ok(framed_digest(
            b"airk/catalog-index-setup-request/1",
            &[&serde_json::to_vec(self)?],
        ))
    }
}

fn identity(domain: &[u8], parts: &[&[u8]]) -> Result<Incarnation> {
    framed_digest(domain, parts).to_string()[..32].parse()
}
pub fn setup_index_id(request: &IndexSetupRequest) -> Result<IndexId> {
    request.validate()?;
    Ok(IndexId::from_identity(identity(
        b"airk/catalog-index-setup-identity/1",
        &[
            request.key.namespace.as_bytes(),
            catalog_operation_id(request.caller, &request.key)?.as_bytes(),
        ],
    )?))
}
pub fn setup_source_ids(request: &IndexSetupRequest) -> Result<Vec<SourceRootId>> {
    request.validate()?;
    let index = setup_index_id(request)?.to_string();
    request
        .sources
        .iter()
        .map(|source| {
            Ok(SourceRootId::from_identity(identity(
                b"airk/catalog-index-setup-source/1",
                &[index.as_bytes(), source.canonical_path.as_bytes()],
            )?))
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetupPreparationReview {
    pub expected_catalog_revision: CatalogRevision,
    pub expected_source_revision: CatalogRevision,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreparationEvidence {
    pub digest: Sha256Digest,
    pub storage_identity: Sha256Digest,
    pub source_configuration_digest: Sha256Digest,
    pub review: SetupPreparationReview,
    /// Native saved-evidence intent. Ordinary setup cannot complete this
    /// reservation or enqueue its background work.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legacy_adoption: Option<Sha256Digest>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SetupStage {
    Reserved,
    Prepared,
    Ready,
}
impl SetupStage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Reserved => "reserved",
            Self::Prepared => "prepared",
            Self::Ready => "ready",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetupCompletion {
    pub prepared_digest: Sha256Digest,
    pub expected_catalog_revision: CatalogRevision,
    pub expected_source_revision: CatalogRevision,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legacy_adoption: Option<crate::catalog_legacy_adoption::LegacyAdoptionPublication>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndexSetupReceipt {
    pub schema: u32,
    pub operation_id: Incarnation,
    pub request: IndexSetupRequest,
    pub request_digest: Sha256Digest,
    pub index_id: IndexId,
    /// Same order as request.sources. Native identity never comes from the UI.
    pub source_ids: Vec<SourceRootId>,
    pub stage: SetupStage,
    pub catalog_revision: CatalogRevision,
    pub source_revision: CatalogRevision,
    pub preparation: Option<PreparationEvidence>,
    pub completion: Option<SetupCompletion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legacy_adoption: Option<Sha256Digest>,
}
impl IndexSetupReceipt {
    pub fn validate(&self) -> Result<()> {
        self.request.validate()?;
        ensure!(
            self.schema == 1
                && self.request.identity()? == self.request_digest
                && catalog_operation_id(self.request.caller, &self.request.key)?
                    == self.operation_id
                && setup_index_id(&self.request)? == self.index_id
                && setup_source_ids(&self.request)? == self.source_ids,
            "setup receipt identity conflict"
        );
        ensure!(
            match self.stage {
                SetupStage::Reserved => self.preparation.is_none() && self.completion.is_none(),
                SetupStage::Prepared => self.preparation.is_some() && self.completion.is_none(),
                SetupStage::Ready => self
                    .preparation
                    .as_ref()
                    .zip(self.completion.as_ref())
                    .is_some_and(|(prepared, completion)| prepared.digest
                        == completion.prepared_digest
                        && completion.expected_source_revision == CatalogRevision::INITIAL),
            },
            "invalid setup receipt stage"
        );
        let expected_revision = if let Some(completion) = &self.completion {
            completion.expected_catalog_revision.next()?
        } else {
            self.request.expected_catalog_revision.next()?
        };
        ensure!(
            self.catalog_revision == expected_revision
                && self.source_revision
                    == if self.stage == SetupStage::Ready {
                        CatalogRevision::INITIAL.next()?
                    } else {
                        CatalogRevision::INITIAL
                    },
            "invalid setup receipt revision"
        );
        if let Some(prepared) = &self.preparation {
            ensure!(
                prepared.legacy_adoption == self.legacy_adoption,
                "native preparation changed reserved adoption intent"
            );
            ensure!(
                prepared.review.expected_source_revision == CatalogRevision::INITIAL
                    && prepared.review.expected_catalog_revision
                        >= self.request.expected_catalog_revision.next()?,
                "invalid native preparation review revision"
            );
            if let Some(completion) = &self.completion {
                ensure!(
                    prepared.legacy_adoption
                        == completion
                            .legacy_adoption
                            .as_ref()
                            .map(|publication| publication.plan_digest),
                    "setup completion changed adoption intent"
                );
                if let Some(publication) = &completion.legacy_adoption {
                    publication.validate()?;
                }
                ensure!(
                    completion.expected_catalog_revision
                        >= prepared.review.expected_catalog_revision,
                    "setup completion precedes native preparation review"
                );
            }
        }
        encoded_size(self, MAX_RECEIPT_BYTES)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> Result<IndexSetupRequest> {
        Ok(IndexSetupRequest {
            caller: Incarnation::from_bytes([1; 16]),
            key: RequestKey {
                namespace: Incarnation::from_bytes([2; 16]),
                epoch: 0,
                nonce: Incarnation::from_bytes([3; 16]),
            },
            expected_catalog_revision: CatalogRevision::INITIAL,
            name: "Native setup".into(),
            registration: ProjectRegistration::new(
                "/source".into(),
                "/data".into(),
                "fixture".into(),
            )?,
            sources: vec![SetupSource {
                canonical_path: "/source".into(),
                aliases: vec![],
                owner: true,
            }],
            setup_watch: false,
        })
    }

    #[test]
    fn complete_wire_receipt_round_trip_preserves_request_identity_and_source_ids() -> Result<()> {
        let request = request()?;
        let receipt = IndexSetupReceipt {
            legacy_adoption: None,
            schema: 1,
            operation_id: catalog_operation_id(request.caller, &request.key)?,
            request_digest: request.identity()?,
            index_id: setup_index_id(&request)?,
            source_ids: setup_source_ids(&request)?,
            request: request.clone(),
            stage: SetupStage::Reserved,
            catalog_revision: CatalogRevision::INITIAL.next()?,
            source_revision: CatalogRevision::INITIAL,
            preparation: None,
            completion: None,
        };
        receipt.validate()?;
        #[derive(Serialize)]
        #[serde(tag = "kind", rename_all = "snake_case")]
        enum UnboxedObservation<'a> {
            IndexSetup { receipt: &'a IndexSetupReceipt },
        }
        let observation = crate::service::CatalogOutcomeObservation::IndexSetup {
            receipt: Box::new(receipt.clone()),
        };
        let original = serde_json::to_vec(&UnboxedObservation::IndexSetup { receipt: &receipt })?;
        assert_eq!(serde_json::to_vec(&observation)?, original);
        assert_eq!(
            serde_json::from_slice::<crate::service::CatalogOutcomeObservation>(&original)?,
            observation
        );
        let encoded = serde_json::to_value(&receipt)?;
        assert_eq!(encoded["stage"], "reserved");
        assert_eq!(encoded["request"]["setup_watch"], false);
        assert_eq!(
            serde_json::from_value::<IndexSetupReceipt>(encoded.clone())?,
            receipt
        );
        let mut unknown = encoded;
        unknown["prepared_by_frontend"] = serde_json::json!(true);
        assert!(serde_json::from_value::<IndexSetupReceipt>(unknown).is_err());
        let mut changed = request;
        changed.name = "Changed review".into();
        assert_ne!(changed.identity()?, receipt.request_digest);
        // Native operation allocation stays stable; the body digest detects reuse.
        assert_eq!(setup_index_id(&changed)?, receipt.index_id);
        changed.key.namespace = Incarnation::from_bytes([4; 16]);
        assert_ne!(setup_index_id(&changed)?, receipt.index_id);
        Ok(())
    }

    #[test]
    fn source_identity_survives_source_order_without_merging_distinct_paths() -> Result<()> {
        let mut request = request()?;
        request.sources.push(SetupSource {
            canonical_path: "/support".into(),
            aliases: vec!["support".into()],
            owner: false,
        });
        let mut ids = setup_source_ids(&request)?;
        let canonical = canonical_setup_sources(&request.sources)?;
        request.sources.reverse();
        ids.reverse();
        assert_eq!(setup_source_ids(&request)?, ids);
        assert_eq!(canonical_setup_sources(&request.sources)?, canonical);
        request.sources[0].canonical_path = "/support-other".into();
        assert_ne!(setup_source_ids(&request)?[0], ids[0]);
        request.sources[0].aliases.push("extra".into());
        assert!(request.validate().is_err());
        assert!(setup_source_ids(&request).is_err());
        Ok(())
    }

    #[test]
    fn normal_setup_refuses_alias_shapes_the_native_pipeline_cannot_publish() -> Result<()> {
        let mut primary_alias = request()?;
        primary_alias.sources[0]
            .aliases
            .push("primary_alias".into());
        assert!(primary_alias.validate().is_err());
        let mut multiple_aliases = request()?;
        multiple_aliases.sources.push(SetupSource {
            canonical_path: "/support".into(),
            aliases: vec!["first".into(), "second".into()],
            owner: false,
        });
        assert!(multiple_aliases.validate().is_err());
        multiple_aliases.sources[1].aliases.clear();
        assert!(multiple_aliases.validate().is_err());
        // Historical imported catalog metadata retains its representation. Its
        // effective scope is separately validated; this is not new setup admission.
        let imported = crate::source_policy::SourceRoot {
            id: SourceRootId::from_identity(Incarnation::from_bytes([9; 16])),
            canonical_path: "/source".into(),
            owner: true,
            aliases: vec!["legacy_a".into(), "legacy_b".into()],
        };
        imported.validate()?;
        Ok(())
    }
}
