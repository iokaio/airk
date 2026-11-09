# AIRK compatibility inventory (S1.1 / R1)

**Draft for S1 review, 9 November 2026; proposal only — authorizes no deletion.**

This is the compatibility inventory required by [simplify-plan.md](../simplify-plan.md)
§5.1 R1, §5.2 S1.1 and Appendix D. It records which predecessor layouts current
code recognizes, how each clean-break duty is implemented today, and the exact
`airk-state/1` schema that 1.8.16 creates. The governing policy is
[upgrade-rebuild-fallback.md](../upgrade-rebuild-fallback.md) (forced clean break
at the 1.8.16 baseline).

Method and limits:

- Read-only source review on branch `feat/simplify` at `43d7005`. The AIRK MCP
  server was not used; evidence comes from `rg` and direct reads. No build or
  test ran for this document. Test names are cited as **proof candidates**, not
  as passing evidence.
- **[V]** means verified by reading the cited code. **[I]** means inferred from
  code, comments or the dated release record and still needs confirmation.
  Version ranges for registry schemas are mostly **[I]**: the code identifies
  schema numbers, not releases.
- Paths are relative to the repository root. Line numbers refer to `43d7005`.
- **9 November 2026, after S4:** module honesty moved most `#[path]`-mounted
  files to their canonical module locations (for example
  `runtime_layout.rs` became `runtime_install/layout.rs`). Paths below still name
  the `43d7005` files; `git log --follow` or
  `python scripts/dev/s4_module_honesty.py` maps them.

## 1. How the boundary is decided

Recognition happens along two independent dimensions. An installation is one
point in each:

- **Registry dimension.** The `user_version` and physical shape of
  `<control>/service-1.4.sqlite3` (rows P3–P8).
- **Packaging dimension.** The runtime payload shape and version under
  `<install>/runtimes/<sha>` (rows P9–P12).

The boundary decision is native and shared. [V]

- `crates/airk-cli/src/runtime_state_contract.rs:69` `required()` returns true
  (reset required) when the approved predecessor version is below 1.8.16, when
  `airk-state-contract.json` is absent (`:103`), or when the payload is not a
  single executable (`runtime_pair::single_executable`,
  `crates/airk-cli/src/runtime_pair.rs:209`). `candidate()` (`:107`) refuses a
  pre-baseline candidate or a downgrade.
- The extension mirrors this only for presentation
  (`packages/vscode/src/runtimeRebuild.ts:23` `runtimeCleanBreakRequired`). A
  missing native policy is treated as "reset required". It never authorizes
  reuse of legacy state.
- The clean-break orchestrator is `crates/airk-cli/src/runtime_rebuild.rs`
  (`activate` at `:444`, `advance` at `:716`; journal `rebuild.json`). It runs
  ordinary Genesis for the successor (`:846`).

### 1.1 Shared clean-break duties (referenced by code below)

| Code | Duty | Implementation [V] | Proof candidates (tests) |
|---|---|---|---|
| D1 | Boundary decision | `runtime_state_contract.rs:69,103,107`; marker `airk-state-contract.json` (`:8`, read `:44`) | `runtime_state_contract_managed_tests.rs` (`clean_break_managed_old_versions_and_missing_or_future_contract_stay_fenced`, `…_crossed_or_previous_public_selection`); TS `runtimeRebuild.test.ts` ("clean break presentation follows the baseline and native state contract…") |
| D2 | Saved-registry read without migration | `ServiceRegistry::open_saved` accepts `user_version` 2..=13 read-only (`crates/airk-store/src/service_registry.rs:521`); 12/13 require a validated activated catalog (`:535-541`); `catalog_state()` is `None` below 12 (`service_catalog.rs:601`) | `service_predecessor_schema8_tests.rs::authentic_managed_schema8_migrates_before_catalog_admission` (opens with migration, not `open_saved`) |
| D3 | Predecessor image inventory | `runtime_retirement.rs:17,20` → `runtime_superseded.rs`; `runtime_legacy_launchers.rs:103` `inspect_clean_break_locked`; `runtime_upgrade_stop.rs:87-178` (≤20 images, stable + universal launcher) | `runtime_upgrade_stop_roster_tests.rs` (4 tests); `runtime_legacy_launchers_tests.rs::absent_historical_image_needs_exact_exclusion_or_complete_observation` |
| C1 | Source selection and preference capture | `runtime_rebuild_settings.rs:305` `preflight_clean_break`, `:359` `capture_clean_break`; `ServiceRegistry::catalog_clean_break_sources` (`service_catalog_binding_lookup.rs:15`); `Collection::capture_clean_break_preferences` (`service_collection_settings_capture.rs:69`); `data_scope::has_saved_binding` (`service_data_scope.rs:235`). Pause and watch come from the registry; vector opt-out and detail come from `vector-policy.json`/`extraction-policy.json`; roots come from the catalog or `roots.json` | `runtime_rebuild_settings_tests.rs` (`clean_break_capture_enumerates_two_catalog_indexes_and_all_raw_roots_without_old_storage`, `clean_break_preferences_keep_supported_choices_and_discard_old_engine_settings`); `runtime_rebuild_continuation_tests.rs` (3) |
| C2 | Client registrations and predecessor command images | `runtime_rebuild.rs:507-522` (registrations), `:631` `capture_client_metadata` | `runtime_rebuild_tests.rs::clean_break_explicit_launcher_inspection_qualifies_old_pair_without_relaxing_normal_gate` |
| C3 | Predecessor HTTP ownership snapshot | `runtime_http_cleanup.rs:12` `capture` (called at `runtime_rebuild.rs:779` after exit proof); extension `serviceHttp.ts:195-229` | `runtime_rebuild_tests.rs::clean_break_native_http_lookup_*`, `clean_break_http_snapshot_survives_headless_old_control_and_payload_retirement` |
| C4 | Host client declarations and uninstall receipts (separate CAS plan) | `packages/vscode/src/serviceExtension.ts:1917` `captureCleanBreakClients`, `:1731`, `:1959`; `agentMigration.ts:207-353`; `extensionUninstall.ts:74,97` | `agentMigration.test.ts` ("clean break removes exact disabled client declarations…", "…preserves unproven AIRK declarations…"); `extensionUninstall.test.ts` |
| S1 | Hard stop and exit proof | `runtime_upgrade_stop.rs:11` `stop`, `:33` `stop_all_predecessors`, `:67` `preflight_all_predecessors`, `:218` `stop_paths` (5 s budget); process audit `crates/airk-cli/src/legacy_processes.rs`; macOS `runtime_upgrade_stop_macos.rs` | `runtime_upgrade_stop.rs::old_unresponsive_process_is_stopped_by_verified_handle_only`; `runtime_legacy_launchers_tests.rs::old_owned_launcher_process_is_stopped_while_foreign_image_is_preserved` |
| S2 | Writer exclusion and Stop-sequence check | `runtime_rebuild.rs:407` `OldExclusion` (service, project, embedding and watch leases); `:707` `predecessor_stop_allows_reset`; `:761-777` | `runtime_rebuild_tests.rs` (rest of file; not enumerated here) |
| R1 | Successor layout handoff (old state preserved) | `runtime_layout_successor.rs:200` `preflight_clean_break` (16-path union), `:349` `admit_successor`, `:132` | `runtime_layout_successor_tests.rs` (8 tests) |
| R2 | Resumable cleanup journal | `runtime_clean_break_cleanup.rs:488` `cleanup`; phases `Executables→Collections→DataRoots→Control→ControlFinal→State` (`:28`); journal `clean-break-cleanup-<sha256(old_control)>.json` (`:70`) | `runtime_clean_break_cleanup_tests.rs` (12 tests) |
| R3 | Storage disposal and quota census | `crates/airk-store/src/service_data_scope_clean_break.rs`; `quota/clean_break.rs`; `quota/funding_retired.rs` (via `service_data_scope_clean_break.rs:4,335`) | `service_data_scope_clean_break_tests.rs` (8), `…_replay_tests.rs` (10), `service_collection_clean_break_purge_tests.rs` (6) |
| R4 | Executable and launcher retirement | `runtime_legacy_launchers.rs` `retire_locked_recording` (called at `runtime_clean_break_cleanup.rs:509`); `runtime_retirement.rs:404,408` | `runtime_legacy_launchers_tests.rs::clean_break_launcher_proof_handoff_survives_death_before_intent_removal`, `durable_roster_cannot_be_extended_or_crossed_after_partial_deletion` |
| R5 | Resume and background continuation | `runtime_rebuild.rs` phases `Accepted→Switched→Genesis→Setup→Starting→Complete` (`:27`); `runtime_clean_break_background.rs` | `runtime_rebuild_tests.rs::rebuild_resume_each_setup_boundary_reuses_exact_native_receipts_and_index_job`; `runtime_clean_break_background_tests.rs` (2) |

Receipts: native `rebuild.json` and `clean-break-cleanup-*.json` under the
installation. The extension journal comes from `privateIntentStore.ts`. Dated
qualification: one authentic 1.8.11→1.8.16 two-index journey passed (all seven
held predecessor identities exited, cleanup completed;
`docs/releases/release-1.8.16.md:211`, r37). The R39 installed-editor gates
failed. That run is historical evidence, not acceptance.

## 2. Predecessor inventory (Appendix D)

"Surviving owner after S2" is a **proposal — needs review** in every row.

| # | Layout or version range | Discovery (code; proof) | Captured selections and preferences (code; tests) | Shutdown and exit proof (code; receipts) | Resumable cleanup and retirement (code; receipts) | Refusal cases | Code currently serving it | Surviving owner after S2 (proposal — needs review) | Status |
|---|---|---|---|---|---|---|---|---|---|
| P1 | ≤1.3 standalone CLI: OS startup task plus per-collection derived files [I: "1.3" from `current/legacy_retirement.rs:2`] | Explicit only. The operator supplies `--root/--data-dir/--collection/--executable` (`crates/airk-cli/src/legacy_startup.rs:15-80`; `airk service legacy`, `main.rs:707,2023`). No disk scan, per contract. Tests: `legacy_startup_tests.rs::review_is_read_only…`; `tests/legacy_native.rs::legacy_review_never_executes_the_selected_old_binary_or_creates_project_state` | None. Sources and settings are preserved, not captured (`Preview.settings_preserved`) | `legacy_startup.rs` OS review; worker audit via `legacy_processes::audit` (`legacy_startup.rs:227`) | `legacy_startup_journal.rs` (≤16 receipts); `airk-store/src/current/legacy_retirement.rs` (bounded turns); test `tests/legacy_native.rs::explicit_old_derived_cleanup_uses_actual_cli_and_keeps_originals_and_settings` | Foreign or unavailable OS proof; changed receipts; unknown running worker (`legacy_startup_tests.rs`) | `legacy_startup.rs`, `legacy_startup_journal.rs`, `store/current/legacy_retirement.rs`; extension `serviceBootstrap.ts:813-840`, console actions | Same modules; outside the automatic clean break. The user must decide whether explicit 1.3 cleanup remains supported | Live, explicit-only; not part of forced reset |
| P2 | Standalone index adoption: AIRK inventory 2/3 through v1.1.4, generation-4/5 saved catalogs, extension-managed standalone caches, original stdio declarations [V module headers] | Native origin capture from client configs and caches: `runtime_legacy_origin.rs`, `runtime_legacy_cache_retirement.rs` (`inspect/prepare-legacy-caches`); extension `serviceBootstrap.ts:481-547` | Superseded by policy: the clean break does not import old evidence. Adoption code still copies saved evidence (`store/legacy_adoption*.rs`) | `runtime_legacy_maintenance.rs` (maintenance fence) | `runtime_legacy_cache_retirement.rs`, `…_reprepare.rs`; store `service_catalog_legacy_adoption.rs` | Many (adoption-specific) | CLI `runtime_legacy_{origin,authority,index_adoption,maintenance,cache_retirement,cache_reprepare}`, `catalog_legacy_adoption`, `runtime_http_legacy`; store `legacy_adoption*`, `service_catalog_legacy_adoption`, `service_catalog_adopted_*`; kernel `legacy_adoption`; core `catalog_legacy_*`, `legacy_adoption`; extension `serviceBootstrap` legacy-cache/adoption, `agentConfig.ts:146-186` | None for adoption (contract §"What the reset preserves"). Exception: the schema-13 adopted **layout recognizer** may have to stay readable (see §3.4 and Q2) | Superseded but still compiled **and wired**. Native commands are not gated by D1 (Q1) |
| P3 | Managed service, registry `user_version 1` [I: earliest 1.4.x] | D1 fires; then `open_saved` refuses versions below 2 (`service_registry.rs:521`) [V] | Not captured (refusal) [I] | — | — | `unsupported saved service registry format` before capture [I: path traced, not tested] | D2 | D1 + D2 | Refused; untested (Q4) |
| P4 | Raw registry `user_version 2..7` [I: 1.4.x–1.5.x; `:847` notes 1.5.3 created traversal without a version bump] | D1, D2 (`registration_state` false below 5, `watch_traversal_state` false below 7: `service_registry.rs:548-549`) | C1 raw branch: registrations, pause, watch, `roots.json`, vector/detail envelopes; preferences missing ⇒ semantic disabled (`runtime_rebuild_settings.rs:482-486`). C2, C3, C4 | S1, S2 | R1–R5 | >64 indexes; >128 KiB; include/exclude policy; aliases needing translation; storage without ownership receipt (`:437`); missing data root with no catalog roots (`:456-459`); retained root unsupported; new User Stop (`runtime_rebuild.rs:776`) | D1–R5 plus old-schema decode in `ServiceRegistry` read methods | D1–R5 owners (live) | Live duty; no per-version fixture (Q5) |
| P5 | Raw registry `user_version 8` (authentic managed 1.5.12 DDL) | As P4 | As P4 | S1, S2 | R1–R5 | As P4 | As P4 | As P4 | Live duty. Inline DDL fixture `service_predecessor_schema8_tests.rs` (source rev `d3b80dd…`) |
| P6 | Raw registry `user_version 9..11` [I: 1.6 semantic transition `:894`, 1.6.2 removal `service_project_removal.rs:17`, background audit at 11] | As P4 (`background::audit::validate_existing`, `:544`) | As P4 | S1, S2 | R1–R5 | As P4 | As P4 | As P4 | Live duty; no fixture |
| P7 | Activated catalog registry `user_version 12/13`, any authority/policy layout (pre-1.8.16 catalog installs) [I] | D1, D2 (`catalog::validate_existing` + `activated`, `service_registry.rs:535-541`) | C1 catalog branch: `catalog_clean_break_sources`; owner root must equal registration; one alias per additional root (`runtime_rebuild_settings.rs:305-356,390-430`) | S1, S2 | R1–R5 | As P4, plus catalog/registration census mismatch (`:350-353,520-523`) and catalog revision change (`:526-533`) | As P4 plus `service_catalog.rs` layout recognizer (including `adopted_layout::recognize`) | As P4 | Live duty. Authentic 1.8.11 journey (release record) |
| P8 | Catalog with **pending/unsettled** P5 upgrade (`index-catalog-upgrade.json`, `activated=0`), e.g. the retained 1.8.15 `staged_verified` journal [I] | D1; `open_saved` returns `AdmissionPending` (`service_registry.rs:536-539`) | Not captured; C1 aborts [I] | — | P5 recovery exists: `service_catalog_upgrade_recovery.rs`, `…_retirement_terminal.rs`, CLI `runtime_legacy_retirement.rs` | `AdmissionPending` refusal [I: traced, no test found] | Store `service_catalog_upgrade*`, `_compatibility`, `_v1_settlement`; kernel `service_catalog_upgrade.rs`; core `catalog_upgrade_recovery.rs`; CLI `runtime_catalog_activation.rs:249-280`; extension `runtimeLegacyRetirement.ts` | **Open (Q3)**: either keep the P5 recovery owner or define an explicit refusal plus manual recovery | Unresolved; blocks deletion of the P5 recovery code |
| P9 | Packaging schema 1 "pair": `airk` + `airk-embed` helper, stable `launcher.json`/launcher, universal launcher, stdio MCP; versions <1.8.14 (e.g. 1.8.11) | D1 (pair ⇒ reset, `runtime_state_contract.rs:84-87`); helper via `runtime_pair::payload_root` (`runtime_upgrade_stop.rs:183-185`); D3 | C1–C4 for its registry row; stdio declarations via C4 | S1 including helper and both launchers; S2 | R1–R5; R4 retires launchers and runtimes | Modified or unknown launcher (`runtime_upgrade_stop.rs:153`); image roster >20; candidate crossing | D1–R5, `runtime_legacy_launchers.rs`, `runtime_pair.rs:209-250` | D1–R5 owners; `runtime_legacy_launchers` **live** | Live duty. Authentic 1.8.11→1.8.16 journey passed (r37, `release-1.8.16.md:211`) |
| P10 | Single-image pre-baseline 1.8.14–1.8.15 (packaging schema 2, HTTPS) [I: `scripts/build-service-runtime.py:318`] | D1 (version below 1.8.16) | C1–C4; C3 HTTP snapshot (128 IDs / 64 KiB) | S1, S2 | R1–R5; extension CAS for HTTP declarations | As P4; foreign or unavailable HTTP ownership blocks disposal | D1–R5, `runtime_http_cleanup.rs`, extension `serviceHttp.ts` | D1–R5 owners | Live duty; R39 installed gate failed (stop record) |
| P11 | Historically staged, never-started payload lacking optional process-role aliases | D3 `absent_optional_process_aliases` (`runtime_retirement.rs:20`) | — | `preflight_all_predecessors` requires a complete zero-match audit (`runtime_upgrade_stop.rs:51-75`) | — | `clean_break_predecessor_process_observation_unsupported` before admission | `runtime_upgrade_stop.rs`, `runtime_superseded.rs`, `legacy_processes.rs` | Same (live) | Explicit refusal (documented limit) |
| P12 | ≥1.8.16 binary over old state: missing, foreign or crossed `airk-state-contract.json`, or unknown contract | D1: a missing marker ⇒ reset (`:103`); a foreign marker ⇒ `unsupported_state_contract` / `state_contract_identity_changed` (`:51-63`) | As P4–P7 if the reset proceeds | As P4 | As P4 | Crossed install, control or namespace; future contract | `runtime_state_contract.rs` | Same (live) | Live duty; tests in D1 |
| P13 | Historical default index-root location (`UserPaths::historical`, `service-1.4` data dir) | `runtime_data_migration.rs:29` `needed` (ordinary activation, `runtime_activation.rs:643-660`) | n/a (rename, no capture) | Runs after owned processes drain | `service_data_relocation.rs` (`index-root-migration.json`, `index-root-layout.json`) | Custom locations are not inferred (`runtime_data_migration.rs:56`) | CLI `runtime_data_migration.rs`; store `service_data_relocation.rs` | Relocation: open (Q6). `data_relocation::require_settled` is called by Genesis (`service_catalog_genesis.rs:200`) and must survive | Mixed |

## 3. Current layout: 1.8.16 `airk-state/1`

### 3.1 Creation sequence [V]

1. **Registry steps 1→11 run on every fresh installation.**
   `ServiceRegistry::open` (`service_registry.rs:574`) creates `service_meta` and
   `projects` at `user_version 1` (`:737-750`). It then runs every step
   `<2 … <7`, recovery/setup tables, `<9`, `project_removal::migrate` (→10) and
   `background::audit::migrate` (→11) (`:762-915`). The result must equal the
   compiled reference `service_registry_schema11.sql` exactly
   (`service_catalog.rs:204-224,440-446`).
2. **Genesis** (`service_catalog_genesis.rs`) verifies the empty schema-11
   source (`:278-347`), takes a backup, installs `service_catalog_schema.sql`
   plus capacity triggers and write barriers, inserts `index_catalog_meta`, and
   sets `user_version 12` in one transaction (`:684-704`). It does all of this
   under `index-catalog-genesis.json`.
3. **Authority 1**: `promote_authority_v1` applies
   `service_catalog_authority_v1.sql` and sets `authority_contract=1`
   (`service_catalog_activation.rs:502-549`). Readiness and activation then
   settle Genesis (`GenesisStage::Settled`).
4. The **state marker** is written after settlement
   (`runtime_catalog_authority.rs:96,143` → `runtime_state_contract::initialize`).
5. **Later layers are lazy.** They are not part of Genesis:
   - policy 1 on the first source-policy change (`service_catalog_policy.rs:300`);
   - split v2 or extraction v3 on the first split (`service_catalog_split.rs:531-533`);
   - `user_version 13` (`IN_PROCESS_SCHEMA`, no DDL) armed by
     `runtime_contract::arm_in_transaction` on an in-process provider fence
     (`service_catalog_fences.rs:404`), an embedded default model write
     (`service_runtime_contract.rs:203`), data relocation (`:655`) or legacy
     adoption (`service_catalog_setup.rs:464`, `adopted_layout.rs:96`).

### 3.2 Layers (DDL in source) [V counts from `CREATE` statements]

| Layer | Source | Tables | Indexes | Triggers | Notes |
|---|---|---|---|---|---|
| Registry 11 | `service_registry_schema11.sql` (reference) and step code | 31 (`service_*`, `projects`) | 5 | 9 | Built by steps 1→11. The SQL file is only the identity reference |
| Catalog 12 | `service_catalog_schema.sql` | 17 (`index_catalog_meta`, `index_definitions`, `index_sources`, `index_source_aliases`, `query_projects`, `project_index_memberships`, `project_index_scopes`, `project_default_indexes`, **`legacy_index_projects`**, `index_contexts`, `context_projects`, `context_default_indexes`, `index_access_grants`, `index_access_memberships`, `index_access_scopes`, `index_context_imports`, `index_operation_receipts`) | 9 | 3 | Plus generated triggers (`service_catalog.rs:135-195`): per-table capacity (6 tables at 1024 rows), 8 group caps, 2 grant-selection caps, and `catalog_write_<table>_{insert,update,delete}` for every table present at install time except `index_catalog_meta`, **including the 31 registry tables** |
| Authority 1 | `service_catalog_authority_v1.sql` | 0 | 2 | 3 | Adds 5 `index_access_grants` columns (`provenance_kind`, `import_*`). Drops `index_access_grants_capacity` |
| Policy 1 | `service_catalog_policy_v1.sql` | 1 (`index_source_policies`) | 0 | 2 | `policy_contract=1` |
| Split 2 | `service_catalog_split_v2.sql` | 5 (`index_split_*`) | 0 | 5 | `policy_contract=2` |
| Extraction 3 | `service_catalog_split_extraction_v3.sql` | 1 (`index_split_extraction_policies`) | 0 | 1 | `policy_contract=3` |
| Adopted authority | `service_catalog_adopted_authority.sql` + `adopted_layout.rs:11-45` | rebuilds 3 grant tables; adds `index_adopted_authorities`, `index_adopted_configuration_claims` | 5 | 10 (+ capacity and write barriers) | Requires `user_version 13` (`service_catalog.rs:317-326`). Reached only through P2 adoption [I] |

The registry is `application_id = APPLICATION`, `page_size 4096`, bounded by
`MAX_DATABASE_BYTES`, `journal_mode=DELETE` (`service_registry.rs:731-736`,
`service_catalog.rs:507-524`). `index_catalog_meta.legacy_schema_digest` must
equal the digest of the compiled schema-11 objects (`service_catalog.rs:480-487`),
so the schema-11 reference DDL is part of every current catalog's identity.

### 3.3 Reachable `airk-state/1` registry states

The `CatalogLayout` and marker pairs accepted by `validate_catalog_layout`
(`service_catalog.rs:314-349`) are listed below.

| `user_version` | `CatalogLayout` | authority / policy | Reachable on 1.8.16 |
|---|---|---|---|
| 11 | `Schema11` | — | Transient: fresh registry before Genesis migration [V] |
| 12 | `Schema12Authority0` | 0 / 0 | Transient: Genesis `Migrated`/`Promoting` [V] |
| 12 or 13 | `Schema12Authority1` | 1 / 0 | Settled fresh install [V]; 13 after the first in-process fence or embedded model write [I: likely common] |
| 12 or 13 | `Schema12Authority1Policy1` / `Policy2` / `Policy3` | 1 / 1, 2, 3 | After policy or split operations [V] |
| 13 | adopted variants of the four authority-1 layouts | 1 / 0–3 | Only after P2 adoption [I] (Q2) |

**Consequence for S3** [V]: `Schema11` and `Schema12Authority0` are on the fresh
Genesis path, and all authority-1 variants are valid current states. Appendix A's
"registry steps 1–12 and non-final `CatalogLayout` variants" are therefore **not
predecessor-only code**. Registry steps 1→11 create every new installation.
Collapsing them changes how the current layout is produced, which needs the
schema-identity proof and D8.

### 3.4 Readers [V]

- `ServiceRegistry::open` (`service_registry.rs:574`) and `open_saved`/`open_existing` (`:457-560`).
- `catalog::validate_existing`, `validate_catalog_layout`, `state` (`service_catalog.rs:314-524`).
- `runtime_contract::required_capabilities` (`service_runtime_contract.rs:64`).
- `genesis::pending` (`service_catalog_genesis.rs:143`).
- `runtime_state_contract` `read` (`:44`).

Other persisted `airk-state/1` files that are not covered by any schema fixture:

- `runtime-layout.json` and `selected.json` (install root);
- `airk-state-1.4.owner.json`, `airk-control-1.4.owner.json` and `service-1.4.owner.json`;
- `airk-state-contract.json`;
- `index-catalog-genesis.json`, including `AuthorityV1ActivationRecord.predecessor_refusals: Vec<upgrade::RuntimeRefusal>` (`service_catalog_activation.rs:35,53`);
- Genesis backups;
- `rebuild.json`;
- per-index storage (`collections-1.4`, `.quota-1.4`, Current journal; `ArtifactRef.legacy_origin` is optional and skipped when `None`, `airk-core/src/lib.rs:105`).

### 3.5 Retained schema fixtures: **none** [V]

- No `*.sqlite`/`*.db` file is tracked (`git ls-files`).
- `fixtures/wire/`, cited by the plan, does not exist.
- Existing shape checks compare against layouts rebuilt in memory from the same
  compiled DDL (`expected_layout`, `service_catalog_layout_tests.rs`). This is
  self-referential: it cannot detect a change to the DDL itself.
- The only independent historical DDL is the inline 1.5.12 schema-8 fixture.

Before S3, retain authentic byte or DDL fixtures produced by a real 1.8.16 build.
Cover each reachable row in §3.3, plus the JSON receipts listed in §3.4.

## 4. Candidate S2 modules (Appendix A, "compatibility proof required")

Classification: **LIVE** = implements a clean-break duty or the current layout;
**MIXED** = contains LIVE items; **SUPERSEDED** = serves only P2 or P8 adoption
and migration; **NOT-PRED** = not predecessor handling despite its name.

| Module | Rows served | Class | Evidence and notes |
|---|---|---|---|
| store `service_catalog_upgrade.rs` | P8; Genesis | **MIXED** | Genesis calls `validate_registry_files` (`:514`), `recover_owned_registry` (`:619`), `genesis_worker_leases` (`:689`) and `PRODUCTION_ACTIVATION_ENABLED` (`:428`) (`service_catalog_genesis.rs:251,401,483`; activation `:445,587`). `CatalogUpgrade` (`:729+`) is the P5 migration |
| store `service_catalog_settlement.rs` | Current; P8 | **MIXED** | Defines `RuntimeRefusal`/`ReadinessEvidence`, which are persisted in the Genesis receipt (`service_catalog_activation.rs:35,53`). Deleting it changes the `airk-state/1` format |
| store `service_catalog_exclusion.rs` | Genesis; P8 | **MIXED** | `exclusion::worker_leases` backs `genesis_worker_leases` (`service_catalog_upgrade.rs:690`) |
| store `service_catalog_compatibility.rs`, `_v1_settlement.rs` | P8, P2 | SUPERSEDED [I] | Reached through `CatalogAuthorityProbe --native-host` (`runtime_install_commands.rs:856-861`) and P5 recovery. The fresh path uses `genesis::activation` |
| store `service_catalog_upgrade_recovery.rs`, `_retirement_terminal.rs` | P8 | SUPERSEDED, **pending Q3** | Only owner of pending-P5 recovery |
| store `service_catalog_import.rs` | P7/P8 migration | SUPERSEDED | Mounted only in `service_catalog_upgrade.rs:57` |
| store `service_catalog_adopted_authority.rs`, `_adopted_layout.rs` | P2; layout reader | **MIXED** | `adopted_layout::recognize` is in the read path (`service_catalog.rs:305`). Removing it makes adopted schema-13 states unreadable (Q2) |
| store `legacy_adoption*.rs`, `service_catalog_legacy_adoption.rs` | P2 | SUPERSEDED | Adoption-only decoders. Wired through CLI adoption commands |
| store `maintenance_legacy.rs` | none found | likely generation-era | Mounted in `maintenance.rs:21`. Settle with the R2 compile evidence |
| store `legacy_saved.rs` | P2 data in Current | **unresolved** | Used by kernel `current/{runtime,update,metadata}.rs` and store `current/journal.rs`. It may be part of the persisted Current format |
| store registry steps 1–12 (`service_registry.rs:737-915`) | **Current creation**; P4–P6 reads | **LIVE** | Fresh installations run them (§3.1) |
| store non-final `CatalogLayout` variants | Current (Genesis transient) | **LIVE** | §3.3 |
| store `current/legacy_retirement.rs` | P1 | LIVE (explicit) | Used by `legacy_startup.rs:484-504` |
| store `quota/clean_break.rs`, `quota/funding_retired.rs` | P4–P10 cleanup | **LIVE** (R3) | `service_data_scope_clean_break.rs:4,335` |
| store `quota/retired.rs` | current project removal | **NOT-PRED** | `service_project_removal.rs:154` (`AbsentCollectionSettlement`) |
| CLI `legacy_processes.rs` | P1, P9–P11 | **LIVE** (S1) | `runtime_upgrade_stop.rs:53,71,226` |
| CLI `runtime_legacy_launchers.rs` | P9 | **LIVE** (D3, R4) | `runtime_rebuild.rs:641`, `runtime_clean_break_cleanup.rs:461,509`, `runtime_staging.rs:184`, `runtime_superseded.rs:368` |
| CLI `legacy_startup.rs`, `_journal.rs` | P1 | LIVE (explicit) | Also used by `runtime_legacy_index_adoption.rs:270-288` (P2) |
| CLI `runtime_legacy_{origin,authority,index_adoption,maintenance,cache_retirement,cache_reprepare}.rs`, `catalog_legacy_adoption.rs` | P2 | SUPERSEDED, **still wired** | `runtime_install_commands.rs:592-663`; extension `serviceBootstrap.ts:481-547` |
| CLI `runtime_http_legacy.rs` | P2/P7 imported grants | SUPERSEDED [I] | Still in the live HTTP backend (`http_backend.rs:80,87,291`). Whether imported grants can exist in `airk-state/1` depends on Q1 |
| CLI `runtime_legacy_retirement.rs` | P8 | SUPERSEDED, pending Q3 | `CatalogUpgrade` recovery commands (`runtime_install_commands.rs:125-138`) |
| CLI `runtime_data_migration.rs` | P13 | open (Q6) | Ordinary activation only |
| CLI `embed_pool_legacy_{context,entry}.rs` | Producer of the P9 helper | NOT a consumer duty | Compiled only into `airk-embed` (`embedding-worker` feature). That binary is not built for ≥1.8.14 (`scripts/build-service-runtime.py:318,461`). `include!` mounts (`embed_pool.rs:4-6`) break module honesty |
| CLI `embed_pool_retirement.rs` | current runtime | **NOT-PRED (live)** | `include!`d into the single-image library by `inference_engine.rs:16-18` (`lib.rs:27`) as well as `airk-embed`. Current pool-membership retirement, not predecessor code [V] |
| CLI `worker_read_settlement.rs` | none | **NOT-PRED** | Current read-channel retirement (`project_supervisor.rs:36`). Not in Appendix A; listed only to prevent name-based deletion |
| kernel `legacy_adoption.rs` | P2 | SUPERSEDED | |
| kernel `service_catalog_upgrade.rs` | P8 | SUPERSEDED | `runtime_catalog_activation.rs:272`, `catalog_test_fixture.rs:12` |
| kernel `historical.rs` | none | **NOT-PRED** | Removed-API boundary. `HistoryAvailability` is referenced only by kernel tests [V by `rg`] |
| core `catalog_legacy_adoption.rs`, `catalog_legacy_authority.rs`, `legacy_adoption.rs` | P2 | SUPERSEDED / format question | `ArtifactRef.legacy_origin` is a persisted optional field (Q2) |
| core `catalog_upgrade_recovery.rs` | P8 | SUPERSEDED, pending Q3 | |
| ingest `adapters/legacy.rs` | none | **NOT-PRED** | Current extraction path: `extraction_jobs.rs:87`, `composite.rs:3`, `continuity/*`, `specialist_gpu.rs:251`, `lib.rs:1024` |
| extension `runtimeRebuild.ts`, `agentMigration.ts` clean-break functions, `extensionUninstall.ts` clean-break receipts, `serviceHttp.ts` predecessor proof, `serviceExtension.ts:1731-1980` | P4–P12 | **LIVE** (C3, C4, D1) | Keep clean-break capture (plan Appendix A) |
| extension `serviceBootstrap.ts` legacy-cache/adoption, `agentConfig.ts` legacy JSON review, `indexingCatalog.ts:131` | P2 | SUPERSEDED | Adoption is gated by `clean_break_does_not_migrate_legacy_grants` only when a reset is required (`serviceBootstrap.ts:716,727`) |
| extension legacy-startup console actions | P1 | LIVE (explicit) | `serviceConsoleController.ts:915,1092` |
| extension `runtimeLegacyRetirement.ts` | P8 | SUPERSEDED, pending Q3 | |

**Live clean-break duties that carry "legacy" names and must not be deleted on
name evidence:**

- `legacy_processes.rs` (exit proof);
- `runtime_legacy_launchers.rs` (image discovery and retirement);
- `legacy_startup*` and `store/current/legacy_retirement.rs` (explicit P1 cleanup);
- `quota/clean_break.rs` and `quota/funding_retired.rs` (storage census);
- registry steps 1–11 and schema-11 reference DDL, `CatalogLayout::Schema11`/`Schema12Authority0`, and the `legacy_index_projects` table (current layout);
- Genesis helpers inside `service_catalog_upgrade.rs`, `service_catalog_settlement.rs` and `service_catalog_exclusion.rs`.

## 5. Open questions and uncertainty

- **Q1. Is P2 adoption reachable on `airk-state/1`?** Native adoption, cache and
  `http-legacy` commands do not check D1/`state_contract`. The extension blocks
  them only while a reset is required. If a 1.8.16 installation can still adopt,
  adopted schema-13 layouts and imported grants are current states, and the P2
  code is not deletable without a user decision to disable it. Needs a product
  decision plus a caller audit of `serviceExtension.ts:1237-1260`.
- **Q2. Adopted-authority layouts and `ArtifactRef.legacy_origin`.** Depends on
  Q1. If any `airk-state/1` installation can hold them, the readers must stay
  until a new state contract (D8) is approved.
- **Q3. Pending P5 catalog upgrade (P8).** `open_saved` refuses
  `AdmissionPending`, so the clean break cannot capture such a predecessor [I].
  Decide whether to keep `service_catalog_upgrade_recovery` as the settling
  owner or to refuse with manual guidance. No test covers clean break over a
  pending P5 upgrade.
- **Q4. Registry `user_version 1` (P3)** is refused by `open_saved` [I]. No
  test shows the user-facing refusal or that the original state is preserved.
- **Q5. Per-version capture coverage.** Except for schema 8, no authentic
  fixture exists for registry 2–7, 9–11 or for pre-1.8.16 catalog layouts.
  Capture tests build state with current code.
- **Q6. P13 relocation.** Can an `airk-state/1` installation still need
  `data_migration` in ordinary activation, or is it reachable only through
  predecessors? `require_settled` stays a Genesis dependency either way.
- **Q7. Ordinary activation still calls `catalog_upgrade::begin_if_enabled` /
  `resume_if_enabled`** (`runtime_activation.rs:640-660`). For `airk-state/1`
  these should be no-ops; confirm this before treating `CatalogUpgrade` as
  predecessor-only.
- **Q8. Release-to-schema mapping** (P3–P7, P10) is inferred from comments and
  the dated record. Confirm it against tagged sources before using it in
  user-facing text.
- **Q9. The `airk-embed` helper target.** It remains buildable, and container
  and runtime scripts still build it for versions below 1.8.14. Decide whether
  historical-fixture production still needs it before treating
  `embed_pool_legacy_*` as removable.
- **Q10.** The qualification status of the clean break itself is incomplete:
  the R39 installed-editor gates failed
  (`docs/research/installer-stop-2026-11-09.md`). This inventory records code
  ownership, not acceptance.
