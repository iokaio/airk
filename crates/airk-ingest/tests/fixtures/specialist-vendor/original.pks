-- Original AIRK fixture; no database connection is required.
CREATE OR REPLACE PACKAGE archive_tools AS
  PROCEDURE record_item(p_name IN VARCHAR2);
  FUNCTION item_count RETURN NUMBER;
END archive_tools;
/
