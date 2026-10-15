CREATE OR REPLACE PACKAGE BODY archive_tools AS
  CURSOR pending_items IS SELECT item_name FROM archive_items WHERE ready = 0;
  PROCEDURE record_item(p_name IN VARCHAR2) IS
  BEGIN
    INSERT INTO archive_items(item_name) VALUES (p_name);
  END record_item;
  FUNCTION item_count RETURN NUMBER IS
    total NUMBER;
  BEGIN
    SELECT COUNT(*) INTO total FROM archive_items;
    RETURN total;
  END item_count;
END archive_tools;
/
