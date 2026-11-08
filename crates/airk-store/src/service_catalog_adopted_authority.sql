-- SPDX-License-Identifier: Apache-2.0
-- Explicit schema-13 adoption authority layout. Called only in an owned native
-- transaction after exact old-shape validation. Preserve every original grant,
-- membership and scope byte; never reinterpret a managed upgrade's lineage.
CREATE TABLE adoption_grants_next(
 id TEXT PRIMARY KEY CHECK(substr(id,1,4)='grt_' AND length(id)=36 AND substr(id,5) NOT GLOB '*[^0-9a-f]*'),
 host TEXT NOT NULL CHECK(length(host)=64 AND host NOT GLOB '*[^0-9a-f]*'),
 revision INTEGER NOT NULL CHECK(revision>=1),
 revoked INTEGER NOT NULL CHECK(revoked IN(0,1)),
 provenance_kind TEXT NOT NULL DEFAULT 'reviewed' CHECK(provenance_kind IN('reviewed','imported_legacy','imported_standalone_adoption')),
 import_upgrade_id TEXT CHECK(import_upgrade_id IS NULL OR length(import_upgrade_id)=32 AND import_upgrade_id NOT GLOB '*[^0-9a-f]*'),
 import_plan_digest TEXT CHECK(import_plan_digest IS NULL OR length(import_plan_digest)=64 AND import_plan_digest NOT GLOB '*[^0-9a-f]*'),
 import_legacy_binding TEXT REFERENCES legacy_index_projects(legacy_binding) ON DELETE RESTRICT,
 import_configuration_digest TEXT CHECK(import_configuration_digest IS NULL OR length(import_configuration_digest)=64 AND import_configuration_digest NOT GLOB '*[^0-9a-f]*'),
 CHECK((provenance_kind='reviewed' AND substr(id,5,1) IN('0','1','2','3','4','5','6','7')
     AND import_upgrade_id IS NULL AND import_plan_digest IS NULL AND import_legacy_binding IS NULL AND import_configuration_digest IS NULL)
   OR (provenance_kind='imported_legacy' AND substr(id,5,1) IN('8','9','a','b','c','d','e','f')
     AND import_upgrade_id IS NOT NULL AND import_plan_digest IS NOT NULL AND import_legacy_binding IS NOT NULL AND import_configuration_digest IS NOT NULL)
   OR (provenance_kind='imported_standalone_adoption' AND substr(id,5,1) IN('8','9','a','b','c','d','e','f')
     AND import_upgrade_id IS NULL AND import_plan_digest IS NULL AND import_legacy_binding IS NULL AND import_configuration_digest IS NULL))) WITHOUT ROWID;
INSERT INTO adoption_grants_next SELECT id,host,revision,revoked,provenance_kind,import_upgrade_id,import_plan_digest,import_legacy_binding,import_configuration_digest FROM index_access_grants;
CREATE TABLE adoption_memberships_next(
 grant_id TEXT NOT NULL REFERENCES adoption_grants_next(id) ON DELETE CASCADE,
 project_id TEXT NOT NULL CHECK(substr(project_id,1,4)='prj_' AND length(project_id)=36 AND substr(project_id,5) NOT GLOB '*[^0-9a-f]*'),
 index_id TEXT NOT NULL REFERENCES index_definitions(id) ON DELETE RESTRICT,
 membership_revision INTEGER NOT NULL CHECK(membership_revision>=1),
 source_revision INTEGER NOT NULL CHECK(source_revision>=1),
 policy_epoch INTEGER NOT NULL CHECK(policy_epoch>=0),
 policy_digest TEXT CHECK(policy_digest IS NULL OR length(policy_digest)=64 AND policy_digest NOT GLOB '*[^0-9a-f]*'),
 CHECK((policy_epoch=0 AND policy_digest IS NULL) OR (policy_epoch>0 AND policy_digest IS NOT NULL)),
 PRIMARY KEY(grant_id,project_id,index_id)) WITHOUT ROWID;
INSERT INTO adoption_memberships_next SELECT * FROM index_access_memberships;
CREATE TABLE adoption_scopes_next(
 grant_id TEXT NOT NULL, project_id TEXT NOT NULL, index_id TEXT NOT NULL, source_id TEXT NOT NULL,
 kind TEXT NOT NULL CHECK(kind IN('exact','subtree')),
 path TEXT NOT NULL CHECK(length(CAST(path AS BLOB))<=4096 AND (kind='subtree' OR path<>'')),
 PRIMARY KEY(grant_id,project_id,index_id,source_id,kind,path),
 FOREIGN KEY(grant_id,project_id,index_id) REFERENCES adoption_memberships_next(grant_id,project_id,index_id) ON DELETE CASCADE,
 FOREIGN KEY(index_id,source_id) REFERENCES index_sources(index_id,id) ON DELETE RESTRICT) WITHOUT ROWID;
INSERT INTO adoption_scopes_next SELECT * FROM index_access_scopes;
DROP TABLE index_access_scopes;
DROP TABLE index_access_memberships;
DROP TABLE index_access_grants;
ALTER TABLE adoption_grants_next RENAME TO index_access_grants;
ALTER TABLE adoption_memberships_next RENAME TO index_access_memberships;
ALTER TABLE adoption_scopes_next RENAME TO index_access_scopes;
CREATE INDEX index_grant_memberships ON index_access_memberships(index_id,grant_id,project_id);
CREATE INDEX source_grants ON index_access_scopes(source_id,grant_id);
CREATE UNIQUE INDEX index_imported_grant_binding ON index_access_grants(import_legacy_binding) WHERE provenance_kind='imported_legacy';
CREATE INDEX index_grant_provenance_kind ON index_access_grants(provenance_kind);
CREATE TRIGGER index_grant_source_ready BEFORE INSERT ON index_access_memberships
 WHEN NOT EXISTS(SELECT 1 FROM index_definitions WHERE id=NEW.index_id AND tombstone=0 AND source_import_state='complete')
 BEGIN SELECT RAISE(ABORT,'index source metadata is unavailable'); END;
CREATE TRIGGER index_grant_provenance_identity BEFORE UPDATE OF id,host,provenance_kind,import_upgrade_id,import_plan_digest,import_legacy_binding,import_configuration_digest ON index_access_grants
 BEGIN SELECT RAISE(ABORT,'index grant provenance identity is immutable'); END;
CREATE TRIGGER index_imported_grants_capacity BEFORE INSERT ON index_access_grants
 WHEN NEW.provenance_kind IN('imported_legacy','imported_standalone_adoption') AND (SELECT count(*) FROM index_access_grants WHERE provenance_kind IN('imported_legacy','imported_standalone_adoption'))>=1024
 BEGIN SELECT RAISE(ABORT,'imported compatibility grant capacity exceeded'); END;
CREATE TRIGGER index_reviewed_grants_capacity BEFORE INSERT ON index_access_grants
 WHEN NEW.provenance_kind='reviewed' AND (SELECT count(*) FROM index_access_grants WHERE provenance_kind='reviewed')>=1024
 BEGIN SELECT RAISE(ABORT,'reviewed project grant capacity exceeded'); END;
CREATE TABLE index_adopted_authorities(
 grant_id TEXT PRIMARY KEY REFERENCES index_access_grants(id) ON DELETE RESTRICT,
 operation_id TEXT NOT NULL UNIQUE CHECK(length(operation_id)=32 AND operation_id NOT GLOB '*[^0-9a-f]*'),
 adopted_binding TEXT NOT NULL UNIQUE REFERENCES index_definitions(legacy_binding) ON DELETE RESTRICT,
 original_binding TEXT NOT NULL CHECK(length(original_binding)=64 AND original_binding NOT GLOB '*[^0-9a-f]*'),
 config_sha256 TEXT NOT NULL CHECK(length(config_sha256)=64 AND config_sha256 NOT GLOB '*[^0-9a-f]*'),
 payload BLOB NOT NULL CHECK(length(payload)<=65536)) WITHOUT ROWID;
CREATE TRIGGER index_adopted_authority_immutable BEFORE UPDATE ON index_adopted_authorities
 BEGIN SELECT RAISE(ABORT,'standalone adoption authority is immutable'); END;
CREATE TRIGGER index_adopted_authority_retained BEFORE DELETE ON index_adopted_authorities
 BEGIN SELECT RAISE(ABORT,'standalone adoption authority is retained'); END;
CREATE TRIGGER index_adopted_authority_kind BEFORE INSERT ON index_adopted_authorities
 WHEN NOT EXISTS(SELECT 1 FROM index_access_grants WHERE id=NEW.grant_id AND provenance_kind='imported_standalone_adoption')
 BEGIN SELECT RAISE(ABORT,'standalone adoption authority requires native imported grant'); END;
-- The owning Rust adapter reinstalls the exact capacity/write barriers for the
-- rebuilt tables and the new companion, then validates the complete DDL shape.

CREATE TABLE index_adopted_configuration_claims(
 grant_id TEXT NOT NULL REFERENCES index_adopted_authorities(grant_id) ON DELETE RESTRICT,
 origin_sha256 TEXT NOT NULL UNIQUE CHECK(length(origin_sha256)=64 AND origin_sha256 NOT GLOB '*[^0-9a-f]*'),
 config_sha256 TEXT NOT NULL CHECK(length(config_sha256)=64 AND config_sha256 NOT GLOB '*[^0-9a-f]*'),
 PRIMARY KEY(grant_id,origin_sha256)) WITHOUT ROWID;
CREATE INDEX adopted_config_claim_lookup ON index_adopted_configuration_claims(config_sha256,grant_id);
CREATE TRIGGER adopted_configuration_claims_capacity BEFORE INSERT ON index_adopted_configuration_claims
 WHEN (SELECT count(*) FROM index_adopted_configuration_claims WHERE grant_id=NEW.grant_id)>=32
 BEGIN SELECT RAISE(ABORT,'standalone configuration claim capacity exceeded'); END;
CREATE TRIGGER adopted_configuration_claim_immutable BEFORE UPDATE ON index_adopted_configuration_claims
 BEGIN SELECT RAISE(ABORT,'standalone configuration claim is immutable'); END;
CREATE TRIGGER adopted_configuration_claim_retained BEFORE DELETE ON index_adopted_configuration_claims
 BEGIN SELECT RAISE(ABORT,'standalone configuration claim is retained'); END;
