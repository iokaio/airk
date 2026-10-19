// SPDX-License-Identifier: Apache-2.0
//! Verified lightweight representations of exact committed publications.
//!
//! Publication identity remains unchanged. These administrative contracts bind
//! a separate representation; they never imply historical search support.
use airk_core::identity::{PublicationReceipt, RevisionToken, Sha256Digest, framed_digest};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    path::{Path, PathBuf},
    sync::Arc,
};

pub const HISTORY_DESCRIPTOR_VERSION: u32 = 1;
pub const HISTORY_BINDING_VERSION: u32 = 1;
pub const HISTORY_INTENT_VERSION: u32 = 1;
pub const MAX_HISTORY_DESCRIPTOR_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_HISTORY_DEPENDENCIES: usize = 65_536;
pub const MAX_HISTORY_INTENT_BYTES: usize = 16 * 1024;
pub const MAX_HISTORY_INTENTS: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryRemovalStage {
    IntentDurable,
    PackageDetached,
    PhysicalRemoved,
    ReceiptRetired,
    IntentComplete,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryRemovalIntent {
    pub version: u32,
    pub id: Sha256Digest,
    pub binding: HistoryBinding,
    pub authorized_head: RevisionToken,
    pub stage: HistoryRemovalStage,
}

impl HistoryRemovalIntent {
    pub fn new(binding: HistoryBinding, authorized_head: RevisionToken) -> Result<Self> {
        let id = framed_digest(
            b"history-removal/1",
            &[&serde_json::to_vec(&(&binding, authorized_head))?],
        );
        let intent = Self {
            version: HISTORY_INTENT_VERSION,
            id,
            binding,
            authorized_head,
            stage: HistoryRemovalStage::IntentDurable,
        };
        intent.validate()?;
        Ok(intent)
    }
    pub fn validate(&self) -> Result<()> {
        self.binding.validate()?;
        self.binding
            .publication
            .token
            .validate_against(&self.authorized_head)?;
        ensure!(
            self.version == HISTORY_INTENT_VERSION
                && self.binding.publication.token != self.authorized_head,
            "invalid historical retirement scope"
        );
        ensure!(
            self.id
                == framed_digest(
                    b"history-removal/1",
                    &[&serde_json::to_vec(&(&self.binding, self.authorized_head))?]
                )
                && serde_json::to_vec(self)?.len() <= MAX_HISTORY_INTENT_BYTES,
            "invalid historical retirement identity or size"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct HistoryRemoval {
    pub token: RevisionToken,
    pub status: ConversionStatus,
    pub stage: Option<HistoryRemovalStage>,
    pub removed_bytes: u64,
}

/// Neutral object ownership shared by representation-specific manifests.
/// A history descriptor accepts only Source and Payload dependencies.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyKind {
    Source,
    Payload,
    Embedding,
    Model,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectDependency {
    pub kind: DependencyKind,
    pub digest: Sha256Digest,
    pub bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoredDigest {
    pub sha256: Sha256Digest,
    pub bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryMetadata {
    Inventory,
    Lineage,
    Manifest,
}

impl HistoryMetadata {
    pub fn file_name(self) -> &'static str {
        match self {
            Self::Inventory => "inventory.json",
            Self::Lineage => "lineage.json",
            Self::Manifest => "manifest.json",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryCapability {
    File,
    Context,
    Outline,
    Info,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryDescriptor {
    pub version: u32,
    pub publication: PublicationReceipt,
    /// The original publication's observation time, not conversion time.
    pub observed_unix_seconds: u64,
    pub capabilities: BTreeSet<HistoryCapability>,
    pub metadata: BTreeMap<HistoryMetadata, StoredDigest>,
    /// Sorted unique identity union, independent of the number of file aliases.
    pub dependencies: Vec<ObjectDependency>,
}

impl HistoryDescriptor {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.version == HISTORY_DESCRIPTOR_VERSION,
            "unsupported history descriptor"
        );
        ensure!(
            !self.capabilities.is_empty(),
            "history has no declared read capabilities"
        );
        ensure!(
            self.metadata.len() == 3
                && [
                    HistoryMetadata::Inventory,
                    HistoryMetadata::Lineage,
                    HistoryMetadata::Manifest
                ]
                .iter()
                .all(|kind| self.metadata.get(kind).is_some_and(|item| item.bytes > 0)),
            "incomplete history metadata binding"
        );
        ensure!(
            self.metadata[&HistoryMetadata::Inventory].sha256 == self.publication.inventory_digest
                && self.metadata[&HistoryMetadata::Manifest].sha256
                    == self.publication.manifest_digest,
            "history metadata differs from original publication receipt"
        );
        ensure!(
            self.metadata[&HistoryMetadata::Inventory].bytes
                <= airk_core::publication::MAX_INVENTORY_BYTES as u64
                && self.metadata[&HistoryMetadata::Lineage].bytes
                    <= airk_core::changes::MAX_LINEAGE_BYTES as u64
                && self.metadata[&HistoryMetadata::Manifest].bytes <= 131_072,
            "history metadata exceeds supported bounds"
        );
        ensure!(
            self.dependencies.len() <= MAX_HISTORY_DEPENDENCIES,
            "history dependency count exceeded"
        );
        ensure!(
            self.dependencies
                .iter()
                .all(|item| matches!(item.kind, DependencyKind::Source | DependencyKind::Payload)),
            "history cannot retain search or model dependencies"
        );
        ensure!(
            self.dependencies
                .windows(2)
                .all(|pair| (pair[0].kind, pair[0].digest) < (pair[1].kind, pair[1].digest)),
            "history dependencies must be sorted and unique"
        );
        ensure!(
            self.dependencies
                .iter()
                .all(|item| item.kind != DependencyKind::Payload || item.bytes > 0),
            "empty historical payload dependency"
        );
        self.dependency_bytes()?;
        Ok(())
    }

    pub fn dependency_bytes(&self) -> Result<u64> {
        self.dependencies.iter().try_fold(0u64, |total, item| {
            total
                .checked_add(item.bytes)
                .ok_or_else(|| anyhow::anyhow!("history dependency byte overflow"))
        })
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let bytes = serde_json::to_vec(self)?;
        ensure!(
            bytes.len() <= MAX_HISTORY_DESCRIPTOR_BYTES,
            "history descriptor exceeds byte bound"
        );
        Ok(bytes)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        ensure!(
            bytes.len() <= MAX_HISTORY_DESCRIPTOR_BYTES,
            "history descriptor exceeds byte bound"
        );
        let descriptor: Self = serde_json::from_slice(bytes)?;
        ensure!(
            descriptor.canonical_bytes()? == bytes,
            "noncanonical history descriptor"
        );
        Ok(descriptor)
    }

    pub fn binding_digest(&self) -> Result<StoredDigest> {
        let bytes = self.canonical_bytes()?;
        Ok(StoredDigest {
            sha256: Sha256Digest::from_bytes(Sha256::digest(&bytes).into()),
            bytes: bytes.len() as u64,
        })
    }
}

/// Stored transactionally beside, never instead of, the publication receipt.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryBinding {
    pub version: u32,
    pub publication: PublicationReceipt,
    pub descriptor: StoredDigest,
    pub intent_id: Sha256Digest,
}

impl HistoryBinding {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.version == HISTORY_BINDING_VERSION
                && self.descriptor.bytes > 0
                && self.descriptor.bytes <= MAX_HISTORY_DESCRIPTOR_BYTES as u64,
            "invalid history binding"
        );
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversionStage {
    IntentDurable,
    PackageVerified,
    PackagePublished,
    BindingCommitted,
    FullDetached,
    FullRemoved,
    IntentComplete,
}

/// Paths are derived from validated identities; no stored arbitrary path input.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConversionIntent {
    pub version: u32,
    pub id: Sha256Digest,
    pub publication: PublicationReceipt,
    pub authorized_head: RevisionToken,
    pub descriptor: StoredDigest,
    pub stage: ConversionStage,
}

impl ConversionIntent {
    pub fn new(
        publication: PublicationReceipt,
        authorized_head: RevisionToken,
        descriptor: StoredDigest,
    ) -> Result<Self> {
        let id = Self::identity(&publication, &authorized_head, &descriptor)?;
        let intent = Self {
            version: HISTORY_INTENT_VERSION,
            id,
            publication,
            authorized_head,
            descriptor,
            stage: ConversionStage::IntentDurable,
        };
        intent.validate()?;
        Ok(intent)
    }

    fn identity(
        publication: &PublicationReceipt,
        authorized_head: &RevisionToken,
        descriptor: &StoredDigest,
    ) -> Result<Sha256Digest> {
        Ok(framed_digest(
            b"history-conversion/1",
            &[&serde_json::to_vec(&(
                publication,
                authorized_head,
                descriptor,
            ))?],
        ))
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.version == HISTORY_INTENT_VERSION,
            "unsupported history conversion intent"
        );
        self.publication
            .token
            .validate_against(&self.authorized_head)?;
        ensure!(
            self.publication.token != self.authorized_head,
            "cannot demote active publication"
        );
        ensure!(
            self.id == Self::identity(&self.publication, &self.authorized_head, &self.descriptor)?,
            "history intent identity mismatch"
        );
        ensure!(
            self.descriptor.bytes > 0
                && self.descriptor.bytes <= MAX_HISTORY_DESCRIPTOR_BYTES as u64,
            "invalid conversion descriptor bound"
        );
        ensure!(
            serde_json::to_vec(self)?.len() <= MAX_HISTORY_INTENT_BYTES,
            "history conversion intent exceeds bound"
        );
        Ok(())
    }
}

/// A capability-limited pin. It cannot be passed to the retrieval search engine.
pub struct HistoryCatalog {
    pub descriptor: HistoryDescriptor,
    pub inventory: airk_core::publication::InventoryV2,
    pub lineage: airk_core::changes::LineageEnvelope,
    pub snapshot: airk_core::Snapshot,
    store: crate::FileStore,
    _lease: File,
    _lifecycle: Option<Arc<File>>,
}

impl airk_core::ArtifactSource for HistoryCatalog {
    fn load_artifact(&self, path: &str) -> Result<airk_core::Artifact> {
        let artifact = self
            .snapshot
            .artifacts
            .get(path)
            .context("path absent from historical inventory")?;
        self.store.load_retained(artifact)
    }
}

impl HistoryCatalog {
    pub fn file(&self, path: &str) -> Option<&airk_core::publication::InventoryFile> {
        self.inventory.files.get(path)
    }

    pub fn source(&self, path: &str) -> Result<Vec<u8>> {
        let artifact = self
            .snapshot
            .artifacts
            .get(path)
            .context("path absent from historical inventory")?;
        self.store.read_retained_source(artifact)
    }
}

pub(crate) struct VerifiedHistory {
    pub descriptor: HistoryDescriptor,
    pub inventory: airk_core::publication::InventoryV2,
    pub lineage: airk_core::changes::LineageEnvelope,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OriginalManifest {
    schema: u32,
    engine: String,
    token: RevisionToken,
    collection: String,
    chunks: usize,
    vectors: usize,
    model_sha256: Option<String>,
    embedding: String,
    inventory_digest: Sha256Digest,
    components: BTreeMap<String, StoredDigest>,
}

impl OriginalManifest {
    fn supported_format(&self) -> bool {
        match self.schema {
            2 => {
                self.engine == "tantivy-0.26.2/diskann-0.59.0/airk-file-1/generation-4/inventory-2"
            }
            3 => {
                matches!(
                    self.engine.as_str(),
                    "tantivy-0.26.2/diskann-0.59.0/airk-file-1/generation-5/inventory-2/model-objects-1"
                        | "tantivy-0.26.2/diskann-0.59.0/airk-file-1/generation-5/inventory-2/model-objects-1/source-context-1"
                ) && !self
                    .components
                    .keys()
                    .any(|name| name.starts_with("model/"))
                    && self.components.contains_key("model-objects.json")
                        == self.model_sha256.is_some()
            }
            _ => false,
        }
    }
}

fn plain(path: &Path) -> Result<fs::Metadata> {
    let metadata = path.symlink_metadata()?;
    ensure!(
        !metadata.file_type().is_symlink(),
        "linked historical state"
    );
    Ok(metadata)
}

fn directory(store: &crate::FileStore, token: &RevisionToken) -> PathBuf {
    store
        .directory()
        .join("history")
        .join(token.revision.to_string())
}

fn plain_ancestors(store: &crate::FileStore, path: &Path) -> Result<()> {
    ensure!(
        plain(store.directory())?.is_dir(),
        "invalid historical collection root"
    );
    let relative = path
        .strip_prefix(store.directory())
        .context("history path escaped collection")?;
    let mut current = store.directory().to_path_buf();
    for component in relative.components() {
        ensure!(
            matches!(component, std::path::Component::Normal(_)),
            "invalid history path component"
        );
        current.push(component.as_os_str());
        ensure!(
            plain(&current)?.is_dir(),
            "invalid historical state ancestor"
        );
    }
    Ok(())
}

fn metadata_bytes(
    path: &Path,
    digest: &StoredDigest,
    charge: &mut dyn FnMut(u64) -> Result<()>,
) -> Result<Vec<u8>> {
    charge(digest.bytes)?;
    let metadata = plain(path)?;
    ensure!(
        metadata.is_file() && metadata.len() == digest.bytes,
        "historical metadata type or length mismatch"
    );
    crate::blobs::read_verified(path, &digest.sha256.to_string(), digest.bytes)
}

fn inventory_dependencies(
    inventory: &airk_core::publication::InventoryV2,
) -> Result<Vec<ObjectDependency>> {
    let mut union = BTreeMap::new();
    for file in inventory.files.values() {
        for (kind, digest, bytes) in std::iter::once((
            DependencyKind::Payload,
            file.state.payload_digest,
            file.payload_bytes as u64,
        ))
        .chain(file.raw_source.then_some((
            DependencyKind::Source,
            file.state.source_hash,
            file.source_bytes as u64,
        ))) {
            if let Some(old) = union.insert((kind, digest), bytes) {
                ensure!(old == bytes, "conflicting historical dependency lengths");
            }
            ensure!(
                union.len() <= MAX_HISTORY_DEPENDENCIES,
                "history dependency count exceeded"
            );
        }
    }
    Ok(union
        .into_iter()
        .map(|((kind, digest), bytes)| ObjectDependency {
            kind,
            digest,
            bytes,
        })
        .collect())
}

fn verify_dependencies(
    store: &crate::FileStore,
    descriptor: &HistoryDescriptor,
    charge: &mut dyn FnMut(u64) -> Result<()>,
) -> Result<()> {
    use std::io::Read;
    if descriptor.dependencies.is_empty() {
        return Ok(());
    }
    ensure!(
        plain(&store.directory().join("objects"))?.is_dir(),
        "invalid historical object root"
    );
    for dependency in &descriptor.dependencies {
        charge(dependency.bytes)?;
        let kind = match dependency.kind {
            DependencyKind::Source => "sources",
            DependencyKind::Payload => "artifacts",
            _ => anyhow::bail!("unsupported history dependency"),
        };
        let root = store.directory().join("objects").join(kind);
        ensure!(plain(&root)?.is_dir(), "invalid historical dependency root");
        let path = root.join(dependency.digest.to_string());
        ensure!(
            plain(&path)?.is_file(),
            "invalid historical dependency type"
        );
        let mut file = File::open(&path)?;
        ensure!(
            file.metadata()?.len() == dependency.bytes,
            "historical dependency length mismatch"
        );
        let mut hash = Sha256::new();
        let mut remaining = dependency.bytes;
        let mut buffer = [0u8; 65536];
        while remaining > 0 {
            charge(0)?;
            let length = remaining.min(buffer.len() as u64) as usize;
            let read = file.read(&mut buffer[..length])?;
            ensure!(read > 0, "historical dependency truncated");
            hash.update(&buffer[..read]);
            remaining -= read as u64;
        }
        ensure!(
            file.metadata()?.len() == dependency.bytes
                && Sha256Digest::from_bytes(hash.finalize().into()) == dependency.digest,
            "historical dependency digest mismatch"
        );
    }
    Ok(())
}

pub(crate) fn verify_package(
    store: &crate::FileStore,
    package: &Path,
    binding: &HistoryBinding,
    charge: &mut dyn FnMut(u64) -> Result<()>,
) -> Result<VerifiedHistory> {
    let verified = verify_package_metadata(store, package, binding, charge)?;
    verify_dependencies(store, &verified.descriptor, charge)?;
    Ok(verified)
}

pub(crate) fn verify_package_work(
    store: &crate::FileStore,
    package: &Path,
    binding: &HistoryBinding,
    work: &mut crate::maintenance::WorkBudget,
) -> Result<VerifiedHistory> {
    let verified =
        verify_package_metadata(store, package, binding, &mut |bytes| work.metadata(bytes))?;
    verify_dependencies(store, &verified.descriptor, &mut |bytes| work.verify(bytes))?;
    Ok(verified)
}

fn verify_package_metadata(
    store: &crate::FileStore,
    package: &Path,
    binding: &HistoryBinding,
    charge: &mut dyn FnMut(u64) -> Result<()>,
) -> Result<VerifiedHistory> {
    binding.validate()?;
    plain_ancestors(
        store,
        package
            .parent()
            .context("historical package parent missing")?,
    )?;
    ensure!(
        plain(package)?.is_dir(),
        "historical package is not a directory"
    );
    let mut names = BTreeSet::new();
    for entry in fs::read_dir(package)? {
        let entry = entry?;
        ensure!(
            entry.file_type()?.is_file() && !entry.file_type()?.is_symlink(),
            "invalid historical package entry"
        );
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| anyhow::anyhow!("non-UTF8 history entry"))?;
        ensure!(
            [
                "descriptor.json",
                "inventory.json",
                "lineage.json",
                "manifest.json"
            ]
            .contains(&name.as_str()),
            "undeclared historical package entry"
        );
        names.insert(name);
        ensure!(names.len() <= 4, "historical package entry limit");
    }
    ensure!(names.len() == 4, "historical package incomplete");
    let descriptor = HistoryDescriptor::from_bytes(&metadata_bytes(
        &package.join("descriptor.json"),
        &binding.descriptor,
        charge,
    )?)?;
    ensure!(
        descriptor.publication == binding.publication,
        "historical publication receipt mismatch"
    );
    let inventory_bytes = metadata_bytes(
        &package.join("inventory.json"),
        &descriptor.metadata[&HistoryMetadata::Inventory],
        charge,
    )?;
    let inventory: airk_core::publication::InventoryV2 = serde_json::from_slice(&inventory_bytes)?;
    inventory.validate()?;
    let token = descriptor.publication.token;
    ensure!(
        inventory.incarnation == token.incarnation
            && inventory.generation == token.generation
            && airk_core::identity::revision_digest(
                inventory.retrieval_specification.as_bytes(),
                &inventory_bytes
            ) == token.revision,
        "historical inventory revision mismatch"
    );
    ensure!(
        store.directory().file_name().and_then(|name| name.to_str())
            == Some(inventory.collection.as_str()),
        "foreign historical collection"
    );
    ensure!(
        descriptor.dependencies == inventory_dependencies(&inventory)?,
        "historical dependencies differ from verified inventory"
    );
    let lineage_bytes = metadata_bytes(
        &package.join("lineage.json"),
        &descriptor.metadata[&HistoryMetadata::Lineage],
        charge,
    )?;
    ensure!(
        Sha256Digest::from_bytes(Sha256::digest(&lineage_bytes).into()) == inventory.lineage_digest,
        "historical lineage digest mismatch"
    );
    let lineage: airk_core::changes::LineageEnvelope = serde_json::from_slice(&lineage_bytes)?;
    lineage.validate()?;
    ensure!(
        lineage.incarnation == token.incarnation
            && lineage.observation.generation == token.generation
            && lineage.observation.observed_unix_seconds == descriptor.observed_unix_seconds,
        "historical observation scope mismatch"
    );
    let manifest: OriginalManifest = serde_json::from_slice(&metadata_bytes(
        &package.join("manifest.json"),
        &descriptor.metadata[&HistoryMetadata::Manifest],
        charge,
    )?)?;
    ensure!(
        manifest.supported_format()
            && manifest.token == token
            && manifest.inventory_digest == descriptor.publication.inventory_digest
            && manifest.engine == inventory.retrieval_specification
            && manifest.collection == inventory.collection
            && manifest.components.len() <= 1024
            && manifest.embedding.len() <= 256
            && Some(manifest.chunks)
                == inventory
                    .files
                    .values()
                    .try_fold(0usize, |sum, file| sum.checked_add(file.chunks))
            && manifest.vectors <= manifest.chunks
            && manifest.model_sha256 == inventory.embedding_model,
        "historical original manifest scope mismatch"
    );
    for kind in [HistoryMetadata::Inventory, HistoryMetadata::Lineage] {
        ensure!(
            manifest.components.get(kind.file_name()) == descriptor.metadata.get(&kind),
            "historical copied component differs from original manifest"
        );
    }
    Ok(VerifiedHistory {
        descriptor,
        inventory,
        lineage,
    })
}

/// Caller holds the writer or collection lifecycle exclusion while traversing roots.
pub(crate) fn verified_roots_work(
    store: &crate::FileStore,
    work: &mut crate::maintenance::WorkBudget,
) -> Result<Vec<VerifiedHistory>> {
    root_bindings(store)?
        .into_iter()
        .map(|binding| {
            verify_package_work(
                store,
                &directory(store, &binding.publication.token),
                &binding,
                work,
            )
        })
        .collect()
}

fn root_bindings(store: &crate::FileStore) -> Result<Vec<HistoryBinding>> {
    let bindings = crate::sqlite::history_bindings(store.directory())?;
    let root = store.directory().join("history");
    if !root.exists() {
        ensure!(bindings.is_empty(), "committed history root missing");
        return Ok(vec![]);
    }
    ensure!(plain(&root)?.is_dir(), "invalid history root");
    let expected = bindings
        .iter()
        .map(|binding| binding.publication.token.revision.to_string())
        .collect::<BTreeSet<_>>();
    let mut found = BTreeSet::new();
    for entry in fs::read_dir(&root)? {
        let entry = entry?;
        ensure!(
            entry.file_type()?.is_dir() && !entry.file_type()?.is_symlink(),
            "invalid history root entry"
        );
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| anyhow::anyhow!("invalid history root name"))?;
        ensure!(
            expected.contains(&name),
            "uncommitted history package prevents root traversal"
        );
        found.insert(name);
    }
    ensure!(found == expected, "committed history package missing");
    Ok(bindings)
}

pub(crate) fn lease_tokens(store: &crate::FileStore) -> Result<Vec<RevisionToken>> {
    if !store.directory().join("index.sqlite3").exists() {
        ensure!(
            pending_count(store)? == 0 && !store.directory().join("history").exists(),
            "historical state lacks root authority"
        );
        return Ok(vec![]);
    }
    let mut tokens = crate::sqlite::history_bindings(store.directory())?
        .into_iter()
        .map(|binding| binding.publication.token)
        .collect::<Vec<_>>();
    for token in intents(store)?
        .into_iter()
        .map(|intent| intent.publication.token)
    {
        if !tokens.contains(&token) {
            tokens.push(token);
        }
    }
    for intent in removal_intents(store)? {
        let token = intent.binding.publication.token;
        if !tokens.contains(&token) {
            tokens.push(token);
        }
    }
    if crate::sqlite::persistent_revision(store.directory())?
        .and_then(|head| head.token)
        .is_some()
    {
        for checkpoint in crate::sqlite::history_admin(store.directory())?
            .checkpoints
            .values()
        {
            if !tokens.contains(&checkpoint.publication.token) {
                tokens.push(checkpoint.publication.token);
            }
        }
    }
    Ok(tokens)
}

impl crate::FileStore {
    /// Exact history only. Reads create neither packages nor missing lease files.
    pub fn history_at(
        &self,
        token: &RevisionToken,
        maximum: u64,
    ) -> Result<Option<HistoryCatalog>> {
        if !self.directory().join("index.sqlite3").exists() {
            return Ok(None);
        }
        let lifecycle = self.lifecycle(false)?;
        let Some(current) = self.persistent_revision()?.and_then(|head| head.token) else {
            return Ok(None);
        };
        token.validate_against(&current)?;
        crate::sqlite::validate_retained_token(self.directory(), token)?;
        let path = crate::blobs::reader_lease_path(self.directory(), &token.revision.to_string())?;
        let lease = match File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                ensure!(
                    self.receipt(token)?.is_none()
                        && crate::sqlite::history_binding(self.directory(), token)?.is_none(),
                    "retained history lease missing"
                );
                return Ok(None);
            }
            Err(error) => return Err(error.into()),
        };
        lease.lock_shared()?;
        let Some(binding) = crate::sqlite::history_binding(self.directory(), token)? else {
            ensure!(
                !directory(self, token).exists() || conversion_intent(self, token)?.is_some(),
                "unbound historical package without conversion intent"
            );
            return Ok(None);
        };
        ensure!(
            self.receipt(token)?.as_ref() == Some(&binding.publication),
            "historical publication receipt missing or changed"
        );
        let root = self.directory().join("history");
        ensure!(plain(&root)?.is_dir(), "invalid historical root");
        let mut charged = 0u64;
        if retired_package(self, &binding, &mut |bytes| {
            charged = charged
                .checked_add(bytes)
                .context("historical read budget overflow")?;
            ensure!(
                charged <= maximum,
                "historical metadata exceeds request budget"
            );
            Ok(())
        })? {
            return Ok(None);
        }
        let verified = verify_package(self, &directory(self, token), &binding, &mut |bytes| {
            charged = charged
                .checked_add(bytes)
                .context("historical read budget overflow")?;
            ensure!(
                charged <= maximum,
                "historical metadata exceeds request budget"
            );
            Ok(())
        })?;
        let mut snapshot = verified.inventory.projection()?;
        snapshot.persistent_indexes = false;
        Ok(Some(HistoryCatalog {
            descriptor: verified.descriptor,
            inventory: verified.inventory,
            lineage: verified.lineage,
            snapshot,
            store: self.clone(),
            _lease: lease,
            _lifecycle: lifecycle,
        }))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversionStatus {
    Complete,
    Pending,
    Protected,
}

#[derive(Clone, Debug, Serialize)]
pub struct HistoryConversion {
    pub token: RevisionToken,
    pub status: ConversionStatus,
    pub stage: Option<ConversionStage>,
    pub removed_bytes: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct HistoryRecovery {
    pub conversions: Vec<HistoryConversion>,
    pub removals: Vec<HistoryRemoval>,
    pub pending: bool,
}

fn intent_path(store: &crate::FileStore, token: &RevisionToken) -> PathBuf {
    store
        .directory()
        .join("maintenance/history-intents")
        .join(format!("{}.json", token.revision))
}

fn staging_path(store: &crate::FileStore, intent: &ConversionIntent) -> PathBuf {
    store
        .directory()
        .join("maintenance/history-staging")
        .join(intent.id.to_string())
}

fn detached_path(store: &crate::FileStore, token: &RevisionToken) -> PathBuf {
    store
        .directory()
        .join("generations")
        .join(format!("history-removing-{}", token.revision))
}

fn ensure_directory(store: &crate::FileStore, path: &Path) -> Result<()> {
    let mut current = store.directory().to_path_buf();
    ensure!(plain(&current)?.is_dir(), "invalid collection root");
    for part in path.strip_prefix(store.directory())?.components() {
        ensure!(
            matches!(part, std::path::Component::Normal(_)),
            "invalid history directory component"
        );
        current.push(part);
        if !current.exists() {
            fs::create_dir(&current)?;
            crate::maintenance::sync_parent(current.parent().unwrap())?;
        }
        ensure!(plain(&current)?.is_dir(), "invalid historical directory");
    }
    Ok(())
}

pub(crate) fn conversion_intent(
    store: &crate::FileStore,
    token: &RevisionToken,
) -> Result<Option<ConversionIntent>> {
    let parent = store.directory().join("maintenance/history-intents");
    if !parent.exists() {
        return Ok(None);
    }
    ensure!(
        plain(&store.directory().join("maintenance"))?.is_dir() && plain(&parent)?.is_dir(),
        "invalid history intent root"
    );
    let path = intent_path(store, token);
    let metadata = match path.symlink_metadata() {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    ensure!(
        metadata.is_file()
            && !metadata.file_type().is_symlink()
            && metadata.len() <= MAX_HISTORY_INTENT_BYTES as u64,
        "invalid history conversion intent file"
    );
    use std::io::Read;
    let mut bytes = Vec::new();
    File::open(path)?
        .take(MAX_HISTORY_INTENT_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= MAX_HISTORY_INTENT_BYTES,
        "history intent exceeded byte bound"
    );
    let intent: ConversionIntent = serde_json::from_slice(&bytes)?;
    intent.validate()?;
    ensure!(
        intent.publication.token == *token,
        "foreign history conversion intent"
    );
    Ok(Some(intent))
}

pub(crate) fn intents(store: &crate::FileStore) -> Result<Vec<ConversionIntent>> {
    let path = store.directory().join("maintenance/history-intents");
    if !path.exists() {
        return Ok(vec![]);
    }
    ensure!(
        plain(&store.directory().join("maintenance"))?.is_dir() && plain(&path)?.is_dir(),
        "invalid history intent directory"
    );
    let mut intents = Vec::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        ensure!(
            entry.file_type()?.is_file() && !entry.file_type()?.is_symlink(),
            "invalid history intent entry"
        );
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| anyhow::anyhow!("non-UTF8 history intent name"))?;
        let revision = name
            .strip_suffix(".json")
            .context("unknown history intent entry")?;
        ensure!(
            crate::blobs::valid_digest(revision),
            "invalid history intent identity"
        );
        let metadata = entry.metadata()?;
        ensure!(
            metadata.len() <= MAX_HISTORY_INTENT_BYTES as u64,
            "history intent byte limit exceeded"
        );
        let mut bytes = Vec::new();
        use std::io::Read;
        File::open(entry.path())?
            .take(MAX_HISTORY_INTENT_BYTES as u64 + 1)
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() <= MAX_HISTORY_INTENT_BYTES,
            "history intent byte limit exceeded"
        );
        let intent: ConversionIntent = serde_json::from_slice(&bytes)?;
        intent.validate()?;
        ensure!(
            intent.publication.token.revision.to_string() == revision,
            "history intent filename mismatch"
        );
        intents.push(intent);
        ensure!(
            intents.len() <= MAX_HISTORY_INTENTS,
            "history intent count exceeded"
        );
    }
    intents.sort_by_key(|intent| intent.publication.token.generation);
    Ok(intents)
}

fn persist_intent(
    store: &crate::FileStore,
    intent: &ConversionIntent,
    reservation: &crate::quota::Reservation,
    observer: &dyn Fn(ConversionStage, &ConversionIntent) -> Result<()>,
) -> Result<()> {
    intent.validate()?;
    crate::maintenance::write_json(
        store,
        &intent_path(store, &intent.publication.token),
        intent,
        reservation,
    )?;
    observer(intent.stage, intent)
}

fn descriptor_from_full(
    store: &crate::FileStore,
    receipt: &PublicationReceipt,
    work: &mut crate::maintenance::WorkBudget,
) -> Result<HistoryDescriptor> {
    let source = store
        .directory()
        .join("generations")
        .join(receipt.token.revision.to_string());
    let mut metadata = BTreeMap::new();
    for kind in [
        HistoryMetadata::Inventory,
        HistoryMetadata::Lineage,
        HistoryMetadata::Manifest,
    ] {
        let path = source.join(kind.file_name());
        let bytes = plain(&path)?.len();
        work.metadata(bytes)?;
        metadata.insert(
            kind,
            StoredDigest {
                sha256: Sha256Digest::from_bytes([0; 32]),
                bytes,
            },
        );
    }
    let inventory = verify_full_bounded(store, receipt, work)?;
    metadata
        .get_mut(&HistoryMetadata::Inventory)
        .unwrap()
        .sha256 = receipt.inventory_digest;
    metadata.get_mut(&HistoryMetadata::Manifest).unwrap().sha256 = receipt.manifest_digest;
    metadata.get_mut(&HistoryMetadata::Lineage).unwrap().sha256 = inventory.lineage_digest;
    let lineage: airk_core::changes::LineageEnvelope = serde_json::from_slice(&metadata_bytes(
        &source.join("lineage.json"),
        &metadata[&HistoryMetadata::Lineage],
        &mut |bytes| work.metadata(bytes),
    )?)?;
    lineage.validate()?;
    ensure!(
        lineage.incarnation == receipt.token.incarnation
            && lineage.observation.generation == receipt.token.generation,
        "history candidate lineage scope mismatch"
    );
    let descriptor = HistoryDescriptor {
        version: HISTORY_DESCRIPTOR_VERSION,
        publication: receipt.clone(),
        observed_unix_seconds: lineage.observation.observed_unix_seconds,
        capabilities: BTreeSet::from([
            HistoryCapability::File,
            HistoryCapability::Context,
            HistoryCapability::Outline,
            HistoryCapability::Info,
        ]),
        metadata,
        dependencies: inventory_dependencies(&inventory)?,
    };
    descriptor.validate()?;
    Ok(descriptor)
}

fn write_component(
    store: &crate::FileStore,
    path: &Path,
    bytes: &[u8],
    reservation: &crate::quota::Reservation,
    replace_private: bool,
) -> Result<()> {
    use std::io::Write;
    ensure!(
        path.starts_with(store.directory()),
        "history output escaped collection"
    );
    if path.exists() {
        ensure!(plain(path)?.is_file(), "invalid history output type");
        let expected = Sha256Digest::from_bytes(Sha256::digest(bytes).into());
        if crate::blobs::read_verified(path, &expected.to_string(), bytes.len() as u64).is_ok() {
            return Ok(());
        }
        ensure!(replace_private, "verified history component changed");
        fs::remove_file(path)?;
    }
    let destination = path
        .parent()
        .context("history component parent missing")?
        .canonicalize()?
        .join(
            path.file_name()
                .context("history component filename missing")?,
        );
    let mut file = reservation.create_file(destination)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    crate::maintenance::sync_parent(path.parent().unwrap())
}

fn converted_binding(intent: &ConversionIntent) -> HistoryBinding {
    HistoryBinding {
        version: HISTORY_BINDING_VERSION,
        publication: intent.publication.clone(),
        descriptor: intent.descriptor.clone(),
        intent_id: intent.id,
    }
}

#[allow(clippy::too_many_arguments)]
fn process_conversion(
    store: &crate::FileStore,
    mut intent: ConversionIntent,
    descriptor: Option<HistoryDescriptor>,
    head: RevisionToken,
    reservation: &crate::quota::Reservation,
    work: &mut crate::maintenance::WorkBudget,
    observer: &dyn Fn(ConversionStage, &ConversionIntent) -> Result<()>,
) -> Result<HistoryConversion> {
    let token = intent.publication.token;
    intent.validate()?;
    intent.authorized_head.validate_against(&head)?;
    ensure!(
        store.receipt(&token)?.as_ref() == Some(&intent.publication),
        "history conversion receipt changed"
    );
    let source = store
        .directory()
        .join("generations")
        .join(token.revision.to_string());
    let package = directory(store, &token);
    let staging = staging_path(store, &intent);
    let detached = detached_path(store, &token);
    let binding = converted_binding(&intent);
    let outcome = |status, stage, work: &crate::maintenance::WorkBudget| HistoryConversion {
        token,
        status,
        stage: Some(stage),
        removed_bytes: work.removed_bytes(),
    };
    if work.expired() {
        return Ok(outcome(ConversionStatus::Pending, intent.stage, work));
    }
    if !package.exists() {
        ensure!(
            !matches!(
                intent.stage,
                ConversionStage::PackagePublished
                    | ConversionStage::BindingCommitted
                    | ConversionStage::FullDetached
                    | ConversionStage::FullRemoved
                    | ConversionStage::IntentComplete
            ),
            "published historical package disappeared"
        );
        let descriptor = match descriptor {
            Some(value) => value,
            None => descriptor_from_full(store, &intent.publication, work)?,
        };
        ensure!(
            descriptor.binding_digest()? == intent.descriptor,
            "history candidate changed during conversion"
        );
        ensure_directory(store, &staging)?;
        for entry in fs::read_dir(&staging)? {
            let entry = entry?;
            ensure!(
                entry.file_type()?.is_file()
                    && !entry.file_type()?.is_symlink()
                    && [
                        "descriptor.json",
                        "inventory.json",
                        "lineage.json",
                        "manifest.json"
                    ]
                    .iter()
                    .any(|name| entry.file_name() == *name),
                "unknown private historical component"
            );
        }
        for (kind, digest) in &descriptor.metadata {
            if work.expired() {
                return Ok(outcome(ConversionStatus::Pending, intent.stage, work));
            }
            let bytes = metadata_bytes(&source.join(kind.file_name()), digest, &mut |bytes| {
                work.metadata(bytes)
            })?;
            write_component(
                store,
                &staging.join(kind.file_name()),
                &bytes,
                reservation,
                intent.stage == ConversionStage::IntentDurable,
            )?;
        }
        write_component(
            store,
            &staging.join("descriptor.json"),
            &descriptor.canonical_bytes()?,
            reservation,
            intent.stage == ConversionStage::IntentDurable,
        )?;
        verify_package_work(store, &staging, &binding, work)?;
        crate::maintenance::sync_parent(&staging)?;
        intent.stage = ConversionStage::PackageVerified;
        persist_intent(store, &intent, reservation, observer)?;
        ensure_directory(store, package.parent().unwrap())?;
        fs::rename(&staging, &package)?;
        crate::maintenance::sync_parent(package.parent().unwrap())?;
        crate::maintenance::sync_parent(staging.parent().unwrap())?;
        intent.stage = ConversionStage::PackagePublished;
        persist_intent(store, &intent, reservation, observer)?;
    }
    verify_package_work(store, &package, &binding, work)?;
    if work.expired() {
        return Ok(outcome(ConversionStatus::Pending, intent.stage, work));
    }
    crate::sqlite::commit_history_binding(store.directory(), &binding, &head, reservation)?;
    if !matches!(
        intent.stage,
        ConversionStage::FullDetached
            | ConversionStage::FullRemoved
            | ConversionStage::IntentComplete
    ) {
        intent.stage = ConversionStage::BindingCommitted;
        persist_intent(store, &intent, reservation, observer)?;
    }
    ensure!(
        !(source.exists() && detached.exists()),
        "history conversion has two full representations"
    );
    if source.exists() {
        verify_full_bounded(store, &intent.publication, work)?;
        fs::rename(&source, &detached)?;
        crate::maintenance::sync_parent(source.parent().unwrap())?;
        intent.stage = ConversionStage::FullDetached;
        persist_intent(store, &intent, reservation, observer)?;
    }
    if detached.exists() {
        if !crate::maintenance::delete_verified_remaining(
            store,
            &detached,
            &package.join("manifest.json"),
            &intent.publication,
            work,
        )? {
            return Ok(outcome(ConversionStatus::Pending, intent.stage, work));
        }
    } else {
        ensure!(
            matches!(
                intent.stage,
                ConversionStage::FullDetached
                    | ConversionStage::FullRemoved
                    | ConversionStage::IntentComplete
            ),
            "full representation disappeared before authorized detach"
        );
    }
    intent.stage = ConversionStage::FullRemoved;
    persist_intent(store, &intent, reservation, observer)?;
    intent.stage = ConversionStage::IntentComplete;
    persist_intent(store, &intent, reservation, observer)?;
    fs::remove_file(intent_path(store, &token))?;
    crate::maintenance::sync_parent(intent_path(store, &token).parent().unwrap())?;
    Ok(outcome(ConversionStatus::Complete, intent.stage, work))
}

impl crate::FileStore {
    pub fn history_convert(
        &self,
        writer: &crate::WriterLock,
        quota: &crate::quota::Quota,
        token: &RevisionToken,
        budget: crate::retention::MaintenanceBudget,
    ) -> Result<HistoryConversion> {
        self.history_convert_with_observer(writer, quota, token, budget, &|_, _| Ok(()))
    }

    pub fn history_convert_metered(
        &self,
        writer: &crate::WriterLock,
        quota: &crate::quota::Quota,
        token: &RevisionToken,
        budget: crate::history_policy::HistoryWorkBudget,
        observer: &dyn Fn(ConversionStage, &ConversionIntent) -> Result<()>,
    ) -> Result<HistoryConversion> {
        crate::maintenance::check_writer(self, writer, quota)?;
        ensure!(
            !self.directory().join("checkpoints").exists(),
            "checkpoint protections require qualified history policy adapter"
        );
        let mut work = crate::maintenance::WorkBudget::history(budget)?;
        let budget = budget.maintenance;
        let current = self
            .catalog()?
            .context("history requires an indexed collection")?;
        let head = current
            .receipt
            .as_ref()
            .context("history requires v2 publication")?
            .token;
        token.validate_against(&head)?;
        ensure!(
            *token != head
                && current
                    .lineage
                    .as_ref()
                    .and_then(|lineage| lineage.observation.parent)
                    != Some(*token),
            "active/searchable predecessor cannot be demoted"
        );
        crate::sqlite::validate_retained_token(self.directory(), token)?;
        let source = self
            .directory()
            .join("generations")
            .join(token.revision.to_string());
        let Some(_lease) = crate::retention::exclusive(&source)? else {
            return Ok(HistoryConversion {
                token: *token,
                status: ConversionStatus::Protected,
                stage: None,
                removed_bytes: 0,
            });
        };
        if let Some(binding) = crate::sqlite::history_binding(self.directory(), token)? {
            verify_package_work(self, &directory(self, token), &binding, &mut work)?;
            if conversion_intent(self, token)?.is_none() {
                ensure!(
                    !source.exists() && !detached_path(self, token).exists(),
                    "history binding has unowned remaining full representation"
                );
                return Ok(HistoryConversion {
                    token: *token,
                    status: ConversionStatus::Complete,
                    stage: Some(ConversionStage::IntentComplete),
                    removed_bytes: 0,
                });
            }
        }
        let existing = conversion_intent(self, token)?;
        let descriptor = if existing.is_none() {
            let receipt = self
                .receipt(token)?
                .context("historical candidate unavailable")?;
            Some(descriptor_from_full(self, &receipt, &mut work)?)
        } else {
            None
        };
        let reservation =
            quota.reserve_available(&current.snapshot.collection, budget.max_metadata_bytes)?;
        let intent = if let Some(intent) = existing {
            intent
        } else {
            ensure!(
                intents(self)?.len() < MAX_HISTORY_INTENTS,
                "history intent limit reached"
            );
            let descriptor = descriptor.as_ref().unwrap();
            let intent = ConversionIntent::new(
                descriptor.publication.clone(),
                head,
                descriptor.binding_digest()?,
            )?;
            persist_intent(self, &intent, &reservation, observer)?;
            intent
        };
        process_conversion(
            self,
            intent,
            descriptor,
            head,
            &reservation,
            &mut work,
            observer,
        )
    }

    pub fn history_recover(
        &self,
        writer: &crate::WriterLock,
        quota: &crate::quota::Quota,
        budget: crate::retention::MaintenanceBudget,
    ) -> Result<HistoryRecovery> {
        crate::maintenance::check_writer(self, writer, quota)?;
        let mut work = crate::maintenance::WorkBudget::new(budget)?;
        let current = self
            .catalog()?
            .context("history recovery requires indexed collection")?;
        let head = current
            .receipt
            .as_ref()
            .context("history recovery requires v2 publication")?
            .token;
        let predecessor = current
            .lineage
            .as_ref()
            .and_then(|lineage| lineage.observation.parent);
        let reservation =
            quota.reserve_available(&current.snapshot.collection, budget.max_metadata_bytes)?;
        recover_conversions(
            self,
            head,
            predecessor,
            &reservation,
            &mut work,
            budget.max_candidates,
        )
    }
}

pub(crate) fn recover_conversions(
    store: &crate::FileStore,
    head: RevisionToken,
    predecessor: Option<RevisionToken>,
    reservation: &crate::quota::Reservation,
    work: &mut crate::maintenance::WorkBudget,
    maximum: usize,
) -> Result<HistoryRecovery> {
    let mut result = HistoryRecovery {
        conversions: vec![],
        removals: vec![],
        pending: false,
    };
    for intent in intents(store)? {
        if result.conversions.len() >= maximum || work.expired() {
            result.pending = true;
            break;
        }
        let token = intent.publication.token;
        ensure!(
            token != head && Some(token) != predecessor,
            "history intent targets protected searchable revision"
        );
        let source = store
            .directory()
            .join("generations")
            .join(token.revision.to_string());
        let outcome = if let Some(_lease) = crate::retention::exclusive(&source)? {
            process_conversion(store, intent, None, head, reservation, work, &|_, _| Ok(()))?
        } else {
            HistoryConversion {
                token,
                status: ConversionStatus::Protected,
                stage: Some(intent.stage),
                removed_bytes: 0,
            }
        };
        result.pending |= outcome.status != ConversionStatus::Complete;
        result.conversions.push(outcome);
    }
    for intent in removal_intents(store)? {
        if result.conversions.len() + result.removals.len() >= maximum || work.expired() {
            result.pending = true;
            break;
        }
        let token = intent.binding.publication.token;
        ensure!(
            token != head && Some(token) != predecessor,
            "history retirement targets protected revision"
        );
        let path = store
            .directory()
            .join("generations")
            .join(token.revision.to_string());
        let outcome = if let Some(_lease) = crate::retention::exclusive(&path)? {
            process_removal(store, intent, head, reservation, work, &|_, _| Ok(()))?
        } else {
            HistoryRemoval {
                token,
                status: ConversionStatus::Protected,
                stage: Some(intent.stage),
                removed_bytes: 0,
            }
        };
        result.pending |= outcome.status != ConversionStatus::Complete;
        result.removals.push(outcome);
    }
    Ok(result)
}

fn removal_path(store: &crate::FileStore, token: &RevisionToken) -> PathBuf {
    store
        .directory()
        .join("maintenance/history-retirements")
        .join(format!("{}.json", token.revision))
}
fn removal_witness(store: &crate::FileStore, token: &RevisionToken) -> PathBuf {
    store
        .directory()
        .join("maintenance/history-descriptors")
        .join(format!("{}.json", token.revision))
}
fn removal_directory(store: &crate::FileStore, intent: &HistoryRemovalIntent) -> PathBuf {
    store
        .directory()
        .join("history")
        .join(format!("removing-{}", intent.id))
}
pub(crate) fn removal_intents(store: &crate::FileStore) -> Result<Vec<HistoryRemovalIntent>> {
    use std::io::Read;
    let root = store.directory().join("maintenance/history-retirements");
    if !root.exists() {
        return Ok(vec![]);
    }
    ensure!(
        plain(&store.directory().join("maintenance"))?.is_dir() && plain(&root)?.is_dir(),
        "invalid history retirement root"
    );
    let mut intents = vec![];
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        ensure!(
            entry.file_type()?.is_file() && !entry.file_type()?.is_symlink(),
            "invalid history retirement entry"
        );
        let mut bytes = vec![];
        File::open(entry.path())?
            .take(MAX_HISTORY_INTENT_BYTES as u64 + 1)
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() <= MAX_HISTORY_INTENT_BYTES,
            "history retirement exceeds bound"
        );
        let intent: HistoryRemovalIntent = serde_json::from_slice(&bytes)?;
        intent.validate()?;
        ensure!(
            entry.path() == removal_path(store, &intent.binding.publication.token),
            "history retirement filename mismatch"
        );
        intents.push(intent);
        ensure!(
            intents.len() <= MAX_HISTORY_INTENTS,
            "history retirement count exceeded"
        );
    }
    Ok(intents)
}
fn persist_removal(
    store: &crate::FileStore,
    intent: &HistoryRemovalIntent,
    reservation: &crate::quota::Reservation,
    observer: &dyn Fn(HistoryRemovalStage, &HistoryRemovalIntent) -> Result<()>,
) -> Result<()> {
    intent.validate()?;
    crate::maintenance::write_json(
        store,
        &removal_path(store, &intent.binding.publication.token),
        intent,
        reservation,
    )?;
    observer(intent.stage, intent)
}

fn process_removal(
    store: &crate::FileStore,
    mut intent: HistoryRemovalIntent,
    head: RevisionToken,
    reservation: &crate::quota::Reservation,
    work: &mut crate::maintenance::WorkBudget,
    observer: &dyn Fn(HistoryRemovalStage, &HistoryRemovalIntent) -> Result<()>,
) -> Result<HistoryRemoval> {
    intent.validate()?;
    intent.authorized_head.validate_against(&head)?;
    let token = intent.binding.publication.token;
    ensure!(
        !crate::checkpoints::protected(store, &token)?,
        "checkpoint history cannot be retired"
    );
    let package = directory(store, &token);
    let detached = removal_directory(store, &intent);
    let witness = removal_witness(store, &token);
    plain_ancestors(store, package.parent().unwrap())?;
    let result = |status, stage, work: &crate::maintenance::WorkBudget| HistoryRemoval {
        token,
        status,
        stage: Some(stage),
        removed_bytes: work.removed_bytes(),
    };
    ensure!(
        !store
            .directory()
            .join("generations")
            .join(token.revision.to_string())
            .exists()
            && conversion_intent(store, &token)?.is_none(),
        "historical retirement conflicts with full representation"
    );
    if work.expired() {
        return Ok(result(ConversionStatus::Pending, intent.stage, work));
    }
    if let Some(proof) = crate::sqlite::retirement_proof(store.directory(), &token)? {
        ensure!(
            proof.intent_id == intent.id && !package.exists() && !detached.exists(),
            "historical retirement proof conflicts with representation"
        );
    } else {
        ensure!(
            crate::sqlite::history_binding(store.directory(), &token)?.as_ref()
                == Some(&intent.binding)
                && store.receipt(&token)?.as_ref() == Some(&intent.binding.publication),
            "historical retirement authority changed"
        );
        if intent.stage == HistoryRemovalStage::IntentDurable && package.exists() {
            let verified = verify_package_work(store, &package, &intent.binding, work)?;
            ensure_directory(store, witness.parent().unwrap())?;
            write_component(
                store,
                &witness,
                &verified.descriptor.canonical_bytes()?,
                reservation,
                true,
            )?;
        }
        plain_ancestors(store, witness.parent().unwrap())?;
        let descriptor = HistoryDescriptor::from_bytes(&metadata_bytes(
            &witness,
            &intent.binding.descriptor,
            &mut |bytes| work.metadata(bytes),
        )?)?;
        ensure!(
            descriptor.publication == intent.binding.publication,
            "retirement witness publication mismatch"
        );
        ensure!(
            !(package.exists() && detached.exists()),
            "duplicate historical retirement representation"
        );
        if package.exists() {
            ensure!(
                intent.stage == HistoryRemovalStage::IntentDurable,
                "historical package reappeared after detachment"
            );
            verify_package_work(store, &package, &intent.binding, work)?;
            fs::rename(&package, &detached)?;
            crate::maintenance::sync_parent(package.parent().unwrap())?;
        }
        if detached.exists() {
            ensure!(plain(&detached)?.is_dir(), "invalid detached history");
            let mut remaining = vec![];
            // Authenticate every remaining file before deleting any of this pass.
            for entry in fs::read_dir(&detached)? {
                let entry = entry?;
                let name = entry
                    .file_name()
                    .into_string()
                    .map_err(|_| anyhow::anyhow!("non-UTF8 detached history entry"))?;
                let digest = if name == "descriptor.json" {
                    &intent.binding.descriptor
                } else {
                    let kind = match name.as_str() {
                        "inventory.json" => HistoryMetadata::Inventory,
                        "lineage.json" => HistoryMetadata::Lineage,
                        "manifest.json" => HistoryMetadata::Manifest,
                        _ => anyhow::bail!("unknown detached historical component"),
                    };
                    &descriptor.metadata[&kind]
                };
                metadata_bytes(&entry.path(), digest, &mut |bytes| work.metadata(bytes))?;
                remaining.push((entry.path(), digest.bytes));
                ensure!(
                    remaining.len() <= 4,
                    "detached history entry bound exceeded"
                );
            }
            intent.stage = HistoryRemovalStage::PackageDetached;
            persist_removal(store, &intent, reservation, observer)?;
            for (path, bytes) in remaining {
                if !work.remove_file(&path, bytes)? {
                    return Ok(result(ConversionStatus::Pending, intent.stage, work));
                }
            }
            fs::remove_dir(&detached)?;
            crate::maintenance::sync_parent(detached.parent().unwrap())?;
        } else {
            ensure!(
                matches!(
                    intent.stage,
                    HistoryRemovalStage::PackageDetached | HistoryRemovalStage::PhysicalRemoved
                ),
                "historical package disappeared without authorized detachment"
            );
        }
        intent.stage = HistoryRemovalStage::PhysicalRemoved;
        persist_removal(store, &intent, reservation, observer)?;
        let proof = crate::maintenance::RetirementProof {
            token,
            receipt_digest: framed_digest(
                b"retention-metadata/1",
                &[&serde_json::to_vec(&intent.binding.publication)?],
            ),
            intent_id: intent.id,
            authorized_head: head,
        };
        crate::sqlite::retire_history_binding(
            store.directory(),
            &proof,
            &intent.binding,
            reservation,
        )?;
        intent.stage = HistoryRemovalStage::ReceiptRetired;
        persist_removal(store, &intent, reservation, observer)?;
    }
    intent.stage = HistoryRemovalStage::IntentComplete;
    persist_removal(store, &intent, reservation, observer)?;
    if witness.exists() {
        plain_ancestors(store, witness.parent().unwrap())?;
        metadata_bytes(&witness, &intent.binding.descriptor, &mut |bytes| {
            work.metadata(bytes)
        })?;
        fs::remove_file(&witness)?;
        crate::maintenance::sync_parent(witness.parent().unwrap())?;
    }
    fs::remove_file(removal_path(store, &token))?;
    crate::maintenance::sync_parent(removal_path(store, &token).parent().unwrap())?;
    Ok(result(ConversionStatus::Complete, intent.stage, work))
}

impl crate::FileStore {
    pub fn history_remove(
        &self,
        writer: &crate::WriterLock,
        quota: &crate::quota::Quota,
        token: &RevisionToken,
        budget: crate::retention::MaintenanceBudget,
    ) -> Result<HistoryRemoval> {
        self.history_remove_with_observer(writer, quota, token, budget, &|_, _| Ok(()))
    }
    #[doc(hidden)]
    pub fn history_remove_metered(
        &self,
        writer: &crate::WriterLock,
        quota: &crate::quota::Quota,
        token: &RevisionToken,
        budget: crate::history_policy::HistoryWorkBudget,
        observer: &dyn Fn(HistoryRemovalStage, &HistoryRemovalIntent) -> Result<()>,
    ) -> Result<HistoryRemoval> {
        crate::maintenance::check_writer(self, writer, quota)?;
        ensure!(
            !crate::checkpoints::protected(self, token)?,
            "checkpoint history cannot be retired"
        );
        ensure!(
            !self.directory().join("checkpoints").exists(),
            "checkpoint protections require qualified history policy adapter"
        );
        let mut work = crate::maintenance::WorkBudget::history(budget)?;
        let budget = budget.maintenance;
        let current = self
            .catalog()?
            .context("historical retirement requires indexed collection")?;
        let head = current
            .receipt
            .as_ref()
            .context("historical retirement requires v2 publication")?
            .token;
        token.validate_against(&head)?;
        crate::sqlite::validate_retained_token(self.directory(), token)?;
        ensure!(
            token != &head
                && current
                    .lineage
                    .as_ref()
                    .and_then(|lineage| lineage.observation.parent)
                    != Some(*token),
            "searchable keep set cannot be retired as history"
        );
        let lease_path = self
            .directory()
            .join("generations")
            .join(token.revision.to_string());
        let Some(_lease) = crate::retention::exclusive(&lease_path)? else {
            return Ok(HistoryRemoval {
                token: *token,
                status: ConversionStatus::Protected,
                stage: None,
                removed_bytes: 0,
            });
        };
        let existing = removal_intents(self)?
            .into_iter()
            .find(|intent| intent.binding.publication.token == *token);
        if existing.is_none() && crate::sqlite::retirement_proof(self.directory(), token)?.is_some()
        {
            return Ok(HistoryRemoval {
                token: *token,
                status: ConversionStatus::Complete,
                stage: None,
                removed_bytes: 0,
            });
        }
        let reservation =
            quota.reserve_available(&current.snapshot.collection, budget.max_metadata_bytes)?;
        let intent = if let Some(intent) = existing {
            intent
        } else {
            ensure!(
                removal_intents(self)?.len() < MAX_HISTORY_INTENTS,
                "history retirement intent limit reached"
            );
            let binding = crate::sqlite::history_binding(self.directory(), token)?
                .context("historical representation unavailable")?;
            ensure!(
                conversion_intent(self, token)?.is_none() && !lease_path.exists(),
                "conversion must finish before historical retirement"
            );
            verify_package_work(self, &directory(self, token), &binding, &mut work)?;
            let intent = HistoryRemovalIntent::new(binding, head)?;
            persist_removal(self, &intent, &reservation, observer)?;
            intent
        };
        process_removal(self, intent, head, &reservation, &mut work, observer)
    }
}

/// A durable, descriptor-bound retirement explains an unavailable package.
/// Merely absent files, unbound markers, and changed remaining bytes do not.
pub(crate) fn retired_package(
    store: &crate::FileStore,
    binding: &HistoryBinding,
    charge: &mut dyn FnMut(u64) -> Result<()>,
) -> Result<bool> {
    if directory(store, &binding.publication.token).exists() {
        return Ok(false);
    }
    let Some(intent) = removal_intents(store)?
        .into_iter()
        .find(|intent| intent.binding.publication.token == binding.publication.token)
    else {
        return Ok(false);
    };
    ensure!(
        intent.binding == *binding,
        "retirement intent binding mismatch"
    );
    plain_ancestors(
        store,
        removal_witness(store, &binding.publication.token)
            .parent()
            .unwrap(),
    )?;
    plain_ancestors(
        store,
        directory(store, &binding.publication.token)
            .parent()
            .unwrap(),
    )?;
    let descriptor = HistoryDescriptor::from_bytes(&metadata_bytes(
        &removal_witness(store, &binding.publication.token),
        &binding.descriptor,
        charge,
    )?)?;
    ensure!(
        descriptor.publication == binding.publication,
        "retirement witness identity mismatch"
    );
    let detached = removal_directory(store, &intent);
    if detached.exists() {
        ensure!(
            plain(&detached)?.is_dir(),
            "invalid retiring history directory"
        );
        for entry in fs::read_dir(&detached)? {
            let entry = entry?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| anyhow::anyhow!("invalid retiring history name"))?;
            let expected = if name == "descriptor.json" {
                &binding.descriptor
            } else {
                let kind = match name.as_str() {
                    "inventory.json" => HistoryMetadata::Inventory,
                    "lineage.json" => HistoryMetadata::Lineage,
                    "manifest.json" => HistoryMetadata::Manifest,
                    _ => anyhow::bail!("unknown retiring historical component"),
                };
                &descriptor.metadata[&kind]
            };
            metadata_bytes(&entry.path(), expected, charge)?;
        }
    } else {
        ensure!(
            matches!(
                intent.stage,
                HistoryRemovalStage::PackageDetached | HistoryRemovalStage::PhysicalRemoved
            ),
            "historical package missing without durable detachment"
        );
    }
    Ok(true)
}

pub(crate) fn pending_count(store: &crate::FileStore) -> Result<usize> {
    Ok(intents(store)?.len() + removal_intents(store)?.len())
}

pub(crate) fn convert_selected(
    store: &crate::FileStore,
    token: RevisionToken,
    head: RevisionToken,
    parent: Option<RevisionToken>,
    reservation: &crate::quota::Reservation,
    work: &mut crate::maintenance::WorkBudget,
) -> Result<HistoryConversion> {
    token.validate_against(&head)?;
    ensure!(
        token != head && Some(token) != parent,
        "cannot demote searchable revision"
    );
    crate::sqlite::validate_retained_token(store.directory(), &token)?;
    let source = store
        .directory()
        .join("generations")
        .join(token.revision.to_string());
    let Some(_lease) = crate::retention::exclusive(&source)? else {
        return Ok(HistoryConversion {
            token,
            status: ConversionStatus::Protected,
            stage: None,
            removed_bytes: 0,
        });
    };
    let existing = conversion_intent(store, &token)?;
    if existing.is_none()
        && let Some(binding) = crate::sqlite::history_binding(store.directory(), &token)?
    {
        verify_package_work(store, &directory(store, &token), &binding, work)?;
        ensure!(
            !source.exists() && !detached_path(store, &token).exists(),
            "history binding has unowned full representation"
        );
        return Ok(HistoryConversion {
            token,
            status: ConversionStatus::Complete,
            stage: Some(ConversionStage::IntentComplete),
            removed_bytes: 0,
        });
    }
    let descriptor = if existing.is_none() {
        let receipt = store
            .receipt(&token)?
            .context("historical candidate unavailable")?;
        Some(descriptor_from_full(store, &receipt, work)?)
    } else {
        None
    };
    let intent = if let Some(intent) = existing {
        intent
    } else {
        ensure!(
            intents(store)?.len() < MAX_HISTORY_INTENTS,
            "history intent limit reached"
        );
        let descriptor = descriptor.as_ref().unwrap();
        let intent = ConversionIntent::new(
            descriptor.publication.clone(),
            head,
            descriptor.binding_digest()?,
        )?;
        persist_intent(store, &intent, reservation, &|_, _| Ok(()))?;
        intent
    };
    process_conversion(
        store,
        intent,
        descriptor,
        head,
        reservation,
        work,
        &|_, _| Ok(()),
    )
}

pub(crate) fn remove_selected(
    store: &crate::FileStore,
    token: RevisionToken,
    head: RevisionToken,
    parent: Option<RevisionToken>,
    reservation: &crate::quota::Reservation,
    work: &mut crate::maintenance::WorkBudget,
) -> Result<HistoryRemoval> {
    token.validate_against(&head)?;
    ensure!(
        token != head && Some(token) != parent,
        "cannot retire searchable revision"
    );
    ensure!(
        !crate::checkpoints::protected(store, &token)?,
        "checkpoint protects historical revision"
    );
    let source = store
        .directory()
        .join("generations")
        .join(token.revision.to_string());
    let Some(_lease) = crate::retention::exclusive(&source)? else {
        return Ok(HistoryRemoval {
            token,
            status: ConversionStatus::Protected,
            stage: None,
            removed_bytes: 0,
        });
    };
    let intent = if let Some(intent) = removal_intents(store)?
        .into_iter()
        .find(|intent| intent.binding.publication.token == token)
    {
        intent
    } else {
        ensure!(
            removal_intents(store)?.len() < MAX_HISTORY_INTENTS,
            "history retirement intent limit reached"
        );
        ensure!(
            conversion_intent(store, &token)?.is_none() && !source.exists(),
            "conversion must finish before retirement"
        );
        let binding = crate::sqlite::history_binding(store.directory(), &token)?
            .context("historical representation unavailable")?;
        verify_package_work(store, &directory(store, &token), &binding, work)?;
        let intent = HistoryRemovalIntent::new(binding, head)?;
        persist_removal(store, &intent, reservation, &|_, _| Ok(()))?;
        intent
    };
    process_removal(store, intent, head, reservation, work, &|_, _| Ok(()))
}

fn verify_full_bounded(
    store: &crate::FileStore,
    receipt: &PublicationReceipt,
    work: &mut crate::maintenance::WorkBudget,
) -> Result<airk_core::publication::InventoryV2> {
    let root = store
        .directory()
        .join("generations")
        .join(receipt.token.revision.to_string());
    let path = root.join("manifest.json");
    ensure!(
        plain(&root)?.is_dir() && plain(&path)?.is_file(),
        "invalid history conversion candidate"
    );
    let manifest_bytes = plain(&path)?.len();
    work.metadata(manifest_bytes)?;
    let bytes = crate::blobs::read_verified(&path, &receipt.manifest_digest.to_string(), 131_072)?;
    let manifest: OriginalManifest = serde_json::from_slice(&bytes)?;
    ensure!(
        manifest.token == receipt.token && manifest.components.len() <= 1024,
        "invalid candidate manifest identity or bound"
    );
    for component in manifest.components.values() {
        work.verify(component.bytes)?;
    }
    ensure!(
        !work.expired(),
        "history conversion verification deadline exceeded"
    );
    crate::blobs::verify_v2_generation(store.directory(), receipt)
}

impl crate::FileStore {
    #[doc(hidden)]
    pub fn history_convert_with_observer(
        &self,
        writer: &crate::WriterLock,
        quota: &crate::quota::Quota,
        token: &RevisionToken,
        budget: crate::retention::MaintenanceBudget,
        observer: &dyn Fn(ConversionStage, &ConversionIntent) -> Result<()>,
    ) -> Result<HistoryConversion> {
        self.history_convert_metered(
            writer,
            quota,
            token,
            crate::history_policy::HistoryWorkBudget {
                maintenance: budget,
                verification_bytes: budget.max_metadata_bytes,
            },
            observer,
        )
    }
    #[doc(hidden)]
    pub fn history_remove_with_observer(
        &self,
        writer: &crate::WriterLock,
        quota: &crate::quota::Quota,
        token: &RevisionToken,
        budget: crate::retention::MaintenanceBudget,
        observer: &dyn Fn(HistoryRemovalStage, &HistoryRemovalIntent) -> Result<()>,
    ) -> Result<HistoryRemoval> {
        self.history_remove_metered(
            writer,
            quota,
            token,
            crate::history_policy::HistoryWorkBudget {
                maintenance: budget,
                verification_bytes: budget.max_metadata_bytes,
            },
            observer,
        )
    }
}

pub(crate) fn describe_revision(
    store: &crate::FileStore,
    token: &RevisionToken,
    work: &mut crate::maintenance::WorkBudget,
) -> Result<HistoryDescriptor> {
    let receipt = store
        .receipt(token)?
        .context("historical candidate receipt missing")?;
    if let Some(binding) = crate::sqlite::history_binding(store.directory(), token)? {
        ensure!(
            binding.publication == receipt,
            "historical candidate receipt mismatch"
        );
        Ok(verify_package_work(store, &directory(store, token), &binding, work)?.descriptor)
    } else {
        let descriptor = descriptor_from_full(store, &receipt, work)?;
        verify_dependencies(store, &descriptor, &mut |bytes| work.verify(bytes))?;
        Ok(descriptor)
    }
}

pub(crate) fn observation(
    store: &crate::FileStore,
    receipt: &PublicationReceipt,
    work: &mut crate::maintenance::WorkBudget,
) -> Result<crate::history_policy::Observation> {
    let historical = crate::sqlite::history_binding(store.directory(), &receipt.token)?;
    let path = if let Some(binding) = &historical {
        ensure!(
            &binding.publication == receipt,
            "historical observation receipt changed"
        );
        directory(store, &receipt.token)
    } else {
        store
            .directory()
            .join("generations")
            .join(receipt.token.revision.to_string())
    };
    plain_ancestors(store, &path)?;
    let manifest_path = path.join("manifest.json");
    let length = plain(&manifest_path)?.len();
    ensure!(
        length <= 131072,
        "historical observation manifest exceeds bound"
    );
    work.metadata(length)?;
    let manifest: OriginalManifest = serde_json::from_slice(&crate::blobs::read_verified(
        &manifest_path,
        &receipt.manifest_digest.to_string(),
        131072,
    )?)?;
    ensure!(
        manifest.supported_format()
            && manifest.token == receipt.token
            && manifest.inventory_digest == receipt.inventory_digest
            && manifest.components.len() <= 1024,
        "historical observation manifest binding invalid"
    );
    let lineage = manifest
        .components
        .get("lineage.json")
        .context("historical observation lineage missing")?;
    ensure!(
        lineage.bytes <= airk_core::changes::MAX_LINEAGE_BYTES as u64,
        "historical observation lineage exceeds bound"
    );
    let bytes = metadata_bytes(&path.join("lineage.json"), lineage, &mut |bytes| {
        work.metadata(bytes)
    })?;
    let lineage: airk_core::changes::LineageEnvelope = serde_json::from_slice(&bytes)?;
    lineage.validate()?;
    ensure!(
        lineage.incarnation == receipt.token.incarnation
            && lineage.observation.generation == receipt.token.generation,
        "historical observation lineage scope mismatch"
    );
    Ok(crate::history_policy::Observation {
        publication: receipt.clone(),
        observed_unix_seconds: lineage.observation.observed_unix_seconds,
        historical: historical.is_some(),
    })
}
