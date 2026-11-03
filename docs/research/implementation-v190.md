# AIRK 1.9.0 implementation ledger

**Started:** 2 November 2026. **Branch:** `feat/finetuning`.
**Approved plan:** [phases, contracts and performance gates](plan-v190.md).
**Current working version:** 1.8.8, selected 3 November 2026 for manual testing
before later 1.9.0 promotion. Earlier checkpoints below retain their original
versions and exact payload identities; they are not relabeled.
The maintainer authorized sequential implementation and signed phase commits.
Progress below is observed local evidence; it does not establish publication or
cross-platform qualification.

## Entry identity and boundaries

- Starting commit: `6d00d42eb56d459055383ed64f743332030ba6d3`.
- Entry working tree: only the reviewed `plan-v190.md` refinement was modified.
  The approved document is included with the initial baseline checkpoint.
- Native/extension/lockfile version: **1.8.7**, verified before the future P0b
  1.9.0 increment. No index schema, source assignment or normal installed service
  is changed for baseline testing.
- Native build-input identity before implementation:
  `72f9c7a33d882d9e9911161e64a6b94d026f82790698191814ce04c56d5c2b61`
  (750 files under the runtime builder's declared input inventory).
- Rust 1.98.0 is installed under the contributor's Cargo directory. Its absence
  from PATH was resolved per process; no global configuration changed.
- Both pinned INT8 MiniLM profiles were provisioned and hash-verified using the
  repository script. Pinned ONNX CPU libraries/notices were verified from an
  existing offline runtime and copied into ignored build inputs.
- Raw runtime builds, receipts and logs remain in ignored `artifacts/v190/`.
  Earlier retained 1.8.7 runtimes had different build-input identities, so a fresh
  release pair is required for the source-bound baseline.
- The fresh release pair matches the native input identity above. Main SHA-256:
  `bb8d02336a2f8ef4db9e90f628b9972a809c6320b3757757af61b3d27f93ad76`;
  helper: `ebf02003ccf5076e01c3aaf842c896633f1a393ece7b359fd647d2895a515016`.

## Phase status

| Phase | State | Acceptance / recovery boundary |
| --- | --- | --- |
| P0a | Complete — `60f5c9d` | Frozen controls, relevance judgments and matched MCP measurements; no runtime behavior change |
| P0b | Complete — `3021995` | Synchronized 1.9.0 increment and forward-line policy; no schema migration |
| P1 | Complete — `d9eff91` | Compact-plan correctness and nested object inputs; no persistent format change |
| P2a | Complete - `94f544a` | Independent reply detail; qualified native/transport/export and performance evidence below |
| P2b | Complete - `1b623ce` | Bounded existing-observation guidance; matched quality/latency gates below |
| P3 | Complete - `b72fbc8` | ADRs, schemas, authority/partition fixtures and upgrade fault boundaries |
| P4 | Complete - `a425ba0` | Single-index saved-source verification; maintainer selected Report as the default, with explicit Off and Require |
| P5 | Complete — this locally qualified checkpoint | Inert normalized catalogs and owned U0–U6 upgrade/recovery; production activation disabled until P6 |
| P6–P7 | Not started | Indexing/grants activation, then complete combined-read parity |
| P8–P10 | Not started | Recommendations/harness scopes, permanent trim, split and integrated advice |
| P11a–P12 | Not started | Effective-profile reporting, explicit retained-profile migration and refresh |
| P13a/P13b | Conditional | Ranking/diversity only if frozen relevance/resource gates pass |
| P14 | Not started | Combined qualification and release-candidate evidence |

## P0a — reproducible controls and relevance measurements

The original corpus in `scripts/fixtures/retrieval-v190.json` freezes ten source
files and nine query judgments across code, manifests, prose, tables and a
negative lexical case. Development and held-out queries are reported separately.
This is a small regression corpus, not broad production relevance qualification.
Do not tune held-out labels or substitute a different model to improve results.

`scripts/check-retrieval-quality.py` retains its existing synthetic qualification
path. Its new explicit `--corpus`/`--model-dir` path validates catalog-pinned model
assets and uses actual native indexing and persistent stdio MCP. It records
nDCG@10 and Recall@10 separately for lexical/vector/hybrid, retaining misses and
failed attempts. The first occurrence of each judged file earns gain at its
original chunk rank; duplicate chunks consume slots without increasing recall.
Negative cases have a separate empty-result outcome and do not inflate the
positive-query macro averages. All returned citations must retain the fixture's
exact source hash and requested prefix.

The compact/rich chained-plan probe runs through actual MCP `parse`, independently
of relevance scores. It retains the advertised plan input schema and step states
to distinguish the workflow bug from connector schema limitations.

The new `owned` operation of `benchmark-mcp-investigation.py` prepares original
disposable sources and a private service before reusing its existing replay.
It never installs AIRK or changes the normal service. Existing `run` remains a
read-only replay of a separately prepared configuration. All three connection
cohorts retain the identical nine-tool batch and separate warmup/first-status
accounting. Configuration validation now recognizes Windows extended native
paths through physical directory identity; a different source remains rejected.

Controlled no-op and one-file-edit jobs run **after** the graded query samples.
They bind observed source progress to the exact native job receipt and keep source
job duration distinct from semantic completion. Only generated fixture bytes may
be edited, as declared by the frozen corpus control; every other source hash must
remain unchanged.

### Commands and observations so far

| Check | Observed outcome |
| --- | --- |
| `python scripts/release-version.py check --expected 1.8.7` | Passed |
| Focused existing kernel compact-symbol-ID test, default inference disabled | 1 passed; this does not cover the reported compact unique-path binding failure |
| Initial use of `--exact` with an unqualified Rust test name | Selected zero tests; corrected to the complete test identity above, not counted as coverage |
| `python -m unittest discover -s scripts/tests -p 'test_retrieval*.py' -v` | Latest focused run: 14 passed, including controlled-edit containment and failure-receipt privacy |
| MCP investigation harness unit suite | Latest run: 12 passed, including Windows extended-path identity and different-directory rejection |
| `npm test --prefix packages/vscode` | 541 passed, 19 skipped, zero failures; skips include installed-native qualification |
| Python script suite | Initial 304-test run had one Git ownership-environment error and six skips; process-local `safe.directory` rerun passed all runnable cases. Final complete run: 307 tests, six skipped, zero failures/errors |
| Fresh release runtime build, formatting, workspace Clippy | Passed; release pair source identity and both executable hashes retained above |
| Full unfiltered native workspace suite | Finished with 1,836 reported passes, five private-boundary fixture failures and 31 declared ignores across 73 summaries, including owned subprocess reports; original command exited 101. Every failed identity then passed under the qualified fixture base; no assertion was weakened |
| Exact workspace-feature `native_owner_recovery` target in a fresh owner-only namespace | 3 passed; the owned subprocess-only helper remains intentionally ignored as a standalone test |
| Two failed store identities, rerun using the exact full-workspace test binary and qualified base | Both passed: original-root replacement fencing and untouched native retained-producer layout |
| Corrected relevance preflight through actual MCP | All nine query cases completed in lexical/vector/hybrid modes as declared; compact plan reproduced partial/skipped dependent read, rich equivalent completed |
| Controlled indexing preflight | No-op: zero extraction/publication, same token. One-file edit: one extraction, one committed change/publication; all untouched source hashes retained |
| Owned MCP replay preflight | All three connection cohorts passed coverage/cleanup checks with one sample and concurrency one; these are functional checks, not performance qualification |
| Uncontended MCP baseline | 270 measured trials, 2,430 task calls, zero errors/retries; all nine cohort/concurrency groups passed full coverage, publication and cleanup guards |
| Uncontended graded baseline | 75 measured queries, zero failed attempts; mode/split relevance, no-op and edit controls retained in the frozen baseline |
| Installed-editor, other-platform and physical-I/O campaigns | Not run; no qualification claim |

The initial relevance setup reimported an otherwise equal model manifest with
different JSON bytes. Existing native upgrade guards correctly refused to use
that distinct ONNX object identity; the preflight reached its readiness deadline.
The paired helper's extraction confirmed identical manifest fields but different
manifest byte hashes. The harness now requests the exact paired bundled model and
independently verifies matching pinned offline input assets. Production model
guards were preserved.

The native ownership failures reproduced outside the sandbox with an ordinary
temporary directory. A **new** owner-only fixture directory satisfies the native
Windows boundary, and all three tests then passed. The exact workspace-feature
target also passed inside the sandbox with its own new owner-only directory.
Only newly created fixture ACLs were restricted; existing user directories and
production assertions were unchanged. Ordinary-temp failures remain in the logs.
An outside-sandbox benchmark attempt could not read the sandbox-owned sealed
runtime, so replay uses the same sandbox identity and its qualified private base;
no runtime permissions were relaxed.

The first owned replay also caught a Windows `\\?\` path-form comparison issue
in the existing harness. The correction uses directory identity, with a regression
that still rejects a different directory. Corrected preflights and their original
failures remain separate ignored receipts. Timings collected while native tests
were running are excluded from the uncontended performance baseline.

### Frozen baseline and numerical gates

[The committed baseline](baseline-v190.json) contains sanitized runtime/model/
fixture identities, score summaries, matched cohort timings and frozen gates.
It binds the full ignored receipt hashes; raw data and private paths stay out of
Git. Median task time including frontend setup ranged from approximately 389–1,038
ms across the declared cohorts, not a measured improvement over another release.
The first and second 15-sample halves differed by less than 7% in median time.
Every explicitly primed connection's first status was available (93 observations,
including the three reused connections); unprimed connections have no invented
separate first-status measurement.

For each matched cohort, both frozen-baseline and previous-phase comparisons
require median time no greater than reference plus the larger of **10% or 25 ms**,
and p95 no greater than reference plus the larger of **15% or 50 ms**. These are
regression decision bounds, not a permitted slowdown target or an SLA. Investigate
breaches using a matched control without changing timeouts or thresholds silently.
The cumulative bound remains tied to 1.8.7.

No mode/split may lose nDCG@10 or Recall@10 beyond arithmetic tolerance; lexical
negative cases must remain empty. S1 needs at least **+0.02 held-out lexical
nDCG@10** with its recall/evidence/resource guards before inclusion. The small
corpus does not establish general quality. Held-out lexical nDCG@10 is currently
approximately 0.556 and Recall@10 0.667; vector and hybrid are approximately 0.887
and 1.0 respectively. Keep these misses visible rather than making baseline
fixtures easier.

No-op and edit timing observations are descriptive single source-job samples;
their work-count/token assertions are mandatory. Repeat indexing/resource
baselines before phases that materially change indexing. Whole-service CPU/RSS,
physical I/O, installed-editor elapsed time and other platforms remain unmeasured,
with an explicit pre-acceptance instrumentation gate for affected work.

### Reproduction commands

Use a fresh ignored output path and the matching source-bound runtime pair:

```console
python scripts/check-retrieval-quality.py --binary artifacts/v190/baseline-runtime/airk.exe --output artifacts/v190/baseline-relevance.json --trials 3 --detail agent_lean --corpus scripts/fixtures/retrieval-v190.json --model-dir target/local-vector-model/minilm-l6-v2-u8-v1
python scripts/benchmark-mcp-investigation.py owned --runtime-dir artifacts/v190/baseline-runtime --output artifacts/v190/baseline-mcp --label baseline --samples 30 --concurrency 1 4 8
```

The model path must select a verified supported profile on the host. The command
creates and drains only its private disposable service/collections. It does not
install AIRK, read private user indexes or write into a source repository.
For native private-boundary measurements, set `AIRK_PRIVATE_CENSUS_BASE` to the
new positively qualified fixture base for the executing identity. Never point it
at user source/index state or change an existing directory's ACL to qualify it.

Final script/repository checks and the explicit complete-diff review precede the
P0a commit. Record its accepted commit identity in P0b's ledger update. P0b and
later phases remain gated until that checkpoint.

**P0a review:** the complete code/documentation diff was explicitly reviewed by
the implementer, including the approved plan refinement, source/model identity,
scope containment, failed-attempt accounting, native job boundaries, cleanup and
default synthetic-run compatibility. This was a self-review, not independent
review. README, documentation index, the existing release record and Marketplace
description agree on current 1.8.7 behavior. No production schema, kernel handler,
MCP input or extension runtime behavior changed. Repository/whitespace and local
documentation links pass; a pre-existing model-catalog link in the affected index
was corrected to its current bundled-profile section.

## P0b — 1.9.0 candidate identity

Entry commit: `60f5c9daa2a9c0c2c466d4cf8af33ea1cb06b46f`, verified human DCO
sign-off and clean tree. P0a is accepted. The version increment was explicitly
requested and is performed once for this implementation sequence.

All four version-bearing manifests/lockfiles now agree on 1.9.0; local Cargo lock
entries alone changed, with no third-party dependency update. The supported
extension forward-activation range now includes 1.9. The matrix retains downgrade,
unknown 1.10/major-line and malformed-version refusals. Its expanded positive
cases failed before the policy update, as expected. The related onboarding
downgrade fixture now derives a newer patch from the package version instead of
hard-coding 1.9.0 (which had become equal to the candidate). All refusal assertions
remain intact; the initial failing extension run is retained separately.

README, documentation index, contributor/versioning guidance, developer guide
headers and the new [candidate record](../releases/release-1.9.0.md) identify the
unpublished candidate and inherited behavior. Dated 1.8.7 evidence and baseline
identities remain unchanged. No index rebuild, Project registry schema migration
or normal installed runtime transition is performed by this checkpoint.

### Validation and candidate identity

- Version agreement, formatting and all-target/all-feature workspace Clippy pass.
- Python: 307 tests, six skipped, zero failures/errors.
- Extension: 541 passed, 19 skipped, zero failures after correcting the outdated
  downgrade fixture; installed-native/editor qualification remains skipped.
- Fresh release runtime source identity:
  `e699166f26eca2e6b31cfce0bd840d3ef9405453fd9fce39d2cf2403c16710d7`
  (750 declared inputs). Main SHA-256:
  `3e311402897408771d61a264a8e85ba20f82b425b8912856406bc44ab4d65357`;
  helper: `ebff6f1072f3a4d18522c223a66a28bff45759667f7a07942152db069e371a42`.
  Both are staged in an isolated ignored directory; no user installation changed.
- Workspace tests use the checked-in nextest `ci` profile, pinned 0.9.148, all
  features and the qualified private fixture base. Its four existing workload
  exclusions remain unchanged; P0a ran those workloads. Result: **1,836 passed,
  35 skipped** (31 declared ignores plus those four exclusions), zero failures.
  Serial workspace doctests also pass (zero doctests defined). Logs are retained
  separately.
- Actual staged-runtime relevance: 75 successful attempts; every mode/split's
  nDCG@10, Recall@10 and negative-query outcome exactly matches the frozen baseline.
  No-op and one-file edit work/publication assertions pass. The existing compact
  plan failure remains reproduced; its correction belongs to P1.
- Uncontended legacy MCP sentinel: concurrency one, 30 samples per connection
  cohort, 90 trials/810 calls, zero errors/retries; complete coverage, unchanged
  source and clean service/frontend drain. Task-with-connection median/p95 (ms):
  status-first **1,025.082 / 1,108.769**; unprimed **1,008.707 / 1,049.305**;
  reused **941.273 / 1,000.500**. All pass the frozen median/p95 limits; P0a is
  both the frozen and preceding accepted performance reference. Wider concurrency
  campaigns remain in their planned later gates.
- Stable relevance receipt SHA-256:
  `aa292f9f552b2202e30aa95714052d595240079f3b2741d187581a0b45856e33`;
  MCP receipt: `5da72b095e4646ea0ff1260409fecdf1bbaf8149e68ebdbf8c71f9146024ffa1`.
  Raw receipts remain ignored under `artifacts/v190/`.

The first P0b relevance run completed all queries and index controls but was
correctly rejected because the implementer edited this ledger during checkout
identity measurement. Its failed receipt is retained as `p0b-relevance.json`.
The complete repeat (`p0b-relevance-stable.json`) held source/binary identities
fixed and passed; no guard was relaxed. An initial benchmark invocation also
rejected an unsupported label before measurement; the corrected candidate run
above is separate.

**P0b review:** the implementer reviewed the complete phase diff and earlier
baseline controls, version/lock/catalog agreement, activation/refusal sibling
paths, downgrade fixture and current documentation. External dependencies and
catalog content are unchanged apart from the intended version. This is explicit
self-review, not independent review. Repository, documentation links, version and
whitespace checks precede the signed checkpoint. Installed-editor and other
platform qualification remain unperformed; this checkpoint claims neither.

Recovery before later state changes is an ordinary source revert. This phase
has no new persistent format, migration or job receipt. Frozen P0a artifacts and
the normal 1.8.7 installed service remain untouched.

## Historical restart handoff — after P0b

P0a and P0b are complete. The P0b commit is the current branch tip at this stop;
obtain its exact identity with `git log -1` and verify its human DCO trailer.
No push, PR, installation or publication was performed. Resume on `feat/finetuning`
with **P1**, starting from this clean checkpoint; do not repeat the version bump.
No P1 code has been changed. Later phases remain authorized but are not started.

P1 investigation traced the compact-plan defect to
`crates/airk-kernel/src/workbench.rs::locations`: aggregation/expansion recognizes
rich citations but ignores MCP minimal page-local `items/files/provenance`
bindings. Decode each page independently, preserve file hashes, original document
coordinates, provenance and truncation, and fail closed on invalid bindings or
out-of-scope evidence. Aggregated facts must be self-contained, without dangling
page-table indexes. Do not invent native symbol IDs omitted by compact replies.
Audit all four call sites and `expand`; avoid extra queries/source reads.
`crates/airk-mcp/src/lib.rs::ParseArgs` currently advertises unconstrained nested
JSON (`plan` schema is `true`); advertise object-valued nested inputs while keeping
canonical kernel validation. Add actual-handler compact/rich chained-plan tests
and native/CLI transport regressions, not just renderer fixtures. Relevant existing
tests are in `crates/airk-mcp/src/workbench.rs` and kernel Workbench tests.

Build/test prerequisites remain provisioned in ignored `target/` and `artifacts/`.
Add the contributor Cargo directory to PATH per process; use pinned nextest
0.9.148 with the unchanged `ci` profile, then serial doctests. On this host, set
`AIRK_PRIVATE_CENSUS_BASE` from ignored `artifacts/v190/sandbox-fixture-base.txt`
for native ownership fixtures. Read that path only locally; it is not a portable
configuration. Set process-local Git `safe.directory` as needed. Match the sandbox
identity to the sealed runtime/fixture ACLs; never relax existing user ACLs.

## P1 — compact-plan correctness

Resumed on 2 November 2026 from clean, signed commit
`3021995d272f3d95487dd0d781920febe7c4ca50`. P0a/P0b remain accepted; the 1.9.0
version increment is not repeated. Installed navigation MCP still runs 1.8.7
against the configured repository collection. Its indexed evidence was checked
against live affected sources before edits.

New actual-handler regressions first failed on both captured defects: compact
plans returned zero cited evidence and skipped dependent reads, and `parse.plan`
advertised the unconstrained schema `true`. The shared kernel consumer now
resolves each minimal page independently, preserves exact hash/original location,
inlines file/provenance descriptors for aggregation, and rejects malformed or
out-of-scope pages without retaining a leaking raw result. Rich reads keep their
existing path. Aggregation decodes once per result; no extra query, source read,
model work or indexing is introduced.

The native envelope uses optional object maps for plan/recipe/budgets/output;
kernel contracts still validate nested fields. `unique_definition_path` also
supports a subsequent symbols/code-facts lookup. This provides a visible,
budgeted rich locator step before `selected_symbol_ids` when compact rows omit
IDs. No IDs or semantic facts are synthesized and no hidden lookup is performed.

Focused evidence so far: actual MCP handler compact/rich dependent plans and
schema tests pass; kernel Workbench suite passes 21 cases (one existing ignored
measurement); extension passes 542 with 19 skips. Actual native CLI/stdio tests
also pass the compact/rich three-read plan and compare complete step results,
arguments and evidence with CLI parse and tools-call paths. The first transport
comparison caught fixture source work changing running/idle freshness between
calls. The test now waits for that initial job to settle and retains freshness
in equality checks instead of omitting it. Initial assertion/fixture failures
and corrected logs remain separate. The final acceptance results follow below.

Recovery remains a source revert; no persistent schema, profile, source policy,
normal runtime installation or grant changes occur in this phase.

### P1 validation receipts

- `cargo fmt --all --check` and all-target/all-feature workspace Clippy pass,
  including the final transport assertions.
- `python -m unittest discover -s scripts/tests -v`: 307 tests, six skipped,
  zero failures/errors. No Python implementation changed in P1.
- `npm test --prefix packages/vscode`: 542 passed, 19 skipped, zero failures.
  Installed-editor/native lifecycle qualification remains distinct from those
  skipped cases.
- Full `airk-cli --test mcp`, all features, two test threads: 11 passed after
  strengthening equality to complete step reports, step counts and envelope fields.
  Only documented request/cache observation timing and step elapsed time are
  normalized. Freshness state, coverage, evidence, errors and arguments remain
  compared. This targeted rerun follows the earlier complete workspace build;
  it does not hide or replace any workspace failure.
- `ci-local.ps1 -SkipBuild -RuntimeDir artifacts/v190/p1-runtime -Repeat 1
  -Suites @('smoke','clients')`: both suites passed with four logical CPUs, no
  load workers; smoke had seven passed cases. This is one focused repetition,
  not the five-repeat pre-PR campaign or installed-editor qualification.
  Original logs: ignored `artifacts/ci-local/20261102-161924-44382882/`.
- Final runtime builder receipt (`artifacts/v190/p1-final-runtime/airk-build.json`)
  binds 751 native inputs to
  `211cb59e35c39cb6ada723e9ebb771340c0c16facbe8e18655250a1eb63fd96b`.
  Main SHA-256: `21de22cae5c572a573cc616a4283b793490e9618b6a56ce82b94f5138e667901`;
  helper: `fd2bec7ddb6d9c9826f7beb6d51df914930e6415876d5470467696dbe404addc`.
  Both executable hashes exactly match the earlier pair used by the staged
  client checks. The final builder rerun accounts for transport-test-only review
  changes in the input inventory; nothing was manually relabeled or installed.
- Repository checks: 1,498 public files; version agreement, affected Markdown
  links/anchors/fences and whitespace checks pass. Final ledger/release-record
  edits will receive those checks again before commit.

The workspace run uses the unchanged pinned nextest CI profile and qualified
private fixture base: **1,843 passed, 35 skipped**, zero failures. Serial workspace
doctests pass (zero cases defined). The four existing workload exclusions and
31 declared ignores remain unchanged. The final transport-test-only strengthening
also passed the complete 11-case CLI/MCP target and Clippy separately.

**Uncontended acceptance:** 75 graded queries completed without failures; every
mode/split nDCG@10, Recall@10 and negative-query outcome exactly matches both P0a
and P0b. No-op and one-file edit extraction/publication guards pass. The captured
two-step compact workflow now completes with one cited record and three calls,
matching its rich equivalent; previously it had no cited records and skipped its
dependent read. Its actual native schema is now `type: [object, null]`, not `true`.

The legacy MCP sentinel completed 90 trials/810 calls at concurrency one, with
30 samples for each connection cohort and zero errors/retries. Coverage, source
identity, publication and cleanup guards pass. Task-with-connection median/p95
(ms): status-first **1,036.549 / 1,151.170**; unprimed **1,043.929 / 1,118.155**;
reused **959.387 / 1,023.786**. Both task-only and task-with-connection median/p95
limits pass independently against the frozen 1.8.7 baseline and previous P0b
receipts. No build/test campaign or source edit ran during these measurements.
This is preservation within the frozen decision bounds, not a speedup claim.

Receipt SHA-256 values:

- Relevance: `1e05d4e37ac520d07b56126a98c85de449254b299948902969d87e8ca77e52ad`.
- MCP: `a8953904b414b77c5d460335f9521d450ee5023b862c2e30417913b382098177`.
- Final builder: `8f6f02341f13d1780f672dc041faa4bbf043f1d49b1909203411a4204fa369da`.

**P1 review:** explicit implementer self-review covered the complete code/docs
diff, canonical callers and consumers, earlier baseline/version changes, table
locality, scope/revision refusal, byte/fanout bounds, missing-ID expansion, actual
transport equality and receipt identities. The review strengthened complete
step/envelope comparisons and corrected overstatements about minimal fields in
the current MCP guide. It was not an independent review. README, documentation
index, candidate record and Marketplace description agree on implemented behavior
and planned features. Final repository/link/version/whitespace checks precede the
human DCO-signed checkpoint; an ordinary source revert remains its recovery path.

P1 is accepted. Resume with P2a after its signed commit. Physical I/O, whole-service
peak resources, other platforms, external-client schema-cache refresh and
installed-editor acceptance remain outside these observations. Later indexing,
verification, combined-query and release phases retain their separate gates.

## P2a — independent reply projection

Entry: clean, human DCO-signed `d9eff91f0bdf0d0c3284d59581df4e194783a0d9`.
The new actual-handler regression first failed because `reply_detail` was not an
accepted native argument. Core now owns the optional Lean/Rich enum and legacy
conflict resolution, with no new dependency or storage/profile change. Explicit
choices select existing per-tool projections; absent arguments preserve prior
defaults and encoding. The native catalog and tool-help schemas intentionally
gain the new input; default evidence replies gain no null placeholder field.

CLI aliases, tools-call/shell, parse defaults/per-step overrides and Workbench's
dynamic forms use the same native choice. Effective plan choices participate in
existing digest and representation fences. Structured details, session drafts and
full/selected exports retain explicit choices. Static documentation/help has no
alternative evidence projection and retains its payload. The existing limited
Strategy converter refuses unsupported fields instead of silently dropping a
choice; native exports preserve those plans.

Focused results so far: 84 MCP cases passed; the expanded retention/projection
matrix verifies exact legacy equivalents and cursor fences. All 11 CLI/MCP
process cases and three tools cases pass, including a real tools-shell request.
CLI parser/contract tests pass and regenerate the affected reference. Extension
checks pass 543 cases with 19 skips. Native fallback minimal tables are also
recognized before MCP normalization. Complete workspace validation, source-bound
runtime and uncontended comparisons remain pending.

The final focused Core/Kernel checks pass: the Core policy matrix and 22 Kernel
Workbench cases (one existing ignored measurement). Nullable projection flags
retain Default semantics; null per-step choices inherit a supplied plan default.
The slash parser and advertised boolean unions agree. Recipe execution and an
explicit rich step inside a Lean plan also pass. CLI request-file/flag conflicts
reject rather than overwriting a stored choice. The public help route validates
the common input without broadening the UI adapter's strict schema or querying
an index. All these new cases have also passed inside the still-running workspace
suite. Formatting, final all-target Clippy, 307 Python tests (six skipped), 543
extension cases (19 skipped), version/repository checks (1,500 public files) and
affected Markdown links/anchors/fences pass. Final workspace/doctest, runtime
and uncontended performance/quality results remain required before acceptance.

No normal installation, grant, persistent schema or retained profile changes
occur. Recovery is a source revert before later state-changing phases.

Final self-review caught and corrected a plan mismatch before acceptance: Rich
search must not imply full chunks. The new regression first failed against the
initial implementation (`p2a-search-review-red.log`), then passed across all
Lean/Rich × absent/false/true `full_chunks` combinations. Search expansion is now
independent, with no change to hit or byte budgets. Native Workbench and actual
CLI/MCP tests also cover that boundary. All 84 MCP cases pass after correction.
The initial staged pair passed one four-CPU smoke/client repetition; it is retained
as superseded evidence, not the final runtime. A fresh source-bound pair and
post-review Clippy/transport checks are required before measurement and acceptance.

The workspace CI run completed with **1,847 passed, 35 skipped**, zero failures;
its unchanged exclusions remain four existing long workloads and 31 declared
ignores. Serial workspace doctests pass (zero cases defined). The post-review
84-case MCP library, corrected native Workbench regression and complete 11-case
CLI/MCP process target pass. Formatting and all-target/all-feature workspace
Clippy pass again after review. A new Kernel assertion initially expected a
focused excerpt to be complete; its preserved failure showed honest upstream
text truncation. The corrected assertion preserves that partial status and passes;
no production completeness guard was relaxed. Complete Core/Kernel library
rechecks and final source-bound runtime acceptance remain pending.

The `p2a-final-runtime` builder correctly refused publication when the test-only
assertion changed during its paired build. The subsequent stable build staged
`artifacts/v190/p2a-reviewed-runtime`; its source inventory matches live native
inputs. The initial runtime and refused attempt are superseded and not used for
acceptance measurements. The extra unfiltered Core/Kernel run passed all 58 Core
cases and reached only the existing long catalog-measurement workload; that
redundant workload was stopped without claiming a completed Kernel suite. The
unchanged nextest CI profile now supplies the complete post-review library check.
Final staged smoke/client checks are also running. No source/doc edits or other
build/test campaigns will run during the following frozen measurements.

### P2a final acceptance — 2 November 2026

All preceding pending results are resolved by these final observations:

- `cargo fmt --all --check` and `cargo clippy --workspace --all-targets
  --all-features --locked --offline -- -D warnings`: passed after final review.
- Workspace nextest, unchanged `ci` profile: **1,847 passed, 35 skipped**.
  Serial `cargo test --doc --workspace --locked --offline`: passed, zero cases.
- Post-review `cargo nextest run -p airk-core -p airk-kernel --lib --all-features
  --locked --offline --profile ci`: **204 passed, two skipped**, no failures.
  Those are the existing ignored role measurement and excluded large-catalog
  workload; no filter or test assertion was weakened. The interrupted extra
  unfiltered attempt is not counted as a completed suite.
- `cargo test -p airk-mcp --all-features --locked --offline --lib`: **84 passed**.
  Final `airk-cli --test mcp`, all features, two test threads: **11 passed**,
  including full result comparisons and independent search expansion through
  actual CLI aliases, tools-call, MCP and canonical Workbench handlers.
  The three tools-shell cases also passed in the initial focused validation.
- Python unittest discovery: **307 run, six skipped**, no failures. Extension
  compile/tests: **543 passed, 19 skipped**, no failures. The tests cover forms,
  sessions, explicit choices in full/selected JSON/Markdown and Strategy refusal.
- Final staged `ci-local.ps1 -SkipBuild -RuntimeDir
  artifacts/v190/p2a-reviewed-runtime -Repeat 1 -Suites @('smoke','clients')`:
  **two suites passed**, seven smoke cases, four logical CPUs, zero load workers.
  Logs: ignored `artifacts/ci-local/20261102-180232-9860d886/`. This is not the
  five-repeat pre-PR or installed-editor/installer qualification.

The final builder binds **752 native inputs** to
`1c135997269705091eb5565a9eb86dc1de3b858f4407dd20dcf008424bd4b28b`.
Main executable SHA-256:
`07e0fd843826eb52d8e4482bb16bde00d3564793430a48178ad18f06e34edc5e`;
helper: `86e5e5aea7b706d5e892885bf1cb92ddd2be8c1222cd457617e98eaf3fc20c7a`.
The builder inventory was independently recomputed from live files after staging.
No tested binary was relabeled or installed into the ordinary user profile.

**Uncontended quality:** 75 attempts pass. All mode/split nDCG@10, Recall@10,
negative outcomes and scored/failure counts exactly match the frozen baseline
and P1. Latency is compared separately, not mistaken for score equality. Both
compact/rich dependent probes complete with one cited record and three calls.
No-op indexing extracts/publishes zero changes and preserves its token; one-file
edit extracts/commits/publishes exactly one change. Corpus/model/binary and source
identity guards pass.

**Uncontended MCP sentinel:** 90 trials, 810 calls, zero errors/retries; required
coverage, publication consistency, cleanup and unchanged source pass. Thirty
samples per concurrency-one cohort. Task-with-connection median/p95 (ms):

| Cohort | Median | p95 |
| --- | ---: | ---: |
| Fresh, status first | 1,048.240 | 1,122.529 |
| Fresh, unprimed | 1,031.802 | 1,100.633 |
| Reused, status first | 965.273 | 1,025.928 |

Task-only and task-with-connection median/p95 limits pass against **both** frozen
1.8.7 and P1 under the original tolerance. All 810 matched default response byte
counts equal P1. The discovered input catalog intentionally grows from 61,303 to
67,002 bytes (**5,699 bytes**) to advertise the optional controls. No source/doc
edit, build, test or other measurement campaign overlapped these measurements.
These observations establish preservation within the frozen bounds, not a speedup.

Receipt SHA-256 values:

- Relevance: `41ca3a938233586a340cea43885c6c3601c7294f6c8a72a047010118c1986373`.
- MCP: `d24868c15eb92a0ff15a8f103dac8a4e779d902489098c805cf03c8768cb2d2d`.
- Builder: `43844671fb3d62e3cca0fb359788e390ccff2faa41e6a7799b2cdd331afaee43`.

**Review:** explicit implementer self-review covered the complete phase diff and
prior P1 consumers, canonical native handlers, default/nullable controls, schema
and CLI alias catalogs, prepared digests/cursors, minimal locations, session and
export preservation, limits and test/receipt identities. It caught and corrected
the Rich/full-chunk mismatch before acceptance. This is not an independent review.
README, documentation index, current guides, candidate notes and Marketplace
README agree on 1.9.0 and implemented behavior. Final repository, version, affected
Markdown links/fences and whitespace checks precede the human DCO checkpoint.

P2a is accepted; next is **P2b, existing-observation reasons and guidance**.
Source revert remains recovery before later state-changing phases. Physical I/O,
whole-service peak CPU/RSS, other platforms, external-client schema caches and
installed-editor acceptance remain unqualified. The later phase gates still apply.


## P2b — existing-observation reasons and guidance

Entry: clean, human DCO-signed `94f544a52ac8fc2df5cd39457882edca5553b5e9`.
The native missing-evidence regression first reproduced absent guidance after its
fixture import was corrected. Core now derives a bounded shared reason/action
vocabulary from returned metadata only. Native handlers and Kernel Workbench
fallback share the projection. There are no new query inputs, storage profiles,
source reads, registry queries, coverage scans, extraction/model calls or mutations.

Advice retains subjects and basis fields; empty records never establish support,
retention, exclusion or source absence. Essential native retention and explicit
provider imports remain distinct. Source-watermark lag is not per-file source
verification. Capability advice applies only to one explicitly selected adapter.
The family inventory/effective-profile work remains P3/P11a. Suggestions are inert
and preserve the original scope, publication, representation and limits.

Initial tests pass the Core reason/boundary matrices, four focused native cases
(including the two new actual-handler fixtures), all 86 MCP library cases before
final review, and 544 extension cases (19 skipped). Native fixtures cover selected
adapter flags, Essential retention, vector exclusion, truncated/empty results and
pending/failed/lagging fenced source-cache observations. Structured/full/selected
export coverage passes. Complete real-transport, final workspace, runtime and
uncontended comparisons remain pending. No phase acceptance is claimed yet.

Review found a transport-fixture race: the specialized-tool comparison read once
while source work was pending and once after idle. The original 10-pass/one-fail
result is preserved in `p2b-transport-focused.log`. The fixture now waits for idle
before comparing; it preserves complete freshness/readiness states and new
qualifiers rather than deleting advice. That correction passed all 11 stdio/CLI
cases and three tools cases, including shell guidance. The sibling durable-key and
change-validation comparisons were also audited: their shared helper now retains
freshness/readiness state, watermarks, failures and work identity, normalizing only
the request/observation identity. Those fixtures now wait for idle explicitly;
a final complete transport run is pending. No production scheduling/deadline or
completeness assertion was relaxed.

The final vocabulary also distinguishes explicit source unavailability. Advice
remains per native read step; aggregate plans keep their own completeness fields
and do not acquire a second inferred verdict. The current coverage guide now uses
the actual service-backed CLI status/inspection commands rather than obsolete
coverage aliases. Runtime staging will start only after final native review, so
its receipt binds the accepted implementation and transport tests together.

Final focused validation after the sibling audit passed all 11 CLI/MCP and three
tools cases. Final formatting/all-target Clippy and Python checks pass; the full
workspace CI run is still passing in progress. The initial staged P2b pair passed
one four-CPU smoke/client repetition (logs in ignored
`artifacts/ci-local/20261102-183900-1500687e/`). Final review then added native search
and capability catalog wording for the new guidance. That description-only change
requires a fresh builder receipt and focused catalog/transport checks; the initial
pair is retained as superseded, not used for acceptance measurements. No retrieval,
retention, budget or advice algorithm changed after the initial full-suite build.

A final trust/wording review also distinguished empty primary rows from
supplemental definition pointers and scan continuations from guaranteed further
matches. The new supplemental-pointer fixture preserves the pointer unchanged;
unknown advice does not claim a complete absence. Passage advice is conditional
on a citation being available. These are message/qualification corrections, with
no new reads, changed query selection or budget/ranking behavior. The preceding
`p2b-final-runtime` is superseded; the qualified receipt will bind this final wording
and fixture. Frozen measurements have not started and no earlier attempt is
being presented as final acceptance.

Final functional results: workspace CI **1,851 passed, 35 skipped**, with unchanged
four workload exclusions and 31 declared ignores; serial workspace doctests pass
(zero cases). Final formatting/all-target/all-feature Clippy, both Core guidance
tests, **86 MCP** and **14 CLI/MCP/tools** cases pass after the last wording review.
The final extension run passes **544 cases, 19 skipped**; it also displays the
native read-guidance vocabulary/limits in structured details. Python remains
**307 run, six skipped**, no failures. Full workspace preceded the final
message/catalog-wording-only changes; the affected complete native targets and
Clippy passed again afterward. No query execution, scope, source work or selection changed.

The qualified pair in `artifacts/v190/p2b-qualified-runtime` is staged and its
native source inventory matches live bytes. Final staged smoke/client checks
and uncontended quality/latency/byte comparisons are next. All earlier runtime
pairs are superseded for acceptance. Source/doc edits, builds, tests and other
measurement campaigns will stop before the frozen measurements begin.

### P2b final acceptance — 2 November 2026

The final acceptance runtime is **`artifacts/v190/p2b-qualified-runtime`**. Its
753 native inputs hash to
`c592ba4f98a4b7f0e4eb01e3547d88da531a871594d14a315d3c8b39b2f0725d`.
Main executable: `46a08c5c2bcf76399e7bd0c9964fdc04c87b587ef0d9463b24aa41568471613e`;
helper: `7fa95d2d50d4df43cae5d4b1c6ab7f17b98c2aec334512ac8361b2d3a26c4eb2`.
The source inventory was recomputed against live native bytes after staging.

Final functional evidence:

- Unchanged workspace nextest `ci` profile, all features/locked/offline:
  **1,851 passed, 35 skipped**, zero failures. Serial workspace doctests pass
  (zero cases). The four existing workload exclusions and 31 declared ignores
  remain unchanged. This full build preceded the final message/catalog-only
  refinements; final affected-target runs below cover those changes.
- Final `cargo fmt --all --check` and workspace all-target/all-feature Clippy,
  warnings denied/locked/offline: pass. Both Core guidance tests, all **86 MCP**
  library cases, **11 CLI/MCP** process cases and **three tools** cases pass after
  final review. Complete responses retain readiness/freshness state and guidance,
  including real public MCP, aliases, tools-call, tools-shell and parse steps.
- Python discovery: **307 run, six skipped**, no failures. Final extension
  compile/tests: **544 passed, 19 skipped**. Structured vocabulary/observations,
  sessions and full/selected export tests pass without adding query actions.
- Final qualified `ci-local.ps1`, `-SkipBuild`, `-Repeat 1`, suites smoke/clients:
  **two suites passed**, seven smoke cases, four logical CPUs, zero load workers.
  Logs: ignored `artifacts/ci-local/20261102-190855-a631255c/`. This is one focused
  repetition, not five-repeat pre-PR or installed-editor/installer qualification.

**Measurement correction retained:** the first successful quality/sentinel runs
were chained after `ci-local.ps1` in the same PowerShell process. That script
changes its process affinity, inherited by later children. The parent affinity
was restored, but the exact handoff to those campaigns could not be established;
therefore `p2b-relevance.json` and `p2b-mcp/` are **unmatched, not acceptance
receipts**, regardless of their metrics. The scoped process inspection initially
hit a sandbox WMI access restriction; a permitted read-only inspection identified
only the owned campaign. No normal user process or installation was changed.
No metric-driven rerun or tolerance change occurred.

Both campaigns were repeated serially from a fresh shell, with **64 logical CPUs
and unrestricted affinity `-1`** observed before starting. The affinity receipt
and final reports are separate; no build/test campaign or source/doc edit
interleaved with them. Other applications/OS caches remain uncontrolled as in the
baseline. The accepted files are `p2b-qualified-relevance.json` and
`p2b-qualified-mcp/replay/report.json`.

**Quality:** all 75 attempts pass. Every mode/split nDCG@10, Recall@10,
negative-query outcome, scored count and failure count equals **both** frozen
1.8.7 and P2a. Timing is compared separately. Compact and rich dependent probes
both complete with one cited record and three calls. No-op/edit work guards pass:
zero extraction/publication with unchanged bytes; exactly one extraction,
committed change and publication for the controlled edit. Source, corpus, model,
binary and publication bindings pass.

**MCP:** 90 trials/810 calls, zero errors/retries, full required coverage,
publication consistency, cleanup and unchanged source. Thirty samples for each
concurrency-one cohort. Task-with-connection median/p95 (ms):

| Cohort | Median | p95 |
| --- | ---: | ---: |
| Fresh, status first | 1,040.013 | 1,110.695 |
| Fresh, unprimed | 1,024.519 | 1,091.216 |
| Reused, status first | 963.903 | 1,015.524 |

Both task-only and task-with-connection median/p95 gates pass against **both**
frozen 1.8.7 and preceding P2a with the original tolerances. This is preservation
within the decision bounds, not a speedup claim. The intentional native catalog
wording grows discovery from **67,002 to 67,170 bytes** (+168). Matched per-call
response growth versus P2a: symbols/outline **+360 bytes**; search **0 or +427**;
references, code facts, read-file, context and status **0** in this sentinel.
These are measured fixture outcomes, not a bound for every response; at most eight
fixed-text observations are emitted and advice omission remains disclosed.
No extra calls or retries were needed to provide the explanations.

Receipt SHA-256 values:

- Qualified relevance: `9a202a69966edf7a66cbe8ea79692b08fc472b54d78c6a417039c42ed8aac8c2`.
- Qualified MCP: `c3f5f4bd54477ec269ce2fb6f6ee9c5289041fa2d9c97740243501247834cefa`.
- Qualified builder: `8e18920fed3a7e452826bfeb73d61933b1715060757039074153a0d105f16f74`.
- Fresh-shell affinity: `163f8351751d5df8a230d923735a29ee9b776f208ff2f1454c9c4104bc328bfd`.

**Review:** explicit implementer self-review covered the entire phase diff,
canonical native rendering, Kernel fallback, static capability catalog,
CLI/public aliases, status cache/unified wrappers, prepared plans, preservation
of qualifiers and source bytes, bounded projection work and complete/selected
exports. Cached/unified status retains the entire body and original observations.
The review corrected source-work comparison races and advice overstatements about
empty primary rows and continuations. It was **not an independent review**.
README, documentation index, architecture/usage guides, candidate notes and
Marketplace README agree on current 1.9.0 behavior. Final repository/version,
affected Markdown link/anchor/fence and whitespace checks precede the human
DCO-signed checkpoint. An ordinary source revert remains recovery.

**P2b is accepted. Next: P3 design gates (ADR 002/003, scopes, profile-family
inventory, permanent-policy ownership, receipt/schema and migration/fault fixtures).**
No live registry/collection upgrade, profile migration, normal installation or
publication occurred. Physical I/O, whole-service peak resources, other platforms,
installed-editor acceptance and external-client catalog caches remain unqualified;
all later phase gates remain in force.


## P3 - design contracts and implementation safety gates

Entry: accepted P2b `1b623ceaedaaf551c076c59522e223f1a18a3e3f`.
Added ADR 002/003, the original versioned contract pack and executable design
checks, plus the legacy profile-family inventory. Reconciled the plan/feature
research, README, documentation index, candidate notes and Marketplace guide.

Decisions fix explicit Project/index selectors and granted-only discovery;
component-preserving scope unions and exact shared-index attribution; initial
fan-out one and bounded RRF; registry schema 12 with preserved immutable backing
identity; explicit owned U0-U6 activation/backup/import/recovery; permanent native
policy surviving collection purge; original-remainder plus one destination split;
and dated saved-byte receipts with explicit budgets, error/export semantics and
future member slots. Policy references must not alter the original owner digest.
Ordinary opens retain WAL rejection. Post-upgrade user writes prohibit automatic
backup restore. No new schema or policy was activated.

Validation: `scripts/check-v190-design.ps1` passes 41 positive/negative wire
fixtures across four draft-7 schemas, plus semantic shared-index/attribution,
partition/future-path, policy root, receipt association/count/hash/time checks,
24 workspace outcomes, legacy family references and U0-U6 coverage. These are
**design checks**, not runtime migration/old-reader or interface-parity evidence.
Repository checks pass for 1,512 public files; version check remains 1.9.0;
AGENTS/CLAUDE equality, affected Markdown links and whitespace are checked.

Native source inventory is still exactly 753 files,
`c592ba4f98a4b7f0e4eb01e3547d88da531a871594d14a315d3c8b39b2f0725d`,
matching `artifacts/v190/p2b-qualified-runtime/airk-build.json`. This documentation
and validation-script phase does not repeat native/extension suites or expensive
performance campaigns; accepted P2b evidence remains the preceding runtime gate.
No platform or performance claim is added.

Explicit self-review covered the complete design diff, live lifecycle/owner and
registry boundaries, schema examples, authority narrowing, error/budget contracts,
phase activation and recovery. This was not an independent review. One input
encoding mistake while updating the ledger was corrected from the unchanged HEAD
ledger before applying the intended text edits; no user content was discarded.
Recovery is an ordinary source revert. P4 is next; source verification remains
unimplemented until its end-to-end acceptance. No install, push or publication.


## P4 - saved-source verification in progress

The maintainer explicitly selected Report as the default on 2 November 2026,
superseding P3's original Off default. Evidence reads resolve omitted controls to
8 distinct sources, 16 MiB aggregate raw bytes and a cooperative 2,000-ms deadline;
Off remains an explicit no-hashing path. Metadata-only reads/plans skip capture.
The [guide](../source-verification.md) records controls, receipts and limitations.

Implemented, not yet accepted: bounded reference inventory; read-only native
streamed SHA-256 capture with policy, identity and publication fences; complete
dated receipts and required-check failures; native CLI/MCP/Workbench parity;
response-wide plan deduplication or explicit per-step policies; complete/selected
exports; Inspector/Checker native receipt reuse without duplicate hashing.
Native capture does no extraction, inference, indexing or publication work.
Existing source-work/global freshness retains its independent meaning.

Focused checks before broad qualification: Core 4 passed, native verifier 4
passed, Windows ingestion verification 2 passed, MCP 90 passed, extension 547
passed/19 declared skips. The latest additions are included in the pending full
workspace rerun. Workspace Clippy passed with all targets/features and warnings
denied. Python suite: 308 tests, six declared skips, zero failures/errors.

An earlier actual-CLI parity fixture used a public MCP short name for native CLI
tools-call; it correctly refused that name. The fixture was corrected to the
canonical CLI tool identity and passed. During the Report default transition,
three MCP tests failed: one still rejected Report without explicit budgets, one
compared response-wide verification with per-step verification, and one compared
receipts for deliberately different reply projections. Those tests now exercise
the new preset, equivalent policies, and the explicit Off compatibility path;
complete default Report receipt parity has separate coverage. No result fields
were removed to make transport parity pass; only documented observation timing
and transport request identities are normalized.

No staged P4 runtime or matched performance campaign has yet been accepted.
The new dedicated verification measurement compares default Report with Off on
original disposable small, medium, large and eight-source fixtures. The existing
frozen MCP workload now supports a recorded argument override without changing
fixture bytes, so explicit Off regression and intentional Report costs can be
reported separately. P4 is not committed; P5 has not started.


The first full workspace attempt is retained in
`artifacts/v190/p4-workspace-current.log`: 1,868 run, 1,861 passed, seven failed
and 35 declared skips. It exposed older
comparison helpers that normalized source-work timing but not the new dated
receipt timing, a generated CLI reference awaiting regeneration, and two legacy
small-output assertions. The comparison helpers now keep every receipt field
except observation times; the legacy 4-KiB clipping case explicitly tests Off,
while enabled response verification admits at least an 8-KiB output budget.
The dated warning points to receipt timing rather than embedding a second,
transport-varying timestamp in prose. Qualification reruns remain pending.
The audit also rejects per-step verification with an evidence-only view before
checking sources, and maps response-wide Require plan failures to complete
nonretryable native errors rather than a successful transport envelope.


Corrected contract validation finished with 141 passed. The preceding focused
attempt had 139 passes and two fixture failures: a remaining comparison needed
explicit step scope, and the new scalar-sizing fixture had accidentally fitted
its allowance after the budget field shortened. Both fixtures now exercise their
intended contract. No receipt data is stripped from equivalent-policy checks.
Python's final suite passed 309 tests with six declared skips. The final extension
run passed 547 tests with 19 declared skips. A fresh source-bound release pair,
final workspace run, final Clippy/doctests and uncontended performance acceptance
are in progress; no P4 acceptance or commit is asserted yet.


The fresh staged pair is `artifacts/v190/p4-qualified-runtime`:
757 native build inputs, SHA-256
`396c65041da5fd6ecb9659bded179cf3b1f8759c7bc05aa5e89c1e80a0052088`;
main `e4209d771049a5660d4c94a3d782b284a0c816172b09406623046513f42a079e`;
helper `7110903086bc4d750b82553cd8b0fa8212ac42912f8da9ab005eba9d8835c469`.
The builder verified unchanged inputs across both builds. Final workspace Clippy
passed with all features/targets and warnings denied. Serial doctest checks
completed successfully for eight targets, each with zero executable examples.
The staged `ci-local` smoke and client suites both passed (one pass each, four
logical processors, no synthetic load workers); this is not installed-editor,
macOS/Linux, remote-filesystem or concurrent editor-save qualification.

The qualification harness additionally compares effective verification input
schemas and requiredness through actual native CLI discovery, public SDK MCP
and the canonical native Workbench catalog. Explanatory title/description
annotations are excluded; types, enums, bounds and defaults remain exact.
Before measurements, finish all tests/builds and preserve a fresh shell's CPU
affinity; the four-core smoke restriction is not a matched baseline condition.


The final workspace run passed all 1,868 tests with 35 declared skips. The first
verification-harness preflights failed before accepted measurement: an unnecessary
`rich:true` capability read returned the same direct-connection error in both
P2b and P4 private runtimes. Default capabilities succeeded in both and contain
the complete canonical Workbench input catalog (23,142 vs 29,358 serialized reply
bytes in the isolated small-source probe). This pre-existing rich-read limitation
remains unresolved; it is not claimed as successful transport qualification.
The harness now uses the full default catalog and bounded typed-read handling.

Later preflights exposed an initial pending management observation that settled
to idle, rather than verification-created source work. The harness explicitly
settles that observation before timed samples and records its warmup read count.
Preflight 7 completed all source/hash/count/byte/Require checks, 68 MCP/Workbench
and 72 CLI/MCP input-field comparisons, unchanged source-work observations and
clean frontend/service drain. One-sample timings are not acceptance measurements.
Its four actual native receipts validate against the canonical schema. File-info
can reference its whole result using the valid empty JSON Pointer; the unpublished
schema and positive/negative locator fixtures now cover that case and reject
non-pointer locators. Design validation now covers 43 wire fixtures.


### P4 performance investigation - 3 November 2026

The pre-router candidate's graded corpus completed 75 attempts with exact frozen
and P2b nDCG, recall, negative outcomes and source hashes. Explicit Off completed
90 trials/810 calls with zero errors/retries, exact frozen fixture identity,
complete coverage/cleanup and no verification receipts. Several original latency
gates missed narrowly. A matched P2b diagnostic run was also slower than its
historical record, but this did not waive the frozen limits. One confirmation
campaign was declared in advance and combined with every original Off sample
(60 per cohort); no failures were discarded and no thresholds changed. Combined
unprimed/reused cohorts passed; fresh status-first median task time 1,057.47 ms
still exceeded the frozen 1,045.04-ms and historical P2b 1,046.74-ms bounds. Its
with-connection median also failed. P4 remains unaccepted and uncommitted.

Default Report separately completed 90 trials/810 calls without errors/retries;
all 720 evidence reads had verified receipts, totaling 746,010 logical raw-source
bytes. Median nine-tool batch times were 1,640.87 / 1,679.00 / 1,638.53 ms for
fresh primed / fresh unprimed / reused connections, versus Off's initial
1,048.23 / 1,069.71 / 1,021.44 ms. This is intentional enabled-check cost,
not unchanged default performance or physical disk-I/O qualification.

The dedicated 30-sample paired file-size campaign completed all input-catalog,
raw-hash, count/byte limit, Require, unchanged-source-work, provenance and cleanup
checks. Warm-cache median Off / Report milliseconds were:

| Fixture | Logical raw bytes per Report read | Off | Report |
| --- | ---: | ---: | ---: |
| Small file | 32 | 73.95 | 146.81 |
| Medium file | 65,536 | 79.41 | 152.19 |
| Large file | 8,659,200 | 77.62 | 152.90 |
| Eight referenced files | 304 | 259.35 | 337.30 |

All these measurements remain tied to the pre-router candidate's source/payload
identity above. Raw receipts: `p4-qualified-relevance.json`, `p4-off-mcp/`,
`p4-off-mcp-confirmation/`, `p4-off-pooled-gates.json`, `p4-report-mcp/`,
`p4-diagnostic-p2b-mcp/` and `p4-qualified-verification/` under ignored artifacts.
Other applications/OS caches remain uncontrolled; whole-process peak memory,
physical I/O, other platforms and editor-save responsiveness remain unqualified.

The cold-session audit found that each native direct client rebuilt identical
tool-router schemas. Native sessions now clone cached immutable built-in routes;
mutable route maps/schema changes stay caller-local. Kernels, grants/admission,
client family, runtime/source-work context and response observations remain
per server/session. The complete 92-test MCP suite passed, including independent
raw-definition equality, mutation isolation and separate project-kernel dispatch.
A new source-bound runtime and full CLI/Clippy validation are underway. Further
acceptance measurements must use that new pair; old timings are not relabeled.


The router-cache correction passed 92 MCP tests, including new route-definition,
mutation and separate-kernel isolation coverage; the complete CLI suite passed
412 cases with 11 declared skips. Formatting and workspace all-target/all-feature
Clippy passed with warnings denied. The previously complete 1,868-case workspace
run remains recorded above; unchanged extraction/storage suites were not repeated
for this MCP-only correction. Repository/version/whitespace and 43 design wire
fixtures passed again.

Replacement pair: `artifacts/v190/p4-router-runtime`, 757 native inputs with
SHA-256 `98cedcccab4f4010bda01f5262f235df7362e9fc8cb736c605eb45057867575b`;
main `e65fdd79a705788bbd247ff3f87bd343d4f75aa7373026a1505241ae685af39e`;
helper `29a96f8116f0c128562a97c9cbfae1d26a90b10e7160b09b0a567bf2654a76b4`.
No installable candidate or normal runtime was replaced. Replacement smoke and
performance qualification are still pending; P4 is not accepted or committed.


### P4 matched-control investigation and final error review — 3 November 2026

The router candidate completed staged smoke/client checks (2/2, four CPUs) and
75 graded retrieval attempts with exact frozen/P2b quality and source identity.
Its explicit-Off campaign completed 90 trials/810 calls without errors/retries.
Historical-point comparisons still missed: fresh status-first task median
1,049.39 ms exceeded frozen/P2b bounds of 1,045.04/1,046.74 ms; p95 1,158.83 ms
exceeded P2b's 1,151.38-ms bound. With-connection median 1,140.37 ms exceeded
the frozen 1,134.67-ms bound. These failures remain recorded, not relabeled passes.

Following the frozen baseline's noise rule, both exact reference payloads were
remeasured with the same fixture, model, concurrency and 64-CPU affinity. The
frozen 1.8.7 binary itself now measured 1,065.05 ms fresh status-first versus
950.04 ms archived. The preceding P2b binary measured 1,013.32 ms versus
951.58 ms archived. Against these current executions of both immutable reference
payloads, the router candidate passed all 24 original median/p95 task and
with-connection gates (all three cohorts). Thresholds remain 10%/25 ms for
medians and 15%/50 ms for p95; neither frozen JSON nor original receipts changed.
This is matched-control evidence of environmental drift, not a historical-point
pass or a general performance guarantee. Other applications/OS caches remain
uncontrolled. Reports: `p4-router-off-gates.json`, `p4-diagnostic-frozen-mcp/`,
`p4-diagnostic-p2b-mcp/`, `p4-router-matched-control-gates.json`.

Router default Report completed 90 trials/810 calls with zero errors/retries;
all 720 evidence reads were verified. Median nine-tool batches were
1,631.25 / 1,626.64 / 1,631.16 ms (fresh primed/unprimed/reused). The dedicated
paired campaign completed 30 samples per mode/fixture, all 140 actual input-field
comparisons and all hash/count/byte/Require/source-work/cleanup checks:

| Fixture | Raw bytes per Report read | Off median ms | Report median ms |
| --- | ---: | ---: | ---: |
| Small | 32 | 68.35 | 139.89 |
| Medium | 65,536 | 68.61 | 141.91 |
| Large | 8,659,200 | 69.20 | 140.04 |
| Eight sources | 304 | 249.36 | 332.69 |

Report's added checking cost is intentional and explicit; logical bytes do not
measure physical disk I/O. Whole-process peak memory, other platforms, installed
editors and concurrent editor-save responsiveness remain unqualified. Raw reports
are `p4-router-report-mcp/`, `p4-router-verification/` and
`p4-router-relevance.json`. These remain tied to the router payload identity above.

The explicit final review found that late plan projection correctly refused a
claim after removing a checked location, but direct/unified MCP could lose its
typed error. Both callers now retain `verification_projection_changed` and
`retryable:false`; an actual SDK regression exercises public/legacy aliases in
both routing paths. All 93 MCP tests passed. A replacement source-bound pair and
final qualification are in progress; earlier measurements are not relabeled.

The same review covers whole-result pointers: a removed result serialized as JSON
null now also invalidates its receipt, rather than being treated as an existing
checked location. The SDK test uses this whole-result case. The final focused
Core/MCP run passed all 168 tests; the four router campaign receipts also passed
the current canonical schema. The superseded `p4-final-runtime` build was stopped
and excluded after review changed its inputs; final evidence must name the
replacement `p4-projection-runtime` identity.

The error mapping was further narrowed to verification-specific refusals so that
unrelated malformed backend plans retain their existing connection-failure
boundary. A negative regression exercises both aliases. The intermediate
`p4-projection-runtime` build was also stopped/excluded after this review change;
only the final replacement pair can supply phase acceptance evidence. This is
an explicit self-review, not an independent review. It covers the combined
verification controls, source capture/policy/publication fences, bounded receipt
and plan projection, native CLI/MCP error contracts, Workbench/Inspector exports,
schema/fixture consistency and packaging/module lists. Earlier historical
performance misses and platform/physical-I/O limits remain open observations.

The combined final Core/MCP/CLI command initially omitted both the checked-in
`ci` profile and all-feature qualification. That default-profile run completed
568 tests: 566 passed, two failed, 11 skipped. The plan parity fixture exhausted
its bounded read retries with `selection_busy`; the history fixture did not
observe its expected expiry message under the unbounded parallel load. These
failures are retained in `p4-final-contracts.log`. The corrected complete run uses
`--profile ci --all-features --locked`, preserving the repository's four-thread
and native-service groups, assertions, deadlines and zero retry policy. No
product change or relaxed test assertion was made to address these observations.

The prescribed all-feature Core/MCP/CLI run completed 581 tests: 578 passed,
three failed, 11 declared skips (`p4-final-contracts-qualified.log`). It resolved
the history expiry observation but exposed: (1) independent readiness availability
and derived receipt-headroom differences in plan parity, (2) an acknowledged
status receipt on a verification publication refusal, and (3) a usage-attribution
fixture deliberately holding the writer while default Report attempted its
publication fence. These are retained failures, not accepted qualification.

The renderer now acknowledges observations only when verification produced a
reply rather than an error. A deterministic oversized-reply refusal exercises
this shared error path and requires the original session receipt unchanged.
The usage fixture explicitly selects Off for its single measured attribution
read under the held writer; exact client/count assertions remain. Verification
with native writers and refusal receipts remains covered separately. Plan parity
retains full native values and exact available-readiness equality. Only when
one independently timed readiness probe explicitly reports unavailable does it
validate the available typed ready/disabled fixture state and independently
recompute receipt headroom from each original projection before normalizing
those dated observations. Requested/effective work budgets, references, hashes,
publications, failure/accounting fields and all other evidence remain compared.
The focused native regressions and complete final suites must pass before build.

All four focused native regressions passed under the all-feature `ci` profile:
undelivered-observation refusal, full CLI/MCP dependent-plan parity, held-writer
Explorer attribution and watcher/restart reads. The explicit post-correction
review retains the prior connection-failure boundary, whole-result null fence
and complete attempted Require failures. Full Core/MCP/CLI qualification,
workspace formatting/Clippy, a fresh pair and staged smoke/client checks are now
running sequentially. Performance measurements remain idle until they finish.

The corrected complete Core/MCP/CLI run passed all 582 tests with 11 declared
skips (`p4-settled-contracts.log`, all features, locked dependencies, unchanged
`ci` profile). Formatting and workspace all-target/all-feature Clippy with warnings
denied also passed. No full extraction/storage suite repeat was added after the
MCP rendering correction; the preceding 1,868-test workspace pass remains recorded
with its distinct source identity. The final pair build and staged checks follow
these completed tests; no benchmark overlaps this build/test work.


### P4 final local acceptance — 3 November 2026

This checkpoint supersedes earlier pending P4 status; original failed runs and
excluded intermediate builds above remain historical evidence. Final native
build inputs: 757 files, SHA-256
`55304dfa8cb2c4c5248cb24f9d7d6563b7416ad9b4771720534bd895cc3c6710`.
Runtime `artifacts/v190/p4-final-qualified-runtime`: main
`ea240bfec7b318b7cd604e106ddd47c74e59f19677d22d60ae24d0d499f63147`,
helper `92dcef4ee40147ca45d7d7e75b0178e2c510456f74d6cc4aacfff1191e5e9576`.
The builder verified unchanged inputs across both optimized builds and staged
366 third-party notices. Smoke and client suites passed 2/2 at four CPUs,
zero load workers (`artifacts/ci-local/20261103-013213-0c59ba82`).

A fresh measurement shell confirmed 64 logical processors and affinity -1.
No builds/tests or repository edits overlapped the sequential campaigns. Off
and default Report each completed 90 trials/810 calls with zero errors/retries,
complete coverage, clean owned-service/frontend drain and unchanged sources.
Off returned no receipts; all 720 Report evidence reads had verified receipts.

| Connection cohort | Off median / p95 task ms | Report median / p95 task ms |
| --- | ---: | ---: |
| Fresh status-first | 978.91 / 1,054.11 | 1,536.99 / 1,621.42 |
| Fresh unprimed | 977.24 / 1,009.75 | 1,557.48 / 1,681.56 |
| Reused status-first | 946.29 / 1,120.09 | 1,563.47 / 1,752.44 |

Final Off passed **all 24 historical-point gates** against the unchanged frozen
and preceding P2b receipts, plus all 24 comparisons against the exact remeasured
reference payloads. Both task-only and with-connection median/p95 passed in all
three cohorts, using the original allowances. No baseline reset, pooled-sample
substitution or threshold change was necessary for this final candidate.
Earlier candidates' historical misses remain failures, not relabeled passes.
Raw evidence: `p4-final-off-mcp/`, `p4-final-report-mcp/`, `p4-final-gates.json`.

The final dedicated campaign passed all source/count/byte/Require/provenance,
unchanged source-work, cleanup and 140 actual input-field comparisons. Four
actual receipts validate against the current canonical schema. Thirty samples
per mode/fixture produced these warm-cache medians:

| Fixture | Logical saved bytes per Report read | Off ms | Report ms |
| --- | ---: | ---: | ---: |
| Small | 32 | 66.67 | 135.76 |
| Medium | 65,536 | 69.13 | 141.97 |
| Large | 8,659,200 | 67.00 | 137.39 |
| Eight sources | 304 | 245.17 | 322.57 |

The graded corpus completed 75 attempts with 84 exact frozen/preceding metric
comparisons (1e-12 tolerance), unchanged source hashes, negative-query outcomes,
source identity and executable hash. Reports: `p4-final-verification/`,
`p4-final-relevance.json`, `p4-final-quality-gates.json`.

Final affected qualification passed 582 Core/MCP/CLI tests with 11 declared
skips, formatting and workspace all-target/all-feature Clippy with warnings
denied. Earlier complete workspace (1,868/1,868, 35 skips), serial doctest,
Python (309, six skips) and extension (547, 19 skips) evidence remains recorded
with its exact checkpoint. Unchanged Store/extraction/extension suites were not
repeated after the final MCP-only refusal correction. Final pre-commit repository checks passed for 1,522 public files; native, extension
and lockfile versions agree at 1.9.0; all 43 design fixtures and whitespace checks
passed. AGENTS.md and CLAUDE.md remain byte-identical.

Report's extra work is an intentional default change requested by the maintainer,
not unchanged default latency. Logical saved bytes exclude policy/index metadata;
physical I/O, whole-process peak memory, installed editors, concurrent Windows
editor-save responsiveness, remote filesystems and macOS/Linux remain unqualified.
The pre-existing `rich:true` capability limitation above remains unresolved; the
full default native catalog and actual CLI/MCP/Workbench input parity passed.
No normal installation, production migration, push or publication occurred.
P5 begins from this accepted commit, with production activation disabled until P6.


## P5 — catalog and upgrade safety foundation (in progress)

Entry commit: `a425ba01d048bca3ee5a7521a20a7448abd07983`, accepted P4.
Core now defines prefixed random-identity wrappers, bounded metadata, positive
SQLite-compatible revision counters and the inert source-policy format. These
contracts activate no schema, source filter, permission or public capability.
Random IDs will be allocated by Kernel and persisted in the upgrade intent before
backfill so restart reuses them. No deterministic path-derived identity replaces
the approved ADR. Production migration stays disabled until P6; public combined
retrieval stays disabled until P7. P5 remains unaccepted while implementation,
fault tests, actual predecessor probes and performance checks are unfinished.


P5 foundation checks: the six initial Core format/predicate tests passed, followed
by six Store schema/isolation/barrier tests. Schema validation includes implicit
unique indexes and foreign-key DDL, rejects missing reverse indexes/extra triggers,
and bounds untrusted metadata in UTF-8 bytes before Rust string allocation.
Initial generated-DDL CRLF differences were normalized to LF; SQLite's empty-BLOB
substring returning NULL required an explicit empty-BLOB fallback. The corrected
Store run passed all six cases. Ordinary creation/reads remain schema 11; normal
schema-12 readers/writers refuse pending activation or unknown authority/policy
contracts, with exact database bytes unchanged on refusal. No production upgrade
has been initiated. U0–U6 ownership, backup/import/recovery and actual predecessor
probes remain implementation work, so P5 is not accepted.

### P5 migration, import and recovery checkpoint — 3 November 2026

The explicit Store adapter now holds the service, embedding, watcher and all
saved/discovered worker leases before migrating. A durable owner-field fence
refuses ordinary candidate reads and old readers before schema changes. Kernel
allocates random Project/index/source IDs and persists the exact intent before
backfill; restart reuses it. Unreadable private source configuration stays
`pending_metadata`, with only the original primary source known. Migration does
not scan source directories, initialize collection slots, extract or load models.

SQLite's existing pinned safe backup API now produces a bounded, integrity- and
foreign-key-checked schema-11 snapshot. File identity, complete-file SHA-256 and a
typed digest of every legacy table bind recovery to that snapshot. An unclaimed
backup file is preserved; at most four slots are attempted. Committed WAL is
included before SQLite checkpoints it. The adapter recognizes bounded WAL frame
checksums, commit boundaries and stale reset tails; it does not replay pages or
manually remove sidecars. Hot rollback-journal recovery requires the matching
saved migration/restoration intent, qualified backup and all ownership leases.

The additive migration and exact legacy Project mapping commit atomically. Resume
checks stored roots, aliases, membership atoms, defaults, counters and identity,
not just row counts. Legacy jobs, pause/fencing state, watermarks and receipts are
covered by the unchanged legacy-row digest. Context imports have explicit request
identities, revision checks and bounded selections. Replays do not recreate deleted
contexts; defaults outside the selected Projects are refused. Missing editor state
can remain pending without guessed grouping or access grants.

Readiness records bind a candidate observation to its installer identity and
catalog revision, plus predecessor refusal evidence. An installer-only CLI probe
returns the complete native observation and checks its running payload identity.
This is a metadata qualification path, not a public Indexing/query capability.
Production settlement remains disabled until P6. The underlying settlement logic
is exercised in isolated tests on both sides of activation and fence clearing.
Before the first settled write, restoration uses SQLite's backup API and touches
only the registry; after any settled new or legacy write, the durable barrier
requires forward repair and preserves new work.

Observed checks at this checkpoint:

- All **27 focused Store catalog tests passed**, including child-process exits
  leaving real committed WAL and a real hot rollback journal, worker exclusion,
  commit retries, authority changes, import replay and restoration boundaries.
- The first settlement run passed 25/27: two restoration cases exposed a Windows
  `sync_all` access error on a read-only file handle. Using an owned writable flush
  handle corrected the path; the complete 27-test rerun passed.
- Formatting, tracked whitespace checks and warnings-denied Clippy for Core,
  Store, Kernel and CLI with all targets passed at the preceding checkpoint.
  The subsequent actual CLI compatibility tests and Kernel preparation changes
  still require their own completed runs and final combined checks.

Actual retained predecessor/candidate process probes, the complete management and
overlap contracts, activation integration, capacity/fault expansion, full suites
and matched performance campaigns remain unfinished. Unit readiness digests are
explicit fixtures and do not count as actual runtime compatibility evidence.
P5 remains in progress and uncommitted; no normal installation or user state has
been upgraded, and no P6+ capability is advertised.

Subsequent P5 checks passed **30/30 focused Store cases**. Added cases preserve
unclaimed partial-backup sidecars, reject overlays on an immutable qualified
snapshot, refuse hot-journal recovery with a missing fence/wrong stage/changed
backup, and reach the actual aggregate **32 MiB** registry limit. The capacity
failure rolls back without evicting the existing index or legacy registration.
The first negative-journal test used a noncanonical Windows fixture path when
persisting its altered intent; that test setup was corrected to use the saved
canonical control root, leaving the ancestry guard unchanged.

The two actual CLI compatibility tests also passed, explicitly including the
normally ignored retained-runtime campaign. They exercised the source-built
debug candidate, frozen 1.8.7 runtime and accepted P4 1.9.0 runtime. Both retained
main/helper pairs were checked against their existing payload receipts. Original
schema-11 list/forget commands succeeded; after admission both reads and writes
reached the owner-format refusal. Removing only the fence field in an isolated
fixture forced both old payloads to reach and refuse the schema-12 floor. Control
files and the source/collection sentinel stayed unchanged on refusal. The actual
candidate's complete observation matched Store, a crossed payload identity was
refused, and old readers worked again after qualified restoration. Production
activation remained disabled. The initial transport run could not read held
Windows zero-length ownership locks while recording fixture bytes; recording
their verified empty length avoids reading the locked byte beyond EOF. The
complete two-test rerun passed. These debug-process observations do not replace a
fresh source-bound release pair, managed activation or installed-editor evidence.

Managed activation/rollback now checks explicit catalog capabilities before
changing the launcher for an activated schema-12 catalog. Equal application
version strings do not establish grant/policy compatibility. The legacy-registry
path does not start an extra executable probe. This additional integration and
its tests are under validation; the broader three-library regression run and
final workspace checks remain outstanding at this checkpoint.

The subsequent all-feature `ci`-profile library checkpoints passed **707/707
Core/Store/Kernel tests (5 skips)** and **303/303 CLI tests (7 skips)**. These runs
precede the latest observation/startup additions and are not final P5 acceptance.
Their ignored logs are `p5-foundation-libraries.log` and `p5-cli-libraries.log`.
The new service admission check preserves the existing diagnostic path for other
damaged registries, but propagates a typed pending-upgrade refusal before source,
helper or project supervisors start. The actual two-test CLI campaign passed
again with this startup assertion. Its initial run stopped earlier at the sandbox
temporary directory's owner-only requirement; the successful rerun used the
already qualified private fixture base, without relaxing permissions. The final
debug-process log is `p5-catalog-native-private.log`.

The Store catalog suite subsequently passed **31/31**, adding transfer of an
already held service lease into migration and back without releasing service
ownership. Normal admission remains fenced through that transfer. The next
native startup integration can therefore avoid a release/reacquire gap.

### P5 bounded source observations (implementation in progress)

Core/Store now have an explicit saved-root relationship read, independent of
evidence retrieval. It uses native path-component comparison (including Windows
ordinal case rules), retains inactive roots, applies explicit binding selections,
and binds pagination to the namespace, proposed root, selection and complete
bounded metadata snapshot. It never opens source or collection files. Empty or
unknown selections refuse; a changed cursor cannot broaden scope or restart at
page one. The metadata pass is bounded by 1,024 indexes, 17 roots each, 8 MiB root
metadata and a cooperative two-second deadline. Replies have independent row/byte
limits, disclose omissions and retain exact matching counts when the metadata
pass completed. Escaped JSON envelope size is included in byte admission.

Legacy watcher caches explicitly leave alias completeness unknown. Schema-12
observations retain stable index/source IDs, disabled retained roots and pending
source imports. Missing legacy-to-index mappings fail instead of hiding roots.
The **four focused Store relationship tests passed** at their checkpoint.

Kernel now implements explicit repository-marker observations for a root and an
optional sample of at most 128 direct child entries plus one lookahead. It does
not recurse or execute Git. A narrow Ingest adapter reuses protected no-follow
capture only for a root/direct-child `.git` file, with 4,097 bytes at most to
distinguish the 4 KiB limit. Reported `gitdir:` targets are not followed. Native
home, volume, conventional user-container, temporary and synced-folder hints use
bounded location comparisons rather than opening candidate locations. These are
hints, not a proof that an unflagged directory is safe or that a repository is
understood. Source bytes, extraction, models, jobs and registrations are unchanged.
The first three new Windows Kernel observation cases passed; reply/deadline
expansion and the marker adapter test are being validated. The Unix alias case
has not run on this Windows host. An intermediate Clippy run reported two nested
conditionals in the new hint classifier; those were simplified and await the
combined rerun.

These are internal native contracts. Administrative and configured/granted
observations must remain separate when actual CLI/MCP/Workbench routes are added;
an agent-supplied binding is not authority. Transport/UI parity, remaining managed
startup/upgrade integration, wider faults/capacity cases and performance campaigns
are still required. No new public Indexing or combined-query capability is enabled.

The next focused runs passed all five new Windows Kernel observation cases (plus
one existing resource-observation case selected by the same filter) and the narrow
Ingest marker-reader case. Expired requests perform zero marker captures/child
enumeration; reply-byte, directory-sample and deadline cutoffs stay distinct.
Workspace/all-target/all-feature Clippy with warnings denied then passed; its log
is `p5-observations-clippy-final.log`. A combined all-feature focused regression
run is in progress. The dated ADR 002 clarification records the explicit-read,
administrative-versus-agent and future prefix/policy requirements. Routine status,
Full status and configured `projects` retain their existing lightweight meanings.

The combined all-feature focused run completed: **55/55 passed**, with 934 cases
outside that selection (`p5-foundation-focused.log`). It includes the Core formats,
31 catalog cases, four relationship cases, five new Windows repository-hint cases,
the marker adapter and existing protected source-reader regressions.

The next actual CLI campaign passed **3/3**, including the explicitly selected
retained-runtime case (`p5-catalog-control-corrected.log`). An additional isolated
schema-12 transport fixture served a real `RequestEpoch`, shut down and drained
owned descendants, preserved the legacy pause setting and source sentinel, and
created no paused index collection. Its native startup writes set the no-restore
barrier; subsequent public Store restoration refused with forward repair and left
database bytes unchanged. This fixture manually seeds an activated format solely
to exercise ordinary native transport. It does **not** qualify managed activation
or bypass the distributed production gate; actual Store settlement/recovery has
the separate interruption tests above. The first compile of that added test used
Debug formatting for a response type without Debug; diagnostics now serialize
the complete response, and the corrected three-test run passed.

The richer preservation suite then passed **33/33 catalog tests**
(`p5-catalog-retention-qualified.log`). It now carries pending/running/failed jobs,
idempotent request receipts, a cancellation, source outcomes, manual watcher
watermarks and removal history through migration and restoration. Their complete
typed legacy-table fingerprint remains unchanged. A private collection fixture
also migrates its known alias while both source directories are offline, then
restores the registry without recreating those sources or changing its roots and
opaque saved extraction-policy bytes. This proves preservation; it does not claim
to interpret an unknown extraction-policy version.

Pending activation now also has a separate process-exclusion primitive. It pins
all saved/discovered worker, helper, watcher and service leases without opening
SQLite, so a drain/stop observation need not recover an interrupted journal.
Database readiness and migration/recovery remain distinct. Its hot-journal
non-mutation assertion and the factored ownership census await the next combined
check; managed activation wiring remains outstanding.

The combined preservation/exclusion run subsequently passed **57/57** focused
all-feature cases (`p5-retention-exclusion-focused.log`). A further relocation
audit found that the legacy all-table string rewriter could move a directory
before immutable catalog identities rejected its SQL. Schema 12 now refuses
that path before an intent or directory move; completed historical receipts
remain usable. Its first regression run found that opening a writable registry
before refusal still changed SQLite header bytes. The guard now selects a
read-only, fully validated refusal path before that open. All **3/3** relocation
cases passed (`p5-relocation-guard.log`). ADR 002 and the plan record the required
legacy-relocation-before-catalog ordering, without enabling general relinking.

Installer resume also checks its recorded payload identity **before** SQLite
rollback-journal recovery. The process-exclusion guard transfers the service
lease into the upgrade owner; subordinate ownership is reacquired and the intent
rechecked before opening the database. The hot-journal case passed with both
wrong-installer entry points preserving database/journal bytes and the matching
installer recovering and resuming (`p5-installer-identity-recovery.log`).

An internal native `runtime catalog-upgrade-migrate` command now owns U2/U3 for
an already fenced, exact upgrade. It validates the control binding, upgrade ID
and executable/pair identity, executes the canonical Store backup/migration and
returns its complete receipt. It cannot initialize, admit, activate or start a
service. This provides the candidate-process boundary needed by the managed
coordinator; it is not an Indexing or MCP entry point. The first new process-test
compile used equality on a receipt type without `PartialEq`; the assertions now
compare complete serialized values. The corrected campaign passed **5/5** actual
CLI cases, including explicitly selected retained 1.8.7/P4 predecessors
(`p5-native-owner-predecessor-full.log`). It covers unadmitted/wrong-ID/wrong-payload
refusal without mutation, exact migration replay without rebuilding, complete
probe data, actual predecessor refusal, restoration and the separate activated
format/startup write-barrier fixture. Production activation remains disabled;
managed orchestration, public observation parity and final P5 qualification are
still outstanding. A combined all-feature regression rerun is in progress.

That combined run passed **60/60** selected native cases, with 932 outside the
selection (`p5-native-owner-focused.log`). Workspace/all-target/all-feature
Clippy with warnings denied passed (`p5-native-owner-clippy.log`). Repository
checks passed for 1,548 public files, synchronized version 1.9.0 and all 43 design
wire fixtures passed. Extension tests passed **547**, with **19 skipped**
(`p5-extension-tests.log`). The first full Python run had one packaging provenance
error because that process lacked the repository's required per-process Git
safe-directory setting; no test assertion or repository identity was changed.
With the setting supplied, the suite passed **309 cases, six skipped**
(`p5-python-qualified.log`; initial failure retained in `p5-python-tests.log`).
An initially mistyped design-check command pointed at a nonexistent Python file;
the actual `scripts/check-v190-design.ps1` check subsequently passed. The full
native workspace suite is compiling/linking; its result remains pending. None of
these checks qualifies installed or non-Windows behavior, or accepts P5 yet.

The relocation cases additionally replay completed legacy receipts against a
schema-12 format fixture and verify unchanged registry bytes: **3/3 passed**
(`p5-relocation-completed-receipt.log`). The format fixture does not qualify a
catalog migration. Release license notices were generated for **366 packages**
(`p5-license-notices.log`); the existing pinned SQLite dependency enables its
backup API without a lockfile/version change. The dependency advisory audit could
not complete: its normal path could not lock the read-only Cargo advisory cache;
the cached/no-fetch attempt reached an unusable npm audit report. Both failures
are retained (`p5-dependency-audit*.log`); no clean advisory result is claimed.

Workspace doctest checks completed successfully for all eight crates (zero
executable examples in these targets, `p5-workspace-doctests.log`). Store
all-target/all-feature Clippy also passed after the additional completed-receipt
fixture (`p5-store-completed-receipt-clippy.log`). An explicit self-review of this
increment checked the native command's pre-admission/ID/payload refusal ordering,
service-lease transfer, the absence of implicit activation and the read-only
relocation guard. This is a local self-review, not independent or final P5 review.
README, documentation index, architecture, storage guidance and the candidate
record now consistently describe P5 as inactive and in progress; the existing
Marketplace feature claims remain limited to implemented behavior.

The full all-feature native workspace run then completed: **1,932 passed, 36
skipped**, including four cases excluded by the CI profile's default filter
(`p5-native-owner-workspace.log`, 65 binaries). The separately selected retained
predecessor campaign above passed its normally ignored compatibility case. The
full run compiled before the last completed-relocation-receipt fixture expansion;
that test-only expansion has its separate passing targeted tests and Store
Clippy result above. This is Windows regression evidence, not a resource campaign
or cross-platform/installed qualification. P5 remains unaccepted and uncommitted:
the managed installer coordinator and its recovery wiring, remaining authorized
observation/consumer integration, source-bound release-pair measurements against
the frozen/P4 controls and final phase review are still required. P6 has not
started, and catalog production activation is still disabled.

### P5 continuation: managed installer coordinator

This continuation keeps P5 open until its staged activation, regression and
performance gates complete. A passing intermediate test run is not a phase exit.

The installer now has a gated catalog coordinator after legacy directory
relocation and before activation completion, plus an earlier resume route before
ordinary readiness/legacy metadata reads. Disabled production gates return
without filesystem or payload work. The matching staged candidate performs the
already admitted migration in its own contained process; the installer validates
the complete returned receipt, probes actual candidate readiness and collects
actual predecessor registry read/write refusal evidence. Parser and connection
errors are not compatible refusal evidence. Existing context imports remain
explicit; missing editor state is not inferred into grants or workspace groups.

Review of this wiring identified the pre-fence admission gap and both U6
settlement/restore receipt gaps. Strict installer acquisition now verifies its
saved identity/namespace before SQLite access at all these boundaries, rechecks
under service ownership and preserves actual later writes. In particular, a
restore completed before its external receipt can finalize that receipt without
copying the snapshot over resumed legacy writes. The **33 catalog tests passed**
with these expanded cases (`p5-managed-restore-settlement.log`).

Native command output uses private temporary files, monitored byte limits and
owned child cleanup, avoiding blocked pipe-reader tasks. Complete output,
overflow refusal, timeout cleanup and disabled-gate tests passed at the first
targeted checkpoint. Windows containment-admission failure also now terminates
and reaps the exact spawned child; the forwarding path shares that ownership
adapter. Its process-handle witness test passed (`p5-containment-failure-corrected.log`);
the initial compile lacked a test-only trait import, now fixed. An intermediate
Clippy run flagged a redundant constant assertion in the new gate test; removing
it retained the actual zero-work assertions. The corrected workspace/all-target/
all-feature Clippy run passed (`p5-managed-coordinator-clippy-corrected.log`).

The broader affected regression run and the isolated staged-pair installer
campaign are next. The latter also covers U6 dispatch boundaries as explicit
format fixtures; those fixtures do not substitute for actual enabled UI/grant
cutover qualification in P6. Source-bound performance gates and final P5 review
remain required before accepting or committing the phase.

### Working identity changed to 1.8.8 — 3 November 2026

The maintainer requested **1.8.8** as the working/manual-test version, retaining
**1.9.0** for promotion after implementation and manual acceptance before CI and
Marketplace release. Workspace, eight local Cargo lock entries, extension and
both npm root lock entries now agree at 1.8.8. Extension TypeScript checks and
catalog generation completed; the native template catalog's only generated
change is `extension_version: 1.8.8`. The promotion-only release tooling's
increasing-version guard remains intact.

Current README/index, developer/user guides, version policy, plan and the new
[1.8.8 working-build record](../releases/release-1.8.8.md) describe this choice.
Earlier 1.9.0 records, artifacts and measured controls retain their exact original
identities. The just-completed `p5-coordinator-debug-runtime` is one such 1.9.0
build; it is not relabeled or used as a 1.8.8 qualification input. A new 1.8.8
pair is being built in `p5-coordinator-188-debug-runtime`.

The version audit found no 1.9-only feature check in the task/verification quality
harnesses or extension feature paths. Forward supported runtime lines retain the
planned 1.9 support and ordinary downgrade refusal. Extension tests explicitly
include 1.8.7→1.8.8 acceptance and 1.9.0→1.8.8 refusal; the native same-version
capability test now uses the actual compiled version. Storage/grant/policy
compatibility remains independent from the application label.

At this checkpoint, extension tests passed **547, 19 skipped**, and Python passed
**309 cases, six skipped** (`p5-working-188-*-tests.log`). Repository checks passed
for **1,550 public files**, all 43 design fixtures passed, version agreement passed
at 1.8.8, and whitespace checks passed. The latest pre-adjustment affected native
run passed **348/348** (462 outside that selection); its source/version remains
the original 1.9.0 checkpoint (`p5-managed-coordinator-regressions.log`). Fresh
native staged-pair and performance qualification for 1.8.8 is still pending.
Nothing was installed, pushed, dispatched to CI or published.

### P5 review boundary and installed-pin correction

The maintainer now authorizes completing P5, committing and pushing this branch,
including both files under `docs/issue-analysis/`, with **no PR**, followed by a
stop for review. P6 must not start. The working version remains 1.8.8.

The first actual staged installer campaign failed before migration: the ordinary
installed namespace validator correctly refused the pending catalog fence, which
also prevented the already admitted candidate from reaching its owned migration
handler (`p5-working-188-staged-coordinator.log`). Raw development fixtures did not
exercise this independent state-level namespace pin. The fix keeps ordinary
validation strict and adds a read-only installer path that verifies the saved
payload/namespace against the unchanged installed pin. It validates the pin again
under native exclusion. Corrupt or mismatched pins are preserved and refused;
there is no implicit repinning, SQLite recovery or directory creation. The new
installed-pin non-mutation regression passed.

The caller audit also found that ordinary lifecycle inspection precedes installer
resume. A gated owned-catalog route now precedes that inspection only for the
committed matching activation; disabled gates still perform no state work.
Catalog admission rechecks saved Stop and pending lifecycle state before creating
or arming an intent, so accepted recovery cannot bypass user stop policy. The
initial/pre-fence Stop and pending-restart regression passed all four cases.
The first edit had a missing delimiter and the early-resume wrapper initially
returned the wrong unit type; both compile errors were corrected before running
the affected suite. Source-bound staged-pair requalification and the final P5
performance/review gates remain pending.

The corrected debug staged campaign then reached every migration/readiness/U6
dispatch/restore step, but its last pause assertion found a fixture error:
`register_with_setup(..., Some(true))` enables watching; it does not pause. The
fixture now registers and explicitly pauses through the canonical API and asserts
that initial state before admission. The unchanged preservation assertions pass
with the retained debug pair. The final source-bound release pair was rebuilt
after that test-only correction; the earlier failed runs remain recorded.

The final **1.8.8 release** pair has **784 native inputs**, source SHA-256
`594438057cae526009b40e960ab443856bcdc6006c41ca9560379ff2175a6510`.
Main SHA-256: `2697af91b1cfe70936a2802e5711944001cdbb3984fa148c6d931318b193152f`;
helper: `31251b5569f201484c2948f97f5726d30b46477487c41a6f6946dae3e9df60c2`.
Its immutable pair, build record, approved inventory and 366-package license
notices are under `artifacts/v190/p5-188-qualified-runtime`. Both executable
version probes return 1.8.8. The actual release staged installer campaign passed,
and all **5/5** catalog transport cases passed against this pair and retained
1.8.7/P4 predecessor images (`p5-188-release-staged-coordinator.log` and
`p5-188-release-catalog-transports.log`). U6 dispatch's manually seeded boundary
states remain format fixtures; Store's actual SQL interruption tests provide the
separate cutover/restore evidence. Production activation is still disabled.

Final workspace/all-target/all-feature Clippy passed with warnings denied, and
all eight doctest targets passed with zero executable examples. The staged pair
also passed **2/2** local managed suites at four logical processors, one pass each:
native MCP smoke and clients (`p5-188-ci-local.log`). This is local Windows
evidence, not installed-editor or macOS/Linux qualification. The final full
workspace suite and uncontended performance/quality campaigns are still pending.

The final full workspace/all-feature nextest run passed **1,939 tests, 37
skipped**, across 65 binaries in 1,107.932 seconds
(`p5-188-final-workspace.log`). The skipped count includes the four default
profile exclusions; it is not an assertion that those workloads ran. Clippy,
doctests, the actual staged release-pair campaign, retained-predecessor transport
tests and the four-core managed smoke results above qualify this same frozen
native source identity. No native changes were made during the performance run.

The requested issue-research agent read both reports under `docs/issue-analysis/`
and integrated an ordered P6a–P6f remediation sequence into `plan-v190.md`, carrying
its regression obligations through P7–P14. The primary agent reviewed the plan
diff against the stop boundary: it changes future implementation/testing order,
not P5 acceptance or activation. Both original reports are included unchanged as
dated evidence. This review neither reproduces their installed failures nor
claims their fixes implemented. P6 implementation has not started.

### P5 final local acceptance — 3 November 2026

The frozen 1.8.8 release pair above completed the final uncontended campaign.
The measurement shell verified 64 logical processors and unrestricted affinity,
separately from the four-core smoke shell. No build or other heavy test ran
concurrently. Raw receipts remain under ignored `artifacts/v190/`; no thresholds,
corpus judgments, source files or binaries were changed to obtain a pass.

| MCP task cohort | Candidate Off median / p95, ms | Candidate default Report median / p95, ms |
| --- | --- | --- |
| Fresh frontend, status first | 922.94 / 967.85 | 1465.32 / 1539.37 |
| Fresh frontend, unprimed | 944.50 / 1012.14 | 1494.37 / 1549.06 |
| Reused connection, status first | 931.10 / 1015.52 | 1541.12 / 1577.15 |

Each candidate/control mode used three cohorts of 30 trials, nine calls per
trial: **90 trials / 810 calls**, zero errors/retries and complete consistent
publications/coverage with clean cleanup. Each default Report run returned
**720 verified receipts**; Off returned none. Off passed against the frozen
baseline and archived/contemporary P4 Off; Report passed against archived and
contemporary P4 Report. All **60** comparisons passed, covering task and
connection-inclusive median/p95 with unchanged allowances: median
`reference + max(10%, 25 ms)`, p95 `reference + max(15%, 50 ms)`.

Dedicated verification passed all five checks and **140** input-field parity
comparisons (68 MCP/Workbench, 72 CLI/MCP). Sources, native inputs and source-work
identity/watermarks stayed unchanged; frontend EOF was graceful and the owned
service drained. At 30 samples per case/mode, Off / default Report medians were
**66.89 / 134.87 ms** for 32 bytes, **67.46 / 131.55 ms** for 65,536 bytes,
**66.77 / 135.59 ms** for 8,659,200 bytes, and **241.61 / 304.63 ms** for eight
sources. All **16** median/p95 comparisons against P4 passed the same allowances.
An initial ad hoc summary used `report` instead of the receipt's
`default_report` key and failed with KeyError; correcting that reader required
no new measurement or changed gate. The raw reports remain unchanged.

The three-trial relevance run completed **75 query attempts** without failures.
All **42** non-latency score comparisons matched P4 to absolute tolerance 1e-12,
including development/held-out nDCG@10, Recall@10 and the lexical negative case.
The corpus/source hashes and final binary hash matched. This small warm-cache
corpus is regression evidence, not general relevance or cold-start qualification.

Exact final measurement commands (run sequentially):

```console
python scripts/benchmark-mcp-investigation.py owned --runtime-dir artifacts/v190/p5-188-qualified-runtime --output artifacts/v190/p5-188-off-mcp --label candidate --samples 30 --concurrency 1 --verification off
python scripts/benchmark-mcp-investigation.py owned --runtime-dir artifacts/v190/p4-final-qualified-runtime --output artifacts/v190/p5-188-control-off-mcp --label baseline --samples 30 --concurrency 1 --verification off
python scripts/benchmark-mcp-investigation.py owned --runtime-dir artifacts/v190/p5-188-qualified-runtime --output artifacts/v190/p5-188-report-mcp --label candidate --samples 30 --concurrency 1
python scripts/benchmark-mcp-investigation.py owned --runtime-dir artifacts/v190/p4-final-qualified-runtime --output artifacts/v190/p5-188-control-report-mcp --label baseline --samples 30 --concurrency 1
python scripts/benchmark-source-verification.py --runtime-dir artifacts/v190/p5-188-qualified-runtime --output artifacts/v190/p5-188-verification --samples 30
python scripts/check-retrieval-quality.py --binary artifacts/v190/p5-188-qualified-runtime/airk.exe --output artifacts/v190/p5-188-relevance.json --trials 3 --detail agent_lean --corpus scripts/fixtures/retrieval-v190.json --model-dir target/local-vector-model/minilm-l6-v2-u8-v1
```

`p5-188-gates.json` retains all 60 comparisons and 42 score comparisons, evaluated
from the raw reports without reruns or noise waivers. Dedicated verification's
raw cases retain all individual timings, bytes and receipts. Logical source-byte
checks do not measure physical I/O, whole-process peak RSS or editor-save latency.

The primary agent performed an explicit final review of the combined staged
code, contracts, migration SQL, callers, fixtures, documentation and validation
evidence. This was a self-review, not an independent code review. The separate
research agent's plan integration was reviewed by the primary agent. The accepted
P5 boundary is the locally qualified, **inactive** foundation: actual migration,
imports, backup/recovery, predecessor refusal, settlement interruption and
post-start write barriers are covered with the fixture distinctions above.
No new public observation route, Indexing UI, grants enforcement, combined
retrieval, permanent trim or split is enabled. Those remain P6 onward.

The dependency advisory audit remains unavailable as recorded above; 366-package
license notices passed. Installed-editor, macOS/Linux, signed-channel and general
relevance qualification remain outstanding. The user's installed 1.8.7 service
and its configuration were preserved. Working source stays **1.8.8**. After final
repository checks, create the human DCO-signed P5 commit, push only
`feat/finetuning`, create no PR and stop for maintainer review. Preserve the
unrelated untracked `docs/reviews/shark-tank-todos.md` outside the commit.

Final source review checks passed: `cargo fmt --all --check`,
`python scripts/release-version.py check --expected 1.8.8`,
`pwsh -NoProfile -File scripts/check-v190-design.ps1` (43 fixtures),
`pwsh -NoProfile -File scripts/check-repo.ps1` (**1,551 public files**), both
working/cached `git diff --check`, AGENTS/CLAUDE byte agreement and an independent
recalculation of the qualified pair's **784-file** native source identity.
README, documentation index, working/planned release records and Marketplace
description distinguish accepted P5 foundations from disabled P6+ capabilities.
