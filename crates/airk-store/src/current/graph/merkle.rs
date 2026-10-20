// SPDX-License-Identifier: Apache-2.0
//! Bounded proof assembly. Hash domains and canonical empty subtrees belong to core.
use airk_core::{graph_storage::*, identity::Sha256Digest};
use anyhow::{Result, ensure};

pub(super) type Siblings = [Sha256Digest; GRAPH_TREE_HEIGHT as usize];

pub(super) fn proof_root(
    scope: &GraphScope,
    id: u32,
    mut digest: Sha256Digest,
    siblings: &Siblings,
) -> Result<Sha256Digest> {
    let mut index = id;
    for (offset, sibling) in siblings.iter().enumerate() {
        let (left, right) = if index & 1 == 0 {
            (digest, *sibling)
        } else {
            (*sibling, digest)
        };
        index >>= 1;
        digest = branch_hash(scope, offset as u8 + 1, index, left, right)?;
    }
    Ok(digest)
}

pub(super) fn verify_absent(head: &GraphHead, id: u32, siblings: &Siblings) -> Result<()> {
    head.validate()?;
    ensure!(
        u64::from(id) >= head.allocated_count,
        "allocated graph record is missing"
    );
    ensure!(
        proof_root(&head.scope, id, empty_hash(&head.scope, 0)?, siblings)? == head.merkle_root,
        "graph absence proof mismatch"
    );
    Ok(())
}

/// Apply one unique final record to an already authenticated base proof, using
/// earlier afterimages for sibling subtrees. Calls remain bounded to 32 reads
/// and 33 writes; the candidate's prior root must agree before any path changes.
pub(super) fn update_path(
    scope: &GraphScope,
    id: u32,
    old: Option<&GraphRecord>,
    new: &GraphRecord,
    base_proof: (&Siblings, Sha256Digest),
    mut get: impl FnMut(u8, u32) -> Result<Option<Sha256Digest>>,
    mut put: impl FnMut(GraphMerkleNode) -> Result<()>,
) -> Result<Sha256Digest> {
    let (base_siblings, current_root) = base_proof;
    let mut siblings = *base_siblings;
    let mut index = id;
    for (height, sibling) in siblings.iter_mut().enumerate() {
        if let Some(updated) = get(height as u8, index ^ 1)? {
            *sibling = updated;
        }
        index >>= 1;
    }
    let old_hash = match old {
        Some(record) => leaf_hash(scope, id, record)?,
        None => empty_hash(scope, 0)?,
    };
    ensure!(
        proof_root(scope, id, old_hash, &siblings)? == current_root,
        "graph afterimage base proof mismatch"
    );
    let mut digest = leaf_hash(scope, id, new)?;
    put(GraphMerkleNode {
        height: 0,
        index: id,
        digest,
    })?;
    let mut index = id;
    for (height, sibling) in siblings.iter().enumerate() {
        let (left, right) = if index & 1 == 0 {
            (digest, *sibling)
        } else {
            (*sibling, digest)
        };
        index >>= 1;
        digest = branch_hash(scope, height as u8 + 1, index, left, right)?;
        put(GraphMerkleNode {
            height: height as u8 + 1,
            index,
            digest,
        })?;
    }
    Ok(digest)
}
