-- SPDX-License-Identifier: Apache-2.0
WITH recent AS (SELECT id FROM archive.entries)
SELECT r.id AS entry_id FROM recent AS r JOIN archive.labels AS l ON r.id = l.entry_id;
CREATE VIEW archive.visible AS SELECT id FROM archive.entries;
CREATE INDEX entries_id ON archive.entries (id);
CREATE FUNCTION archive.answer() RETURNS integer LANGUAGE sql AS $$ SELECT 7 $$;
CREATE TRIGGER entries_after AFTER INSERT ON archive.entries FOR EACH ROW EXECUTE FUNCTION archive.changed();
