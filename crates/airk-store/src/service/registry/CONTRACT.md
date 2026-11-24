# Existing service registry observations and stopped-owner recovery

Owner: `airk-store::service::registry`.

Decision recorded 15 November 2026 before dependent replacement behavior.
`open_saved` remains read-only: no initialization, migration, recovery, receipt
pruning, or service ownership acquisition. A hot SQLite rollback journal may
therefore refuse inspection with `SQLITE_READONLY_ROLLBACK`.

`recover_existing_after_stop(&ServiceLease, expected_namespace)` is a separate
narrow mutation boundary. The caller proves the predecessor process/image
exclusion first and retains the actual exclusive native ServiceLease for the
exact control root. The completed native owner and expected namespace must
match before SQLite opens anything for writing. Existing database/journal
objects must be bounded, regular and unaliased. The existing SQLite owner opens
without CREATE, performs only SQLite's interrupted-transaction rollback, and
validates the existing schema/namespace. It returns no registry/write handle;
it never initializes, migrates, prunes receipts, mutates lifecycle state, or
opens source/index content. An absent journal validates through `open_saved`
without changing database bytes. Missing/foreign inputs refuse.

The caller must recapture and compare its complete read-only ownership/source
roster after recovery before removal. Recovery is not authorization to remove
unreviewed locations or to reuse an unproved pending operation. No wire shape,
digest input or persisted format changes. The owning regressions are in
`stopped_recovery_tests.rs`; installer replacement regressions additionally
exercise the process-excluded recovery ordering and refusal before moves.

Before any original SQLite recovery, Store recovers bounded private copies of
both exact DB/journal byte images and runs the same existing-schema and actual
SQL namespace validation there. A foreign database beneath an otherwise matching
owner refuses with original DB/journal bytes unchanged. Actual file IDs and
bounded bytes are rechecked against retained handles/path objects before the
original recovery under the still-held ServiceLease. Preview scratch is owned
by the private native control root and is never persisted as registry authority.

A retained canceled job vetoes automatic semantic setup for its own project. The registry checks the worker lease and project epoch before exposing that veto. Its immediate enqueue transaction checks cancellation again alongside the watch, active job and engine/settings signature gates.

## Reviewed-grant removal upgrade (change-log entry 68 part 3)

The user decided on 23 November 2026 to remove reviewed catalog grants
entirely. The build that wrote them was never deployed beyond its author's
machine, so their application records are deleted on upgrade rather than kept
readable. The terminal catalog layout has no grant tables
(`index_access_grants`, `index_access_memberships`, `index_access_scopes`),
no `legacy_index_projects`, no `index_context_imports` and no
standalone-adoption tables (`index_adopted_authorities`,
`index_adopted_configuration_claims`); fresh registries never create them.
It is identified physically, as the description layer was: the authority and
policy markers are unchanged, and every predecessor layout (authority 0 or 1,
policy 1 to 4, with or without descriptions, and the recognized adopted
layout) is recognized from its frozen DDL (`service_catalog_schema_v12.sql`
and the authority and adopted SQL) and refused by ordinary readers with the
typed `CatalogUpgradeRequired`.

`ServiceRegistry::open` (read-write) runs `catalog::grant_removal::upgrade`
after the incarnation check and before any stored record is decoded, in one
immediate transaction: it deletes the saved records only a grant-bearing build
decodes (`RECORD_REMOVALS`: fenced `create_grant`/`revoke_grant` receipts,
completed authority fences whose proof names a grant-scoped ref, and reference
refresh jobs with their cascaded rows), drops `REMOVED_TABLES` children first,
and requires the result to equal the terminal layout of the same policy level.
An unfinished fence that still waits on a grant-scoped ref refuses with
`CatalogUpgradeBlocked` and changes nothing. A second open finds the terminal
layout and does nothing. Read-only opens never upgrade. Another removal is one
more entry in either list plus its regression in `grant_removal/tests.rs`. The
schema-identity check is extended to the terminal layout, never relaxed;
`fixtures/schema/airk-state-1-terminal/` (derived by
`scripts/dev/derive_terminal_registry_schema.py`) retains the terminal
schemas.

## Bounded operation records and dead files (change-log entry 68 part 4)

The user's record cleanups of 23 November 2026 (local trust plan, decision 3)
bound the catalog's operation records. This build was never deployed beyond
its author's machine, so no backward compatibility is owed for them. No table
schema changes; only rows and dead files are deleted.

- **Receipt pruning** (`catalog::record_pruning`). `index_operation_receipts`
  admits at most 1,024 rows and previously evicted none, so the catalog refused
  every change after about 1,024 changes. The request-epoch pass
  (`request_epoch`, and the supervisor's `maintain_job_receipts`) now runs a
  catalog pass inside the job-receipt pass's immediate transaction. From 896
  rows it keeps every receipt the checkpoint's terminal classifier
  (`checkpoint::receipts::terminal`) calls unfinished, every terminal receipt a
  live reader needs, and the newest 256 other terminal receipts of the
  `catalog_management`, `catalog_management_fenced`,
  `catalog_source_completion` and `catalog_index_setup` kinds. Protected
  readers: retirement evidence (a `RetireIndex` fence while its definition is
  retained), source-change recovery (a begin until its completion is
  applied), the current source policy's fence, and the fences and final
  receipts of a retained split. When a receipt is deleted the request epoch
  rotates, as for job receipts, so its key expires explicitly
  (`catalog request key expired`) and is never replayed. 256 is far beyond any
  retry window (a retry comes from its own caller within seconds) and leaves
  room under the cap for the bounded protected owners. Readers checked:
  `runtime_contract::required_capabilities` (the pruned rows lower no
  recorded schema; `user_version` keeps it), retirement evidence,
  source-change discovery and recovery, the checkpoint capture, the source
  policy parent read and the split readers; `adopted_authority.rs` was
  removed in part 3. Test:
  `operations::tests::more_than_1024_catalog_mutations_succeed_and_unfinished_receipts_survive`.
- **Finished splits** are pruned in the same pass; see the split `CONTRACT.md`.
- **Retired definitions** (`retirement_evidence::delete_retired_definitions`).
  A tombstoned `index_definitions` row is deleted once its retirement cleanup
  is acknowledged: its complete `RetireIndex` fence is applied, no fence is
  pending, its registration is gone or kept unregistered with its cleanup guard
  cleared, and `service_removed_projects` holds its exact cleanup identity. A
  definition still named by a membership, a context default, a source policy,
  an active split participant or an enabled source stays. Its sources, aliases,
  description and the retained registration go with it; a registration that
  anything else still refers to keeps both (the foreign-key refusal rolls that
  one back). The removal history stays. Afterwards its retirement evidence
  answers none, and its fence is ordinary prunable history. Test:
  `source_change::tests::retirement_evidence::acknowledged_retirement_cleanup_deletes_the_tombstone_and_frees_its_fence`.
- **`index-catalog-upgrade.json`** no longer refuses startup. With no owner
  fence nothing admitted it and nothing reads it, so the read-write open and
  Genesis admission delete it (`genesis::retire_dead_upgrade_journal`); under a
  `catalog_upgrade` owner fence it stays and the fence refuses on its own.
  `index-catalog-genesis.json` and the schema-11 backups are kept. Test:
  `genesis::tests::competing_intents_unexplained_fences_and_missing_owned_database_refuse`.