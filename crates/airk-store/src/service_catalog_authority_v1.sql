-- SPDX-License-Identifier: Apache-2.0
-- Authority-contract 1 is an explicitly validated schema-12 physical layout.
-- This file is executed only inside the owned promotion transaction. Normal
-- reads never execute DDL, and the authority marker is set by that owner later.
ALTER TABLE index_access_grants ADD COLUMN provenance_kind TEXT NOT NULL DEFAULT 'reviewed'
 CHECK(provenance_kind IN('reviewed','imported_legacy'));
ALTER TABLE index_access_grants ADD COLUMN import_upgrade_id TEXT
 CHECK(import_upgrade_id IS NULL OR length(import_upgrade_id)=32 AND import_upgrade_id NOT GLOB '*[^0-9a-f]*');
ALTER TABLE index_access_grants ADD COLUMN import_plan_digest TEXT
 CHECK(import_plan_digest IS NULL OR length(import_plan_digest)=64 AND import_plan_digest NOT GLOB '*[^0-9a-f]*');
ALTER TABLE index_access_grants ADD COLUMN import_legacy_binding TEXT
 REFERENCES legacy_index_projects(legacy_binding) ON DELETE RESTRICT;
ALTER TABLE index_access_grants ADD COLUMN import_configuration_digest TEXT
 CHECK(import_configuration_digest IS NULL OR length(import_configuration_digest)=64 AND import_configuration_digest NOT GLOB '*[^0-9a-f]*')
 CHECK((provenance_kind='reviewed' AND substr(id,5,1) IN('0','1','2','3','4','5','6','7')
     AND import_upgrade_id IS NULL AND import_plan_digest IS NULL
     AND import_legacy_binding IS NULL AND import_configuration_digest IS NULL)
   OR (provenance_kind='imported_legacy' AND substr(id,5,1) IN('8','9','a','b','c','d','e','f')
     AND import_upgrade_id IS NOT NULL AND import_plan_digest IS NOT NULL
     AND import_legacy_binding IS NOT NULL AND import_configuration_digest IS NOT NULL));
CREATE UNIQUE INDEX index_imported_grant_binding ON index_access_grants(import_legacy_binding)
 WHERE provenance_kind='imported_legacy';
CREATE INDEX index_grant_provenance_kind ON index_access_grants(provenance_kind);
CREATE TRIGGER index_grant_provenance_identity BEFORE UPDATE OF id,host,provenance_kind,import_upgrade_id,import_plan_digest,import_legacy_binding,import_configuration_digest ON index_access_grants
 BEGIN SELECT RAISE(ABORT,'index grant provenance identity is immutable'); END;
DROP TRIGGER index_access_grants_capacity;
CREATE TRIGGER index_imported_grants_capacity BEFORE INSERT ON index_access_grants
 WHEN NEW.provenance_kind='imported_legacy' AND (SELECT count(*) FROM index_access_grants WHERE provenance_kind='imported_legacy')>=1024
 BEGIN SELECT RAISE(ABORT,'imported compatibility grant capacity exceeded'); END;
CREATE TRIGGER index_reviewed_grants_capacity BEFORE INSERT ON index_access_grants
 WHEN NEW.provenance_kind='reviewed' AND (SELECT count(*) FROM index_access_grants WHERE provenance_kind='reviewed')>=1024
 BEGIN SELECT RAISE(ABORT,'reviewed project grant capacity exceeded'); END;
