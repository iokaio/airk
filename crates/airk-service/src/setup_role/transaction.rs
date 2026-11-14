// SPDX-License-Identifier: Apache-2.0
//! Native custody for the reviewed private setup owner. Ordinary I4 readers
//! remain strict; only this exact-operation path classifies interrupted bytes.
use super::{Installed, files, plan};
use airk_core::{
    identity::Incarnation,
    install_record::{
        IndexDeletion, Locations, MAX_POINTER_BYTES, MAX_RECORD_BYTES, OperationKind, RECORD_FILE,
        SetupOperation, SetupReceipt, canonical,
        setup_recovery::{
            InstallationImage, MAX_RECOVERY_BYTES, PublicationPosition, PublicationReceipt,
            RetainedInstallation, RetainedRoots, RootIdentity, RootObservation,
            active::{
                ACTIVE_SCHEMA, ActiveSetup, CreatedRoot, RootBinding, RootBindings, RootSnapshot,
                Stage, Target,
            },
        },
    },
};
use airk_process::directory::{ObjectIdentity, OpenedDirectory};
use anyhow::{Context, Result, bail, ensure};
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

const ACTIVE_FILE: &str = "active-setup.json";

/// Original admission survives a crash before the first main receipt write.
/// This grants no effects; full live-root/custody checks follow under the lock.
pub(super) fn admission_time(
    pointer: &Path,
    operation: &SetupOperation,
    digest: airk_core::identity::Sha256Digest,
) -> Result<Option<u64>> {
    let folder = pointer.parent().context("pointer parent")?;
    let Some(bytes) = optional(&folder.join(ACTIVE_FILE), MAX_RECOVERY_BYTES)? else {
        return Ok(None);
    };
    let active: ActiveSetup = serde_json::from_slice(&bytes)?;
    active.validate().map_err(refusal)?;
    ensure!(
        canonical(&active) == bytes,
        "setup_owner_refused: noncanonical active owner"
    );
    if active.operation != *operation || active.operation_sha256 != digest {
        return Ok(None);
    }
    let directory = OpenedDirectory::open(folder)?;
    airk_ipc::endpoint::require_private_directory_handle(directory.as_file())?;
    ensure!(
        native(&directory)? == active.authority_root,
        "setup_owner_refused: authority changed"
    );
    Ok(Some(active.admitted_unix_ms))
}

/// A published candidate may start while its own completing setup holds the
/// installation lock. All other starts retain that lock through readiness.
pub(super) fn startup_allowed(pointer: &Path, control: &Path) -> Result<bool> {
    let folder = pointer.parent().context("pointer parent")?;
    let Some(bytes) = optional(&folder.join(ACTIVE_FILE), MAX_RECOVERY_BYTES)? else {
        return Ok(false);
    };
    let active: ActiveSetup = serde_json::from_slice(&bytes)?;
    active.validate().map_err(refusal)?;
    ensure!(
        canonical(&active) == bytes,
        "setup_owner_refused: noncanonical active owner"
    );
    let authority = OpenedDirectory::open(folder)?;
    airk_ipc::endpoint::require_private_directory_handle(authority.as_file())?;
    let mut owner = Owner {
        active,
        authority,
        folder: folder.to_owned(),
        held: Vec::new(),
    };
    owner.revalidate()?;
    if owner.active.stage == Stage::Settled {
        let receipt = files::read_receipt(
            Path::new(&owner.active.operation.locations.state),
            &owner.active.operation.operation_id,
        )?
        .context("setup_owner_refused: settled owner receipt missing")?;
        owner
            .active
            .validate_settlement(&receipt)
            .map_err(refusal)?;
        owner.existing(pointer)?;
        return Ok(false);
    }
    ensure!(
        owner.active.stage == Stage::PairPublished,
        "installation_in_progress: setup {} owns placement",
        owner.active.operation.operation_id
    );
    let candidate = owner.candidate()?;
    let (own_path, own_digest) = super::own_image()?;
    ensure!(
        owner
            .active
            .candidate_start_allowed(candidate.record.install_id, own_digest)
            && super::canonical_or_same(control)
                == super::canonical_or_same(Path::new(&candidate.record.locations.state))
            && own_path
                == super::canonical_or_same(
                    &Path::new(&candidate.record.locations.program)
                        .join(&candidate.record.executables.airk.path)
                ),
        "installation_in_progress: only the exact published candidate may start"
    );
    let actual = owner
        .existing(pointer)?
        .context("candidate installation missing")?;
    ensure!(
        actual.pointer == candidate.pointer && actual.record == candidate.record,
        "setup_owner_refused: published candidate changed"
    );
    super::verify_existing(pointer)?;
    super::inno::require_start_custody(candidate)?;
    Ok(true)
}

#[cfg(windows)]
pub(super) fn completed_for(pointer: &Path, operation: &SetupOperation) -> Result<ActiveSetup> {
    let folder = pointer.parent().context("pointer parent")?;
    let bytes = optional(&folder.join(ACTIVE_FILE), MAX_RECOVERY_BYTES)?
        .context("setup_owner_refused: completed active owner missing")?;
    let active = ActiveSetup::decode(&bytes, operation).map_err(refusal)?;
    ensure!(
        active.stage == Stage::Settled,
        "setup_owner_refused: setup has not settled"
    );
    let authority = OpenedDirectory::open(folder)?;
    airk_ipc::endpoint::require_private_directory_handle(authority.as_file())?;
    let mut owner = Owner {
        active,
        authority,
        folder: folder.to_owned(),
        held: Vec::new(),
    };
    owner.revalidate()?;
    owner.existing(pointer)?;
    let receipt = files::read_receipt(
        Path::new(&operation.locations.state),
        &operation.operation_id,
    )?
    .context("setup_owner_refused: completed receipt missing")?;
    owner
        .active
        .validate_settlement(&receipt)
        .map_err(refusal)?;
    Ok(owner.active)
}

fn refusal(error: airk_core::install_record::Refusal) -> anyhow::Error {
    anyhow::anyhow!("setup_owner_refused: {}", error.code())
}

fn image(installed: &Installed) -> InstallationImage {
    InstallationImage {
        pointer: installed.pointer.clone(),
        record: installed.record.clone(),
    }
}

fn native(directory: &OpenedDirectory) -> Result<RootIdentity> {
    Ok(match directory.identity()? {
        ObjectIdentity::Windows {
            volume_serial,
            file_id,
        } => RootIdentity::Windows {
            volume_serial,
            file_id: file_id.iter().map(|byte| format!("{byte:02x}")).collect(),
        },
        ObjectIdentity::Unix { device, inode } => RootIdentity::Unix { device, inode },
    })
}

fn root(path: &Path, held: &mut Vec<OpenedDirectory>) -> Result<RootObservation> {
    match std::fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(RootObservation::Absent),
        Err(error) => Err(error.into()),
        Ok(_) => {
            let pin = OpenedDirectory::open(path)?;
            airk_ipc::endpoint::require_private_directory_handle(pin.as_file())?;
            let observed = RootObservation::Present {
                identity: native(&pin)?,
            };
            held.push(pin);
            Ok(observed)
        }
    }
}

fn snapshot(locations: &Locations, held: &mut Vec<OpenedDirectory>) -> Result<RootSnapshot> {
    Ok(RootSnapshot {
        program: root(Path::new(&locations.program), held)?,
        retained: RetainedRoots {
            data: root(Path::new(&locations.data), held)?,
            state: root(Path::new(&locations.state), held)?,
            cache: root(Path::new(&locations.cache), held)?,
            logs: root(Path::new(&locations.logs), held)?,
        },
    })
}

pub(super) fn admission_roots(locations: &Locations) -> Result<RootSnapshot> {
    snapshot(locations, &mut Vec::new())
}

pub(super) fn owns_control(pointer: &Path, control: &Path) -> Result<Option<bool>> {
    let folder = pointer.parent().context("pointer parent")?;
    let Some(bytes) = optional(&folder.join(ACTIVE_FILE), MAX_RECOVERY_BYTES)? else {
        return Ok(None);
    };
    let active: ActiveSetup = serde_json::from_slice(&bytes)?;
    active.validate().map_err(refusal)?;
    ensure!(
        canonical(&active) == bytes,
        "setup_owner_refused: noncanonical active owner"
    );
    let root = super::canonical_or_same(control);
    let selected = super::canonical_or_same(Path::new(&active.operation.locations.state));
    let prior = match &active.target {
        Target::Place { publication } => publication
            .prior
            .as_ref()
            .map(|image| super::canonical_or_same(Path::new(&image.record.locations.state))),
        _ => None,
    };
    Ok(Some(
        root == selected || prior.as_ref().is_some_and(|prior| prior == &root),
    ))
}

fn bindings(roots: &RootSnapshot) -> RootBindings {
    let binding = |initial: &RootObservation| RootBinding {
        initial: initial.clone(),
        created: None,
    };
    RootBindings {
        program: binding(&roots.program),
        data: binding(&roots.retained.data),
        state: binding(&roots.retained.state),
        cache: binding(&roots.retained.cache),
        logs: binding(&roots.retained.logs),
        program_replacement: None,
    }
}

fn optional(path: &Path, limit: usize) -> Result<Option<Vec<u8>>> {
    match std::fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
        Ok(_) => files::read_bounded(path, limit, true).map(Some),
    }
}

pub(super) struct Owner {
    pub active: ActiveSetup,
    authority: OpenedDirectory,
    folder: PathBuf,
    // Windows retains all selected names during effects. Unix retains objects;
    // rooted mutation adapters must still use these descriptors.
    held: Vec<OpenedDirectory>,
}

impl Owner {
    pub fn observe(pointer: &Path) -> Result<Option<Self>> {
        let folder = pointer.parent().context("install pointer parent")?;
        let Some(bytes) = optional(&folder.join(ACTIVE_FILE), MAX_RECOVERY_BYTES)? else {
            return Ok(None);
        };
        let active: ActiveSetup = serde_json::from_slice(&bytes)?;
        active.validate().map_err(refusal)?;
        ensure!(
            canonical(&active) == bytes,
            "setup_owner_refused: noncanonical active owner"
        );
        let authority = OpenedDirectory::open(folder)?;
        airk_ipc::endpoint::require_private_directory_handle(authority.as_file())?;
        let mut owner = Self {
            active,
            authority,
            folder: folder.to_owned(),
            held: Vec::new(),
        };
        owner.revalidate()?;
        Ok(Some(owner))
    }

    pub fn load(pointer: &Path, operation: &SetupOperation) -> Result<Option<Self>> {
        let Some(owner) = Self::observe(pointer)? else {
            return Ok(None);
        };
        let bytes = canonical(&owner.active);
        if owner.active.stage == Stage::Settled {
            let receipt = files::read_receipt(
                Path::new(&owner.active.operation.locations.state),
                &owner.active.operation.operation_id,
            )?
            .context("setup_owner_refused: settled owner has no main receipt")?;
            owner
                .active
                .validate_settlement(&receipt)
                .map_err(refusal)?;
            owner.existing(pointer)?;
            let history = owner.folder.join("setup-history");
            let history_pin = private_child(&owner.authority, "setup-history")?;
            airk_ipc::endpoint::require_private_directory_handle(history_pin.as_file())?;
            let name = format!("{}.json", owner.active.operation.operation_id);
            if let Some(previous) = optional(&history.join(&name), MAX_RECOVERY_BYTES)? {
                ensure!(
                    previous == bytes,
                    "setup_owner_refused: completed history changed"
                );
            } else {
                airk_ipc::private_record::write_private_at(
                    &history_pin,
                    &name,
                    &bytes,
                    MAX_RECOVERY_BYTES,
                    false,
                )?;
                history_pin.sync_all()?;
            }
            airk_ipc::private_record::remove_private_at(&owner.authority, ACTIVE_FILE, &bytes)?;
            owner.authority.sync_all()?;
            return Ok(None);
        }
        owner.active.admit_same_operation(operation).map_err(|_| {
            anyhow::anyhow!(
                "installation_in_progress: operation {} still owns setup",
                owner.active.operation.operation_id
            )
        })?;
        Ok(Some(owner))
    }

    pub fn admit(
        pointer: &Path,
        operation: &SetupOperation,
        admitted_unix_ms: u64,
        prior: Option<&Installed>,
        candidate: Option<InstallationImage>,
        expected_roots: Option<&RootSnapshot>,
    ) -> Result<Self> {
        let folder = pointer
            .parent()
            .context("install pointer parent")?
            .to_owned();
        let authority = OpenedDirectory::open(&folder)?;
        airk_ipc::endpoint::require_private_directory_handle(authority.as_file())?;
        let mut held = Vec::new();
        let selected = snapshot(&operation.locations, &mut held)?;
        ensure!(
            expected_roots.is_none_or(|expected| expected == &selected),
            "setup_owner_refused: selected roots changed during admission"
        );
        let prior_roots = if !plan::removes(operation.kind) {
            prior
                .map(|installed| snapshot(&installed.record.locations, &mut held))
                .transpose()?
        } else {
            None
        };
        let target = if plan::removes(operation.kind) {
            Target::Remove {
                installation: Box::new(image(prior.context(
                    "removal_ownership_unproven: no installation or active original proof",
                )?)),
            }
        } else {
            Target::Place {
                publication: Box::new(PublicationReceipt {
                    schema: airk_core::install_record::setup_recovery::PUBLICATION_SCHEMA.into(),
                    operation_id: operation.operation_id,
                    operation_sha256: files::digest_bytes(&canonical(operation)),
                    payload_inventory_sha256: operation.payload_inventory_sha256,
                    prior: prior.map(image),
                    candidate: candidate.context("candidate installation missing")?,
                }),
            }
        };
        let active = ActiveSetup {
            schema: ACTIVE_SCHEMA.into(),
            sequence: 1,
            operation_sha256: files::digest_bytes(&canonical(operation)),
            operation: operation.clone(),
            admitted_unix_ms,
            authority_root: native(&authority)?,
            target,
            selected_roots: bindings(&selected),
            prior_roots,
            stage: Stage::Admitted,
        };
        active.validate().map_err(refusal)?;
        let owner = Self {
            active,
            authority,
            folder,
            held,
        };
        owner.save(false)?;
        Ok(owner)
    }

    fn save(&self, replace: bool) -> Result<()> {
        self.require_current_authority()?;
        airk_ipc::private_record::write_private_at(
            &self.authority,
            ACTIVE_FILE,
            &canonical(&self.active),
            MAX_RECOVERY_BYTES,
            replace,
        )?;
        self.authority.sync_all()?;
        Ok(())
    }

    fn require_current_authority(&self) -> Result<()> {
        let named = OpenedDirectory::open(&self.folder)?;
        airk_ipc::endpoint::require_private_directory_handle(named.as_file())?;
        ensure!(
            native(&named)? == self.active.authority_root
                && native(&self.authority)? == self.active.authority_root,
            "setup_owner_refused: authority directory name was rebound"
        );
        Ok(())
    }

    fn update(&mut self, next: ActiveSetup) -> Result<()> {
        next.validate_successor(&self.active).map_err(refusal)?;
        let previous = std::mem::replace(&mut self.active, next);
        if let Err(error) = self.save(true) {
            self.active = previous;
            return Err(error);
        }
        Ok(())
    }

    pub fn advance(&mut self, stage: Stage) -> Result<()> {
        if self.active.stage == stage {
            return Ok(());
        }
        let mut next = self.active.clone();
        next.sequence = next
            .sequence
            .checked_add(1)
            .context("setup owner sequence exhausted")?;
        next.stage = stage;
        self.update(next)
    }

    pub fn revalidate(&mut self) -> Result<()> {
        self.require_current_authority()?;
        let mut held = Vec::new();
        let selected = snapshot(&self.active.operation.locations, &mut held)?;
        let prior = match &self.active.target {
            Target::Place { publication } => publication
                .prior
                .as_ref()
                .map(|image| snapshot(&image.record.locations, &mut held))
                .transpose()?,
            Target::Remove { .. } => None,
        };
        self.active
            .validate_live_roots(&native(&self.authority)?, &selected, prior.as_ref(), None)
            .map_err(refusal)?;
        self.held = held;
        Ok(())
    }

    /// This is the sole interrupted-publication exception. Actual inventory,
    /// source image and live-root admission remain mandatory at its caller.
    pub fn existing(&self, pointer: &Path) -> Result<Option<Installed>> {
        let pointer_bytes = optional(pointer, MAX_POINTER_BYTES)?;
        let installed = |image: &InstallationImage| Installed {
            pointer: image.pointer.clone(),
            record: image.record.clone(),
        };
        match &self.active.target {
            Target::Place { publication } => {
                let candidate_bytes = optional(
                    &Path::new(&publication.candidate.pointer.program).join(RECORD_FILE),
                    MAX_RECORD_BYTES,
                )?;
                let prior_bytes = publication
                    .prior
                    .as_ref()
                    .filter(|prior| prior.pointer.program != publication.candidate.pointer.program)
                    .map(|prior| {
                        optional(
                            &Path::new(&prior.pointer.program).join(RECORD_FILE),
                            MAX_RECORD_BYTES,
                        )
                    })
                    .transpose()?
                    .flatten();
                match publication
                    .observe(
                        pointer_bytes.as_deref(),
                        candidate_bytes.as_deref(),
                        prior_bytes.as_deref(),
                    )
                    .map_err(refusal)?
                {
                    PublicationPosition::PairPublished => {
                        Ok(Some(installed(&publication.candidate)))
                    }
                    _ if self.active.stage == Stage::Settled => {
                        bail!("setup_owner_refused: settled candidate pair changed")
                    }
                    _ => Ok(publication.prior.as_ref().map(installed)),
                }
            }
            Target::Remove { installation } => {
                let record = optional(
                    &Path::new(&installation.pointer.program).join(RECORD_FILE),
                    MAX_RECORD_BYTES,
                )?;
                ensure!(
                    pointer_bytes
                        .as_deref()
                        .is_none_or(|bytes| bytes == canonical(&installation.pointer))
                        && record
                            .as_deref()
                            .is_none_or(|bytes| bytes == canonical(&installation.record))
                        && !(pointer_bytes.is_some() && record.is_none()),
                    "removal_ownership_unproven: foreign pointer or record"
                );
                if self.active.stage == Stage::Settled {
                    ensure!(
                        pointer_bytes.is_none() && record.is_none(),
                        "removal_ownership_unproven: settled files remain"
                    );
                }
                Ok(Some(installed(installation)))
            }
        }
    }

    pub fn authorize_login(&mut self, install_id: Incarnation, action: &str) -> Result<()> {
        use airk_core::install_record::setup_recovery::active::Stage;
        match &self.active.target {
            Target::Place { publication } => {
                let candidate = self.active.stage == Stage::PairPublished
                    && publication.candidate.record.install_id == install_id
                    && matches!(action, "status" | "register")
                    && self.active.operation.options.login_startup;
                let prior = matches!(self.active.stage, Stage::Admitted | Stage::Prepared)
                    && matches!(action, "status" | "unregister")
                    && publication.prior.as_ref().is_some_and(|prior| {
                        prior.record.install_id == install_id
                            && prior.pointer.program != publication.candidate.pointer.program
                    });
                ensure!(
                    candidate || prior,
                    "login_startup_owner_unproven: registration change not authorized"
                );
            }
            Target::Remove { installation } => ensure!(
                self.active.stage == Stage::Prepared
                    && installation.record.install_id == install_id
                    && matches!(action, "status" | "unregister"),
                "login_startup_owner_unproven: removal registration not authorized"
            ),
        }
        self.existing(&files::pointer_path()?)?;
        self.revalidate()
    }

    pub fn prior_inventory(
        &self,
        root: &OpenedDirectory,
        expected: airk_core::identity::Sha256Digest,
    ) -> Result<files::PayloadInventory> {
        use std::io::Read;
        let state = self
            .pinned_root("state")?
            .context("selected state root missing")?;
        let setup = private_child(&state, "setup")?;
        let retained = private_child(&setup, "prior-inventories")?;
        let name = format!("{}.json", self.active.operation.operation_id);
        let limit = airk_core::install_record::payload_inventory::MAX_INVENTORY_BYTES;
        let bytes = if let Some(bytes) =
            airk_ipc::private_record::read_private_at(&retained, &name, limit)?
        {
            bytes
        } else {
            let file = root.open_file(OsStr::new(
                airk_core::install_record::payload_inventory::INVENTORY_FILE,
            ))?;
            let mut bytes = Vec::new();
            file.take(limit as u64 + 1).read_to_end(&mut bytes)?;
            files::PayloadInventory::decode(&bytes, expected)?;
            airk_ipc::private_record::write_private_at(&retained, &name, &bytes, limit, false)?;
            bytes
        };
        files::PayloadInventory::decode(&bytes, expected)
    }

    pub fn moved_prior(&self) -> Result<Option<(Installed, OpenedDirectory)>> {
        let Target::Place { publication } = &self.active.target else {
            return Ok(None);
        };
        let Some(prior) = publication
            .prior
            .as_ref()
            .filter(|prior| prior.pointer.program != publication.candidate.pointer.program)
        else {
            return Ok(None);
        };
        let binding = &self
            .active
            .prior_roots
            .as_ref()
            .context("prior roots missing")?
            .program;
        let RootObservation::Present { identity } = binding else {
            bail!("prior program identity missing")
        };
        let root = self
            .held
            .iter()
            .find(|root| native(root).ok().as_ref() == Some(identity))
            .context("prior program not pinned")?
            .clone();
        Ok(Some((
            Installed {
                pointer: prior.pointer.clone(),
                record: prior.record.clone(),
            },
            root,
        )))
    }

    pub fn authority(&self) -> &OpenedDirectory {
        &self.authority
    }

    pub fn candidate(&self) -> Result<&InstallationImage> {
        match &self.active.target {
            Target::Place { publication } => Ok(&publication.candidate),
            _ => bail!("setup_owner_refused: removal has no candidate"),
        }
    }

    pub fn pinned_root(&self, role: &str) -> Result<Option<OpenedDirectory>> {
        let binding = match role {
            "program" => &self.active.selected_roots.program,
            "data" => &self.active.selected_roots.data,
            "state" => &self.active.selected_roots.state,
            "cache" => &self.active.selected_roots.cache,
            "logs" => &self.active.selected_roots.logs,
            _ => bail!("setup_owner_refused: unknown retained role"),
        };
        let expected = match &binding.initial {
            RootObservation::Present { identity } => Some(identity),
            RootObservation::Absent => binding.created.as_ref().map(|created| &created.identity),
        };
        let Some(expected) = expected else {
            return Ok(None);
        };
        for pin in &self.held {
            if &native(pin)? == expected {
                return Ok(Some(pin.clone()));
            }
        }
        if role == "program"
            && matches!(self.active.target, Target::Remove { .. })
            && matches!(self.active.stage, Stage::RemovalApplied | Stage::Settled)
            && !Path::new(&self.active.operation.locations.program).try_exists()?
        {
            return Ok(None);
        }
        bail!("setup_owner_refused: retained root is not pinned")
    }

    pub fn remove_program_root(&mut self, path: &Path) -> Result<()> {
        ensure!(
            self.active.stage == Stage::RemovalApplied,
            "setup_owner_refused: program removal is not admitted"
        );
        #[cfg(unix)]
        {
            let _ = path;
            bail!(
                "program_root_preserved: no qualified atomic identity-bound Unix directory removal; empty owned directories are retained"
            )
        }
        #[cfg(windows)]
        {
            let pin = self
                .pinned_root("program")?
                .context("original program root missing")?;
            let expected = pin.identity()?;
            let parent = OpenedDirectory::open(path.parent().context("program parent missing")?)?;
            let mut retained = Vec::new();
            for held in &self.held {
                if held.identity()? != expected {
                    retained.push(held.clone());
                }
            }
            self.held = retained;
            drop(pin);
            parent.remove_child_if_matches(
                path.file_name().context("program name missing")?,
                expected,
            )?;
            Ok(())
        }
    }

    pub fn settle(&mut self, receipt: &SetupReceipt) -> Result<()> {
        let mut next = self.active.clone();
        next.sequence = next
            .sequence
            .checked_add(1)
            .context("setup owner sequence exhausted")?;
        next.stage = Stage::Settled;
        next.validate_settlement(receipt).map_err(refusal)?;
        self.revalidate()?;
        let pointer = self.folder.join("current.json");
        match &self.active.target {
            Target::Place { publication } => {
                let actual = self
                    .existing(&pointer)?
                    .context("settlement candidate missing")?;
                ensure!(
                    actual.pointer == publication.candidate.pointer
                        && actual.record == publication.candidate.record,
                    "setup_owner_refused: settlement pair changed"
                );
            }
            Target::Remove { installation } => {
                ensure!(
                    optional(&pointer, MAX_POINTER_BYTES)?.is_none()
                        && optional(
                            &Path::new(&installation.pointer.program).join(RECORD_FILE),
                            MAX_RECORD_BYTES
                        )?
                        .is_none(),
                    "setup_owner_refused: removal files remain"
                );
            }
        }
        self.update(next)
    }

    pub fn retain_before_removal(&mut self) -> Result<()> {
        if self.active.operation.kind != OperationKind::Uninstall
            || self.active.operation.options.delete_indexes == IndexDeletion::Purge
        {
            return Ok(());
        }
        self.revalidate()?;
        let Target::Remove { installation } = &self.active.target else {
            bail!("setup_owner_refused: retention requires original removal image")
        };
        let roots = snapshot(&self.active.operation.locations, &mut Vec::new())?.retained;
        let retained = RetainedInstallation {
            schema: airk_core::install_record::setup_recovery::RETAINED_SCHEMA.into(),
            operation_sha256: self.active.operation_sha256,
            operation: self.active.operation.clone(),
            installation: installation.as_ref().clone(),
            roots,
        };
        RetainedInstallation::decode(&canonical(&retained)).map_err(refusal)?;
        let directory = private_child(&self.authority, "retained")?;
        airk_ipc::endpoint::require_private_directory_handle(directory.as_file())?;
        airk_ipc::private_record::write_private_at(
            &directory,
            &format!("{}.json", installation.record.install_id),
            &canonical(&retained),
            MAX_RECOVERY_BYTES,
            true,
        )?;
        directory.sync_all()?;
        self.authority.sync_all()?;
        Ok(())
    }

    pub fn reopen_failed(&mut self, receipt: &mut SetupReceipt) -> Result<()> {
        ensure!(
            receipt.closed
                && receipt.failure.is_some()
                && receipt.operation_sha256 == self.active.operation_sha256
                && receipt.operation_id == self.active.operation.operation_id,
            "setup_owner_refused: failure belongs to another operation"
        );
        self.revalidate()?;
        let bytes = canonical(receipt);
        let folder = Path::new(&self.active.operation.locations.state)
            .join("setup")
            .join("failures");
        let state = self
            .pinned_root("state")?
            .context("setup state root missing")?;
        let setup = private_child(&state, "setup")?;
        let pin = private_child(&setup, "failures")?;
        airk_ipc::endpoint::require_private_directory_handle(pin.as_file())?;
        let name = format!(
            "{}-{}.json",
            receipt.operation_id,
            files::digest_bytes(&bytes)
        );
        if let Some(previous) = optional(&folder.join(&name), MAX_RECOVERY_BYTES)? {
            ensure!(
                previous == bytes,
                "setup_owner_refused: failure archive changed"
            );
        } else {
            airk_ipc::private_record::write_private_at(
                &pin,
                &name,
                &bytes,
                MAX_RECOVERY_BYTES,
                false,
            )?;
            pin.sync_all()?;
        }
        if receipt
            .steps
            .last()
            .is_some_and(|step| step.phase == airk_core::install_record::SetupPhase::Complete)
        {
            receipt.steps.pop();
        }
        ensure!(
            !receipt
                .steps
                .iter()
                .any(|step| step.phase == airk_core::install_record::SetupPhase::Complete),
            "setup_owner_refused: invalid terminal failure history"
        );
        receipt
            .steps
            .retain(|step| step.phase != airk_core::install_record::SetupPhase::LoginFailed);
        receipt.closed = false;
        receipt.failure = None;
        files::write_receipt(&state, receipt)
    }

    pub fn ensure_roots(&mut self) -> Result<()> {
        if self.active.stage != Stage::Admitted {
            return self.revalidate();
        }
        let locations = self.active.operation.locations.clone();
        for (role, path) in plan::roles(&locations) {
            if plan::removes(self.active.operation.kind) {
                break;
            }
            let binding = match role {
                "program" => &self.active.selected_roots.program,
                "data" => &self.active.selected_roots.data,
                "state" => &self.active.selected_roots.state,
                "cache" => &self.active.selected_roots.cache,
                "logs" => &self.active.selected_roots.logs,
                _ => unreachable!(),
            };
            if matches!(binding.initial, RootObservation::Present { .. }) {
                continue;
            }
            if path.try_exists()? {
                ensure!(
                    binding.created.is_some(),
                    "setup_owner_refused: an unbound root appeared"
                );
                continue;
            }
            let parent_path = path.parent().context("root parent missing")?;
            prepare_parents(parent_path)?;
            let parent = OpenedDirectory::open(parent_path)?;
            airk_ipc::endpoint::require_protected_parent_handle(parent.as_file())?;
            let created = if let Some(created) = &binding.created {
                created.clone()
            } else {
                use ring::rand::SecureRandom;
                let mut bytes = [0; 16];
                ring::rand::SystemRandom::new()
                    .fill(&mut bytes)
                    .map_err(|_| anyhow::anyhow!("root staging nonce unavailable"))?;
                let nonce = Incarnation::from_bytes(bytes);
                let staged =
                    parent_path.join(staging(self.active.operation.operation_id, role, nonce));
                let pin = airk_ipc::endpoint::create_root_at(
                    &parent,
                    staged.file_name().context("staging name")?,
                )?;
                airk_ipc::endpoint::require_private_directory_handle(pin.as_file())?;
                let created = CreatedRoot {
                    identity: native(&pin)?,
                    parent: native(&parent)?,
                    nonce,
                };
                let mut next = self.active.clone();
                next.sequence = next
                    .sequence
                    .checked_add(1)
                    .context("setup owner sequence exhausted")?;
                let selected = match role {
                    "program" => &mut next.selected_roots.program,
                    "data" => &mut next.selected_roots.data,
                    "state" => &mut next.selected_roots.state,
                    "cache" => &mut next.selected_roots.cache,
                    "logs" => &mut next.selected_roots.logs,
                    _ => unreachable!(),
                };
                selected.created = Some(created.clone());
                self.update(next)?;
                created
            };
            ensure!(
                native(&parent)? == created.parent,
                "setup_owner_refused: staging parent changed"
            );
            let name = staging(self.active.operation.operation_id, role, created.nonce);
            {
                let staged = parent.open_child(OsStr::new(&name))?;
                ensure!(
                    native(&staged)? == created.identity,
                    "setup_owner_refused: staging root changed"
                );
            }
            airk_host::install_layout::publish_new_at(
                parent.as_file(),
                parent_path,
                OsStr::new(&name),
                path.file_name().context("root has no name")?,
            )?;
            parent.sync_all()?;
        }
        self.revalidate()
    }
}

pub(super) fn retained_for(
    pointer: &Path,
    locations: &Locations,
) -> Result<Option<RetainedInstallation>> {
    let folder = pointer.parent().context("pointer parent")?.join("retained");
    if !folder.try_exists()? {
        return Ok(None);
    }
    let held = OpenedDirectory::open(&folder)?;
    airk_ipc::endpoint::require_private_directory_handle(held.as_file())?;
    let mut found = None;
    let mut budget = 128;
    for entry in std::fs::read_dir(&folder)? {
        ensure!(
            budget > 0,
            "retained_ownership_unproven: too many retained records"
        );
        budget -= 1;
        let entry = entry?;
        let bytes = files::read_bounded(&entry.path(), MAX_RECOVERY_BYTES, true)?;
        let retained = RetainedInstallation::decode(&bytes).map_err(refusal)?;
        ensure!(
            entry.file_name()
                == OsStr::new(&format!("{}.json", retained.installation.record.install_id)),
            "retained_ownership_unproven: wrong retained file name"
        );
        let old = &retained.installation.record.locations;
        if old.data != locations.data
            || old.state != locations.state
            || old.cache != locations.cache
            || old.logs != locations.logs
        {
            continue;
        }
        let mut roots = Vec::new();
        let observed = snapshot(locations, &mut roots)?.retained;
        ensure!(
            observed == retained.roots,
            "retained_ownership_unproven: retained roots changed"
        );
        let receipt = files::read_receipt(Path::new(&old.state), &retained.operation.operation_id)?
            .context("retained_ownership_unproven: completion receipt missing")?;
        retained.validate_completion(&receipt).map_err(refusal)?;
        ensure!(
            found.is_none(),
            "retained_ownership_unproven: ambiguous retained installations"
        );
        found = Some(retained);
    }
    Ok(found)
}

fn staging(operation: Incarnation, role: &str, nonce: Incarnation) -> String {
    format!(".airk-setup-{operation}-{role}-{nonce}")
}

pub(super) fn approved_inventory(
    pointer: &Path,
    installed: &Installed,
) -> Result<Option<airk_core::identity::Sha256Digest>> {
    airk_ipc::install_reader::completed_inventory(pointer, &image(installed))
}

fn private_child(parent: &OpenedDirectory, name: &str) -> Result<OpenedDirectory> {
    let child = match parent.open_child(OsStr::new(name)) {
        Ok(child) => child,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            airk_ipc::endpoint::create_root_at(parent, OsStr::new(name))?
        }
        Err(error) => return Err(error.into()),
    };
    airk_ipc::endpoint::require_private_directory_handle(child.as_file())?;
    Ok(child)
}

fn create_private(path: &Path) -> Result<()> {
    let parent = OpenedDirectory::open(path.parent().context("private root parent missing")?)?;
    airk_ipc::endpoint::create_root_at(
        &parent,
        path.file_name().context("private root name missing")?,
    )?;
    Ok(())
}

fn prepare_parents(path: &Path) -> Result<()> {
    if path.try_exists()? {
        files::safe_parent(path)?;
        return Ok(());
    }
    prepare_parents(path.parent().context("root parent missing")?)?;
    create_private(path)
}

#[cfg(test)]
mod tests;

pub(super) fn ordinary_management_allowed(pointer: &Path) -> Result<()> {
    let folder = pointer.parent().context("pointer parent")?;
    let Some(bytes) = optional(&folder.join(ACTIVE_FILE), MAX_RECOVERY_BYTES)? else {
        return Ok(());
    };
    let active: ActiveSetup = serde_json::from_slice(&bytes)?;
    active.validate().map_err(refusal)?;
    ensure!(
        canonical(&active) == bytes && active.stage == Stage::Settled,
        "installation_in_progress: unsettled setup owns OS management"
    );
    let authority = OpenedDirectory::open(folder)?;
    let mut owner = Owner {
        active,
        authority,
        folder: folder.to_owned(),
        held: Vec::new(),
    };
    owner.revalidate()?;
    let receipt = files::read_receipt(
        Path::new(&owner.active.operation.locations.state),
        &owner.active.operation.operation_id,
    )?
    .context("settled setup receipt missing")?;
    owner
        .active
        .validate_settlement(&receipt)
        .map_err(refusal)?;
    owner.existing(pointer)?;
    Ok(())
}
