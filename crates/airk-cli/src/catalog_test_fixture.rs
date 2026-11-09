// SPDX-License-Identifier: Apache-2.0
//! Complete physical-format component fixtures, NOT managed activation proofs.
use airk_core::{
    catalog_fences::{FencePlan, FencePreparation},
    catalog_management::{CatalogOperation, CatalogRequest},
    catalog_setup::{IndexSetupRequest, SetupCompletion, SetupPreparationReview, SetupSource},
    identity::{Incarnation, Sha256Digest},
    index_catalog::{CatalogRevision, Membership, PathScope, QueryProjectId, ScopeAtom},
    service::{ProjectRegistration, RequestKey},
};
use airk_store::service::{
    ServiceLease,
    registry::{
        ServiceRegistry,
        catalog::{
            genesis::{
                CatalogGenesis,
                activation::{
                    AuthorityV1CandidateObservation, AuthorityV1NativeQualification,
                    AuthorityV1ReadinessProducer,
                },
            },
            setup::NativeIndexPreparation,
        },
    },
};
use anyhow::{Context, Result};
use std::path::Path;

/// Isolated fresh-installation producer: no predecessor payload exists.
struct FreshInstallation;
impl AuthorityV1ReadinessProducer for FreshInstallation {
    fn required_predecessor_identities(&self) -> Result<Vec<Sha256Digest>> {
        Ok(Vec::new())
    }
    fn qualify(
        &self,
        expected: &AuthorityV1CandidateObservation,
    ) -> Result<AuthorityV1NativeQualification> {
        Ok(AuthorityV1NativeQualification {
            candidate: expected.clone(),
            predecessor_refusals: Vec::new(),
        })
    }
}

/// Settles an empty activated authority-1 catalog through the real owned
/// Genesis adapter. The registry namespace is `instance`; the caller supplies
/// no projects, so Genesis refuses any populated registry.
pub(crate) fn authority_one(root: &Path, instance: Incarnation) -> Result<ServiceRegistry> {
    crate::local_ipc::prepare_root(root)?;
    drop(ServiceRegistry::open(root, instance)?);
    let mut genesis = CatalogGenesis::begin(
        ServiceLease::acquire(root)?,
        Sha256Digest::from_bytes([2; 32]),
        Incarnation::from_bytes([3; 16]),
    )?;
    genesis.backup()?;
    genesis.migrate()?;
    genesis.promote_authority_v1()?;
    genesis.record_authority_v1_readiness(Incarnation::from_bytes([4; 16]), &FreshInstallation)?;
    genesis.settle_authority_v1()?;
    drop(genesis);
    ServiceRegistry::open(root, instance)
}

/// One existing collection admitted through canonical index setup, plus a
/// Project whose whole-root membership and default is that index. Only the
/// physical native preparation proof is substituted; storage is not created.
pub(crate) fn reviewed_index(
    registry: &mut ServiceRegistry,
    registration: &ProjectRegistration,
    sources: Vec<SetupSource>,
    seed: u8,
) -> Result<()> {
    let caller = Incarnation::from_bytes([seed; 16]);
    let (namespace, epoch) = registry.saved_request_epoch()?;
    let key = |offset: u8| RequestKey {
        namespace,
        epoch,
        nonce: Incarnation::from_bytes([seed.wrapping_add(offset); 16]),
    };
    let request = IndexSetupRequest {
        caller,
        key: key(1),
        expected_catalog_revision: registry
            .catalog_state()?
            .context("fixture catalog absent")?
            .revision,
        name: registration.collection.clone(),
        registration: registration.clone(),
        sources: sources.clone(),
        setup_watch: false,
    };
    let reserved = registry.reserve_index_setup(&request)?;
    let native = NativeIndexPreparation::from_validated_native_review(
        registration.clone(),
        sources,
        Sha256Digest::from_bytes([seed; 32]),
        Sha256Digest::from_bytes([seed.wrapping_add(1); 32]),
        SetupPreparationReview {
            expected_catalog_revision: reserved.catalog_revision,
            expected_source_revision: reserved.source_revision,
        },
    )?;
    let prepared = registry.record_index_preparation(
        caller,
        &request.key,
        reserved.request_digest,
        &native,
    )?;
    let ready = registry.complete_index_setup(
        caller,
        &request.key,
        reserved.request_digest,
        &SetupCompletion {
            legacy_adoption: None,
            prepared_digest: prepared
                .preparation
                .as_ref()
                .context("fixture preparation absent")?
                .digest,
            expected_catalog_revision: prepared.catalog_revision,
            expected_source_revision: prepared.source_revision,
        },
    )?;
    let project = QueryProjectId::from_identity(Incarnation::from_bytes([seed; 16]));
    let mut apply = |offset: u8, operation: CatalogOperation| -> Result<()> {
        let request = CatalogRequest {
            caller,
            key: key(offset),
            expected_catalog_revision: registry
                .catalog_state()?
                .context("fixture catalog absent")?
                .revision,
            operation,
        };
        let preview = registry.catalog_preview(&request)?.identity()?;
        if !request.operation.requires_authority_fence() {
            registry.catalog_apply(&request, preview)?;
            return Ok(());
        }
        // No grant or installed scope exists yet, so the fence has no targets.
        let plan = FencePlan::new(caller, 1, Vec::new())?;
        let FencePreparation::Prepared(intent) =
            registry.catalog_prepare_fence(&request, preview, &plan)?
        else {
            anyhow::bail!("fresh fixture fence unexpectedly replayed");
        };
        registry.catalog_record_fence_confirmations(
            caller,
            &request.key,
            intent.prepared_digest,
            &[],
        )?;
        registry.catalog_apply_fenced(&request, preview, intent.prepared_digest)?;
        Ok(())
    };
    apply(
        2,
        CatalogOperation::CreateProject {
            id: project,
            name: format!("Fixture project: {}", registration.collection),
        },
    )?;
    apply(
        3,
        CatalogOperation::SetMembership {
            membership: Membership {
                project_id: project,
                index_id: ready.index_id,
                revision: CatalogRevision::INITIAL,
                scope: ready
                    .source_ids
                    .iter()
                    .map(|source| ScopeAtom {
                        source_root_id: *source,
                        kind: PathScope::Subtree,
                        path: String::new(),
                    })
                    .collect(),
            },
            expected_scope_revision: CatalogRevision::INITIAL,
            expected_membership_revision: None,
        },
    )?;
    apply(
        4,
        CatalogOperation::SetProjectDefaults {
            project_id: project,
            expected_defaults_revision: CatalogRevision::INITIAL,
            index_ids: vec![ready.index_id],
        },
    )?;
    Ok(())
}
