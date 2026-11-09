# S3 schema consolidation: retained fixtures and decision

Status: record, 9 November 2026, branch `feat/simplify`.

## What is retained

`fixtures/schema/airk-state-1/` holds the physical schema that the 1.8.16
baseline (`43d7005`) creates for every persisted `airk-state/1` SQLite
database. For each label there are two files:

- `<label>.objects.json`: `sqlite_master` rows (type, name, tbl_name, sql),
  sorted by type, name and tbl_name. `rootpage` is omitted, and CRLF is
  normalized to LF in `sql` only.
- `<label>.meta.json`: `user_version` and `application_id`.

`manifest.json` records the source revision, version, producer SQLite version
and how each database was created.

`scripts/dev/s3_capture_schema.py` reproduces the fixtures from a temporary
`git worktree` of the baseline, using a separate target directory. The
capture driver is written only into that worktree. `--check` recaptures and
fails on any difference.

| Database | Labels |
|---|---|
| Control registry and catalog (`service-1.4.sqlite3`) | schema 11; authority 0 (Genesis, `user_version` 12); authority 1; policies 1–3; the armed variants of authority 1 and policies 1–3 (`user_version` 13); adopted authority × policies 0–3 |
| Genesis backup (`catalog-schema11-*.sqlite3`) | `catalog-schema11-backup` |
| Collection store (`current.sqlite3`) | `current.service` (WAL, `user_version` 140); `current.standalone` (`user_version` 126) |
| Scheduler (`scheduler-1.2.sqlite3`) | `scheduler-1.2` |
| Usage (`usage.sqlite3`) | `usage.base`, `usage.supplemental` |

Excluded: transient graph overlays, the test-only vector registry, quota
staging databases, and the refused 1.1 formats.

Compare-only tests create each database with the *current* code and require
parsed equality with the fixtures. A reader test feeds every retained
registry layout, including the adopted layouts, which current code no longer
writes, to the current layout recognizer.

## Correction to the plan

The plan's Appendix D row "Genesis `user_version` 12 + layers" is incomplete.
`runtime_contract::arm_in_transaction` raises the catalog to `user_version`
13 lazily, on the first catalog setup, fence, relocation or embedded-default
write. An `airk-state/1` installation that has been used is therefore most
likely at 13. Both the armed and the unarmed layouts are retained.

## Decision: keep the layering

The S3 rule permits writing the final layout directly at Genesis only if the
result is identical to existing installations. Otherwise the layering stays.
It stays, for these reasons:

- **Order matters.** The reader requires the exact `sqlite_schema` of an
  in-memory replay of
  `LEGACY_SCHEMA → SCHEMA → capacity_triggers → write_barriers → AUTHORITY_V1
  → POLICY_V1 → SPLIT_V2 → SPLIT_V3`. `write_barriers` covers only the tables
  that exist when it runs. A direct write would have to reproduce that order
  exactly, which is the layering under another name.
- **Nothing would be deleted.** Existing installations sit at every layer
  (authority 1, policies 1–3, armed or not, adopted) and upgrade lazily, so
  every `apply_*` layer and expected layout stays. An interrupted 1.8.16
  Genesis must still resume through its stages.
- **Changing it needs D8.** The Genesis receipt stores `layout_digest` and
  `legacy_schema_digest`. A Genesis that wrote a later layout would change
  those persisted values.

The worthwhile follow-up is structural only: one ordered DDL list shared by
`expected_layout` and the `apply_*` functions, with these fixtures as its
golden outputs.

## Limits

- The fixtures prove equality after CRLF normalization, not exact bytes.
  Three `.sql` sources are checked out with CRLF line endings on Windows
  despite `.gitattributes`, so a Windows build embeds CRLF in `sqlite_master`.
- The FTS5 shadow-table DDL in `current.*` comes from SQLite itself. A
  rusqlite or SQLite upgrade can fail that comparison without a real
  incompatibility. If it does, recapture the fixtures and review them; do
  not weaken the test.
- The fixtures cover fresh 1.8.16 state, not state upgraded from earlier
  layouts. Inventory question Q5 remains open.
