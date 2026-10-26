// SPDX-License-Identifier: Apache-2.0
//! Storage-neutral captured publication contracts for the native journal.
use super::{
    CurrentHead, CurrentToken, MAX_INPUT_METADATA_BYTES, MAX_LEXICAL_MANIFEST_BYTES,
    MAX_PENDING_INPUTS, MAX_PENDING_METADATA_BYTES, database, map, vector_map::PreparedOccurrence,
};
use crate::FileStore;
use airk_core::{
    Artifact,
    graph_storage::GraphDeltaDescriptor,
    identity::{Sha256Digest, framed_digest},
    model_objects::ModelObjectSet,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;
const MAX_DESCRIPTOR_BYTES: usize = 196_608;
const MAX_CAPTURED_OBJECT_BYTES: u64 = 64 * 1024 * 1024 * 1024;
const MAX_PENDING_MAP_CHANGES: u64 = super::MAX_PENDING_MAP_CHANGES as u64;
const MAX_PENDING_MAP_BYTES: u64 = super::MAX_PENDING_MAP_BYTES as u64;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransactionPhase {
    Ready,
    Prepared,
    Applying,
}

#[derive(Clone, Debug)]
pub struct CurrentCommit {
    pub token: CurrentToken,
    /// A successful durable publication may leave quota/GC cleanup to a retry.
    pub maintenance_error: Option<String>,
}

#[derive(Clone, Debug)]
pub struct CurrentCapacityChange {
    pub capacity_bytes: u64,
    /// The owner limit changed durably; its unused admission grant needs cleanup.
    pub maintenance_error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapturedInput {
    pub artifact: Artifact,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub postings_version: u32,
}

/// A changed source path in a single current-index publication. Upserts carry
/// compact, retained and source-verified metadata; deletes carry the exact
/// normalized path whose current map entry is removed.
#[derive(Clone, Debug)]
pub enum ArtifactChange {
    Upsert(Box<CapturedInput>),
    Delete(String),
}

pub struct ArtifactVectorChanges<'a> {
    pub artifacts: &'a [ArtifactChange],
    /// Captured current artifacts used only to attach vectors; their catalog
    /// and lexical documents are not rewritten by this transaction.
    pub vector_inputs: Option<&'a [CapturedInput]>,
    pub occurrences: &'a [PreparedOccurrence],
    pub configuration: Option<&'a super::collection::CollectionConfiguration>,
}

impl ArtifactChange {
    pub(crate) fn path(&self) -> &str {
        match self {
            Self::Upsert(input) => &input.artifact.path,
            Self::Delete(path) => path,
        }
    }
}

pub(crate) fn valid_current_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 8192
        && !path.contains(['\\', ':', '\0'])
        && Path::new(path).is_relative()
        && path.split('/').all(|part| !matches!(part, "" | "." | ".."))
}

fn is_zero(value: &u32) -> bool {
    *value == 0
}

impl CapturedInput {
    pub fn new(artifact: Artifact) -> Self {
        Self {
            artifact,
            postings_version: 1,
        }
    }
    pub(super) fn bytes(&self) -> Result<Vec<u8>> {
        ensure!(
            self.postings_version <= 1,
            "unsupported captured posting version"
        );
        let artifact = &self.artifact;
        ensure!(
            valid_current_path(&artifact.path),
            "invalid captured artifact path"
        );
        artifact.content_hash.parse::<Sha256Digest>()?;
        ensure!(
            artifact.chunks.is_empty() && artifact.facts.is_empty(),
            "captured inputs require compact retained metadata"
        );
        let retained = artifact
            .retained
            .as_ref()
            .context("captured extraction reference missing")?;
        retained.object.parse::<Sha256Digest>()?;
        let profile = retained
            .profile
            .as_ref()
            .context("captured extraction profile missing")?;
        profile.identity()?;
        ensure!(
            profile.chunk_version == 3
                && retained.raw_source
                && retained.object_bytes <= airk_core::extraction::MAX_PAYLOAD_BYTES
                && artifact.bytes <= airk_core::extraction::MAX_SOURCE_BYTES,
            "captured input exceeds supported extraction bounds"
        );
        let bytes = serde_json::to_vec(self)?;
        ensure!(
            bytes.len() <= MAX_INPUT_METADATA_BYTES,
            "captured input metadata exceeds byte bound"
        );
        Ok(bytes)
    }
    pub(super) fn verify(&self, store: &FileStore) -> Result<()> {
        self.bytes()?;
        store.load_retrieval_artifact(&self.artifact)?;
        store.read_retained_source(&self.artifact)?;
        Ok(())
    }
}

/// One item at a time, in strictly increasing artifact-path order.
pub trait CapturedInputs {
    fn visit(&self, visitor: &mut dyn FnMut(&CapturedInput) -> Result<()>) -> Result<()>;
}
impl CapturedInputs for [CapturedInput] {
    fn visit(&self, visitor: &mut dyn FnMut(&CapturedInput) -> Result<()>) -> Result<()> {
        for input in self {
            visitor(input)?;
        }
        Ok(())
    }
}
impl CapturedInputs for Vec<CapturedInput> {
    fn visit(&self, visitor: &mut dyn FnMut(&CapturedInput) -> Result<()>) -> Result<()> {
        self.as_slice().visit(visitor)
    }
}
impl<const N: usize> CapturedInputs for [CapturedInput; N] {
    fn visit(&self, visitor: &mut dyn FnMut(&CapturedInput) -> Result<()>) -> Result<()> {
        self.as_slice().visit(visitor)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingTransaction {
    pub version: u32,
    pub id: Sha256Digest,
    pub expected: CurrentToken,
    pub target: CurrentHead,
    pub graph: GraphDeltaDescriptor,
    pub inputs_count: u64,
    pub metadata_bytes: u64,
    pub object_bytes: u64,
    pub inputs_digest: Sha256Digest,
    pub map_count: u64,
    pub map_bytes: u64,
    pub map_digest: Sha256Digest,
    pub lexical_manifest_bytes: u64,
    pub model: Option<ModelObjectSet>,
    pub database_capacity_bytes: u64,
}

impl PendingTransaction {
    pub(crate) fn bytes(&self) -> Result<Vec<u8>> {
        self.target.validate()?;
        self.graph.validate()?;
        database::validate_capacity(self.database_capacity_bytes)?;
        ensure!(
            self.version == 3
                && self.inputs_count <= MAX_PENDING_INPUTS as u64
                && self.metadata_bytes <= MAX_PENDING_METADATA_BYTES
                && self.object_bytes <= MAX_CAPTURED_OBJECT_BYTES
                && self.map_count <= MAX_PENDING_MAP_CHANGES
                && self.map_bytes <= MAX_PENDING_MAP_BYTES
                && self.lexical_manifest_bytes <= MAX_LEXICAL_MANIFEST_BYTES as u64
                && self.graph.target == self.target.graph_head
                && self.expected.incarnation == self.target.scope.incarnation
                && self.expected.sequence.checked_add(1) == Some(self.target.sequence),
            "invalid pending current transaction"
        );
        if self.map_count == 0 {
            ensure!(
                self.map_bytes == 0 && self.map_digest == MapDigest::new().finish(),
                "empty pending map has a nonempty descriptor"
            );
        }
        if let Some(model) = &self.model {
            model.validate()?;
        }
        ensure!(
            self.id == transaction_id(&self.expected, &self.target, &self.graph, &self.model)?,
            "pending transaction identity mismatch"
        );
        let bytes = serde_json::to_vec(self)?;
        ensure!(
            bytes.len() <= MAX_DESCRIPTOR_BYTES,
            "pending descriptor exceeds byte bound"
        );
        Ok(bytes)
    }
}

fn transaction_id(
    expected: &CurrentToken,
    target: &CurrentHead,
    graph: &GraphDeltaDescriptor,
    model: &Option<ModelObjectSet>,
) -> Result<Sha256Digest> {
    Ok(framed_digest(
        b"current-pending-transaction/1",
        &[
            &serde_json::to_vec(expected)?,
            &target.canonical_bytes()?,
            &serde_json::to_vec(graph)?,
            &serde_json::to_vec(model)?,
        ],
    ))
}
pub(crate) struct NativeIntent<'a> {
    pub expected: CurrentToken,
    pub target: CurrentHead,
    pub graph: GraphDeltaDescriptor,
    pub inputs: &'a dyn CapturedInputs,
    pub model: Option<ModelObjectSet>,
    pub capacity: u64,
    pub lexical_manifest_bytes: u64,
}
pub(crate) fn native_intent<'a>(
    fields: NativeIntent<'_>,
    changes: impl IntoIterator<Item = &'a map::MapChange>,
) -> Result<PendingTransaction> {
    let mut input_digest = InputDigest::new();
    fields.inputs.visit(&mut |input| {
        let bytes = input.bytes()?;
        input_digest.add(input, &bytes)
    })?;
    let mut map_digest = MapDigest::new();
    for change in changes {
        map_digest.add(change.key(), change.value())?;
    }
    let pending = PendingTransaction {
        version: 3,
        id: transaction_id(
            &fields.expected,
            &fields.target,
            &fields.graph,
            &fields.model,
        )?,
        expected: fields.expected,
        target: fields.target,
        graph: fields.graph,
        inputs_count: input_digest.count,
        metadata_bytes: input_digest.bytes,
        object_bytes: input_digest.objects,
        inputs_digest: input_digest.finish(),
        map_count: map_digest.count,
        map_bytes: map_digest.bytes,
        map_digest: map_digest.finish(),
        lexical_manifest_bytes: fields.lexical_manifest_bytes,
        model: fields.model,
        database_capacity_bytes: fields.capacity,
    };
    pending.bytes()?;
    Ok(pending)
}

pub(crate) struct InputDigest {
    hash: Sha256,
    pub(crate) count: u64,
    pub(crate) bytes: u64,
    pub(crate) objects: u64,
    previous: Option<String>,
}
impl InputDigest {
    pub(crate) fn new() -> Self {
        let mut hash = Sha256::new();
        hash.update(b"airk-current-captured-inputs/1");
        Self {
            hash,
            count: 0,
            bytes: 0,
            objects: 0,
            previous: None,
        }
    }
    pub(crate) fn add(&mut self, input: &CapturedInput, bytes: &[u8]) -> Result<()> {
        ensure!(
            self.previous
                .as_ref()
                .is_none_or(|path| path < &input.artifact.path),
            "captured input paths must be unique and ordered"
        );
        self.count += 1;
        self.bytes = self
            .bytes
            .checked_add(bytes.len() as u64)
            .context("captured metadata overflow")?;
        let retained = input
            .artifact
            .retained
            .as_ref()
            .context("captured input reference missing")?;
        self.objects = self
            .objects
            .checked_add(input.artifact.bytes as u64)
            .and_then(|n| n.checked_add(retained.object_bytes as u64))
            .context("captured object bytes overflow")?;
        ensure!(
            self.count <= MAX_PENDING_INPUTS as u64
                && self.bytes <= MAX_PENDING_METADATA_BYTES
                && self.objects <= MAX_CAPTURED_OBJECT_BYTES,
            "captured transaction input budget exceeded"
        );
        self.hash.update((bytes.len() as u64).to_be_bytes());
        self.hash.update(bytes);
        self.previous = Some(input.artifact.path.clone());
        Ok(())
    }
    pub(crate) fn finish(&self) -> Sha256Digest {
        Sha256Digest::from_bytes(self.hash.clone().finalize().into())
    }
}

struct MapDigest {
    hash: Sha256,
    count: u64,
    bytes: u64,
    previous: Option<Sha256Digest>,
}
impl MapDigest {
    fn new() -> Self {
        let mut hash = Sha256::new();
        hash.update(b"airk-current-pending-map/1");
        Self {
            hash,
            count: 0,
            bytes: 0,
            previous: None,
        }
    }
    fn add(&mut self, key: Sha256Digest, value: Option<&[u8]>) -> Result<()> {
        ensure!(
            self.previous.is_none_or(|previous| previous < key),
            "pending map keys must be unique and ordered"
        );
        let value_bytes = value.map_or(0, |bytes| bytes.len());
        ensure!(
            value_bytes <= map::MAX_VALUE_BYTES,
            "pending map value exceeds limit"
        );
        self.count = self
            .count
            .checked_add(1)
            .context("map change count overflow")?;
        self.bytes = self
            .bytes
            .checked_add(32 + 1 + 4 + value_bytes as u64)
            .context("map change byte overflow")?;
        ensure!(
            self.count <= MAX_PENDING_MAP_CHANGES && self.bytes <= MAX_PENDING_MAP_BYTES,
            "pending map change budget exceeded"
        );
        self.hash.update(key.as_bytes());
        self.hash.update([u8::from(value.is_some())]);
        self.hash.update((value_bytes as u32).to_be_bytes());
        if let Some(value) = value {
            self.hash.update(value);
        }
        self.previous = Some(key);
        Ok(())
    }
    fn finish(&self) -> Sha256Digest {
        Sha256Digest::from_bytes(self.hash.clone().finalize().into())
    }
}

pub(crate) fn verify_model(store: &FileStore, model: &Option<ModelObjectSet>) -> Result<()> {
    if let Some(model) = model {
        model.validate()?;
        let maximum = model.assets.values().try_fold(0u64, |sum, item| {
            sum.checked_add(item.bytes)
                .context("model verification budget overflow")
        })?;
        store.validate_model_reference(model, maximum)?;
    }
    Ok(())
}
