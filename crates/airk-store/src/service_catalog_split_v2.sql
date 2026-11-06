-- SPDX-License-Identifier: Apache-2.0
-- Policy contract 2 adds independent split journals; older layouts refuse it.
CREATE TABLE index_split_operations(
 operation_id TEXT PRIMARY KEY CHECK(length(operation_id)=32),
 caller TEXT NOT NULL CHECK(length(caller)=32),namespace TEXT NOT NULL CHECK(length(namespace)=32),
 epoch INTEGER NOT NULL CHECK(epoch>=0),nonce TEXT NOT NULL CHECK(length(nonce)=32),
 original_index TEXT NOT NULL,destination_index TEXT NOT NULL,
 revision INTEGER NOT NULL CHECK(revision>=1),receipt_digest TEXT NOT NULL CHECK(length(receipt_digest)=64),
 payload BLOB NOT NULL CHECK(length(payload)<=49152),
 completion_reserve BLOB NOT NULL CHECK(length(completion_reserve)<=3145728),
 UNIQUE(caller,namespace,epoch,nonce)) WITHOUT ROWID;
CREATE TABLE index_split_participants(
 index_id TEXT PRIMARY KEY,operation_id TEXT NOT NULL REFERENCES index_split_operations(operation_id) ON DELETE RESTRICT,
 role TEXT NOT NULL CHECK(role IN('original_remainder','destination')),
 active INTEGER NOT NULL CHECK(active IN(0,1))) WITHOUT ROWID;
CREATE TABLE index_split_history(
 operation_id TEXT NOT NULL REFERENCES index_split_operations(operation_id) ON DELETE RESTRICT,
 revision INTEGER NOT NULL CHECK(revision>=1),digest TEXT NOT NULL CHECK(length(digest)=64),
 payload BLOB NOT NULL CHECK(length(payload)<=16384),PRIMARY KEY(operation_id,revision)) WITHOUT ROWID;
CREATE TABLE index_split_jobs(
 job TEXT PRIMARY KEY CHECK(length(job)=64),
 operation_id TEXT NOT NULL REFERENCES index_split_operations(operation_id) ON DELETE RESTRICT,
 payload BLOB NOT NULL CHECK(length(payload)<=2048)) WITHOUT ROWID;
CREATE TABLE index_split_recoveries(
 caller TEXT NOT NULL,namespace TEXT NOT NULL,epoch INTEGER NOT NULL CHECK(epoch>=0),nonce TEXT NOT NULL,
 operation_id TEXT NOT NULL REFERENCES index_split_operations(operation_id) ON DELETE RESTRICT,
 payload BLOB NOT NULL CHECK(length(payload)<=16384),PRIMARY KEY(caller,namespace,epoch,nonce)) WITHOUT ROWID;
CREATE TRIGGER index_split_capacity BEFORE INSERT ON index_split_operations
 WHEN (SELECT count(*) FROM index_split_operations)>=64
 BEGIN SELECT RAISE(ABORT,'split operation receipt capacity exceeded'); END;
CREATE TRIGGER index_split_history_capacity BEFORE INSERT ON index_split_history
 WHEN (SELECT count(*) FROM index_split_history WHERE operation_id=NEW.operation_id)>=64
 BEGIN SELECT RAISE(ABORT,'split transition receipt capacity exceeded'); END;
CREATE TRIGGER index_split_recovery_capacity BEFORE INSERT ON index_split_recoveries
 WHEN (SELECT count(*) FROM index_split_recoveries WHERE operation_id=NEW.operation_id)>=16
 BEGIN SELECT RAISE(ABORT,'split recovery receipt capacity exceeded'); END;
CREATE TRIGGER index_split_write_insert AFTER INSERT ON index_split_operations
 BEGIN UPDATE index_catalog_meta SET accepted_new_writes=1 WHERE id=1; END;
CREATE TRIGGER index_split_write_update AFTER UPDATE ON index_split_operations
 BEGIN UPDATE index_catalog_meta SET accepted_new_writes=1 WHERE id=1; END;
