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
