// SPDX-License-Identifier: Apache-2.0
//! Private evaluation only: profiles that join the evaluation catalog without
//! entering the installation's asset store.
//!
//! The private fixture authenticates each original asset (length and SHA-256)
//! once and keeps its read descriptor. The store is never written, weights are
//! never copied, and a registration is bound to the exact service lease that
//! owns the evaluation, so another lease or a product owner never sees it.
use super::*;
use airk_retrieval::arbiter::runtime::AssetSource;
use std::{
    collections::BTreeMap,
    fs::File,
    io::{Read, Seek, SeekFrom},
    sync::{Mutex, Weak},
};

#[derive(Clone)]
pub(in crate::service_runtime) struct CatalogOnlyAssets(
    Arc<Mutex<BTreeMap<String, (Asset, File)>>>,
);

type Registry = BTreeMap<
    airk_core::identity::Sha256Digest,
    (Weak<airk_store::service::ServiceLease>, CatalogOnlyAssets),
>;

static REGISTRY: Mutex<Registry> = Mutex::new(BTreeMap::new());

/// Drop registrations whose lease has ended, closing their held descriptors
/// (an open handle would block deleting a disposable installation on Windows).
fn prune(registry: &mut Registry) {
    registry.retain(|_, (owner, _)| owner.strong_count() > 0);
}

#[cfg(test)]
fn authenticated(asset: &Asset, file: &mut File) -> Result<()> {
    use sha2::{Digest, Sha256};
    ensure!(
        file.metadata()?.is_file() && file.metadata()?.len() == asset.bytes,
        "catalog-only asset length differs"
    );
    file.seek(SeekFrom::Start(0))?;
    let mut digest = Sha256::new();
    let mut buffer = vec![0_u8; 1 << 20];
    let mut remaining = asset.bytes;
    while remaining > 0 {
        let limit = usize::try_from(remaining.min(buffer.len() as u64))?;
        let read = file.read(&mut buffer[..limit])?;
        ensure!(read > 0, "catalog-only asset shortened");
        digest.update(&buffer[..read]);
        remaining -= u64::try_from(read)?;
    }
    ensure!(
        file.read(&mut [0])? == 0
            && airk_core::identity::Sha256Digest::from_bytes(digest.finalize().into())
                == asset.sha256,
        "catalog-only asset bytes differ from the profile"
    );
    Ok(())
}

/// Authenticate and hold every asset of `profile` for `lease`. One profile is
/// registered once per lease; a different lease replaces a dead registration.
/// Only the ignored private fixture registers.
#[cfg(test)]
pub(in crate::service_runtime) fn register(
    lease: &Arc<airk_store::service::ServiceLease>,
    profile: &Profile,
    mut source: impl FnMut(&Asset) -> Result<File>,
) -> Result<()> {
    profile.validate()?;
    ensure!(
        profile.qualification.is_none()
            && profile.downloads.is_empty()
            && profile.profile_id.starts_with("private-")
            && profile.device_execution.is_none(),
        "catalog-only assets are for unqualified private CPU evaluation profiles"
    );
    let mut files = BTreeMap::new();
    for asset in &profile.assets {
        let mut file = source(asset)?;
        authenticated(asset, &mut file)?;
        files.insert(asset.name.clone(), (asset.clone(), file));
    }
    let mut registry = REGISTRY
        .lock()
        .map_err(|_| anyhow::anyhow!("catalog-only registry unavailable"))?;
    prune(&mut registry);
    ensure!(
        !registry.contains_key(&profile.manifest),
        "catalog-only profile is already held for a live lease"
    );
    registry.insert(
        profile.manifest,
        (
            Arc::downgrade(lease),
            CatalogOnlyAssets(Arc::new(Mutex::new(files))),
        ),
    );
    Ok(())
}

/// The held descriptors for `manifest`, only for the lease that registered them.
pub(in crate::service_runtime) fn lookup(
    manifest: airk_core::identity::Sha256Digest,
    lease: &Arc<airk_store::service::ServiceLease>,
) -> Result<Option<CatalogOnlyAssets>> {
    let mut registry = REGISTRY
        .lock()
        .map_err(|_| anyhow::anyhow!("catalog-only registry unavailable"))?;
    prune(&mut registry);
    Ok(registry.get(&manifest).and_then(|(owner, assets)| {
        owner
            .upgrade()
            .filter(|owner| Arc::ptr_eq(owner, lease))
            .map(|_| assets.clone())
    }))
}

/// Whether an evaluation stage under `lease` holds authenticated catalog-only
/// descriptors for `manifest`: the availability and warm gates may treat
/// exactly this as provisioned, in place of a store receipt. A product
/// (non-evaluation) stage, another lease or an unregistered manifest never does.
pub(in crate::service_runtime) fn held(
    evaluation: bool,
    manifest: airk_core::identity::Sha256Digest,
    lease: &Arc<airk_store::service::ServiceLease>,
) -> Result<bool> {
    Ok(evaluation && lookup(manifest, lease)?.is_some())
}

impl AssetSource for CatalogOnlyAssets {
    fn read_asset(&self, asset: &Asset) -> Result<Vec<u8>> {
        let mut files = self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("catalog-only asset descriptors unavailable"))?;
        let (declared, file) = files
            .get_mut(&asset.name)
            .context("catalog-only asset missing")?;
        ensure!(declared == asset, "catalog-only asset declaration changed");
        let length = usize::try_from(asset.bytes)?;
        file.seek(SeekFrom::Start(0))?;
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(length)?;
        file.take(asset.bytes.saturating_add(1))
            .read_to_end(&mut bytes)?;
        ensure!(bytes.len() == length, "catalog-only asset length changed");
        // The Scorer independently checks the exact SHA-256 it consumes.
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::identity::Sha256Digest;
    use sha2::{Digest, Sha256};

    fn asset(name: &str, bytes: &[u8]) -> Asset {
        Asset {
            name: name.into(),
            role: AssetRole::Notice,
            bytes: bytes.len() as u64,
            sha256: Sha256Digest::from_bytes(Sha256::digest(bytes).into()),
        }
    }

    #[test]
    fn descriptors_are_authenticated_and_bound_to_the_registering_lease() -> Result<()> {
        let root = tempfile::tempdir()?;
        let path = root.path().join("weights.bin");
        std::fs::write(&path, b"original weights")?;
        let declared = asset("weights.bin", b"original weights");
        let mut file = File::open(&path)?;
        authenticated(&declared, &mut file)?;
        let wrong = asset("weights.bin", b"different weights");
        assert!(authenticated(&wrong, &mut file).is_err());
        let held = CatalogOnlyAssets(Arc::new(Mutex::new(BTreeMap::from([(
            declared.name.clone(),
            (declared.clone(), File::open(&path)?),
        )]))));
        assert_eq!(held.read_asset(&declared)?, b"original weights");
        assert!(held.read_asset(&wrong).is_err());
        let lease = Arc::new(airk_store::service::ServiceLease::acquire(root.path())?);
        let manifest = Sha256Digest::from_bytes([77; 32]);
        REGISTRY
            .lock()
            .unwrap()
            .insert(manifest, (Arc::downgrade(&lease), held));
        assert!(lookup(manifest, &lease)?.is_some());
        assert!(lookup(Sha256Digest::from_bytes([78; 32]), &lease)?.is_none());
        let other_root = tempfile::tempdir()?;
        let other = Arc::new(airk_store::service::ServiceLease::acquire(other_root.path())?);
        assert!(lookup(manifest, &other)?.is_none());
        drop(lease);
        assert!(lookup(manifest, &other)?.is_none());
        REGISTRY.lock().unwrap().remove(&manifest);
        Ok(())
    }

    fn hold(
        manifest: Sha256Digest,
        lease: &Arc<airk_store::service::ServiceLease>,
        path: &std::path::Path,
    ) -> Result<()> {
        let declared = asset("weights.bin", &std::fs::read(path)?);
        REGISTRY.lock().unwrap().insert(
            manifest,
            (
                Arc::downgrade(lease),
                CatalogOnlyAssets(Arc::new(Mutex::new(BTreeMap::from([(
                    declared.name.clone(),
                    (declared, File::open(path)?),
                )])))),
            ),
        );
        Ok(())
    }

    #[test]
    fn registrations_of_ended_leases_are_pruned_and_release_their_files() -> Result<()> {
        let root = tempfile::tempdir()?;
        let path = root.path().join("weights.bin");
        std::fs::write(&path, b"pruned weights")?;
        let manifest = Sha256Digest::from_bytes([80; 32]);
        let lease_root = tempfile::tempdir()?;
        let lease = Arc::new(airk_store::service::ServiceLease::acquire(lease_root.path())?);
        hold(manifest, &lease, &path)?;
        assert!(REGISTRY.lock().unwrap().contains_key(&manifest));
        drop(lease);
        // Any lookup prunes the dead registration and closes its descriptor.
        let other_root = tempfile::tempdir()?;
        let other = Arc::new(airk_store::service::ServiceLease::acquire(other_root.path())?);
        assert!(lookup(Sha256Digest::from_bytes([81; 32]), &other)?.is_none());
        assert!(!REGISTRY.lock().unwrap().contains_key(&manifest));
        std::fs::remove_file(&path)?;
        Ok(())
    }

    fn focus_profile(manifest: Sha256Digest) -> Result<Profile> {
        use airk_core::arbiter::{ModelChoice, Stage, Task};
        let mut profile: Profile = serde_json::from_slice(include_bytes!(
            "../../../../../fixtures/arbiter/profile-downloads.json"
        ))?;
        let policy = airk_retrieval::arbiter::compiler::focus::policy_digest()?;
        profile.model = ModelChoice::FocusQwenReranker06B;
        profile.stage = Some(Stage::Focus);
        profile.tasks = std::collections::BTreeSet::from([Task::FocusRank]);
        profile.task_policies = BTreeMap::from([(Task::FocusRank, policy)]);
        profile.policy = policy;
        profile.abi = serde_json::from_value(serde_json::json!({
            "readout":"focus_yes_no",
            "inputs":[{"name":"input_ids","element":"int64","axes":["batch","tokens"]},
                {"name":"attention_mask","element":"int64","axes":["batch","tokens"]},
                {"name":"position_ids","element":"int64","axes":["batch","tokens"]}],
            "output":{"name":"logits","element":"float32","axes":["batch","labels"]}}))?;
        profile.max_tokens = 512;
        profile.max_candidates = 4;
        profile.profile_id = "private-catalog-only-status".into();
        profile.manifest = manifest;
        profile.qualification = None;
        profile.qualified_targets.clear();
        profile.downloads.clear();
        profile.cpu_fit = None;
        profile.validate()?;
        Ok(profile)
    }

    /// The lifecycle status row (`StageOwner::status`) reports a held
    /// catalog-only profile as `requested` instead of `missing_assets`. The
    /// configure and restart gates call the same `catalog_only_held`; they
    /// need a live supervisor and are proven by the warm smoke.
    #[test]
    fn lifecycle_status_reports_held_catalog_only_assets_as_requested() -> Result<()> {
        use airk_catalog_control::catalog_admission::NativeCatalogAdmission;
        use airk_core::{identity::Incarnation, index_catalog::CatalogRevision};
        let root = tempfile::tempdir()?;
        let path = root.path().join("weights.bin");
        std::fs::write(&path, b"status weights")?;
        let lease_root = tempfile::tempdir()?;
        let lease = Arc::new(airk_store::service::ServiceLease::acquire(lease_root.path())?);
        let gate = NativeCatalogAdmission::new(
            Incarnation::from_bytes([93; 16]),
            Incarnation::from_bytes([94; 16]),
            CatalogRevision::INITIAL,
            1,
        )?;
        let manifest = Sha256Digest::from_bytes([82; 32]);
        let mut owner = super::super::Owner::new(lease.clone(), gate)?;
        owner.focus.evaluation = Some(airk_kernel::inference_owner::evaluation_catalog(
            lease.clone(),
            vec![focus_profile(manifest)?],
        )?);
        let row = |owner: &super::super::Owner| -> Result<(Availability, String)> {
            let status = owner.focus.status()?;
            let row = status
                .profiles
                .iter()
                .find(|row| row.profile == Some(manifest))
                .context("catalog profile row missing")?;
            Ok((row.availability, row.reason.clone()))
        };
        let (before, _) = row(&owner)?;
        assert_eq!(before, Availability::MissingAssets);
        hold(manifest, &lease, &path)?;
        let (after, reason) = row(&owner)?;
        assert_eq!(after, Availability::Requested, "{reason}");
        assert!(reason.contains("catalog-only assets are held"), "{reason}");
        REGISTRY.lock().unwrap().remove(&manifest);
        Ok(())
    }

    /// Regression for the change-set-1 blocker: a registered catalog-only
    /// extra must count as provisioned for the evaluation stage that owns its
    /// lease (the store holds no receipt for it), and for nothing else.
    #[test]
    fn registered_descriptors_satisfy_the_evaluation_stage_availability_gate() -> Result<()> {
        use airk_catalog_control::catalog_admission::NativeCatalogAdmission;
        use airk_core::{identity::Incarnation, index_catalog::CatalogRevision};
        let root = tempfile::tempdir()?;
        let path = root.path().join("weights.bin");
        std::fs::write(&path, b"held weights")?;
        let declared = asset("weights.bin", b"held weights");
        let lease = Arc::new(airk_store::service::ServiceLease::acquire(root.path())?);
        let gate = NativeCatalogAdmission::new(
            Incarnation::from_bytes([91; 16]),
            Incarnation::from_bytes([92; 16]),
            CatalogRevision::INITIAL,
            1,
        )?;
        let owner = super::super::Owner::new(lease.clone(), gate)?;
        let manifest = Sha256Digest::from_bytes([79; 32]);
        assert!(!owner.focus.catalog_only_held(manifest)?);
        assert!(!held(true, manifest, &lease)?);
        REGISTRY.lock().unwrap().insert(
            manifest,
            (
                Arc::downgrade(&lease),
                CatalogOnlyAssets(Arc::new(Mutex::new(BTreeMap::from([(
                    declared.name.clone(),
                    (declared.clone(), File::open(&path)?),
                )])))),
            ),
        );
        assert!(held(true, manifest, &lease)?);
        // A stage without an evaluation catalog (product) never qualifies.
        assert!(!owner.focus.catalog_only_held(manifest)?);
        assert!(!held(false, manifest, &lease)?);
        let other_root = tempfile::tempdir()?;
        let other = Arc::new(airk_store::service::ServiceLease::acquire(other_root.path())?);
        assert!(!held(true, manifest, &other)?);
        REGISTRY.lock().unwrap().remove(&manifest);
        assert!(!held(true, manifest, &lease)?);
        Ok(())
    }
}
