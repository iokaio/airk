-- SPDX-License-Identifier: Apache-2.0
-- Exact policy-3 addition to owned policy-2. No public snapshot embeds this body.
CREATE TABLE index_split_extraction_policies(
 operation_id TEXT PRIMARY KEY REFERENCES index_split_operations(operation_id) ON DELETE CASCADE,
 body BLOB NOT NULL CHECK(length(body) BETWEEN 1 AND 131072)) WITHOUT ROWID;
CREATE TRIGGER index_split_extraction_immutable BEFORE UPDATE ON index_split_extraction_policies
 BEGIN SELECT RAISE(ABORT,'split extraction policy is immutable'); END;
