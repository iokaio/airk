// SPDX-License-Identifier: Apache-2.0
//! Bounded ACK identity lookup, separate from native parent authorization.
use super::*;

#[test]
fn checkpoint_ack_parent_lookup_is_once_per_snapshot_and_requires_exact_identity() -> Result<()> {
    let db = Connection::open_in_memory()?;
    db.execute_batch("CREATE TABLE index_split_operations(operation_id TEXT PRIMARY KEY)")?;
    for n in 0u128..64 {
        db.execute(
            "INSERT INTO index_split_operations VALUES(?1)",
            [Incarnation::from_bytes(n.to_be_bytes()).to_string()],
        )?;
    }
    let operation = Incarnation::from_bytes(63u128.to_be_bytes());
    let id = identity(operation, b"ack")?.to_string();
    let mut classifier = CheckpointReceiptClassifier::default();
    assert_eq!(classifier.ack_operation(&db, &id)?, operation);
    assert_eq!(classifier.ack_operations.as_ref().unwrap().len(), 64);
    db.execute_batch("DROP TABLE index_split_operations")?;
    assert_eq!(
        classifier.ack_operation(&db, &id)?,
        operation,
        "later ACK lookup must use the captured mapping, not rescan metadata"
    );
    assert!(
        classifier
            .ack_operation(
                &db,
                &identity(Incarnation::from_bytes([255; 16]), b"ack")?.to_string()
            )
            .is_err()
    );
    // Lookup alone grants no terminal status: the real native parent must
    // still be read and validated even when its operation was cached.
    assert!(
        classifier
            .terminal(&db, &id, ACK_KIND, "acknowledged", 1, b"[]")
            .is_err()
    );
    Ok(())
}
