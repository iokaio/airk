CREATE OR ALTER PROCEDURE [archive].[list_items] @minimum int
AS
BEGIN
  WITH selected AS (SELECT id FROM [archive].[items] WHERE id > @minimum)
  SELECT s.id FROM selected AS s JOIN archive.labels AS l ON l.id = s.id;
END;
GO
CREATE FUNCTION archive.bump(@value int) RETURNS int
AS BEGIN RETURN @value + 1; END;
GO
CREATE TRIGGER archive.changed ON archive.items AFTER INSERT
AS BEGIN INSERT INTO archive.audit(id) SELECT id FROM inserted; END;
GO
