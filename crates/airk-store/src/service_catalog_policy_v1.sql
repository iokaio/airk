-- SPDX-License-Identifier: Apache-2.0
-- Complete policy-1 extension to the exact schema-12 authority-1 layout.
CREATE TABLE index_source_policies(
 index_id TEXT PRIMARY KEY REFERENCES index_definitions(id) ON DELETE RESTRICT,
 payload BLOB NOT NULL CHECK(length(payload)<=1048576),
 completion_reserve BLOB NOT NULL CHECK(length(completion_reserve)<=16384)) WITHOUT ROWID;
CREATE TRIGGER index_source_policies_write_insert AFTER INSERT ON index_source_policies
 WHEN EXISTS(SELECT 1 FROM index_catalog_meta WHERE id=1 AND activated=1 AND accepted_new_writes=0)
 BEGIN UPDATE index_catalog_meta SET accepted_new_writes=1 WHERE id=1; END;
CREATE TRIGGER index_source_policies_write_update AFTER UPDATE ON index_source_policies
 WHEN EXISTS(SELECT 1 FROM index_catalog_meta WHERE id=1 AND activated=1 AND accepted_new_writes=0)
 BEGIN UPDATE index_catalog_meta SET accepted_new_writes=1 WHERE id=1; END;
