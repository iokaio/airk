// SPDX-License-Identifier: Apache-2.0
//! Stable required split capability and native lease-bound staged access.
use super::*;
use crate::service::registry::catalog::split::NativeSplitState;
use airk_core::catalog_source_change::SourceConfigurationSnapshot;
use airk_core::{
    catalog_split::{
        SplitEvidenceConstraint, SplitJobPurpose, SplitOwnerReference, SplitOwnerRole,
        SplitRunState, SplitSnapshot, SplitStage,
    },
    service::ProjectRegistration,
};
use std::sync::Arc;

#[path = "service_collection_split_terminal.rs"]
mod terminal;
pub(crate) use terminal::{NativeTerminalPublication, TerminalSlot};
const MAX_SPLIT_STATE: usize = 128 * 1024;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SplitOwnerPointer {
    pub reference: SplitOwnerReference,
    pub revision: airk_core::index_catalog::CatalogRevision,
    pub digest: Sha256Digest,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Capsule {
    schema: u32,
    snapshot: SplitSnapshot,
}

fn filename(digest: Sha256Digest) -> String {
    format!("split-state-{digest}.json")
}
fn capsule(directory: &Path, pointer: &SplitOwnerPointer) -> Result<Capsule> {
    let path = directory.join(filename(pointer.digest));
    let metadata = plain(&path)?;
    ensure!(
        metadata.is_file() && metadata.len() <= MAX_SPLIT_STATE as u64,
        "split lifecycle capsule exceeds bound"
    );
    let mut bytes = Vec::new();
    File::open(&path)?
        .take(MAX_SPLIT_STATE as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= MAX_SPLIT_STATE
            && framed_digest(b"airk/stable-split-state/1", &[&bytes]) == pointer.digest,
        "split lifecycle capsule identity differs"
    );
    let value: Capsule = serde_json::from_slice(&bytes)?;
    value.snapshot.validate()?;
    ensure!(
        value.schema == 1
            && serde_json::to_vec(&value)? == bytes
            && value.snapshot.receipt.revision == pointer.revision,
        "split lifecycle capsule noncanonical"
    );
    pointer
        .reference
        .validate_for(&value.snapshot.request, &value.snapshot.receipt)?;
    Ok(value)
}

pub(super) fn validate_capsule_for_retirement(
    directory: &Path,
    pointer: &SplitOwnerPointer,
    permit: Option<&SplitBuildPermit>,
) -> Result<()> {
    let state = capsule(directory, pointer)?.snapshot;
    if !matches!(
        state.receipt.state,
        SplitRunState::Complete
            | SplitRunState::CanceledBeforeFence {
                cleanup_pending: false
            }
    ) {
        let permit =
            permit.context("active split cannot be removed through ordinary retirement")?;
        ensure!(
            permit.purpose == SplitJobPurpose::DestinationCleanup,
            "active split removal requires exact canceled destination cleanup"
        );
        permit.validate_store(&FileStore::new(directory))?;
    }
    Ok(())
}
pub(super) fn validate_retained_capsule_file(path: &Path) -> Result<()> {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .context("invalid split capsule name")?;
    let digest: Sha256Digest = name
        .strip_prefix("split-state-")
        .and_then(|value| value.strip_suffix(".json"))
        .context("invalid split capsule name")?
        .parse()?;
    let metadata = plain(path)?;
    ensure!(
        metadata.is_file() && metadata.len() <= MAX_SPLIT_STATE as u64,
        "invalid retained split capsule"
    );
    let bytes = fs::read(path)?;
    ensure!(
        framed_digest(b"airk/stable-split-state/1", &[&bytes]) == digest,
        "retained split capsule identity differs"
    );
    let value: Capsule = serde_json::from_slice(&bytes)?;
    value.snapshot.validate()?;
    ensure!(
        value.schema == 1 && serde_json::to_vec(&value)? == bytes,
        "retained split capsule is noncanonical"
    );
    Ok(())
}

/// Native setup format adapter. It seals an unpublished destination owner and
/// returns its content-addressed lifecycle file before any document bytes exist.
pub(crate) fn initial_split_seal(
    files: &mut [(&'static str, Vec<u8>)],
    state: &NativeSplitState,
) -> Result<(String, Vec<u8>)> {
    let value = &state.0;
    value.validate()?;
    ensure!(
        value.receipt.stage == SplitStage::Accepted
            && !matches!(
                value.receipt.state,
                SplitRunState::CanceledBeforeFence { .. } | SplitRunState::Complete
            ),
        "initial split shell requires exact active Accepted intent"
    );
    let (_, owner_bytes) = files
        .iter_mut()
        .find(|(name, _)| *name == "service-owner.json")
        .context("initial split owner metadata missing")?;
    let mut owner: Owner = serde_json::from_slice(owner_bytes)?;
    ensure!(
        owner_registration(&owner)? == value.request.destination_registration
            && owner.required_split.is_none(),
        "initial split destination owner differs"
    );
    let reference = SplitOwnerReference {
        required_contract: airk_core::catalog_split::SPLIT_CONTRACT,
        operation_id: value.receipt.operation_id,
        request_digest: value.receipt.request_digest,
        index_id: value.request.destination.index_id,
        role: SplitOwnerRole::Destination,
        activation_receipt: None,
    };
    reference.validate_for(&value.request, &value.receipt)?;
    let bytes = serde_json::to_vec(&Capsule {
        schema: 1,
        snapshot: value.clone(),
    })?;
    ensure!(
        bytes.len() <= MAX_SPLIT_STATE,
        "initial split capsule exceeds bound"
    );
    let digest = framed_digest(b"airk/stable-split-state/1", &[&bytes]);
    owner.required_split = Some(SplitOwnerPointer {
        reference,
        revision: value.receipt.revision,
        digest,
    });
    *owner_bytes = serde_json::to_vec(&owner)?;
    ensure!(
        owner_bytes.len() <= MAX_METADATA,
        "initial split owner exceeds bound"
    );
    Ok((filename(digest), bytes))
}

fn owner_registration(owner: &Owner) -> Result<ProjectRegistration> {
    ProjectRegistration::new(
        owner.root.clone(),
        owner.data_root.clone(),
        owner.collection.clone(),
    )
}

/// Not serializable and not constructible by a frontend. It retains the actual
/// claimed worker lease and exact admitted phase, rather than a wire bypass flag.
#[derive(Clone, Debug)]
pub struct SplitBuildPermit {
    registration: ProjectRegistration,
    operation: Incarnation,
    request: Sha256Digest,
    revision: airk_core::index_catalog::CatalogRevision,
    purpose: SplitJobPurpose,
    job: airk_core::catalog_split::SplitJobRef,
    index: airk_core::index_catalog::IndexId,
    _lease: Arc<File>,
    retry_predecessor: Option<airk_core::catalog_split::SplitJobRef>,
}
impl SplitBuildPermit {
    pub(super) fn retained_policy_already_installed(
        &self,
        store: &FileStore,
        logical: &crate::service::registry::catalog::setup::storage::SetupDirectory,
        slot: &crate::service::registry::catalog::setup::storage::SetupDirectory,
        quota: &Quota,
    ) -> Result<bool> {
        self.validate_pinned_store(store, logical)?;
        let funding = quota
            .funding_use()
            .context("installed retained policy requires exact funding")?;
        self.validate_funding_use(funding)?;
        quota.require_store_funding(store)?;
        ensure!(
            self.purpose == SplitJobPurpose::DestinationBuild,
            "only exact DestinationBuild can observe installed initial retention"
        );
        let owner: Owner = super::retention_barrier::read_json(
            logical,
            "service-owner.json",
            MAX_METADATA as u64,
        )?;
        if owner.required_extraction_contract
            != Some(super::RequiredExtractionContract::try_from(3)?)
        {
            return Ok(false);
        }
        let pointer = owner
            .required_split
            .as_ref()
            .context("installed retained capability has no split owner")?;
        let bytes = logical.read_file(
            std::ffi::OsStr::new(&filename(pointer.digest)),
            MAX_SPLIT_STATE as u64,
        )?;
        ensure!(
            framed_digest(b"airk/stable-split-state/1", &[&bytes]) == pointer.digest,
            "installed retained split capsule changed"
        );
        let capsule: Capsule = serde_json::from_slice(&bytes)?;
        capsule.snapshot.validate()?;
        ensure!(
            capsule.schema == 1
                && serde_json::to_vec(&capsule)? == bytes
                && capsule.snapshot.receipt.revision == self.revision
                && capsule.snapshot.receipt.operation_id == self.operation
                && capsule.snapshot.receipt.request_digest == self.request,
            "installed retained split phase changed"
        );
        let Some(reference) = capsule.snapshot.request.extraction_policy.as_ref() else {
            return Ok(false);
        };
        let Some(body) =
            crate::extraction_policy::extraction_policy_bytes_at(slot, &self.registration.root)?
        else {
            return Ok(false);
        };
        reference.validate_body(&body)?;
        Ok(true)
    }

    pub(crate) fn claimed(
        state: &NativeSplitState,
        work: &airk_core::catalog_split::SplitJobRef,
        lease: &crate::service::ProjectLease,
    ) -> Result<Self> {
        let value = &state.0;
        value.validate()?;
        ensure!(
            value.receipt.jobs.iter().any(|job| job == work) && lease.identity().0 == work.backing,
            "split permit differs from claimed native job"
        );
        let registration = match work.purpose {
            SplitJobPurpose::DestinationBuild | SplitJobPurpose::DestinationCleanup => {
                value.request.destination_registration.clone()
            }
            _ => value.request.original_registration.clone(),
        };
        Ok(Self {
            registration: registration.clone(),
            operation: value.receipt.operation_id,
            request: value.receipt.request_digest,
            revision: value.receipt.revision,
            purpose: work.purpose,
            job: work.clone(),
            index: if registration.binding == value.request.destination_registration.binding {
                value.request.destination.index_id
            } else {
                value.request.original_index
            },
            _lease: Arc::new(lease.duplicate_split_pin()?),
            retry_predecessor: None,
        })
    }
    pub(crate) fn with_native_retry_predecessor(
        mut self,
        previous: airk_core::catalog_split::SplitJobRef,
    ) -> Result<Self> {
        ensure!(
            previous.purpose == self.purpose
                && previous.backing == self.job.backing
                && previous.engine_identity == self.job.engine_identity
                && previous.job != self.job.job,
            "split retry predecessor differs"
        );
        self.retry_predecessor = Some(previous);
        Ok(self)
    }
    pub(crate) fn validate_funding_retry_takeover(
        &self,
        previous: &crate::quota::funding::FundingPhase,
        next: &crate::quota::funding::FundingUse,
    ) -> Result<()> {
        self.validate_funding_use(next)?;
        let predecessor = self
            .retry_predecessor
            .as_ref()
            .context("split retry lacks exact native terminal predecessor")?;
        ensure!(
            previous.purpose == self.purpose
                && previous.job == predecessor.job
                && previous.revision < next.phase.revision
                && matches!(
                    self.purpose,
                    SplitJobPurpose::DestinationBuild
                        | SplitJobPurpose::OriginalRemoval
                        | SplitJobPurpose::OriginalCleanup
                ),
            "split funding retry crossed native predecessor phase"
        );
        Ok(())
    }
    pub(crate) fn validate_funding_use(
        &self,
        funding: &crate::quota::funding::FundingUse,
    ) -> Result<()> {
        ensure!(
            funding.operation_id == self.operation
                && funding.request_digest == self.request
                && funding.index_id == self.index
                && funding.data_root == Path::new(&self.registration.data_root)
                && funding.phase.job == self.job.job
                && funding.phase.purpose == self.purpose
                && funding.phase.revision == self.revision.get(),
            "split funding differs from exact claimed native work"
        );
        Ok(())
    }
    pub(crate) fn validate_pinned_store(
        &self,
        store: &FileStore,
        pin: &crate::service::registry::catalog::setup::storage::SetupDirectory,
    ) -> Result<()> {
        use crate::service::registry::catalog::setup::storage::SetupDirectory;
        use std::ffi::OsStr;
        let managed = store.managed_path()?;
        ensure!(
            SetupDirectory::open_canonical(managed.logical)?.identity()? == pin.identity()?,
            "split named collection differs from held directory"
        );
        let bytes = pin.read_file(OsStr::new("service-owner.json"), MAX_METADATA as u64)?;
        let owner: Owner = serde_json::from_slice(&bytes)?;
        ensure!(
            serde_json::to_vec(&owner)? == bytes
                && owner_registration(&owner)? == self.registration,
            "split pinned owner differs"
        );
        let pointer = owner
            .required_split
            .as_ref()
            .context("split pinned owner has no capability")?;
        let state_bytes = pin.read_file(
            OsStr::new(&filename(pointer.digest)),
            MAX_SPLIT_STATE as u64,
        )?;
        ensure!(
            framed_digest(b"airk/stable-split-state/1", &[&state_bytes]) == pointer.digest,
            "split pinned capsule identity differs"
        );
        let state: Capsule = serde_json::from_slice(&state_bytes)?;
        state.snapshot.validate()?;
        ensure!(
            state.schema == 1
                && serde_json::to_vec(&state)? == state_bytes
                && state.snapshot.receipt.revision == self.revision
                && state.snapshot.receipt.operation_id == self.operation
                && state.snapshot.receipt.request_digest == self.request,
            "split pinned phase differs"
        );
        pointer
            .reference
            .validate_for(&state.snapshot.request, &state.snapshot.receipt)?;
        self.validate_store(store)?;
        ensure!(
            SetupDirectory::open_canonical(managed.logical)?.identity()? == pin.identity()?
                && pin.read_file(OsStr::new("service-owner.json"), MAX_METADATA as u64)? == bytes,
            "split pinned collection changed during authorization"
        );
        Ok(())
    }
    pub(crate) fn validate_store(&self, store: &FileStore) -> Result<()> {
        let managed = store.managed_path()?;
        ensure!(
            managed.namespace == "collections-1.4"
                && managed.root == Path::new(&self.registration.data_root)
                && managed.collection == self.registration.collection,
            "split permit backing differs"
        );
        let owner: Owner = json(&managed.logical.join("service-owner.json"))?;
        let pointer = owner
            .required_split
            .as_ref()
            .context("split permit requires stable owner capability")?;
        ensure!(
            owner_registration(&owner)? == self.registration,
            "split permit source binding differs"
        );
        let value = capsule(managed.logical, pointer)?.snapshot;
        ensure!(
            value.receipt.operation_id == self.operation
                && value.receipt.request_digest == self.request
                && value.receipt.revision == self.revision,
            "split permit phase expired"
        );
        let permitted = match self.purpose {
            SplitJobPurpose::DestinationBuild => {
                pointer.reference.role == SplitOwnerRole::Destination
                    && matches!(
                        value.receipt.stage,
                        SplitStage::DestinationPrepared | SplitStage::Building
                    )
                    && matches!(
                        value.receipt.state,
                        SplitRunState::Pending | SplitRunState::Running
                    )
            }
            SplitJobPurpose::OriginalWitness => {
                pointer.reference.role == SplitOwnerRole::OriginalRemainder
                    && value.receipt.stage == SplitStage::DestinationValidated
            }
            SplitJobPurpose::OriginalRemoval => {
                pointer.reference.role == SplitOwnerRole::OriginalRemainder
                    && value.receipt.stage == SplitStage::OriginalFenced
            }
            SplitJobPurpose::DestinationCleanup => {
                pointer.reference.role == SplitOwnerRole::Destination
                    && matches!(
                        value.receipt.state,
                        SplitRunState::CanceledBeforeFence {
                            cleanup_pending: true
                        }
                    )
            }
            SplitJobPurpose::OriginalCleanup => {
                pointer.reference.role == SplitOwnerRole::OriginalRemainder
                    && value.receipt.stage >= SplitStage::MembershipApplied
            }
        };
        ensure!(
            permitted,
            "split permit purpose is unavailable at this lifecycle stage"
        );
        Ok(())
    }
}

pub fn validate_split_store_access(store: &FileStore) -> Result<()> {
    let Ok(managed) = store.managed_path() else {
        return Ok(());
    };
    if managed.namespace != "collections-1.4" {
        return Ok(());
    }
    let owner_path = managed.logical.join("service-owner.json");
    match fs::symlink_metadata(&owner_path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && managed.slot.is_none() => {
            return Ok(());
        }
        Err(error) => return Err(error.into()),
        Ok(_) => (),
    }
    let owner: Owner = json(&owner_path)?;
    let Some(pointer) = &owner.required_split else {
        return Ok(());
    };
    let value = capsule(managed.logical, pointer)?;
    match pointer
        .reference
        .evidence_constraint(&value.snapshot.request, &value.snapshot.receipt)?
    {
        SplitEvidenceConstraint::PreserveOriginalAdmission
        | SplitEvidenceConstraint::ReviewedActive => Ok(()),
        _ => store
            .split_access
            .as_ref()
            .context("split evidence is staged or awaiting restrictive cutover")?
            .validate_store(store),
    }
}

impl Collection {
    /// Select an actual owned slot for worker diagnostics before claim. The
    /// returned store carries NO split permit, so current/evidence entry points
    /// retain their ordinary stage denial. This is metadata selection only.
    pub fn select_for_diagnostics(&self) -> Result<FileStore> {
        let owner: Owner = json(&self.directory.join("service-owner.json"))?;
        ensure!(
            owner_digest(&owner)? == self.binding
                && owner_registration(&owner)? == owner_registration(&self.owner)?,
            "diagnostic collection ownership changed"
        );
        let split = owner
            .required_split
            .as_ref()
            .map(|pointer| capsule(&self.directory, pointer))
            .transpose()?;
        let canceled = split.as_ref().is_some_and(|state| {
            matches!(
                state.snapshot.receipt.state,
                SplitRunState::CanceledBeforeFence {
                    cleanup_pending: true
                }
            )
        });
        let _lock = match retry_manifest_lock(|| {
            crate::service::acquire_shared_file(&self.directory, "manifest.lock")
        }) {
            Ok(lock) => lock,
            Err(error)
                if canceled
                    && error
                        .downcast_ref::<std::io::Error>()
                        .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound) =>
            {
                match fs::symlink_metadata(self.directory.join("incarnations")) {
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        return Ok(FileStore::new(&self.directory));
                    }
                    Err(error) => return Err(error.into()),
                    Ok(_) => anyhow::bail!(
                        "split cleanup lacks its manifest lock but retains unexplained incarnations"
                    ),
                }
            }
            Err(error) => return Err(error),
        };
        match self.manifest() {
            Ok(manifest) => Ok(FileStore::new(self.slot(manifest.active))),
            Err(error)
                if error
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound)
                    && split.as_ref().is_some_and(|state| {
                        matches!(
                            state.snapshot.receipt.state,
                            SplitRunState::CanceledBeforeFence {
                                cleanup_pending: true
                            }
                        )
                    }) =>
            {
                match fs::symlink_metadata(self.directory.join("incarnations")) {
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        Ok(FileStore::new(&self.directory))
                    }
                    Err(error) => Err(error.into()),
                    Ok(_) => anyhow::bail!(
                        "split cleanup has no manifest but retains unexplained incarnations"
                    ),
                }
            }
            Err(error) => Err(error),
        }
    }
    /// Build only a compact native job configuration from owned metadata. No
    /// evidence read/model bind/source capture is authorized by this result.
    pub fn split_work_configuration(
        &self,
        state: &NativeSplitState,
        purpose: SplitJobPurpose,
    ) -> Result<airk_core::service_configuration::ConfigurationRequest> {
        use airk_core::service_configuration::{ConfigurationRequest, ProjectConfiguration};
        let value = &state.0;
        value.validate()?;
        let registration = match purpose {
            SplitJobPurpose::DestinationBuild | SplitJobPurpose::DestinationCleanup => {
                &value.request.destination_registration
            }
            SplitJobPurpose::OriginalWitness | SplitJobPurpose::OriginalCleanup => {
                &value.request.original_registration
            }
            SplitJobPurpose::OriginalRemoval => {
                anyhow::bail!("original removal reuses its exact admitted P9 policy configuration")
            }
        };
        ensure!(
            owner_registration(&self.owner)? == *registration,
            "split configuration collection differs"
        );
        let owner: Owner = json(&self.directory.join("service-owner.json"))?;
        if let Some(pointer) = owner.required_split.as_ref() {
            let previous = capsule(&self.directory, pointer)?.snapshot;
            ensure!(
                previous.request == value.request
                    && previous.plan == value.plan
                    && pointer.revision <= value.receipt.revision,
                "split configuration owner lineage differs"
            );
        } else {
            ensure!(
                purpose == SplitJobPurpose::OriginalWitness,
                "split configuration lacks stable required owner"
            );
        }
        let allowed = match purpose {
            SplitJobPurpose::DestinationBuild => {
                matches!(
                    value.receipt.stage,
                    SplitStage::DestinationPrepared | SplitStage::Building
                ) && !matches!(
                    value.receipt.state,
                    SplitRunState::CanceledBeforeFence { .. } | SplitRunState::Complete
                )
            }
            SplitJobPurpose::DestinationCleanup => matches!(
                value.receipt.state,
                SplitRunState::CanceledBeforeFence {
                    cleanup_pending: true
                }
            ),
            SplitJobPurpose::OriginalWitness => {
                value.receipt.stage == SplitStage::DestinationValidated
                    && !matches!(
                        value.receipt.state,
                        SplitRunState::CanceledBeforeFence { .. } | SplitRunState::Complete
                    )
            }
            SplitJobPurpose::OriginalCleanup => {
                value.receipt.stage >= SplitStage::MembershipApplied
                    && value.receipt.stage < SplitStage::Complete
            }
            SplitJobPurpose::OriginalRemoval => false,
        };
        ensure!(allowed, "split job configuration phase differs");
        let store = self.select_for_diagnostics()?;
        let incarnation = store
            .service_incarnation()?
            .context("split slot already removed; use exact saved cleanup configuration")?;
        let _writer = store.existing_writer_exclusion()?;
        let settings = settings_id(&settings(&store, &registration.root)?);
        if purpose == SplitJobPurpose::OriginalWitness {
            ensure!(
                incarnation == value.request.original_incarnation
                    && settings == value.request.original_settings_digest,
                "split original witness settings/incarnation changed"
            );
        }
        let request = ConfigurationRequest {
            prepare: false,
            incarnation,
            settings,
            configuration: ProjectConfiguration::SplitWork {
                operation_id: value.receipt.operation_id,
                request_digest: value.receipt.request_digest,
                expected_receipt_digest: value.receipt_digest,
                purpose,
                require_semantic_ready: purpose == SplitJobPurpose::DestinationBuild
                    && value.request.require_semantic_ready,
            },
        };
        request.validate()?;
        Ok(request)
    }
    /// Inspect only the stable owner/capsule before any recovery or slot open.
    pub fn split_evidence_constraint(&self) -> Result<Option<SplitEvidenceConstraint>> {
        let owner: Owner = json(&self.directory.join("service-owner.json"))?;
        ensure!(
            owner_digest(&owner)? == self.binding
                && owner_registration(&owner)? == owner_registration(&self.owner)?,
            "split owner metadata changed"
        );
        let Some(pointer) = owner.required_split.as_ref() else {
            return Ok(None);
        };
        let value = capsule(&self.directory, pointer)?.snapshot;
        Ok(Some(
            pointer
                .reference
                .evidence_constraint(&value.request, &value.receipt)?,
        ))
    }
    /// Metadata-only observation; opening this owner confers no evidence access.
    pub fn required_split(&self) -> Result<Option<SplitOwnerReference>> {
        let owner: Owner = json(&self.directory.join("service-owner.json"))?;
        Ok(owner.required_split.map(|pointer| pointer.reference))
    }
    pub fn with_split_access(mut self, permit: SplitBuildPermit) -> Result<Self> {
        ensure!(
            permit.registration == owner_registration(&self.owner)?,
            "split collection permit binding differs"
        );
        self.split_access = Some(permit);
        Ok(self)
    }
    pub(super) fn check_split_access(&self) -> Result<()> {
        let manifest = self.manifest()?;
        let mut store = FileStore::new(self.slot(manifest.active));
        if let Some(permit) = &self.split_access {
            store = store.with_split_access(permit.clone())?;
        }
        validate_split_store_access(&store)
    }
    /// Publish a content-addressed immutable capsule BEFORE changing its owner
    /// pointer. Readers of an earlier pre-fence original capsule stay available.
    /// The caller supplies the exact funded quota after admission; an Accepted
    /// metadata shell may use its existing finite setup metadata allowance.
    pub fn retain_split_state(&self, state: &NativeSplitState, quota: &Quota) -> Result<()> {
        let value = &state.0;
        value.validate()?;
        let registration = owner_registration(&self.owner)?;
        let (role, index) = if registration == value.request.original_registration {
            (
                SplitOwnerRole::OriginalRemainder,
                value.request.original_index,
            )
        } else {
            ensure!(
                registration == value.request.destination_registration,
                "split owner registration differs"
            );
            (
                SplitOwnerRole::Destination,
                value.request.destination.index_id,
            )
        };
        ensure!(
            quota.root() == Path::new(&registration.data_root),
            "split metadata quota owner differs"
        );
        let reference = SplitOwnerReference {
            required_contract: airk_core::catalog_split::SPLIT_CONTRACT,
            operation_id: value.receipt.operation_id,
            request_digest: value.receipt.request_digest,
            index_id: index,
            role,
            activation_receipt: value.receipt.membership_receipt,
        };
        reference.validate_for(&value.request, &value.receipt)?;
        let bytes = serde_json::to_vec(&Capsule {
            schema: 1,
            snapshot: value.clone(),
        })?;
        ensure!(
            bytes.len() <= MAX_SPLIT_STATE,
            "split capsule exceeds format budget"
        );
        let digest = framed_digest(b"airk/stable-split-state/1", &[&bytes]);
        let _lock = manifest_lock(&self.directory)?;
        let mut owner: Owner = json(&self.directory.join("service-owner.json"))?;
        ensure!(
            owner_digest(&owner)? == self.binding && owner_registration(&owner)? == registration,
            "split stable ownership changed"
        );
        if let Some(old) = &owner.required_split {
            let previous = capsule(&self.directory, old)?;
            if old.reference.operation_id == reference.operation_id {
                ensure!(
                    old.reference.request_digest == reference.request_digest
                        && old.revision <= value.receipt.revision,
                    "split owner lifecycle regressed"
                );
                if old.revision == value.receipt.revision {
                    ensure!(old.digest == digest, "split owner revision was reused");
                    return Ok(());
                }
            } else {
                ensure!(
                    matches!(
                        previous.snapshot.receipt.state,
                        SplitRunState::Complete
                            | SplitRunState::CanceledBeforeFence {
                                cleanup_pending: false
                            }
                    ),
                    "previous split owner remains active"
                );
            }
        }
        let reservation = quota.reserve(
            &registration.collection,
            u64::try_from(bytes.len() + MAX_METADATA * 2)?,
        )?;
        reservation.charge(bytes.len() as u64)?;
        let path = self.directory.join(filename(digest));
        if path.try_exists()? {
            ensure!(fs::read(&path)? == bytes, "split capsule name collision");
        } else {
            let mut file = tempfile::Builder::new()
                .prefix(".service-metadata-")
                .tempfile_in(&self.directory)?;
            file.write_all(&bytes)?;
            file.as_file().sync_all()?;
            crate::durability::persist_synced(file, &path)?;
        }
        owner.required_split = Some(SplitOwnerPointer {
            reference,
            revision: value.receipt.revision,
            digest,
        });
        let owner_bytes = serde_json::to_vec(&owner)?;
        ensure!(
            owner_bytes.len() <= MAX_METADATA,
            "split owner metadata exceeds bound"
        );
        reservation.charge(owner_bytes.len() as u64)?;
        let mut file = tempfile::Builder::new()
            .prefix(".service-metadata-")
            .tempfile_in(&self.directory)?;
        file.write_all(&owner_bytes)?;
        file.as_file().sync_all()?;
        crate::durability::persist_synced(file, &self.directory.join("service-owner.json"))
    }
    /// Native metadata checkpoint under the participant's existing funded
    /// phase. The internal store never receives an evidence permit and never
    /// escapes this method; journal qualification returns only quota accounting.
    pub fn retain_split_state_and_reconcile(
        &self,
        state: &NativeSplitState,
        expected: &crate::quota::funding::FundingReceipt,
    ) -> Result<crate::quota::funding::FundingReceipt> {
        let value = &state.0;
        value.validate()?;
        expected.validate_for(&value.request)?;
        let registration = owner_registration(&self.owner)?;
        let index = if registration == value.request.original_registration {
            value.request.original_index
        } else {
            ensure!(
                registration == value.request.destination_registration,
                "split metadata checkpoint owner differs"
            );
            value.request.destination.index_id
        };
        ensure!(
            expected.data_root == Path::new(&registration.data_root)
                && value.receipt.funding_receipts.get(&registration.data_root)
                    == Some(&expected.identity()?),
            "split metadata checkpoint funding was not admitted"
        );
        let quota = Quota::open_service(&expected.data_root)?;
        quota.validate_funding(expected)?;
        let participant = expected
            .participants
            .iter()
            .find(|participant| participant.index_id == index)
            .context("split metadata funding participant missing")?;
        let phase = participant
            .phase
            .as_ref()
            .context("split metadata checkpoint requires its admitted participant phase")?;
        let owner = quota.adopt_funding_phase(expected, index, phase)?;
        let quota = quota.with_funding(owner.clone())?;
        let mut store = self.select_for_diagnostics()?;
        let writer = store.lock()?;
        ensure!(
            self.select_for_diagnostics()?.directory() == store.directory(),
            "split metadata selected slot changed"
        );
        // Private quota-only attachment. split_access remains absent, so ordinary
        // current/evidence APIs still refuse the staged destination.
        store.funding = Some(owner);
        self.retain_split_state(state, &quota)?;
        store.reconcile_native_split_metadata(&writer, &quota)
    }
    /// Native protected setup validates this owned metadata without granting an
    /// evidence reader to the staged destination.
    pub fn split_preparation(
        &self,
        state: &NativeSplitState,
    ) -> Result<crate::service::registry::catalog::split::NativeSplitPreparation> {
        let value = &state.0;
        ensure!(
            owner_registration(&self.owner)? == value.request.destination_registration,
            "split preparation owner differs"
        );
        let manifest = self.manifest()?;
        let store = FileStore::new(self.slot(manifest.active));
        let roots = crate::roots::RootConfig::load(store.directory(), &self.owner.root)?;
        ensure!(
            SourceConfigurationSnapshot {
                primary_root: self.owner.root.clone(),
                aliases: roots.roots
            } == value.request.source_configuration,
            "split prepared root configuration differs"
        );
        let pointer: Owner = json(&self.directory.join("service-owner.json"))?;
        let pointer = pointer
            .required_split
            .context("split preparation lacks required staging owner")?;
        ensure!(
            pointer.reference.role == SplitOwnerRole::Destination
                && pointer.reference.operation_id == value.receipt.operation_id,
            "split preparation staging owner differs"
        );
        Ok(
            crate::service::registry::catalog::split::NativeSplitPreparation::new(
                state,
                framed_digest(
                    b"airk/split-prepared-storage/1",
                    &[&serde_json::to_vec(&(
                        self.binding,
                        manifest.active,
                        &value.request.source_configuration,
                    ))?],
                ),
            ),
        )
    }
}
