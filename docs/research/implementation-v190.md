# AIRK 1.9.0 implementation ledger

**Started:** 2 November 2026. **Branch:** `feat/finetuning`.
**Approved plan:** [phases, contracts and performance gates](plan-v190.md).
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
| P3 | Complete - signed checkpoint follows | ADRs, schemas, authority/partition fixtures and upgrade fault boundaries |
| P4 | Not started | Opt-in single-index source verification |
| P5–P7 | Not started | Safe metadata upgrade, Indexing/grants, then complete combined-read parity |
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
