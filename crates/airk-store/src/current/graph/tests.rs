// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::quota::Quota;
use airk_core::identity::{Incarnation, Sha256Digest};
use rusqlite::{Connection, params};

fn scope() -> GraphScope {
    GraphScope {
        collection: "graph-fixture".into(),
        root: "original-graph-source".into(),
        incarnation: Incarnation::from_bytes([1; 16]),
        graph_instance: Incarnation::from_bytes([2; 16]),
        specification: Sha256Digest::from_bytes([3; 32]),
        record_bytes: 32,
    }
}
fn record(id: u32) -> GraphRecord {
    GraphRecord {
        embedding_key: (id != 0).then(|| {
            airk_core::identity::framed_digest(b"original-graph-fixture/1", &[&id.to_be_bytes()])
        }),
        live: id != 0,
        bytes: vec![(id % 251) as u8; 32],
    }
}
fn limits() -> GraphOverlayLimits {
    GraphOverlayLimits {
        maximum_bytes: 1024 * 1024,
        ..Default::default()
    }
}
fn connection() -> Result<Connection> {
    let connection = Connection::open_in_memory()?;
    create(&connection)?;
    Ok(connection)
}
fn candidate(reservation: &Reservation, count: u32) -> Result<GraphOverlay> {
    let overlay = GraphOverlay::empty(reservation, scope(), limits())?;
    for id in 0..count {
        overlay.append(id, record(id))?;
    }
    Ok(overlay)
}
fn txn_id() -> Sha256Digest {
    Sha256Digest::from_bytes([9; 32])
}
fn publish(connection: &mut Connection, delta: &dyn PreparedGraphDelta) -> Result<()> {
    let transaction = connection.transaction()?;
    let descriptor = stage(&transaction, delta, txn_id())?;
    apply(&transaction, txn_id(), &descriptor)?;
    transaction.commit()?;
    Ok(())
}

#[test]
fn sparse_empty_proofs_and_paged_afterimages_authenticate_every_record() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let quota = Quota::open(directory.path())?;
    let reservation = quota.reserve("graph-fixture", 2 * 1024 * 1024)?;
    let sealed = candidate(&reservation, 130)?.seal()?;
    assert_eq!(sealed.descriptor().record_count, 130);
    assert!(sealed.descriptor().node_count <= 33 * 130);
    assert_eq!(sealed.statistics().sealed_records, 130);
    assert!(sealed.statistics().private_page_bytes <= limits().maximum_bytes);
    let mut database = connection()?;
    let empty = GraphHead::empty(scope())?;
    for id in [0, 1, 0x8000_0000, u32::MAX] {
        assert!(sql::read_proof(&database, &empty, id)?.record.is_none());
    }
    publish(&mut database, &sealed)?;
    for id in 0..130 {
        assert_eq!(
            read_record(&database, &sealed.descriptor().target, id)?,
            record(id)
        );
    }
    assert!(read_record(&database, &sealed.descriptor().target, 130).is_err());
    assert!(
        sql::read_proof(&database, &sealed.descriptor().target, u32::MAX)?
            .record
            .is_none()
    );
    let pending: i64 =
        database.query_row("SELECT COUNT(*) FROM pending_graph_records", [], |row| {
            row.get(0)
        })?;
    assert_eq!(pending, 0);
    Ok(())
}

#[test]
fn committed_record_scope_id_key_live_payload_and_siblings_cannot_be_substituted() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let quota = Quota::open(directory.path())?;
    let reservation = quota.reserve("graph-fixture", 2 * 1024 * 1024)?;
    let sealed = candidate(&reservation, 3)?.seal()?;
    for mutation in [
        "UPDATE graph_records SET live=0 WHERE id=1",
        "UPDATE graph_records SET embedding_key=zeroblob(32) WHERE id=1",
        "UPDATE graph_records SET record=zeroblob(32) WHERE id=1",
        "UPDATE graph_records SET record=(SELECT record FROM graph_records WHERE id=2) WHERE id=1",
        "DELETE FROM graph_records WHERE id=1",
        "UPDATE graph_merkle SET digest=zeroblob(32) WHERE height=0 AND idx=0",
        "DELETE FROM graph_merkle WHERE height=0 AND idx=0",
    ] {
        let mut database = connection()?;
        publish(&mut database, &sealed)?;
        database.execute_batch(mutation)?;
        assert!(
            read_record(&database, &sealed.descriptor().target, 1).is_err(),
            "{mutation}"
        );
    }
    let mut database = connection()?;
    publish(&mut database, &sealed)?;
    let mut foreign = sealed.descriptor().target.clone();
    foreign.scope.graph_instance = Incarnation::from_bytes([7; 16]);
    assert!(read_record(&database, &foreign, 1).is_err());
    Ok(())
}

#[test]
fn latest_afterimages_are_stable_across_repeated_writes_and_tombstone_toggles() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let quota = Quota::open(directory.path())?;
    let first = quota.reserve("graph-fixture", 2 * 1024 * 1024)?;
    let second = quota.reserve("graph-fixture", 2 * 1024 * 1024)?;
    let a = candidate(&first, 3)?;
    let b = candidate(&second, 3)?;
    let mut changed = record(1);
    changed.bytes[0] = 99;
    a.put(1, changed.clone())?;
    a.put(1, record(1))?;
    a.set_live(1, false)?;
    a.set_live(1, true)?;
    let a = a.seal()?;
    let b = b.seal()?;
    assert_eq!(a.descriptor(), b.descriptor());
    assert!(a.statistics().write_operations > b.statistics().write_operations);
    Ok(())
}

#[test]
fn nonempty_base_changes_remain_proven_and_seal_releases_the_reader() -> Result<()> {
    use crate::{
        FileStore,
        current::{CurrentHead, CurrentScope},
    };
    let directory = tempfile::tempdir()?;
    let quota = Quota::open(directory.path())?;
    let store = FileStore::new(quota.root().join("collections-1.2/graph-fixture"));
    let graph_scope = scope();
    let scope = CurrentScope {
        root: graph_scope.root.clone(),
        collection: graph_scope.collection.clone(),
        incarnation: graph_scope.incarnation,
    };
    let mut head = CurrentHead {
        version: 2,
        scope: scope.clone(),
        sequence: 0,
        artifact_root: crate::current::map::empty_root(&scope)?,
        lexical_sequence: 0,
        lexical_commit: Sha256Digest::from_bytes([5; 32]),
        graph_head: GraphHead::empty(graph_scope)?,
    };
    let capacity = 4 * 1024 * 1024;
    {
        let lock = store.lock()?;
        store
            .current_write(&lock, &quota, &scope, capacity)?
            .initialize(head.clone())?;
    }
    for turn in 0..3 {
        let reservation = quota.reserve("graph-fixture", 2 * 1024 * 1024)?;
        let read = store.current_read(&scope)?;
        let weak = Arc::downgrade(&read);
        let overlay = GraphOverlay::new(&reservation, read, limits())?;
        if turn == 0 {
            for id in 0..3 {
                overlay.append(id, record(id))?;
            }
        } else if turn == 1 {
            assert_eq!(overlay.read(1)?, record(1));
            assert_eq!(overlay.read(1)?, record(1));
            assert_eq!(overlay.statistics()?.base_proof_cache_hits, 1);
            let mut changed = record(1);
            changed.bytes[0] = 42;
            overlay.put(1, changed)?;
            overlay.set_live(2, false)?;
            overlay.append(3, record(3))?;
            assert_eq!(overlay.read(0)?, record(0));
        }
        let sealed = overlay.seal()?;
        assert!(
            weak.upgrade().is_none(),
            "sealed graph retained its shared gate"
        );
        if turn == 2 {
            assert_eq!(sealed.descriptor().record_count, 0);
            assert_eq!(sealed.descriptor().node_count, 0);
            assert_eq!(sealed.descriptor().target, head.graph_head);
        }
        let mut target = head.clone();
        target.sequence += 1;
        target.graph_head = sealed.descriptor().target.clone();
        let lock = store.lock()?;
        let mut writer = store.current_write(&lock, &quota, &scope, capacity)?;
        let pending = writer.prepare(&head.token()?, target.clone(), &sealed, &[], None)?;
        writer.mark_applying(&pending)?;
        writer.commit(&pending, target.lexical_commit)?;
        head = target;
    }
    let read = store.current_read(&scope)?;
    assert_eq!(
        read.counts()?,
        GraphCounts {
            allocated_count: 4,
            live_nodes: 2
        }
    );
    assert_eq!(read.read(1)?.bytes[0], 42);
    assert!(!read.read(2)?.live);
    assert_eq!(read.read(3)?, record(3));
    Ok(())
}

#[test]
fn invalid_mutation_quota_limits_and_explicit_poison_prevent_sealing() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let quota = Quota::open(directory.path())?;
    for mode in 0..5 {
        let reservation = quota.reserve("graph-fixture", 2 * 1024 * 1024)?;
        let overlay = candidate(&reservation, 2)?;
        match mode {
            0 => assert!(overlay.append(9, record(9)).is_err()),
            1 => assert!(overlay.put(1, record(2)).is_err()),
            2 => assert!(overlay.set_live(0, true).is_err()),
            3 => assert!(overlay.read(2).is_err()),
            _ => overlay.poison(),
        }
        assert!(overlay.counts().is_err());
        assert!(overlay.seal().is_err());
    }
    let reservation = quota.reserve("graph-fixture", 2 * 1024 * 1024)?;
    let overlay = GraphOverlay::empty(
        &reservation,
        scope(),
        GraphOverlayLimits {
            maximum_dirty_records: 1,
            ..limits()
        },
    )?;
    overlay.append(0, record(0))?;
    assert!(overlay.append(1, record(1)).is_err());
    assert!(overlay.seal().is_err());
    let reservation = quota.reserve("graph-fixture", 16 * 1024)?;
    assert!(GraphOverlay::empty(&reservation, scope(), limits()).is_err());
    let reservation = quota.reserve("graph-fixture", 64 * 1024)?;
    let overlay = GraphOverlay::empty(
        &reservation,
        scope(),
        GraphOverlayLimits {
            maximum_bytes: 64 * 1024,
            ..limits()
        },
    )?;
    let exhausted = (0..512).any(|id| overlay.append(id, record(id)).is_err());
    assert!(exhausted, "private redb capacity was not enforced");
    assert!(overlay.seal().is_err());
    Ok(())
}

#[test]
fn operation_statistics_never_reject_or_poison_valid_graph_work() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let quota = Quota::open(directory.path())?;
    for prior in [9_999_999, 99_999_999, u64::MAX - 1] {
        let reservation = quota.reserve("graph-fixture", 2 * 1024 * 1024)?;
        let overlay = candidate(&reservation, 2)?;
        // Exercise both former ceilings and counter saturation without spending
        // millions of redundant reads merely to advance a diagnostic counter.
        overlay.state.lock().unwrap().statistics.operations = prior;
        overlay.state.lock().unwrap().statistics.read_bytes = u64::MAX - 1;
        assert_eq!(overlay.read(1)?, record(1));
        overlay.set_live(1, false)?;
        assert_eq!(overlay.counts()?.live_nodes, 0);
        assert!(!overlay.read(1)?.live);
        let sealed = overlay.seal()?;
        assert_eq!(sealed.statistics().operations, prior.saturating_add(4));
        assert_eq!(sealed.statistics().read_bytes, u64::MAX);
        assert_eq!(sealed.descriptor().target.live_nodes, 0);
    }
    Ok(())
}

#[test]
fn indexing_overlay_capacity_tracks_remaining_quota_and_keeps_other_staging_room() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let quota = Quota::open(directory.path())?;
    for (allowance, charged, expected) in [
        (512 << 20, 256 << 20, 64 << 20),
        (4 << 30, 0, 1 << 30),
        (32 << 30, 0, 4 << 30),
    ] {
        let reservation = quota.reserve("graph-fixture", allowance)?;
        reservation.charge(charged)?;
        let limits = GraphOverlayLimits::for_indexing(&reservation);
        assert_eq!(limits.maximum_bytes, expected);
        assert_eq!(limits.maximum_dirty_records, 1_000_000);
        let overlay = GraphOverlay::empty(&reservation, scope(), limits)?;
        overlay.append(0, record(0))?;
        assert!(reservation.remaining() >= (allowance - charged) * 3 / 4);
        let sealed = overlay.seal()?;
        assert!(sealed.statistics().private_page_bytes < 1 << 20);
    }
    Ok(())
}

#[test]
fn corrupt_private_rows_and_sealed_afterimages_are_rejected() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let quota = Quota::open(directory.path())?;
    let reservation = quota.reserve("graph-fixture", 2 * 1024 * 1024)?;
    let overlay = candidate(&reservation, 2)?;
    {
        use redb::ReadableTable;
        let state = overlay.state.lock().unwrap();
        let mut table = state
            .database
            .transaction()
            .open_table(PrivateTable::Records.definition())?;
        let key = 1u32.to_be_bytes();
        let mut bytes = table.get(key.as_slice())?.unwrap().value().to_vec();
        *bytes.last_mut().unwrap() ^= 1;
        table.insert(key.as_slice(), bytes.as_slice())?;
    }
    assert!(overlay.read(1).is_err());
    assert!(overlay.seal().is_err());
    let reservation = quota.reserve("graph-fixture", 2 * 1024 * 1024)?;
    let sealed = candidate(&reservation, 2)?.seal()?;
    {
        let transaction = sealed.database.database().begin_write()?;
        let mut table = transaction.open_table(PrivateTable::Nodes.definition())?;
        table.insert([32, 0, 0, 0, 0].as_slice(), [0u8; 32].as_slice())?;
        drop(table);
        transaction.commit()?;
    }
    assert!(sealed.visit_nodes(&mut |_| Ok(())).is_err());
    let mut database = connection()?;
    {
        let transaction = database.transaction()?;
        assert!(stage(&transaction, &sealed, txn_id()).is_err());
    }
    assert_eq!(
        database.query_row("SELECT COUNT(*) FROM pending_graph_records", [], |row| row
            .get::<_, i64>(
            0
        ))?,
        0
    );
    Ok(())
}

struct Forged {
    descriptor: GraphDeltaDescriptor,
    records: Vec<(u32, GraphRecord)>,
    nodes: Vec<GraphMerkleNode>,
}
impl PreparedGraphDelta for Forged {
    fn descriptor(&self) -> &GraphDeltaDescriptor {
        &self.descriptor
    }
    fn visit_records(
        &self,
        visitor: &mut dyn FnMut(u32, &GraphRecord) -> Result<()>,
    ) -> Result<()> {
        for (id, record) in &self.records {
            visitor(*id, record)?;
        }
        Ok(())
    }
    fn visit_nodes(&self, visitor: &mut dyn FnMut(GraphMerkleNode) -> Result<()>) -> Result<()> {
        for node in &self.nodes {
            visitor(*node)?;
        }
        Ok(())
    }
}

#[test]
fn staging_derives_root_instead_of_trusting_self_consistent_forged_descriptor() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let quota = Quota::open(directory.path())?;
    let reservation = quota.reserve("graph-fixture", 2 * 1024 * 1024)?;
    let sealed = candidate(&reservation, 2)?.seal()?;
    let mut forged = Forged {
        descriptor: sealed.descriptor().clone(),
        records: vec![],
        nodes: vec![],
    };
    sealed.visit_records(&mut |id, record| {
        forged.records.push((id, record.clone()));
        Ok(())
    })?;
    sealed.visit_nodes(&mut |node| {
        forged.nodes.push(node);
        Ok(())
    })?;
    forged.descriptor.target.merkle_root = Sha256Digest::from_bytes([8; 32]);
    let mut records =
        digest::Stream::new(true, &forged.descriptor.base, &forged.descriptor.target)?;
    for (id, record) in &forged.records {
        records.record(&scope(), *id, record)?;
    }
    forged.descriptor.records_digest = records.finish();
    let mut nodes = digest::Stream::new(false, &forged.descriptor.base, &forged.descriptor.target)?;
    for node in &forged.nodes {
        nodes.node(*node)?;
    }
    forged.descriptor.nodes_digest = nodes.finish();
    let mut database = connection()?;
    let transaction = database.transaction()?;
    assert!(
        stage(&transaction, &forged, txn_id())
            .unwrap_err()
            .to_string()
            .contains("target root/count")
    );
    Ok(())
}

#[test]
fn recovery_rejects_changed_missing_or_extra_pending_afterimages() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let quota = Quota::open(directory.path())?;
    let reservation = quota.reserve("graph-fixture", 2 * 1024 * 1024)?;
    let sealed = candidate(&reservation, 2)?.seal()?;
    for mutation in [
        "UPDATE pending_graph_records SET live=0 WHERE id=1",
        "DELETE FROM pending_graph_records WHERE id=1",
        "UPDATE pending_graph_merkle SET digest=zeroblob(32) WHERE height=32",
        "INSERT INTO pending_graph_records SELECT txn,2,embedding_key,live,record FROM pending_graph_records WHERE id=1",
    ] {
        let mut database = connection()?;
        {
            let transaction = database.transaction()?;
            stage(&transaction, &sealed, txn_id())?;
            transaction.commit()?;
        }
        database.execute_batch(mutation)?;
        {
            let transaction = database.transaction()?;
            assert!(
                apply(&transaction, txn_id(), sealed.descriptor()).is_err(),
                "{mutation}"
            );
        }
        assert_eq!(
            database.query_row("SELECT COUNT(*) FROM graph_records", [], |row| row
                .get::<_, i64>(0))?,
            0
        );
    }
    Ok(())
}

#[test]
fn oversized_row_is_bounded_before_decode_even_if_schema_checks_are_bypassed() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let quota = Quota::open(directory.path())?;
    let reservation = quota.reserve("graph-fixture", 2 * 1024 * 1024)?;
    let sealed = candidate(&reservation, 2)?.seal()?;
    let mut database = connection()?;
    publish(&mut database, &sealed)?;
    database.execute_batch("PRAGMA ignore_check_constraints=ON;")?;
    database.execute(
        "UPDATE graph_records SET record=?1 WHERE id=1",
        params![vec![0u8; 64 * 1024]],
    )?;
    assert!(
        read_record(&database, &sealed.descriptor().target, 1)
            .unwrap_err()
            .to_string()
            .contains("fixed byte bound")
    );
    Ok(())
}
