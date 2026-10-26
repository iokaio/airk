// SPDX-License-Identifier: Apache-2.0
use super::{GraphOverlayLimits, State, digest, merkle, private, sql};
use crate::quota::{PrivateKv, PrivateTable};
use airk_core::{graph_storage::*, identity::Sha256Digest};
use anyhow::{Result, ensure};

pub(super) fn seal(
    base: &GraphHead,
    limits: GraphOverlayLimits,
    state: &mut State,
) -> Result<GraphDeltaDescriptor> {
    let stored = state.database.len(PrivateTable::Records)?;
    ensure!(
        stored == state.statistics.dirty_records && stored <= limits.maximum_dirty_records,
        "private graph row count changed"
    );
    // Two bounded dirty-row passes, base proofs, candidate sibling reads and
    // at most 33 final nodes per changed ID. Charge this conservative allowance
    // as logical work; it does not represent resident memory or physical I/O.
    let read_bytes = stored
        .checked_mul(3 * u64::from(base.scope.record_bytes) + 64 + 2 * 32 * 32 + 33 * 37)
        .ok_or_else(|| anyhow::anyhow!("graph seal read bound overflow"))?;
    state.statistics.read_bytes = state.statistics.read_bytes.saturating_add(read_bytes);
    let mut root = base.merkle_root;
    let mut counts = base.counts();
    let mut cursor = None;
    loop {
        let page = private::record_page(&state.database, &base.scope, cursor)?;
        if page.is_empty() {
            break;
        }
        for (id, record) in page {
            ensure!(
                state.statistics.sealed_records < stored,
                "extra private graph row"
            );
            let proof = match &state.base {
                Some(reader) => {
                    state.statistics.base_record_reads += 1;
                    state.statistics.proof_nodes += 32;
                    reader.with_connection(|connection| sql::read_proof(connection, base, id))?
                }
                None => sql::Proof::empty(base, id)?,
            };
            update_counts(&mut counts, base, id, proof.record.as_ref(), &record)?;
            let connection = &state.database;
            root = merkle::update_path(
                &base.scope,
                id,
                proof.record.as_ref(),
                &record,
                (&proof.siblings, root),
                |height, index| private::node(connection, height, index),
                |node| private::put_node(connection, node),
            )?;
            cursor = Some(id);
            state.statistics.sealed_records += 1;
        }
    }
    ensure!(
        counts == state.counts && state.statistics.sealed_records == stored,
        "graph candidate counts changed"
    );
    let target = GraphHead {
        scope: base.scope.clone(),
        allocated_count: counts.allocated_count,
        live_nodes: counts.live_nodes,
        merkle_root: root,
    };
    target.validate()?;
    let zero = Sha256Digest::from_bytes([0; 32]);
    let node_count = state.database.len(PrivateTable::Nodes)?;
    state.statistics.merkle_afterimages = node_count;
    let mut descriptor = GraphDeltaDescriptor {
        version: GRAPH_STORAGE_VERSION,
        base: base.clone(),
        target,
        record_count: stored,
        node_count,
        record_bytes: stored * u64::from(base.scope.record_bytes),
        records_digest: zero,
        nodes_digest: zero,
    };
    digest::descriptor_bounds(&descriptor)?;
    let mut records = digest::Stream::new(true, &descriptor.base, &descriptor.target)?;
    raw_records(&state.database, &descriptor, &mut |id, record| {
        records.record(&base.scope, id, record)
    })?;
    descriptor.records_digest = records.finish();
    let mut nodes = digest::Stream::new(false, &descriptor.base, &descriptor.target)?;
    raw_nodes(&state.database, &mut |node| nodes.node(node))?;
    descriptor.nodes_digest = nodes.finish();
    Ok(descriptor)
}

pub(super) fn update_counts(
    counts: &mut GraphCounts,
    base: &GraphHead,
    id: u32,
    old: Option<&GraphRecord>,
    new: &GraphRecord,
) -> Result<()> {
    new.validate(&base.scope, id)?;
    match old {
        Some(old) => {
            ensure!(
                u64::from(id) < base.allocated_count && old.embedding_key == new.embedding_key,
                "existing graph identity changed"
            );
            counts.live_nodes = counts
                .live_nodes
                .checked_sub(u64::from(old.live))
                .ok_or_else(|| anyhow::anyhow!("graph live count underflow"))?;
        }
        None => {
            ensure!(
                u64::from(id) == counts.allocated_count,
                "graph afterimages contain allocation gap"
            );
            counts.allocated_count += 1;
        }
    }
    counts.live_nodes += u64::from(new.live);
    counts.validate()
}

fn raw_records(
    connection: &PrivateKv,
    descriptor: &GraphDeltaDescriptor,
    visitor: &mut dyn FnMut(u32, &GraphRecord) -> Result<()>,
) -> Result<()> {
    let mut cursor = None;
    let mut count = 0;
    loop {
        let page = private::record_page(connection, &descriptor.target.scope, cursor)?;
        if page.is_empty() {
            break;
        }
        for (id, record) in page {
            ensure!(
                count < descriptor.record_count,
                "extra graph record afterimage"
            );
            visitor(id, &record)?;
            cursor = Some(id);
            count += 1;
        }
    }
    ensure!(
        count == descriptor.record_count,
        "missing graph record afterimage"
    );
    Ok(())
}
fn raw_nodes(
    connection: &PrivateKv,
    visitor: &mut dyn FnMut(GraphMerkleNode) -> Result<()>,
) -> Result<()> {
    let mut cursor = None;
    loop {
        let page = private::node_page(connection, cursor)?;
        if page.is_empty() {
            break;
        }
        for node in page {
            visitor(node)?;
            cursor = Some((node.height, node.index));
        }
    }
    Ok(())
}
pub(super) fn visit_records(
    connection: &PrivateKv,
    descriptor: &GraphDeltaDescriptor,
    visitor: &mut dyn FnMut(u32, &GraphRecord) -> Result<()>,
) -> Result<()> {
    digest::descriptor_bounds(descriptor)?;
    let mut stream = digest::Stream::new(true, &descriptor.base, &descriptor.target)?;
    raw_records(connection, descriptor, &mut |id, record| {
        stream.record(&descriptor.target.scope, id, record)?;
        visitor(id, record)
    })?;
    ensure!(
        stream.count == descriptor.record_count
            && stream.bytes == descriptor.record_bytes
            && stream.finish() == descriptor.records_digest,
        "sealed graph record digest mismatch"
    );
    Ok(())
}
pub(super) fn visit_nodes(
    connection: &PrivateKv,
    descriptor: &GraphDeltaDescriptor,
    visitor: &mut dyn FnMut(GraphMerkleNode) -> Result<()>,
) -> Result<()> {
    digest::descriptor_bounds(descriptor)?;
    let mut stream = digest::Stream::new(false, &descriptor.base, &descriptor.target)?;
    raw_nodes(connection, &mut |node| {
        ensure!(
            stream.count < descriptor.node_count,
            "extra graph node afterimage"
        );
        stream.node(node)?;
        visitor(node)
    })?;
    ensure!(
        stream.count == descriptor.node_count && stream.finish() == descriptor.nodes_digest,
        "sealed graph node digest mismatch"
    );
    Ok(())
}
