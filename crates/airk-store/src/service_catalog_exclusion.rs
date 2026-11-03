// SPDX-License-Identifier: Apache-2.0
//! Exclusion proof for pending activation, without opening or recovering SQLite.
use super::*;

pub(super) fn worker_leases(
    root: &Path,
    known: impl IntoIterator<Item = Sha256Digest>,
) -> Result<Vec<ProjectLease>> {
    let mut bindings = known.into_iter().collect::<BTreeSet<_>>();
    for (number, entry) in fs::read_dir(root)?.enumerate() {
        ensure!(
            number < MAX_DIRECTORY_ENTRIES,
            "catalog ownership census exceeds bound"
        );
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if let Some(suffix) = name.strip_prefix("worker-1.4-") {
            bindings.insert(
                suffix
                    .strip_suffix(".lock")
                    .context("unrecognized worker ownership entry")?
                    .parse()?,
            );
        }
    }
    ensure!(
        bindings.len() <= MAX_WORKER_LOCKS,
        "catalog worker ownership census exceeds bound"
    );
    bindings
        .into_iter()
        .map(|binding| ProjectLease::acquire(root, binding))
        .collect()
}

/// Lets the matching installer finish owner drain while an interrupted catalog
/// has a hot journal. It proves only process exclusion, never database readiness,
/// compatibility or permission to restore/start a predecessor.
pub struct PendingCatalogExclusion {
    root: PathBuf,
    namespace: Incarnation,
    upgrade: Incarnation,
    installer_identity: Sha256Digest,
    _workers: Vec<ProjectLease>,
    _watcher: SourceWatchLease,
    _embedding: EmbeddingLease,
    _service: ServiceLease,
}
impl PendingCatalogExclusion {
    pub fn acquire(root: &Path) -> Result<Self> {
        let root = root.canonicalize()?;
        super::super::super::data_relocation::require_settled(&root)?;
        let owner =
            super::super::super::owner_record(&root, true, Incarnation::from_bytes([0; 16]))?;
        let id = owner
            .catalog_upgrade
            .context("catalog admission is not fenced")?;
        ensure!(
            owner.ready && owner.format == 2,
            "catalog exclusion requires an owned initialized registry"
        );
        let receipt = pending(&root)?.context("catalog exclusion intent missing")?;
        ensure!(
            receipt.plan.id == id
                && receipt.plan.namespace == owner.incarnation
                && !matches!(
                    receipt.stage,
                    UpgradeStage::Settled | UpgradeStage::Restored
                ),
            "catalog exclusion intent does not match its pending owner"
        );
        let service = ServiceLease::acquire(&root)?;
        let embedding = EmbeddingLease::acquire(&root)?;
        let watcher = SourceWatchLease::acquire(&root)?;
        let workers = worker_leases(
            &root,
            receipt
                .plan
                .indexes
                .iter()
                .map(|index| index.registration.binding),
        )?;
        let current = super::super::super::owner_record(&root, true, owner.incarnation)?;
        ensure!(
            current.incarnation == owner.incarnation
                && current.ready
                && current.format == 2
                && current.catalog_upgrade == Some(id)
                && pending(&root)?
                    .is_some_and(|current| current.plan_digest == receipt.plan_digest
                        && current.stage == receipt.stage),
            "catalog exclusion fence changed"
        );
        Ok(Self {
            root,
            namespace: owner.incarnation,
            upgrade: id,
            installer_identity: receipt.plan.installer_identity,
            _workers: workers,
            _watcher: watcher,
            _embedding: embedding,
            _service: service,
        })
    }
    pub fn control(&self) -> &Path {
        &self.root
    }
    pub fn namespace(&self) -> Incarnation {
        self.namespace
    }
    pub fn upgrade_id(&self) -> Incarnation {
        self.upgrade
    }
    pub fn installer_identity(&self) -> Sha256Digest {
        self.installer_identity
    }

    /// Keep service ownership across the transition to database recovery. The
    /// adapter reacquires the subordinate leases before opening SQLite; it also
    /// rechecks the saved identity in case the pending intent changed.
    pub fn into_upgrade(self, installer: Sha256Digest) -> Result<CatalogUpgrade> {
        ensure!(
            self.installer_identity == installer,
            "catalog upgrade belongs to another verified installer payload"
        );
        let Self {
            _workers: workers,
            _watcher: watcher,
            _embedding: embedding,
            _service: service,
            ..
        } = self;
        drop(workers);
        drop(watcher);
        drop(embedding);
        CatalogUpgrade::acquire_with_identity(service, Some(installer), true)
    }
}
