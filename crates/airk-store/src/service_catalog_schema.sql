-- SPDX-License-Identifier: Apache-2.0
-- Additive schema 12. Executed only by the owned upgrade adapter, never a read.
CREATE TABLE index_catalog_meta(
 id INTEGER PRIMARY KEY CHECK(id=1),
 revision INTEGER NOT NULL CHECK(revision>=1),
 activated INTEGER NOT NULL CHECK(activated IN(0,1)),
 accepted_new_writes INTEGER NOT NULL CHECK(accepted_new_writes IN(0,1)),
 authority_contract INTEGER NOT NULL CHECK(authority_contract>=0),
 policy_contract INTEGER NOT NULL CHECK(policy_contract>=0),
 upgrade_id TEXT NOT NULL CHECK(length(upgrade_id)=32 AND upgrade_id NOT GLOB '*[^0-9a-f]*'),
 legacy_schema_digest TEXT NOT NULL CHECK(length(legacy_schema_digest)=64 AND legacy_schema_digest NOT GLOB '*[^0-9a-f]*'),
 backup_digest TEXT NOT NULL CHECK(length(backup_digest)=64 AND backup_digest NOT GLOB '*[^0-9a-f]*'));

CREATE TABLE index_definitions(
 id TEXT PRIMARY KEY CHECK(substr(id,1,4)='idx_' AND length(id)=36 AND substr(id,5) NOT GLOB '*[^0-9a-f]*'),
 legacy_binding TEXT NOT NULL UNIQUE CHECK(length(legacy_binding)=64 AND legacy_binding NOT GLOB '*[^0-9a-f]*'),
 active_registration TEXT REFERENCES projects(binding) ON DELETE SET NULL,
 backing_root TEXT NOT NULL CHECK(length(CAST(backing_root AS BLOB)) BETWEEN 1 AND 4096),
 data_root TEXT NOT NULL CHECK(length(CAST(data_root AS BLOB)) BETWEEN 1 AND 4096),
 namespace TEXT NOT NULL CHECK(namespace='collections-1.4'),
 collection TEXT NOT NULL CHECK(length(CAST(collection AS BLOB)) BETWEEN 1 AND 128),
 name TEXT NOT NULL CHECK(length(CAST(name AS BLOB)) BETWEEN 1 AND 256),
 definition_revision INTEGER NOT NULL CHECK(definition_revision>=1),
 source_revision INTEGER NOT NULL CHECK(source_revision>=1),
 policy_epoch INTEGER NOT NULL CHECK(policy_epoch>=0),
 policy_digest TEXT CHECK(policy_digest IS NULL OR length(policy_digest)=64 AND policy_digest NOT GLOB '*[^0-9a-f]*'),
 tombstone INTEGER NOT NULL CHECK(tombstone IN(0,1)),
 source_import_state TEXT NOT NULL CHECK(source_import_state IN('complete','pending_metadata')),
 CHECK(active_registration IS NULL OR active_registration=legacy_binding),
 CHECK((policy_epoch=0 AND policy_digest IS NULL) OR (policy_epoch>0 AND policy_digest IS NOT NULL)),
 UNIQUE(data_root,namespace,collection)) WITHOUT ROWID;
CREATE TRIGGER index_definition_identity BEFORE UPDATE OF id,legacy_binding,backing_root,data_root,namespace,collection ON index_definitions
 BEGIN SELECT RAISE(ABORT,'index backing identity is immutable'); END;

CREATE TABLE index_sources(
 id TEXT PRIMARY KEY CHECK(substr(id,1,4)='src_' AND length(id)=36 AND substr(id,5) NOT GLOB '*[^0-9a-f]*'),
 index_id TEXT NOT NULL REFERENCES index_definitions(id) ON DELETE RESTRICT,
 canonical_path TEXT NOT NULL CHECK(length(CAST(canonical_path AS BLOB)) BETWEEN 1 AND 32768),
 owner INTEGER NOT NULL CHECK(owner IN(0,1)),
 enabled INTEGER NOT NULL CHECK(enabled IN(0,1)),
 UNIQUE(index_id,id), UNIQUE(index_id,canonical_path)) WITHOUT ROWID;
CREATE UNIQUE INDEX index_sources_primary ON index_sources(index_id) WHERE owner=1;
CREATE TRIGGER index_source_identity BEFORE UPDATE OF id,index_id,canonical_path,owner ON index_sources
 BEGIN SELECT RAISE(ABORT,'source identity is immutable'); END;
CREATE TABLE index_source_aliases(
 index_id TEXT NOT NULL,
 source_id TEXT NOT NULL,
 alias TEXT NOT NULL CHECK(length(CAST(alias AS BLOB)) BETWEEN 1 AND 128),
 PRIMARY KEY(index_id,alias),
 FOREIGN KEY(index_id,source_id) REFERENCES index_sources(index_id,id) ON DELETE CASCADE) WITHOUT ROWID;
CREATE INDEX index_alias_source ON index_source_aliases(source_id);

CREATE TABLE query_projects(
 id TEXT PRIMARY KEY CHECK(substr(id,1,4)='prj_' AND length(id)=36 AND substr(id,5) NOT GLOB '*[^0-9a-f]*'),
 name TEXT NOT NULL CHECK(length(CAST(name AS BLOB)) BETWEEN 1 AND 256),
 definition_revision INTEGER NOT NULL CHECK(definition_revision>=1),
 scope_revision INTEGER NOT NULL CHECK(scope_revision>=1),
 defaults_revision INTEGER NOT NULL CHECK(defaults_revision>=1)) WITHOUT ROWID;
CREATE TABLE project_index_memberships(
 project_id TEXT NOT NULL REFERENCES query_projects(id) ON DELETE CASCADE,
 index_id TEXT NOT NULL REFERENCES index_definitions(id) ON DELETE RESTRICT,
 revision INTEGER NOT NULL CHECK(revision>=1),
 PRIMARY KEY(project_id,index_id)) WITHOUT ROWID;
CREATE INDEX index_project_memberships ON project_index_memberships(index_id,project_id);
CREATE TABLE project_index_scopes(
 project_id TEXT NOT NULL, index_id TEXT NOT NULL, source_id TEXT NOT NULL,
 kind TEXT NOT NULL CHECK(kind IN('exact','subtree')),
 path TEXT NOT NULL CHECK(length(CAST(path AS BLOB))<=4096 AND (kind='subtree' OR path<>'')),
 PRIMARY KEY(project_id,index_id,source_id,kind,path),
 FOREIGN KEY(project_id,index_id) REFERENCES project_index_memberships(project_id,index_id) ON DELETE CASCADE,
 FOREIGN KEY(index_id,source_id) REFERENCES index_sources(index_id,id) ON DELETE RESTRICT) WITHOUT ROWID;
CREATE INDEX index_scope_sources ON project_index_scopes(source_id);
CREATE TABLE project_default_indexes(
 project_id TEXT NOT NULL, index_id TEXT NOT NULL,
 PRIMARY KEY(project_id,index_id),
 FOREIGN KEY(project_id,index_id) REFERENCES project_index_memberships(project_id,index_id) ON DELETE CASCADE) WITHOUT ROWID;

CREATE TABLE legacy_index_projects(
 legacy_binding TEXT PRIMARY KEY REFERENCES index_definitions(legacy_binding) ON DELETE RESTRICT,
 project_id TEXT REFERENCES query_projects(id) ON DELETE SET NULL,
 project_identity TEXT NOT NULL UNIQUE CHECK(substr(project_identity,1,4)='prj_' AND length(project_identity)=36 AND substr(project_identity,5) NOT GLOB '*[^0-9a-f]*'),
 CHECK(project_id IS NULL OR project_id=project_identity)) WITHOUT ROWID;

CREATE TABLE index_contexts(
 id TEXT PRIMARY KEY CHECK(substr(id,1,4)='ctx_' AND length(id)=36 AND substr(id,5) NOT GLOB '*[^0-9a-f]*'),
 host TEXT NOT NULL CHECK(length(host)=64 AND host NOT GLOB '*[^0-9a-f]*'),
 context_key TEXT NOT NULL CHECK(length(context_key)=64 AND context_key NOT GLOB '*[^0-9a-f]*'),
 kind TEXT NOT NULL CHECK(kind IN('folder','workspace','empty','untitled','legacy_pending')),
 name TEXT NOT NULL CHECK(length(CAST(name AS BLOB)) BETWEEN 1 AND 256),
 revision INTEGER NOT NULL CHECK(revision>=1),
 UNIQUE(host,context_key)) WITHOUT ROWID;
CREATE TABLE context_projects(
 context_id TEXT NOT NULL REFERENCES index_contexts(id) ON DELETE CASCADE,
 project_id TEXT NOT NULL REFERENCES query_projects(id) ON DELETE CASCADE,
 PRIMARY KEY(context_id,project_id)) WITHOUT ROWID;
CREATE INDEX project_contexts ON context_projects(project_id,context_id);
CREATE TABLE context_default_indexes(
 context_id TEXT NOT NULL REFERENCES index_contexts(id) ON DELETE CASCADE,
 index_id TEXT NOT NULL REFERENCES index_definitions(id) ON DELETE RESTRICT,
 PRIMARY KEY(context_id,index_id)) WITHOUT ROWID;
CREATE INDEX index_context_defaults ON context_default_indexes(index_id,context_id);

CREATE TABLE index_access_grants(
 id TEXT PRIMARY KEY CHECK(substr(id,1,4)='grt_' AND length(id)=36 AND substr(id,5) NOT GLOB '*[^0-9a-f]*'),
 host TEXT NOT NULL CHECK(length(host)=64 AND host NOT GLOB '*[^0-9a-f]*'),
 revision INTEGER NOT NULL CHECK(revision>=1),
 revoked INTEGER NOT NULL CHECK(revoked IN(0,1))) WITHOUT ROWID;
-- Project IDs are frozen history, not cascading current preferences. Effective
-- authority must also resolve the current Project and membership at delivery.
CREATE TABLE index_access_memberships(
 grant_id TEXT NOT NULL REFERENCES index_access_grants(id) ON DELETE CASCADE,
 project_id TEXT NOT NULL CHECK(substr(project_id,1,4)='prj_' AND length(project_id)=36 AND substr(project_id,5) NOT GLOB '*[^0-9a-f]*'),
 index_id TEXT NOT NULL REFERENCES index_definitions(id) ON DELETE RESTRICT,
 membership_revision INTEGER NOT NULL CHECK(membership_revision>=1),
 source_revision INTEGER NOT NULL CHECK(source_revision>=1),
 policy_epoch INTEGER NOT NULL CHECK(policy_epoch>=0),
 policy_digest TEXT CHECK(policy_digest IS NULL OR length(policy_digest)=64 AND policy_digest NOT GLOB '*[^0-9a-f]*'),
 CHECK((policy_epoch=0 AND policy_digest IS NULL) OR (policy_epoch>0 AND policy_digest IS NOT NULL)),
 PRIMARY KEY(grant_id,project_id,index_id)) WITHOUT ROWID;
CREATE INDEX index_grant_memberships ON index_access_memberships(index_id,grant_id,project_id);
CREATE TRIGGER index_grant_source_ready BEFORE INSERT ON index_access_memberships
 WHEN NOT EXISTS(SELECT 1 FROM index_definitions WHERE id=NEW.index_id AND tombstone=0 AND source_import_state='complete')
 BEGIN SELECT RAISE(ABORT,'index source metadata is unavailable'); END;
CREATE TABLE index_access_scopes(
 grant_id TEXT NOT NULL, project_id TEXT NOT NULL, index_id TEXT NOT NULL, source_id TEXT NOT NULL,
 kind TEXT NOT NULL CHECK(kind IN('exact','subtree')),
 path TEXT NOT NULL CHECK(length(CAST(path AS BLOB))<=4096 AND (kind='subtree' OR path<>'')),
 PRIMARY KEY(grant_id,project_id,index_id,source_id,kind,path),
 FOREIGN KEY(grant_id,project_id,index_id) REFERENCES index_access_memberships(grant_id,project_id,index_id) ON DELETE CASCADE,
 FOREIGN KEY(index_id,source_id) REFERENCES index_sources(index_id,id) ON DELETE RESTRICT) WITHOUT ROWID;
CREATE INDEX source_grants ON index_access_scopes(source_id,grant_id);

CREATE TABLE index_context_imports(
 import_key TEXT PRIMARY KEY CHECK(length(import_key)=64 AND import_key NOT GLOB '*[^0-9a-f]*'),
 request_digest TEXT NOT NULL CHECK(length(request_digest)=64 AND request_digest NOT GLOB '*[^0-9a-f]*'),
 catalog_revision INTEGER NOT NULL CHECK(catalog_revision>=1),
 context_id TEXT REFERENCES index_contexts(id) ON DELETE SET NULL,
 context_identity TEXT NOT NULL CHECK(substr(context_identity,1,4)='ctx_' AND length(context_identity)=36 AND substr(context_identity,5) NOT GLOB '*[^0-9a-f]*'),
 CHECK(context_id IS NULL OR context_id=context_identity)) WITHOUT ROWID;
CREATE INDEX context_import_receipts ON index_context_imports(context_identity);
CREATE TABLE index_operation_receipts(
 id TEXT PRIMARY KEY CHECK(length(id)=32 AND id NOT GLOB '*[^0-9a-f]*'),
 kind TEXT NOT NULL CHECK(length(CAST(kind AS BLOB)) BETWEEN 1 AND 32),
 stage TEXT NOT NULL CHECK(length(CAST(stage AS BLOB)) BETWEEN 1 AND 64),
 revision INTEGER NOT NULL CHECK(revision>=1),
 payload BLOB NOT NULL CHECK(length(payload)<=16384)) WITHOUT ROWID;
