// SPDX-License-Identifier: Apache-2.0
//! Neutral, bounded record storage for the DiskANN provider. No search algorithm lives here.
use crate::identity::{Incarnation, Sha256Digest, framed_digest};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

pub const GRAPH_STORAGE_VERSION: u32 = 1;
pub const GRAPH_TREE_HEIGHT: u8 = 32;
pub const MAX_GRAPH_RECORD_BYTES: usize = 16 * 1024;
pub const GRAPH_ID_CAPACITY: u64 = u32::MAX as u64 + 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphScope {
    pub collection: String,
    pub root: String,
    pub incarnation: Incarnation,
    pub graph_instance: Incarnation,
    pub specification: Sha256Digest,
    pub record_bytes: u32,
}

impl GraphScope {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.collection.is_empty()
                && self.collection.len() <= 64
                && self
                    .collection
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b)),
            "invalid graph collection"
        );
        ensure!(
            !self.root.is_empty() && self.root.len() <= 32_768 && !self.root.contains('\0'),
            "invalid graph root"
        );
        ensure!(
            (1..=MAX_GRAPH_RECORD_BYTES).contains(&(self.record_bytes as usize)),
            "graph record exceeds byte bound"
        );
        Ok(())
    }
    pub fn digest(&self) -> Sha256Digest {
        framed_digest(
            b"current-graph-scope/1",
            &[
                self.collection.as_bytes(),
                self.root.as_bytes(),
                self.incarnation.as_bytes(),
                self.graph_instance.as_bytes(),
                self.specification.as_bytes(),
                &self.record_bytes.to_be_bytes(),
            ],
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphCounts {
    /// Exclusive physical-ID bound. A created graph reserves non-live ID zero.
    pub allocated_count: u64,
    pub live_nodes: u64,
}

impl GraphCounts {
    pub fn validate(self) -> Result<()> {
        ensure!(
            self.allocated_count <= GRAPH_ID_CAPACITY
                && self.live_nodes <= self.allocated_count.saturating_sub(1),
            "invalid graph allocation counts"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphHead {
    pub scope: GraphScope,
    pub allocated_count: u64,
    pub live_nodes: u64,
    pub merkle_root: Sha256Digest,
}

impl GraphHead {
    pub fn counts(&self) -> GraphCounts {
        GraphCounts {
            allocated_count: self.allocated_count,
            live_nodes: self.live_nodes,
        }
    }
    pub fn empty(scope: GraphScope) -> Result<Self> {
        scope.validate()?;
        let merkle_root = empty_hash(&scope, GRAPH_TREE_HEIGHT)?;
        Ok(Self {
            scope,
            allocated_count: 0,
            live_nodes: 0,
            merkle_root,
        })
    }
    pub fn validate(&self) -> Result<()> {
        self.scope.validate()?;
        self.counts().validate()?;
        if self.allocated_count == 0 {
            ensure!(
                self.merkle_root == empty_hash(&self.scope, GRAPH_TREE_HEIGHT)?,
                "nonempty root for unallocated graph"
            );
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphRecord {
    /// None is permitted only for the reserved search seed at physical ID zero.
    pub embedding_key: Option<Sha256Digest>,
    pub live: bool,
    /// Exact provider encoding, including vector and adjacency bits.
    pub bytes: Vec<u8>,
}

impl GraphRecord {
    pub fn validate(&self, scope: &GraphScope, id: u32) -> Result<()> {
        scope.validate()?;
        ensure!(
            self.bytes.len() == scope.record_bytes as usize,
            "invalid graph record length"
        );
        ensure!(
            (id == 0 && self.embedding_key.is_none() && !self.live)
                || (id != 0 && self.embedding_key.is_some()),
            "invalid graph record identity"
        );
        Ok(())
    }
}

/// Implementations authenticate committed reads against their pinned head.
/// Private overlays may serve checksummed candidate rows over an authenticated base.
pub trait GraphRecordReader: Send + Sync + 'static {
    fn scope(&self) -> &GraphScope;
    fn counts(&self) -> Result<GraphCounts>;
    fn read(&self, id: u32) -> Result<GraphRecord>;
}

/// Mutations are private until sealed, durably staged and committed by the store.
/// Every failure poisons the whole candidate. Existing IDs/keys are never reassigned.
pub trait GraphRecordOverlay: GraphRecordReader {
    fn append(&self, expected_id: u32, record: GraphRecord) -> Result<()>;
    fn put(&self, id: u32, record: GraphRecord) -> Result<()>;
    fn set_live(&self, id: u32, live: bool) -> Result<()>;
    fn poison(&self);
}

/// Height zero is a leaf; height 32/index zero is the root.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphMerkleNode {
    pub height: u8,
    pub index: u32,
    pub digest: Sha256Digest,
}

impl GraphMerkleNode {
    pub fn validate(&self) -> Result<()> {
        validate_coordinate(self.height, self.index)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphDeltaDescriptor {
    pub version: u32,
    pub base: GraphHead,
    pub target: GraphHead,
    pub record_count: u64,
    pub node_count: u64,
    pub record_bytes: u64,
    pub records_digest: Sha256Digest,
    pub nodes_digest: Sha256Digest,
}

impl GraphDeltaDescriptor {
    pub fn validate(&self) -> Result<()> {
        self.base.validate()?;
        self.target.validate()?;
        ensure!(
            self.version == GRAPH_STORAGE_VERSION && self.base.scope == self.target.scope,
            "unsupported or mismatched graph delta"
        );
        ensure!(
            self.target.allocated_count >= self.base.allocated_count
                && self.record_count <= self.target.allocated_count
                && self.record_count >= self.target.allocated_count - self.base.allocated_count,
            "invalid graph delta allocation"
        );
        ensure!(
            self.record_count
                .checked_mul(u64::from(self.target.scope.record_bytes))
                == Some(self.record_bytes),
            "invalid graph delta byte count"
        );
        ensure!(
            self.node_count <= self.record_count.saturating_mul(33),
            "graph delta exceeds proof work bound"
        );
        Ok(())
    }
}

/// Sealed data owns only bounded private after-images, never a current read lease.
/// Visitors run in ascending ID / (height,index) order, with no whole-delta buffer.
pub trait PreparedGraphDelta {
    fn descriptor(&self) -> &GraphDeltaDescriptor;
    fn visit_records(&self, visitor: &mut dyn FnMut(u32, &GraphRecord) -> Result<()>)
    -> Result<()>;
    fn visit_nodes(&self, visitor: &mut dyn FnMut(GraphMerkleNode) -> Result<()>) -> Result<()>;
}

pub fn validate_coordinate(height: u8, index: u32) -> Result<()> {
    ensure!(
        height <= GRAPH_TREE_HEIGHT && u64::from(index) < (1u64 << (GRAPH_TREE_HEIGHT - height)),
        "invalid graph proof coordinate"
    );
    Ok(())
}

/// Empty subtrees use a canonical height-specific digest. This avoids enumerating
/// billions of position-specific empty leaves when initializing a sparse tree.
pub fn empty_hash(scope: &GraphScope, height: u8) -> Result<Sha256Digest> {
    ensure!(
        height <= GRAPH_TREE_HEIGHT,
        "invalid empty graph subtree height"
    );
    Ok(framed_digest(
        b"current-graph-empty/1",
        &[scope.digest().as_bytes(), &[height]],
    ))
}

pub fn leaf_hash(scope: &GraphScope, id: u32, record: &GraphRecord) -> Result<Sha256Digest> {
    record.validate(scope, id)?;
    let key = record
        .embedding_key
        .as_ref()
        .map(|key| key.as_bytes().as_slice())
        .unwrap_or_default();
    Ok(framed_digest(
        b"current-graph-record/1",
        &[
            scope.digest().as_bytes(),
            &id.to_be_bytes(),
            key,
            &[u8::from(record.live)],
            &record.bytes,
        ],
    ))
}

pub fn branch_hash(
    scope: &GraphScope,
    height: u8,
    index: u32,
    left: Sha256Digest,
    right: Sha256Digest,
) -> Result<Sha256Digest> {
    validate_coordinate(height, index)?;
    ensure!(height != 0, "graph branch cannot have leaf height");
    let empty = empty_hash(scope, height - 1)?;
    if left == empty && right == empty {
        return empty_hash(scope, height);
    }
    Ok(framed_digest(
        b"current-graph-branch/1",
        &[
            scope.digest().as_bytes(),
            &[height],
            &index.to_be_bytes(),
            left.as_bytes(),
            right.as_bytes(),
        ],
    ))
}

pub fn verify_record(
    head: &GraphHead,
    id: u32,
    record: &GraphRecord,
    siblings: &[Sha256Digest; 32],
) -> Result<()> {
    head.validate()?;
    ensure!(
        u64::from(id) < head.allocated_count,
        "graph ID outside committed allocation"
    );
    let mut hash = leaf_hash(&head.scope, id, record)?;
    let mut index = id;
    for (offset, sibling) in siblings.iter().enumerate() {
        let (left, right) = if index & 1 == 0 {
            (hash, *sibling)
        } else {
            (*sibling, hash)
        };
        index >>= 1;
        hash = branch_hash(&head.scope, offset as u8 + 1, index, left, right)?;
    }
    ensure!(hash == head.merkle_root, "graph record proof mismatch");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope() -> GraphScope {
        GraphScope {
            collection: "fixture".into(),
            root: "original-root".into(),
            incarnation: Incarnation::from_bytes([1; 16]),
            graph_instance: Incarnation::from_bytes([2; 16]),
            specification: Sha256Digest::from_bytes([3; 32]),
            record_bytes: 4,
        }
    }

    #[test]
    fn sparse_proof_binds_record_identity_bits_and_live_state() -> Result<()> {
        let scope = scope();
        let seed = GraphRecord {
            embedding_key: None,
            live: false,
            bytes: vec![0; 4],
        };
        let record = GraphRecord {
            embedding_key: Some(Sha256Digest::from_bytes([4; 32])),
            live: true,
            bytes: vec![1, 2, 3, 4],
        };
        let mut siblings = [Sha256Digest::from_bytes([0; 32]); 32];
        siblings[0] = leaf_hash(&scope, 0, &seed)?;
        let mut root = branch_hash(&scope, 1, 0, siblings[0], leaf_hash(&scope, 1, &record)?)?;
        for (height, sibling) in siblings.iter_mut().enumerate().skip(1) {
            *sibling = empty_hash(&scope, height as u8)?;
            root = branch_hash(&scope, height as u8 + 1, 0, root, *sibling)?;
        }
        let head = GraphHead {
            scope,
            allocated_count: 2,
            live_nodes: 1,
            merkle_root: root,
        };
        verify_record(&head, 1, &record, &siblings)?;
        for mode in 0..4 {
            let mut changed = record.clone();
            match mode {
                0 => changed.bytes[0] ^= 1,
                1 => changed.embedding_key = Some(Sha256Digest::from_bytes([5; 32])),
                2 => changed.live = false,
                _ => changed.bytes.push(0),
            }
            assert!(verify_record(&head, 1, &changed, &siblings).is_err());
        }
        assert!(verify_record(&head, 0, &record, &siblings).is_err());
        assert!(verify_record(&head, 2, &record, &siblings).is_err());
        let mut changed = head.clone();
        changed.scope.graph_instance = Incarnation::from_bytes([9; 16]);
        assert!(verify_record(&changed, 1, &record, &siblings).is_err());
        let mut changed = siblings;
        changed[20] = Sha256Digest::from_bytes([7; 32]);
        assert!(verify_record(&head, 1, &record, &changed).is_err());
        Ok(())
    }

    #[test]
    fn sparse_defaults_and_allocation_bounds_are_explicit() -> Result<()> {
        let scope = scope();
        GraphHead::empty(scope.clone())?.validate()?;
        for height in 1..=32 {
            let child = empty_hash(&scope, height - 1)?;
            assert_eq!(
                branch_hash(&scope, height, 0, child, child)?,
                empty_hash(&scope, height)?
            );
        }
        assert!(validate_coordinate(33, 0).is_err());
        assert!(validate_coordinate(32, 1).is_err());
        assert!(
            GraphCounts {
                allocated_count: GRAPH_ID_CAPACITY + 1,
                live_nodes: 0
            }
            .validate()
            .is_err()
        );
        assert!(
            GraphCounts {
                allocated_count: 1,
                live_nodes: 1
            }
            .validate()
            .is_err()
        );
        Ok(())
    }
}
