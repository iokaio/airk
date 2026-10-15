CREATE OR REPLACE TRIGGER audit_archive
AFTER INSERT ON archive_items
BEGIN
  audit_tools.record_change('créé');
END;
/
-- CREATE PROCEDURE not_a_definition AS BEGIN NULL; END;
