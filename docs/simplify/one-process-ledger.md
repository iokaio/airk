# One resident process: task ledger

## Service/extension split planning (10 November 2026)

- Request: create `feat/split` from the current `feat/simplify` HEAD and write
  `docs/split/split-plan.md`; preserve the source branch's features.
- Acceptance: branch starts at `59dd3c721ca4c8301664827abaa9ea2ac6fd8dc8`;
  the plan assigns signed standalone service installation to downloads from
  airk.ioka.io/GitHub Releases and limits the extension to Explorer/Workbench,
  verified discovery and missing-service guidance. Cover feature preservation,
  predecessor handling, packaging, compatibility, signing and validation.
- Boundary: planning documentation and this ledger only; no implementation,
  installer repair, build, version bump, commit, push or publication.
- Review checkpoint: at most 15 minutes. AIRK MCP is absent from available
  tools; `.codex/config.toml` disables it. Use live local source inspection.
- Source-review findings: standalone installer source already exists; extension
  source still bundles and installs native payloads. Current version is 1.8.23;
  the stopped HTTP-cleanup correction has no rebuilt/qualified installer.
- Required blocker for eventual delivery: installed transition and cleanup
  proof. This planning task does not resume that stopped repair. A universal
  VSIX and independent product versioning remain proposals to assess.
- User clarification: Linux signing is optional. The plan retains Windows/macOS
  signing requirements and verified, explicitly unsigned Linux delivery.
- Intended validation: review plan against live installation/extension source
  and contracts; check local Markdown links; run `pwsh -NoProfile -File
  scripts/check-repo.ps1`, `git diff --check`, verify branch and source identity.
  No reproduction, compiler/test campaign or installed qualification is claimed.
- Results: `feat/split` and `feat/simplify` both resolve to the acceptance SHA.
  Plan completed and reviewed by the primary agent and a read-only service
  reviewer; no review blockers. Repository checks passed for 2,230 public files,
  including local Markdown links and the production self-spawn roster. The
  tracked diff passed whitespace checks; the new untracked plan needed a trailing
  blank line removed before its final whitespace check.
- Environment: sandbox process is `TYAI\codexsandboxoffline`; checkout, `.git`,
  new branch reference, plan and ledger ownership are `TYAI\tdotn`. Git commands
  use command/process-scoped `safe.directory`, without persistent configuration
  or ownership changes. Unrelated Git ignore/cache read-permission warnings
  remain environmental limits; repository checks completed successfully.
- Status: documentation-only task complete; final whitespace verification passed,
  including the Linux-signing clarification.
  No builds, Rust formatting/Clippy/workspace tests or installed qualification
  were run; no executable inputs changed.

### Split plan revision 2 (10 November 2026)

- Request: review, improve and expand `docs/split/split-plan.md`. Add a clear
  extension/service boundary, HTTPS admin pages on the MCP endpoint, a separate
  signed `airkctl` CLI over the local pipe with a service-only `airk`, code and
  installer maps, an extension that leaves old installs alone and handles MCP
  entries, and a service installer that removes old AIRK.
- Boundary: the plan and this ledger only. No implementation, build, test
  campaign, version bump, commit or publication.
- AIRK MCP: configured in `.mcp.json` but the server closed its connection
  (`CONNECTION_CLOSED`). Local tools were used.
- Source-review findings (not reproductions): three read-only investigations
  mapped native process/IPC/HTTPS, the extension, and installers/release. The
  primary agent spot-checked the gateway `Origin` refusal, the
  `service_already_running` refusal, client/service version equality, pipe
  naming, the single-executable rule, `REPLACEMENT_THROUGH`, the window
  session spawn, the HTTPS evidence client and the process-topology roster.
- **User decisions** (answers given in this session; plan §2.1 U3–U17):
  management is admin-only; `airkctl` owns agent configuration writes; one
  `airkctl session` per window; Explorer Assistance routed through that
  session; fixed-layout installer with a CI-signed setup step; receipts-only
  removal that continues and reports; admin search and evidence in the first
  release with its own index selection; one service per user.
- **Agent proposals awaiting the user** (plan §2.2 O1–O12): two new crates,
  the D8 contract changes, the HTTPS boundary change, state-contract scope,
  later-install index policy, MCP entries without a matching index, version,
  whether the stopped repair is superseded, uninstall index policy, the home
  of the setup step, the crate rename and the universal VSIX.
- Validation: local Markdown links and repository checks through
  `scripts/check-repo.ps1`; whitespace check of the tracked diff and of the
  untracked plan.
- Results: `pwsh -NoProfile -File scripts/check-repo.ps1` passed for 2,230
  public files, including the self-spawn roster. `git diff --check` passed. The
  untracked plan was checked separately: every local Markdown link and every
  source path it names resolves, with no trailing whitespace, tabs or carriage
  returns. No build, compiler, test or installed run was made, and no
  executable input changed.
- Status: documentation-only task complete. Implementation waits for the
  user's review of the plan and the open decisions.

## Reported HTTP cleanup refusal (10 November 2026)

Acceptance: correct the reported `HTTP cleanup requires the exact prebaseline
predecessor or its retained reset snapshot` refusal for replacement of an
initialized modern installation. Preserve exact selected/predecessor identity,
disposal-only authority, unavailable-metadata reporting and read-only lookup.
The repair boundary is native HTTP disposal lookup and its existing regressions;
no installed-editor journey, live installation mutation or publication is planned.
Initial review checkpoint: 15 minutes.

Observed: the current editor AIRK log
`20261110T144528/window1/exthost/output_logging_20261110T221444/5-AIRK.log`
records the exact automatic-installation failure with elapsed 90 ms. Installed
runtime inspection is unavailable to this sandbox (access denied); its exact
version and authority have not been established. AIRK MCP tools are absent and
the workspace Codex declaration is disabled. Local source navigation is used.

Source-review finding: `captureReplacementHostArtifacts` queries disposal proof
for the selected installed runtime before native replacement. Native lookup
accepts a retained exact reset snapshot, otherwise requires both selected
predecessor equality and `state_contract::required`. The latter is false for an
initialized modern runtime, so this caller composition is rejected despite
having exact current predecessor identity. This is a source-supported cause,
not yet a reproduced defect. Missing snapshots must not be inferred as deleted.

Validation plan: batch modern active/expired/revoked ownership, crossed identity
and unavailable-ledger regressions in the existing native test owner; run one
scoped diagnostic batch before production changes. Review the complete minimal
correction, then format, run scoped CLI Clippy with Cargo timings and the affected
native regressions using persistent `artifacts/v190/p10-target`, incremental on.
Record selected counts and receipts. Full workspace, platform, runtime/VSIX and
installed acceptance remain separate unrun gates unless explicitly undertaken.

First diagnostic attempt (`artifacts/one-process/http-cleanup-diagnostic-r1.json`)
did not execute tests: all three link targets lacked the static ORT handoff
(`LNK1181`, placeholder ORT library). Inputs were unchanged; elapsed 31.71 s,
incremental on, persistent P10 target and Cargo timing retained. This is a
validation-environment failure, not a reproduction. The existing runner's
`--static` option validates and applies the retained static-library handoff;
the unchanged diagnostic selection will use that option before any correction.

**Stopped at the user's direction:** apply only the small fix, commit and push;
do not test the fix, build another installer or create a PR. The diagnostic r2
was interrupted: it selected six tests and recorded three expected old-guard
failures and two passing refusal cases before interruption, with no final batch
receipt. This confirms the analyzed native failure class only; it does not
qualify the correction or the user's installed runtime. The provisional test
additions were removed. No test-owned AIRK/Cargo processes remained in the
post-stop process observation; the user's installed service/window session were
left untouched.

The final correction accepts exact selected predecessor equality and separately
retains `state_contract::required(root)?` as saved-state identity validation.
Its Boolean result no longer rejects disposal-only proof for initialized modern
state. All snapshot, endpoint, private-ledger and current-namespace checks remain.
No schema, persisted format or response field changed. No post-fix test, build,
runtime/VSIX package or installed qualification was run. Commit scope is this
small correction and its service-installation documentation; unrelated existing
working-tree changes and the earlier staged release-record rename were preserved
by that first commit (`9d332e7c`).

The user then explicitly requested bundling **all** pending work into
`feat/simplify` and removing the linked Git worktrees. The main checkout stays.
The current 1.8.23 source, extension, scripts, fixtures and documentation,
including the previously authorized Arbiter future concept, are checkpointed
without more tests or builds. Four dirty historical worktrees are preserved as
signed-off source snapshots in branch ancestry while the latest integrated tree
remains authoritative. Clean diagnostic branches retain their original history;
obsolete diagnostic tracing is not restored as production code. Ignored receipts
and failed/private fixtures move to ignored local worktree-retention storage
before worktree removal; none is staged or published. No PR is authorized.

Push destination evidence: origin is `https://github.com/iokaio/airk.git`,
matching Cargo and extension repository metadata. Authenticated GitHub account
`tylerje` reports `ADMIN` permission on the public `iokaio/airk` repository.
This read-only check followed an automatic approval rejection of the initial push
for missing destination-trust evidence.

Historical source preservation (no historical source copied over the integrated
1.8.23 tree):

| Retiring worktree | Signed-off source snapshot | Source paths |
| --- | --- | --- |
| `artifacts/v190/p10-worktree` | `2240086ce2e9e749bbd88b57aacfce7d03a04c14` | 59 |
| `artifacts/v190/p6e-worktree` | `af79e434f25ca93e24ab1735a17c16b0dccb1688` | 13 |
| `artifacts/v190/p6f-worktree` | `22c32f3e501338544c1ec45dd169302cd5885079` | 117 |
| `artifacts/worktrees/one-process-workers` | `d2bf99ca3d99f1e7c23b5d842a8e20a0695611f8` | 28 |

The preservation merge records all four snapshots as parents while retaining
the current integrated source tree. Local raw-byte archives and digest manifests
also retain CRLF-only/stat-only working-copy differences under ignored
`artifacts/worktree-retention/2026-11-10/`. The three clean cold-status worktrees
have no uncommitted work; existing diagnostic branch refs remain intact.
Worktree removal and ignored-evidence relocation receipts are collected there
after source preservation. No build or test is part of this housekeeping.

Removal completed: all seven linked worktrees were removed; only
`D:/code/Github/airk` on `feat/simplify` remains registered. Receipt:
`artifacts/worktree-retention/2026-11-10/retirement.json`. The entire ignored
artifact trees from P10 and one-process-workers, plus their retained Nextest
receipts and P6f's Nextest receipts, were moved locally before removal.
Source archive/digest manifests remain alongside them. Existing diagnostic and
archive branch refs were preserved. No private state, index or artifact was
staged; no test, build, installed repair or PR followed the stop request.

## Manual-test installer correction (10 November 2026)

**Windows debug manual-test installer built; installed-editor acceptance pending.** The user
reported broken agent reconnection, a pre-install PID 1080, an unexpected index
and growing service memory, then authorized fixing the installer and removing
automatic index generation. Installations through **1.9.0 inclusive** must
discard predecessor indexes and wait for explicit index setup. Source folders
remain read-only. Ordinary window activation is not another installation.


### Bounded Windows installer handoff (10 November 2026)

Final VSIX: `artifacts/one-process/bounded-extension-1823-final/publish/airk-1.8.23-win32-x64.vsix`
(86,525,906 bytes). SHA-256:
`9913a08e1fc6323b7dff01b6a26e8857e87f0cddd59e6f8bbeaa234690329b89`.
Final package receipt: `artifacts/one-process/installer-bounded-final-package-r1.json`;
exit zero, unchanged consumed inputs, 21.33 seconds. Only the package README
status changed after the native UI run; no runtime or JavaScript rebuild occurred.

The user set a 30-minute delivery limit. The delivered candidate uses the debug
runtime, not an optimized release runtime. No production installation was changed.
The native ZIP and VSIX package checks passed with unchanged consumed inputs:
`artifacts/one-process/installer-bounded-build-r1.json`. Cargo used the persistent
`artifacts/v190/p10-target`, incremental compilation enabled, with timing receipts;
the final debug runtime compilation took 24.62 seconds. The build sequence also
passed the one-test ordinary-reopen regression and one-test generated CLI reference.

Validation before packaging: scoped native Clippy passed; the native batch selected
54 library tests (53 passed; the remaining ordinary-reopen defect was corrected and
its one-test rerun passed), and both actual CLI lifecycle tests passed. The full
mocked extension batch selected 1,425 tests: 1,404 passed, zero failed, 21 gated skips.
Receipts: `installer-bounded-native-validation-r1.json`,
`installer-bounded-native-tests-r1.json`, `installer-bounded-extension-tests-r1.json`.
The final packaged-runtime native UI selection passed all four tests, with zero
failures or skips, in 73.84 seconds. Unchanged source inputs and runtime seals
are recorded in `installer-bounded-native-ui-r1.json`. Only the README status
changed for final VSIX packaging; the tested runtime and extension JavaScript
were reused unchanged. Process inspection after the tests found only the user's
pre-existing PIDs 1080 and 10188; no test AIRK, Cargo or rustc processes remained.

Full current-source workspace tests, optimized builds, installed-editor acceptance,
macOS/Linux, signing and release publication were not completed for this handoff.
The pre-existing structural boundary failure and earlier failed receipts remain
recorded below; they have not been waived or relabeled as passing. This is an
unpublished Windows manual-test candidate. AGENTS.md and CLAUDE.md now contain the
same bounded investigation, scope and stopping rules and remain byte-identical.

Original observed evidence (collected before tests or production changes):

- Windows process metadata identifies PID 1080 as the 1.8.22 bundle's
  `airk service session`, started at 17:20:29 by VS Code PID 16828 (started
  17:20:18). PID 10188 is the installed `runtime managed-serve`, started
  17:20:58. The session predates the runtime installation and executes native
  installer commands; it is not evidence of an old service surviving cleanup.
- The current editor log,
  `%APPDATA%/Code/logs/20261110T144528/window1/exthost/output_logging_20261110T172019/13-AIRK.log`,
  records removal of 1.8.20, then no registered index (line 11), then
  `Reconnect created index` for this repository (line 23), followed by both
  `reconnect_scope_unconfirmed` failures (lines 24–25). The visible index came
  from Reconnect, not a retained testing index.
- Installed `out/serviceExtension.js` and live source both require
  `catalogSession` in `reconnectScope`. The real Search scope path produces
  `combinedSession`, including singleton selections. Creation precedes that
  refusal and is retained. Existing tests mock away this composition.
- Replacement's same-runtime exemption preserves state during an explicit
  reinstall; the version gate also permits ordinary upgrades at exactly 1.9.0.
  Removing the exemption without distinguishing installation from reconnection
  would incorrectly wipe state on every window activation.
- Replacement captures existing executable files only, missing stale entries
  whose launcher was already removed. It omits custom registered data roots.
  Process-stop errors can disappear if subsequent directory renames succeed.
  These are source-review findings, not reproductions of this installed run.
- A genuinely empty service does not eagerly load inference. Index creation
  and resumed native work can explain memory activity; no memory leak has been
  established by these observations.

Correction invariants and ownership:

- Native installer owner: explicit reinstall admission through 1.9.0;
  authenticated interrupted-install resumption; complete owned data inventory;
  verified predecessor shutdown before removal; historical launcher identities.
- Extension reconnect owner: no index setup from Reconnect; accept reviewed
  singleton/combined scopes without broadening authority; preserve full failure
  information and unrelated configuration bytes.
- Primary: installation/reconnection intent at extension call sites, source and
  documentation integration, independent review of delegated changes, and sole
  ownership of compilation/testing. No commit, push or live installation change
  is requested. A new package must use the next unused patch under the standing
  version instruction.

Validation plan: add sibling regressions together, run one scoped diagnostic
batch before production corrections, complete/review the combined correction,
then freeze inputs for scoped Clippy with timings and affected native/extension
checks. Record selected counts and receipts. Final repository/workspace/package
gates remain required; no earlier installed-editor failure is relabeled as a
pass and no third expensive attempt occurs without resolving its missing cheap
seam first. Preserve existing `airk-state/1` schemas and owned-source protections.

Diagnostic receipts collected before production correction:

- `artifacts/one-process/installer-empty-diagnostic-r1.json`: no tests selected;
  MSVC could not create a temporary linker file under the retained owner-private
  fixture directory with the restricted token. This is a host access failure.
- `installer-empty-diagnostic-r2.json`: **19 selected, 13 passed, 6 failed**;
  all six intended native defects reproduced. Command:
  `cargo test -p airk-cli --no-default-features --features local-inference --locked --timings --lib runtime_install::replacement::tests:: -- --test-threads=1`.
  Persistent target `artifacts/v190/p10-target`, incremental `1`, elapsed
  100.66 seconds, native inputs unchanged. Timing evidence:
  `installer-empty-diagnostic-r2-cargo-timing.html` beside the receipt.
- `installer-empty-ts-diagnostic-r2.json`: **15 selected, 6 passed, 9 failed**;
  unwanted Reconnect index creation, missing installation-operation forwarding,
  missing generation and replay behavior reproduced. Two combined-scope tests
  instead exposed incomplete synthetic fixtures; those failures do not reproduce
  the product defect. Initial restricted-token fixture failures are retained in
  `installer-empty-ts-diagnostic-r1.json`.
- `installer-empty-scope-diagnostic-r3.json`: **3 selected, 0 passed**. The
  no-index regression reproduced the side effect; the other two still stopped
  at synthetic host binding/context preconditions. A complete fixture state-flow
  review is required before another scope diagnostic, rather than production
  correction based on those precondition failures. No native rebuild is needed
  for a fixture-only change.

Latest user constraint: a same-build installation must complete a full uninstall
first. Reconnection is not reinstallation. Explicit retry retains its operation
identity until setup completes; concurrent windows share a native idempotent
installation operation. Neither Reconnect nor installer startup may create an
index. Explicit Index library setup remains the indexing admission path.

- `installer-empty-scope-diagnostic-r4.json`: **2 selected, 0 passed**;
  the direct reviewed-combined-adapter composition reproduced the scope refusal,
  and the no-index case reproduced index setup mutations. Production Reconnect
  correction followed this reproduction.
- `installer-empty-extension-compile-r1.log`: complete pinned Node 24.15.0
  extension, both webview/assistance typechecks and Explorer bundle compilation
  passed for the coherent extension correction.
- `installer-empty-extension-scoped-r1.json`: **224 selected, 221 passed,
  2 failed, 1 skipped**, 24.88 seconds, frozen extension inputs. Actual scope
  Apply composition, no-index behavior, loader generation, operation forwarding,
  fresh-install generation admission and cleanup resumption passed. The replay
  race fixture retained an old version after simulating completed replacement,
  triggering its unrelated legacy-rebuild mock. An earlier expectation also
  incorrectly treated a second explicit same-build install as a reconnect.
  Both need one reviewed fixture/policy-expectation correction before repeating
  the affected extension batch. No native artifact is consumed by these changes.
- `installer-empty-extension-test-compile-r2.log`: pinned host TypeScript
  compilation passed after the fixture/policy expectation correction; no native
  rebuild and no repeated webview compilation.
- `installer-empty-extension-scoped-r2.json`: **224 selected, 223 passed,
  1 skipped, 0 failed**, 25.02 seconds, extension inputs unchanged.
- `installer-empty-extension-full-r1.json`: **1,400 selected, 1,379 passed,
  21 skipped, 0 failed**, 27.79 seconds, extension inputs unchanged. This is the
  mocked extension suite, not installed/native transport qualification.

Independent native source review is in progress before compilation. It found
additional correction obligations: keep default roots within their owning
channel, defer destructive inventory on ordinary reconnect, preserve consumed
generation identity across standalone reinstalls, protect new successors from
pending cleanup, and allow exact retired-directory cleanup after partial
deletion removes an owner marker. All are source-review findings until their
new regressions run. The existing installer JSON writer already bounds the full
receipt to 512 KiB; the initially suspected unbounded journal write was ruled
out by reading that canonical writer.

Native implementation and primary source review are complete; inputs are now
frozen for the first correction validation. Regressions include the six original
reproductions, replay/generation continuity, no-clobber retirement, partial-delete
recovery, original-control reauthentication, missing registered storage, channel
isolation and actual CLI full replacement/reactivation with an empty successor.
Public destructive replacement requires an operation identity. The new journal
is installation metadata outside removed roots; existing `airk-state/1` formats
are unchanged. Unsupported unproven legacy layouts and older timestamp-named
residue are not silently treated as proven deletion targets.

Next ordered validation: formatting, scoped `airk-cli` Clippy with local-inference
and timings in `artifacts/v190/p10-target` (Windows incremental on), then one
batched affected native selection including actual CLI transport. Expected:
nonzero selected tests, no failures, empty successor catalog and no indexing jobs,
original source bytes preserved. Stop at compiler/check failure and collect the
complete diagnostics before another correction. No optimized build starts until
these checks pass and the combined review is complete.

- Independent final native review: all concrete findings dispositioned. The
  last missing-storage report was stale: live physical-plan construction already
  filters absent roots. A real native regression was added for that boundary.
- `installer-empty-clippy-r1.json`: **failed**, 81.74 seconds, inputs unchanged;
  complete diagnostics identify two `collapsible_if` style errors in the new
  launcher discovery paths, and no other errors. The primary collected both lib
  and lib-test diagnostics, corrected both sites together using let chains,
  reviewed unchanged evaluation order and formatted the owning file. No tests
  were selected because Clippy failed. Timings retained beside the receipt.
- `installer-empty-clippy-r2.json`: **passed**, 34.19 seconds, all-targets
  `airk-cli`, no-default-features + local-inference, Windows incremental `1`,
  persistent P10 target, frozen native/offline inputs. Timings retained.
- `installer-empty-native-scoped-r1.json`: **40 library tests selected,
  33 passed, 7 failed**, 64.04 seconds including compilation. Cargo stopped
  before the CLI integration binary; those transport tests were not run.
  All seven failures share Windows directory rename refusal while the installer
  itself holds write exclusions on child executable files. The full failure
  batch was collected before editing. No T2/T3 or installer build was started.
- Cheap Windows API matrix (no Cargo):
  `artifacts/private/installer-image-exclusion-matrix-r1/receipt.json` and
  `close-before-move-receipt.json`. No-handle directory moves pass; child file
  handles block parent moves regardless of sharing or replacement flags.
  Unlinking the exact owned image under its held exclusion makes the name
  unreachable and permits parent movement while the exclusion remains held.
  This agrees with Microsoft's
  [documented directory rename restriction](https://learn.microsoft.com/windows-hardware/drivers/ddi/ntifs/ns-ntifs-_file_rename_information).
  The correction must durably record qualified image retirement after frozen
  source/namespace proof, unlink under exclusion, then move/delete directories;
  resume and rollback disclosures must retain that irreversible phase.
- `installer-empty-legacy-loader-diagnostic-r1.json`: **3 selected, 0 passed**.
  Source review found that an already loaded 1.8.22 update loader cannot expose
  the new installation-generation Symbol to corrected hot-loaded code. The
  actual older-host path reproduced an undefined native operation. A bounded
  reader of the installed manifest must share the loader's validator and bind
  the exact product/version before deriving the same generation. Helper API and
  crossed-manifest expectations are new protective cases, not separate observed
  installation failures. No live installation/configuration was changed.
- `installer-empty-compat-extension-compile-r1.log`: full pinned extension and
  webview/assistance compilation passed with the shared bounded compatibility
  reader. `installer-empty-compat-extension-scoped-r1.json`: **229 selected,
  228 passed, 1 skipped, 0 failed**, 21.33 seconds. The real older-host path and
  captured module graph both pass. `installer-empty-compat-extension-full-r1.json`:
  **1,405 selected, 1,384 passed, 21 skipped, 0 failed**, 23.41 seconds. All
  extension inputs stayed frozen; native integration remains a separate gate.
- Additional native source review identified a trust-boundary gap in the newly
  introduced journal: a regular bounded JSON file in a shared custom parent is
  not private deletion authority. The correction uses an adjacent owner-private
  child validated through canonical `local_ipc` checks before each read/write;
  readonly planning never creates it and existing permissive children are refused
  without changing their permissions. This is a source-review finding awaiting
  its native negative regression, not an observed attack or live user-state change.

- `installer-empty-clippy-r3.json`: formatting and all-targets `airk-cli`
  Clippy with local-inference passed, 23.08 seconds, Windows incremental `1`,
  persistent P10 target; native and offline inputs unchanged, timings retained.
- `installer-empty-native-scoped-r2.json`: **44 library tests passed**,
  including private-journal refusal, held-image unlink ordering and interrupted
  recovery. Actual CLI transport selected **2 tests: 1 passed, 1 failed**.
  Total 49.39 seconds, frozen inputs. The failed transport fixture invented
  `runtime stop`, which is not a command; all three stop sites, including its
  result-discarding Drop, share that harness error. Production library evidence
  remains valid. The test-only correction must use canonical instance-fenced
  Lifecycle Stop and retain both owner/source fixtures before activation.
- The failed fixture `.tmpNfRH9B` left test-owned PID 16640 running after its
  destructor partially removed installation metadata. Exact process image,
  creation time, health/image digest and runtime identity were verified before
  recovery. Native `runtime inspect` failed because metadata was already absent,
  so no complete original Lifecycle Stop receipt is claimed. The exact service
  then exited after graceful `service shutdown`; the failed fixture and both
  `manual-cleanup-receipt.json` and `manual-graceful-cleanup.json` remain there.
  User processes 1080 and 10188 were preserved. This is a harness cleanup defect,
  not evidence of predecessor cleanup passing or failing in the new installer.

- `installer-empty-cli-validation-r3.json`: scoped formatting/Clippy passed;
  **2 CLI tests selected, 1 passed, 1 failed**, 65.85 seconds total. The full
  same-build removal, reactivation, new namespace and exact shutdown completed.
  Its final assertion then requested an invalid pending-project page of 64;
  the canonical registry API accepts 1–32. Failed owner/source fixtures remain
  retained after verified Stop (`.tmps0Z4HR`, `.tmpUwVtcu`). Primary audited the
  complete test and cleanup API calls, corrected the request to the supported
  maximum of 32 with the empty-result assertion unchanged, and reuses the
  completed Clippy evidence for this test-only literal correction. The native
  production artifact and 44 passing library cases are unchanged. The generated
  CLI-reference check was not reached by this stop-on-failure batch.

- `installer-empty-cli-validation-r4.json`: **2 actual CLI replacement tests
  passed**, 25.84 seconds of test execution. Full owned same-build removal,
  reactivation into a different empty namespace, no pending indexing projects,
  preserved source bytes and verified final Stop all passed. The following
  generated-reference test failed only on native Clap option ordering. Its
  canonical generator was run using that exact existing test executable; the
  retained `installer-cli-reference-generated.diff` changes option ordering
  only. A fresh readonly run of that same generated-reference test passed
  (**1 selected**) without recompiling or repeating installation tests.
- Standing candidate authorization applied: manifests and lockfiles changed
  **1.8.22 → 1.8.23** immediately before build preparation. Pinned Node 24.15.0
  host compilation, generated native catalog, complete extension compilation
  and `release-version.py check --expected 1.8.23` passed in 22.17 seconds;
  `installer-1823-prepare.log` retains exact commands. No earlier artifact was
  relabeled. Primary reviewed the complete native/extension correction and the
  five native-fixture failure-retention changes before final validation.

Final Windows qualification ladder (frozen inputs; primary is sole Cargo owner):

1. `installer-1823-final-t1-r1`: workspace all-targets/all-features Clippy via
   `scripts/dev-check.py`, separate lightweight CLI all-targets check, boundary
   and archive-document checks, full Python suite, repository/whitespace checks.
   Persistent target `artifacts/v190/p10-target`, Windows incremental `1`;
   exact native/harness/fixture/offline hashes and timings retained by runner.
2. Once T1 passes, matched development runtime:
   `python scripts/build-service-runtime.py --profile debug --destination artifacts/one-process/debug-runtime-1823`.
   Expect version/source seals, embedded model execution and static imports to
   pass. Native package plus extension `--stage-only` will provide a complete
   matched development stage for F09; actual adapter tests use isolated owners
   and retain failed fixtures. No normal-profile editor installation is run.
3. Actual native extension/client preflight, required workspace tests/doctests,
   then one optimized runtime:
   `python scripts/build-service-runtime.py --profile release --destination artifacts/one-process/final-runtime-1823`.
   Release incremental environment is unset. Reuse passed development evidence
   with its identity; the optimized payload still needs its own artifact/model
   and package verification. Exact commands and consumed identities are frozen
   in each runner receipt. Stop at the first failed gate and collect diagnostics
   before correction. Do not repeat the stopped installed-editor Help campaign.
4. Fresh native ZIP and Windows VSIX, verified at their artifact boundaries.
   No commit, push, live-profile install or publication is authorized. Report
   actual Windows evidence and preserve unqualified Linux/macOS/signing limits.

- `installer-1823-final-t1-r1.json`: workspace all-targets/all-features Clippy
  passed (83 seconds; `installer-1823-workspace-clippy-timing.html`), and the
  separate lightweight CLI all-targets check passed (76 seconds, one existing
  unused `Child` import warning in the non-inference configuration). The batch
  then stopped at the module-cycle guard. Module honesty and crate edges pass;
  the unreviewed SCC membership fails. Total 171.74 seconds; inputs unchanged.
  Independent review matched all 19 non-installer members byte-for-byte to the
  successful **1.8.22** `runtime-build-r2.json` source roster and reproduced the
  exact SCC after excluding every installer-repair-modified file from graph
  evidence. This is a retained pre-existing one-process boundary failure, not
  a passing check or an exemption. No baseline or production code was changed
  to suppress it. Repair behavior's scoped T0/T1 checks and both configurations
  pass; a local manual candidate is not full structural/release qualification.
- `installer-1823-repository-checks-r1.json`: remaining independent checks pass,
  118.07 seconds, frozen inputs: Python **932 selected, 921 passed, 11 skipped**;
  archive-document check; repository checks over **2,229 public files**;
  whitespace; and strict-offline dependency checks. The latter are limited
  cached Rust evidence: npm/fresh advisories were not checked. No release-audit
  qualification is claimed. The pre-existing cycle check remains failed.

- `installer-1823-debug-runtime-r1.json`: **passed**, 147.11 seconds, native
  inputs unchanged; Cargo development compilation 103 seconds, incremental `1`,
  P10 target and timing HTML retained. Source-sealed **1.8.23** runtime at
  `artifacts/one-process/debug-runtime-1823`; embedded model/static-import and
  release-license inventory checks passed (431 dependency notice packages).
- Development native preflight command, reviewed before execution:
  `runner.py --groups native,harness,extension,offline-assets --static --runtime artifacts/one-process/debug-runtime-1823 --tier T2 --fixture-base artifacts/one-process/native-fixtures-private installer-1823-native-preflight-r1 python artifacts/one-process/installer-native-preflight.py`.
  The wrapper creates fresh canonical debug native/extension stages, then runs
  the full extension suite serially with the sealed current native binary and
  explicit F09 stage root. Expect all 18 current native-gated cases to run
  (including F09's additional Windows ACL child), no failures, and only the two
  POSIX cases plus unavailable historical predecessor-helper case to skip.
  Stage bytes are independently hashed before/after; failures retain fixtures.

- `installer-1823-native-preflight-r1.json`: **failed**, 634.81 seconds total,
  suite 607.86 seconds: **1,406 tests, 1,400 passed, 3 failed, 3 skipped**.
  Runtime, complete staged extension and consumed source hashes stayed unchanged.
  All three failures share one qualification-plumbing cause: the Assistance
  probe omits the explicitly injected stdio transport, so it refuses before
  native execution. This affects the independent Assistance test and F09's
  Windows ACL child (with parent failure propagation). The native UI/help-only
  forwarder remains supported; evidence MCP's production stdio refusal stays
  required. Exact affected call sites and existing transport fixtures were read
  before proposing a correction. Failed fixtures and Stop receipts are retained.
  All other current native cases passed, including setup without inference,
  actual scope orchestration, cold HTTPS and sequential/concurrent clients.
  Only two POSIX cases and the unavailable historical helper skipped. These
  passing cases retain their original debug artifact and test identities.
- **User-directed review pause (10 November, local evening):** no further
  expensive validation/build starts while the anticipatory review is incomplete.
  The preceding failures exposed avoidable harness assumptions (`runtime stop`,
  pending-project limit 64, missing explicit transport) that source/API review
  should have caught. Primary requested four read-only reviews covering native
  installation/process state, extension intent/index admission, all native-gated
  harness/API assumptions, and CLI cleanup assertions. Each review must name
  concrete invariants, consumed API/value rosters, interruption/recovery paths,
  ranked findings and the smallest cheap reproduction; no source edits or tests
  occur during collection. Batch sibling regressions and one coherent correction
  after findings, then review/freeze before validation. Existing passed native
  artifacts and unaffected results must be reused. The manual installer remains
  the authorized objective; no 1.8.23 VSIX or optimized runtime exists yet.

Anticipatory source review (four component reviews plus primary; no new runs):

| Finding | Evidence class and correction boundary | Cheap confirmation |
|---|---|---|
| Native replacement omits managed-policy gate | Concrete source finding: external provider image can lie outside removed roots; reuse canonical managed refusal before admission | Existing managed External/UserOnDemand/detach fixtures; no policy/state/provider/journal changes |
| Default names override another installation's ownership | Concrete source finding: `owned_by_airk` accepts reserved names even when authenticated owner binds another install | Inject default-candidate roster under a temporary owner, retain real native ownership reads and prove foreign state/data untouched |
| First-use defer occurs after persistent admission | Concrete source finding with user effect: valid generation leaves pending host/native operation; next candidate then refuses despite deferred install | Valid-generation Not now/dismissal: readonly discovery allowed, no operation Memento or native admission |
| Host retirement sweeps unrelated HTTP records/variables | Concrete source finding: global key/token-name regex ignores installation ownership | Two installations with distinct enrollment/trust records; replace one and compare other's complete preimages |
| Reconnect ledger uses discovery positions after grouped execution | Concrete source finding: interleaved roots assign B/C each other's outcomes | Three interleaved entries with distinct outcomes; compare destination/container/server identities |
| Trust bundle unlink failures count as completed retirement | Concrete source finding: catch/log followed by planned count and completion | Inject unlink failure; retain per-item retry evidence and report only actual completion |
| Explicit/automatic host operation can overwrite a shared slot | Source-supported race hypothesis, not yet reproduced | Gate two contexts' shared Memento access and interrupt native winner; preserve accepted operation for retry |
| Upgrade qualification uses obsolete replacement arguments | Concrete script finding: apply lacks required explicit operation; null clean-break evidence also passes weak assertion | Mock exact argv and require complete canonical snapshot/table evidence |
| CLI shutdown qualification stopped its predecessor first | Coverage gap, not runtime failure: it never exercised installer-driven shutdown | Owned selected-main foreground Child, restart before replacement, exact child exit; complete live empty observations before Stop and stable full checkpoint afterward |
| Native test failure retention misses cleanup errors/Assistance storage | Source findings plus retained r1 evidence: outside-owner storage removed; finally errors bypass body catch | Tiny existing-fixture cleanup seam with injected close errors; preserve original errors, Stop proof and storage |
| Polling accepts late success | Source finding, not reproduced; existing timeout-zero one-observation expectation is intentional | Existing mocked-clock seam; preserve timeout-zero expectation and full refusal details |

Two findings remain explicit limits, without an unreviewed policy rewrite:
the new journal safely refuses after 128 distinct consumed IDs (automatic VSIX
normally adds one, explicit operation with a new generation can add two), and
native uninstall does not retire that history. No silent pruning or budget
increase is authorized by a passing test. Session image handoff intentionally
allows accepted work to drain while a new image serves new work; an existing
test explicitly requires that overlap. JavaScript disposal is not process-exit
proof. Do not rewrite that assertion or claim a changed first-upgrade guarantee
from an already-loaded 1.8.22 module. Installer-owned predecessor exit receives
its own exact-child regression. Any later drain-policy change is a separate
decision supported by evidence, not an inferred fix.

The installed 1.8.22 manifest was read: its VS Code `installedTimestamp` is an
actual numeric metadata field, validating the old-loader fallback assumption on
this host. This observation does not qualify later real installed-host timing.
Tests-first work is now scoped to the findings above. All owners must finish and
freeze together before one cheap diagnostic batch; no expensive run or optimized
build starts during this work. New source-review findings remain distinct from
the original reproduced defects and from qualification results.

The new diagnostic inputs are frozen after all four owners' source reviews and
primary review. Formatting and pinned host TypeScript compilation passed before
the batch. Exact command:
`runner.py --groups native,harness,extension,fixtures,offline-assets --static --tier T0 --fixture-base artifacts/one-process/native-fixtures-private installer-review-diagnostic-r1 python artifacts/one-process/installer-review-diagnostic.py`.
This collects scoped Clippy, all named replacement library/CLI assertions, 13
named Node cases and the upgrade script's Python module in one diagnostic batch.
Known new semantic failures are expected; the capacity and timeout-zero sibling
cases should pass. Native assertions do not run if Clippy fails; independent
Node/Python diagnostics still collect. No source changes occur until the entire
batch has been collected and assessed, and no expensive qualification is run.

- `installer-review-diagnostic-r1.json`: **60.09 seconds**, all inputs frozen.
  Scoped Clippy found one shared test-fixture export warning repeated across
  other integration binaries; native assertions were correctly not run.
  Node **13 selected, 1 passed, 12 failed**: eleven semantic regressions confirmed
  the anticipated defects; the trust-unlink test instead hit TypeScript's
  getter-only module wrapper before its injection, so that case is not yet a
  reproduction. Python **47 tests**, 15 expected subtest failures expose the
  obsolete argv and weak empty-evidence checks. Entire diagnostics were collected
  before edits. No long campaign or native package rebuild ran.
- Reviewed harness corrections: a crate-local type alias replaces the unused
  reexport without lint suppression; unlink injection now targets the actual
  writable Node module with an explicit attempted-unlink precondition. Python
  source corrections follow the reproduced argv/evidence cases. Native production
  ownership checks and all other extension production behavior remain unfixed
  until their required focused semantic observations are collected.
  `installer-review-native-diagnostic-r2` reuses passed Node evidence and selects
  only the remaining native assertions, trust-unlink observation and corrected
  Python module. Its exact commands and frozen identities are retained by runner.

- `installer-review-native-diagnostic-r2.json`: **131.16 seconds**, frozen
  inputs. Scoped Clippy passed. Library **50 selected, 45 passed, 5 failed**:
  all four managed-policy/detach/origin cases reproduced actual fixture metadata
  removal while leaving provider bytes; these are confirmed ownership defects.
  The foreign-default case instead tried writing through `open_saved` during
  fixture setup, so it does not yet reproduce that source finding. Its canonical
  writable registry fixture is corrected under the existing service lease;
  `installer-review-foreign-observation-r3` selects only that remaining unit case.
  The capacity-bound case passed, retaining IDs and rejecting excess admission.
- Actual CLI **2 selected, 1 passed, 1 failed**: the strengthened test reached
  fresh live resource observations after proving exact running predecessor Child
  exit. Catalog, inference and source checks passed. Its invented zero policy
  reservation failed: the retained sample has zero admitted jobs/workers/threads,
  I/O and drains, RSS **36,253,696 bytes**, and a fixed cold policy reservation
  **1,375,731,712 bytes**. `Resources::reserved` explicitly includes model,
  service and watcher headroom with no running work. This is a wrong harness
  expectation, not proof of index admission or a memory leak. Source-reviewed
  correction captures an independent complete cold sample before setup and
  compares the post-reset snapshot to that golden, excluding only documented
  host telemetry (RSS, available memory, pressure, telemetry completeness).
  Every zero-work assertion and complete archived response remains required.
- The focused trust-unlink case reproduced the real lost-retry/false-completion
  defect after its injection precondition was fixed. Corrected upgrade-script
  Python module: **47 passed**, 0.473 seconds. Those results are retained rather
  than rerun for unrelated source work.

Coherent correction ownership now active: extension install/reconnect owner
fixes first-use ordering, scoped host records, per-entry ledger identity,
truthful resumable trust cleanup and positively reconciled per-operation host
records. Qualification/transport owner fixes explicit Assistance injection,
complete failed-cleanup retention and late-success budgets without changing
timeout-zero behavior. Native ownership correction waits only for the remaining
foreign-owner observation; actual managed mutation is already reproduced.
Primary remains sole build/test owner. No optimized runtime or new VSIX exists;
old debug artifacts retain their historical source seals and cannot be relabeled
as current after the native ownership changes.

## Previous candidate (10 November 2026, evening)

The user chose to test 1.8.22 by hand. The VSIX is `artifacts/one-process/final-extension-1822/publish/airk-1.8.22-win32-x64.vsix` (SHA-256 `01e342b3…`).

The installed-editor journeys stopped after two strikes; see the root-cause note below. A third harness attempt waits for the user's report.

Nothing is committed. The section below is the earlier pickup note and is now history.

## Resume here (paused 10 November 2026, VS Code restart)

**State of the tree.** Everything is uncommitted in the main checkout `D:\code\Github\airk` on branch `feat/simplify`: about 133 changed or untracked paths, including the earlier uncommitted 1.8.19/1.8.20 work. Nothing has been committed or pushed. No installer has been built. The manifests are still at **1.8.20**; the docs already say 1.8.21. No agents, builds or locks are running; `artifacts/one-process/*.lock` is clear.

- **Approved plan:** `C:\Users\tdotn\.claude\plans\that-was-a-terrible-luminous-puzzle.md`.
- **User decisions:** top of `docs/one-airk.md`.
- **This ledger** is the authoritative record.

**Done and validated (Windows):**

- A, B, C, D, E, F, G, J. The last green runs were `integ-compile-r2` and `integ-ext-full-r2` (1,382 selected, 0 fail), `review-tests-r1` (96 selected, 96 pass) and `owner-tests-r1`.
- The H patch is merged; `integ-clippy-r1` (cli/mcp/kernel/retrieval/ingest) is clean.
- The docs (step I) are drafted for 1.8.21.

**Interrupted:**

1. **`integ-tests-r1`** (combined native batch) was stopped at about 1,537 of 1,846 tests. It has no receipt and must be rerun after the workers corrections. Command, run from `artifacts/one-process`:

   `python runner.py --static --tier T1 --fixture-base native-fixtures-private integ-tests-r2 cargo nextest run -p airk-cli -p airk-kernel -p airk-ingest -p airk-retrieval -p airk-mcp --no-default-features --features local-inference --locked --profile ci --no-fail-fast --lib --test service_runtime --test derived_native --test unregister_native --test pdf_worker --test native_worker_storage --test integrity_restart --test helper_authority_roles --test cli_commands --test tools --test mcp --test combined_reads --test native_read_retry --test helper_outage_reads --test usage_native --test configuration_native --test native_owner_recovery`

   Cargo is at `C:\Users\tdotn\.cargo\bin`. The target directory is `artifacts\v190\p10-target`.
2. **Workers review corrections were not started.** The agent was stopped while still reading; it made no edits. Its full brief is the 13-item list under "Workers patch review" below. Restart that work from that list:
   - quarantine replaces every in-process `exit(75/76)`;
   - drain coverage before `install` and in startup, activation and reads;
   - the derived-worker Drop guard;
   - quarantine for the stall watchdog;
   - PDF slot classification, busy deferred and interrupted treated as drain;
   - usage spans treated as overlapped when another owner is active;
   - poisoned-cache recovery and per-root cache release;
   - a non-blocking `ChildWorker::drop`;
   - Unix PDF parent-death exit;
   - bool-checkpoint audit;
   - watcher stale pulse handled by quarantine;
   - `check-self-spawn.py` false negatives;
   - a production in-process start test.

   Only these crate files are in scope: `project_worker.rs`, `derived_worker.rs`, `project_supervisor.rs`, `source_supervisor.rs`, `worker_host.rs`, `worker_scope.rs`, `airk-kernel/src/{service_execution,resources}.rs`, `airk-ingest/src/{pdf_slot,pdf_worker,extraction_pool,extraction_jobs}.rs`, the caches (`source_policy.rs`, `current/lexical/cache.rs`, `verified_cache.rs`), `split.rs`, and `scripts/check-self-spawn.py`.

**Then, in order:**

3. Fill in the docs placeholders that the docs agent marked "see the ledger": the stuck-owner status field names and reason, the drain period, the PDF wait and retry bounds, and whether circuit-open is reported separately. They are in `docs/process-model.md` and `docs/releases/release-1.8.21.md`. Also fix `docs/service-progress.md:22`, which still says exit code 76.
4. Run one combined validation:
   - scoped Clippy, then `integ-tests-r2` above;
   - rerun the 15 tests that failed only in the worktree, in the main checkout: 6 `project_worker_split`/`split_tests` lib tests, plus `native_owner_recovery` ×3, `permanent_trim`, `reference_refresh` and `split` ×4;
   - the extension compile and full suite;
   - `python scripts/check-self-spawn.py`, `scripts/check-repo.ps1` and the whitespace check.
5. Bump to **1.8.21** immediately before building (standing user authorization; see "Versioning" below), then `python scripts/release-version.py check --expected 1.8.21`.
6. Build:
   - the optimized runtime with `scripts/build-service-runtime.py --profile release` and the static ORT environment (about 14 min);
   - the native ZIP, then the VSIX (`scripts/package-extension.py`).
7. Write and run `scripts/check-process-topology.py` (plan Verification section), then the installed-editor journeys. Two failures on the same component means stop and write a root-cause note.
8. Hand the VSIX to the user for manual testing.
9. Commit only when the user asks. Include `docs/arbiter/concept.md`. Use the user's DCO sign-off and no agent trailers.

**Still unverified:**

- whether Codex trusts a CA installed in the OS certificate store;
- which Windows shell runs the header helpers;
- Linux and macOS: Clippy, the in-process suites, and provider process groups without a guardian.

**Deferred with the user's knowledge:**

- H7: direct in-memory calls instead of in-process pipes;
- H10: deleting the worker role subcommands after migrating their test callers.

**Open user question:** stable, preview and custom installs each run their own service. That is by design; the user has not answered whether to change it.

Plan approved by the user on 10 November 2026. Its user decisions:

- Only the service stays resident. Workers run inside it.
- The only allowed temporary processes are:
  - one session per VS Code window;
  - CLI and installer one-shots;
  - one PDF parser child;
  - a user-opened `tools shell`.
- Agents connect over HTTPS directly.
- Explorer warns about broken agent entries and offers a one-click **Reconnect**, which runs after one confirmation.
- Everything ships together.

This record reverses the 1.8.19 narrowing "agent configurations outside VS Code are not edited". That narrowing was made during implementation and never approved by the user.

States: `pending`, `in_progress`, `blocked`, `implemented`, `qualified`.

Each entry distinguishes:

- source-review findings;
- reproductions;
- passing hypotheses;
- qualification results.

Receipts are kept in `artifacts/one-process/`.

## Build ownership

- One agent runs Cargo per checkout at a time.
- Before running Cargo, an agent creates `artifacts/one-process/cargo-owner-<checkout>.lock` with an exclusive create, writes its name into it, and deletes the file when the run ends.
- The extension TypeScript compile and Node tests use `artifacts/one-process/ts-owner.lock` in the same way.

## Versioning

The user gave standing authorization on 10 November 2026: every new installer build gets the next unused patch number. The first installer from this work is **1.8.21**.

- Bump immediately before the VSIX/native package build, not during implementation.
- Keep native, extension, lockfiles, the template catalog and docs synchronized.
- Verify with `python scripts/release-version.py check --expected <version>`.

## Commit scope

User instruction, 10 November 2026: the commit for this work also includes `docs/arbiter/concept.md` (72,199 bytes, untracked). It is a future-plan concept, not current behavior. List it in the documentation index only as a future concept. Commits use the user's DCO sign-off with no agent trailers.

## Integration runs (primary)

| Receipt | Scope | Result |
| --- | --- | --- |
| `integ-compile-r1` | `npm run compile`: extension, webviews, Explorer bundles, uninstall worker | Pass, 15 s. This run includes the integrator's allowlist and packager-module edits and the login opt-out wiring. |
| `integ-ext-full-r1` | Full mocked extension suite | 1,375 selected, 1,354 pass, 0 fail, 21 skipped |

## Independent review (10 November 2026, read-only, no runs)

Review of the main-checkout diff, sorted by severity.

**Confirmed from source, not reproduced:**

1. HIGH: a cancel/answer race kills the window session, including running installer work.
2. The extension's 5-second close grace kills exclusive work that the native drain contract allows to finish.
3. `http-credential` was admitted in-session, so the session could return bearer tokens.
4. A partial replacement failure leaves unreported moved folders.
5. The credential helper points at a version-specific runtime path.

**Plausible risks, being fixed:**

- in-session output budget below the former one-shot buffer;
- no install lock around replacement;
- an unreadable install record is treated as a replacement;
- weak ownership proof for state/logs folders;
- ledger capture misses normalized URLs and helper-only references;
- unverified Codex helper key.

**Plausible risks, recorded without change:**

- The extension replaces a pre-1.9.0 installation without a prompt. This matches the user's 1.8.19 decision.
- Cancelled work continues after `cancelled`; cancellation is cooperative.
- Single instance is per control root. Stable, preview and custom installs are separate installations by design.
- On owner loss, the session drains started exclusive work up to that request's deadline. This is a documented exception to "exits with its window".
- Different-build one-shot fallbacks are limited to predecessors.

**Integrator contract change:** session responses go up to 16 MiB, matching the former one-shot buffer (`service_ipc::MAX_CODEC_BYTES`). The fixture now carries a `budgets` object that both Rust and TypeScript assert. The corrections were sent back to the native, extension and agents owners.

**Extension corrections: implemented.**

- Each cancel id is kept until it is answered. `cancelled`/`failed` answers to cancels are benign.
- Release drains started work up to its own deadline plus 5 s, and kills only after that.
- The TypeScript budgets equal the fixture's `budgets`.
- There is one window-session subscription.
- `ext-d-full-r2`: 1,379 selected, 1,358 pass, 0 fail, 21 skipped.

**Agents corrections: implemented.**

- Runtime transitions refresh owned credential helpers before retire-previous runs.
- New finding `https_helper_missing`, repaired through the refresh path rather than a full Reconnect.
- Normalized endpoint URLs and helper-only references are now captured.
- Nothing is written before the confirmation.
- The Codex `mcp_servers.<id>.http_headers_helper` key is confirmed verbatim in the Codex config reference.
- `t1-full-r3`: 1,382 selected, 1 fail, a test fixture whose runtime image path didn't exist. That fixture was fixed. `t0-r3` (agentReconnect + agentConfigHealth): 31 selected, 31 pass.
- The full suite is still to be rerun in the integration batch.

**Native corrections: implemented.**

- `http-credential` and `runtime launch` are refused in-session.
- A partial move is rolled back newest first. If the rollback fails, every folder left aside is listed as residue.
- A `.<leaf>.replace-1.lock` beside the installation refuses a concurrent replacement before any side effect.
- Unreadable records cause a typed refusal.
- Layout folders outside the default locations need an AIRK owner record.
- Diagnostics are scoped to the request's stderr.
- Display-formatted reconnect instruction.
- The 16 MiB response budget is judged on the encoded frame.
- `review-clippy-r1` passes. `review-tests-r1`: 96 selected, 96 pass.

**Integration rerun:** `integ-compile-r2` passes. `integ-ext-full-r2`: 1,382 selected, 1,361 pass, 0 fail, 21 skipped.

## Workers patch review (10 November 2026, read-only)

**Confirmed from source:**

1. A query past its 125 s deadline calls `process::exit(75)`, which now ends the whole service.
2. Drain only works inside `service_execution::run`. A slow startup or activation, or a blocking read, leads to exit 76, and recovery replays the work, so every project goes into a crash loop.
3. On early `?` returns, the cleanup owner exits without joining its job.
4. The stall watchdog restarts the whole service.
5. PDF slot outcomes are misclassified: busy is treated as a fatal job failure, and an interruption marks the document as disappeared.
6. Usage spans see whole-service counters.
7. A poisoned shared cache affects every project, and cache notification handles outlive their owner.
8. `ChildWorker::drop` can block the supervisor.
9. On Unix, the PDF child can be orphaned after a service crash.
10. `scripts/check-self-spawn.py` has false negatives.

**Plausible risks:**

- the watcher memory check is gone;
- exit 76 during stop/upgrade;
- callers that turn a checkpoint into a bool can continue past a drain;
- the production in-process start path is not covered by native tests.

**Correction round, resumed after the reboot (10 November 2026).** Item 12 was done by the primary; three agents own the rest, with disjoint files. No agent runs Cargo, so there is one combined batch after review.

- workers agent: items 1–4, 8, 10, 11 and 13;
- PDF agent: items 5 and 9;
- caches and usage agent: items 6 and 7.

**Item 12, `scripts/check-self-spawn.py`: implemented, Python T0 passed.**

Source-review false negatives:

- An image path passed to a start through a parameter, field or const was invisible. Examples: `embedding_pool_process::start_from_main`, `universal_launcher::run`, `start_with_observation_clock`.
- `CreateProcessW` (`native_process::spawn_detached`, `child_job_named::spawn_guarded_io`), `spawn_guarded*` and `Invocation::new` were not start primitives.
- A `;` inside a signature (`[u8; 4]`) ended the function early.
- `br"…"` raw strings were not recognized.
- A role literal in a const was skipped.

Correction:

- A reviewed `STARTS` roster now lists every production function that creates a process, with what it starts: service, one-shot, pdf, legacy, external or primitive. A new start fails until it is classified.
- The parser fixes above.
- The regression test is `test_former_false_negatives_are_found`.

Evidence: `python -m unittest scripts/tests/test_check_self_spawn.py` ran 4 tests and all passed. On the current tree: 6 self-spawn sites, 5 legacy, 33 reviewed starts.

**Items 6 and 7 (caches and usage agent): implemented, not compiled.**

- **Usage spans.** The epoch advances on every start and end. Disabled-usage requests are counted as unmeasured work. A new `airk_kernel::OwnerActivity` guard covers owner work, and a poisoned tracker lock is recovered and reported as overlapped. Reporting goes through the existing `ResourceMeasurement.overlapped` / `ResourceSummary.overlapped_intervals` fields; there is no wire change.
- **Caches.** Poison recovery and per-collection release for `verified_cache`, `lexical/cache` (change-notification handles) and kernel `source_policy` `RUNTIMES`, the latter for expired bindings only.
- **New public items:** `airk_kernel::{OwnerActivity, release_collection_caches}` and `airk_store::current::release_collection_caches`.
- **Integrator wiring is pending** until the workers agent finishes: `OwnerActivity` in `service_execution::run` and in the source watcher, and release at project/cleanup owner exit.
- **Recorded risk:** more spans report as overlapped and carry no counters.

**Items 5 and 9 (PDF agent): implemented, not compiled; the Unix code is uncompiled.**

- A busy slot defers the document: its previous artifact stands in under an unchanged binding, it gets one retry at the next drain, and is then recorded with the `extraction_deferred` skip code.
- An interrupted wait stops the scan with the typed `PdfSlotInterrupted`, settling nothing.
- Unix parent death: Linux `PR_SET_PDEATHSIG` plus a `getppid` re-check; macOS kqueue `NOTE_EXIT`. The decision rule is a pure `parent_watch`.
- Bounds: 60 s slot wait per attempt, at most two waits per document per scan, a 15 s / 10 s CPU / 512 MiB parse.

Integrator follow-ups:

- **B, done:** `inventory.rs` coverage returns the slot error instead of caching `ExtractionFailed` for that content hash. Risk: coverage fails while another owner holds the slot.
- **C, done:** a `#[serde(skip)] IndexReport::extraction_deferred` count, with no wire change. It is incremented in `ScanSink::defer`, and `verify_source_inventory` refuses when it is nonzero.
- **A, pending, in the workers agent's file:** `service_execution::run` maps `PdfSlotInterrupted` to the owner's typed `Draining`/`Canceled`/`WorkStalled`.
- **D, not done:** optional progress text.
- **Linux residual:** the window between fork and the first `getppid` remains; closing it needs `pre_exec` in `child_group.rs`.

**Items 1–4, 8, 10, 11 and 13 (workers agent): implemented.**

- `ExecutionScope` keeps scope-level `draining`/`stuck` flags that exist before `install`. `checkpoint()` honors drain with or without a grant.
- `StopStep::Quarantine` replaces `ExitService`; `exit_service_for_stuck_owner` is removed.
- Stopping owners are polled from a thread-local `STOPPING` list on the actor's turn. `ChildWorker::drop` never waits for an in-process owner.
- The query deadline and the stall watchdog quarantine the owner's own scope. Only a child-process fixture still exits 75/76.
- The lease is held in `WorkerScope` until the owner thread returns.
- The `CleanupJob` guard joins its job thread.
- The watcher gets `StoppingWatch`: 5 s, then 20 s.
- Item 10 audit: `split.rs:174`, `observe_native_integrity`, `record_watch_roots` and `observe_maintenance` were fixed; the rest were reviewed and are OK.
- New tests: the quarantine regression and its release, self-reported stuck, the production start refusal, the in-process start, the watcher quarantine, and drain before install.

**Integrator integration (primary), source-reviewed:**

- **`catalog.rs` `DrainWorker`.** It relied on the old blocking `Drop`. It now returns a typed `WorkerDrainPending` while the owner is in `STOPPING`, and `CatalogControlHandle::drain_worker` asks again for up to 35 s (stop grace + drain + 5 s). After that it fails as quarantined.
- **`service_execution::run`.** It holds `OwnerActivity`, and maps `PdfSlotInterrupted` to the owner's `Draining`/`Canceled`/`WorkStalled`; if no flag is set, the original error is kept.
- **Cache release.** `worker_scope::CollectionCaches`, a drop guard calling `airk_kernel::release_collection_caches`, is placed right after the registration in `project_worker::serve` and `derived_worker::serve`, so it drops after the kernels and the job.
- **Decision: the source watcher is not registered as `OwnerActivity`.** Marking its 250 ms pulses would make nearly every span overlapped. Residual: the watcher's own CPU can still land in a concurrent span.
- **Shutdown with a quarantined owner was checked.** `ChildWorker::drop` returns without joining, and the actor loop ends once every remaining owner is quarantined.
- **Docs filled in.** `process-model.md`, `release-1.8.21.md` and `service-progress.md:22` now have the real field names, codes and bounds. The drafted "typed stuck-owner status observation" was corrected: status uses the existing `automatic_start_blocked` and cannot tell a quarantine apart from an exhausted circuit; the reason is in the service log. A distinct status field would be a D8 contract change, left to the user.

**Validation (frozen inputs):**

- `cargo fmt --all`.
- `integ-clippy-r2` (cli/mcp/kernel/retrieval/ingest/store, `--all-targets`, local-inference): exit 101 with one diagnostic, E0308 at `project_worker.rs:190` (`Ok(())` matched against `Result<Sha256Digest>`). Fixed with `Ok(_)`.
- `integ-clippy-r3`: exit 0 in 29.3 s, inputs unchanged. The `cfg(unix)` code is uncompiled.
- **`integ-tests-r2`** (the full combined native batch, private fixture base): 1,863 selected, 1,862 pass, **1 fail**, 44 skipped, 1,269 s, inputs unchanged.
  - Every new test from the three agents passed.
  - `native_owner_recovery` ×3 passed. The fixture-ACL diagnosis is confirmed.
  - The `project_worker_split`/`split` and `combined_reads` tests (`permanent_trim`, `split`, `reference_refresh`) passed. The 15 worktree-only failures do not recur in the main checkout.
- **The one failure was the new test's wrong premise, not a code defect.** `production_owner_start_refuses_an_unverified_runtime_before_starting_any_owner` assumed `engine_identity()` refuses in a test image. An absent receipt is the valid single-executable identity (`runtime_pair.rs:113`), so the production path correctly started an owner in process. The test was rewritten as `production_owner_start_runs_in_the_service_holds_its_lease_and_is_reaped`: the real `ChildWorker::spawn` and real engine identity against a registered project, then an in-process start, the lease held, a non-blocking stop, a reap and the lease freed.
- `integ-clippy-r4` (airk-cli, all targets): exit 0.
- `integ-t0-r1` (`project_supervisor::tests::*`, store `verified_cache::*`, `current::lexical::cache::*`): 13 selected, 13 pass.
- `cargo fmt --all -- --check`: clean.
- Extension suite:
  - `integ-ext-full-r3`, run under the **system Node 24.21.0 by mistake**: 1,382 selected, 1,360 pass, 1 fail, the late-Memento variant of `serviceOnboarding.test.ts:28` (`console_operation_failed`). This is not a valid receipt because the pinned toolchain is 24.15.0. The failure is unexplained; the possible Node-version sensitivity is not investigated.
  - **`integ-ext-full-r4` under the pinned Node 24.15.0:** compile plus 1,382 selected, 1,361 pass, 0 fail, 21 skipped.
- `scripts/check-self-spawn.py` passes, and its unit tests pass (4/4).
- `git diff --check` is clean.
- `integ-check-repo-r1` (`scripts/check-repo.ps1`): passed, 2,225 public files.

**Version bump and build (10 November 2026):**

- Bumped to **1.8.21**: workspace `Cargo.toml`, the 8 local crates in `Cargo.lock`, `package.json`, both root entries in `package-lock.json`, and `explorer_templates.json` (regenerated with `build-explorer.cjs --write-mcp-catalog`). `release-version.py check --expected 1.8.21` reports "Native, extension and lockfiles agree: 1.8.21".
- `runtime-build-r1` (`build-service-runtime.py --profile release`, static ORT, `--release`): pass, 867 s, inputs unchanged. `airk.exe` SHA-256 `b76bdcc4291a738803585d0885052ee09738a5127478fdc8f26abdedb566cc54`.
- `native-package-r1`: pass, 16 s.
- `extension-package-r1` (`--channel stable`): pass, 34 s. `final-extension/publish/airk-1.8.21-win32-x64.vsix`, 939 files, 77.45 MB, platform VSIX verified.
- New `scripts/check-process-topology.py`, with modes snapshot, watch, cli-burst and refuse-serve, plus `scripts/tests/test_check_process_topology.py` (4/4 pass). A live snapshot of this machine showed one service, the installed 1.8.20 `runtime managed-serve`.
- `installed-explorer-r1` on the 1.8.21 VSIX: pass, 18 s, inputs unchanged.

- **`installed-native-r1` on the 1.8.21 VSIX: FAIL (strike 1 for the installed-editor component),** 309 s, inputs unchanged.
  - Scenarios `same` and `different` passed; `multi` failed; `usage` and `untrusted` did not run.
  - Failure: after the real window reload (23:16:59.4), window A's driver read in `definitions()` was refused 14 s later by `airk.exe` with exit 1 in 35 ms: "another AIRK installer/launcher is active; retry after it finishes: lock acquisition failed because the operation would block". The service ran throughout (23:16:28 to its cleanup stop at 23:17:18).
  - Evidence: `artifacts/editor-qualification/native-installed-SvhRg4/multi/` (`a-error.json`, `a-phase.json`, the post-reload `2-AIRK.log`).
  - **Source-review hypothesis, not reproduced:** read-only install observations (`inspect-legacy-caches`, `legacy_caches.rs:322`; `lookup-http-connection`, `http_commands.rs:230`; launcher inspect; `commands.rs:170`) take `existing_install_lock`, an exclusive `try_lock` with no wait. Two concurrent observations therefore refuse each other at once.
    - The reloaded extension runs `inspect-legacy-caches` during activation (`serviceExtension.ts:1673`). The driver runs the same command in `provePrivateCacheScope`.
    - 1.8.21 makes the overlap likelier: the window session runs requests concurrently, and the new agent-health check adds `lookup-http-connection` at activation.
    - 1.8.20 passed the same journey.
  - **Not established:** which process held the lock at 23:17:13.6.
  - **Missing cheap test:** a T0 native test of two concurrent read-only observations on one installation.
  - Correction direction, for review: read-only observations take a shared lock, or wait a bounded time like `connection_install_lock`. Writers stay exclusive.
- `topology-watch-native-r1`, a forbidden-role watch alongside the journeys for 600 s: **pass**.
  - It saw no worker-role, guardian, stdio MCP or alias process.
  - It did see: service ×5 across the scenarios (`runtime managed-serve` ×3, `serve` ×2), `service session` ×6, `runtime self-test` ×2, and the harness's own one-shots (`runtime inspect` ×11, `http-connection-info` ×9, `inspect-legacy-caches` ×3, `service rpc` ×4, and others).

**User decision (10 November 2026): fix first, then 1.8.22.**

Lock correction (`runtime_install.rs`):

- `existing_install_lock` stays exclusive, because `inspect-legacy-caches` can settle an interrupted withdrawal. It now waits for a held lock up to `EXISTING_INSTALL_LOCK_WAIT` (15 s) before refusing, through `existing_install_lock_within(root, budget, pause)` with an injected pause.
- `install_lock`, used by long installers, still refuses at once. The activation writer `ensure` already uses the bounded `connection_install_lock`.
- Callers: legacy-cache inspect/retire/reprepare, legacy-launcher inspect, `lookup-http-connection`, predecessor-cleanup lookup, and the `commands.rs:170` registry transaction. All run on CLI one-shot or session-request threads, never on the service runtime.

Regressions:

- `connection_lock_tests::concurrent_existing_install_observations_wait_for_each_other`: the second observation is observed waiting, by event and without a sleep, then acquires once the first releases.
- `connection_lock_tests::an_existing_install_observation_refuses_after_its_wait_budget`.

Validation and docs:

- `lockfix-clippy-r1` (airk-cli, all targets): pass.
- `lockfix-tests-r1` (`connection_lock_tests`, `legacy_caches`, `legacy_launchers`, `http_cleanup`, `http_commands`): 36 selected, 36 pass.
- Docs: the current-guidance references move to 1.8.22, and `release-1.8.21.md` is renamed to `release-1.8.22.md` with an "About 1.8.21" section. This ledger keeps 1.8.21 as dated history.

**1.8.22 build and installed runs:**

- `runtime-build-r2`: pass, 909 s, inputs unchanged. `airk.exe` SHA-256 `479e1a6e90a31f0f07601ed5bae8443cbc0fc3cada6f15a4a3d445cfcfabb932`.
- `native-package-r2`: pass.
- `extension-package-r2`: pass. `final-extension-1822/publish/airk-1.8.22-win32-x64.vsix`, 939 files, 77.46 MB, SHA-256 `01e342b3ec01c335e31e81f37ea370a1a7e8dd14b83896231e6a997615b69f12`.
- Pre-build checks: `integ-check-repo-r2` passed (2,227 files). `integ-ext-full-r5` (pinned Node 24.15.0): 1,382 selected, 1,361 pass, 0 fail.
- `installed-explorer-r2`: pass, 12 s.
- **`installed-native-r2`: FAIL, strike 2 for the installed-editor component.** 416 s, inputs unchanged. `same` and `different` passed; `multi` failed; `usage` and `untrusted` did not run.
  - The lock correction is confirmed at the earlier failure point. After the reload, the restored-definition read, endpoint refresh, standalone assistance and Explorer-ready steps all completed (`multi/a-reload-definition-observations.json`, `a-c8-*`, `a-explorer-ready.json`).
  - The new failure is in the external UI automation (`scripts/editor-ui.cjs:302`), on the "AIRK Help" Quick Pick, clicking "Browse all help": "Quick pick click has an unknown outcome; no fallback dispatch". The real mouse click was dispatched, and 100 ms later the picker was still open with that row focused. The harness by design never re-dispatches.
  - The picker is a plain `vscode.window.showQuickPick` with two fixed items (`explorerController.ts:730`), unchanged on this branch. The failure is not yet explained: either the product did not accept the pick, or the 100 ms post-click observation is too short under load. At the sample there were no notifications and one widget.
  - Evidence: `artifacts/editor-qualification/native-installed-1G96R7/multi/` (`a-ui-diagnostics.json` holds the picker samples).
  - The harness failure cleanup was incomplete: "Failure Stop response is incomplete or crossed", and `native-stop.json` is missing. Afterwards no fixture `airk.exe` was running; only the user's installed service, PID 3832.
- `topology-watch-native-r2`, 900 s alongside `installed-native-r2`: **pass**, with no forbidden role. New processes seen: service ×6, `service session` ×6, `runtime self-test` ×2, one-shots ×57.
- **Residue found and removed.** Both installed runs left six fixture login tasks (`airksvc-*`) pointing at `artifacts/editor-qualification/.../native-service` runtimes, including from passed scenarios. Cause: step E registers login startup on every fresh install, and the journey harness never unregisters it; 1.8.20 needed the same manual cleanup. All six were removed with each fixture runtime's `runtime login --install-dir <fixture> uninstall` (log: `installed-native-login-cleanup.txt`). Only the user's own `airksvc-251cb4b…` remains.

**Root-cause note (two strikes; stopped before a third attempt).**

- **Wrong assumption:** that one installed-journey run would confirm the corrected candidate. In fact the `multi` scenario failed at two unrelated points, each needing a 15-minute rebuild plus a journey run to observe.
- **Missing cheap tests:**
  1. A T0 native test of concurrent install observations. It now exists (`connection_lock_tests`).
  2. A harness-level unit test that the Help Quick Pick accepts within the observation window, or a harness rule that waits for the picker to close within a bounded deadline before declaring the outcome unknown.
  3. A harness cleanup step that unregisters login startup for every owned fixture installation.
- **Next step, awaiting the user:** none, until the user decides.
  - If the Quick Pick failure is harness timing, only the harness changes, and the journeys can rerun on the same 1.8.22 VSIX without a rebuild.
  - If it is product behavior, the cause has to be reproduced first.

Review status: the primary reviewed the integration paths and the riskiest agent code (stop polling, quarantine, query deadline, lease retention, shutdown, catalog drain). This was a targeted review, not a line-by-line independent review of every agent change.

**The `native_owner_recovery` ×3 failures are a fixture ACL problem, not a code defect.** They come from the partial `integ-tests-r1.log` (`native Windows boundary policy unavailable`).

- Cause: the runner's `native-fixtures/` directory inherits the repository ACL, which includes Authenticated Users with Modify rights. The native private-boundary check rightly refuses that.
- The documented precondition (release-readiness.md, "native-fixtures") is a directory with FullControl for only the creator, SYSTEM and Administrators. `readiness-1820/native-fixtures` has the same broad ACL.
- Correction: a new directory, `artifacts/one-process/native-fixtures-private/`, has its inheritance removed and those three grants. No existing ACL was changed.
- `integ-tests-r2` passes `--fixture-base artifacts/one-process/native-fixtures-private`.

Open for the user: the Explorer Assistance start (`explorer_assistance::command`) runs VS Code's Node runtime for an external agent. It is not an `airk.exe` process, but it is a per-agent process.

**Integrator design decision:** quarantine replaces the service exit. A stuck owner keeps its lease, its project's circuit opens and its status reports the problem; other projects keep running, and an explicit restart is the recovery. The full correction batch was returned to the workers owner, who now works in the main checkout.

## Steps

| Step | Owner | State | Evidence |
| --- | --- | --- | --- |
| A. Session protocol contract (`crates/airk-cli/src/session_protocol.rs`, `fixtures/wire/session_frames.json`) | primary | implemented | `session-contract-r2`: T0, 5 selected, 5 pass. The first run, r1, failed to compile: the types derived `Debug`/`Clone`/`PartialEq` over `ControlMethod`/`ControlResponse`, which don't implement them. |
| B. CLI output sink | native | implemented | **Golden capture** (`artifacts/one-process/golden/`): 60 offline runs.<br>- 56 are byte-identical.<br>- 4 differ: `runtime --help` and `service --help` (two runs each), only by the new `http-credential`/`session` rows.<br>- Before binary `f358b213…`, after `173c5da2…`. |
| C. `airk service session` | native | implemented | **Tests:** `bc-tests-r1`, then `efj-tests-r1`: 94 selected, 3 fail. `efj-tests-r2`: 22 selected, 22 pass. Scoped Clippy `efj-clippy-r3` passes.<br>**Real binary:** `cli_commands::service_session_matches_one_shot_output_and_exits_on_eof` checks hello identity, then four in-session runs that are byte-equal to the one-shot output, then that `serve` is refused, then exit on EOF.<br>**Owner loss fixed:** the session now drains instead of exiting immediately. It cancels read-only work and lets a started exclusive command finish, but never past that request's own deadline (pure `drain_until`/`may_exit`, injected timing and owner probe). `owner-clippy-r1` passes; `owner-tests-r1`: 19 selected, 19 pass.<br>**Recorded limits:**<br>- success-only exit status;<br>- non-UTF-8 stdout is converted lossily;<br>- Unix stream-redirect code not yet compiled. |
| D. Extension: one session per window | extension | implemented | Evidence: `nativeSession.ts`, the spawn guardrail in `explorerBoundaries.test.ts`, and wire-fixture encode/decode tests.<br>`ext-d-r1`: 580 selected, 576 pass, 0 fail, 4 skipped. `ext-d-full-r1`: 1,367 selected, 1,346 pass, 0 fail, 21 skipped.<br>Tested only against a fake child and the fixture; no real-binary run yet.<br>Remaining allowed spawns: the session; different-build and window-less one-shots; the console for a different-build runtime; the uninstall worker; the `tools shell` terminal. The stdio MCP spawn was returned to the agents agent for removal. |
| E. Continuously available service | native + extension | implemented | **Native:**<br>- `--idle-exit-seconds` is omitted when the saved catalog authority contract is 1 or higher.<br>- Typed `service_already_running` for a second start.<br>- `startup-opt-out.json` in the install root is written by `login uninstall` and cleared by `login install`. It is reported as `startup_manager.user_opted_out`. This is a new install-root file, not an `airk-state/1` format.<br><br>Earlier extension note: | **Extension:** login startup is now registered after upgrades too. The idle-exit restart is replaced by a recovery path used only when OS registration was refused.<br>**Native:** no idle exit in HTTPS mode and a persisted login opt-out record, both in progress. |
| F. Replacement installer obeys one-airk §10.3 | native + agents | implemented | **Extension:** captures the reconnect ledger (`airk.reconnect.ledger.1`) before `--apply`, then retires the replaced host artifacts. **Native:** ownership is proven before anything else, the CA is retired through its receipt, `client_references` is reported, a replacement lock is taken, a partial move rolls back, and `http-credential` exists; see "Native corrections" above. A replacement deletes `startup-opt-out.json` with the old folder, so only an upgrade honors the opt-out. |
| G. Explorer agent warning and Reconnect | agents | implemented | Second round:<br>- One confirmation now runs the reviewed chain: index setup draft/apply, then the grant through `applySearchScope`, then `enrollHttpSpec`, then the rewrite.<br>- **Disclosed side effect:** the grant makes the new index this window's search scope.<br>- The stdio MCP spawn is retired; the native-execution-off switch closes the window session.<br>- Evidence: `t1-full-r2`, 1,374 selected, 1,353 pass, 0 fail, 21 skipped.<br>- The integrator added `agentReconnect.js` to the packager modules and `nativeSession.ts` to the uninstall bundle allowlist.<br><br>First round: | **Implemented:** classification, notification, banner, status bar and Reconnect for reviewed indexes. Evidence: agents `t0-r2`, 271 selected, 271 pass; `t1-full-r1`, 1,367 selected, 0 fail, 21 skipped.<br>**Returned to the agent:** creating the index and grant after the user's single confirmation. The agent had stopped at `awaiting_index`.<br>**Open:** whether Codex trusts the OS certificate store outside VS Code; which Windows shell runs the header helpers. Both need checks against the real clients. |
| H. Workers in the service | workers | in_progress | **Merged into the main checkout** (patch applied cleanly; `scripts/check-self-spawn.py` roster key updated to `run_with`). `integ-clippy-r1` over cli/mcp/kernel/retrieval/ingest is clean. **One PDF child for the whole service** is enforced by `airk-ingest/src/pdf_slot.rs`: `h8-ingest` 15/15, `h8-integration` 69/69. The review corrections above are in progress (quarantine replaces exit 76).<br>Earlier worktree record: |**Implemented (Windows evidence):** H1 transport carriers, H2 per-owner scopes, H3 in-process source watcher, H4 in-process project and cleanup owners, H5 cooperative drain.<br>**H5 drain timing:** after 10 s, then a drain, then 20 s more; after that the service exits with code 76. The 20 s is the agent's choice.<br>**H6:** accounting is count-based plus service RSS. Usage spans now see whole-service counters.<br>**H9:** no guardian in-process. Unix cleanup on abrupt service death is not done.<br>**H11:** `scripts/check-self-spawn.py`.<br>**Evidence:** `final-clippy-2` clean. `final-cli-workers`: 220 run, 214 pass; the 6 failures also fail at unmodified HEAD in the worktree (fixture location). `h4-parity`: 140 selected, 131 pass; the 9 failures also fail at HEAD (`head-parity-failures`). Those 15 must be rerun in the main checkout.<br>**H8:** the PDF child is spawned by the service, but up to 4 can run concurrently. Returned to the agent: one PDF child for the whole service.<br>**Deferred:** H7, direct in-memory calls instead of in-process pipes (no extra process involved). H10, deleting the role subcommands after migrating their test callers. |
| I. Documentation | primary | pending | |
| J. CLI outside VS Code | native | implemented | `StartRoute`/`start_route()` at both election sites. With login startup registered, the CLI starts the service through the OS manager and never spawns `serve` itself. A second `serve`/`managed-serve`/`foreground`/`host` fails with a `service_already_running:` message; the JSON `code` is still `operation_failed`. The OS-manager start still needs the T2 topology check. |
