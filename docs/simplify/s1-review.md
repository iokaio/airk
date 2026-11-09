# Milestone S1 review record

**Status:** implemented on branch `feat/simplify`, 9 November 2026, for user
review. This is the S1 exit gate of the
[simplification plan](../simplify-plan.md#milestone-s1-prove-the-approach-on-one-bounded-slice-next-milestone).
**Milestone S2 and later are not planned or started.** CLAUDE.md requires the
user to review these results first.

Nothing was pushed, merged, packaged or published. The stopped installed-editor
repair (D10) was not resumed.

## 1. Decisions applied and scope

The user asked for the plan to be implemented, with milestone commits on this
branch. Where §9 offered a recommended option, that option was applied:

| Decision | Applied |
|---|---|
| D1 | Track R narrowly, plus Milestone S1 |
| D2 | Delete only by evidence class |
| D7 | Commits on `feat/simplify` only; no push, merge or `release/1.9` |
| D8 | No wire, digest or persisted-format change |
| D11 | S1.4 component: the CLI install-file helpers |

D4, D6 and D10 were left untouched. D5 remains open; the pilot recommendation
is in §5.

| Item | Commit | Outcome |
|---|---|---|
| R2 deletion with evidence, R4 metrics | `69677a2` | 9 source files removed (about 4.4k lines), plus 2 scripts. Report-only tools added. |
| Pre-existing lint fix | `ae0bc69` | The scoped no-default Clippy (`dev-check.py -p airk-cli`) now passes |
| R3/S1.2 binding seam | `7bfa8a0` | The R39 race is reproduced deterministically (2 todo cases); 3 refusal cases pass |
| S1.1/R1 inventory | `c63228f` | [Compatibility inventory](compatibility-inventory.md), a draft for review |
| S1.4 untangling | `c2a9298` | `install_files` leaf module and contract; `user_paths` inversion |
| S1.2/S1.3 fixtures and pilot | `1ddba83` | [Pilot report](contract-generation-pilot.md); `fixtures/wire/` |

## 2. Track R2: what was deleted, and what was not

**Deleted:**

| Evidence class | Code |
|---|---|
| Not compiled (no target root reaches it) | Kernel `conditional.rs`, `lineage.rs`, `suggestions.rs`, `maintenance_pressure_tests.rs`; `airk-cli/src/bundled_model.rs` (the real file is generated into `OUT_DIR`) |
| No caller after a clean compile and Clippy | Store `vector_registry.rs` (never constructed); Store `catalog_experiment.rs` and `tests/catalog_experiment.rs` |
| No caller, test or doc reference | `scripts/check-service-four-projects.py`, `scripts/render-explorer.cjs` |

**Not deleted.** Each of these corrects a claim in the plan:

- `kernel/src/reconciliation.rs` is compiled into the kernel integration test
  `file_continuity_v2` through `#[path]`. It is test-only, not uncompiled.
  Deleting it needs a decision about that test.
- Store `sqlite`, `history`, `history_policy`, `retention` and `checkpoints`
  have production callers: `FileStore` methods, `reset.rs`, `relocation.rs`,
  `quota.rs` and `service_checkpoint.rs`. Kernel `history_controls` exposes
  `checkpoints` types in its public API. Store `maintenance` is used across
  crates (`WorkBudget`, `RepairOperation` and others), not only for
  `sync_parent`/`check_writer`. `vector_policy.rs` is live: the kernel calls
  `FileStore::vector_policy_bytes`. Appendix A's "No production callers" class
  does not hold for these. Proving the remainder dead needs the S2
  `pub(crate)`-tightening evidence.
- **Scripts:** 26 tracked scripts have no caller and no test; the plan said 24.
  Only 4 have no reference at all, and 2 of those were kept:
  - `check-usage-editor.cjs`, whose header names a private harness caller;
  - `refresh-license-texts.py`, a license-provenance maintenance tool.

  22 are documented entry points (bootstrap wrappers, doc-image renderers,
  user-facing verification). `release-1.4.4-repair.py` is **live**: it has a
  caller, a test and CI wiring.

**Test-identity proof:**

- `python scripts/dev/test_identities.py compare` (lightweight configuration).
- 70 binaries / 3,381 identities before; 69 / 3,373 after.
- Removed: exactly the 14 tests of the deleted modules.
- Added: the 6 new S1 tests.
- No other identity changed.

## 3. R3/S1.2: binding-identity seam

`packages/vscode/src/test/serviceBindingIdentity.test.ts` pauses the real
credential-source scan and orders refreshes with deferred gates, with no
sleeps.

- **Reproduced:** equivalent-authority replacement during the scan, and a
  provider entering during a pending equivalent refresh, both raise
  `workspace_registration_changed`.
  - The frames match the installed R39 stack exactly.
  - The definitions-change event fired 0 times.
  - Both cases are `todo` with owner D10 and expiry at the D10 decision.
- **Still refused, as required:** genuine runtime, control-directory and
  generation changes.
- **Additional observation for D10:**
  - A genuine *runtime* change also fires no definitions event, so a correct
    refusal has no retry signal either.
  - The sibling `httpCredential` path is uncovered.

This meets the S1 exit criterion: a seam has reproduced a recorded failure
class. It does **not** prove that the installed fixture saw an equivalent
refresh.

## 4. S1.1: compatibility inventory (summary)

See the [inventory](compatibility-inventory.md); the agent draft was
spot-checked against source.

- **13 predecessor rows** are recorded.
- **Registry steps 1–11** and the `Schema11`/`Schema12Authority0` layouts
  create every current installation, so they are live, not predecessor code.
- **No retained `airk-state/1` schema fixture exists.** That blocks S3.
- **Open questions Q1–Q10.** Q1 matters most: whether P2 adoption is still
  reachable on `airk-state/1`.

## 5. S1.2/S1.3: fixtures and pilot (summary)

See the [pilot report](contract-generation-pilot.md).

- **Golden capture.** Exact bytes for the `catalog_receipt` control read and
  its persisted payload. The digest vector was confirmed by an independent
  implementation. There are 32 reviewed read-retry decision cases.
- **Rust and TypeScript differ in four places.** These are W1–W4. W3 is the
  only permissive one: `retry_after_ms: 25.0` is accepted by `readRetry.ts`
  and refused natively.
- **D5 recommendation:**
  - Do not adopt type generation for wire types yet.
  - Keep reviewed fixtures and decision cases as the cross-language contract.
  - If generation is wanted later, use schemars for input schemas with
    hand-written format annotations, never ts-rs for `JSON.parse` numbers.
- **Not covered:** `file_info`, because its digest seam is private; it is an
  S2 candidate. The read-path example in the pilot is the combined-read retry
  rule.

## 6. S1.4: in-place untangling (before and after)

The module contract is
[`install_files/CONTRACT.md`](../../crates/airk-cli/src/install_files/CONTRACT.md).
The rewrite is re-verifiable with
`python scripts/dev/s1_4_install_files.py --check`.

| Production module graph (`simplify_metrics.py`) | Before | After |
|---|---|---|
| airk-cli top-level modules / edges | 65 / 277 | 66 / 284 |
| Largest airk-cli cycle (modules) | 41 | **36** |
| Modules in any airk-cli cycle | 43 | 40 |
| `runtime_install` fan-in (modules / files) | 22 / 29 | **18 / 24** |
| `user_paths` in a cycle | yes | **no** (no production edges) |
| `install_files` | — | leaf, fan-in 12 / 15, in no cycle |

The edge count rose because 12 modules now depend on the leaf
`install_files`. Edges into a leaf cannot form cycles. This is the intended
shape, not a regression.

**Not changed:** `validate_control_root` and `installed_control` stay in
`runtime_install`, because they depend on its `layout` child and lifecycle.
Moving them would drag the hub along. `runtime_install` itself still sits in
the main cycle.

**Clock seam (S1.2, second bullet):** not added. The S1.4 component makes no
time-based decisions, and a `Clock` there would be cosmetic, which "measure, do
not game" forbids. A justified first Clock seam was found instead; see §8,
finding F1.

## 7. Gate records

All runs were on local Windows 11 (x86_64-pc-windows-msvc) with Rust 1.98.0.

**Environment:**

- Target directory: `D:\code\Github\airk\target`, incremental on.
- Cargo was run from `~/.cargo/bin`. It is not on the shell `PATH` here, so
  the first `dev-check.py` attempt reported "Cargo was not found" and is not
  counted.

**Configurations:**

- "No-default" is `--no-default-features`, the lightweight configuration.
- **All-feature test binaries could not be linked.** The static ONNX Runtime
  is not provisioned on this host (`LNK1181` on
  `add_ort_library_path_or_enable_feature_download-binaries_see_ort_docs.lib`).
  Inference-feature tests were therefore **not run**.

| # | Check | Scope | Elapsed | Result | Timing report (`target/cargo-timings/`) |
|---|---|---|---|---|---|
| 1 | AIRK MCP `status` | binding | — | `direct_observation_unavailable`, not retried; local tools used | — |
| 2 | `dev-check.py --workspace --all-features` (fmt + Clippy) | baseline | 112 s | pass | `cargo-timing-20261109T181124591Z-…` |
| 3 | `cargo test --workspace --all-features --lib --bins --tests -- --list` | baseline | 82 s | **unavailable**: link needs static ORT | `…T181349398Z-…` |
| 4 | `cargo test --workspace --no-default-features --lib --bins --tests --no-run` | baseline | 95 s | pass, 70 executables; one pre-existing dead-code warning | `…T181555474Z-…` |
| 5 | `test_identities.py record` (lib, bins, tests) | baseline | 31 s | 70 binaries, 3,381 identities | — |
| 6 | `test_identities.py record --run -- --workspace --no-default-features --lib` | baseline | 528 s | 2,657 selected; **6 pre-existing failures** (F2) | — |
| 7 | `dev-check.py --skip-fmt --workspace --all-features` | R2 + S1.4 | 51 s | pass | `…T182751259Z-…` |
| 8 | `dev-check.py --skip-fmt -p airk-cli -p airk-store` | R2 + S1.4 | 44 s | **fail**: pre-existing `live_https_status` dead code; fixed in `ae0bc69` | `…T182843101Z-…` |
| 9 | `dev-check.py --skip-fmt -p airk-core -p airk-cli -p airk-store` | all S1 Rust | 51 s | pass | `…T183221863Z-…` |
| 10 | `dev-check.py --skip-fmt --workspace --all-features` | all S1 Rust | 19 s | pass | `…T183312870Z-…` |
| 11 | `cargo test -p airk-core --no-default-features --lib` | fixture capture | 35 s | 174 pass, 1 expected capture failure; capture reviewed, then accepted | — |
| 12 | `cargo fmt --all -- --check` | final | — | pass | — |
| 13 | `test_identities.py record` (lib, bins, tests) | final | 134 s | 69 binaries, 3,373 identities; compare as §2 | — |
| 14 | `test_identities.py record --run -- -p airk-core -p airk-cli -p airk-store --lib` | final | about 4 min | 1,910 selected; outcomes equal the baseline except F1 | — |
| 15 | Single test `private_native_imported_fact_pages_…` alone | F1 diagnosis | 147 s (test 75.3 s) | pass; confirms the load hypothesis | — |
| 16 | `npm run compile` (packages/vscode) | TS | 14 s | pass | — |
| 17 | `node --test out/test/wireFixtures.test.js out/test/serviceBindingIdentity.test.js` | TS T0 | about 1 s | 44 tests: 42 pass, 2 todo, 0 fail | — |
| 18 | `python -m unittest discover -s scripts/tests` | T1 | 165 s | 931 run, OK, 13 skipped | — |
| 19 | `python -m unittest scripts.tests.test_simplify_metrics` and `test_container_inputs` | T0 | <1 s | 8 + 4 OK | — |
| 20 | `git diff --check` | whitespace | — | pass | — |
| 21 | Generator pilot (scratch crate, not in the repository) | S1.3 | — | shadows reproduce all fixture bytes; 8/12 rejections | — |

The final `check-repo.ps1` run for the record commit is noted below §9.

**Not run** (deliberately, per the request to hold lengthy integration tests
until the plan is complete):

- Integration-test outcomes. Integration-test identities were compared
  (§2), but their outcomes were not run.
- The full mocked and native-enabled extension suites.
- Workspace `cargo test` and doctests.
- Lightweight-main `cargo check` as a separate gate. It is covered by the
  no-default `--all-targets` Clippy runs 8–9 for `airk-cli`.
- Runtime and VSIX builds, installed journeys, Docker, Neo/macOS/Linux and
  hosted CI.

Nothing here qualifies another platform.

## 8. Findings to carry into review

- **F1. Wall-clock deadline in a long Store test.**
  - **Cause:** `CurrentRead` arms a fixed 120 s `Instant` deadline at open
    (`airk-store/src/current/read.rs:534`).
    `private_native_imported_fact_pages_preserve_complete_cursor_and_representation_parity`
    holds one reader across up to 1,500 page reads.
  - **Evidence:** alone, the test takes 75 s (63% of its deadline). It failed
    with `CurrentRequestExpired` when run with the Python suite in parallel.
    The change under review does not touch this path.
  - **Proposed:** the first S2 Clock seam. Inject the reader clock or deadline
    so the test controls it. Until then it is a timing-dependent test, to be
    quarantined with an owner and expiry if it recurs. It was not retried to
    green, and its deadline was not widened.
- **F2. Pre-existing host-environment failures.** Identical before and after.
  - **CLI (4):** two "native runtime directory is not owner-only", one
    "clean-break selection changed since review", one
    `managed::catalog` callback count.
  - **Store (2):** "native Windows boundary policy unavailable". This needs
    `AIRK_PRIVATE_CENSUS_BASE` (CONTRIBUTING.md).
  - **Next step:** these need a supported fixture directory, not code
    changes, and should be re-qualified before T2.
- **F3. Plan corrections.** §2 above and inventory §3.3/§4 correct
  Appendix A on four points:
  - which kernel files were truly uncompiled;
  - the Store generation-era row (production callers exist);
  - `maintenance.rs`;
  - registry steps 1–12.

  The metric baselines in plan §8 were hand counts. From now on, use the
  script's figures (`records/metrics-s1-before.json` and `-after.json`).
  Under its definitions, the last 200 commits span 3+ components 62.0% of
  the time and 5+ components 36.0%.
- **F4. W3** (`readRetry.ts` accepts an integral float delay) is the one
  permissive cross-language gap. Fixing it changes extension behavior, so it
  needs its own approval.

## 9. Questions for the S1 review (user decisions)

1. **Inventory:** answer Q1 (P2 adoption reachability) and Q3 (pending P5
   upgrades). These gate any S2 deletion of adoption and upgrade code.
2. **D5:** accept "fixtures first, no type generation for now", or ask for a
   wider pilot.
3. **F1:** approve the Clock or deadline seam on `CurrentRead` as the first S2
   determinism step.
4. **D10:** the seam is ready. Whether to authorize the R39 repair stays your
   decision.
5. **S2:** which candidate milestone to plan in detail. Candidates are deletion
   with proof (needs Q1/Q3) or module honesty (S4).
6. **Plan text:** whether to fold the corrections in F3 into a plan revision 4.

`pwsh -NoProfile -File scripts/check-repo.ps1` was run on the tree of the
commit that adds this record; its result is in that commit's message.

## 10. Review decisions (user, 9 November 2026)

| Question | Decision |
|---|---|
| Q1: P2 standalone adoption on `airk-state/1` | **Refuse.** Adoption is not supported on 1.8.16 and later installations. Adoption code becomes an S2 deletion candidate once a refusal and the surviving duties are proven. |
| Q3: pending P5 catalog upgrade | **Explicit refusal** with a typed error and manual guidance. P5 upgrade-recovery code becomes an S2 deletion candidate once a refusal regression proves the original state is preserved. |
| D5: contract generation | **Fixtures first.** No type generation. Each operation gets reviewed `fixtures/wire` bytes or decision cases, and the hand-written validators are tested against them. |
| Next work | All four were authorized, in this order: the `CurrentRead` clock seam with the W3 fix; the R39 repair (D10); S4 module honesty; S2 deletion with proof. T2/T3 installed qualification still needs a separate go-ahead. |

## 11. Follow-up record (after the review)

| Step | Commit | Result |
|---|---|---|
| Clock seam + W3 | `faf3060` | `CurrentRead` reads an injected `RequestClock`. F1 is now deterministic, and a deadline regression shows the reader is alive at 119 s and expired at 120 s. `readRetry.ts` refuses non-integer number literals. Store outcomes match the baseline. Mocked extension suite: 1,342 tests, 1,321 pass, 21 skipped, 0 fail. |
| R39 repair (D10) | `7cd8840` | 15 binding staleness checks plus the HTTP health watch now compare request authority (`sameRuntimeAuthority`), not object identity. A genuine authority change fires the definitions event. The 2 former `todo` reproductions pass, and 3 cases were added (7/7). **Installed acceptance not rerun.** |
| S4 module honesty | `2e1a8fb` | The committed script `scripts/dev/s4_module_honesty.py` made 339 file moves. Non-platform `#[path]` mounts went from 361 to 32. Test identities are unchanged: 69 binaries / 3,374 tests, the only difference being the deadline test from `faf3060`. Library outcomes equal the baseline except the expected additions and deletions. Python: 931 OK. Clippy is clean in both configurations. |

**S4: what stays, and why** (each is listed by `--plan`):

- 20 thin platform switches.
- Children of a file that keeps a platform `#[path]`. rustc resolves these
  beside that file, as it does for `mod.rs`.
- Children of integration-test roots in `tests/`. Their canonical location,
  `tests/x.rs`, is a new Cargo test target.
- Files reached from more than one declaration.
- The `../` mount of `kernel/src/reconciliation.rs` from a test.
- `remote_onnx.rs`, the feature-switched alternative to `onnx.rs`.
- The `include!` embed-pool scheme (17 uses). Replacing it needs per-feature
  runtime builds, which is T2, so it is a separate step.

Two defects were caught and fixed before any compile was accepted:

1. The first plan moved test-root children into `tests/` and would have
   created new test binaries.
2. `include_str!` paths were mapped only for moved files.

**S2 plan** (approved scope, not started). Native adoption is still called by
the extension's agent-migration flow (`migrateAgentConnections` →
`adoptStandaloneConfigurations`). A native-only refusal would therefore turn
those migrations into hard failures without deciding what happens to the
standalone client configurations. The proposed order, one reviewed step each:

1. **Behavior (Q1).**
   - Native `adopt-legacy-index(es)` and adoption preparation refuse with a
     typed `standalone_adoption_unsupported` error.
   - The extension stops offering adoption. Standalone client declarations
     stay for review through the existing clean-break client capture (C4).
   - Regressions on both sides.
2. **Behavior (Q3).**
   - The clean break refuses a pending P5 upgrade (`AdmissionPending`) with a
     typed error and manual guidance.
   - A regression proves the original state is preserved.
3. **Deletion.** Remove the native adoption operations: CLI `legacy_*`
   adoption, Store `legacy_adoption*`, Kernel and Core adoption. Keep the
   adopted-layout **reader** and `ArtifactRef.legacy_origin`, because existing
   `airk-state/1` installations may hold them and changing that needs D8.
4. **Deletion.** Remove P5 upgrade recovery, keeping the Genesis helpers that
   inventory §4 marks MIXED.
5. **Deletion.** Remove the extension adoption and P5 recovery paths.

### S2 step 1 (Q1: no standalone adoption)

**Extension:**

- Every migration first runs the non-journaling `inspect-legacy-caches`.
- **While a standalone entry exists:**
  - The entry stays byte-for-byte for review, and the output names it.
  - No cache-retirement journal is prepared, so later activations are never
    blocked.
  - The cache retirement step is skipped.
  - Every completion check exempts exactly that file, server and container
    entry.
- Installation still completes, because a retained user entry is not a
  cleanup failure.
- A locator from a pending inspection whose entry the user has already
  removed is not retained, so its caches retire normally.
- The adoption method and its two adoption-success tests are removed.

**Native:**

- `adopt-legacy-index(es)` refuse a new operation with
  `standalone_adoption_unsupported` before reading any input.
- An operation that already left a batch or maintenance journal still resumes,
  so an interrupted adoption keeps its native owner. The CLI is that surviving
  owner.
- The adoption implementation therefore stays compiled. Deleting it (S2 step 3)
  first needs proof that no such journal can remain.

**Evidence:**

- Mocked extension suite: 1,334 tests, 1,313 pass, 21 skipped, 0 fail. That is
  11 adoption cases removed and 3 retention cases added.
- One retention case proves that a retained entry does not waive native proof
  for another entry launching a cached image. The non-journaling inspection
  stays the cache evidence for those proof checks.
- `airk-cli` library outcomes match the S4 baseline plus 2 refusal tests.
- Clippy is clean in both configurations, and the CLI reference was
  regenerated.
- The reference had been stale since the standalone CLI installer commit (it
  lacked `uninstall` and still showed the old `setup`), so this regeneration
  also repairs that drift.

**Left for later:** `scripts/check-standalone-adoption*.py` exercise new
adoption end to end. They are now obsolete qualification harnesses and are
candidates for removal in step 3. (Removed later: the failure variant in
`dfeb3e8`, the main script and its test with the S2 step 4b wave.)

### S2 step 2 (Q3: pending catalog upgrade)

**Behavior:**

- Clean-break activation, preflight and capture open the predecessor registry
  through `settings::open_predecessor`.
- That helper maps exactly `catalog::AdmissionPending` to the typed refusal
  `clean_break_predecessor_catalog_upgrade_pending`, with manual guidance. The
  original error is kept as the cause.
- Every step before that open is read-only, so the refusal precedes any reset
  step.

**Regression:**
`clean_break_refuses_a_predecessor_with_an_unfinished_catalog_upgrade_without_changes`.

- A Genesis catalog that was migrated but never activated is refused by both
  preflight and capture.
- Every file under the fixture is byte-identical afterwards.
- The `rebuild` test module passes (34 tests), and Clippy is clean in both
  configurations.

**Consequence for S2 step 4:** the clean break no longer needs the P5 recovery
code. Ordinary activation still calls `catalog_upgrade::begin_if_enabled` and
`resume_if_enabled` for the selected installation's own upgrade (inventory Q7).
Confirm those are no-ops on `airk-state/1` before deleting that path.

### S2 steps 3–5: blocked pending decisions (not started)

The plan allows deletion only where a surviving owner is proven. Both
remaining deletions are blocked on that rule.

- **Step 3: delete adoption.**
  - After step 1, the only remaining duty of the adoption code is resuming an
    interrupted adoption (a batch journal or `legacy-maintenance.json`).
  - That resume path is the code being deleted, so it has no other owner.
  - An unresumed maintenance journal also blocks manager changes
    (`require_manager_change_settled`) and activation (`require_settled`).
  - **Decision needed:** refuse a pending adoption journal with a typed error
    and manual guidance, as Q3 does. Deletion could then follow.
- **Step 4: delete P5 recovery.**
  - `upgrade::PRODUCTION_ACTIVATION_ENABLED` is `true`.
  - Ordinary activation (`catalog_upgrade::begin_if_enabled`,
    `resume_if_enabled` and `resume_catalog_before_lifecycle`) can still start
    or resume a catalog upgrade for the selected installation.
  - `resume_if_enabled` also resumes a pending Genesis, which is the live
    fresh-install path.
  - **Needed:** a trace showing that no admitted `airk-state/1` or clean-break
    path can begin a P5 upgrade. Then a behavior step disables P5 begin and
    resume (keeping Genesis resume), and only after that the deletion.
- **Step 5: delete the extension paths.** Follows steps 3 and 4.

### S2 step 3 (decision: settle an interrupted adoption, then delete adoption)

User decision: an interrupted standalone adoption "should be corrected if
possible and deleted otherwise to keep an uninterrupted flow".

- A settlement routine replaces the resume path:
  - the cache commands restore or deduplicate withheld helper images and clear
    the journal's withdrawal;
  - activation `begin` and the manager-change callers clear the adoption's own
    native fence under installer exclusion, restart only a service the
    adoption stopped, and delete the adoption journals and the owner-verified
    private archive copies.
- `require_manager_change_settled` now refuses only a maintenance journal that
  settlement had to keep (its own fence while the service runs, or an
  unverifiable journal).
- Catalog rows of an adoption interrupted before Ready are reported and kept;
  changing persisted catalog state needs D8. User decision: the report carries
  code `interrupted_adoption_rebuild_required` with guidance to rebuild the
  index from its original sources in Indexing, retire the inert entry if
  needed, and keep the machine and AIRK running until the rebuild completes.
- `adopt-legacy-index(es)` refuse unconditionally. The adoption implementation
  is deleted from `airk-cli` (`legacy_index_adoption.rs`,
  `runtime_legacy_authority.rs`, the maintenance capability, cache withdrawal,
  the catalog adoption helpers and `record_legacy_adoption`) and from the
  extension (`RuntimeHost.adoptLegacyIndexes`, its 16 MiB reply budget and
  `migrateStandaloneHttpEntry`).
- Kept for the next wave: `NativeSetupReview.legacy_adoption` and its digest
  domain, and every public store, kernel and core adoption API.

### S2 step 4a (decision: no in-place catalog upgrade; behavior, CLI side)

- P5 can no longer begin or resume: `activation::resume`,
  `resume_catalog_before_lifecycle` and `catalog_upgrade::activate` keep only the
  fresh-Genesis and ordinary-update paths.
- An ordinary update drains, verifies the candidate and predecessor payloads,
  then requires an existing index catalog. Without one it fails with
  `catalog_upgrade_unsupported`.
- The P5-only CLI code is deleted:
  - `legacy_retirement.rs`;
  - `prepare_readiness`, `resume_restore`, `resolved_control` and
    `control_for_resume`;
  - the predecessor-refusal capture in `authority.rs`;
  - `layout::catalog_control`;
  - the managed `Disabled` path.
- The P5 commands stay in the CLI surface and refuse with the same typed code
  before touching any state.
- Store's Genesis guard keeps refusing an in-place upgrade journal, now with the
  typed message.

**Kept for the next wave:**

- Public Store, Kernel and Core P5 APIs. Many CLI test fixtures still build
  catalogs with `CatalogUpgrade`.
- `PRODUCTION_ACTIVATION_ENABLED`.
- Persisted and wire shapes (`catalog_upgrade` owner field,
  `AuthorityV1Basis::LegacyUpgrade`, `RuntimeRefusal`).
- The extension's `runtimeLegacyRetirement.ts`. It now receives the typed
  refusal. (Removed in `dfeb3e8`, with
  `scripts/check-runtime-legacy-retirement.*`.)

**Evidence:**

- Clippy is clean (workspace all-features and no-default `airk-cli`).
- The workspace test targets compile.
- `airk-cli` library: 832 passed, plus the same 4 host-environment failures.
- `catalog_upgrade` integration test: 2/2. It now asserts the typed refusals
  without state access.
- `cli_commands` gives the same 9 passed / 5 failed as unmodified `HEAD`. Four
  fail in the native self-test, which cannot run without default features on
  this host. One is a pre-existing gap: the `uninstall` command has no test
  mapping.
- The new regression
  `ordinary_update_refuses_a_registry_without_a_catalog_and_keeps_existing_catalogs`
  checks that no file changes either way.

### S2 step 4b (P5 and adoption writers: Store, Kernel and Core)

Implemented by a delegated agent and reviewed here. There is no behavior
change on any admitted path: step 4a had already stopped P5 from beginning
or resuming, and step 3 had removed every caller of the adoption writers.

**Fixture migration (tests only):**

- `catalog_test_fixture::authority_one` keeps its signature. It now builds
  authority 1 through Genesis (`begin`, `backup`, `migrate`,
  `promote_authority_v1`, readiness, `settle_authority_v1`).
- A new `reviewed_index` helper creates the index and project that P5 used to
  import, through the real setup reserve/record/complete calls.
- A Genesis catalog starts at revision 2, where P5 started at 1. Expected
  revisions are therefore read from the live catalog, not hard-coded. No
  assertion's meaning changed.
- The `catalog_upgrade` integration test now builds its authority-0 state
  from Genesis `migrate` plus the same manual activation it had before.

**Deleted:**

| Crate | Removed |
|---|---|
| Store | `CatalogUpgrade` and its plan, backfill, receipt, stage, recovery, import and activation modules; the adoption writers (`NativeLegacyAdoptionCompletion`, standalone authority, archive verification and historical readers reachable only from them); `backup::restore`; the saved-adoption setup storage path |
| Kernel | `service_catalog_upgrade` and its prepared adoption evidence; `bootstrap_legacy_adoption` |
| Core | `catalog_upgrade_recovery` |

Also removed: the store `PRODUCTION_ACTIVATION_ENABLED` constant. It was
already `true`, and `capabilities()` still reports
`production_activation_enabled: true`, so the wire value is unchanged.

**Kept:** every persisted or wire shape, copied byte for byte. That covers:

- `LegacyPublicationIdentity`, `OriginalCurrentPublication`,
  `OriginalCatalogPublication`, `SavedFile`, `AdoptionResource` and
  `LegacyCompatibility`;
- `RuntimeRefusal`, the `catalog_upgrade` owner field and
  `AuthorityV1Basis::LegacyUpgrade`;
- the Genesis guard on `index-catalog-upgrade.json`;
- the registry validation and recovery that Genesis calls;
- the adopted-standalone readers that still serve
  `legacy_compatibility_snapshot`.

The last script pair, `scripts/check-standalone-adoption.py` and its test, is
removed in the same commit.

**Evidence:**

- Test identities: 3,345 before, 3,251 after. 94 removed (each tested a
  deleted writer or reader), 0 added, 0 outcome changes.
- Library tests:
  - Core 173/0 failed;
  - Kernel 219/0 failed;
  - Store 793 passed plus the 2 known host failures;
  - CLI 806 passed plus the 4 known host failures.
- Integration tests: `combined_reads` 32/32 before and after; `catalog_upgrade`
  2/2 after the fixture correction.
- Clippy is clean, workspace with all features and each crate in scope.
  Formatting and whitespace checks are clean.

**Residual:**

- The kept adopted-standalone readers and the `LegacyPublicationIdentity`
  serde round-trip have no remaining tests. Their tests were among the
  removed writer tests.
- Several kept items still live in modules named `upgrade`, `settlement`,
  `current_archive` and `historical_catalog`. Renaming them is a separate
  structural step.

### S4 completion: embed-pool `include!` scheme removed by deleting `airk-embed`

The evidence for this step is the answer to inventory question Q9 in
[s6-s7-plan.md](s6-s7-plan.md): no build path for version 1.8.14 or later
produces the helper. Implemented by a delegated agent and reviewed here.

**Removed:**

- the `embedding-worker` feature, the `airk-embed` binary and the guarded
  provider fixture target;
- the helper-only entry and context sources;
- two integration-test binaries and the helper plumbing in the test common
  module.

**Converted:** the remaining pool and inference sources became ordinary
modules under `inference_engine/`, moved with `git mv`. The macro unwrap is
`scripts/dev/s4_unwrap_embed_macros.py`. `include!` of Rust source dropped
from 17 to 0, and `#[path]` mounts from 52 to 47.

**Re-gated:** `split_tests` and the split cutover fixture never used the
helper, so they move from `embedding-worker` to `local-inference`. They still
run under `--all-features`.

**Builders:** the runtime and container builders refuse versions below 1.8.14
before building anything. Historical pairs are built from their tagged
sources. Recognition of historical pair payloads stays.

**Evidence (this host):**

- Test identities with no default features: 3,251 tests on both sides; 0
  removed, 0 added, 0 outcome changes. The helper's two integration binaries
  compiled to 0 tests in that configuration.
- Clippy is clean for the workspace with all features, for `airk-cli` with no
  default features, and for `airk-cli` with `local-inference`.
- `cargo check` with no default features passes.
- CLI library: 806 passed plus the 4 known host failures.
- Scripts suite: 921 OK.
- `boundary_check` passes, with its baseline down from 49 to 27 exceptions.

**Not run here:** linking or running `local-inference`, including the moved
pool and owner tests. That waits for the full ladder.

**Dead-code candidates left for a later evidence step:**

- the guarded bootstrap acceptance used only by the helper;
- the guarded-pair qualification tests;
- the no-`local-inference` helper-spawn path.

The help text that still names the helper is corrected separately.
