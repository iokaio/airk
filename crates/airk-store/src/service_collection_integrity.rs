// SPDX-License-Identifier: Apache-2.0
//! Durable observations only. This module grants no relaxed read authority.
use super::*;
use crate::current::{CurrentHead, CurrentRead};
#[path = "service_collection_native_owner.rs"]
pub mod native_owner;
use crate::service::{ProjectLease, data_scope::BoundDataRoot, registry::ServiceRegistry};

#[path = "service_collection_census.rs"]
pub mod census;
#[path = "service_collection_verify.rs"]
pub mod verify;

const RECEIPT: &str = "integrity-observation.json";
const MAX_DAMAGE: usize = 32;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrivacyEligibility {
    /// Owner/mode warnings are not complete owner/DACL or ACL evidence.
    PlatformEvidenceUnavailable,
    /// Dated observation only; never reconstructs the live borrowed capability.
    OwnerCensus {
        version: u32,
        objects: usize,
        observed_unix_ms: u64,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicationBasis {
    RetainedHeadMetadataAnchor,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PhysicalAudit {
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnownDamage {
    pub incarnation: Incarnation,
    pub component: Sha256Digest,
    pub observed_unix_ms: u64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntegrityObservation {
    version: u32,
    owner: Sha256Digest,
    namespace: Incarnation,
    worker_epoch: u64,
    pub head: CurrentHead,
    pub basis: PublicationBasis,
    pub physical_audit: PhysicalAudit,
    pub privacy: PrivacyEligibility,
    pub known_damage: Vec<KnownDamage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verify: Option<verify::VerifyProgress>,
}

pub(super) fn owned_scratch(path: &Path, owner: Sha256Digest) -> bool {
    json::<IntegrityObservation>(path).is_ok_and(|value| {
        value.version == 1
            && value.owner == owner
            && value.known_damage.len() <= MAX_DAMAGE
            && value.head.validate().is_ok()
    }) || verify::owned_recovery_scratch(path, owner)
        || verify::owned_projection_scratch(path, owner)
}

/// Non-Clone borrow retains the actual process lease and rechecks its registry
/// epoch and bound data root for each metadata operation. Never retain CurrentRead.
pub struct IntegrityOwner<'a> {
    collection: &'a Collection,
    registry: &'a ServiceRegistry,
    lease: &'a ProjectLease,
    bound: &'a BoundDataRoot,
}
impl Collection {
    pub fn integrity_owner<'a>(
        &'a self,
        registry: &'a ServiceRegistry,
        lease: &'a ProjectLease,
        bound: &'a BoundDataRoot,
    ) -> Result<IntegrityOwner<'a>> {
        let owner = IntegrityOwner {
            collection: self,
            registry,
            lease,
            bound,
        };
        owner.check()?;
        Ok(owner)
    }
}
impl IntegrityOwner<'_> {
    fn check(&self) -> Result<(Incarnation, u64)> {
        let (registration, namespace, epoch) = self
            .registry
            .integrity_registration(self.lease, self.bound)?;
        ensure!(
            registration.root == self.collection.owner.root
                && registration.collection == self.collection.owner.collection
                && registration.data_root == self.collection.owner.data_root,
            "foreign integrity collection authority"
        );
        Ok((namespace, epoch))
    }
    fn load(&self, namespace: Incarnation) -> Result<Option<IntegrityObservation>> {
        let path = self.collection.directory.join(RECEIPT);
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
            Ok(_) => {}
        }
        let state: IntegrityObservation = json(&path)?;
        ensure!(
            state.version == 1
                && state.owner == self.collection.binding
                && state.namespace == namespace
                && state.known_damage.len() <= MAX_DAMAGE,
            "foreign or unsupported integrity observation"
        );
        state.head.validate()?;
        ensure!(
            state.head.scope.root == self.collection.owner.root
                && state.head.scope.collection == self.collection.owner.collection,
            "foreign integrity publication"
        );
        Ok(Some(state))
    }
    /// Anchor only metadata already selected by the caller's authorized read.
    /// No lexical/object audit, no writer-certificate promotion, no query bypass.
    pub fn anchor(&self, read: &CurrentRead) -> Result<IntegrityObservation> {
        self.publish(read, None, PrivacyEligibility::PlatformEvidenceUnavailable)
    }
    /// Bounded initial native metadata enrollment under the actual logical writer.
    /// Opens its own fresh read; accepts no retained pre-mutation snapshot. This
    /// owning operation may use existing collection lock creation. It creates no
    /// restart/query authority, producer certificate or physical audit coverage.
    pub fn enroll_native_metadata(
        &self,
        writer: &crate::WriterLock,
    ) -> Result<Option<IntegrityObservation>> {
        self.check()?;
        let selected = self.collection.select()?;
        crate::current::gate::check_writer(&selected.store, writer)?;
        let Some(scope) = selected.store.current_owned_scope()? else {
            return Ok(None);
        };
        let read = selected.store.current_read(&scope)?;
        if read.settled_native_head()?.is_none() {
            return Ok(None);
        }
        Ok(Some(self.anchor(&read)?))
    }
    /// Durable sticky detection, never a claim that other components are clean.
    /// Only actual verifier callers may use this; permission/busy is not damage.
    pub fn record_damage(
        &self,
        read: &CurrentRead,
        component: Sha256Digest,
        observed_unix_ms: u64,
    ) -> Result<IntegrityObservation> {
        self.bound.revoke_native_integrity();
        let control = self.registry.persist_integrity_damage(
            self.lease,
            read.head().scope.incarnation,
            component,
            observed_unix_ms,
        );
        let local = self.publish(
            read,
            Some(KnownDamage {
                incarnation: read.head().scope.incarnation,
                component,
                observed_unix_ms,
            }),
            PrivacyEligibility::PlatformEvidenceUnavailable,
        );
        match (local, control) {
            (Ok(value), Ok(())) => Ok(value),
            (Err(error), Ok(())) => {
                Err(error.context("collection damage receipt failed; control veto was persisted"))
            }
            (Ok(_), Err(error)) => {
                Err(error.context("control veto failed; collection damage receipt was persisted"))
            }
            (Err(error), Err(control)) => Err(error.context(format!(
                "neither damage store confirmed persistence; control failure: {control:#}"
            ))),
        }
    }
    fn publish(
        &self,
        read: &CurrentRead,
        damage: Option<KnownDamage>,
        privacy: PrivacyEligibility,
    ) -> Result<IntegrityObservation> {
        let _lock = manifest_lock(&self.collection.directory)?;
        let (namespace, epoch) = self.check()?;
        let selected = self.collection.manifest()?;
        let expected = FileStore::new(self.collection.slot(selected.active));
        // A replaced slot/recovered publication cannot be relabelled by an old read.
        ensure!(
            expected.current_owned_scope()?.as_ref() == Some(&read.head().scope),
            "integrity selection changed"
        );
        read.token()?;
        // Follow the existing reader's journal/WAL authentication; never turn a
        // stale in-memory head into an observation over an unsafe overlay.
        // A valid Applying transaction may still expose its authenticated base.
        {
            let current = expected.current_read(&read.head().scope)?;
            ensure!(
                current.token()? == read.token()?,
                "integrity publication changed"
            );
        }
        let head = read.head().clone();
        head.validate()?;
        ensure!(
            head.scope.root == self.collection.owner.root
                && head.scope.collection == self.collection.owner.collection,
            "foreign integrity head"
        );
        let previous = self.load(namespace)?;
        let verify = previous.as_ref().and_then(|value| value.verify.clone());
        let mut known_damage = previous.map_or_else(Vec::new, |value| value.known_damage);
        if let Some(damage) = damage
            && !known_damage.iter().any(|old| {
                old.incarnation == damage.incarnation && old.component == damage.component
            })
        {
            ensure!(
                known_damage.len() < MAX_DAMAGE,
                "integrity damage receipt full; repair required"
            );
            known_damage.push(damage);
        }
        let state = IntegrityObservation {
            version: 1,
            owner: self.collection.binding,
            namespace,
            worker_epoch: epoch,
            head,
            known_damage,
            basis: PublicationBasis::RetainedHeadMetadataAnchor,
            physical_audit: PhysicalAudit::Unknown,
            privacy,
            verify,
        };
        // Same lease/root fence immediately before durable metadata publication.
        ensure!(
            self.check()? == (namespace, epoch),
            "integrity worker changed"
        );
        write(self.collection, RECEIPT, &state)?;
        Ok(state)
    }
}
