// SPDX-License-Identifier: Apache-2.0
//! Storage-independent graph proofs; the authenticated byte domains stay in core.
use super::merkle;
use airk_core::{graph_storage::*, identity::Sha256Digest};
use anyhow::{Context, Result, ensure};

#[derive(Clone)]
pub(super) struct Proof {
    pub record: Option<GraphRecord>,
    pub siblings: merkle::Siblings,
}
impl Proof {
    pub fn empty(head: &GraphHead, id: u32) -> Result<Self> {
        ensure!(
            head.allocated_count == 0,
            "missing base for allocated graph"
        );
        let mut siblings = [Sha256Digest::from_bytes([0; 32]); 32];
        for (height, sibling) in siblings.iter_mut().enumerate() {
            *sibling = empty_hash(&head.scope, height as u8)?;
        }
        merkle::verify_absent(head, id, &siblings)?;
        Ok(Self {
            record: None,
            siblings,
        })
    }
}

pub(super) fn read_proof(
    connection: &dyn super::GraphStorage,
    head: &GraphHead,
    id: u32,
) -> Result<Proof> {
    head.validate()?;
    let record = connection.graph_record(&head.scope, id)?;
    let mut siblings = [Sha256Digest::from_bytes([0; 32]); 32];
    let mut index = id;
    for (height, sibling) in siblings.iter_mut().enumerate() {
        *sibling = connection
            .graph_node(height as u8, index ^ 1)?
            .unwrap_or(empty_hash(&head.scope, height as u8)?);
        index >>= 1;
    }
    match &record {
        Some(record) => verify_record(head, id, record, &siblings)?,
        None => merkle::verify_absent(head, id, &siblings)?,
    }
    Ok(Proof { record, siblings })
}

pub(crate) fn read_record(
    connection: &dyn super::GraphStorage,
    head: &GraphHead,
    id: u32,
) -> Result<GraphRecord> {
    ensure!(
        u64::from(id) < head.allocated_count,
        "graph ID outside committed allocation"
    );
    read_proof(connection, head, id)?
        .record
        .context("allocated graph record missing")
}
