# One AIRK completion plan

**8 November follow-up authorization, refined:** retain automatic rebuilding
and offer it earlier as an explicit upgrade choice, without classifying all
incompatible-failure permutations. Native `runtime activate --rebuild-indexes`
requires the exact `--expected-active` inventory; after admission it automatically
rebuilds from eligible raw managed saved sources in fresh successor state.
Preserve old state, supported settings, source boundaries and the separate
migration requirements. Unsupported settings or catalog/grant configurations
must be refused before state changes; successor client re-enrollment is explicit. The
[rebuild recovery contract](upgrade-rebuild-fallback.md) records this bounded
production requirement; native/editor source is implemented, with installed and
frozen qualification pending. The authentic 1.5.12 schema-8 metadata bridge passed
its scoped regression; installed producer and imported-authority acceptance are
still separate. This refinement does not waive review, freeze, run or publication gates.

**8 November 2026, revision 2: approved for local completion.**

The user approved the proposed section 7 choices on 8 November 2026:
temporary Windows CurrentUser\\Root enrollment for one Code run followed by
owned removal; documented refusals with zero failures/uncertain outcomes and no
minimum success count; raw API and held-inference checks required. Missing
performance thresholds, additional inference/fault scenarios, two-user testing
and universal/many-client migrations are recorded limitations. Neo GUI Code and
a second OS account have not been supplied and remain unqualified. The finish
line is local completion; commits, pushes, PRs, merges and publication remain
unauthorized. This approval permits implementation and the ordered validation
ladder; its review, freeze and stop gates still apply.

This revision replaces the first draft. It keeps that draft's constraints. It
adds the following, all checked against the live working tree on 8 November 2026:

- code anchors for each workstream;
- design decisions that are already made, so they are not reopened;
- a test budget priced from measured costs;
- stop rules;
- the open completion items the first draft left out;
- one combined set of the decisions that only the user can make.

Line numbers may shift as you edit. Re-locate each anchor by its function name
before editing.

The implementation stop pending approval is lifted. Apart from the explicitly
approved temporary Windows trust operation, this plan does not authorize:

- publication, commits, pushes or PRs;
- changes to an operating-system trust store;
- changes to the user's installed configuration.

This plan finishes the existing [One AIRK plan](one-airk.md). It does not replace
that plan's acceptance contracts or widen AIRK's scope. Historical evidence stays
in the [implementation ledger](research/one-airk-implementation.md) and the
[release record](releases/release-1.8.14.md).

## 0. Why work stalled, and the rules that prevent it

The ledger shows the same cycle more than 40 times:

1. Patch one symptom, often in the test harness only.
2. Rebuild or reseal the runtime.
3. Rerun a long campaign.
4. Hit the next symptom a few minutes further in.

Examples:

- 22 Windows debug runtimes, r4 to r25.
- Neo Mac batches r6 to r13. Each failed at a later stage than the one before.
- About 20 attempts at the staged-editor journey.
- A 33-minute workspace run made while native inputs were still changing
  (`native_inputs_unchanged=false`), so its result was unusable.
- Two test filters that selected zero tests.
- The same faults coming back several times:
  - owner-private ACL fixtures, five times;
  - Windows path length, five times;
  - a forward step in the catalog clock, three times;
  - Unix-only Clippy errors that only Neo exposes.

Almost every one of these could have been found in seconds by reading the code or
running a targeted test.

### 0.1 Cost tiers (measured on this project)

| Tier | Examples | Measured cost | When allowed |
| --- | --- | --- | --- |
| **T0** | Targeted extension tests run with `node --test` on named files; one Python `unittest` module; pure Rust unit tests through a filtered `nextest` | 0.3–3 s per run (plus `tsc` or an incremental compile) | Any time during repair, as often as needed |
| **T1** | Scoped `python scripts/dev-check.py -p <crate>`; mocked full extension suite (`npm test --prefix packages/vscode`); full Python suite; workspace Clippy | Workspace Clippy 9–74 s; mocked extension suite 19–24 s; full Python suite 68–92 s | During repair, before handing a component back |
| **T2** | Debug runtime build and VSIX; enabled-native extension suite; installed-editor journeys; 1/10/64-client campaign; actual Code host run; Neo batch | Runtime build 42–163 s; enabled-native suite 286–312 s; installed editor 250 s; client campaign 379 s; Code host 106 s; Neo batch 600–680 s | **Only** after the review gate in section 5, and only on frozen inputs |
| **T3** | Full workspace `nextest`; optimized build; Docker qualification build; `ci-local.ps1` | Workspace tests 1,850–1,975 s; optimized build 794 s; Docker build over 2,250 s before compilation started (r2) | Once per host, in the final ladder (section 6) |

### 0.2 Run rules

These rules are mandatory.

1. **Find defects with T0 and T1 checks.** T2 and T3 runs confirm reviewed code.
   They are not a way to discover what is wrong.
2. **Declare the expected result before every T2 or T3 run.** Write down:
   - the exact command;
   - the input identities (native, extension, harness, fixtures);
   - the expected result;
   - the condition that would make you stop.

   Use commands already recorded in the receipts under
   `artifacts/local/one-airk-1.8.14/resume-*.json` instead of inventing new ones.
3. **Freeze inputs before you start.** Do not start a T2 or T3 run while any source
   it consumes is being edited. Receipts already record `native_inputs_unchanged`,
   `script_inputs_unchanged` and `extension_inputs_unchanged`; abort the run if any
   of them is false.
4. **Check the selected test count.** Every filtered run must report the number of
   selected tests, and that number must be above zero. Treat zero selected tests as
   a failed run, never as a pass.
5. **Changes to the harness or fixtures re-run only what uses them.** Do not
   rebuild native artifacts for a change that touches only scripts or tests. The
   receipt identities show what changed.
6. **Two strikes, then stop.** If a T2 or T3 step fails twice for the same
   workstream, stop all runs. Write a root-cause note that names the assumption
   that turned out to be wrong, and bring it to the user. Do not start a third
   cycle without the user.
7. **Collect the whole run before correcting anything.** Read every failure in a
   run first. Then make one coherent correction across all of them.
8. **Never make a check pass by weakening it.** Do not:
   - weaken an assertion;
   - widen a limit, queue or retry;
   - relabel a skipped or failed check as passing;
   - restart a service just to manufacture a cleanup receipt.

   Keep every failed fixture and every incomplete cleanup receipt.

## 1. Current position (8 November follow-up)

The [implementation ledger](research/one-airk-implementation.md) now includes
the retained r25 results and the subsequent r26 repair/acceptance cycle. The
baseline below remains dated evidence; it is not the current candidate status.

- **Current manual-test candidate.** Windows debug **r28** and its matching
  VSIX are built and package-verified. Native manifest `5882675c…`, compiled
  source `c173380b…` / 1,136 inputs, VSIX SHA-256 `2312dd70…`. Workspace Clippy
  and 49 selected native regressions passed; the full mocked extension suite
  passed 1,096 with 21 skips. See the [working record](releases/release-1.8.14.md)
  for the local file and receipts. Installed editors, platform installers and
  final release qualification remain pending.

- **Preserved candidate.** Windows debug r26, manifest `2069c2d6…`, was built
  from source `ac2620f0…` / 1,116 inputs. Native packaging passed in 17.471 s
  (`completion-native-package-r26.json`), VSIX packaging in 20.498 s
  (`completion-vsix-package-r26.json`), and enabled-native extension validation
  passed 1,108 tests with 3 skips and no failures in 162.923 s wrapper time
  (`completion-enabled-extension-r26-r2.json`). All 811 original runtime seals
  and consumed inputs remained unchanged. These are debug component/package
  observations, not installed-editor or final qualification.
- **Checkpoint repair.** Setup preflight passed 1/1 and held checkpoint passed
  1/1, including 66 actual embedding calls before the checkpoint sealed. Only
  two external test modules changed; their separate test identity is
  `9335cf1d…`. The original r26 build identity remains unchanged.
- **Migration prerequisite.** The authentic producer failed at predecessor
  registration before indexing or candidate activation. Reviewed 1.8.12 catalog
  migration cannot substitute for imported-legacy lineage. The
  [fixture matrix](testing/fixture-compatibility.md) records the compatibility
  prerequisites. The authentic schema-8 metadata bridge now passed its scoped
  regression using original 1.5.12 DDL; original publication, imported authority,
  native ownership and installed producer acceptance remain unqualified. The
  failed fixture and exact native Stop receipts remain preserved.
- **Early rebuild source.** Native/editor source implements the bounded option,
  preserved old state, fresh compatible storage, progress and interrupted resume.
  Store checks passed 8/8. Native selection passed 32/33 with one MCP fixture
  budget failure retained; corrected MCP selection then passed 2/2. Fourteen
  Store-backed interruption subcases passed. `Ready` describes succeeded native
  Index jobs; semantic readiness is separate. Actual CLI/external HTTPS,
  installed/platform and frozen qualification remain pending. Production changes
  require a new candidate identity; do not rebuild or relabel r26.
  Explicitly preserved rebuild predecessors are outside ordinary obsolete-payload
  cleanup; later upgrades retire unprotected intermediates and report protected
  evidence separately. An unavailable historical job receipt proves neither
  success nor failure and does not replace current native readiness.
- **Standalone source.** Guided CLI setup/launch/uninstall and four-platform
  installer/release support are implemented in source. Packaging contracts
  passed 77/77 and release/site contracts 51/51 using mocked platform tools.
  The latest mocked extension suite selected 1,116 tests: 1,095 passed, 21
  skipped, no failures, 9.052 s wrapper. These are not native installer builds,
  signing, installed-client or publication evidence. Guided native/launch
  validation remains separate; installers and attestations are unpublished.
- **Python qualification.** The clean full contributor-identity rerun passed
  896 selected tests: 883 passed and 13 skipped, in 77.768 s wrapper time
  (`completion-python-t1-r1.json`). Native/script inputs stayed unchanged. The
  earlier sandbox TLS/Git failures and affected-module 22/22 rerun remain dated
  evidence rather than being relabeled as this full pass.
- **Remaining acceptance.** One authentic producer preflight precedes all six
  editor scenarios, including `different`, Workbench exports and saved-config
  refresh. Windows 1/10/64-client load, real Python/Node/PowerShell API clients,
  issuer rotation and actual Code with authorized temporary OS trust remain.
  Neo retirement/uninstall/native acceptance, Docker qualification/image smoke,
  full workspace/excluded campaigns/doctests, lightweight/inference/native-client
  gates, license/advisory checks, `ci-local`, optimized build and packaging remain
  separate frozen qualification work.

### 1.1 Historical baseline at revision 2

- **Versions.** The native, extension and lockfile versions are **1.8.14**. Do
  not bump them again.
- **Branch.** `feat/finetuning`, latest commit `f54fb4d`. The working tree has
  about 84 uncommitted changes. Preserve them.
- **Build.** Windows debug **r25** was built from native source `9bef1b79…`, with
  1,115 inputs. It is a debug candidate only. The tree has changed since r25, so
  no r25 result qualifies the current tree.
- **AIRK MCP.** It is disabled in `.codex/config.toml` at the user's request. Use
  live source navigation (`rg` and file reads).

### 1.2 Retained r25 baseline evidence

The ledger's dated reconciliation retains these results from
`artifacts/local/one-airk-1.8.14/resume-*` and
`artifacts/local/one-airk-1814/neo-r13-*`. The agent guide now includes the
actual Claude acceptance, with its Windows debug scope and projection limit.

| Area | Observed (receipt) | Limit |
| --- | --- | --- |
| Formatting, workspace Clippy, lightweight compile and default build on r25 | Passed. `resume-workspace-clippy-r25.json` (74 s), `resume-lightweight-r25.json`, `resume-workspace-build-r25.json` | Only these gates |
| Workspace tests on r25 | Interrupted at about 961 of 3,242 (`resume-workspace-tests-r25.log`) | No completed result. Doctests not run |
| Native targeted corrections | Retirement, session and provenance checks passed. Reference-refresh journey and private HTTP command selection passed 11/11 | Selected tests only |
| Windows r25 runtime and VSIX | Embedded model and import checks, HTTPS and model clients, archive and VSIX verification passed | Debug identities |
| Enabled-native extension suite | 1,088 passed, 3 skipped (1,091 total) | Run before the latest controller change |
| Actual Claude 2.1.118 | Scoped reads, reconnect, additive trust and wrong-issuer rejection passed | Windows debug only. Claude's control projection omits MCP `isError` |
| Actual Code 1.145.2 | Failed TLS before tool discovery (`resume-actual-code-r25-collected-r1.json`) | **Tested only the Node extra-CA path.** OS user trust was never enrolled (see E). Native cleanup is unqualified: `native-stop.json` was missing |
| Installed journeys (`resume-installed-editor-r25.json`) | `same` passed. `different` timed out after native index creation | Later scenarios did not run |
| Client campaign (`resume-client-load-r25-r1.json`) | 1 and 10 clients passed. At 64 clients the run stopped on HTTP 503 admission refusals | Later phases did not run |
| Admission diagnostic | 16 reads succeeded, 36 `authentication_busy`, 12 `native_validation_busy`. Every session closed and the native service stopped | No proof of fairness |
| Neo r13 | Python, formatting, workspace Clippy, lightweight compile, 51 native regressions, debug model, runtime and checkpoint passed | Uninstall failed with `ESRCH` (see B). Cargo cleanup was done |
| Linux r3 | Canceled during archive preparation | Static build not claimed |

## 2. Frozen decisions (do not reopen)

The ledger shows each of these being decided more than once. They are now fixed.
To change one, bring evidence and a reason to the user.

1. **Client trust.** Use normal CA validation and keep existing public and
   enterprise roots (`one-airk.md:333`).
   - Code and Copilot get per-user OS trust through the existing reviewed command,
     **AIRK: Trust Local HTTPS Issuer**: Windows `CurrentUser\Root`, or the macOS
     login keychain (`docs/agent-integration.md:137-142`).
   - Claude and Codex keep the additive `NODE_EXTRA_CA_CERTS` /
     `CODEX_CA_CERTIFICATE` bundle.
   - Linux Code trust needs an administrator and is a documented limitation in
     1.8.14. It is not implemented.
   - Never disable TLS validation or VS Code's proxy and system-certificate
     integration.
2. **The 64-client case.** A bounded, typed refusal is acceptable
   (`one-airk.md:375-377`, `:447`). Nothing in §13 requires all 64 requests to
   succeed at the same time. Do not raise semaphores such as `AUTH_TASKS=16`, add
   queues, or retry refused requests to make 64 succeed.
3. **Catalog clock.** If the catalog clock moves forward and authority is
   unchanged, the session is kept. This rule caused three separate failures.
   Tests must not assume a fixed catalog revision.
4. **Busy-retry policy.** Retries live at the shared native boundary. The single
   50 ms `selection_busy` safe-read retry stays as it is.
5. **Old launchers.** They are removed, not kept as a fallback. The payload is one
   image.
6. **Native limits.** The checkpoint deadline is 180 s. The credential ledger
   holds 128 records in 1 MiB.
7. **Test environment.** TLS, ACL and owner-private suites run as the contributor
   identity, with the existing protected fixture base
   (`AIRK_PRIVATE_CENSUS_BASE`). Never run them in a sandbox, and never change the
   ACLs on a user's directory.
8. **Container inputs.** Admit them file by file. Never admit a whole repository
   directory to make a build compile.

## 3. Workstreams

Each workstream lists:

- the verified defect;
- the required design (deviate only with a written reason);
- the files it owns;
- its T0 and T1 checks;
- "Done when".

A workstream is handed back only when "Done when" holds. No T2 run happens
inside a workstream.

### A. Extension: separate ownership for management, evidence and advice

**Owner files:**

- `packages/vscode/src/indexingCatalogController.ts`
- `packages/vscode/src/explorerController.ts`
- `packages/vscode/src/explorerEvidence.ts`
- `packages/vscode/src/explorerErrors.ts`
- `packages/vscode/src/explorerWebview/{indexing,workspaces,app}.tsx`
- the related tests

**Verified state.**

- The evidence epoch is `selectionEpoch` (`explorerController.ts:125`). It
  advances in `select()` (`:306`) and in `specsChanged()` (`:321`). A newly
  registered evidence spec takes the `specsChanged()` path.
- The uncommitted diff takes the catalog owner off the epoch. The owner is now
  `[observationIdentity, windowContextIdentity]` (`:152`). Every `indexing.*`
  message except `indexing.recommendations*` is now exempt from the
  post-dispatch epoch check (`:570-575`). That fixes the original failure, where
  the setup receipt was discarded.
- Advice still has **no token of its own**:
  - `recommendations()` (`indexingCatalogController.ts:52-57`) runs inside the
    management `run()`. It shares the management `sequence`, the busy flag and the
    clearing of errors and reviews (`:89-92`), and it bumps `view.generation`
    (`:55`).
  - `dismissRecommendations()` and `clearEvidenceRecommendations()` (`:61-62`)
    bump nothing. A late result is therefore stored after the user dismissed it.
    This is the failing test `test/indexingCatalog.test.ts:25`, lines 36-37.
- Advice can also break unrelated management work:
  - Storing advice bumps `view.generation`. That makes the `setupApply`
    generation check fail (`:294`).
  - Each advice read increments `sequence`. That breaks the split check
    `splitPreparedSequence !== sequence-1` (`:447`, `:463`).
  - A failed advice read clears `review` and `setupReview`.
- `manage.action` and `manage.command` (`explorerController.ts:764-786`, which
  include Trust Issuer and issuer rotation) are still fenced by the evidence epoch
  in `handle` (`:575`). So is the UI-local `notice.dismiss`, `details.copy`,
  `workbench.toggle` and `extension.reload`. A management action that committed can
  therefore be reported as `REQUEST_SUPERSEDED`.
- The webview gives the Indexing screen a `key` that includes `selectionEpoch`
  (`workspaces.tsx:97`). When evidence discovery changes the epoch, the screen
  remounts and drops the new `workingNow` lock (`indexing.tsx:17-70`). The UI
  tests at `explorerIndexingUi.test.ts:150-192` change the epoch without
  remounting, so they never exercise this.
- The missing-scope error `http_connection_requires_reviewed_scope`
  (`explorerEvidence.ts:197`) is wrapped as `SERVICE_UNAVAILABLE` by `connect()`
  (`:211`). The user sees "The AIRK service is not connected"
  (`explorerErrors.ts:29`, `:52`).

**Required design.**

1. **Give advice its own fence.**
   - Move `recommendations()` out of `run()`.
   - Add an advice sequence that is bumped by request, clear, dismiss, `select()`
     and `specsChanged()`. Keep any advice-pending flag separate from the
     management `busy` flag.
   - Store a result only if three things are unchanged: the advice sequence, the
     evidence epoch and the catalog owner. Otherwise reject with
     `stale_catalog_view`.
   - Advice must never touch the management `sequence`, `view.generation`,
     `busy`, `review` or `setupReview`. If the webview needs a re-render signal,
     publish a separate `adviceGeneration` field.
   - Copy and use keep their current guards and also check the advice sequence.
2. **Exempt non-evidence messages from the epoch check in `handle`.** Do this for
   `manage.action`, `manage.command` and the UI-local messages listed above.
   Review each one. The rule is that a committed management action always reports
   its real outcome.
3. **Stop the Indexing screen remounting.** Remove `selectionEpoch` from its
   `key`. Evidence screens keep the epoch in their keys.
4. **Classify the missing scope properly.** Give
   `http_connection_requires_reviewed_scope` its own classification and advice
   (for example `SCOPE_NOT_GRANTED`: "review the scope grant") in `connect()` and
   `explorerErrors.ts`. It must never map to `SERVICE_UNAVAILABLE`.
5. **Audit the full ownership table.** Use the table in section 5. Cover setup,
   grant, scope and set updates, source policy, deletion, split, storage, Usage,
   exports, reconnection and disposal. Preserve exactly-once mutation. Do not
   replay a setup or reapply a scope.

**Tests.**

Keep these existing tests passing:

- `indexingCatalog.test.ts:25`
- `explorerShell.test.ts:106-156` and `:492-509`
- `indexingSplit.test.ts:71-73`

Add these tests:

1. An advice result that arrives during setup apply changes neither `generation`
   nor `sequence`, and the setup receipt still commits.
2. Split prepare, then an advice read, then split apply: the apply still succeeds.
3. A `manage.action` that completes across an epoch change reports its outcome.
4. Changing the epoch through the real Indexing remount path keeps the
   `workingNow` lock.
5. An ungranted scope produces the scope notice, not the service-down notice.

**T0 command**, from `packages/vscode`:

```
npx tsc -p . && node --test out/test/indexingCatalog.test.js out/test/explorerShell.test.js out/test/explorerIndexingUi.test.js out/test/indexingSplit.test.js out/test/localSearchScope.test.js
```

**T1 command:** `npm test --prefix packages/vscode`. Without `AIRK_TEST_BINARY`
the native tests skip.

**Done when:** T0 and T1 pass and the ownership table is filled in. The
enabled-native suite and the installed `different` journey are T2 and wait for
section 6.

### B. Mac process retirement and uninstall

**Owner files:**

- `crates/airk-cli/src/runtime_upgrade_stop_macos.rs`
- `crates/airk-cli/src/runtime_upgrade_stop.rs`
- the retirement test gating in `crates/airk-cli/src/runtime_retirement.rs`

**Verified state.**

- `wait()` (`runtime_upgrade_stop_macos.rs:198-209`) starts with
  `owner.path()?` (`:200`). That returns any path error immediately. The deadline
  is checked only after a successful observation of a live process.
- The uncommitted diff makes `ESRCH` count as retirement only when a dead
  task-name right also proves it (`:27-31`, `:93-100`). That is correct. But it
  means a missing path without that proof now makes capture (`:176`), terminate
  (`:183`) and wait (`:200`) fail hard, instead of waiting.
- `stop_paths()` (`runtime_upgrade_stop.rs:64-92`) runs audit, capture and
  terminate with **no deadline**. It then creates a new 5 s budget (`:84`) for
  `wait`.
- The uninstall chain is:
  `runtime_install_commands.rs:619` →
  `runtime_retirement::uninstall` (`:631`) →
  `manager.stop()` (`:698`) →
  `stop_paths` (`:700`).
  Other callers of `stop_paths`:
  - `stop`, reached from `runtime_activation.rs:542`,
    `runtime_install_commands.rs:345` and `runtime_retirement.rs:365`;
  - `runtime_legacy_launchers.rs:322`.
- The uninstall process test `runtime_retirement.rs:72-74` is gated to
  `cfg(any(windows, linux))`, so uninstall has no macOS coverage.
- `mod native` compiles only on macOS: `build.rs:8-16` builds Objective-C with
  `-Werror` and needs an Apple SDK. Pure code outside `mod native` compiles and
  tests on Windows, because `runtime_upgrade_stop.rs:5-7` includes the module
  under `cfg(test)`.

**Required design.**

1. **One deadline for the whole stop.** Create it in `stop_paths` before the audit,
   or take it from the caller, and pass it to capture, terminate and wait. Check it
   before and after each observation. No new budget appears after terminate.
2. **A pure classifier outside `mod native`.** Its inputs are the path-observation
   result (or errno), the dead-name bit and whether the image matches. Its output is
   `Live`, `Retired` (only with positive proof), `Unknown {phase, errno}` or `Hard`.
   - `EPERM` and a changed image are `Hard` immediately.
   - `ESRCH` without proof is `Unknown`.
3. **A generic `wait_until(observe, now, sleep, deadline)`, also outside
   `mod native`.**
   - `Unknown` keeps polling.
   - If the deadline expires while the state is `Unknown`, return a
     pending/unknown error. The error carries the phase, PID, errno, elapsed time
     and the last classification. That is never a success.
4. **Capture and terminate wait as well.** When they see `Unknown`, they poll
   within the same deadline instead of failing.
   - Never send a signal without the verified audit-token identity.
   - Never signal a bare numeric PID.
5. **Check the macOS test gate.** Find out why `runtime_retirement.rs:72-74`
   excludes macOS. If the only reason was this defect, add macOS to the gate.

**Tests.**

Pure tests that run on Windows:

- `Unknown` then `Retired` inside the deadline succeeds.
- `Unknown` that persists gives the pending error with context.
- `EPERM` is `Hard`.
- A changed image is `Hard`.
- A deadline already expired before the first observation fails.

Keep the macOS owned-child test (`:215-255`) and
`old_unresponsive_process_is_stopped_by_verified_handle_only` (the wait-before-reap
case).

**T0 command (Windows):**

```
cargo nextest run -p airk-cli --lib -E 'test(upgrade_stop) | test(retirement)'
```

Check that it selects more than zero tests.

**T1 command (Windows):** `python scripts/dev-check.py -p airk-cli` with the
features the change needs.

**Done when:** the Windows T0 and T1 checks pass. The macOS compile, Clippy and
test steps are the **first steps** of the single Neo batch (section 6, step 4).
Neo r7 and r8 failed on Unix-only Clippy errors; checking first catches those in
about 150 s, not 10 minutes.

### C. Container inputs and archive extraction

**Owner files:**

- `.dockerignore`
- `Dockerfile` (the source stage)
- `scripts/build-service-container.py`
- `scripts/provision-static-onnx-tools.py`
- their tests under `scripts/tests/`

**Verified inventory.** Every file below is needed by the container test stage
(`nextest --workspace --all-features`) but is not admitted:

| Input | Consumer |
| --- | --- |
| `tools/parser-train/` files: `manifest.json`, `role-queries.json`, `novel-role-queries.json`, `conversational-regression-v4.json`, `safety.json` | `include_str!` in `crates/airk-kernel/src/workbench.rs:3811, 3884, 3905-3906, 4226-4227, 4389` (test only) |
| `tools/parser-train/` files: `fixtures.json`, `fixtures-v2.json`, `inference.json`, `evaluation.json` | `include_str!` in `crates/airk-retrieval/src/intent.rs:169, 175, 203, 212` (test only) |
| `docs/research/contracts-v190/fixtures.json` | `include_str!` in `crates/airk-mcp/src/remote.rs:1221`. Runtime reads in `crates/airk-core/src/source_policy_tests.rs`, `source_policy_split_tests.rs` and `verification.rs:960, 1025` |
| `docs/cli-reference.md` | `crates/airk-cli/tests/cli_commands.rs:1434`; `crates/airk-cli/src/main.rs:3019` (a binary unit test) |
| `scripts/check-{container,macos-direct-runtime,managed-runtime,runtime-activation,runtime-legacy-retirement,service-clients,service-install,tokenizer-calibration,usage-inputs}.py` | The source-map table `ELSEWHERE` in `cli_commands.rs:972-1378`, read at `:1547` |
| `packages/vscode/src/test/{diagnosticLogs,extensionUninstall,serviceBootstrap}.test.ts` | The same table |

Inputs under `crates/**`, the `.sql` files, the model catalog, the licenses and
`httpAuthorityDigest.json` are already admitted. The runtime payload identity
(`scripts/build-service-runtime.py:242-262`) leaves out `tools/`, `docs/` and
`packages/` on purpose, because they are test-only inputs. Keep that, and say so in
the container documentation.

**Archive defect.** `archive_entries` (`provision-static-onnx-tools.py:65-125`)
returns its entries sorted by path (`:125`). `extract` (`:128-153`) then fetches
each member by name, in sorted order (`:146`). On a gzip stream, every backward
lookup decompresses from the start again. That makes extraction roughly quadratic,
and every file is decompressed twice. This is the archive-preparation cost seen in
Linux r2 (2,252 s before any compilation) and r3.

**Required design.**

1. **One manifest of public container inputs** (for example
   `scripts/container-inputs.json`) listing exact files.
   - `build-service-container.py` reads it. It replaces the one-off allowlist
     exception at `:94-95`.
   - A unit test checks that `.dockerignore` and the `COPY` lines in `Dockerfile`
     match the manifest exactly.
   - Keep the private-state filter (`:113-114`) and every existing exclusion
     unchanged.
2. **An inventory test that stops this recurring** (T0, seconds). It scans
   `crates/**` for these forms and resolves each path that leaves `crates/`:
   - `include_str!`, `include_bytes!`, `include!`;
   - `concat!(env!("CARGO_MANIFEST_DIR"), …)`;
   - `Path::new(env!("CARGO_MANIFEST_DIR")).join(…)`.

   It also parses the `cli_commands.rs` `ELSEWHERE` table. It asserts that every
   resolved path is in the manifest. List any runtime reads it cannot detect
   explicitly in the test, with a comment.
3. **Extract in a single sequential pass.**
   - Open the gzip archive in stream mode (`tarfile.open(mode="r|gz")`). Stream mode
     forbids backward seeks, so it enforces the fix.
   - Apply the existing checks to each member as it arrives: root, duplicates,
     omitted links and link targets, type, size and expansion budgets.
   - Write each file with `xb` and hash it while copying.
   - Sort the receipts by path only after the pass, then call `verify_tree`.
   - Zip archives are random-access and need no change. `archive_entries` stays for
     the case where the destination already exists.

**Tests.** `test_container_runtime_build.py` (exact-exception tests at `:33` and
`:53`), `test_container_check.py`, `test_static_onnx_tools.py` and
`test_static_onnx_provisioning.py`. Add an extraction test whose archive members
are out of alphabetical order, and assert identical sorted receipts.

**T0 command:**

```
python -m unittest discover -s scripts/tests -p "test_container_*.py"
```

Do the same for `-p "test_static_onnx_*.py"`.

**Done when:** T0 passes, the inventory test passes, and the reviewer has read the
manifest. Run no Docker build before section 6, step 5.

### D. HTTP refusal evidence and load accounting

**Owner files:**

- `scripts/http_fixture.py`
- `scripts/benchmark-service.py`
- `scripts/benchmark-mcp-investigation.py`
- `crates/airk-cli/src/http_gateway_tests.rs`
- `docs/local-https-api.md`

**Verified state.**

- **Native.** `authentication_busy` (`http_gateway.rs:397-402`) and
  `native_validation_busy` (`:491-496`) are refusals from `try_acquire_owned` on
  the `AUTH_TASKS=16` semaphores (`:67`, `:123-124`).
  - The body is only `{"schema":"airk.local-api-error/1","error":{"code"}}`
    (`:1191-1205`).
  - Neither code is documented (`docs/local-https-api.md:288-298`).
  - No native test asserts that `authentication_busy` is emitted.
- **`http_fixture.py`.**
  - Bodies have only the token redacted (`:324`). A session ID echoed in a body is
    not redacted.
  - A body over 1 MiB raises a bare `ValueError` (`:313-315`) with no evidence.
- **`benchmark-service.py`.**
  - `HttpSession.request` (`:99`) does `assert "error" not in reply`. That discards
    the body of an HTTP 200 JSON-RPC error.
  - The refusal table at `:345-347` is local to this wrapper.
- **`benchmark-mcp-investigation.py`.** It turns `HttpRefused` into
  `RuntimeError("http_refused")` (`:500-507`). That drops the headers, the request
  identity and the redaction flags, and it does no classification.

**Required design.**

1. **Document both codes as pre-admission refusals**, in the error table of
   `docs/local-https-api.md`: no work was submitted and no session or credential
   state changed. Add a native test that saturates `AUTH_TASKS` and asserts the
   exact `authentication_busy` body. Change no limits and no response fields.
2. **One shared refusal classifier in `http_fixture.py`.** Move the table from
   `benchmark-service.py:345-347` there, and make both wrappers use it.
   - Count only the documented `(status, code)` pairs as refusals. Everything else
     is a failure: unknown, fenced, credential and uncertain outcomes.
   - Never retry an uncertain exchange.
3. **Keep JSON-RPC errors as evidence.** Replace the `assert` at
   `benchmark-service.py:99` with a typed failure that keeps a bounded body and the
   request identity.
   - Redact session values in bodies as well as headers.
   - Replace the bare oversize `ValueError` with a typed error that carries a
     bounded prefix and the request identity.
   - `benchmark-mcp-investigation.py` keeps the headers, identity and redaction
     flags.
4. **Account for every phase.** Record succeeded, refused (by code), failed,
   uncertain and not executed.
   - A phase passes when it has no failed and no uncertain outcomes, every refusal
     is documented, and every model-owner, process-role, resource-release,
     readiness, source-preservation and parity assertion holds.
   - Refusals do not stop later phases.
   - Do not invent a success-count threshold. If the user wants one, the user sets
     it (section 7).

**T0 tests:** `test_http_fixture.py`, `test_benchmark_service_http.py`,
`test_benchmark_http_transport.py` and `test_mcp_investigation_benchmark.py`, plus
`cargo nextest run -p airk-cli --lib -E 'test(http_gateway::tests)'`.

**Done when:** T0 passes and the refusal/error matrix is written down. It covers:
code, status, phase, counted as, retry allowed, and cleanup.

### E. Code and Copilot trust

**Verified state.**

- In the r25 Code run, `extra_ca_environment_present` was true and Code's network
  settings had `systemCertificates: true`. Code's patched fetch ignored the extra
  CA and failed with `UNABLE_TO_VERIFY_LEAF_SIGNATURE`.
- This machine has no AIRK user-trust receipt (`user-trust-1.json`). **The
  documented supported path, per-user OS trust, has never been exercised.**
- `McpHttpServerDefinition` (`@types/vscode` 1.140) has no field for a CA. The
  manual `mcp.json` route has none either.
- The native trust implementation is `crates/airk-cli/src/http_endpoint_trust.rs`:
  install, inspect and remove, with a digest-checked receipt. Windows is at
  `:112` and `:434`, macOS at `:269-285`, and Linux is unsupported (`:417-422`).

**Required action.**

1. Before the run, fix the harness cleanup: `run-code-mcp-host.cjs` stopped the
   service without writing `multi/native-stop.json`. This is a harness fix, so
   check it with T0 only.
2. Run once, **with the user's authorization** (section 7):
   1. Install trust for the candidate's issuer through `runtime http-trust` (the
      command behind **AIRK: Trust Local HTTPS Issuer**).
   2. Run the recorded Code host command (`resume-actual-code-r25-r1.json`
      arguments) against the new candidate.
   3. Remove the owned trust. Keep both the install receipt and the removal
      receipt.
3. If it passes, Code is qualified on Windows debug only.
4. If it fails while OS trust is installed, collect the error chain and **stop**.
   Do not try combinations of settings. Report it to the user as a product
   decision.

The Mac login-keychain path needs a Code host with a GUI on Neo. Record it as
unqualified unless the user provides one.

## 4. Open §13 items the first draft did not cover

[§13 of the One AIRK plan](one-airk.md#13-verification-and-completion-checklist)
has 18 items. This table retains the revision-2 baseline classification; the
approved choices in section 7 and current position above govern remaining work.

| §13 item | State | Proposed handling for 1.8.14 (**the user decides**) |
| --- | --- | --- |
| 1. Baseline metrics with acceptance thresholds | Harness exists. No thresholds | User sets thresholds, or accepts "measured, no threshold" for this release |
| 4. Raw JSON API with Python, Node and PowerShell on all routes | Fixture level only | Run against the final Windows candidate in the T2 batch |
| 6. MiniLM under CPU contention, out-of-memory and FFI failure | Only a 384-dimension repeatability smoke test | Add component tests where they are cheap. Otherwise record as an unqualified limitation |
| 7. A held inference call | Subsequently passed on preserved r26: 1/1 held checkpoint case, 66 embedding calls (`completion-modern-checkpoint-r26-r3.json`) | Selected Windows debug coverage; final candidate qualification remains |
| 8. Two OS users and port squatting | Wrong-issuer and crossed-authority cases passed | Needs a second OS account. User provides one, or it is recorded as a limitation |
| 11. Full fault matrix (full disk, corrupt manifest, …) | Partial | List which cases have tests. User decides on the remainder |
| 13. Real predecessor migrations: Mac, universal launchers, many clients | Windows passed for 1.3.1, 1.0.0-build.7 and 1.8.12 | Mac in the Neo batch. Universal launchers and many clients are a user decision |
| 14. Lifecycle on all four targets, with Developer ID and notarized Mac | Windows debug only | **Needs hosted CI and a merge**, which are authorization items |

Until the user has decided an item, do not start work on it, and do not report it
as qualified.

## 5. Agents, ownership and the review gate

The roles are a primary integrator, four repair owners and one reviewer. The
repair owners have separate files: A owns `packages/vscode`; B owns
`runtime_upgrade_stop*`; C owns the container files and scripts; D owns the HTTP
scripts, `http_gateway_tests.rs` and the API document.

- B and D both compile `airk-cli`. **The primary is the only Cargo owner**, and it
  queues the Cargo runs that repair owners request.
- The root-cause work in section 3 is done. Repair owners begin implementing from
  it. They do not repeat the investigation.
- **One reviewer who wrote none of the changes** reads the combined diff and the
  T0/T1 evidence. The reviewer checks for:
  - missing invalidation;
  - widened authority;
  - lost results;
  - wrong error classification;
  - an incomplete input graph;
  - partial recovery.

  The primary also reviews the diff on its own. Nobody approves their own work as
  the independent review.

Every fence decision in A must match this table:

| Operation | Fenced by |
| --- | --- |
| Setup, grant, context, source, policy, retirement, split, page/detail/preview/create/apply/reconcile, set read | Management: catalog owner, `sequence`, `generation`, native `valid()` |
| `scope.apply` / `set.save` | Management, with the owner rechecked after `work()` (today it is checked at entry only, `indexingCatalogController.ts:160-172`) |
| `manage.action` / `manage.command` | Management. Not the epoch |
| Advice: read, store, clear, dismiss, copy, use | The advice sequence, plus the epoch and the owner |
| Status, Usage, search, strategy, studio, planner, recipes, handoff | The evidence epoch |
| UI-local messages (`notice.dismiss`, `details.copy`, `workbench.toggle`, `extension.reload`) | None beyond `authorized()` |
| Reconnection and disposal | Context observation, `sync()` invalidation, `disposed` |

**Gate.** T2 work starts only when all of these hold:

- every workstream meets its "Done when";
- the reviewer has no unresolved blocking findings;
- the input freeze is recorded;
- the user has answered section 7.

## 6. The validation ladder (in order, each step once)

**Live-review additions, 8 November 2026.** Cheap source/T0 review found missing
prerequisites in the original commands: the positive legacy editor fixture
assumed retained launchers despite schema-2 retirement; installed endpoint
coverage observed definitions but not owned saved configurations; installed
Workbench exports were not exercised; the final ladder omitted issuer rotation.
The fixture/helper corrections receive the same T0/T1 and independent review
gate. They add observations to the existing stages below without changing native
limits, acceptance thresholds, frozen decisions or publication authorization.
The required held accepted job is a Vectors job: source Index is source-only.

Each step has a declared command, expected result and stop condition (rule 0.2-2).
A step that fails sends the work back to the component under rule 0.2-6.

1. **T1 integrated pass on Windows.**
   - `cargo fmt --all --check`
   - `python scripts/dev-check.py --workspace --all-features` (about 75 s)
   - `npm test --prefix packages/vscode`
   - `python -m unittest discover -s scripts/tests -v`
   - `git diff --check`
2. **Preserve the frozen Windows debug candidate r28** (runtime and VSIX are
   now built). Preserve r26 and the superseded r27 build/package evidence
   unchanged; neither is final qualification of these production corrections.
   Record the native, extension, harness and fixture identities separately.
   Before the longer installed campaigns, run the reviewed explicit native
   `accepted_vectors_finish_later_batch_before_native_checkpoint_seals`
   preflight against that matched candidate. Its cheap fixture-planning test and
   compilation must already pass; retain the exact selected count and candidate
   source seal. It uses the native provider and real worker, not a legacy helper.
3. **Windows T2 batch on r28**, in this order. Stop at the first failure and
   collect its evidence.
   1. Enabled-native extension suite (about 5 min).
   2. Installed-editor journeys, all scenarios (`scripts/check-native-editor.cjs`,
      about 4 min). `different` must now pass.
      Prepare the reviewed matched legacy fixture with
      `scripts/prepare-native-editor-legacy.py` first, and set
      `AIRK_EDITOR_LEGACY_FIXTURE` to its retained receipt. The six-scenario
      harness rejects missing input before starting and includes actual
      Workbench result/export and owned-config endpoint refresh observations.
   3. Client campaign `benchmark-service.py --clients 1 10 64 --projects 1
      --rounds 1 --semantic` (about 6.5 min). Run all three cohorts on r28 so the
      evidence comes from one identity.
   4. Raw JSON API clients (§13 item 4).
      Use `scripts/check-raw-https-clients.py` for the three real local clients,
      complete schemas and selected full-result parity across an ordinary
      occupied-port restart. Run the existing `scripts/check-issuer-rotation.py`
      against the same candidate separately; its private client trust never
      changes OS trust and does not substitute for the Code host enrollment.
   5. The actual Code host with authorized OS trust (E). Then the Claude check, but
      only if its inputs changed.
4. **One Neo batch.** Bring every result back, then run `cargo clean`, even if the
   batch failed.
   1. Scoped Clippy for `airk-cli` (about 150 s). **If it fails, stop the batch.**
   2. Mac unit tests for `upgrade_stop` and `retirement`.
   3. The rest of the batch as recorded in r13, including the actual uninstall and
      retirement.
5. **One Docker qualification build:** `docker build --target test --build-arg
   AIRK_QUALIFICATION=true .`, then the image smoke with
   `scripts/check-container.py`. Record the time of the preparation stage, to
   confirm the extraction fix.
6. **Final Windows T3 on the frozen source:**
   - full `cargo test --workspace --all-features --locked` (or the `nextest`
     equivalent with `--ignore-default-filter`), which restores the four routine
     excluded campaigns;
   - doctests;
   - lightweight and inference configurations;
   - `check-service-clients.py --model`;
   - `check-repo.ps1`;
   - license notices;
   - dependency audit;
   - `scripts/ci-local.ps1`, which CONTRIBUTING requires before a PR that touches
     native service, MCP or installer code;
   - the optimized build and its packaging.

   Reuse an earlier result only when every production input and artifact seal is
   unchanged. Never relabel a reused build as a new one.
7. **Documentation and evidence review.**
   - Reconcile README, the documentation index, release notes, Marketplace text,
     client classifications and the ledger against what was actually qualified.
   - Fix `docs/agent-integration.md:53`.
   - Keep AGENTS.md and CLAUDE.md byte-identical.

Record the command, scope, target directory, incremental setting, elapsed time,
result and `--timings` path for every Cargo run.

Keep the persistent target `artifacts/v190/p10-target`. Do not `cargo clean` on
Windows.

## 7. Decisions needed from the user (ask once, before section 6)

1. **OS trust for the Code run.** May the agent install the candidate issuer into
   Windows `CurrentUser\Root` for one Code host run, and then remove it? (Section
   3E.) Is a Code host with a GUI on Neo available for the macOS keychain path?
2. **The 64-client acceptance.** Confirm that the rule in section 3D-4 is enough
   (zero failures, zero uncertain outcomes, documented refusals only). Or set a
   minimum success count.
3. **The open §13 items in section 4.** For each one: required for 1.8.14, or a
   recorded limitation.
4. **Hosted CI.** Linux x64, Apple Silicon and container CI run on a PR. macOS
   Intel and the notarized artifacts come only after a merge. Committing, pushing
   and opening a PR each need separate authorization. Until then, completion means
   local completion (section 8).
5. **A second OS account** for the two-user matrix, or acceptance that it is a
   recorded limitation.

## 8. Completion and handoff

There are two finish lines. Report the one actually reached.

**Local completion** (the agent can reach this without further authorization
beyond section 7):

- workstreams A–E corrected and reviewed;
- section 6, steps 1–7 passed on Windows, Neo and Docker;
- every §13 item either qualified, decided by the user as a limitation, or
  explicitly open.

**Release completion** additionally requires:

- the authorized commit, push and PR;
- green hosted PR CI;
- after merge: macOS Intel and the notarized Mac artifacts;
- verification of the published outputs.

Neither line is implied by a green unit selection, a debug VSIX or one host.

The handoff lists:

- what changed and why the whole affected behavior is now correct;
- the reviewer, the review's scope and any findings still open;
- the exact identities of source, artifacts and tests;
- passed, failed, skipped and unrun checks, and which builds are original and
  which are reused;
- actual client and OS support, with any remaining trust or authorization limits;
- failed fixtures kept, and any cleanup still unqualified;
- the verified local artifacts ready for the user to review.

If commits are authorized later: keep version 1.8.14, use only the human
contributor's DCO `Signed-off-by` trailer, and add no agent trailers.

**Next action after approval:** reconcile retained receipts, implement the
workstreams, and complete their T0/T1 and independent review gates before freezing
inputs for section 6. Record observed results in the implementation ledger.
