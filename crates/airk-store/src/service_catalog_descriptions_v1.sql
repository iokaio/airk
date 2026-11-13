-- SPDX-License-Identifier: Apache-2.0
-- U105 description layer over any authority-1 catalog layout. Installed only by
-- the first description write, inside its catalog transaction; never by a read.
CREATE TABLE catalog_descriptions(
 kind TEXT NOT NULL CHECK(kind IN('index','project')),
 id TEXT NOT NULL CHECK(length(id)=36 AND substr(id,1,4)=CASE kind WHEN 'index' THEN 'idx_' ELSE 'prj_' END AND substr(id,5) NOT GLOB '*[^0-9a-f]*'),
 description TEXT NOT NULL CHECK(length(CAST(description AS BLOB)) BETWEEN 1 AND 1024),
 PRIMARY KEY(kind,id)) WITHOUT ROWID;
CREATE TRIGGER catalog_descriptions_capacity BEFORE INSERT ON catalog_descriptions
 WHEN (SELECT count(*) FROM catalog_descriptions)>=2048
 BEGIN SELECT RAISE(ABORT,'catalog description capacity exceeded'); END;
CREATE TRIGGER catalog_descriptions_project_delete AFTER DELETE ON query_projects
 BEGIN DELETE FROM catalog_descriptions WHERE kind='project' AND id=OLD.id; END;
CREATE TRIGGER catalog_write_catalog_descriptions_insert AFTER INSERT ON catalog_descriptions
 WHEN EXISTS(SELECT 1 FROM index_catalog_meta WHERE id=1 AND activated=1 AND accepted_new_writes=0)
 BEGIN UPDATE index_catalog_meta SET accepted_new_writes=1 WHERE id=1; END;
CREATE TRIGGER catalog_write_catalog_descriptions_update AFTER UPDATE ON catalog_descriptions
 WHEN EXISTS(SELECT 1 FROM index_catalog_meta WHERE id=1 AND activated=1 AND accepted_new_writes=0)
 BEGIN UPDATE index_catalog_meta SET accepted_new_writes=1 WHERE id=1; END;
CREATE TRIGGER catalog_write_catalog_descriptions_delete AFTER DELETE ON catalog_descriptions
 WHEN EXISTS(SELECT 1 FROM index_catalog_meta WHERE id=1 AND activated=1 AND accepted_new_writes=0)
 BEGIN UPDATE index_catalog_meta SET accepted_new_writes=1 WHERE id=1; END;
