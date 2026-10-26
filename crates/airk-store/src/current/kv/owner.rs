// SPDX-License-Identifier: Apache-2.0
//! Native ownership, crash-surviving admission and shared worker database handles.
use super::{Database, FORMAT, Snapshot};
use crate::{
    FileStore, WriterLock,
    current::{CurrentHead, CurrentScope, gate::Gate},
    quota::{CurrentGrantScope, Quota},
};
use airk_core::identity::framed_digest;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    ops::Deref,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock, Weak},
};

const OWNER: &str = "current-owner.json";
const BOOTSTRAP: &str = "current-bootstrap.json";
const DATABASE: &str = "current.airk";
const SLACK: u64 = 65536;
const MAX_SLOTS: usize = 1024;
#[derive(Clone, Default, Debug)]
pub(crate) struct Cache(Arc<Mutex<Option<Arc<Slot>>>>);
pub(crate) struct Slot {
    path: PathBuf,
    database: Mutex<Option<Cached>>,
}
struct Cached {
    database: Arc<Database>,
    scope: CurrentScope,
}
impl std::fmt::Debug for Slot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CurrentDatabase")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}
type Registry = BTreeMap<PathBuf, Weak<Slot>>;
static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();
fn slot(store: &FileStore) -> Result<Arc<Slot>> {
    crate::managed_collection::validate_ancestors(store.directory())?;
    let path = store.directory().canonicalize()?;
    let mut cached = store
        .current_engine
        .0
        .lock()
        .map_err(|_| anyhow::anyhow!("current handle cache poisoned"))?;
    if let Some(slot) = &*cached {
        ensure!(slot.path == path, "current handle path changed");
        return Ok(slot.clone());
    }
    let mut registry = REGISTRY
        .get_or_init(Default::default)
        .lock()
        .map_err(|_| anyhow::anyhow!("current handle registry poisoned"))?;
    registry.retain(|_, value| value.strong_count() > 0);
    let entry = if let Some(entry) = registry.get(&path).and_then(Weak::upgrade) {
        entry
    } else {
        ensure!(
            registry.len() < MAX_SLOTS,
            "current handle registry capacity exceeded"
        );
        let entry = Arc::new(Slot {
            path: path.clone(),
            database: Mutex::new(None),
        });
        registry.insert(path, Arc::downgrade(&entry));
        entry
    };
    *cached = Some(entry.clone());
    Ok(entry)
}
pub(crate) fn release_idle() {
    let Some(registry) = REGISTRY.get() else {
        return;
    };
    let Ok(registry) = registry.lock() else {
        return;
    };
    let slots: Vec<_> = registry.values().filter_map(Weak::upgrade).collect();
    drop(registry);
    for slot in slots {
        if let Ok(mut cached) = slot.database.lock()
            && cached.as_ref().is_some_and(|entry| {
                Arc::strong_count(&entry.database) == 1 && entry.database.idle()
            })
        {
            cached.take();
        }
    }
}
/// The caller owns exclusive root lifecycle exclusion. Read the same cached
/// engine when present; opening a second native handle could wait on its lock.
pub(crate) fn frozen_snapshot<T>(
    store: &FileStore,
    scope: &CurrentScope,
    inspect: impl FnOnce(&redb::ReadTransaction) -> Result<T>,
) -> Result<T> {
    let owner = header(store, scope)?;
    ensure!(
        receipt(store, &owner)?.complete,
        "current initialization is incomplete"
    );
    let slot = slot(store)?;
    let cached = slot
        .database
        .lock()
        .map_err(|_| anyhow::anyhow!("current database cache poisoned"))?;
    if let Some(entry) = &*cached {
        ensure!(
            entry.scope == *scope && entry.database.capacity() == owner.capacity,
            "frozen current ownership changed"
        );
        return inspect(entry.database.snapshot()?.transaction());
    }
    use redb::ReadableDatabase;
    let path = store.directory().join(DATABASE);
    let metadata = fs::symlink_metadata(&path)?;
    crate::managed_collection::reject_file_aliases(&path, &metadata)?;
    ensure!(
        metadata.len() > 0 && metadata.len() <= owner.capacity,
        "frozen current file exceeds ownership capacity"
    );
    // A fresh writable open changes redb recovery headers, invalidating a prior
    // relocation inventory. Under lifecycle exclusion with no cached writer,
    // use a read-only native open; it cannot extend or rewrite the file.
    let mut builder = redb::Builder::new();
    builder.set_cache_size(1 << 20);
    let database = builder.open_read_only(&path)?;
    inspect(&database.begin_read()?)
}
pub struct Engine {
    database: Arc<Database>,
    _slot: Arc<Slot>,
}
impl Deref for Engine {
    type Target = Database;
    fn deref(&self) -> &Database {
        &self.database
    }
}
pub struct OwnedSnapshot {
    snapshot: Snapshot,
    _slot: Arc<Slot>,
    _gate: Gate,
}
impl OwnedSnapshot {
    pub fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Owner {
    version: u32,
    scope: CurrentScope,
    capacity: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    replay: Option<crate::current::database::ReplayAdmission>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Bootstrap {
    version: u32,
    head: CurrentHead,
    capacity: u64,
    complete: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Stage {
    Receipt,
    Owner,
    File,
    Schema,
    Head,
    Complete,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GrowthStage {
    BeforeOwner,
    AfterOwner,
}
fn read<T: serde::de::DeserializeOwned + Serialize>(
    path: &Path,
    maximum: u64,
) -> Result<Option<T>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    crate::managed_collection::reject_file_aliases(path, &metadata)?;
    ensure!(
        metadata.len() <= maximum,
        "oversized redb ownership receipt"
    );
    let mut bytes = Vec::new();
    File::open(path)?
        .take(maximum + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= maximum,
        "redb ownership receipt exceeds bound"
    );
    let value: T = serde_json::from_slice(&bytes)?;
    ensure!(
        serde_json::to_vec(&value)? == bytes,
        "noncanonical redb ownership receipt"
    );
    Ok(Some(value))
}
fn persist<T: Serialize>(path: &Path, value: &T, maximum: usize) -> Result<()> {
    let bytes = serde_json::to_vec(value)?;
    ensure!(
        bytes.len() <= maximum,
        "redb ownership receipt exceeds bound"
    );
    let mut file =
        tempfile::NamedTempFile::new_in(path.parent().context("receipt parent missing")?)?;
    file.write_all(&bytes)?;
    crate::durability::persist_synced(file, path).context("replace native ownership receipt")
}
fn validate(store: &FileStore, scope: &CurrentScope, capacity: u64) -> Result<()> {
    scope.validate()?;
    crate::current::database::validate_capacity(capacity)?;
    ensure!(
        store.collection_id()? == scope.collection,
        "foreign redb collection path"
    );
    if let Some(incarnation) = store.service_incarnation()? {
        ensure!(
            incarnation == scope.incarnation,
            "foreign redb incarnation path"
        );
    }
    crate::managed_collection::validate_ancestors(store.directory())?;
    Ok(())
}
pub(super) fn grant_scope(scope: &CurrentScope) -> CurrentGrantScope {
    CurrentGrantScope {
        collection: scope.collection.clone(),
        incarnation: scope.incarnation,
        transaction: framed_digest(
            b"current-redb-capacity/1",
            &[
                scope.root.as_bytes(),
                scope.collection.as_bytes(),
                scope.incarnation.as_bytes(),
            ],
        ),
    }
}
/// Metadata-only quota backing proof. Never opens the database or takes its
/// engine mutex while the quota ledger is locked.
pub(crate) fn accounting_scope(store: &FileStore) -> Result<Option<CurrentGrantScope>> {
    let Some(owner): Option<Owner> = read(&store.directory().join(OWNER), 65536)? else {
        return Ok(None);
    };
    if owner.version != FORMAT {
        return Ok(None);
    }
    validate(store, &owner.scope, owner.capacity)?;
    receipt(store, &owner)?;
    Ok(Some(grant_scope(&owner.scope)))
}
fn allowance(capacity: u64) -> Result<u64> {
    capacity
        .checked_add(SLACK)
        .context("redb capacity allowance overflow")
}
fn admit_growth(
    store: &FileStore,
    writer: &WriterLock,
    quota: &Quota,
    scope: &CurrentScope,
    end: u64,
    current: u64,
) -> Result<u64> {
    crate::maintenance::check_writer(store, writer, quota)?;
    let owner_path = store.directory().join(OWNER);
    let mut owner = header(store, scope)?;
    ensure!(
        owner.capacity >= current,
        "redb owner capacity trails admitted backend"
    );
    let maximum = crate::current::database::MAX_DATABASE_BYTES;
    ensure!(
        end <= maximum,
        "redb database exceeds maximum supported size"
    );
    const GRANULE: u64 = 1 << 20;
    let rounded = end
        .checked_add(GRANULE - 1)
        .map(|value| value / GRANULE * GRANULE)
        .unwrap_or(maximum)
        .min(maximum);
    let next = owner.capacity.max(rounded).max(end);
    crate::current::database::validate_capacity(next)?;
    crate::capacity::require_growth(quota.root(), end.saturating_sub(current))?;
    let minimum = allowance(next)?;
    let mut grant = quota
        .current_grant(writer, &grant_scope(scope))?
        .context("redb capacity grant missing during automatic growth")?;
    if grant.capacity() < minimum {
        grant.resize(writer, minimum)?;
    }
    if owner.capacity < next {
        owner.capacity = next;
        persist(&owner_path, &owner, 65536)?;
    }
    Ok(next)
}
fn owned_file(path: &Path, capacity: u64, create: bool) -> Result<File> {
    if !create {
        let metadata = fs::symlink_metadata(path)?;
        crate::managed_collection::reject_file_aliases(path, &metadata)?;
        ensure!(
            metadata.len() <= capacity,
            "redb database exceeds owned capacity"
        );
    }
    let mut options = OpenOptions::new();
    options.read(true).write(true).create_new(create);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x00200000);
    }
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file() && metadata.len() <= capacity,
        "invalid redb database handle"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        ensure!(metadata.nlink() == 1, "aliased redb database handle");
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure!(
            metadata.file_attributes() & 0x400 == 0,
            "redirected redb database handle"
        );
    }
    crate::managed_collection::reject_file_aliases(path, &metadata)?;
    if create {
        crate::maintenance::sync_parent(path.parent().context("database parent missing")?)?;
    }
    Ok(file)
}
fn header(store: &FileStore, scope: &CurrentScope) -> Result<Owner> {
    let owner: Owner =
        read(&store.directory().join(OWNER), 65536)?.context("redb ownership header missing")?;
    validate(store, scope, owner.capacity)?;
    ensure!(
        owner.version == FORMAT && owner.scope == *scope,
        "foreign/incompatible redb ownership header"
    );
    Ok(owner)
}
fn receipt(store: &FileStore, owner: &Owner) -> Result<Bootstrap> {
    let receipt: Bootstrap = read(&store.directory().join(BOOTSTRAP), 196608)?
        .context("redb bootstrap receipt missing")?;
    receipt.head.validate()?;
    ensure!(
        receipt.version == FORMAT
            && receipt.head.scope == owner.scope
            && receipt.head.sequence == 0
            && receipt.capacity <= owner.capacity
            && (receipt.complete || receipt.capacity == owner.capacity),
        "redb bootstrap ownership mismatch"
    );
    Ok(receipt)
}
fn existing(
    store: &FileStore,
    scope: &CurrentScope,
    quota: &Quota,
    slot: &Arc<Slot>,
) -> Result<Arc<Database>> {
    existing_with_admitter(store, scope, quota, slot, None)
}
fn existing_with_admitter(
    store: &FileStore,
    scope: &CurrentScope,
    quota: &Quota,
    slot: &Arc<Slot>,
    admitter: Option<Arc<super::CapacityAdmitter>>,
) -> Result<Arc<Database>> {
    existing_with_admitter_observed(store, scope, quota, slot, admitter, || Ok(()))
}
fn existing_with_admitter_observed(
    store: &FileStore,
    scope: &CurrentScope,
    quota: &Quota,
    slot: &Arc<Slot>,
    admitter: Option<Arc<super::CapacityAdmitter>>,
    after_owner: impl FnOnce() -> Result<()>,
) -> Result<Arc<Database>> {
    let mut owner = header(store, scope)?;
    ensure!(
        receipt(store, &owner)?.complete,
        "redb initialization requires its owning writer"
    );
    quota.verify_current_allowance(&grant_scope(scope), allowance(owner.capacity)?)?;
    after_owner()?;
    let mut cached = slot
        .database
        .lock()
        .map_err(|_| anyhow::anyhow!("current database cache poisoned"))?;
    if let Some(entry) = &*cached
        && entry.database.needs_reopen()
    {
        ensure!(entry.scope == *scope, "foreign failed database handle");
        if Arc::strong_count(&entry.database) == 1 && entry.database.idle() {
            cached.take();
        } else {
            return Err(entry
                .database
                .reopen_error()
                .context(crate::current::CurrentOpenContention));
        }
    }
    if let Some(entry) = &*cached {
        let observed_capacity = entry.database.capacity();
        if observed_capacity > owner.capacity {
            // Automatic growth persists its grant and header before publishing
            // this high-water. It may have advanced after our first observation.
            // Refresh once against the captured value; later admitted growth
            // must not turn this read into a false integrity failure.
            owner = header(store, scope)?;
            ensure!(
                receipt(store, &owner)?.complete,
                "redb initialization requires its owning writer"
            );
            quota.verify_current_allowance(&grant_scope(scope), allowance(owner.capacity)?)?;
        }
        ensure!(
            entry.scope == *scope && observed_capacity <= owner.capacity,
            "cached redb ownership/capacity changed"
        );
        entry.database.admit_capacity(owner.capacity);
        return Ok(entry.database.clone());
    }
    let file = owned_file(&store.directory().join(DATABASE), owner.capacity, false)?;
    let database = Arc::new(if let Some(admitter) = admitter {
        Database::open_with_admitter(file, owner.capacity, admitter)?
    } else {
        Database::open(file, owner.capacity, false)?
    });
    ensure!(
        database.snapshot()?.head()?.scope == *scope,
        "redb head differs from owner"
    );
    *cached = Some(Cached {
        database: database.clone(),
        scope: scope.clone(),
    });
    Ok(database)
}
pub(super) fn writer_engine(
    store: &FileStore,
    writer: &WriterLock,
    quota: &Quota,
    scope: &CurrentScope,
) -> Result<(Engine, Gate, u64)> {
    crate::maintenance::check_writer(store, writer, quota)?;
    store.ensure_index_ready()?;
    let gate = Gate::shared(store)?;
    let slot = slot(store)?;
    let admitter = growth_admitter(store, writer, quota, scope);
    let database = existing_with_admitter(store, scope, quota, &slot, Some(admitter.clone()))?;
    database.set_capacity_admitter(admitter)?;
    let capacity = database.capacity();
    Ok((
        Engine {
            database,
            _slot: slot,
        },
        gate,
        capacity,
    ))
}
fn growth_admitter(
    store: &FileStore,
    writer: &WriterLock,
    quota: &Quota,
    scope: &CurrentScope,
) -> Arc<super::CapacityAdmitter> {
    let growth_store = FileStore::new(store.directory().to_owned());
    let growth_writer = writer.clone();
    let growth_quota = quota.clone();
    let growth_scope = scope.clone();
    Arc::new(move |end, current| {
        admit_growth(
            &growth_store,
            &growth_writer,
            &growth_quota,
            &growth_scope,
            end,
            current,
        )
    })
}
pub(super) fn grow(
    store: &FileStore,
    writer: &WriterLock,
    quota: &Quota,
    scope: &CurrentScope,
    next: u64,
    gate: &Gate,
    observe: &mut dyn FnMut(GrowthStage) -> Result<()>,
) -> Result<crate::current::CurrentCapacityChange> {
    crate::maintenance::check_writer(store, writer, quota)?;
    crate::current::database::validate_capacity(next)?;
    if !gate.try_exclusive()? {
        return Err(anyhow::anyhow!("redb capacity growth has active readers")
            .context(crate::current::CurrentGateContention::Readers));
    }
    let result = (|| {
        let mut owner = header(store, scope)?;
        ensure!(
            receipt(store, &owner)?.complete && next > owner.capacity,
            "invalid redb capacity growth base"
        );
        let slot = slot(store)?;
        {
            let cached = slot
                .database
                .lock()
                .map_err(|_| anyhow::anyhow!("current database cache poisoned"))?;
            if let Some(entry) = &*cached {
                ensure!(
                    entry.scope == *scope && entry.database.capacity() == owner.capacity,
                    "growth cached owner changed"
                );
                ensure!(
                    entry.database.snapshot()?.phase()? == crate::current::TransactionPhase::Ready,
                    "redb capacity cannot change during publication"
                );
                if Arc::strong_count(&entry.database) != 1 || !entry.database.idle() {
                    return Err(anyhow::anyhow!(
                        "redb capacity growth has retained engine handles"
                    )
                    .context(crate::current::CurrentGateContention::Readers));
                }
            } else {
                let file = owned_file(&store.directory().join(DATABASE), owner.capacity, false)?;
                let database = Database::open(file, owner.capacity, false)?;
                ensure!(
                    database.snapshot()?.phase()? == crate::current::TransactionPhase::Ready,
                    "redb capacity cannot change during publication"
                );
            }
        }
        let mut grant = quota
            .current_grant(writer, &grant_scope(scope))?
            .context("redb capacity grant missing")?;
        let used = fs::metadata(store.directory().join(DATABASE))?.len();
        ensure!(used <= owner.capacity, "redb database exceeds growth base");
        let minimum = allowance(next)?;
        if grant.capacity() < minimum {
            grant.resize(writer, minimum)?;
        }
        crate::capacity::require_growth(quota.root(), minimum - used)?;
        {
            let mut cached = slot
                .database
                .lock()
                .map_err(|_| anyhow::anyhow!("current database cache poisoned"))?;
            if let Some(entry) = &*cached {
                ensure!(
                    Arc::strong_count(&entry.database) == 1 && entry.database.idle(),
                    "redb engine became retained during growth"
                );
            }
            cached.take();
        }
        observe(GrowthStage::BeforeOwner)?;
        owner.capacity = next;
        persist(&store.directory().join(OWNER), &owner, 65536)?;
        observe(GrowthStage::AfterOwner)?;
        Ok(crate::current::CurrentCapacityChange {
            capacity_bytes: next,
            maintenance_error: None,
        })
    })();
    let restored = gate.restore_shared();
    match (result, restored) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(error),
    }
}

pub(super) fn compact(
    store: &FileStore,
    writer: &WriterLock,
    quota: &Quota,
    scope: &CurrentScope,
    gate: &Gate,
) -> Result<(bool, u64, u64, crate::current::CurrentToken)> {
    crate::maintenance::check_writer(store, writer, quota)?;
    if !gate.try_exclusive()? {
        return Err(anyhow::anyhow!("redb compaction has active readers")
            .context(crate::current::CurrentGateContention::Readers));
    }
    let result = (|| {
        let owner = header(store, scope)?;
        ensure!(
            receipt(store, &owner)?.complete,
            "redb bootstrap is incomplete"
        );
        let slot = slot(store)?;
        let before_path = store.directory().join(DATABASE);
        let before_bytes = fs::metadata(&before_path)?.len();
        ensure!(
            before_bytes <= owner.capacity,
            "redb database exceeds owned capacity"
        );
        let cached = {
            let mut cached = slot
                .database
                .lock()
                .map_err(|_| anyhow::anyhow!("current database cache poisoned"))?;
            let entry = cached.as_ref().context("redb database is not open")?;
            ensure!(
                entry.scope == *scope && entry.database.capacity() == owner.capacity,
                "compaction cached owner changed"
            );
            if Arc::strong_count(&entry.database) != 1 || !entry.database.idle() {
                return Err(anyhow::anyhow!(
                    "redb compaction has retained engine or snapshot handles"
                )
                .context(crate::current::CurrentGateContention::Readers));
            }
            cached.take().context("redb database cache disappeared")?
        };
        let token = cached.database.snapshot()?.head()?.token()?;
        let transaction = cached.database.write()?;
        crate::current::kv::publication::verify(
            &transaction,
            &cached.database.snapshot()?.head()?,
            &mut crate::current::kv::publication::Verification::default(),
        )?;
        drop(transaction);
        let scope_copy = cached.scope.clone();
        let was_compacted = match Arc::try_unwrap(cached.database) {
            Ok(database) => database.compact()?,
            Err(database) => {
                let mut lock = slot
                    .database
                    .lock()
                    .map_err(|_| anyhow::anyhow!("current database cache poisoned"))?;
                *lock = Some(Cached {
                    database,
                    scope: scope_copy,
                });
                anyhow::bail!("redb compaction lost exclusive database ownership")
            }
        };
        let file = owned_file(&before_path, owner.capacity, false)?;
        let database = Arc::new(Database::open(file, owner.capacity, false)?);
        let after_head = database.snapshot()?.head()?;
        ensure!(
            after_head.token()? == token,
            "redb compaction changed the committed head"
        );
        let transaction = database.write()?;
        crate::current::kv::publication::verify(
            &transaction,
            &after_head,
            &mut crate::current::kv::publication::Verification::default(),
        )?;
        drop(transaction);
        let after_bytes = fs::metadata(&before_path)?.len();
        let mut lock = slot
            .database
            .lock()
            .map_err(|_| anyhow::anyhow!("current database cache poisoned"))?;
        *lock = Some(Cached {
            database,
            scope: scope.clone(),
        });
        Ok((was_compacted, before_bytes, after_bytes, token))
    })();
    let restored = gate.restore_shared();
    match (result, restored) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(error),
    }
}
impl FileStore {
    pub fn grow_redb_capacity(
        &self,
        writer: &WriterLock,
        quota: &Quota,
        scope: &CurrentScope,
        next: u64,
    ) -> Result<crate::current::CurrentCapacityChange> {
        let (engine, gate, _) = writer_engine(self, writer, quota, scope)?;
        engine.clear_capacity_admitter();
        drop(engine);
        grow(self, writer, quota, scope, next, &gate, &mut |_| Ok(()))
    }
    /// The native owner calls this under its matching writer lease. Only a new
    /// or matching incomplete bootstrap can initialize an empty engine.
    pub fn bootstrap_redb(
        &self,
        writer: &WriterLock,
        quota: &Quota,
        head: &CurrentHead,
        capacity: u64,
    ) -> Result<Engine> {
        self.bootstrap_redb_observed(writer, quota, head, capacity, &mut |_| Ok(()))
    }
    pub(super) fn bootstrap_redb_observed(
        &self,
        writer: &WriterLock,
        quota: &Quota,
        head: &CurrentHead,
        capacity: u64,
        observe: &mut dyn FnMut(Stage) -> Result<()>,
    ) -> Result<Engine> {
        validate(self, &head.scope, capacity)?;
        self.ensure_index_ready()?;
        crate::maintenance::check_writer(self, writer, quota)?;
        // The writer lease serializes initialization. A matching unfinished
        // bootstrap has no readable database yet; selection pins may remain
        // shared while this empty/native-only image is created.
        let _gate = Gate::shared(self)?;
        head.validate()?;
        ensure!(
            head.sequence == 0,
            "redb bootstrap requires the initial head"
        );
        ensure!(
            head.artifact_root == crate::current::map::empty_root(&head.scope)?
                && head.graph_head
                    == airk_core::graph_storage::GraphHead::empty(head.graph_head.scope.clone())?
                && head.lexical_sequence == 0
                && head.lexical_commit == super::lexical::Manifest::empty(&head.scope)?.digest()?,
            "redb bootstrap requires exact empty components"
        );
        let owner_path = self.directory().join(OWNER);
        let database_path = self.directory().join(DATABASE);
        let expected = Owner {
            version: FORMAT,
            scope: head.scope.clone(),
            capacity,
            replay: None,
        };
        let prior_owner: Option<Owner> = read(&owner_path, 65536)?;
        ensure!(
            prior_owner.as_ref().is_none_or(|value| value == &expected),
            "redb bootstrap owner differs; old state preserved"
        );
        for name in [
            "current.sqlite3",
            "current.sqlite3-wal",
            "current.sqlite3-shm",
            "current.sqlite3-journal",
            "index.sqlite3",
            "snapshot.json",
        ] {
            ensure!(
                !self.directory().join(name).try_exists()?,
                "preceding storage requires sibling replacement; old state preserved"
            );
        }
        let bootstrap_path = self.directory().join(BOOTSTRAP);
        let prior: Option<Bootstrap> = read(&bootstrap_path, 196608)?;
        if let Some(prior) = &prior {
            ensure!(
                prior.version == FORMAT
                    && prior.head == *head
                    && (if prior.complete {
                        prior.capacity <= capacity
                    } else {
                        prior.capacity == capacity
                    }),
                "redb bootstrap intention changed"
            );
            if prior.complete {
                ensure!(
                    prior_owner.is_some(),
                    "completed redb bootstrap lost its owner"
                );
                let slot = slot(self)?;
                let database = existing(self, &head.scope, quota, &slot)?;
                return Ok(Engine {
                    database,
                    _slot: slot,
                });
            }
        } else {
            ensure!(
                prior_owner.is_none() && !database_path.try_exists()?,
                "existing redb state has no bootstrap authority"
            );
        }
        ensure!(
            !database_path.try_exists()? || prior_owner.is_some(),
            "existing redb database lost its owner"
        );
        let minimum = allowance(capacity)?;
        if let Some(grant) = quota.current_grant(writer, &grant_scope(&head.scope))? {
            ensure!(
                grant.capacity() >= minimum,
                "redb bootstrap grant is insufficient"
            );
        } else {
            quota.reserve_current(writer, grant_scope(&head.scope), minimum)?;
        }
        let used = if database_path.try_exists()? {
            let metadata = fs::symlink_metadata(&database_path)?;
            crate::managed_collection::reject_file_aliases(&database_path, &metadata)?;
            ensure!(
                metadata.len() <= capacity,
                "redb bootstrap file exceeds capacity"
            );
            metadata.len()
        } else {
            0
        };
        crate::capacity::require_growth(quota.root(), minimum - used)?;
        let mut bootstrap = Bootstrap {
            version: FORMAT,
            head: head.clone(),
            capacity,
            complete: false,
        };
        if prior.is_none() {
            persist(&bootstrap_path, &bootstrap, 196608)?;
        }
        observe(Stage::Receipt)?;
        if prior_owner.is_none() {
            persist(&owner_path, &expected, 65536)?;
        }
        observe(Stage::Owner)?;
        let slot = slot(self)?;
        let mut cached = slot
            .database
            .lock()
            .map_err(|_| anyhow::anyhow!("current database cache poisoned"))?;
        let database = if let Some(entry) = &*cached {
            ensure!(
                entry.scope == head.scope && entry.database.capacity() == capacity,
                "unfinished bootstrap handle changed"
            );
            entry.database.clone()
        } else {
            let file = owned_file(&database_path, capacity, !database_path.try_exists()?)?;
            observe(Stage::File)?;
            Arc::new(Database::resume_bootstrap(file, capacity)?)
        };
        observe(Stage::Schema)?;
        let snapshot = database.snapshot()?;
        let has_head = snapshot
            .transaction()
            .open_table(super::tables::META)?
            .get(0)?
            .is_some();
        if has_head {
            ensure!(
                snapshot.head()? == *head,
                "committed bootstrap head changed"
            );
        }
        drop(snapshot);
        if !has_head {
            ensure!(
                database
                    .initialize_components(head.scope.clone(), head.graph_head.scope.clone())?
                    == *head,
                "bootstrap initial components differ"
            );
        }
        observe(Stage::Head)?;
        *cached = Some(Cached {
            database: database.clone(),
            scope: head.scope.clone(),
        });
        bootstrap.complete = true;
        persist(&bootstrap_path, &bootstrap, 196608)?;
        drop(cached);
        observe(Stage::Complete)?;
        Ok(Engine {
            database,
            _slot: slot,
        })
    }
    /// Pins both the engine snapshot and the external object/reclamation gate.
    /// The native project worker owns admission; collection I/O stays out of the broker.
    pub fn redb_read(&self, scope: &CurrentScope) -> Result<OwnedSnapshot> {
        let mut profile = super::profile_start();
        self.ensure_index_ready()?;
        let gate = Gate::shared(self)?;
        let slot = slot(self)?;
        super::profile_tick("read.gate", &mut profile);
        let quota = Quota::for_store(self)?;
        super::profile_tick("read.quota", &mut profile);
        let database = existing(self, scope, &quota, &slot)?;
        super::profile_tick("read.engine", &mut profile);
        let snapshot = database.snapshot()?;
        super::profile_tick("read.snapshot", &mut profile);
        ensure!(
            snapshot.head()?.scope == *scope,
            "redb request scope changed"
        );
        Ok(OwnedSnapshot {
            snapshot,
            _slot: slot,
            _gate: gate,
        })
    }
}

fn replay_id(pending: &super::journal::Prepared) -> Result<airk_core::identity::Sha256Digest> {
    Ok(framed_digest(
        b"airk-kv-replay-admission/1",
        &[&serde_json::to_vec(pending)?],
    ))
}
pub(super) fn replay_admitted(
    store: &FileStore,
    scope: &CurrentScope,
    pending: &super::journal::Prepared,
) -> Result<bool> {
    let owner = header(store, scope)?;
    let Some(replay) = owner.replay else {
        return Ok(false);
    };
    ensure!(
        replay.pending == replay_id(pending)?
            && replay.expected == pending.expected
            && replay.target == pending.target.token()?
            && replay.high_water <= replay.capacity
            && replay.capacity <= owner.capacity,
        "foreign replay admission receipt"
    );
    Ok(true)
}
pub(super) fn admit_replay(
    store: &FileStore,
    writer: &WriterLock,
    quota: &Quota,
    scope: &CurrentScope,
    engine: &Engine,
    pending: &super::journal::Prepared,
    high_water: u64,
) -> Result<()> {
    // The measured installation includes real padding allocations for separate
    // marker/commit/repair metadata. Keep its high-water funded even if restore,
    // close or recovery trims free tails before replay consumes it again.
    let required = high_water;
    let next = admit_growth(store, writer, quota, scope, required, engine.capacity())?;
    engine.admit_capacity(next);
    let mut owner = header(store, scope)?;
    owner.replay = Some(crate::current::database::ReplayAdmission {
        pending: replay_id(pending)?,
        expected: pending.expected,
        target: pending.target.token()?,
        high_water,
        capacity: next,
    });
    persist(&store.directory().join(OWNER), &owner, 65536)
}
pub(super) fn settle_replay(
    store: &FileStore,
    scope: &CurrentScope,
    token: crate::current::CurrentToken,
) -> Result<()> {
    let mut owner = header(store, scope)?;
    if let Some(replay) = &owner.replay {
        ensure!(
            token == replay.expected || token == replay.target,
            "unsettled replay admission differs from Ready publication"
        );
        owner.replay = None;
        persist(&store.directory().join(OWNER), &owner, 65536)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::{
        graph_storage::{GraphHead, GraphScope},
        identity::{Incarnation, Sha256Digest},
    };
    fn fixture() -> Result<(tempfile::TempDir, FileStore, Quota, CurrentHead)> {
        let directory = tempfile::tempdir()?;
        let (store, quota, head) = fixture_in(directory.path())?;
        Ok((directory, store, quota, head))
    }
    fn fixture_in(directory: &Path) -> Result<(FileStore, Quota, CurrentHead)> {
        let quota = Quota::open_service(directory.join("data"))?;
        let scope = CurrentScope {
            root: "fixture/owned-source".into(),
            collection: "owned".into(),
            incarnation: Incarnation::from_bytes([1; 16]),
        };
        let store = FileStore::new(
            quota
                .root()
                .join("collections-1.4")
                .join("owned")
                .join("incarnations")
                .join(scope.incarnation.to_string()),
        );
        drop(store.lock()?);
        let head = CurrentHead {
            version: 2,
            sequence: 0,
            lexical_sequence: 0,
            artifact_root: crate::current::map::empty_root(&scope)?,
            lexical_commit: super::super::lexical::Manifest::empty(&scope)?.digest()?,
            graph_head: GraphHead::empty(GraphScope {
                root: scope.root.clone(),
                collection: scope.collection.clone(),
                incarnation: scope.incarnation,
                graph_instance: Incarnation::from_bytes([2; 16]),
                specification: Sha256Digest::from_bytes([3; 32]),
                record_bytes: 32,
            })?,
            scope,
        };
        Ok((store, quota, head))
    }
    #[test]
    fn shared_selection_and_concurrent_bootstrap_reads_never_open_a_second_backend() -> Result<()> {
        let (_directory, store, quota, head) = fixture()?;
        let writer = store.lock()?;
        let _selection = Gate::shared(&store)?;
        let engine =
            store.bootstrap_redb_observed(&writer, &quota, &head, 2 << 20, &mut |stage| {
                if !matches!(stage, Stage::Head | Stage::Complete) {
                    return Ok(());
                }
                let complete = matches!(stage, Stage::Complete);
                let source = store.clone();
                let scope = head.scope.clone();
                let (send, receive) = std::sync::mpsc::sync_channel(1);
                let task = std::thread::spawn(move || {
                    let result = source
                        .redb_read(&scope)
                        .map(|read| (read.snapshot().head(), Arc::as_ptr(&read._slot) as usize));
                    let _ = send.send(result);
                });
                let observed = receive
                    .recv_timeout(std::time::Duration::from_secs(2))
                    .context("bootstrap read blocked on duplicate native open")?;
                if complete {
                    let (observed, pointer) = observed?;
                    ensure!(
                        observed? == head && pointer == Arc::as_ptr(&slot(&store)?) as usize,
                        "bootstrap read was torn or opened a separate owner"
                    );
                } else {
                    ensure!(observed.is_err(), "unfinished bootstrap became readable");
                }
                task.join()
                    .map_err(|_| anyhow::anyhow!("bootstrap reader panicked"))?;
                Ok(())
            })?;
        let config = crate::current::collection::CollectionConfiguration {
            roots: Default::default(),
            extractor_version: "fixture".into(),
            extraction_policy_digest: airk_core::identity::Sha256Digest::from_bytes([4; 32]),
            index_detail: Default::default(),
            vector_selection: Default::default(),
            model: None,
        };
        let published = engine.initialize_collection(&store, &config)?;
        drop(engine);
        let reopened = store.bootstrap_redb(&writer, &quota, &head, 2 << 20)?;
        ensure!(
            reopened.snapshot()?.head()? == published && published.sequence == 1,
            "completed bootstrap replaced an existing publication"
        );
        Ok(())
    }
    #[test]
    fn real_main_growth_headroom_denial_keeps_typed_pressure_and_prior_head() -> Result<()> {
        let (_directory, store, quota, head) = fixture()?;
        let writer = store.lock()?;
        drop(store.bootstrap_redb(&writer, &quota, &head, 2 << 20)?);
        let redb = store.redb_writer(&writer, &quota, &head.scope)?;
        let _pressure = crate::capacity::test_growth_headroom(quota.root(), 4096)?;
        let error = redb.test_write_meta(251, &vec![7; 4 << 20]).unwrap_err();
        assert_eq!(
            crate::capacity::storage_pressure_code(&error),
            Some("physical_storage_headroom"),
            "{error:#}"
        );
        assert_eq!(redb.head()?, head);
        Ok(())
    }
    #[test]
    fn initialization_resumes_every_owned_boundary_and_shares_one_worker_handle() -> Result<()> {
        for interrupted in [
            Stage::Receipt,
            Stage::Owner,
            Stage::File,
            Stage::Schema,
            Stage::Head,
            Stage::Complete,
        ] {
            let (_directory, store, quota, head) = fixture()?;
            let writer = store.lock()?;
            assert!(
                store
                    .bootstrap_redb_observed(&writer, &quota, &head, 2 << 20, &mut |stage| {
                        if stage == interrupted {
                            anyhow::bail!("injected bootstrap interruption")
                        };
                        Ok(())
                    })
                    .is_err()
            );
            release_idle();
            let engine = store.bootstrap_redb(&writer, &quota, &head, 2 << 20)?;
            assert_eq!(engine.snapshot()?.head()?, head);
            assert!(receipt(&store, &header(&store, &head.scope)?)?.complete);
            drop(writer);
            let first = store.redb_read(&head.scope)?;
            let second_store = FileStore::new(store.directory());
            let second = second_store.redb_read(&head.scope)?;
            assert!(Arc::ptr_eq(&first._slot, &second._slot));
            let mut target = head.clone();
            target.sequence = 1;
            let pending = engine.prepare(head.token()?, target.clone(), &[])?;
            engine.mark_applying(&pending)?;
            engine.apply(&pending, |_, _| Ok(()))?;
            assert_eq!(first.snapshot().head()?, head);
            assert_eq!(second.snapshot().head()?, head);
            assert_eq!(
                second_store.redb_read(&head.scope)?.snapshot().head()?,
                target
            );
            drop(engine);
            release_idle();
            assert!(
                first._slot.database.lock().unwrap().is_some(),
                "active readers lost the worker handle"
            );
            drop(first);
            drop(second);
            release_idle();
            assert_eq!(store.redb_read(&head.scope)?.snapshot().head()?, target);
        }
        Ok(())
    }
    #[test]
    fn completed_bootstrap_never_recreates_missing_data_and_foreign_scopes_fail() -> Result<()> {
        let (_directory, store, quota, head) = fixture()?;
        let writer = store.lock()?;
        let engine = store.bootstrap_redb(&writer, &quota, &head, 2 << 20)?;
        drop(engine);
        drop(writer);
        let mut foreign = head.scope.clone();
        foreign.root = "fixture/foreign".into();
        assert!(store.redb_read(&foreign).is_err());
        release_idle();
        fs::remove_file(store.directory().join(DATABASE))?;
        assert!(store.redb_read(&head.scope).is_err());
        let writer = store.lock()?;
        assert!(
            store
                .bootstrap_redb(&writer, &quota, &head, 2 << 20)
                .is_err()
        );
        assert!(!store.directory().join(DATABASE).try_exists()?);
        Ok(())
    }
    #[test]
    fn invalid_initial_components_fail_before_receipts_or_admission() -> Result<()> {
        let (_directory, store, quota, mut head) = fixture()?;
        let writer = store.lock()?;
        head.artifact_root = Sha256Digest::from_bytes([99; 32]);
        assert!(
            store
                .bootstrap_redb(&writer, &quota, &head, 2 << 20)
                .is_err()
        );
        assert!(!store.directory().join(DATABASE).try_exists()?);
        assert!(!store.directory().join(OWNER).try_exists()?);
        assert!(quota.current_grants(&writer)?.is_empty());
        Ok(())
    }
    #[test]
    fn compact_bootstrap_fits_the_minimum_admitted_database() -> Result<()> {
        let (_directory, store, quota, head) = fixture()?;
        let writer = store.lock()?;
        let engine = store.bootstrap_redb(&writer, &quota, &head, 65536)?;
        assert_eq!(engine.snapshot()?.head()?, head);
        assert!(fs::metadata(store.directory().join(DATABASE))?.len() <= 65536);
        Ok(())
    }
    #[test]
    fn redb_writer_owns_the_gate_and_reconciles_capacity_growth() -> Result<()> {
        let (_directory, store, quota, head) = fixture()?;
        let writer = store.lock()?;
        drop(store.bootstrap_redb(&writer, &quota, &head, 2 << 20)?);
        let mut native = store.redb_writer(&writer, &quota, &head.scope)?;
        assert_eq!(native.head()?, head);
        assert_eq!(native.phase()?, crate::current::TransactionPhase::Ready);
        assert!(native.pending()?.is_none());
        native.grow_capacity(4 << 20)?;
        assert_eq!(native.capacity(), 4 << 20);
        assert_eq!(native.head()?, head);
        Ok(())
    }
    #[test]
    fn live_redb_growth_is_admitted_before_file_extension_without_reopen() -> Result<()> {
        let (_directory, store, quota, head) = fixture()?;
        let writer = store.lock()?;
        drop(store.bootstrap_redb(&writer, &quota, &head, 2 << 20)?);
        let native = store.redb_writer(&writer, &quota, &head.scope)?;
        let before = native.capacity();
        native.test_write_meta(100, &vec![7; 4 << 20])?;
        let after = native.capacity();
        assert!(
            after > before,
            "backend grew its admitted high-water in place"
        );
        assert_eq!(header(&store, &head.scope)?.capacity, after);
        assert!(fs::metadata(store.directory().join(DATABASE))?.len() <= after);
        let snapshot = store.redb_read(&head.scope)?;
        let meta = snapshot
            .snapshot()
            .transaction()
            .open_table(super::super::tables::META)?;
        assert_eq!(meta.get(100)?.unwrap().value().len(), 4 << 20);
        let status = quota.status()?;
        let grant = quota
            .current_grant(&writer, &grant_scope(&head.scope))?
            .unwrap();
        let actual = fs::metadata(store.directory().join(DATABASE))?.len();
        assert_eq!(status.reserved_bytes, grant.capacity() - actual);
        Ok(())
    }
    #[test]
    fn read_open_preserves_evidence_when_growth_races_the_owner_observation() -> Result<()> {
        let (_directory, store, quota, head) = fixture()?;
        let writer = store.lock()?;
        drop(store.bootstrap_redb(&writer, &quota, &head, 2 << 20)?);
        let native = store.redb_writer(&writer, &quota, &head.scope)?;
        let before = native.capacity();
        let slot = slot(&store)?;
        let database =
            existing_with_admitter_observed(&store, &head.scope, &quota, &slot, None, || {
                native.test_write_meta(100, &vec![7; 4 << 20])
            })?;
        assert!(database.capacity() > before);
        let read = database.snapshot()?;
        assert_eq!(read.head()?, head);
        let meta = read.transaction().open_table(super::super::tables::META)?;
        assert_eq!(meta.get(100)?.unwrap().value().len(), 4 << 20);
        Ok(())
    }
    #[test]
    fn read_open_still_refuses_a_rolled_back_capacity_receipt() -> Result<()> {
        let (_directory, store, quota, head) = fixture()?;
        let writer = store.lock()?;
        drop(store.bootstrap_redb(&writer, &quota, &head, 2 << 20)?);
        let native = store.redb_writer(&writer, &quota, &head.scope)?;
        let before = native.capacity();
        native.test_write_meta(100, &vec![7; 4 << 20])?;
        let admitted = header(&store, &head.scope)?;
        assert!(admitted.capacity > before);
        let mut stale = admitted.clone();
        stale.capacity = before;
        persist(&store.directory().join(OWNER), &stale, 65536)?;
        let refused = store.redb_read(&head.scope);
        let capacity = native.capacity();
        persist(&store.directory().join(OWNER), &admitted, 65536)?;
        assert!(refused.is_err());
        assert_eq!(capacity, admitted.capacity);
        assert_eq!(store.redb_read(&head.scope)?.snapshot().head()?, head);
        Ok(())
    }
    #[test]
    fn quota_counts_owned_capacity_once_and_ungranted_bytes_in_full() -> Result<()> {
        let (_directory, store, quota, head) = fixture()?;
        let writer = store.lock()?;
        let engine = store.bootstrap_redb(&writer, &quota, &head, 2 << 20)?;
        drop(engine);
        let admitted = quota
            .current_grant(&writer, &grant_scope(&head.scope))?
            .unwrap()
            .capacity();
        let actual = fs::metadata(store.directory().join(DATABASE))?.len();
        let status = quota.status()?;
        assert_eq!(status.reserved_bytes, admitted - actual);
        assert_eq!(
            status.collections["owned"].reserved_bytes,
            admitted - actual
        );
        let foreign = store.directory().join("unrelated-retained-file");
        fs::write(&foreign, [7; 8192])?;
        let next = quota.status()?;
        assert_eq!(next.used_bytes, status.used_bytes + 8192);
        assert_eq!(next.reserved_bytes, status.reserved_bytes);
        // A real but unrelated durable grant cannot discount this database.
        let other = CurrentGrantScope {
            collection: head.scope.collection.clone(),
            incarnation: head.scope.incarnation,
            transaction: framed_digest(b"unrelated-test-grant", &[]),
        };
        quota.reserve_current(&writer, other, 131072)?;
        assert_eq!(quota.status()?.reserved_bytes, next.reserved_bytes + 131072);
        Ok(())
    }
    #[test]
    fn dropped_redb_writer_releases_its_admission_lock_clone() -> Result<()> {
        let (_directory, store, quota, head) = fixture()?;
        let writer = store.lock()?;
        drop(store.bootstrap_redb(&writer, &quota, &head, 2 << 20)?);
        let native = store.redb_writer(&writer, &quota, &head.scope)?;
        drop(native);
        drop(writer);
        let reacquired = store.lock()?;
        drop(reacquired);
        Ok(())
    }
    #[test]
    fn failed_compaction_with_retained_snapshot_does_not_retain_writer_lock() -> Result<()> {
        let (_directory, store, quota, head) = fixture()?;
        let writer = store.lock()?;
        drop(store.bootstrap_redb(&writer, &quota, &head, 2 << 20)?);
        let retained = store.redb_read(&head.scope)?;
        let mut native = store.redb_writer(&writer, &quota, &head.scope)?;
        assert!(native.compact().is_err());
        drop(native);
        drop(retained);
        drop(writer);
        let reacquired = store.lock()?;
        drop(reacquired);
        Ok(())
    }
    #[test]
    fn manual_compaction_preserves_head_and_reclaims_deleted_pages() -> Result<()> {
        let (_directory, store, quota, head) = fixture()?;
        let writer = store.lock()?;
        let engine = store.bootstrap_redb(&writer, &quota, &head, 64 << 20)?;
        let tx = engine.write()?;
        {
            let mut meta = tx.open_table(super::super::tables::META)?;
            for key in 100..108 {
                meta.insert(key, vec![key; 1 << 20].as_slice())?;
            }
        }
        tx.commit()?;
        let tx = engine.write()?;
        {
            let mut meta = tx.open_table(super::super::tables::META)?;
            for key in 100..108 {
                meta.remove(key)?;
            }
        }
        tx.commit()?;
        drop(engine);
        let pinned = store.redb_read(&head.scope)?;
        let mut native = store.redb_writer(&writer, &quota, &head.scope)?;
        assert!(native.compact().is_err(), "active readers block compaction");
        drop(pinned);
        let report = native.compact()?;
        assert_eq!(report.token, head.token()?);
        assert!(report.bytes_after < report.bytes_before);
        assert_eq!(
            report.bytes_reclaimed,
            report.bytes_before - report.bytes_after
        );
        assert_eq!(native.head()?, head);
        let status = quota.status()?;
        let capacity = quota
            .current_grant(&writer, &grant_scope(&head.scope))?
            .unwrap()
            .capacity();
        assert_eq!(status.reserved_bytes, capacity - report.bytes_after);
        Ok(())
    }
    #[test]
    fn capacity_growth_waits_for_pins_and_reconciles_both_owner_boundaries() -> Result<()> {
        for interrupted in [GrowthStage::BeforeOwner, GrowthStage::AfterOwner] {
            let (_directory, store, quota, head) = fixture()?;
            let writer = store.lock()?;
            let engine = store.bootstrap_redb(&writer, &quota, &head, 2 << 20)?;
            drop(engine);
            let read = store.current_read(&head.scope)?;
            let error = store
                .grow_redb_capacity(&writer, &quota, &head.scope, 4 << 20)
                .unwrap_err();
            assert!(error.is::<crate::current::CurrentGateContention>());
            assert_eq!(header(&store, &head.scope)?.capacity, 2 << 20);
            drop(read);
            let (engine, gate, _) = writer_engine(&store, &writer, &quota, &head.scope)?;
            drop(engine);
            assert!(
                grow(
                    &store,
                    &writer,
                    &quota,
                    &head.scope,
                    4 << 20,
                    &gate,
                    &mut |stage| {
                        if stage == interrupted {
                            anyhow::bail!("injected growth interruption")
                        };
                        Ok(())
                    }
                )
                .is_err()
            );
            drop(gate);
            let expected = if interrupted == GrowthStage::BeforeOwner {
                2 << 20
            } else {
                4 << 20
            };
            assert_eq!(header(&store, &head.scope)?.capacity, expected);
            let status = quota.status()?;
            let actual = fs::metadata(store.directory().join(DATABASE))?.len();
            let granted = quota
                .current_grant(&writer, &grant_scope(&head.scope))?
                .unwrap()
                .capacity();
            assert_eq!(status.reserved_bytes, granted - actual);
            assert_eq!(store.redb_read(&head.scope)?.snapshot().head()?, head);
            if expected < 4 << 20 {
                store.grow_redb_capacity(&writer, &quota, &head.scope, 4 << 20)?;
            }
            assert_eq!(header(&store, &head.scope)?.capacity, 4 << 20);
            assert_eq!(store.current_read(&head.scope)?.head(), &head);
            assert!(
                quota
                    .current_grant(&writer, &grant_scope(&head.scope))?
                    .unwrap()
                    .capacity()
                    >= allowance(4 << 20)?
            );
        }
        Ok(())
    }
    fn mixed_prepare(
        store: &FileStore,
        quota: &Quota,
        scope: &CurrentScope,
        round: u8,
        count: u32,
    ) -> Result<super::super::journal::Prepared> {
        let base = store.redb_read(scope)?;
        let head = base.snapshot().head()?;
        let reservation = quota.reserve(&scope.collection, 1 << 20)?;
        let graph = crate::current::graph::GraphOverlay::empty(
            &reservation,
            head.graph_head.scope.clone(),
            crate::current::graph::GraphOverlayLimits {
                maximum_bytes: 1 << 20,
                maximum_dirty_records: 1,
            },
        )?
        .seal()?;
        let mut catalog = super::super::catalog::Plan::new(base.snapshot(), &head)?;
        for index in 0..count {
            let key = format!("row-{index:04}");
            let value = vec![round; [64, 8192, 16384][index as usize % 3]];
            catalog.put(
                crate::current::map::MapDomain::Metadata,
                key.as_bytes(),
                (round == 0 || index % 4 != 0).then_some(value.as_slice()),
            )?;
        }
        let writer = store.lock()?;
        let native = store.redb_writer(&writer, quota, scope)?;
        native.prepare_components(
            base.snapshot(),
            &catalog,
            &graph,
            None,
            &Vec::<crate::current::CapturedInput>::new(),
            None,
        )
    }
    fn fix_exact_limits(quota: &Quota, collection: &str) -> Result<()> {
        for _ in 0..2 {
            let status = quota.status()?;
            let item = &status.collections[collection];
            let mut policy = quota.policy()?;
            policy.root_bytes = status.used_bytes + status.reserved_bytes;
            policy
                .collections
                .insert(collection.into(), item.used_bytes + item.reserved_bytes);
            quota.configure(policy)?;
        }
        Ok(())
    }
    fn current_grant_bytes(
        quota: &Quota,
        writer: &WriterLock,
        scope: &CurrentScope,
    ) -> Result<u64> {
        Ok(quota
            .current_grant(writer, &grant_scope(scope))?
            .context("lifetime allowance lost")?
            .capacity())
    }
    fn seed_fragmentation(store: &FileStore, quota: &Quota, head: &CurrentHead) -> Result<()> {
        for round in 0..3 {
            let pending = mixed_prepare(store, quota, &head.scope, round, 96)?;
            let writer = store.lock()?;
            let native = store.redb_writer(&writer, quota, &head.scope)?;
            native.mark_applying(&pending)?;
            native.commit(&pending)?;
        }
        Ok(())
    }
    #[test]
    fn replay_admission_small_quota_and_pinned_snapshot_need_no_new_grant() -> Result<()> {
        let (_directory, store, quota, head) = fixture()?;
        quota.configure(crate::quota::QuotaPolicy {
            root_bytes: 8 << 20,
            default_collection_bytes: 8 << 20,
            collections: Default::default(),
        })?;
        let writer = store.lock()?;
        drop(store.bootstrap_redb(&writer, &quota, &head, 2 << 20)?);
        drop(writer);
        let pending = mixed_prepare(&store, &quota, &head.scope, 1, 1)?;
        let old = store.current_read(&head.scope)?;
        let writer = store.lock()?;
        let native = store.redb_writer(&writer, &quota, &head.scope)?;
        let funded = std::cell::Cell::new(0);
        native.mark_applying_observed(&pending, || {
            fix_exact_limits(&quota, &head.scope.collection)?;
            funded.set(current_grant_bytes(&quota, &writer, &head.scope)?);
            Ok(())
        })?;
        assert!(
            native.capacity() <= 8 << 20,
            "small publications acquired an arbitrary large file allowance"
        );
        assert_eq!(
            current_grant_bytes(&quota, &writer, &head.scope)?,
            funded.get()
        );
        assert_eq!(old.token()?, pending.expected);
        native.commit(&pending)?;
        assert_eq!(
            current_grant_bytes(&quota, &writer, &head.scope)?,
            funded.get()
        );
        assert_eq!(old.token()?, pending.expected);
        assert_eq!(
            store.current_read(&head.scope)?.token()?,
            pending.target.token()?
        );
        let status = quota.status()?;
        assert!(status.used_bytes + status.reserved_bytes <= status.root_limit_bytes);
        Ok(())
    }
    #[test]
    fn insufficient_probe_capacity_stays_prepared_and_can_abort_without_new_source_reads()
    -> Result<()> {
        let (_directory, store, quota, head) = fixture()?;
        let writer = store.lock()?;
        drop(store.bootstrap_redb(&writer, &quota, &head, 2 << 20)?);
        drop(writer);
        let seed = mixed_prepare(&store, &quota, &head.scope, 0, 12)?;
        {
            let writer = store.lock()?;
            let native = store.redb_writer(&writer, &quota, &head.scope)?;
            native.mark_applying(&seed)?;
            native.commit(&seed)?;
        }
        let pending = mixed_prepare(&store, &quota, &head.scope, 1, 192)?;
        fix_exact_limits(&quota, &head.scope.collection)?;
        let old = store.current_read(&head.scope)?;
        let writer = store.lock()?;
        let native = store.redb_writer(&writer, &quota, &head.scope)?;
        let error = native.mark_applying(&pending).unwrap_err();
        assert!(error.is::<crate::quota::QuotaPressure>(), "{error:#}");
        assert!(
            store
                .current_read(&head.scope)
                .err()
                .context("poisoned handle admitted another reader")?
                .is::<crate::current::CurrentOpenContention>()
        );
        assert_eq!(old.token()?, pending.expected);
        drop(native);
        drop(old);
        drop(writer);
        let reopened = store.current_read(&head.scope)?;
        assert_eq!(reopened.token()?, pending.expected);
        assert_eq!(
            reopened.map_get(crate::current::map::MapDomain::Metadata, b"row-0001")?,
            Some(vec![0; 8192])
        );
        let writer = store.lock()?;
        let native = store.redb_writer(&writer, &quota, &head.scope)?;
        assert_eq!(native.phase()?, crate::current::TransactionPhase::Prepared);
        assert_eq!(native.pending()?, Some(pending.clone()));
        native.abort_prepared(&pending)?;
        assert_eq!(native.phase()?, crate::current::TransactionPhase::Ready);
        assert_eq!(native.head()?.token()?, pending.expected);
        Ok(())
    }
    #[test]
    fn replay_admission_exit_child() -> Result<()> {
        let Some(root) = std::env::var_os("AIRK_REPLAY_ADMISSION_ROOT") else {
            return Ok(());
        };
        let root = PathBuf::from(root);
        let (store, quota, head) = fixture_in(&root)?;
        let writer = store.lock()?;
        drop(store.bootstrap_redb(&writer, &quota, &head, 2 << 20)?);
        drop(writer);
        seed_fragmentation(&store, &quota, &head)?;
        let pending = mixed_prepare(&store, &quota, &head.scope, 7, 192)?;
        let old = store.current_read(&head.scope)?;
        let writer = store.lock()?;
        let native = store.redb_writer(&writer, &quota, &head.scope)?;
        let funded = std::cell::Cell::new(0);
        native.mark_applying_observed(&pending, || {
            fix_exact_limits(&quota, &head.scope.collection)?;
            funded.set(current_grant_bytes(&quota, &writer, &head.scope)?);
            Ok(())
        })?;
        assert_eq!(old.token()?, pending.expected);
        assert_eq!(
            current_grant_bytes(&quota, &writer, &head.scope)?,
            funded.get()
        );
        std::fs::write(
            root.join("replay-case.json"),
            serde_json::to_vec(&(pending, funded.get(), native.capacity()))?,
        )?;
        std::process::exit(73)
    }
    #[test]
    fn funded_fragmented_replay_survives_process_exit_at_exact_full_quota_with_live_old_reader()
    -> Result<()> {
        let directory = tempfile::tempdir()?;
        let output = std::process::Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "current::kv::owner::tests::replay_admission_exit_child",
                "--nocapture",
            ])
            .env("AIRK_REPLAY_ADMISSION_ROOT", directory.path())
            .output()?;
        ensure!(
            output.status.code() == Some(73),
            "replay child failed: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let (pending, funded, ceiling): (super::super::journal::Prepared, u64, u64) =
            serde_json::from_slice(&std::fs::read(directory.path().join("replay-case.json"))?)?;
        let (store, quota, head) = fixture_in(directory.path())?;
        fix_exact_limits(&quota, &head.scope.collection)?;
        let old = store.current_read(&head.scope)?;
        assert_eq!(old.token()?, pending.expected);
        let writer = store.lock()?;
        assert_eq!(current_grant_bytes(&quota, &writer, &head.scope)?, funded);
        let native = store.redb_writer(&writer, &quota, &head.scope)?;
        assert_eq!(native.capacity(), ceiling);
        native.mark_applying(&pending)?;
        assert_eq!(current_grant_bytes(&quota, &writer, &head.scope)?, funded);
        native.commit(&pending)?;
        assert_eq!(native.capacity(), ceiling);
        assert_eq!(current_grant_bytes(&quota, &writer, &head.scope)?, funded);
        assert_eq!(old.token()?, pending.expected);
        let read = store.current_read(&head.scope)?;
        assert_eq!(read.token()?, pending.target.token()?);
        assert_eq!(
            read.map_get(crate::current::map::MapDomain::Metadata, b"row-0001")?,
            Some(vec![7; 8192])
        );
        let status = quota.status()?;
        assert!(status.used_bytes + status.reserved_bytes <= status.root_limit_bytes);
        Ok(())
    }
}
