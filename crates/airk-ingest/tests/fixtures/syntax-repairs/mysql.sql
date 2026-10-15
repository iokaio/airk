-- SPDX-License-Identifier: Apache-2.0
SELECT e.id AS entry_id FROM `entries` AS e LEFT JOIN `labels` AS l ON e.id = l.entry_id LIMIT 9;
CREATE VIEW `visible` AS SELECT id FROM `entries`;
CREATE INDEX entries_id ON `entries` (id);
