# AIRK 1.9.0 implementation ledger

**Started:** 2 November 2026. **Branch:** `feat/finetuning`.
**Approved plan:** [phases, contracts and performance gates](plan-v190.md).
**Current working version:** 1.8.8, selected 3 November 2026 for manual testing
before later 1.9.0 promotion. Earlier checkpoints below retain their original
versions and exact payload identities; they are not relabeled.
The maintainer authorized sequential implementation and signed phase commits.
Progress below is observed local evidence; it does not establish publication or
cross-platform qualification.

**Current checkpoint, 6 November 2026:** P9, P10, P11a, P11b and P12 are accepted for
unpublished local development. P10 is committed as `c2584cb`, P11a as `f2bf2d2`
and P11b as `aa28425`. P12 passed primary local phase review: its focused
Windows native selection passed **39/39** on `4121692a…` / 960 inputs, strict
six-package all-target/all-feature Clippy passed, and the full extension suite
passed **776 tests with 19 skips**. All **12** final Windows release qualification
stages and **19** scoped Neo tests passed; Neo `cargo clean` exited zero and
removed its target. The matched release pair built in **815.503 seconds**.
The signed checkpoint is recorded by the subsequent human-authorized commit;
this entry does not invent its hash. P13 ranking/diversity remains
conditional; P14, deleted-source recovery and the final local test installer
remain pending. Earlier entries preserve their exact source/runtime identities,
failed attempts and platform limits; they are not relabeled.

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
| P6 | Complete — Windows local qualification and required full Neo checkpoint | Guarded authority-1 activation, reviewed setup/grants, installed Windows scenarios, all 84 matched native latency gates and source-bound Neo campaign with explicit corrections/reruns; release/platform limits remain separate |
| P7 | Complete — locally qualified checkpoint | Unique-index composition and complete native/CLI/MCP/Workbench/parse/session/verification support; Windows release and Neo debug actual suites, all 84 latency gates and 120 Windows fan-out trials passed; platform/release limits below |
| P8 | Complete — locally qualified checkpoint | Canonical Recommendations and harness scope guidance; Windows release / Neo debug actual transports, useful subset evidence and all 84 unchanged latency gates passed; installed/platform limits below |
| P9 | Accepted locally | Windows workspace/runtime, 84 frozen latency gates and focused Mac ARM checks passed; exact source identities, test-only follow-up and release limits recorded below |
| P10 | Accepted locally — `c2584cb` | Reviewed independent split, native cutover and recovery through funding acknowledgment; dated Windows/Mac evidence below |
| P11a | Accepted locally — `f2bf2d2` | Bounded actual per-file profile family reporting and complete transports |
| P11b | Accepted locally — `aa28425` | Explicit retained Lean/Rich migration, legacy continuity and paid split inheritance |
| P12 | Accepted locally | 39 focused Windows native tests; strict scoped Clippy; UI 776 passed/19 skipped; all 12 final Windows release stages and 19 scoped Neo tests passed; clean exit zero and target removed |
| P13a/P13b | Conditional; current ranking retained | No candidate enabled; adoption needs frozen quality/resource, native disclosure and scope gates |
| P14 | Pending | Combined qualification, platform/fault/resource coverage and final candidate evidence |

**Additional maintainer request — 5 November 2026:** after P9–P14, reproduce and
fix deleted-source failure loops in the current release: individual indexed files
and an entire source folder can disappear, and repeated failures must not make
other indexes unusable. Audit the then-current native discovery, scheduler,
worker admission/recovery and retirement paths before changing them. Provide
bounded cleanup of stale references and explicit affected-index cleanup from
retained ownership metadata, including when the original folder no longer exists.
Preserve source read-only behavior, other indexes and exact operation recovery;
allow temporarily unavailable locations to recover without accidental deletion.
Qualify actual CLI/MCP/Workbench observations and native cleanup/isolation on
Windows and Neo, with Cargo cleanup afterward. This work is not yet implemented
or claimed fixed. Include it before the final local Windows test installer;
publication and normal installation remain separate actions.

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

## P6 — operational foundation and management

**Entry:** P5 was DCO-signed and pushed as `1f66ad1`. The maintainer committed
the selected Shark Tank integration as `7567bdc`, then authorized continuing
P6 onward with commits and pushes per accepted phase, **no PR until all phases
are complete**. The working version remains 1.8.8; normal catalog activation
stays disabled until the complete P6 management/grant gate. The earlier P5 stop
instruction is superseded by this continuation.

### P6a — launcher and immutable release producer

The maintainer explicitly authorized Mac ARM builds/tests on the previously used
Mac **neo**. Its verified SSH connection and existing prerequisite paths are
retained privately, outside the repository. Neo is Apple A18 Pro, six logical
processors, 8 GiB RAM, macOS 26.5.1 (25F80). A fresh task directory and independent
APFS copy-on-write build-cache clone preserve earlier builds, the installed
runtime, source data and global configuration. No publication or notarization
upload has been performed.

The initial proposed embedded-plist fix failed actual Mac tests. A first run
using system Python 3.9.6 failed six imports because `tomllib` requires the
documented newer interpreter. Reusing the previously provisioned Python 3.13.18
and pinned Node 24.15.0 resolved that prerequisite. The six-case Mac runtime-build
campaign then passed four cases and failed two: the copied app-sealed executable
failed strict verification with **invalid resource directory**, and Developer ID
signing returned **errSecInternalComponent**. Embedding the plist did not remove
the app resource seal; Windows mocks/skips could not establish this property.

The implementation therefore switched to an explicitly inventoried, separately
signed duplicate launcher image before sealing the enclosing app. A corrected
actual ad-hoc fixture passed: it rejects a copy of app-sealed main while verifying
and executing the independently signed copied image. This is a fictional native
fixture, not qualification of a real Rust payload or installed editor. Installer
and copied service-role paths are being audited together before acceptance.
The first source snapshot (785 native inputs,
`2e2c5f2091c2332e024166c0fae497804b197dd0c5e015574962f356265c146b`)
belongs to the rejected prototype and cannot qualify the corrected design.

Neo has the existing Developer ID identity. Noninteractive inspection of the
separate AIRK signing keychain reported **User interaction is not allowed**;
the maintainer was asked to unlock it locally. No password was requested in chat,
key exported, ACL changed or credential store reset. Developer ID, notarization,
real installed-editor and final Rust-pair evidence remain pending. Failed fixture
logs are retained alongside subsequent attempts; no signature assertion was
weakened to accept the prototype.

The corrected layout adds `AIRK.app/Contents/MacOS/airk-launcher-image` for
macOS 1.8.8 onward: a copy of the existing compiled main, independently signed
before app sealing. The installer stages/repairs its admitted digest, while
services/workers retain the sealed main and its existing pair/health identity.
The copied process-role aliases were audited too and are not created for this
layout. Native archive, VSIX and CI transfer checks require the image and its
executable mode. Earlier payload layouts remain supported. A real ad-hoc fixture
on neo now proves the separate image survives sealing, copying, strict signature
verification and execution, while a copy of sealed main is correctly refused.

Primary review found two sibling paths needing correction: old known universal
launchers must refresh on Mac, and publishers/retirements from different install
roots need one shared lock across image/pointer transitions. The final correction
retains an owner-private zero-byte coordination inode, fails busy/unknown owners
before mutation and qualifies exact-source interruption recovery. Mac Prepare
Client resumes only the selected approved image through the canonical copier,
without changing service/manager/explicit-Stop policy. Targeted Windows checks
passed **142/142**, with CLI Clippy/formatting/whitespace passing; final workspace
and actual final Mac source runs remain separate gates.

The intermediate Windows workspace passed **1,939 tests, 37 skipped** in
1,110.110 seconds, with workspace Clippy and all eight doctest targets passing.
This precedes the final shared-lock correction and is not final source evidence.
The first corrected neo Rust pair built successfully and passed Mac CLI Clippy
plus **311 CLI tests, six ignored**. Its Python run had **336 cases, two errors,
ten skips**; both errors came from global subprocess mocks intercepting macOS
`platform.platform()` byte decoding, including an existing retrieval-quality
fixture. Both fixtures now isolate their platform observation and explicitly
assert the recorded platform. Their **17** focused tests pass on Windows; the
same corrected suite is rerunning on neo. The earlier R2 pipeline stopped before
native-client/installer smoke rather than reporting those unrun stages as passing.

The full Windows Python run passed **328 cases, eight skipped** (336 total);
the two subsequent fixture-only corrections passed the focused 17-case run.
The final native source is **784 inputs**,
`7880013e0fd55b6cc9bb5505c16e052f123f72ee64caf8893081bff6c46db6cd`.
The independently verified final full snapshot contains 1,558 source files;
archive SHA-256 is `89d66c38a0507ac4548c410562f3e795dcc025bb7120e88bac9065b75c798a38`.
The private remote runner initially had Windows CRLF conversion; normalizing that
runner to LF changed no product bytes or checks. The corrected Mac and Windows
campaigns retain their own logs; no native source edits are planned during them.

Release-producer source tests passed **22 candidate tests** and **55 release
tests**, plus actionlint. Marketplace now selects `signed_run_id` and consumes
the canonical signed GitHub assembly without rebuilding/re-signing/repackaging.
Final Marketplace publication requires matching published GitHub assets first.
Primary review caught a publication-only retry problem; both consumers now share
the successful retained assembly selector, pinning its actual attempt/artifact
ID/digest. Publication/site-only retries preserve original bytes; a newer assembly
invalidates an older preflight. Missing/expired/damaged provenance refuses rather
than falling back. These are local contract tests, not an executed hosted release.

Subsequent 3 November observations supersede the pending-run descriptions above:

- The Windows final-source workspace campaign ran **1,942 tests: 1,941 passed,
  one failed, 37 skipped**, in 1,059.286 seconds. The durable-key CLI/MCP parity
  comparison differed in an optional post-read readiness observation and its
  derived verification-receipt headroom. The complete original responses and
  existing paired-comparison contract are being reviewed before correction;
  this run is not a passing workspace gate. Workspace formatting and Clippy passed.
- Windows Python passed **335 cases, eight skipped** (343 total). An additional
  bootstrap reporting correction now retains nextest failed/flaky/skipped counts
  independently; its ten focused tests pass. This does not convert the failed
  workspace run into success.
- Neo's R4 campaign passed Python **326 cases, ten skipped** (336 total) and the
  actual native-client smoke, including source-byte preservation. Its installer
  smoke correctly stopped at an obsolete copied-role assertion. The smoke now
  checks the versioned Mac layout, exact admitted images, strict signatures on
  stable/universal/repaired copies and unchanged distribution bytes; ten focused
  tests pass. The corrected actual lifecycle run remains pending.
- R4's Mac CLI test count did not include the new publication-lock tests:
  transferred Windows source timestamps could precede the reused Cargo cache.
  Its CLI count/Clippy result therefore cannot qualify the final correction.
  R5 verifies the full source manifest, refreshes CLI compilation-input timestamps
  without changing bytes, and explicitly requires the new Mac/lock tests to run.
  Its starting native identity remains the 784-input hash above. The later
  bootstrap parser/test transfer is a separate Python-only delta.
- Actual Developer ID signing of the R4 Rust pair succeeded in neo's existing
  GUI security session using the matching identity in its unlocked login
  keychain. All four components passed team, hardened-runtime and secure-timestamp
  checks. Independent receipt re-verification passed. A byte-identical standalone
  launcher copy passed strict signature verification and executed as **1.8.8**;
  its SHA-256 is
  `3e517a8a6a4520d9e5258d7fe91edecfb52f6d80bc5d2e9c5f76bedefce4f037`.
  The signed receipt SHA-256 is
  `a2e5bf89e3b377ba94c969d5579a9abb7291b6464032903e4290dafe130d71cd`.
  The exact temporary GUI job was removed and its absence verified. No password
  was read by tooling, credential changed, certificate imported, or user service
  altered. Notarization was not submitted; installed-editor/agent qualification,
  the final fresh-source campaign and performance comparisons remain pending.

The R4 standalone image is **171,367,216 bytes** before distribution compression.
It deliberately reuses the compiled CLI instead of adding a second launcher
implementation or crate. This increases the distribution inventory by one image;
the new installed Mac payload also removes the three former full-size role
copies. Download/compressed-archive cost must be recorded separately from installed
disk use. No retrieval-time extraction, model work or source hashing is added by
this layout; the ordinary performance gates still apply. The release assembler
now also requires the exact post-seal copied-launcher receipt, with negative
digest/version/schema/boolean tests, through the same validator used by Marketplace
staging. Its 22 candidate and 55 release tests pass after this correction.

R5 on neo completed successfully: fresh Mac CLI Clippy, **314 CLI tests passed,
six ignored**, Python **334 passed, ten skipped** (344 total), actual native
clients in 87.38 seconds and the complete corrected installer lifecycle. The
runner explicitly confirmed the new publication-lock and Mac launcher tests ran.
The separate actual Developer ID signed R4 pair then passed the same isolated
installed lifecycle in **140.92 seconds**: strict stable/universal/repaired
launcher checks, helper/self-test, independent client lifetime, foreground
dispatch, upgrade/rollback, retirement and resumable uninstall. Critical scripts,
signed images and receipt hashes remained unchanged, and no matching fixture
processes or target directories remained afterward. This signed pair retains its
original `788001…` native identity; it is not relabeled as a later build.

The parity correction changes three CLI test files, not executable production
code. Shared paired normalization validates each original readiness projection
and derived receipt headroom before excluding only the documented dated
observation difference; negative cases preserve scope, evidence and requested
work-budget comparisons. A subsequent prepared-plan test exposed a separate
fixture race: a writer between plan steps correctly returns a partial result with
`publication_busy`. This fixture now publicly pauses settled source updates
before any plan executes. Production partial-result and single-execution contracts
are unchanged. All **21** affected actual CLI/MCP and retry tests passed in
42.497 seconds, with targeted Clippy, formatting and whitespace passing.

The final native input identity is **784 files**,
`59bb1d92290f07a20949d9ea5e322f578118516bc2079c32a4b083eab24e15af`;
the hash changes because tests are included in the declared input inventory.
Windows formatting and workspace Clippy passed again; the final workspace run
and current-identity release pair are pending. The latest full Windows Python
suite passed **336 cases, eight skipped** (344 total), followed by the focused
release-receipt checks above. Versions and repository checks passed at **1.8.8**,
and AGENTS/CLAUDE remain byte-identical. A separate final-identity Mac pair and the
changed actual integration tests are running without repeating the completed R5
library/Python campaign. Notarization, installed-editor acceptance and final
performance comparisons remain pending; no publication or user installation has
been performed.

That final-identity Mac R6 pair built successfully. Its actual CLI/MCP integration
targets passed **12 + 9 tests**, in 78.80 and 4.62 seconds, and source identity
was checked before and after. All four R6 unsigned component hashes exactly match
the unsigned R4 inputs to the successful Developer ID signing and installed
lifecycle campaign. This resolves the earlier release-binary cache uncertainty
without a redundant re-sign/install run; the original signed receipts retain
their R4 identity. The final standalone input digest is
`1a2d6530c30c83e8c199bfe20c5b45c2d595370d0ddc045f5d0794197ac4247b`;
signing changes it to the separately recorded signed-image digest above.

#### Final P6a source and policy qualification — 3 November 2026

A final ownership review found that retiring an unrelated custom installation
must not create or validate the user's universal publication lock. Read-only
early exits now preserve an absent or differently owned entry point; actual
owned retirement still acquires the shared lease and repeats its identity checks.
The new native input identity is **784 files**,
`e261824ba5cf5e93286de5cee5a1b36a68cbc735990b1985bde50bae01ca6ad7`.
It supersedes the R6 source for final qualification without relabeling prior
signed artifacts.

The Windows R7 campaign passed formatting, workspace Clippy, **1,945 native
tests with 37 skipped** in 1,116.686 seconds, and all eight doctest targets
(zero executable examples). The final Python suite passed **360 cases with nine
skipped** (369 total) in 19.633 seconds. Its first sandbox run had one Git-tree
observation error because Git rejected the checkout's ownership; rerunning with
a process-only `safe.directory` override passed without test/source changes.
Repository checks passed **1,567 public files**, version agreement at **1.8.8**,
43 design fixtures, actionlint, whitespace and AGENTS/CLAUDE byte agreement.
The fresh Windows release pair retained that same native source identity:
main `397b59a1c812a85027f6c22c26c69b4922bd4e09965b341f577754acc9d4cb84`,
helper `d0e6076126f9936ed77f10de4c3022adc21194de65b5541e1e51563443afa8fe`.
Release-mode license notices passed for **366 third-party packages** after
adding the existing Cargo tool directory to this shell's PATH; the first attempt
reported missing Cargo and changed no project inputs.

Neo R7 passed a fresh release build, CLI Clippy, **315 CLI tests with six ignored**,
the actual CLI/MCP integration targets and the complete isolated installer
lifecycle. The runner explicitly required the new ownership/publication-lock
tests and checked exact source identity before and after. The changed production
bytes were then freshly Developer ID signed in the previously qualified GUI
login-keychain session. Independent receipt re-verification and the copied
standalone strict-signature/version probe passed. The receipt SHA-256 is
`2de96f5e21f76e8bf4df7d8aeedae9b200a870de9ce739764d6ac11ff59a7a6b`;
the signed standalone launcher SHA-256 is
`6e781de8fad7cad3115ccbb57e98b20a3f334f791328c633bc7cc3ccc29616c7`.
The temporary GUI job was removed and its absence verified. Notarization and
installed-editor qualification remain outstanding; no user installation changed.
The exact signed R7 pair then passed the complete isolated installer lifecycle in
**139.54 seconds**: strict copied-image checks, client lifetime/EOF, repair,
foreground dispatch, upgrade/rollback, retirement and resumable uninstall.
Its recorded fixture scripts, signed images and signing receipt remained unchanged.

The canonical dependency gate now checks source identity before Cargo metadata
may inspect new sources, then retains full cargo-audit/npm, cargo-deny and
cargo-vet outcomes plus before/after input hashes. Source-policy tests also reject
same-name/version local replacements, new glob members, target/patch/replace
path dependencies and linked aliases. The dedicated workflow explicitly prepares
the repository-pinned Rust toolchain before the checker, which deliberately
never auto-installs it. Bootstrap checks the same exact tool pins and runs this
gate before dependency provisioning/building; setup instructions name private
tool locations and distinguish fresh from cached/offline evidence.

The final fresh Windows policy receipt passed every requested check, with RustSec
revision `6c5fedf8ff03c485ed674d8f941e0180f3f72d70` and three findings covered by
the existing dated dispositions (`paste`, `ttf-parser`, `minimatch`). No exception
was added to hide a new finding. Cargo-vet covers **366 third-party versions**:
**46 fully**, **one partially**, and **319 with baseline exemptions**; these
exemptions are explicitly unreviewed, not independent audits. The separate
offline receipt passed only its requested cached Rust checks and disclosed
omitted npm, fresh advisory and yanked-package coverage. The source graph and
review store stayed unchanged. Raw receipts remain under ignored
`artifacts/dependency-policy/`; no private state is part of the committed policy.
The final offline attempt inside the sandbox could not identify the existing
advisory checkout and correctly returned unavailable. Repeating with access to
that existing cache passed the explicitly limited checks; the failed receipt
was retained separately and no permission or cache configuration was changed.

The final Windows pair passed one complete `ci-local.ps1 -SkipBuild -RuntimeDir
artifacts/v190/p6a-qualified-runtime -Repeat 1` campaign: managed MCP **58.50 s**,
native clients **26.60 s**, installer lifecycle **102.30 s**, all three passing
under four logical processors with no load workers. This is one deliberate
local campaign, not the default five repetitions or an installed-editor result.

The separate serial measurement shell retained **64 logical processors and
unrestricted affinity**. No build, heavy test or source edit ran during the
measurements. Each candidate/control mode completed **90 trials / 810 calls**,
zero errors/retries, complete consistent coverage/publications and clean cleanup.
Each Report campaign returned **720 verified-source receipts**; Off returned none.
All **60** task and connection-inclusive median/p95 comparisons passed against
the frozen baseline and archived/contemporary P5, with unchanged allowances:
median `reference + max(10%, 25 ms)`, p95 `reference + max(15%, 50 ms)`.

| MCP task cohort | Candidate Off median / p95, ms | Candidate default Report median / p95, ms |
| --- | --- | --- |
| Fresh frontend, status first | 908.13 / 947.80 | 1447.49 / 1521.69 |
| Fresh frontend, unprimed | 900.74 / 970.70 | 1460.23 / 1505.14 |
| Reused connection, status first | 876.88 / 944.30 | 1469.01 / 1527.36 |

The first control launch was refused before measurement because the archived P5
runtime belongs to the earlier sandbox account. That existing owner copied its
**675 exact inventory files** into fresh staging; both inventories and source
bytes were verified unchanged without changing any ACL. The remaining campaigns
used that byte-identical control. The passing candidate Off campaign was not
repeated, and the failed access log remains retained separately. Control source
identity is `594438057cae526009b40e960ab443856bcdc6006c41ca9560379ff2175a6510`;
main is `2697af91b1cfe70936a2802e5711944001cdbb3984fa148c6d931318b193152f`.
Commands used `benchmark-mcp-investigation.py owned --samples 30 --concurrency 1`
with candidate `p6a-qualified-runtime`, control `p6a-p5-control-copy`, and explicit
`--verification off` only for Off. Raw reports and the unchanged evaluator's
`p6a-gates.json` remain in ignored `artifacts/v190/`. These are small fictional
warm-cache regressions, not general retrieval relevance or resource qualification.

The actual signed R7 launcher is **171,367,360 bytes**. With the native archive's
compression settings its added member is **23,595,371 compressed bytes**
(23,595,555 bytes for a one-member ZIP). This measures the added member, not a
complete notarized download. Removing three former installed role copies avoids
**514,102,176 bytes** for this pair, separately from the new distribution member
and stable/universal copies. Signed source bytes stayed unchanged during measurement.

The acting primary agent reviewed the complete combined implementation and
evidence, including the separately authored source-policy guard. A separate
read-only reviewer found no additional blocking issue across native lifecycle,
producer, policy/bootstrap, parity fixtures and evidence. That reviewer authored
the guard correction; the primary's separate review covers that component.
The final documentation audit corrected old Mac role-copy descriptions and
keeps README, documentation index, working/planned release records and Marketplace
description aligned at **1.8.8**.

P6a's local source gates are complete. Notarization, real installed-editor/agent
acceptance, hosted release execution and unexercised platform/security cases
remain explicitly unqualified release gates. No publication, PR or user-runtime
installation occurred. The plan permits independent P6b onward work with these
limits retained; normal catalog/grant activation remains disabled until P6f.

## P6b — helper outage recovery (3 November 2026, source gates accepted)

P6a was committed and pushed as `7500fa2f13f43a603c4f7aa1875f90af0a52c416`
with the authorized human DCO trailer. P6b retains working version **1.8.8**.
Final local validation accepted source
`ad49d73ca312e0ccadd4003f21f3ffea6c82727dc8e751e77ec2717273f06435`
over 787 native inputs. P6c may proceed; signed/editor and remaining platform
release qualification stay separate. The checkpoint history below identifies
failures and corrections rather than presenting every earlier run as current.

Before correction, a controlled request to the retained P6a Windows helper
returned generic `failed` for an unknown worker grant, then answered Status and
stayed alive. The receipt `artifacts/v190/p6b-missing-grant-before.json` pins helper
SHA-256 `d0e6076126f9936ed77f10de4c3022adc21194de65b5541e1e51563443afa8fe`.
This reproduces a missing-grant control path, not the cause of an older unexplained
SIGKILL or crash. Source tracing confirmed that the generic reply caused the
supervisor to drain the healthy shared pool, and that its former lifetime
three-attempt cap could never reopen without explicit Start/service restart.

The implementation adds helper-scoped, exact-state CAS operations without DDL.
Three failed attempts enter a sixty-second cooldown; another burst preserves the
original outage anchor. The supervisor translates a saved wall deadline once per
observed state to monotonic time. Healthy reset requires the same live helper for
sixty seconds and an existing acknowledged control exchange after that interval,
and clears only the captured outage. Explicit Stop and terminal startup safety
suppression survive restart until explicit Start. Helper cooldown observations
use existing `waiting`/next-check fields; other recovery scopes retain their
prior behavior. Payload/containment/identity failures are not mislabeled as
transient healthy states.

Missing-grant replies carry the exact worker identity. Repair replaces that
worker's capability through its existing private pipe. Helper connection cleanup
now carries an exact owner witness: delayed release or a canceled request from
the old connection cannot remove repaired or sibling model sessions. A failed
containment attachment terminates/reaps the exact uninitialized child before
returning. Diagnostic codes retain observed exit code/signal and recovery/grant
transitions without adding stderr or model inputs. Malformed reply framing can
still collapse to control-closed; historical causes remain unknown.

Initial focused Windows results: three supervisor recovery tests, two process
diagnostic/containment tests (one child fixture intentionally ignored), seven
Store helper tests, the actual helper missing/revoked-grant integration and the
real tiny-model delayed-owner regression passed. Native IPC and CLI recent/full
export preserved complete recovery log pages; the canonical log-reader test
passed. Extension compilation passed, with **547 passed / 19 skipped** across
566 tests, including complete log-page preservation. These tests separately cover
helper control, canonical inference ownership and log transport; they do not by
themselves prove the entire supervisor grant-repair loop or repeated native loss.

Workspace Clippy/format passed. The first broad-test invocation could not find
the existing pinned nextest executable; its log is retained and the campaign
resumed with that directory on process PATH. Python ran 369 tests with nine skips
and one Git ownership error; that exact test passed with process-scoped
`safe.directory`. Repository checks likewise passed after the same process-only
correction. No global Git identity/configuration or filesystem ACL was changed.
At that checkpoint, Windows release and isolated Mac qualification, repeated
native outage/cooldown/restart, complete production repair, phase latency and
final review were still pending. Final evidence appears below.

The fresh P6b dependency-policy receipt passed all seven checks with the same
three dated advisory dispositions and no graph/policy change
(`artifacts/dependency-policy/p6b-fresh.json`, observed 19:46:49 UTC). Repository
checks covered 1,569 public files at that checkpoint. Mac ARM64 focused checks
passed against source `579606b285a34d42afe402681e1aa02a4f26295f0a6bb2c675b9f07eedaf7510`
(786 native inputs), with before/after hash verification. Its release build was
held for final supervisor integration coverage; these results do not qualify
later source changes. Exact logs and the initial qualification receipt are in
`artifacts/v190/neo-p6b/`.

The first broad Windows campaign completed **1,950 passed / seven failed / 38
skipped**. All seven failures required the existing private native fixture base,
which its invocation had omitted: two catalog-upgrade cases, three exceptional
owner-recovery cases and two Store native-boundary cases. Their owner/privacy
checks correctly refused the ordinary temporary directory. The two Store cases
then passed using the previously qualified private base, and the final CLI
package campaign passed the other five cases. No ACL, assertion or production
ownership rule was changed to obtain those results. The original failed campaign
is retained as `artifacts/v190/p6b-workspace.log`.

A separate Sol review found a sibling control-channel gap: ignored Revoke errors
and configuration rollback could reuse a reply stream after an ambiguous
exchange. A delayed acknowledgement could then match a later exchange. The
canonical Pool control boundary now refuses subsequent I/O after that
uncertainty, with retirement/accounting on the existing supervisor turn.
The known pending-Status continuation and correlated missing-grant repair remain
distinct. Valid negative replies retain rejection codes instead of being called
malformed protocol. Four actual-method/child-pipe regressions cover late replies,
Status continuation, missing grants and negative reply classification. Final
combined qualification passed as recorded below.

The Mac checkpoint pair at source `2023546c89e20e429c65479c97280de88c6e92646ca267b017890d131c797dc8`
built successfully, but its new supervisor fixture failed before worker
admission: initial sibling inference requested 1 MiB, below the real helper's
existing 8 MiB minimum. A single targeted backtrace identified that call. The
fixture now requests the existing admitted minimum and supplies stage context;
its identity, publication, shared-model, sibling-session and source-preservation
assertions are unchanged. Original failure/build/backtrace evidence is retained
in `artifacts/v190/neo-p6b/checkpoint-2023546/`. No production memory limit was
changed. The next production checkpoint was
`be6e29680ceacc11d056f04dfbb7a6cf63487876d01f6bccba7bd2b217542053`
over 787 native inputs. Its strict one-load assertion then exposed a second
fixture setup issue: custom provisioning after indexing deliberately retired
resident inference state, adding an unrelated second load before repair. Moving
reference capture before indexing retained every one-load, identity and repair
assertion. Final production component bytes remain identical to that checkpoint;
only the isolated test ordering changed.

The first repeated-loss fixture also allowed its sole project worker to retire
after the existing fifteen-second idle interval while polling only helper
status. Recovery correctly reopened the saved retry burst but did not start an
unused helper. The corrected fixture keeps real lexical reads active every five
seconds and checks the same worker through cooldown. It explicitly closes its
read-only SQLite observers and captures bounded failure diagnostics. No idle
limit or production restart predicate was changed. Original failed receipts and
logs remain retained.

### Final P6b validation

| Gate | Observed result and evidence |
| --- | --- |
| Windows native suites | Broad checkpoint: 1,950 passed, seven private-fixture failures, 38 skipped. Both affected Store cases then passed; final CLI package passed **439 / 15 skipped**, including the five affected CLI cases. After the control-stream correction, the CLI library passed **322 / 10 skipped**. Final all-target/all-feature CLI Clippy, earlier unchanged workspace Clippy, formatting and workspace doctests passed. Logs: `p6b-workspace.log`, `p6b-final-*.log`, `p6b-r3-cli-lib-tests.log`, `p6b-r4-cli-clippy.log`. These are complementary runs, not one claimed all-green workspace invocation. |
| Actual Windows supervisor | Fresh current-source pair `artifacts/v190/p6b-r4-qualified-runtime`; exact native fixture passed in **2.12 s**, preserving one model load, sibling inference, worker/helper identities/PIDs, publication and source bytes through Missing-to-rebind repair. |
| Hosted-runner CPU reproduction | **3/3** suites passed with four processors, one iteration: managed MCP **56.9 s**, native clients **27.1 s**, installer **101.3 s**. Receipt: `artifacts/ci-local/20261103-132054-dbd3024d/summary.json`. The five-iteration pre-PR/release gate remains separate. |
| Pooled MiniLM | Actual offline two-project pool smoke passed model reuse, regeneration, helper loss, lexical/source preservation and explicit recovery with one shared model load. Log: `artifacts/v190/p6b-r4-pool-smoke.log`. |
| Repeated native outage | Passed in **137.554 s**: three owned helper losses, **60.704 s** cooldown across service restart with the active worker and original outage anchor retained, healthy reset, a fresh later outage anchor, explicit Stop across restart and explicit Start recovery. Exact process handles/image/incarnation were checked before termination. Receipt: `artifacts/v190/p6b-native-outages.json`. |
| Mac ARM | On neo, source/input hashes matched before and after. Focused production-checkpoint Clippy and helper tests passed, including four recovery and six pool-process tests; one owned child entry remained ignored in ordinary discovery and was invoked by its parents. The final test-only correction passed the exact supervisor fixture against retained and current-source pairs (**2.22 s / 2.16 s**). All four production components were byte-identical between those pairs. Full isolated install/repair/upgrade/rollback/retirement/uninstall and client lifetimes passed. Receipt: `artifacts/v190/neo-p6b/final-ad49d73/qualification.json`. Development ad-hoc sealing does not qualify Developer ID/notarization or editor acceptance. |
| Source/tool contracts | Python **360 passed / nine skipped** out of 369, repository checks for **1,570** public files, synchronized **1.8.8** versions, instruction-file byte identity and whitespace checks passed. Complete native log pages, CLI retained/recent exports and console output retained recovery fields; extension compilation/tests passed **547 / 19 skipped**. Dependency policy passed all seven requested fresh checks without a dependency/policy graph change. |
| Performance | **84/84** comparisons passed with the existing median/p95 limits unchanged. Candidate and contemporary P5 control each ran Off and Report: **90 trials / 810 calls per campaign**, zero errors/retries, and **720 verified receipts** in each Report campaign. Gates retain the original 60 frozen/P5 comparisons and add 24 archived P6a comparisons. Source/model work, coverage and receipt assertions remain enforced. Receipt: `artifacts/v190/p6b-gates.json`; reports: `p6b-{off,report,control-off,control-report}-mcp/replay/report.json`. |

The lead reviewed the complete production change independently of its authors;
a separate Sol review found the control-stream issue and reviewed the corrected
native/private fixtures and final evidence. No unrelated user state, global
identity, permissions or ignore rules were changed. No PR, release publication
or version promotion occurred. Historical unexplained helper exits remain
unattributed; malformed output can still appear as control EOF. Remaining
signed/notarized, installed-editor/agent and platform release gates are retained
for P14 rather than claimed from these local source results.

## P6c — policy-qualified outage reads (3 November 2026, completed locally)

P6b was committed and pushed as
`c7e20221232ac8034f2b29631a92736dd8010814` with the authorized human DCO
trailer. P6c retains working version **1.8.8** and Report verification as the
default. Canonical, transport and presentation work was integrated and qualified
together before the phase commit.

The agreed per-search `execution` observation records version 1, original
requested mode, actual served mode and semantic incompleteness with a typed
reason. It does not replace indexed semantic/discovery readiness, byte/count
truncation or source-verification receipts. A typed shared helper-availability
failure permits Default to keep its already-collected lexical branch only when
the saved policy allows it. Explicit vector/hybrid and RequireComplete retain
strict helper-outage behavior. The existing explicit Hybrid semantic-upgrade
lexical exception remains separate and will be disclosed as such.

Strict helper failures use one bounded Core receipt across MCP and Kernel
Workbench fallback, with the selected publication/readiness when known and
`retryable: false` / `retry_later_without_replay`. They do not join the automatic
read-retry roster. Saved opt-out preserves requested Default without being called
a helper outage. No health probe, source refresh, helper spawn or query replay is
introduced by this projection.

The consumer audit identified an existing `execution` key used to identify
diagnostics in Terse rendering. Search execution must remain distinguishable so
adding the observation cannot silently expand normal replies. Complete and
explicitly selected Workbench views/exports must retain the new qualification;
an evidence-only plan view must still report semantic incompleteness even when
its selected format omits raw step payloads. The completed qualification below
exercises canonical policy/errors, actual CLI/MCP/plan paths, native package
behavior, source-verification parity and unchanged performance limits.

### P6c qualification checkpoint

The native implementation is frozen at
`28f6f23ab1860db27b21c6cef252525b5f7c668cd6e4aeb037049221fe55ef86`
(789 native inputs). Windows workspace Clippy passed with all targets/features
and warnings denied. The complete nextest campaign ran **1,973 cases in
1,126.268 s: 1,972 passed, one failed, 39 skipped**. Its sole failure was the
generated CLI reference after the intentional Search help change. Regeneration
through the actual current parser passed; the exact nextest recheck then passed
in **0.069 s**. The original failing campaign remains retained rather than
described as an all-green invocation. All workspace doctest targets then passed
(zero executable examples). The fresh Windows paired release build passed
(**3m16s helper, 4m54s main**), with its source receipt matching the frozen
789-input identity. Package and performance gates subsequently passed without
repeating the full workspace campaign.

Windows actual CLI/tool/shell/MCP/plan outage parity passed in **52.34 s**,
covering all three saved policies and Off/Report/Require. The canonical policy
matrix passed in **30.71 s**; Kernel Workbench passed **24, one measurement
ignored**. Extension compilation and tests passed **550, 19 skipped** using
Node **24.21.0**, rather than the repository's 24.15.0 pin. Python passed
**360, nine skipped**; all seven fresh dependency checks passed without graph or
policy changes. The initially failing MCP fixture supplied verification budgets
with Off, which the existing controls correctly reject; the corrected fixture
uses `VerificationControls::off()` and preserves finite Report/Require budgets.
Its exact corrected test passed; the earlier failed log remains retained.

On neo, current-source workspace Clippy and **211 focused/native tests** passed,
with one explicit role measurement ignored. This includes Core 76, MCP 99,
Kernel canonical matrix one, Kernel Workbench 24, pool-client ten and actual
outage parity one. The fresh paired release build passed (**4m23s helper,
5m20s main**); actual 16-file model-client smoke passed in **92.635 s**.
The full isolated installer/repair/upgrade/rollback/retirement/uninstall and
two-client lifetime smoke passed. All 791 declared inputs matched before and
after; no matching owned runner/client/install processes or named fixture
directories remained. Receipt:
`artifacts/v190/neo-p6c/final-28f6f23/qualification.json`.
Mac full-workspace tests were not run; the 211 cases are the focused matrix,
alongside full-workspace Clippy. No Developer ID, notarization,
installed-editor or native ONNX rollover acceptance is inferred from this
checkpoint.

Under the approved lean execution refinement, P6c performance retains the
original 60 frozen/P5 comparisons and adds 24 comparisons against the immediately
preceding accepted P6b phase. P6a comparisons remain historical evidence; they
are not accumulated again. Limits and scenario assertions are unchanged.

The fresh Windows pair passed all **three four-core package smokes**, one
iteration each: managed MCP **58.10 s**, native clients **27.70 s** and installer
**103.40 s**. Receipt:
`artifacts/ci-local/20261103-150212-1756dbc9/summary.json`.
The separate five-iteration release gate remains outstanding. The pooled-model
smoke passed two-project model reuse, replacement/regeneration, helper-loss
lexical/source preservation and explicit recovery. Log:
`artifacts/v190/p6c-final-pool-smoke.log`.

All **84/84** latency comparisons passed with the existing limits unchanged.
Candidate and contemporary P5 control each ran Off and default Report:
**90 trials / 810 calls per campaign**, zero errors/retries, and **720 verified
receipts** in each Report campaign. Candidate binary hashes matched before and
after; no competing builds/tests ran during measurement. Receipt:
`artifacts/v190/p6c-gates.json`; reports:
`p6c-{off,report,control-off,control-report}-mcp/replay/report.json`.

The lead reviewed the complete combined change independently of its authors;
separate Sol and root reviews found no remaining production blocker. Final
source/receipt checks matched the frozen inputs. No normal hierarchy activation,
user-runtime change, PR, publication or version promotion occurred. Remaining
ONNX rollover, five-iteration release, signed/notarized, installed-editor/agent
and wider platform qualification remain explicitly assigned to P14.
Final formatting, whitespace, instruction-file byte identity, repository checks
for **1,574 public files** and synchronized **1.8.8** versions passed.

## P6d — source admission fairness (3 November 2026, accepted)

Final acceptance is recorded at the end of this section. The intervening
checkpoints preserve the development history; their pending and failure states
describe those earlier checkpoints, not the final accepted source.

P6c was committed and pushed as
`d098d2cd4d5b3935494e4554e2ba1cbaf7398ac3`, with the human DCO trailer
verified. Working version remains **1.8.8**. P6d separates source admission
eligibility from merely observed source lag. One shared metadata-only Store
predicate governs source discovery/acceptance and suppression of semantic-only
continuations, including a transactional recheck after a stale Vector preview.
Setup/source continuation priority, accepted request replay and durable vector
intent remain intact. The supervisor samples one time, visits bounded source
pages before background pages and sends a reversible, exact-grant source-yield
hint through the existing work-status pulse. Worker pause/cancellation takes
precedence; yielding occurs at safe enrichment boundaries without interrupting
inference or publication.

The first focused Store background/source-watch/job run passed **34/34** in
**3.578 s** (471 unrelated cases unselected), including five new regressions.
Log: `artifacts/v190/p6d-store-focused.log`. Worker/kernel, actual native
backlog, package/platform and performance qualification remain pending. The
native fixture must prove source publication before backlog completion and
subsequent vector progress, including explicitly accepted source work with watch
disabled; an insufficient backlog cannot count as a passing fairness test.

P6e's independent Rust and TypeScript slices are being prepared in an isolated
checkout of P6c. They do not enter P6d's source-bound test/commit checkpoint.
Review caught and corrected unintended UI Unicode encoding changes and a
duplicate import before any P6e compile or integration. No P6e test result is
claimed yet.

### Historical frozen P6d checkpoint

At this checkpoint, native source was frozen at
`85f439d63f1e3ed13604de1166926900cb1fe8e0c98e2e7a6afc6fc3fa616ffb`
(789 native inputs). The final canonical Kernel matrix passed in **30.89 s**;
the mixed-private-frame regression and actual claim/finish observation-race
regression passed. Review retained the existing data-root/grant validation on
early yielding, before skipping execution/model admission. A completed or
not-yet-claimed job no longer emits a false metadata failure merely because its
supervisor grant is still being observed. Wrong-project/missing-job refusals and
exact worker/grant checks remain intact.

The optimized Windows native fairness run passed automatic edit/add/delete/rename
and accepted Index work with watching disabled: **zero Vector overtakers**,
source publication before backlog completion, exact citation paths/raw source
hashes, and subsequent saved Vector progress. Observed overhead beyond the
existing turn was **5.857 s / 4.044 s**, below the unchanged **10 s** fixture
allowance. Receipt: `artifacts/v190/p6d-source-fairness-release.json`.

Its matched final-corpus samples contain **30 actual partial-readiness reads**
and **30 semantic-ready reads**, each bound to its returned publication. Backlog
median/p95 was **261.75 / 336.46 ms**; ready-stage median/p95 was **279.61 /
377.48 ms**. The ready-stage resource snapshot still reported one running job:
this is a load-effect observation, not an uncontended-idle or prior-phase
regression qualification. The matched retained-P6c backlog control passed unchanged median/p95 gates:
**290.87 / 381.13 ms** control versus **261.75 / 336.46 ms** candidate, with
the same 387-file corpus, query recipe and partial-readiness cohort. This is
limited to that scenario; normal frozen/preceding phase gates remain pending.

Fixture development retained its failures: a raw debug directory without the
required paired manifest was rejected by the helper; canonical payload validation
now fails before launch. A Windows host-discovery mock and an incorrect automatic
Index-kind assumption were corrected against the real protocol. Sampling only a
brief current Embedding phase was replaced by exact-running-grant inference
evidence. Cached scalar status is diagnostic; actual search publications,
readiness and source hashes qualify source/read observations. One uninstrumented
diagnostics timeout remains unattributed; a subsequent bounded full-diagnostics
probe completed in **0.133 s / 18,035 bytes** without an oversized frame. A debug
timing miss (**11.548 s**) remains diagnostic and did not relax the optimized
allowance. Full bounded failed-wave evidence is retained.

On neo, workspace Clippy and all **36** focused cases passed (Store 34, Kernel
one, private protocol one), followed by the fresh release pair, **91.105 s**
16-file model clients and the full isolated installer lifecycle. Native hashes
and 791 declared inputs matched before/after. Its separate first fairness attempt
refused an original-witness replacement before observed source eligibility; no
production failure was inferred. Durable event-interval proof and the remaining
Mac fairness scenario are being qualified separately. No native source rebuild
is required by a fixture-only validation change.


### Historical P6d claim-race correction checkpoint

The refined neo fairness attempt passed automatic source ordering and the
unchanged 10-second overhead allowance (**5.341 s**), then found a real manual
source race. A Vector continuation was pending at event 43; an explicit Index
was accepted at event 44; that Vector began running at event 45 before the Index
at event 47. This is not the earlier fixture-witness limitation. Receipt:
`artifacts/v190/neo-p6d/fairness-482be3e/qualification.json`; bounded events and
raw failure evidence remain adjacent. The run was not retried and its matched
latency stage did not run.

The correction must cover both pending-job selection and the transactional
claim of an already selected Vector. Eligible source work takes priority over
pending semantic-only Vectors, including explicitly requested Vectors, while
retaining their job IDs, receipts and saved intent. Other accepted work retains
its ordering and pause/recovery rules. Automatic source admission must also
ignore pending Vectors as competitors, so a refused stale Vector claim cannot
leave source work blocked by the same pending job. There is no persisted
background/manual provenance for Vectors; automatic audit metadata describes
Verify work and cannot classify Vectors. No schema or caller-identity inference
is used for this distinction.

Before this correction, frozen Windows formatting, workspace Clippy, all
**1,979** selected workspace tests (**39** explicitly skipped) and all doctest
targets passed. The doctest targets contained no executable examples. Those
results qualify the old `85f439d...` checkpoint, not the pending correction.
New source-bound checks and affected actual native/platform tests remain
required before P6d acceptance. At that checkpoint the phase remained uncommitted.

After all P6a–P6f work is accepted, run a full frozen test build and test campaign
on neo before starting P7, as separately requested. Focused Mac receipts do not
substitute for that checkpoint. Keep working version **1.8.8** and do not infer
release signing, publication or user installation from test-build authorization.


The coherent claim correction also covers saved Index continuations with
watching disabled, automatic watch reconciliation ahead of queued Vectors, exact
stale-grant refusal, request replay, restart and subsequent Vector resumption.
Four regressions were added without changing the schema. Independent review
confirmed that pending semantic-only work cannot block its own source replacement.

The correction's complete Windows Store run selected **597 tests**: **596 passed,
one failed, six were skipped** in **208.641 s**. The sole failure was an old
fixture that tried to claim a new Vector after accepting Index work, ignored the
now-correct `None`, then expected a running-Vector yield hint. The test now proves
that refusal, completes the accepted source, requires an actual new Vector claim,
and only then accepts another source job to exercise yield/pause/cancellation.
Its assertions remain intact and independent review passed. A preceding new-test
field-access compile failure is retained separately; it executed no tests.

Final source is
`a87a1bc8377699c7ae6da76f2aa7dbb23fe3386816bf5edb87af0546b2d6cbf7`
(789 native inputs). All **16** final background tests passed in **1.602 s** and
workspace all-target/all-feature Clippy passed in **16.11 s**. The other **581**
passing Store cases and six original skips are reused from the complete run,
explicitly across this test-only correction. The 587 unselected/skipped entries
in the focused report are not 587 additional unavailable cases. The complete
Python suite passed **404 tests with nine skips** in **22.545 s**; repository
checks passed for **1,576** public files. Final runtime and performance results
are recorded below.


Review ownership is separate and explicit: the primary reviewer assessed the
original D implementation and native fixture, then authored the three-file Store
claim correction. The integration lead independently reviewed that correction,
found the saved-source sibling and reviewed its complete amendment. The lead
subsequently repaired the invalid test setup; the primary independently reviewed
that test-only change. No author review is labeled independent.


### P6d accepted — 3 November 2026

The final Windows pair is `artifacts/v190/p6d-claim-qualified-runtime`, source
`a87a1bc8377699c7ae6da76f2aa7dbb23fe3386816bf5edb87af0546b2d6cbf7`
/789; main executable SHA-256 is
`5146a4140f615746347c8e18a898890f4d8fef1debdc516303358847071d0eca`.
The canonical release builder completed helper/main profiles in **3m03s / 4m49s**.
The final canonical Kernel source/semantic-resumption matrix passed in **30.75 s**.
The earlier full workspace/doctest receipt remains explicitly at `85f439d...`;
only affected checks were repeated after the coherent Store correction. Combined
Store coverage is **597 unique passing cases**, with six original skips, across
the complete correction run and the independently reviewed test-only rerun.

Actual Windows fairness passed automatic edit/add/delete/rename and manual Index
with watching disabled, no Vector overtakers, exact source hash/citation and
publication observations, publication before backlog completion, saved Vector
resumption and owned cleanup. Final overhead was **6.107 / 4.154 s**. The raw
Windows run left its optional inline overhead limit null; the separate
`p6d-claim-source-gates.json` derives both checks against the unchanged frozen
**10 s** allowance and binds the new raw receipt SHA, native source and executable
hash. It does not rewrite the raw receipt or reuse the old source's gate result.

The corrected Windows backlog cohort uses the same 387-file corpus, lexical/Off
query recipe, resource configuration and partial-readiness range as the retained
P6c control. Candidate median/p95 **292.14 / 347.65 ms** passed unchanged bounds
**319.95 / 438.30 ms** against control **290.87 / 381.13 ms**. This is a bounded
regression check, not a speedup claim. Original control receipts remain unchanged;
`p6d-claim-backlog-control-gates.json` records the new comparison.

Windows native CLI/MCP/install smokes passed **3/3**, followed by the actual shared
pool regeneration, helper-failure and existing-session recovery smoke. All **84/84**
normal Off/Report latency comparisons passed unchanged frozen and preceding-phase
bounds (`artifacts/v190/p6d-gates.json`); all four measured cohorts completed before
phase acceptance. Supply-chain evidence is reused from `p6c-fresh.json`: all 24
protected input fingerprints, pinned tool versions, freshness and exception dates
were verified unchanged in `p6d-dependency-reuse.json`. No new advisory/network
audit is claimed. No extension source changed in P6d; its P6c test receipt remains
historical with its original Node-version qualification limit.

On neo, final source and all **795** declared inputs matched before/after.
Formatting, full workspace all-target/all-feature Clippy (**20.86 s**) and all
**38** focused Store cases passed; the fresh pair built in **3m36s / 4m55s**.
One final unchanged fairness run enforced **10 s** inline and passed at **6.400 /
3.413 s**, with zero overtakers. The manual Index moved from pending event 41 to
running event 42 in **68 ms**. Source bytes, hashes, publication/citations and
cleanup all passed. Receipt:
`artifacts/v190/neo-p6d/final-a87a1bc/qualification.json`. Its 30 backlog and 30
semantic-ready samples are same-candidate load observations, without regression
thresholds or a proven idle claim. Earlier failed receipts and the old `85f439d...`
model/installer macro remain separately identified; those macros were not rerun
on final source. Full Mac qualification after all P6a–P6f and before P7 remains
required. No Linux, installed-editor, Developer ID/notarization or release
publication qualification is inferred.

Version remains **1.8.8**, synchronized across native/extension lockfiles. README,
documentation index, working release record and Marketplace copy describe the
implemented source-priority behavior. The final phase commit is recorded by the
next phase's ledger entry; no PR, CI or publication is part of this checkpoint.


## P6e — bounded transport and truthful publication observations (3 November 2026, accepted)

Entry commit is completed P6d **`195eb7f371a331acd8c2d02314a3b9bf362f7aeb`**,
DCO-signed and pushed to `feat/finetuning`. Source remains working **1.8.8**.
The integrated native source is
`62db764503f4f76ede4cf512f30211aff0e3dd95aa8476062a739a124cbc4b59`
/789. It changes only four native inputs from D; UI/package changes remain
separate from that native fingerprint.

The shared stdio runner preserves the 65,536-byte physical line limit including
newline, rejects an overflowing chunk atomically without retaining its payload,
closes admission at terminal input and preserves typed overflow through SDK drain.
Clean EOF remains distinct. The unified frontend and legacy native handler share
the implementation. Actual SDK blocked-handler cases verify accepted work drains;
actual native process pipes cover exact-bound/newline, chunked/no-newline overflow,
content-free stderr/nonzero failure and malformed/EOF behavior.

The first three process tests failed before framing because their helper used
`common::command` without an owned Service; that deliberately selects a missing
installation instead of user state. The corrected helper holds an owned temporary
service through process/output drain, then uses its managed shutdown/reap and
asserts one unchanged source sentinel with no extra source entries. All original
framing assertions remain intact; independent review passed. The setup failures
are retained in `p6e-cli-pipe-missing-runtime-failed.log` and do not count as
framing results.

Windows remote/SDK unit tests passed **18/18**; final native pipe cases passed
**3/3 in 5.65 s**. Formatting and workspace all-target/all-feature Clippy passed
(**13.13 s**). The full all-feature workspace passed **1,989 tests with 39 skips**
in **1,139.429 s**; all doctest targets passed with no executable examples. The
source-bound release pair and final package/performance gates passed as recorded below.

The pure bounded publication comparator orders generations only within the same
captured authority, collection and incarnation. It preserves equal, older, newer,
different, inconsistent and unknown observations, including u64 boundaries and
conflicting envelopes. Host/client lifetime tests prevent late or foreign status
from relabeling retained turns. No comparison performs another status request or
rewrites retained results/exports. Review repaired unintended encoding changes
before integration. The Workbench bundle permits only this exact pure module,
and the package explicitly includes its compiled host module.

Pinned Node **24.15.0** passed the isolated extension compile and **32** focused
publication/lifetime/boundary cases. The integrated canonical extension command
passed **555 tests with 19 skips** (**574 total**, **6.813 s**), including all three
TypeScript graphs and guarded bundles. This is current pinned-tool evidence;
it does not relabel P6c's older Node receipt.

On neo, formatting, workspace all-target/all-feature Clippy (**18.54 s**), all
**18** remote cases and **three** actual pipe cases (**13.42 s**) passed. The
fresh pair built helper/main in **1m53s / 4m26s**, and all **795** declared inputs
matched before/after at the frozen source. Owned process cleanup passed. Receipt:
`artifacts/v190/neo-p6e/final-62db764/qualification.json`. No D fairness, model or
installer macro was repeated; those earlier receipts retain their source scope.
Full public-repository Mac qualification remains required after all P6 and before
P7. No installed editor, Developer ID/notarization, Linux or release claim follows
from these local tests.


### Final P6e acceptance

The final Windows pair `artifacts/v190/p6e-qualified-runtime` retains native source
`62db764503f4f76ede4cf512f30211aff0e3dd95aa8476062a739a124cbc4b59`
/789; main executable SHA-256 is
`e5cee25c9776c19b66f5ed4ddad2a8618d1400847352695d667ccc9e9db3f7b8`.
Helper/main release profiles completed in **2m18s / 4m31s**. Native CLI/MCP/install
smokes passed **3/3** (installer **95.90 s**) and the actual shared embedding-pool
regeneration/failure/recovery smoke passed. All **84/84** unchanged latency gates
passed against the fixed frozen baseline and immediately preceding P6d, without
adding older phase campaigns. Four cohorts each recorded **90 trials / 810 calls,
zero errors and zero retries**; each Report cohort retained **720 verified
receipts**. Report: `artifacts/v190/p6e-gates.json`.

The Python suite passed **404 tests with nine skips** in **20.770 s**; repository
checks passed for **1,578** public files. Dependency evidence reuse verified all
**24** protected hashes, pinned tool versions, receipt freshness and unexpired
review dates (`p6e-dependency-reuse.json`). The original full advisory receipt
remains `p6c-fresh.json`; no new network/advisory scan is claimed. Current Node
qualification uses pinned 24.15.0, separately from the original advisory tool
versions. Native/extension/lockfile versions remain **1.8.8**.

Independent review covered the complete native/UI/package diff and final
validation, including the final process-fixture correction. Exact pure-module
allowlists remain bounded; all original Unicode UI text is preserved. README,
documentation index, working release notes and Marketplace copy agree with the
implemented behavior. The next phase records the resulting DCO commit ID. No PR,
CI run, marketplace publication or user runtime installation accompanies this
checkpoint. Full neo public-repository qualification after all P6 remains pending.

<a id="p6f--catalog-management-and-scoped-native-activation-in-progress"></a>

## P6f — catalog management and scoped native activation

**Local acceptance, 4 November 2026:** implementation and Windows/native/editor
gates are complete, including all 84 matched isolated-native latency comparisons.
Focused Neo checks passed; the separately required full source-bound Neo campaign
remains pending before P7. Working version **1.8.8** remains unpublished.
The dated checkpoints below preserve earlier failures and incomplete states.

P6e was committed and pushed as
`df442dcfaf49d6dd5edb50feee7445513e4e64e3`. At the initial P6f checkpoint, production catalog activation remained disabled.
The continuation below enables guarded candidate execution. Version **1.8.8** is
unchanged. The explicit authority-1 physical format preserves the exact
authority-0/P5 layout and readiness contract. Backend recognition of authority
contract 1 was enabled only after all five physical-layout tests passed,
including atomic rollback, immutable provenance, separate imported/reviewed
capacity limits and the actual aggregate 32 MiB quota. This is format
recognition, not native installer or transport qualification.

Component checkpoints collected before the final integrated freeze:

- Core library: **87/87** passed with no skips. Later source-change and setup
  draft additions compiled through the Store campaign but their new Core tests
  have not yet run.
- Pinned Node 24.15.0 compile and affected trust/Indexing/native-console/client/
  Workbench export tests: **184/184**, no skips. These precede the subsequent
  grant/setup form additions and establish component coverage, not an installed
  editor claim. Three extension packaging/import-closure checks also passed.
- Store catalog lifecycle campaign: **97/102 passed, five failed, no skips** in
  42.36 seconds. The complete log is
  `artifacts/v190/p6f-catalog-lifecycle-check.log`. All source-change, setup,
  durable fencing and P5 regression cases passed. Four new owned legacy
  finalizer fixtures failed while obtaining their actual source configuration;
  one fresh finalizer fixture reported a foreign registry owner. The causes were
  incomplete native collection setup in the legacy fixtures and a noncanonical
  temporary path in the fresh fixture. Actual collection selection/writer/config
  setup and canonical paths fixed those fixtures without relaxing production
  guards. The targeted finalizer rerun passed **12/12**, no skips, in **4.98 s**;
  the original P5 private-source fixture also passed **1/1**. Logs:
  `p6f-finalizer-correction-r2-check.log` and
  `p6f-finalizer-known-source-check.log`. Subsequent source-reconciliation
  additions compiled but their affected tests still require a targeted run.
- The subsequent source-change run passed **14/14**, no skips, in **8.69 s**
  (`p6f-source-change-check.log`). It covers actual failed-job resolution without
  rewriting failure history, restart/requeue, authority-fence admission,
  seventeen-source removal, inactive imported-index retirement and exact backing
  lookup. The grant/full-fence UI checkpoint separately passed its pinned compile
  and **156/156** affected tests; later setup/selection/error-export additions are
  outside that checkpoint.
- The first immutable neo component snapshot
  `af7fbbceaad73f0c4c7cbd9227101bf301ad442d1928c5a6a100d4c91a732933`
  (**841 native / 843 captured inputs**) compiled Core/Store and passed **90 Core,
  seven Unix quota and five anchored-storage tests**, with no failures or skips.
  Before/after input hashes and cleanup were verified. Evidence is under
  `artifacts/v190/p6f-focused-setup-u1/evidence/`. This precedes later preflight,
  wide-inventory and scratch-resume changes; no native CLI, complete suite,
  release pair or signing was tested in that campaign.

- The later immutable neo setup snapshot
  `75da61bf820899e50f1ed3b2456fb08d8d77d1163cca8c9cf053ef06ed3fe010`
  (**848 native / 850 captured inputs**) compiled successfully and passed all
  **90 Core and ten quota tests**. Anchored storage had **six passes and two
  failures**, with no skips. The failures reported `unknown initial collection
  file preserved` during exact replay and `unsupported managed collection
  namespace` during existing-empty-leaf owner-publication crash recovery.
  Complete failed evidence remains in
  `artifacts/v190/p6f-focused-setup-u2b/evidence/`; input hashes, owned-process
  cleanup and retained P6e payloads were unchanged. No retry was run. The earlier
  u2 capture was rejected for changing input bytes and supplied no test result.
- The corrected setup component snapshot
  `94a1af0b76ba2677de0509b2da5696a4bec1b3b1a3368316dc21f05875722379`
  contains **267 consumed Core/Store/Cargo/configuration/fixture inputs**; copied
  CLI files are uncompiled provenance only. After independent source review,
  the affected neo run passed **nine storage and two initial-claim tests**, no
  failures or skips. Core and Store were verified freshly compiled; source bytes
  were unchanged. It recognizes only the exact native selected-slot gate and
  validates quota directories with their owning format rather than the managed
  collection-path parser. The failed u2b result remains historical evidence.
  An intervening harness run selected zero cases because an mtime-zero archive
  reused an old cached binary; minimum-count guards rejected it. A missing
  Cargo cache marker also stopped a clean preflight before tests. The corrected
  harness cleaned only its owned clone's Core/Store outputs and verified fresh
  compiler artifacts plus the exact test catalog before accepting the eleven
  passing cases. Prior Core/quota component results are not relabeled as new.
- The integrated Windows native CLI all-target/all-feature typecheck passed
  without warnings (`p6f-native-all-targets-check-r6.log`). Earlier complete
  diagnostics exposed mechanical error conversions, missing fixture fields and
  stale metadata-response patterns. Fixture updates preserve legacy no-scope
  registration and explicitly require absent fence proof on authority-0 metadata
  responses. This is compile coverage, not executed transport qualification.
- The later pinned extension build passed all three TypeScript configurations
  and bundle construction. **215/215** affected setup-journal, scope-selection,
  trust/native adapter and Workbench error/privacy/export component tests passed
  with no skips, followed by **nine/nine** bundle-boundary tests. An intermediate
  bundle failure exposed an accidentally omitted existing pure error registry
  in the exact allowlist; restoring that entry and an equality regression fixed
  it. These checks precede subsequent Use Grant/revocation/source forms, and do
  not exercise native transport or an installed editor.
- Canonical scoped MCP tests passed **eight/eight**, no failures or skips, in
  **7.31 s** after the test build. They exercise actual LocalTools settings and
  file-intent refusals, complete reviewed/imported metadata pages, continuation
  identity, the rmcp bridge and Unified help/status/error preservation. Setup
  errors explicitly retain an unconfirmed requested selector; uncertainty after
  dispatch does not claim no query was submitted. These are native-handler and
  in-process transport tests, not the pending full CLI/MCP/Workbench campaign.
- A subsequent integrated Windows check passed all native CLI targets/features
  without warnings (`p6f-native-all-targets-check-r7.log`, **46.70 s**). The full
  Core library then passed **90/90**, no skips, in **8.75 s**. The combined Store
  catalog selector passed **119/119**, no skips, in **26.90 s**, including exact
  provider-census persistence/budgets, preinsert-only busy classification,
  source-change recovery and all catalog upgrade/finalization regressions.
  Shared-helper crash-recovery ownership and normal worker-exit drain integration
  are subsequent work and are not covered by these checkpoints.
- The freshly compiled CLI library's focused selector collected **59 passes,
  fourteen failures and one explicit installer ignore** out of 74 cases
  (`p6f-cli-focused-tests.log`). Twelve failures came from Windows fixtures
  precreating permissive runtime directories instead of using the native private
  directory creator; one retained a now-stale authority-0 capability expectation.
  Independently reviewed test-only corrections preserve the production privacy
  guard and explicitly test an earlier same-version capability-0 payload.
  The remaining failure exposed a production Windows pinning defect: an
  attributes-only directory handle did not block renaming. A native owned-fixture
  API probe confirmed that traverse access participates in sharing checks and
  that writable directory handles support strict flushing on this host. The
  complete setup fix is being implemented through the shared staged backend with
  handle-relative Windows operations; this failed campaign is not qualified as
  passing. No ignored installer case was run without its actual payload inputs.
- The ignored scoped-parity campaign's four helper self-tests passed. This checks
  normalization and complete-envelope parsing only; no native transport or
  Workbench campaign has run from that fixture.
- The native Windows named-job selector passed six cases, including three
  meaningful kernel ownership/crash tests and three self-gated child fixtures.
  This does not qualify the subsequent shared-helper guardian integration.
- The later extension startup/legacy checkpoint built successfully and collected
  **283 passes and three failures** out of 286 component cases. The failures
  exposed a storage-preference mutation, a stale test-host module and Windows
  canonical-path alias handling. Corrections are under integration; these results
  do not qualify the unfinished source/recovery forms or installed-editor flow.
  The later Source/Resume build also passed. Twenty affected component suites
  collected **312 passes and five failures** out of 317 cases, no skips, in
  **2.38 s**. The remaining failures concern legacy-provider fixtures, explicit
  setup navigation, storage-preference expectations and guarded Codex setup;
  the new Source/Resume and native-error privacy cases passed. Full installed
  editor/native workflow qualification remains pending.
- The first Windows staged-storage/quota run collected all 27 selected cases:
  **ten passed and seventeen failed**, with no skips. All six shared-provider
  metadata cases passed. The common storage/quota failure was a native relative
  directory `.` reopen returning error 123 before the intended setup effects.
  The correction reopens the retained directory handle directly and validates
  its native identity. The affected rebuilt selector then collected **five
  passes and seventeen failures** across thirteen storage and nine quota cases,
  with no skips: the reopen returned access denied for both flushing and
  inventory. This remains an unresolved native primitive failure; failed hook
  and quota-pressure assertions remain intact rather than being weakened.
  A subsequent owned-fixture native probe isolated the difference: all thirty
  `ReOpenFile` attempts failed, while thirty empty-name native handle-relative
  opens retained the original directory identity. Minimal writable opens
  supported flushing and list-access opens supported enumeration. The staged
  adapter now uses that separate same-object operation, with identity checks and
  an interleaved independent-cursor regression. The rebuilt affected selector
  passed **26/28** cases, no skips, in **5.21 s**: all nine quota and five native
  saved-receipt/absence cases passed; twelve of fourteen Windows storage cases
  passed. Remaining failures are staged-root publication access and a parent
  replacement fixture encountering a rename-blocking handle. These are under
  review. The API observations and partial results do not qualify complete
  Windows setup or cross-platform behavior.
- The guarded-provider/reconciliation integration subsequently passed the full
  native CLI all-target/all-feature typecheck without warnings (**42.79 s**).
  This includes both native executables, the ownership-first Windows setup path,
  saved-request reconciliation and shared-helper consumers. An earlier complete
  diagnostic pass found a missing test trait import and unused recovery APIs;
  the latter were connected to their owning Supervisor paths. Execution remains
  pending. A later read-only audit found a guardian-death containment gap, which
  is being corrected before native crash/lifetime qualification.
- A fresh Core/Store library-test build completed in **1 min 14 s**. Current
  Core passed **90/90** in **9.07 s**. The focused Store catalog contained 49
  cases and collected **48 passes and one failure**, no skips: all twenty
  Windows storage, nine setup-quota, six allocation-settlement, seven shared
  provider metadata and four source-discovery cases passed. The remaining
  retirement-evidence fixture reused a key after retirement intentionally
  advanced the global epoch. A test-only correction obtains the native current
  key; the affected three evidence cases and actual P5 inactive-backfill case
  then passed **four/four** in **1.92 s**. Other passing source was unchanged;
  the original failed run remains recorded. These are component/native-filesystem
  results, not installed-runtime or editor qualification.
- The later extension recovery build passed and collected **316/319** component
  cases, no skips. The three remaining service/trust failures were corrected
  together: exact untrusted-versus-disposed reporting, waiting for the existing
  Console observation, and a valid webview-ready test message. Rebuilding and
  rerunning the affected service/trust suites passed **27/27** in **0.84 s**.
  Full retirement-evidence local tracking controls are a subsequent leaf.

- A later native all-target/all-feature integration check passed without
  warnings in **51.85 s**. The complete Windows CLI library run collected
  **433 passes, six failures and thirteen ignored cases** in **144.77 s**;
  Process collected **seven passes and two failures**, while the embedding
  executable tests passed **13/13**. All failures were collected before the
  combined correction: strict physical-schema fixtures, request-key reuse,
  native page-budget coverage, private runtime directory creation, and actual
  process/resource readiness. Correction verification remains separate from
  those original results.
  The corrected Process suite subsequently passed **9/9** in **0.28 s**;
  actual job member inspection identified console hosts alongside the two
  fixture roles. The six affected CLI cases then collected **five passes and
  one failure**: administrative paging still failed opening its fixture SQLite
  database. No further correction was made until all six outcomes were collected.
  That fixture was then initialized through the native empty-publication API
  before configuring roots, retaining the real multi-page body, complete frame
  checks and exact Roots reassembly. The affected case passed in **5.16 s**;
  its rebuild completed without warnings. The original full run and both
  affected-run results remain distinct evidence.
- The frozen focused Neo provider snapshot (`caf16f99…44b95a`) passed its
  all-target/all-feature check and built an unsealed standalone component
  executable pair. All three actual helper lifecycle qualifiers passed:
  startup/completion, abrupt service exit with predecessor recovery/GC, and
  guardian termination while a real grouped descendant held a resource and
  Pool input remained open. Across eight selected cases, **seven passed and
  one private-directory fixture failed**. Three platform warning messages were
  recorded. The failed receipt is preserved; this is not warning-free or full
  macOS qualification, managed activation, or accepted model-work coverage.
  A separate frozen corrective snapshot (`c721a854…377ac8f3`) then passed the
  warning-free all-target/all-feature check (**43.97 s**), fresh CLI test
  compilation (**99.53 s**) and only the previously failing normal recovery
  case (**0.46 s**). The earlier three actual lifecycle passes remain attached
  to their original snapshot; they were not repeated or relabeled.
- Following the concrete Source configuration contract correction and first
  Workbench opening/export fixes, the full compiled JavaScript suite passed
  **640 tests with zero failures and nineteen explicit native/platform skips**
  in **6.91 s**. The skipped native transports and installed editor behavior
  remain unqualified by this component run.
  The Source correction removes two false identity equalities: a concrete
  collection incarnation and settings digest are independent of the registry
  namespace and backing binding. Structural validation and complete native
  request/preview/original-Begin continuity remain required. The older
  `prepare: true` configuration template retains its separate contract.
- A fresh offline Windows runtime pair built the helper and feature-minimal
  main separately (**84 s** and **83 s**), verified the embedded inventory and
  staged its native source receipt. Both explicitly selected real helper
  lifecycle cases passed: startup/completion (**0.93 s**) and abrupt service
  exit followed by predecessor recovery (**1.09 s**). They do not exercise
  accepted model work or installed activation.
  The first native CLI/MCP/Workbench parity campaign stopped after **29 calls**
  (**7.68 s**) when the read-only SourceDraft probe correctly refused a pending
  derived vectors job. Source bytes were preserved; parity was not qualified.
  Fixture ordering is under review, without canceling accepted work or relaxing
  native source-change admission.
  After bounded native observation allowed the accepted vectors and verification
  work to succeed before pausing, the second campaign passed the actual native
  SourceDraft-to-compiled-validator bridge (six positive/crossed cases). It then
  stopped at **74 calls / 19.24 s** on a CLI/MCP input-schema comparison. Full
  catalog diagnostics are being prepared to distinguish intentional transport
  selectors/public aliases from native input drift. The overall campaign remains
  failed, and source bytes remained unchanged.
  A smaller owned registration-only catalog capture then retained both complete
  catalogs without indexing or activation. Its strict differences identified
  public aliases, optional MCP project routing, bundled UI-help inputs and ten
  UI-only tools. A declared projection checker preserves the raw values and
  compares every remaining native input and tool-metadata field; its captured
  baseline and twelve tamper cases passed offline. It rejects semantic/default
  data masquerading as documentation instead of normalizing it.
  The audit also found two real help-input inconsistencies: Unified rejected
  valid topic qualifiers, and selected public tool aliases could return the
  same native error on both sides of an insufficient test. Canonical qualifier
  delegation and exact-first known alias lookup corrected these paths.
  Unified's nineteen tests passed, followed by fourteen focused help tests
  including positive actual scoped-handler schema/observation equality. A fresh
  runtime pair incorporating the correction was used for the following native
  transport campaign.
- The third native campaign retained its failure after **76 calls / 19.60 s**:
  the fixture copied an outer response-presentation control into a primitive
  Parse step. The fourth retained a separate **79-call / 20.16 s** failure:
  its one-call plan budget covered publication pinning but left no primitive
  read. The corrected fixture keeps primitive and transport inputs distinct
  and budgets exactly two calls; native budget enforcement is unchanged.
  The fifth campaign passed **228 transport calls in 57.99 s**, preserving
  source bytes. It retained 86 complete read comparisons across scoped search,
  twelve evidence families and four selected native-help inputs. Thirteen
  private probes exercised metadata scope, usage purpose, malformed purpose
  and actual unredeemed credential expiry. The bounded child authenticated
  native peers and kept credentials in memory; timeout or malformed framing
  could not count as an observed refusal/EOF.
  The actual native SourceDraft-to-compiled-validator bridge passed six cases.
  Two actual Workbench receiver/copy/export cases retained full and selected
  JSON/Markdown data, including a native refusal. Their VS Code host is a stub
  and the process opener connects to the owned fixture; this is handler and
  transport evidence, not installed-editor or managed-launcher qualification.
  All owned Workbench MCP children exited without forced cleanup.
- The subsequent required format check exposed three formatting differences,
  which were corrected. Clippy exposed oversized Core enum variants; heap
  indirection preserves their complete payloads and serialized wire contract,
  with unboxed-reference byte/digest regressions. All **91 Core tests passed**
  in **8.71 s**. Store lint corrections preserved full saved observations and
  receipt precedence; the next Clippy run passed Store, Kernel and MCP but
  exposed CLI lint failures. Private actor payload indirection and borrowed
  owner context preserve complete responses, guarded delivery and resource
  ownership. After correcting a missing test trait import, workspace Clippy
  passed **all targets and all features with warnings denied** in **20.40 s**.
  Complete earlier diagnostic sets remain retained. Formatting and complete-diff
  whitespace checks passed. Repository checks passed for **1,678 public files**;
  version checks agree on **1.8.8**, and design checks passed 43 wire fixtures
  plus the 24 workspace outcomes and U0–U6 contract cases. The all-feature
  workspace test run and its remaining-target collection reported **2,287
  passed, 28 failed and 38 explicitly ignored**. These totals count only the
  final summary of each Cargo target, excluding nested fixture summaries.
  CLI library tests passed **439/439**; Kernel library tests passed **171/171**
  with one explicit ignore, including the large-catalog regression (**409.11 s**).
  The generated CLI reference was stale; canonical regeneration and its exact
  verification test subsequently passed. The original failure remains retained.
  Store reported two failures: a path-dependent boundary fixture and corrupt
  configuration-result recovery. Twenty-five CLI integration failures were
  collected across fourteen targets; the shared legacy Configure admission
  regression and typed catalog-outcome assertion are under correction. The
  full workspace test gate remains failed pending affected reruns.
  The coherent admission/recovery correction then passed warning-denied
  workspace Clippy (**38.69 s**), all **35 focused Store cases**, and the full
  Store library (**665 passed, zero failed, three explicit ignores; 210.54 s**).
  Thirteen of fourteen affected CLI targets passed, including actual model
  setup, catalog upgrade, configuration, helper-outage, MCP, read-retry, PDF,
  held-helper service responsiveness and vector-selection paths. Only two
  Usage cases still failed. They invoked an engine-bound administrative client
  from a foreign test executable, which native preflight correctly refused.
  The tests now use the real CLI; complete native refusal preservation and the
  targeted Usage/administrative check are under validation, without relaxing
  engine/instance checks or introducing replay.
  The pinned offline dependency gate passed its requested checks with the two
  existing unmaintained-package exceptions. Its receipt remains **limited**:
  cached advisory/import evidence was not refreshed, npm advisories were not
  checked, and fresh/yanked-package coverage is unqualified.
- A separate metadata-only native campaign stopped after **53 calls / 14.85 s**
  on its incorrect expectation of positive partial-subtree status counts.
  The captured native `scoped_read_unavailable` response retained the complete
  selected ScopeObservation. The owning Kernel contract deliberately refuses
  corpus aggregates for a path-restricted scope; its regression requires that
  refusal. All owned children exited and source bytes were preserved. The
  failed report remains evidence; metadata transport parity is not yet qualified.
  After correcting only that fixture expectation, the metadata-only campaign
  passed **80 calls / 18.20 s**. Partial status and diagnostics retained their
  complete native refusals across CLI, MCP, shell and the status convenience
  command. Capabilities succeeded for both scopes; full-source status positively
  counted the two indexed documents and full diagnostics succeeded. Existing
  MCP and new CLI connections both refused after revocation, preserving their
  distinct submission-phase details. Source bytes remained unchanged and all
  owned children exited. This does not qualify partial-subtree aggregate counts.
- The remaining Usage failures now have passing affected reruns. The tests
  exercise usage reports through the actual CLI engine rather than the foreign
  integration-test executable; the native engine-binding guard remains intact.
  A negative test retains the complete `service_rebind_required` response.
  Two Usage cases and four administrative handler cases passed. The accounting
  case then exposed an unsettled physical-audit fixture: it now awaits an actual
  successful Verify job before holding the writer exclusion and passed its
  exact rerun. All 28 failures from the original workspace run have passing
  affected reruns; this is not a new complete workspace pass.
- The lifecycle capture and installed-editor driver passed the combined host,
  webview and assistance TypeScript build. The JavaScript suite passed
  **645 tests**, with **19 explicit skips** and no failures. A shared lifecycle
  DTO initially pulled host-only Node dependencies into the browser build;
  moving that unchanged DTO to the existing browser-safe contract corrected
  the dependency boundary. The full native lifecycle response is retained as
  historical data, with explicit complete JSON copy/export and a redacted
  default diagnostic summary. All four native-read source-contract tests also
  passed. No installed-editor scenario has run.
- The managed-provider audit found a real cutover gap: its normal ensure path
  bypasses the copied installer's Genesis/Upgrade coordinator and the settlement
  callback. A first owning adapter slice using the existing journals passed
  independent source review and workspace all-target/all-feature Clippy with
  warnings denied. All **19 new component cases** have passing results across
  the initial passes and affected reruns. The first run exposed missing control
  directory creation in the owning path; the correction preserves the strict
  lease precondition and refuses to recreate lost initialized control state.
  Existing Genesis **15**, installer **136** and helper-engine **14** tests
  passed, with **7 explicit installer skips**. These are component contracts,
  not executed managed activation or provider containment qualification.
  The explicit instrumented-helper target and all three held-engine owner
  scenarios have type-checked; their process fixtures have not run.
  Installer quiescence also needs actual shared-provider
  containment evidence after abrupt predecessor death; lease absence alone is
  insufficient. Production activation remains disabled.
  The installed-editor driver now uses actual setup/Project/grant/Use forms;
  its compiled dispatch, refusal and cleanup assertions have been reviewed.
  Native lifecycle completion must be retained in full for review rather than
  inferred from a shutdown acknowledgement. No new installed-editor or managed
  activation qualification is claimed.

These checkpoints do not qualify managed activation, installed-editor behavior,
single-index performance, the complete Windows suite or latest-source macOS.
Actual installer predecessor probes, accepted engine work with sibling resource
drainage, and the remaining qualification are still required. The
full frozen public-repository build/test checkpoint on neo remains mandatory
after P6f acceptance and before P7; no release, signing or publication is implied.


### P6f continuation after checkpoint b523e50 (4 November)

The requested checkpoint is `b523e505685f3558e9a9240cb87718bd590884d0`,
with the configured human DCO trailer. Nothing was pushed. The working version
remains 1.8.8. The initial continuation passed formatting, workspace all-target /
all-feature warning-denied Clippy and repository checks (1,692 public files).
The extension build/suite passed 645 cases with nineteen explicit skips.
Python collected 394 passes, one error and nine skips out of 404 cases: the
error was this sandbox account's Git ownership refusal in the exact source-tree
fixture. Its targeted rerun passed with a process-local safe-directory exception;
no repository ownership or global Git configuration was changed.

The installer quiescence audit found that the legacy embedding lease alone did
not cover the separately recorded shared helper. Maintenance now holds actual
service/worker/embedding/watch exclusion and uses the canonical native helper
recovery adapter, preserving recorded service identity and terminal history.
Four focused component cases passed; four explicitly native cases were ignored
in that selector. The freshly staged instrumented pair subsequently passed actual
startup/completion and abrupt-exit installer maintenance, with all owned command
jobs empty at cleanup. The latter preserves the last recorded service identity
and its exact terminal helper row, including an idempotent second observation.

The first pair build withheld its receipt because the ignored runner changed
while it was running. No qualification is claimed from that output. A frozen
copy of the current runner produced receipt
`3cf25015b1bbdb5c4d3b6fd126f990337b83e300b9ccc9640f6255c251db4efb`
under `artifacts/v190/p6f-resume-guarded-pair-r2/`, bound to native input manifest
`6f28b7615cb2962427c985d7da75cf14470c4409c35b8416cc9c3741dd8a5dc5`.
This is an unsealed cfg(test) helper and actual minimal main, not a release pair.

All three accepted-engine-work scenarios initially failed because their common
fixture requested unsupported query concurrency 1. Using the supported minimum 4
allowed restart/WDO recovery to pass. Ordinary exit and Retire then failed only
on an expectation that the broker removes fencing tombstones. Their corrected
checks preserve the tombstone count and additionally assert allocated slots/bytes
remain charged during held work and are released afterward. Both affected cases
passed against the same instrumented pair. Logs and actual test-binary hashes
remain in the three `p6f-resume-native-qualification*` records; the earlier failed
runs are not relabeled. Source preservation, sibling semantic reads, complete
worker/provider obligations, uncertain accepted outcomes and terminal replay
were exercised. These are native component qualifiers, not installed-editor or
managed-upgrade acceptance.

The final activation candidate is under validation. It permanently refuses P5's
old production settlement entry point; only qualified authority-1 settlement can
activate. Exact fresh external reservations may settle Genesis under actual
native exclusion, while their health path still cannot elect a service. Legacy
external layouts remain on their existing read-compatible path. The source flag
is enabled for candidate execution; this is not yet accepted P6f cutover.
The activation changes passed formatting and workspace warning-denied Clippy.
Actual managed first-pin/reconnect/external-health execution, complete upgrade /
installed-editor qualification and final source checks remain outstanding.


The actual managed on-demand and external first-pin/reconnect/native-health case
passed on the sealed activation pair (22.05 s on the later r3 pair). A real
predecessor upgrade initially stopped on the old artifact's strict rejection of
`native_provenance` before its later authority fields. The precise saved-format
classifier and positive/parser/connection regressions were corrected. A new owned
upgrade then passed with unchanged source and original manual MCP configuration,
actual predecessor read/write refusal provenance, settled authority-1 readiness,
post-health and complete native Stop. The failed earlier fixture remains retained.

The first installed VSIX Restricted Mode case was blocked by Electron's install
ACL check when launched from the execution sandbox. The same isolated test passed
outside that sandbox, with Electron sandboxing and machine ACLs unchanged. The
first trusted management attempt exposed a driver error: a read-only Connect click
was recorded as connected immediately and used as fresh installation. The driver
now uses the explicit Start service action and waits for authenticated settled UI
state. That next run exposed a production copied-first-install ordering defect:
a legacy service was probed before fresh Genesis. The source correction qualifies
fresh metadata before its first service start and preserves the actual absent
public selection until native health succeeds. Native namespace initialization
and accepted prefix recovery use the existing claimed layout and actual leases;
no synthetic preceding selection or arbitrary missing-directory repair is used.

The r3 sealed pair passed actual copied first install for both default and separate
state roots, including complete replay without service replacement (68.48 s).
Its initial native-prefix fixture failed before admission because it omitted the
real control-directory preparation step; the managed case passed after that full
run. The fixture correction now uses actual quiescence and native role preparation.
The affected native-prefix rerun passed (114.25 s) without changing its original
namespace. These results do not qualify the unfinished
trusted installed-editor scenarios, final complete workspace or performance gates.

The final release pair built successfully with install-manifest SHA-256
`4fc35d52d163b9e236708989884d399a0466d2b0adaf2ce40c91f7b1f19fe86b`.
The first final extension suite exposed a fixed 200 ms descriptor-publication wait;
it now observes actual publication under a bounded deadline. Its rerun passed
645 tests with nineteen skips. The final Python run passed 402 tests with nine
skips (411 total); repository checks cover 1,694 public files. Formatting,
warning-denied workspace Clippy and workspace doctests passed.

The first final trusted installed run reached native setup and persisted grants.
Window B then refused Use Grant because its extension retained the startup
absence of a runtime after window A had installed it. Its authenticated console
connection was live. Explicit context read now adopts only that already verified
connection, preserving client/instance/trust fences and withdrawing stale scope.
It performs no discovery, registration or implicit grant selection. The extension
suite now covers both prior installation and installation by another window:
646 passes, nineteen explicit skips. Failed owned editor fixtures remain retained;
their missing complete native Stop receipts are not claimed as qualified cleanup.


#### P6f continuation, 4 November 2026 — installed forms and native setup

P6f remains under qualification; P7 has not started. The checkpoint is committed
as `b523e50` with the authorized contributor's DCO. Subsequent corrections are
still working changes. Only one agent/session is active; no reset or independent
review is claimed. Failed fixtures and their incomplete cleanup observations are
retained separately from passing cases.

The installed 1.8.8 r6 VSIX passed same-source and different-source windows
(`native-installed-GIoHFl`), actual multi-root reload (`native-installed-GTMKJo`),
Usage exports and Restricted Mode (`native-installed-VcKOHU`), and an unchanged
original manual MCP configuration after an actual predecessor upgrade
(`native-installed-O7KMLG`). Completed scenarios retain native Stop receipts.
These records use the release pair whose manifest begins `4fc35d52`; they do not
qualify later native corrections. The extension fixes preserve explicit scope
through Index Now, MCP definition resolution and Activity refresh, adopt an
already authenticated peer-installed binding, and avoid an unintended forced
reconnect. Driver corrections reattach only read-only CDP observations after
view replacement, tolerate the exact old-host reload cancellation, and close
owned untitled Usage exports without an interactive Save prompt. Mutation dispatch
is never replayed. The later full extension run passed 646 with nineteen skips
(665 total).

Managed native setup initially refused before reservation: its source-isolation
reader did not recognize the new typed `managed_catalog_origin`. The reader now
validates paired receipts and their install/state/control/incarnation binding,
retains the external payload as a private boundary, and rejects unknown fields.
All five source-isolation tests pass. The actual first-pin fixture now applies a
reviewed tiny-file setup under both managed lifecycle policies, verifies source
preservation and unavailable legacy compatibility authority, and passed in
25.69 seconds on the corrected debug pair.

Reconnect then exposed a separate live rollback-journal classification defect.
Completed Genesis validation now distinguishes actual service-lease exclusion
from the strict stopped-owner recovery lane. The live lane uses the canonical
read-only registry reader; unknown stopped overlays still refuse without repair.
All 21 Genesis tests pass, including the live transaction and orphan journal
regression. A successful native status diagnostic preserves the complete selected
scope observation and confirms that scoped status deliberately rechecks authority
rather than using the legacy frontend scalar cache.

The original local four-core campaign is failed evidence: five native-client/model
runs passed, while five old smoke fixtures and five installer fixtures failed.
The lifetime fixture now creates native reviewed setup/membership/grant authority
and reuses that exact fixture scope across launcher aliases; actual abrupt client
loss, sibling preservation and EOF passed through both tested aliases before the
next installer failure. That failure reproduced an already-activated catalog
update incorrectly rejected as `catalog predecessor equals candidate` when only
package inventory changed. The distinct-engine requirement now applies to actual
migration admission/resume; a compatible activated catalog retains its existing
no-backfill update path. Final native execution is still pending.

A missing imported compatibility grant also surfaced as temporary busy. Native
connection classification now preserves `ScopeReadRefused` as the existing
permanent `scoped_read_unavailable` contract, including the canonical MCP error
receipt. Scoped callers retain the complete native refusal in their existing
setup envelope. Focused classifier and frontend receipt regressions pass. Smoke
fixtures now distinguish deliberately selected grants from unselected source-only
connections and preserve the original scalar byte bound alongside the additional
complete scope provenance. Final transport parity, installer/smoke execution,
performance gates, source review and package identities remain outstanding.


The corrected debug pair passed all seven actual native MCP smoke cases
(`p6f-resume-native-smoke-r5.log`, 203.50 s), including four cold scoped clients,
four cold unselected source clients, complete status provenance, 24 successful
mixed scoped reads and one deliberate permanent source-only refusal. Six actual
publication-busy receipts were handled by the existing typed retry helper; they
are retained, not reported as zero-retry observations. The complete installer
fixture passed after observing the documented 60-second healthy-owner interval
before another rapid stop/update/rollback sequence (`p6f-resume-install-r4.log`).
The prior circuit-open failure remains historical evidence; neither its limits
nor production counters were bypassed. Installer failures now retain their owned
fixture rather than deleting ambiguous state during exception unwinding.

The affected complete Store run passed **765/765**, with four skips, in 179.56 s
(`p6f-resume-final-store-tests-r4.log`). The actual managed setup/lifecycle-policy
case also passed on the corrected debug pair in 23.95 s. Warning-denied complete
workspace Clippy passed after moving the new classifier test module to the end of
its file. The later Python suite passed **402**, with nine skips (411 total), and
repository checks passed for **1,695** public files. These supplement the earlier
full workspace run and corrected targeted reruns; that original run's nine
failures are not relabeled as an all-pass full run. The final release rebuild,
new package/transport execution, corrected four-core campaign and frozen latency
comparisons remain pending. No new full Neo campaign has run; the focused Neo
component snapshots above remain their own historical evidence.

#### P6f local acceptance — 4 November 2026

The final native input identity is
`386c0882bd5b657ecfe85c67f7af121a04e6c8c868ed8eb92a64fb3c5b28ba18`
(**893** inputs). The release pair manifest is
`9ca2560e06af7a03a3c7001f28cf212b9bca0c5f4bd337eddde93c55f29e0a2d`;
main `4e6c39fb6f2d0e14ac241a8c51bb7ca89942adb24d4b90c091a9eb6721253d5c`,
helper `3953c420c7508003e840bac317bccdd0120afe1b0015969d03419670e1bd44c7`.
The verified r8 Windows VSIX SHA-256 is
`845c4087ff6bb6b8253513bb6bede3c022a1a67d535c1caafc262bb944904562`.
Native ZIP/VSIX packaging checks passed. These remain local working artifacts;
no push, PR, publication, promotion or signing qualification is claimed.

Actual installed VSIX r7 passed all six scenario families: same-source and
separate-source windows, real multi-root reload, Usage exports/clear, Restricted
Mode and unchanged original legacy configuration after actual upgrade. Evidence
is in `native-installed-bJJecU` and `native-installed-mVXk4D`, with complete native
Stop and source preservation. Neo's first focused run then found one Unix-only
Clippy error in a test condition. Its correction preserves the assertions and
all production bytes. The r4/r5 payload comparison verifies all **675** payload
files differ only in `airk-build.json` and the enclosing install manifest. The
r7/r8 VSIX comparison verifies all **769** entries differ only in those receipts
and their two enclosing provenance descriptors; every extension/executable byte
is identical. Full correspondence receipts are retained. The new r5 identity
also passed seven actual MCP smoke cases, the complete installer lifecycle,
**219** scoped handler/CLI/MCP/Workbench calls and **73** metadata parity calls,
and a fresh actual predecessor/installed-editor legacy scenario with complete
native Stop. The earlier whole-UI records are not relabeled as r8 execution.

The corrected four-core campaign passed **15/15** suite runs: five smoke runs
(**7/7** each), five native-client/model runs and five complete installer runs.
The fixture observes the existing stable-owner interval before another rapid
restart sequence; it does not reset/bypass the production circuit. Logs and
all outcomes are under `artifacts/ci-local/20261104-103328-73ea822a/`.
This campaign used r4; the exact production-byte correspondence and additional
r5 identity checks above retain that distinction. No hosted CI was used.

The initial performance wrapper accidentally compared an installed launcher
candidate with isolated-native controls. Its **52/84** outcome and raw records
remain retained under `p6f-final-*`; it cannot qualify the plan's isolated-native
gates or establish an installed-launcher regression against a matching installed
baseline. Installed startup observations remain separate. The corrected private
wrapper preserves actual native predecessor upgrade/sealed imported authority,
then invokes the same isolated approved-runtime route as the frozen controls.
It changes no benchmark workload, warmup, sample count, filter, budget, model or
threshold. The canonical benchmark script is unchanged.

All **84/84** corrected comparisons passed in `p6f-matched-final-gates.json`,
including frozen/archived P5/contemporary P5 cumulative controls and the preceding
P6e controls. Bounds remain median +max(10%, 25 ms), p95 +max(15%, 50 ms).
Candidate Off and Report each completed **90 trials / 810 calls**, with complete
mandatory evidence/publications, preserved source bytes, **zero errors and zero
retries**. Off emitted zero verification receipts; Report retained all **720**
required verified receipts. Both contemporary controls retain the same counts
and mode-specific receipt coverage. Candidate fixtures retain complete native
Stop. These are isolated-native latency results; installed/editor observations
and their platform/signing limits remain separate.

Neo's corrected immutable focused snapshot passed warning-denied all-target CLI
Clippy, **20** selected native cases, **25** selected Store cases and five actual
native fixtures: shared-helper maintenance, abrupt-owner recovery, native helper
completion, copied Genesis and managed first-pin/setup for both lifecycle
policies. Before/after hashes retain the same 893-input native identity. The
failed initial snapshot is preserved separately. The focused debug pair is
ad-hoc signed. A further actual Unix guardian-death/descendant-containment
case passed (2.93 s) with unchanged source, bringing focused execution to 51
passing tests. These are not the required full Neo campaign or
Developer ID/notarization/installed-Mac-editor qualification. The pinned Rust,
nextest and Node are available. Verified repository-pinned audit/deny/vet tools
and portable PowerShell were prepared inside the owned qualification directory;
no global installation or ACL was changed.

Formatting, warning-denied workspace Clippy, the earlier full workspace plus
explicit affected reruns, all 765 Store cases, doctests, Python (402 passes,
nine skips), extension (646 passes, nineteen skips), version synchronization and
repository/whitespace checks constitute the local source evidence. Counts and
limitations from the earlier full run are preserved above. The combined owning
paths and validation evidence received a primary-agent review; no independent
review is claimed. Offline dependency policy remains limited: fresh/yanked/npm
advisory qualification and baseline review exemptions are not relabeled as a
fresh full release audit. Full Neo source-bound qualification is the next
required checkpoint before P7; Linux, Intel, credentials and publication gates
remain separate. P7 has not started.

#### Full Neo checkpoint started from committed P6f — 4 November 2026

P6f local acceptance was committed as `1837f586b005ab0f26f7f0ad1f20aedbe5939017`
with the authorized human DCO. The post-P6 full Neo campaign uses a new owned
public-only snapshot of that exact commit: **1,694** source files, native identity
`386c0882bd5b657ecfe85c67f7af121a04e6c8c868ed8eb92a64fb3c5b28ba18` (**893** inputs),
archive `7f30d56188087863e2a2214ce847c041dca28bc911148acdf627de03e26aeb9d`.
Its generated shallow Git metadata contains only the current public commit's
reachable objects; the original Git directory, ignored data and unrelated
untracked audit note were excluded. Source and qualification-harness hashes are
checked before and after execution. No P7 work has started.

The initial complete Neo extension suite ran **665** cases: **634 passed, one
failed, thirty skipped**. The stable-executable fixture incorrectly expected mode
0755 after atomic creation under the campaign's restrictive umask 077, which
correctly produces 0700. The test now compares against the existing process
creation mask without changing permissions or umask. A separate immutable source
copy, differing only in that test, passed **635/635** executed cases with thirty
skips; its before/after correspondence verifies unchanged production/native
sources. The Windows extension suite also passed **646**, with nineteen skips.
The original failed run and its exact source remain retained; they are not
relabeled as a passing full campaign. Full native build/tests and the remaining
Neo checks continued from the unchanged snapshot; the final outcomes below supersede this pending status.


The original full native run also exposed two host-specific fixture conditions.
The administrative pagination fixture built approximately 1.4 KiB paths, beyond
macOS PATH_MAX. Its correction retains the sixteen-root native limit and every
size, continuation and complete-result assertion: Unix filenames contain literal
backslashes, whose JSON escaping keeps the response above 20 KiB while each
filesystem path stays below the platform limit. Windows keeps its original
extended-path geometry. An initial local attempt to use thirty-two shorter roots
correctly failed the native sixteen-root limit and remains retained; the accepted
correction does not change that limit. The corrected Windows fixture passed and
warning-denied workspace Clippy and formatting passed.

The unchanged configured-capacity fixture failed its real sixty-four concurrent
channels under the SSH shell's inherited soft descriptor limit of 256 (hard limit
unlimited). A separate owned child shell with soft limit 4,096 passed the exact
unchanged fixture, preserving all channel, capacity, identity, source-preservation
and shutdown assertions. Only that child shell's limit changed; no global limit,
ACL or application policy changed. CONTRIBUTING now records this qualification
prerequisite. Original failures remain distinct from these observations. The
supplementary capacity Cargo invocation reused the target cache after the full
campaign had finished its CLI fixture phase; later tests use their original
compiled binaries, and the sealed release payload is unchanged. Further builds
and correction checks waited for the complete campaign to finish.


#### Full Neo checkpoint completed — 4 November 2026

P6a–P6f and the required post-P6 Neo checkpoint are complete. P7 has not started.
This completion supersedes the pending statuses above; it does not relabel the
original campaign as an all-pass run. Full evidence is retained locally under
`artifacts/v190/p6f-neo-full-public-r1/`, including the original failed results,
separate immutable correction snapshots and `qualification-summary.json`.

The exact committed source `1837f586b005ab0f26f7f0ad1f20aedbe5939017` passed
release-pair construction, workspace build, formatting, warning-denied full
workspace/all-target/all-feature Clippy and doctests. The complete native catalog
ran **2,296 tests across 67 binaries: 2,294 passed, two failed, 46 skipped**
(1,433.645 s). The two failures were the administrative pagination path geometry
and the sixty-four-channel fixture under the SSH descriptor limit described
above. The corrected native administrative module passed **4/4** tests on Neo;
the unchanged capacity fixture passed with the owned shell's soft limit 4,096.
All original assertions, concurrency, budgets and retry rules remain intact.

The release runtime passed **7/7** actual MCP smoke cases, native clients/model
qualification (**two projects, sixteen files per project, eighteen client tools
and the complete twenty-eight-tool MCP catalog**), complete installed lifecycle
qualification and an actual retained-predecessor upgrade. That upgrade retained
the original manual MCP configuration and source hashes, settled authority-1
through the native coordinator, read the sealed `imported_legacy_snapshot`, and
completed native Stop. Source bytes were preserved by the native client campaign.
The installer verified abrupt-client loss, sibling preservation, stopped-owner
repair, exact-payload health, upgrade, rollback, retirement and resumable uninstall.

All three vector-component workloads initially refused the harness's U8 selection
on ARM64. They passed **3/3** (40.30 s) with the provisioned, hash-verified S8
profile selected by the native CPU policy. No model admission check, workload,
threshold or assertion changed. The existing opt-in CI step now selects S8 for
ARM64 and U8 for x64; routine suite selection and the matrix are unchanged.

The full Python run initially reported eight error observations across five test
methods (five subtest errors in two bootstrap tests and three legacy native
fixtures), with three skips. The bootstrap mocks had intercepted Python's own
macOS platform subprocess; those tests now isolate the platform observation as
existing sibling tests already do. The other three fixtures require the removed
pre-1.4 standalone command syntax, already documented as historical in
`docs/windows-local-build.md`; they now require explicit separate legacy binary
and model inputs. No legacy input was supplied, so those three remain unavailable
and are reported as skipped, not successful current-service qualification.
Current-service coverage is the actual native campaign described above.

The corrected complete Python suite passed **405 tests, six skipped** (411 total),
including the actual compiled-extension/parser/license packaging checks. The
first correction snapshot lacked generated npm/extension inputs and passed with
nine skips; that record remains retained. After locked offline npm preparation
and compilation, the final rerun exercised the three packaging checks. One
supplemental launcher initially lacked `python` on its SSH PATH and failed before
starting tests; its log remains retained and the final runner uses the pinned
interpreter and tool environment. Windows Python passed **402, nine skipped**.
The corrected Neo extension suite passed **635, thirty skipped**; Windows passed
**646, nineteen skipped**. Its original 634/one-failure/thirty-skip run is preserved.

The public-only snapshot exposed a documentation-index link to the unrelated
untracked audit note. The note was not copied or committed. Its dangling public
link was removed; its local bytes remain unchanged. Corrected repository checks
passed **1,694 public source files** on Neo. Licenses, whitespace and native/VSIX
packaging passed in the original full campaign. Before/after verification proved
all 1,694 public source files and the frozen harness unchanged. The correction
snapshot verifies every source byte against the original snapshot plus its eight
declared fixture/configuration/documentation changes; its native identity is
`9de048920ff36f71bfe52633ca983368c372fea64cb3c6df6cfa4dd4c0a70929` (**893** inputs).
Only the Rust test module changes native inputs, and that module is `cfg(test)`;
production Rust and extension implementation sources are unchanged by this
qualification correction. The final documentation records the observed outcomes.

The tested Neo release pair and packages remain bound to the original committed
source identity `386c0882bd5b657ecfe85c67f7af121a04e6c8c868ed8eb92a64fb3c5b28ba18`,
not relabeled as builds from the later test/documentation correction:

- Install manifest: `f3d54c6bf40208d51785bf139a69844ee61013fcd0cb79b4c7161d72e6a3c1cb`.
- Main: `12433069e8227262b72615c4419540e77df30214ddb3bd12b5c198eccc5f554c`.
- Helper: `62434e1d47178a8efdf163b8c4b7cd3332dcaccc900d638dcb3bb6d50902ce75`.
- Native ZIP: `40ecefd28025f9a9b12fecbd4231b632a0acea2fa55be970d076d0d51593a0c7`.
- ARM64 VSIX: `75662f851a292b1e2036c70b055b802ef3502ca54ebf73519eac9d87a82bbcb7`.

Neo used pinned Rust 1.98.0, nextest 0.9.148, Node 24.15.0 and Python 3.13.18
on macOS 26.5.1 ARM64. Runtime signing is ad-hoc. Developer ID/notarization,
installed Mac editors, current Linux/Intel and the full release matrix remain
separate qualification; no cross-platform release success is claimed. The offline
dependency check passed all requested checks with a **limited** result, two
exceptions and no fresh/yanked/npm advisory claim. No global tool installation,
ACL, global resource limit, installed user runtime or private/index-state export
was required. No P7, push, PR or publication occurred. Version remains **1.8.8**.
The primary agent reviewed the combined change and validation evidence; no
independent review is claimed.

## P7 — combined retrieval and consumers

Started 4 November 2026. Initial development checkpoints below retain their
observed failures and pending states; the final local qualification follows them.

Entry commit: `83d6084b14326e057fc85cda9dc0b717c1c36f41`. The required P6f/Neo
checkpoint above was complete before this work began. Working version remains
**1.8.8**; no additional version increment, commit, push, PR, installation or
publication has been performed for P7. The pre-existing untracked research note
was preserved.

The user authorized parallel agents for separate native composition, transport,
parse/session, verification, Workbench and qualification owners. The primary
agent integrates the shared contracts and performs a separate review. At entry,
full P7 acceptance remained pending the native process, complete repository and
frozen performance checks; the observations below begin with development checkpoints.

Implemented contracts are described in [combined reads](../combined-reads.md):
explicit grant-bound Project/index selection, canonical unique-index unions,
fan-out one, deterministic shared budgets, equal-rank contributions, precise
source/Project origins, complete member failures and verification, publication
vectors and full-vector continuations. File follow-ups require one origin.
CLI convenience reads, tool calls/shell, MCP, plans, Workbench and Explorer
consumers retain qualified evidence and full JSON/Markdown results. Applying a
combined Search draft only prefills its captured reviewed selection.

Review found and corrected two native identity/fencing gaps. Current catalog
aliases cannot qualify retained citation paths; an exact-publication protected
source projection now supplies the published mapping without source I/O. A saved
catalog revision alone cannot detect an in-memory pending revoke; the final
native receipt checks the shared admission gate at actual receipt-write polls.
Its scope-admission guarantee does not recall bytes already received by a
frontend. Transport capacity observations precede the finalized body digest,
and subsequent transport rendering preserves that body and receipt together.

Development validation so far:

- Workspace all-target/all-feature check passed, most recently in **15.06 s**.
- Workspace all-target/all-feature Clippy passed with warnings denied, after
  correcting ordinary style diagnostics without lint exemptions.
- The initial focused combined run passed **28 tests** across Core, Kernel and
  MCP, including genuine native handlers through duplex MCP/Unified adapters.
- A broader library run passed **104 Core** and **189 Kernel** cases, with one
  explicitly ignored role measurement. Kernel included the real retained-alias
  regression and its existing large-catalog workload (**397.84 s** total). MCP
  passed **117** and failed **three** discovery integration assertions; the
  unsupported plan response-option injection and metadata-family assertions
  were corrected. The affected final rerun remains required.
- The integrated extension build passed all three TypeScript projects and
  bundling; **667 passed, 19 environment-gated skips, zero failures**. Separate
  source-origin host checks passed **44/44**. Earlier stale catalog-count and
  pure-module inventory failures are retained in the ignored logs.
- Python's full script suite ran **428 cases**, with nine skips and one Git
  ownership-environment error. Its complete affected packaging suite then
  passed **14/14** with a process-local safe-directory setting. No global Git
  configuration or ownership was changed. Focused new benchmark/relevance
  tests passed **25/25** and **12/12**; historical evaluator equivalence
  reproduced **84/84 P6f gates**, which is not P7 performance evidence.
- Repository checks passed for **1,709 public files** at that checkpoint;
  version checks confirmed native/extension/locks agree on **1.8.8**. Final
  repository, formatting and whitespace checks remain required after freeze.

Logs remain under ignored `artifacts/v190/p7-*`. Fresh native-pair and actual
service/CLI/MCP checks are in progress. No P7 timing, installed editor,
macOS/Linux, signing or release success is asserted by the component results.

The next actual-process checkpoint used a fresh, source-bound debug pair
(`p7-runtime-debug-r2`). The selected-subset/revocation, member availability and
**1/2/4/32-index count-one** cases passed. Three other cases failed: the fixture
prepared requests through the test executable's producer identity, parity
comparison retained elapsed allocation/cursor dates, and capabilities exceeded
the canonical body allowance through repeated static catalogs. These are retained
as failed observations, not qualifications. The fixture now uses the actual
staged main for request preparation; timing normalization validates original
delivery/cursor digests before excluding dated observations. Capabilities require
an explicit lossless shared-field representation rather than an increased frame
limit or omitted native data. The corrected actual-process run remains pending.

A separate review found lost Explorer outer continuations and a fresh timeout
per member after an exhausted final source-check deadline. Continuations now
retain exact request inputs, the complete vector and outer cursors up to 32 KiB;
source-check admission uses one aggregate deadline and explicit unqueried
outcomes without native submission after expiry. These corrections still require
the final native regression run. The complete extension suite subsequently
passed **671 tests**, with **19 environment-gated skips** and zero failures
(`p7-vscode-r3.log`); further capabilities consumer changes invalidate the
affected extension checks. Python's complete suite passed **428 tests**, with
**nine skips**, in **22.962 s** (`p7-python-all-r2.log`) using only a process-local
Git safe-directory setting.

After the shared-capabilities and final-receipt consumer corrections, the
integrated extension suite passed **679 tests**, with **19 environment-gated
skips** and zero failures, including compilation of all three TypeScript
projects and bundling (`p7-vscode-r4.log`). The affected packaging suite passed
**21/21**. Workspace all-target/all-feature Clippy passed with warnings denied
in **29.61 s**; its preceding attempt retained one ordinary test-only
`useless_vec` diagnostic, corrected without an exemption. The focused combined
native-library gate passed **59/59** (**17 CLI, 15 Core, 21 Kernel, six MCP**),
including the 32-member exhausted-verification fixture and shared-field
materialization checks. The canonical parser regenerated the CLI reference and
both documentation tests passed. Repository checks passed for **1,713 public
files**, and native/extension/lock versions still agree on **1.8.8**.

The live ignore inventory was regenerated: **57 literal Rust ignores**, split
into **11 helper entry points** and **46 explicit qualification cases**, with
no missing reasons and the same four routine nextest exclusions. These counts
are inventory, not passing qualification. A fresh release pair is being built
before the actual-service and performance runs; no timing pass is inferred from
the focused gates above.

The fresh Windows x64 **release** pair (`p7-runtime-release-r3`) staged
successfully after separate optimized helper (**4m 21s**) and lightweight-main
(**7m 04s**) builds, model verification and notices for **366** dependencies.
Its build receipt binds **906 native source inputs**:

- Source: `079c175deb66cfff56cd437321194f91e0f117a330b2516dfbd6e79a4ac58573`.
- Main: `52088503389829db71fc232abd1f98081efff9d71e03770d819d5b9374a3211a`.
- Helper: `59e549e6e38f50fe3d81b5dacfa697a3b9fce5f90d00da90bab0a544a6052128`.
- Install manifest: `58d768515d7266095a2732a4c8b8a69e26229787e46d1dc6c856d5204e676bc0`.

Against that exact pair, all **seven actual-process combined tests passed** in
**96.90 s**, with the opt-in measurement holder separately ignored
(`p7-native-combined-r3.log`). These cover full native/CLI/MCP read-family
parity, exact capability materialization against canonical scoped handlers,
three paged discovery views, shared-index/source attribution, explicit subsets,
stale/revoked grants, independent member availability, 1/2/4/32-index count-one
budgets, and real helper outage followed by a fresh selected source publication.
The fixture verifies complete authority-format DDL and scoped delivery; it is
not a new Genesis or installed-editor qualification. The legacy native
client/model smoke also passed in **24.404 s**, with original source bytes
preserved (`p7-service-clients-r3.json`). Workspace build passed in **49.26 s**.

The broader nextest CI-profile suite is in progress. It reuses the separately
passed combined integration binary and retains the checked-in four long-case
exclusions. The existing large-catalog Kernel measurement had already passed
in the earlier P7 library checkpoint. Performance runs remain queued until
compilation and tests finish. Current P7 Neo qualification is unavailable in
this sandbox: the historical `neo` target does not resolve and its SSH
configuration is absent; the existing endpoint was requested without changing
credentials, accounts or remote state. Earlier Neo results are not P7 evidence.

The broad Windows CI-profile nextest run finished in **1,189.236 s**:
**2,405 passed, four failed, 50 skipped** (`p7-workspace-nextest-r1.log`).
All **122 MCP library tests** passed. The four failures were CLI catalog
assertions still expecting 18 native/28 public tools. After auditing related
callers, the test-only correction retained exact **19/29** counts, uniqueness,
read-only checks and the closed scope-discovery schema. All four corrected
cases passed in **17.424 s**. The other broad suites were not repeated.
Workspace rustdoc compiled all eight targets successfully; they contain zero
runnable doctest examples. Formatting and all-target/all-feature Clippy passed
again in **5.85 s**.

CI fixture review found that the new helper case needed an owned Cargo-image
pair when no external staged candidate is supplied. The fixture now creates
that exact main/helper digest receipt only in its disposable owner. With the
candidate environment genuinely unset, **six default-feature cases passed** in
**106.70 s**, and the all-feature helper case passed in **29.91 s**. The existing
Windows/macOS focused CI command now includes this one integration target in
its existing one-slot group; no job, matrix or broad suite was added. Python's
updated complete suite passed **433 cases**, with nine skips, in **26.286 s**.
The extension's next complete checkpoint passed **681 tests**, with 19 skips;
a later narrow valid-step-ID cache fix passed its **53 affected tests**.

The refreshed `p7-runtime-release-r4` build reused unchanged optimized binaries
(**5.87 s** helper / **0.82 s** main), verified the payload and recorded the
corrected test sources. Its native input digest is
`f487cd3fd5c1b3b8af01c4430792e3de6bfa8a8338d02ec63e3045e2d6eaf74f`
(906 inputs), and its install manifest is
`94af1805042c4820c75f767a5bc548f2b325c8e29a3107e1d07ef37d5a8d1cef`.
Both executable hashes are byte-identical to r3; the only changed payload files
are `airk-build.json` and `airk-install.json`. The comparison is retained in
`p7-r3-r4-runtime-equivalence.json`; no receipt or producer identity was edited.

On that exact r4 pair, all **84/84 unchanged latency gates passed**
(`p7-gates.json`) against the frozen, archived P5, fresh contemporary P5 and
immediately preceding corrected P6f reports. Each candidate/control Off or
Report replay completed its 90 trials/810 calls with source preservation and
owned shutdown; the evaluator checks the original error/retry and verification
receipt requirements. A separate real predecessor-upgrade fixture created a
new reviewed one-index grant through native draft/apply, preserving its original
imported snapshot. Its **30 paired one-index reads** returned matching complete
hits/publications with zero errors/retries. Observed median/p95 were
**106.20/139.50 ms combined** and **32.22/33.80 ms legacy**. These measurements
show the additional routing cost; they do not claim equal speed.

The first combined fan-out campaign passed **30 samples each at 1, 2 and 4
indexes**, then failed the first 32-index/32-result sample with the explicit
`combined_reply_budget` refusal after **8,351.05 ms**. This failure remains in
`p7-combined-release-r1`; it is not a passing 32-index measurement. A separate
one-sample diagnostic retained full replies and confirmed that repeated header
structure and publication envelopes exceed the canonical 64 KiB allowance.
Lossless header templates, typed member values and exact publication references
are being implemented, leaving evidence rows inline and limits unchanged.
The new production bytes require a fresh matching pair and affected native,
consumer and performance validation before P7 acceptance.

### P7 checkpoint — 4 November 2026, lossless metadata and Windows R6

The combined response now retains native metadata through bounded, typed header
templates and exact references into its publication vector. Evidence rows and
their original result pointers stay inline. Consumers materialize a logical
copy after validating the template namespace, digest, slots, member identity
and publication; raw delivery bytes and saved verification receipt digests are
preserved. Malformed templates refuse explicitly. Whole-file `file_info`
verification compares the logical member copy at the original returned pointer,
including final Report and Require response checks. Rust, Workbench and Python
consumer regressions cover these encoded forms. The Python reader matches the
Core/host golden header digest and validates every actual returned member.

The earlier reply-budget failure above remains part of the record. The separate
R5 diagnostic fitted its response but returned only 29 successful hits: the last
three members reported `scoped_connection_unavailable`, with no query submitted.
Its 32-member positive oracle correctly failed; offline replay confirmed that
all headers and origins decoded and the raw reply stayed unchanged. It is not
a passing 32-result measurement.

The final Windows x64 release pair is `p7-runtime-release-r6`, version **1.8.8**.
Its build receipt binds **908 native inputs**:

- Source: `b09693c5ddb2671f6ac4cab5e0983ee6899df82be79eee4173639ea191673832`.
- Main: `3b997978760662db7de0d3d897deff8b48946bf32b0fd29a978782a87f61c54a`.
- Helper: `e8bfd3defea5492c1fa14080cb265a501610946884ae80ae0b21ce086819278e`.
- Install manifest: `f8a29957c0052b46ed45268e3581f3c58438b4500197067730aefcfdfa73d7aa`.

Against that exact pair, all **seven actual-process combined cases passed** in
**98.40 s**, with the opt-in benchmark holder separately ignored
(`p7-native-combined-r6.log`). The completed fan-out campaign then passed all
**120 samples**, 30 each at 1, 2, 4 and 32 indexes, retaining complete raw replies
and checking the predefined positive evidence oracle without relaxing count or
byte limits (`p7-combined-release-final/measurement/report.json`). Sources were
unchanged and each frontend exited through graceful EOF. Observed latency was:

| Selected indexes | Samples | Median (ms) | p95 (ms) |
| --- | ---: | ---: | ---: |
| 1 | 30 | 136.69 | 210.62 |
| 2 | 30 | 221.19 | 253.78 |
| 4 | 30 | 393.95 | 431.95 |
| 32 | 30 | 2,816.64 | 3,007.36 |

These are absolute local observations with service and OS caches retained.
They do not establish multi-index legacy equivalence or measure process RSS,
CPU, physical I/O or unselected-work counters.

The broad Windows checkpoint remains **2,405 passed, four failed, 50 skipped**;
the four tool-count assertion corrections separately passed **4/4**, as recorded
above. It is not relabeled as a fresh complete broad run. The latest focused
combined library checks passed **66 tests**, verification libraries passed
**36**, and process ownership passed **11**. Formatting and all-target/all-feature
Clippy passed, with the latest Clippy run finishing in **11.37 s**. Python's
complete checkpoint passed **439 tests with nine skips** in **25.615 s**; the
subsequent header-reader and benchmark/retrieval selection passed **49 focused
tests** in **1.541 s**. The extension's complete R6 run passed **693 tests with
19 skips** in **6.868 s** (`p7-vscode-r6.log`).

Neo is reachable again, but its first P7 actual-process run failed **all seven
cases** after the Unix test helper's `SIGCHLD` handling interfered with native
child ownership. The fixture correction and rerun are in progress. No passing
P7 macOS actual-process or release/performance qualification is claimed.

The final R6 legacy Off and Report replays both passed: each retained **90
trials/810 calls**, zero errors/retries, and respectively zero/**720** source
verification receipts. All **84/84 unchanged latency gates passed**
(`p7-final-gates.json`), using the frozen baseline, archived and contemporary P5
controls and the corrected preceding P6f reports. No threshold changed.

The fresh R6 predecessor-upgrade fixture passed **30 paired one-index reads**
with matching complete native hit records and exact member publications, zero
errors/retries and unchanged sources (`p7-one-index-routing-final`). Combined
median/p95 were **103.23/136.72 ms**, versus **32.73/34.37 ms** for legacy.
The complete raw outcomes retain different authority, scope and transport
observations. Calls use a fixed combined-then-legacy order with shared, unflushed
caches; this can favor the second route. The comparison does not assert equal
authority, full response-body equality or equal speed. Outstanding platform
checks still prevent this checkpoint from marking P7 complete.

The final complete Python rerun ran **440 tests**, with **nine skips** and zero
failures in **22.278 s** (`p7-python-final.log`). Repository checks passed for
**1,719 public files** using a process-local Git safe-directory setting; the
first attempt without that setting stopped at Git's ownership check before
enumerating files. The final R6 native client/model smoke passed in **24.314 s**
with source bytes preserved (`p7-service-clients-final.log`).

The frozen original graded corpus rerun passed (`p7-relevance-final-r2.json`):
three trials of all nine cases, with every lexical/vector/hybrid development and
held-out nDCG@10, Recall@10 and negative-empty observation exactly matching the
P5 1.8.8 control. Both compact/rich plan probes completed. This remains the
small original corpus, not broad relevance qualification. The first attempt
completed its queries but correctly failed its checkout identity fence while
the Unix regression test was being edited; its receipt is retained separately.

The Unix-only test runner correction and regressions changed the native input
inventory to **`3a9079a4f2c2c0b09fbed0e358e4bb970a84e6a87aecb43137a82a17be42798c`**
(908 inputs). The canonical final Windows release stage completed in **8.517 s**
(`p7-runtime-release-final`). Its complete payload comparison with R6 found only
`airk-build.json` and `airk-install.json` changed; main, helper and every other
payload byte are identical (`p7-final-runtime-equivalence.json`). Receipts were
generated by the builder, not edited. Its install-manifest SHA-256 is
`c473c14e6807d7ab0e76b40f6cd9e7fe92039b40945975d2c4652c4afda41828`.
Final formatting and workspace all-target/all-feature Clippy passed. The final
staged-pair Windows actual-process rerun passed **7/7** in **97.63 s**
(`p7-native-combined-final.log`); the opt-in measurement holder was ignored in
this functional run and was exercised separately in the 120-trial campaign.
The corrected Neo actual-process run completed with **five passed, two failed
and one ignored** in **116.67 s**. Both failures involve a shared-index native
connection refusal; other fixtures successfully exercise that same scope, so a
deterministic union defect is not established. The six targeted Unix CLI helper
regressions passed, and the earlier **97** scoped native library cases remain
separate evidence. No complete P7 Mac qualification is claimed.

Review found that the positive read-family parity matrix could compare matching
member failures under Report. It now requires every selected member to complete
successfully, preserving complete failed replies and continuing the other read
families. Debug-only, explicitly opted-in diagnostics expose bounded internal
connect/install/renew/grant causes without changing public errors or admission.
The focused diagnostic rerun is pending. Its first observer attempt stopped at
the build script because the wrapper omitted the bundled-model environment;
zero test cases ran, and that harness failure is retained.

The focused diagnostic recovered **`project capacity busy`**, before worker
scope installation or query submission. Neo's observed hardware is **8 GiB RAM
and six logical CPUs**; the unchanged Balanced policy derives a 2 GiB memory
budget and one resident worker there. The positive three-index fixture assumed
two or three residents. Its correction uses native `ConfigureResources` with an
explicit 4 GiB disposable-fixture budget and checks accepted resident capacity;
production defaults, physical-memory clamping and the 32-index measurement
fixture remain unchanged. Temporary debug instrumentation was removed after
collecting the cause.

The strengthened Windows matrix finished **six passed, one failed, one ignored**
in **99.00 s**. The sole failure was the expected canonical refusal of status and
diagnostics under partial source scopes, now incorrectly asserted as positive.
The correction exercises those positive reads through the whole-index C grant
and retains partial-scope refusals as separate negative checks. Evidence reads
still require complete successful members. Corrected platform runs remain pending.

The reviewed final fixture source is
**`823653aa3db28d20099a38c4de9ce94b0873f72d2fb5f766b78241ff2b57eb04`**
(908 native inputs), identical on Windows and Neo. Separate review confirmed
that the explicit fixture budget retains native physical-memory clamping and
pressure/admission checks, requires accepted three-reader capacity, and leaves
the 32-index fixture unchanged. Exact nonempty member sets and successful
outcomes are now checked across every positive native/CLI/MCP family; the
whole-index observation refusals remain separate negative parity cases.

On Neo, the final **debug** pair passed **all seven actual-process tests** in
**110.94 s**, with the opt-in measurement holder ignored. Formatting,
all-target/all-feature Clippy and canonical staging passed on the exact source.
The earlier six Unix ownership regressions and 97 scoped native library cases
remain separately identified evidence. This qualifies the exercised Apple
Silicon native correctness paths; no Mac release latency, installed editor,
Developer ID signing/notarization, Intel or Linux qualification follows.
The final source-bound Windows release stage and corrected-fixture rerun are
still in progress.

Neo's sealed final debug payload (`runtime-qualified-final`) records main
`2a22fdf95044cf60ad8a9f29346f5c7c1ed2876017af2c7ada9e58bd564375a6`,
helper `e19caa405c784fe9b0caf42c1e339bdfa0a50f3224d431974adc22cec6105e3f`
and install manifest
`dd3349746db112d8ccbdfe24310bc423066feca2b376456b8bdb3b2e80509f95`.
All **678 sealed payload files** verified; both executable signatures are
development ad-hoc signatures with no Team Identifier. The **1,718-file public
snapshot** was unchanged before/after the final run and no owned runtime
processes remained. Final receipts are retained in
`p7-neo-native-r5/qualified-final-summary.json` and the `r10-*` logs. The existing
Neo connection is stored in ignored `artifacts/v190/neo-connection.local.json`,
with no key material or committed connection configuration.

### P7 final local qualification — 4 November 2026

The final Windows x64 release stage (`p7-runtime-release-r8`) binds the same
**823653aa… / 908-input** native source as Neo. Its optimized helper/main builds
finished in **3m 17s / 6m 42s**, with pinned model/runtime verification and notices
for **366 dependencies**. Final identities are:

- Main: `58d7b8c83a2a664e221713e02a57429155b8c87fe21976267e6a0a4e984622d6`.
- Helper: `ac09ac7741223d3e508200f09962b71c25636247f6f3885f81ccbba0a89682ff`.
- Install manifest: `248783608ab355f0618aede02dd067d36490cdcb9e2ce1bf47447ac124fe4425`.

Rebuilding produced different executable hashes; the comparison was retained in
`p7-r6-r8-runtime-equivalence.json`, and earlier measurements were **not** relabeled
as final-binary evidence. Against R8, the corrected actual-process suite passed
**7/7** in **92.80 s**, with its opt-in measurement holder separately ignored
(`p7-native-combined-r8.log`). This includes strict successful-member parity,
whole-index status/diagnostics and explicit partial-scope refusals. Final
formatting and all-target/all-feature Clippy passed; workspace rustdoc compiled
all eight targets with zero runnable doctest examples. The broader Windows test
checkpoint and separately corrected catalog cases remain recorded above rather
than being claimed as a fresh broad run.

With compilation and competing tests stopped, both final R8 Off/Report replays
completed **90 trials / 810 calls each**, zero errors/retries and respectively
zero/**720** source-verification receipts. All **84/84 unchanged latency gates
passed** (`p7-r8-gates.json`) against the frozen baseline, archived/contemporary
P5 and corrected preceding P6f controls. Limits and workloads were unchanged.

The final R8 fan-out campaign passed **120/120 trials** in **160.46 s**, with all
required member results and source preservation (`p7-combined-release-r8`).
Each case retains 30 samples and full raw outcomes:

| Selected indexes | Median ms | p95 ms |
| --- | ---: | ---: |
| 1 | 124.26 | 185.10 |
| 2 | 210.29 | 245.38 |
| 4 | 368.14 | 424.02 |
| 32 | 2,692.86 | 2,945.58 |

These are local absolute observations with verification Off and unflushed caches;
they do not establish multi-index legacy equivalence or measure RSS, CPU or
physical I/O. The reviewed 32-index fixture and original limits were unchanged.

The separate R8 predecessor-upgrade fixture passed **30 paired one-index reads**,
matching complete hit records and exact member publications with zero errors or
retries (`p7-one-index-routing-r8`). Combined median/p95 were **96.64/130.85 ms**,
versus **29.79/32.51 ms** for legacy. The previously stated different-authority,
fixed combined-then-legacy order and shared-cache qualifications still apply;
equal response bodies or equal speed are not claimed.

The final R8 frozen original corpus passed all nine cases over three trials,
with every measured lexical/vector/hybrid development/held-out quality score
exactly matching P5 (`p7-relevance-r8.json`). The final native client/model smoke
passed in **24.157 s**, preserving source bytes (`p7-service-clients-r8.log`).
The separately recorded extension **693 passed / 19 skipped** and Python
**440 run / nine skipped / zero failures** checks cover the unchanged consumer
sources. Independent review covered the integrated production diff and final
fixture corrections, including previously untracked implementation files.

P7 is complete at this locally qualified checkpoint. The native/extension/lock
version remains **1.8.8**, unpublished. No P7 installed-editor, Linux, Intel,
Mac release-performance or release-signing/notarization qualification was run.
No commit, push, PR, installation or publication was performed for P7; historical
failures and explicit skips remain visible above.

Final documentation/repository checks passed for **1,719 public files**; native,
extension and lock versions agree on **1.8.8**. The final Python suite ran
**440 tests**, with **nine skips**, zero failures and **21.382 s** elapsed.
Whitespace checks passed and `AGENTS.md`/`CLAUDE.md` remain byte-identical. The
local Neo connection artifact remains excluded by the existing `artifacts/`
ignore rule. A final independent documentation review corrected an inline-only
wording ambiguity; capability-field groups and ordinary native-header templates
are now distinguished explicitly.

## P8 — Recommendations and harness guidance

Implemented on the unchanged **1.8.8** working version. The canonical native
`query_scope` Recommendations view projects only authorized metadata; CLI/MCP
aliases and Workbench plans/forms share its closed schema and complete result.
Core owns selection narrowing and the deterministic decision table. MCP owns
recipe generation from the catalog actually served by that binary; CLI retains
current catalog/grant/delivery fences. The extension renders the result and
revalidates Copy/Use, prefills an explicit temporary form, or opens the existing
separate native defaults review. Split/removal remain explanatory pending P10/P9.
No source scan, evidence query, new watcher or model load is added. Unknown dated
coverage/cost/overlap observations remain unknown rather than using whole-index
counters to claim selected-prefix coverage.

### Contract and validation evidence

The eight decision situations have deterministic tests. Narrowing recomputes the
selected Projects' own disjoint prefixes; it never filters an already broadened
union. A valid selection may retain a Project contributing no selected index,
matching `QuerySelection::resolve`. The failing regression (`unknown advice
selection`) is retained in `p8-unused-project-before.log`; the corrected Core and
actual native/CLI/MCP/Workbench tests pass. Multi-Project scopes use each Project's
existing defaults form rather than proposing another Project's indexes as its
defaults. Temporary scope application preserves existing query/filter/budget and
availability inputs and starts no native client.

Actual transport tests compare complete advisory results through native, CLI,
MCP, explicit Workbench plans and slash commands, after authenticating delivery
receipts. Only documented transport identities/observation timing are normalized.
Copied examples execute through actual MCP; strict member availability survives
copying. The metadata-only phase acquires no project worker lease. Native and
advisory selection observations also match for a useful positive lexical fixture.
The two-index scope reads two unique members and returns three evidence records;
the deliberate one-index scope reads one member and returns one useful record,
with its omitted index explicit. This is a small original fixture, not a general
recall, latency or physical-I/O improvement claim. `p8-subset-observations.json`
retains the observed bytes and selection/policy qualifiers.

Windows validation includes the all-feature workspace checkpoint: **2,433 passed,
47 ignored, zero failed**, including all eight actual combined-read cases. The
initial ambient-temp attempt stopped at two owner-only catalog fixtures; all
four runnable catalog cases then passed using a newly created, isolated,
owner-controlled `AIRK_PRIVATE_CENSUS_BASE`, followed by that full workspace run.
No existing directory permissions or assertions were weakened. The final slash
and unused-Project corrections receive separate focused/native regression runs;
the earlier broad checkpoint is not relabeled as a fresh post-correction run.
All-target/all-feature Clippy and formatting pass. Rustdoc completed all eight
crates with zero runnable examples.

Extension checks pass **698 / 19 skipped** (717 total), including actual host
scope-prefill persistence, complete JSON/Markdown export, late owner/grant fences,
explicit Copy/Use and dismissal. The Python repository contract suite passes
**440 run / nine skipped**. Its initial Git-ownership error was corrected with a
per-process safe-directory setting; no global Git setting or identity changed.
Repository/whitespace and synchronized **1.8.8** version checks pass. Notices for
**366** dependency packages pass. Offline dependency source review, Cargo Deny
and Cargo Vet pass; Cargo Audit is **unavailable** because its cached advisory
checkout is missing, npm advisories were **not run**, and fresh audit qualification
is not claimed.

The saved `neo-connection.local.json` was used only for the owned Neo test
workspace and remains ignored. The first system-Python extraction preflight
failed before tests; the existing Python 3.13 was then selected, preserving that
failure. Final three-file overlays are hash checked against preceding receipts.
Neo passes **21 Core combined-read cases**, the complete **seven combined
transport cases / one opt-in benchmark ignored**, and all-target/all-feature
Clippy. Earlier focused Workbench/MCP cases and slash-command tests are retained.
Its final native source identity is
`ddfb66065c9c7b7d4e20790f7f5f66fa63ecfac57fcc0e811a9ad508fff5e807`
(**910 declared inputs**). Source bytes remain unchanged apart from the explicit
verified overlays. No installed Mac editor, full Mac workspace, Mac performance,
Developer ID/notarization or Linux execution qualification is implied.

An explicit primary-agent review covered the complete production/test diff,
including new files, input families, stale actions, source isolation and the final
corrections. This was **not an independent review**. No commit, push, installed
normal-runtime replacement or publication was performed. Working version 1.8.8
is retained; the already requested version change was not repeated.

### Final candidate and frozen gates

The final Windows x64 release candidate (`p8-runtime-release-r3`) binds the same
**ddfb6606… / 910-input** source as the final Neo receipt. Its independently
verified identities are:

- Main: `d7959fe0fe35cbfc4e10c8a1a6387560a236b19e4b6b117cb92b54ec3faa6169`.
- Helper: `9940aa8a8465b5ea8c5875a60f143d295f7056c1d4bf950a33086e44656108b3`.
- Install manifest: `db6ba8c00823c54a7ae778e7b857ac7389c438fa75da4248e76452fa2e94ede7`.

The complete actual combined transport suite passes **8/8**, with its opt-in
measurement holder separately ignored (`p8-release-r3-combined.log`). This includes
canonical advisory/CLI/MCP/Workbench/slash equality, the valid unused-Project
subset, unchanged read families, exact follow-up origin and failure preservation.
The model-enabled 16-file native-client smoke passes in **27.940 s**, preserving
source bytes (`p8-service-clients-r3.log`). Earlier P8 candidates and pre-correction
receipts remain tied to their own source identities.

With builds and competing suites finished, the final Off and Report replays each
complete **90 trials / 810 calls**, with **zero errors/retries** and respectively
zero/**720** verified source receipts. All **84/84 unchanged latency gates pass**
(`p8-gates.json`): frozen 1.8.7, archived P5, the recorded P5 control and preceding
P7 R8 references. The recorded control is an archived observation, not a newly
run contemporary P8 control. Bounds remain median +max(10%,25 ms) and p95
+max(15%,50 ms); fixtures, sample counts, connection cohorts and query budgets
are unchanged. No claim of universal lower latency follows from this result.

The frozen original nine-case corpus passes three trials in each supported mode
(`p8-relevance.json`). Every measured development/held-out lexical/vector/hybrid
grade metric exactly matches both P5 and P7, across **84 metric/reference checks**
(`p8-quality-comparison.json`). Labels and model inputs remain unchanged. This is
a small regression corpus; the separate purposeful subset fixture records
omissions and work counts without asserting equal recall for a narrower scope.

P8 is complete at this local checkpoint. Installed-editor interaction and manual
acceptance, Linux execution, full Mac workspace/performance and final signed
release qualification remain outside these observations. Fresh dependency
advisory qualification is unavailable as recorded above. P9/P10 management actions
remain pending, and 1.9.0 promotion/publication remains a later reviewed handoff.

## P9 — permanent source exclusions (in progress)

### Resumed checkpoint — 5 November 2026

P9 remains **unaccepted** on working version **1.8.8**. P10–P14 have not started.
The resumed work preserves the existing native admission, read fences, removal,
durable policy capsule, cleanup receipts and extension consumers. The
[usage contract](../source-policy.md) describes implemented candidate behavior
and its explicit limitations; it is not a release-qualification claim.

The checkpoint's two actual native/CLI/MCP trim tests pass after the remaining
vector comparison authenticates and unwraps the complete transport bodies.
The four direct Kernel policy tests also pass after the retained graph fixture
asserts omission of an unresolved endpoint and survival of a proven allowed edge.
These runs precede the following naming corrections and are not final-source
qualification (`p9-resume-transports-r4.log`, `p9-resume-kernel-policy-r2.log`).

The naming audit replaced general Windows/macOS Unicode string comparison with
exact equality, qualified ASCII case equivalence and explicit unsupported
outcomes. Metadata-only native probes reject unsupported case modes/filesystems.
An unsupported comparison propagates through the boolean discovery filter and
watch planner rather than producing a successful partial walk. Denied ignore
control inputs that the current ignore adapter cannot omit cause traversal and
saved-source checks to refuse before reading those controls. Pure predicates and
direct Kernel regressions cover disjoint Unicode names, failed removal/new
capture, retained evidence and source preservation. Early Windows checks pass
three predicate cases and five direct Kernel cases; the final added ignore-control
and watcher regressions await the full source-bound campaigns below.

The first Windows source campaign passes formatting, build, extension tests
(**714 passed / 19 skipped**), Python tests (**440 run / nine skipped**), repository,
366-package notices, version and whitespace checks. It exposes a Store Clippy
lint and an obsolete runtime-activation test which still rejected policy contract
1. The corrected test accepts the implemented contract, explicitly rejects a
same-version policy-0 runtime and continues rejecting an unknown policy contract.
A diagnostic workspace Clippy pass identifies only the subsequently corrected
placement of the policy test module.

The next Windows campaign passes **485 CLI library cases / 21 ignored**, then
exposes a helper fixture calling native configuration preparation from the test
executable's distinct image. Its retained complete result is
`service_rebind_required`. The fixture now exercises the actual CLI preparation
command, preserving engine identity checks. All four helper/model tests then
pass (`p9-helper-stop-native-cli-r1.log`). These partial broad runs do not qualify
the targets skipped by Cargo after a failure. Raw campaign receipts remain in
`p9-source-20261105-r1/` and `p9-source-20261105-r2/`.

The fresh dependency policy gate **passes**
(`p9-dependency-fresh-20261105.json`), including Cargo Audit, npm advisories, Deny,
Vet, source policy and input integrity. Three findings remain covered by the
existing explicit exceptions: `paste` and `ttf-parser` maintenance advisories,
and the moderate `minimatch` advisory. No policy/exception/lock changes were made.
This supersedes the resumed offline audit's missing advisory checkout for this
new observation; historical unavailable results remain identifiable.

Neo's first public snapshot completes its CLI library run with **485 passed /
23 ignored / one failed**: the same obsolete activation assertion. Its owned
runner's `finally` calls `cargo clean` on the exact isolated target; the cleanup
receipt reports **exit 0**, preserving source and external model/runtime inputs.
The initial launcher used an absent Python path and ran no tests; the retry uses
Neo's existing Python 3.13. Neither event is relabeled as a passing campaign.

The next frozen native source is **`b55893f0…` / 922 declared inputs**.
Windows all-target/all-feature Clippy passes with `-D warnings` on this source.
Windows and Neo full workspace runs collect all target outcomes with
`--no-fail-fast`; Neo again guarantees exact-target Cargo cleanup after the run.
Their final results, fresh runtime/resource/latency campaigns and phase review
remain pending. Bounded LLVM coverage is unavailable because `cargo-llvm-cov`
is not installed. No final P9 acceptance, phase commit, push, PR, normal-runtime
replacement or publication has occurred. This review is a primary-agent review,
not an independent review.

### Corrective qualification — 5 November 2026

The broad campaigns on `b55893f0…` completed rather than stopping at the first
failure. Windows reported **2,484 passed / five failed / 47 ignored**; Neo R2
reported **2,442 passed / six failed / 47 ignored**. These are sums of completed
Rust log summaries, including child-test summaries, not a unique-test census.
Failures covered retained-reader contention, two obsolete FTS rebuild fixtures,
the discovery root's ignore-control setup, unsupported policy-version diagnostics
and (Neo only) native review contention during queued setup. They remain failed
historical runs, even where the following corrections pass.

The ignore walker exempts its root from its entry filter, so checking only the
filter was insufficient: it could load excluded root ignore controls first.
Discovery now qualifies the root before the first iterator step. Watch planning
and verification share checked policy predicates. A missing policy capsule now
also blocks derived reset preparation before obsolete owner metadata is removed.
Known native policy authority permits the intended legacy FTS rebuild; cold
unknown authority continues to refuse it without changing its owner bytes.

Focused Windows corrections pass: four policy-runtime unit cases, six direct
Kernel policy cases, six FTS cases, seven Store catalog cases, ten watcher cases,
three native naming-boundary cases, three ingest policy predicates, four scheduler
join cases and the read-only contention regression. Read-only management lock
contention uses the existing retryable `registry_busy`; mutating admissions and
permission failures retain refusal. The trim fixture waits for accepted setup
without canceling or replaying it. Neo R3 passes policy-runtime, direct Kernel,
FTS and watcher checks before exposing the catalog diagnostic mismatch; Neo R4
then passes catalog, read-only contention, both actual CLI/MCP trim cases and
workspace/all-target/all-feature Clippy with `-D warnings`. Both runs complete
their exact-target `cargo clean` with exit 0.

A stronger positive Unblock regression subsequently exposes a cold worker startup
failure masked by a negative-only evidence check: the source-completion selector
was iterating the retention union of source and policy jobs. A succeeded policy
job has no source-change begin receipt. Source-completion recovery now selects
only exact source lineage; generic recovery and pruning retain the union. Current
policy jobs remain pinned after physical cleanup because policy status still
requires their receipts. Superseded jobs retain normal history/pruning semantics.
Store regressions cover both successful-policy recovery and completed-receipt
retention; actual native CLI/MCP tests cover trim, reviewed Unblock, source reindex
and positive evidence delivery after cold restart. All **21 selected Store policy
cases** and **two actual CLI/MCP cases** pass on native source `7a673d55…` / 922
inputs (`p9-policy-recovery-store-r1.log`, `p9-policy-recovery-native-r1.log`).
Temporary production diagnostics used to identify the worker error were removed.

The formatted successor is **`689e2f5e…` / 922 native inputs**. Neo R5 is a new
immutable public-source snapshot with focused recovery/transport/Clippy checks
and unconditional exact-target Cargo cleanup. All six steps pass: formatting,
seven Store catalog cases, 21 selected Store policy cases, the read-only contention
case, both native CLI/MCP cases and all-target/all-feature workspace Clippy with
`-D warnings`. Source verification before/after remains unchanged; Cargo cleanup
reports **exit 0** (`p9-neo-r5-qualification.json`, `p9-neo-r5-cleanup.json`). This
is focused Apple Silicon qualification, not a fresh complete Mac workspace run.
The new Windows release build and workspace rerun remain pending.

An earlier Windows release pair on `b55893f0…` passes model-backed native clients
and **15 four-core local CI cycles** (five repetitions each of smoke, clients and
installation). Its immutable receipt is retained under
`p9-runtime-20261105-r1/`, with local-CI results in
`artifacts/ci-local/20261105-023041-d4bccc3c/summary.json`. These binaries predate
the corrections above and do not qualify the successor source. Final frozen
latency/relevance, fault/upgrade evidence and complete-diff review remain open.
P9 is still unaccepted; P10–P14 remain unstarted. No commit, push, PR, ordinary
runtime replacement, publication or version promotion has occurred.

### Retirement review finding — 5 November 2026

The corrected-source Windows release build completes in 707 seconds with native
inputs unchanged (`p9-final-runtime-build-r2.receipt.json`). Its full workspace
run is still collecting outcomes. Runtime benchmarks were queued behind these
prerequisites, then held before execution when primary source review found an
uncovered retirement path. No latency/relevance result is inferred from the build.

The collection purge allowlist predates `source-policy.json` and refuses that
file as unknown. Simply adding it to the deletion list would erase a durable
enforcement copy. Registry removal also deletes all job receipts, although policy
status requires its exact current Configure job. Finally, retirement can follow a
failed admitted trim before any capsule was written. Cleanup must preserve the
admitted deny in that case as well. Fresh policy-cleanup resumes on a tombstoned
index also require explicit refusal. These linked lifecycle paths are a P9
blocker; correction and native regression evidence remain pending. The queued
measurements on this superseded candidate are held while the existing workspace
run collects its remaining results. Historical passing Neo checks remain valid
only for the paths they exercised, and its required Cargo cleanup is complete.

The Windows workspace run then completes successfully on `689e2f5e…`: **2,489
reported passed / zero failed / 47 ignored**, across 78 completed summaries
(`p9-workspace-final-r2-summary.json`). As above, sums include child summaries;
they are not a unique-test census. The subsequent correction is separately bound
and does not relabel that run as coverage of newly added paths.

The new purge regression reproduces the review finding before the correction:
`unknown removal entry is preserved: source-policy.json`
(`p9-retirement-store-red-r1.log`). Store now validates and preserves the capsule
and stable owner while completing bounded incarnation cleanup. Native retirement
materializes missing/older enforcement copies from admitted policy as pending,
under quota, and preserves the current policy job and its inactive registration.
It clears the removal-polling guard after completion. Fresh policy resumes on a
tombstone refuse; exact already-accepted retry observation retains its existing
semantics. The extension hides the retired-index resume action and its controller
also refuses it. Normal removals without policy retain their original deletion
behavior.

The same review closes ADR 002's stable-owner requirement: its required policy
reference is separately authenticated and excluded from the original ownership
digest. Old owners reject the field. Kernel checks its required digest even if
the retained publication is absent; a missing capsule blocks indexing and reset
preparation rather than becoming an empty policy. Store checks same-epoch digest
reuse, epoch regression, unchanged immutable ownership, corrupt/foreign capsule
refusal and idempotent cleanup.

Focused Windows evidence passes **19 retirement cases**, **23 selected Store
policy cases** and **seven direct Kernel cases**. The expanded actual CLI/MCP
run passes **three cases / one explicitly ignored predecessor qualification**
on `06f43289…`; these include completed-policy retirement and a quota failure
before capsule creation (`p9-retirement-native-r3.log`). An earlier new-test compile
failure used Debug on a Serialize-only response and was corrected to retain its
complete JSON. The first quota injection attempted to revoke an existing retained
allocation and was correctly refused; its corrected bound includes used and
reserved bytes plus 128 KiB for metadata, leaving the 64-MiB publication turn
inadmissible. These earlier failures remain identifiable in the r1/r2 logs.

An additional two-scenario native quota test passes on **`c2039ba6…` / 922 inputs**
(`p9-quota-exact-resume-r1.log`). After the fixture restores its injected capacity
through the owning Store adapter, the actual native resume preserves operation,
policy and failed-job history, returns the same accepted job on exact observation,
publishes removal and delivers positive allowed evidence after explicit grant
renewal. The alternative explicitly retires the failed operation and retains its
pending enforcement copy. This does not qualify every frontend quota-edit flow.
Workspace/all-target/all-feature Clippy with `-D warnings` also passes on this
source (`p9-owner-retirement-clippy-r1.log`).

The original entry-revision Store is built in an isolated public-source archive
with only one declared test-driver module; its production handler files remain
unchanged. Its actual constructor/current-metadata reader succeeds before trim,
then its constructor and native purge refuse specifically the required policy
field after retirement, without changing owner/capsule/source bytes. The explicit
ignored qualification passes on Windows
(`p9-pre-policy-actual-native-r1.log`; helper SHA-256 `14d44336…`, origin
`8c790a53…`). This is a native Store-handler compatibility fixture, not an approved
predecessor service, signed distribution or installed-editor claim. The regenerated
ignore inventory records **58 literal Rust ignores**, including this separately
gated case; unselected qualification remains ignored, not passing.

Neo R6 transfers the frozen `c2039ba6…` source and exact original predecessor
archive for focused lifecycle, quota, actual predecessor-handler and Clippy
checks. Both builds use its exact owned target, and the runner unconditionally
calls Cargo cleanup afterward. R6 results, the matching Windows release pair,
fresh runtime/frozen latency/relevance and the final review remain pending.
P9 remains unaccepted; no phase commit or P10–P14 implementation has occurred.

### Frozen runtime and corrected Neo qualification — 5 November 2026

Neo R6 passes all ten selected steps on `c2039ba6…` / 922 native inputs:
formatting, catalog, Store policy and retirement, Kernel policy, read-only review
contention, actual CLI/MCP trim/Unblock/quota-resume/retirement, original predecessor
Store-handler build/execution and all-target/all-feature Clippy. The ordinary
transport selection reports three passed / one ignored; the predecessor case
is then explicitly selected and passes. Source bytes are verified before/after.
The runner's final Cargo cleanup exits **0**, and a separate SSH check confirms
the entire owned target was removed. Source and external offline model/runtime
inputs remain. Receipts are `p9-neo-r6-qualification.json`,
`p9-neo-r6-cleanup.json` and `p9-neo-r6-pre-policy-*.json`. This is focused
Apple Silicon qualification, not a fresh complete Mac workspace, signed app,
Intel/Linux, editor or Mac performance claim.

The corrected Windows release build completes in 673 seconds with native inputs
unchanged. Its verified complete inventory is `p9-runtime-final-r3-seal.json`:

- Main SHA-256: `84b8bd34183dbc3ea75434ac2ac9cf67879c8722e1038c6cfad3dc8716ebdd6a`.
- Helper SHA-256: `45e9ba2086bb427cee009f267d6606cfb351470267a197356b10af8d08e415e2`.
- Install manifest SHA-256: `d75ea8fdc8964e76c7f72875f9d0dfb83c59750a1ca5885831dcafff60d191b5`.

The serial runtime campaign passes (`p9-final-qualification-r2.json`). It includes
model-backed clients, six publication process-death attempts, unchanged frozen
Off/Report workloads and **84/84 unchanged-bound latency gates**
(`p9-final-gates-r2.json`). The nine-case original graded corpus completes three
trials per supported mode; every measured grade/recall outcome exactly matches P5
and P8 across **84 metric/reference comparisons**
(`p9-final-quality-comparison-r2.json`). The corpus remains a small regression
baseline. No model, labels, ranking or semantic unit was changed. Resource/fixture
conditions and payload hashes are retained in the raw dated receipts.

One fresh four-core local CI cycle passes **3/3** smoke, clients and installation
checks (`artifacts/ci-local/20261105-052839-3fbbaaf4/summary.json`). The older fifteen
cycles remain separately tied to their earlier source. These are isolated runtime
fixtures, not an installer delivered to or activated in the normal installation.
The fault campaign simulates process termination; Windows full-volume tmpfs and
physical power-loss claims are outside its evidence.

The extension rerun passes **714 cases / 19 skipped** on the corrected source.
The direct `npm` wrapper launch initially failed on Windows; using Node's installed
npm entry runs the suite successfully. A subsequent wrapper-only console encoding
error occurred after its successful receipt was saved; the ignored runner now
prints UTF-8. Neither launcher error is recorded as a product-test failure or a
passing qualification step. The actual suite log and receipt remain intact.

A final complete Windows workspace and Python contract rerun on `c2039ba6…` is
now collecting outcomes. Final repository/whitespace/version checks and explicit
complete-diff review remain before P9 acceptance and its signed phase commit.
No P10–P14 implementation, ordinary-runtime replacement, PR, push, publication,
installer delivery or version promotion has occurred.

The Python contract rerun initially reports one error in the source-tree identity
fixture because Git refuses this sandbox token's repository ownership. The same
read-only Git command reproduces that refusal. With the integrated checker's
process-scoped `safe.directory` setting, all **440 tests complete successfully,
with nine skipped** (`p9-owner-python-final-r2.log`); no global Git setting or
assertion changed. The native source identity remains `c2039ba6…` before/after.
The release notice generator also succeeds for **366 third-party packages**
(`p9-final-notices-r1.log`); its initial direct invocation lacked Cargo on PATH,
and the standard local runner supplies the already configured toolchain.
Native/extension/lockfile versions agree at **1.8.8**, contributor instruction
files are byte-identical, and the tracked whitespace check passes. ADR 002 now
names the implemented stable enforcement copy and explicitly refuses ordinary
re-registration of retired backing rather than describing the proposed layout
or unimplemented reactivation as current behavior.

The primary agent's post-implementation review traces the combined P9 admission,
watcher/worker fencing, source-policy and source-change recovery selectors,
retained-read and delivery checks, retirement ownership, compatibility refusal,
and complete CLI/MCP/Workbench observations. Its inventory covers **134 changed
or new public files** (`p9-final-review.json`), including the documentation
reconciliation above. This is an explicit primary review, not independent review.
No further production correction was identified in that pass. Full Windows
workspace completion and final acceptance documentation remain outstanding.

A subsequent status-field audit identifies one remaining correction: the native
policy observation currently hardcodes `blocked: true` even after the last
reviewed Unblock leaves no deny or positive inclusion restriction. Read admission
is already correct, and Explorer's management labels use the actual rules, but
the native observation must describe the effective restriction honestly. Collect
the current workspace result before changing source, then qualify the corrected
native observation through the actual transports. P9 remains unaccepted.

The full Windows workspace run on frozen **`c2039ba6…` / 922 inputs** completes
successfully in **2,169.7 seconds** (`p9-workspace-owner-final-r1.receipt.json`).
Its **78 summaries report 2,493 passed, zero failed and 48 ignored**; subprocess
summaries are included, so this is not a unique-test census. Before/after native
source identities match. This result includes the final stable-owner,
quota-resume and retirement corrections and precedes only the status-field
correction identified above. It does not qualify that later delta by implication.

### Final status and coverage audit — 5 November 2026

The last-Unblock observation regression first fails with the hardcoded true
flag (`p9-empty-policy-observation-red-r1.log`). Its correction retains true
while native application is pending or include/deny restrictions remain, and
reports false for the applied unrestricted policy while preserving its required
format and receipt. Five direct policy cases, three actual native CLI/MCP cases
(one separate predecessor qualification remains ignored) and all-target/all-feature
Clippy pass on **`58282eeb…` / 922 inputs**. Actual complete status bodies agree
after Unblock, including positive false/pending assertions. Neo R7 passes its four
selected steps on the same source, then Cargo cleanup exits zero; a separate SSH
check verifies target removal (`p9-neo-r7-qualification.json`,
`p9-neo-r7-cleanup.json`).

The later inventory-call audit finds a distinct gap: `coverage(true)` still uses
the ordinary inventory adapter without permanent policy, and a pre-trim coverage
cache can return old excluded paths. Managed replacement rebuild calls coverage
validation and therefore shares the affected path. Both new behavioral
regressions fail before the correction (`p9-coverage-red-r2.log`). The earlier r1
is a test compile error from an unqualified digest type, not a behavioral result.

Ingest now passes policy through the existing qualified discovery iterator and
checks native liveness before inventory work, per candidate and at completion.
Unknown naming/ignore-control refusals remain explicit. Kernel binds its private
coverage cache to the exact policy digest, validates its epoch before cache
delivery/persistence, and supplies the service cancellation checkpoint during
refresh. The public coverage result shape remains unchanged; no new evidence
mutation or corpus scan is introduced into ordinary status. Source checks,
retained reads and replacement ownership continue through their existing owners.

On **`c1ac112c…` / 922 inputs**, all **nine direct policy cases** pass, including
pre-policy cache rejection, a 128-byte refresh budget with a 1-MiB excluded source,
actual managed replacement/reopen, unknown-name and excluded-control refusal, and
source-byte preservation (`p9-coverage-green-r2.log`). The first run after the
production correction reports eight passed and one mistaken new assertion: the
Unicode case expected the ignore-control reason. Its corrected assertion checks
the actual Unicode reason, and a separate ignore-control assertion was added in
the correct case. Two obsolete private discovery wrappers were removed after
their callers moved to the qualified iterator. No product assertion was weakened.
The related coverage selection passes **12 cases across its targets**, including
existing multi-root, budget, runtime and cached-coverage cases
(`p9-coverage-related-r1.log`); workspace/all-target/all-feature Clippy also passes
(`p9-coverage-clippy-r1.log`).

Neo R8 and a fresh Windows actual-transport run are collecting results for this
inventory correction. Earlier full-workspace, release, latency and relevance
receipts remain tied to `c2039ba6…`; they are not relabeled as testing these later
deltas. P9 remains unaccepted pending the final scoped results and review. P10–P14
remain unimplemented; no phase commit, push, publication, installer delivery or
version promotion has occurred.

### Protected ignore-control inputs — 5 November 2026

Neo R8 passes its five selected steps on `c1ac112c…`, including the coverage
correction and actual CLI/MCP lifecycle cases. Cargo cleanup exits zero and a
separate SSH check verifies target removal (`p9-neo-r8-*.json`). A further audit
of the locked ignore dependency finds that its control loader opens files
directly. A new Unix native regression deliberately symlinks `.airkignore` to
an excluded file containing `public.md`: the uncorrected code reads that rule
and removes the allowed document while reporting successful indexing. Neo R9
reproduces this failure on `1f864708…` (`p9-neo-r9-linked-control-red.log`). This
is intentional pre-fix evidence, not a passing qualification. Its final Cargo
cleanup also succeeds and target removal is verified.

Ingest now captures controls through its existing protected no-follow source
reader, validates held identity/pathname and passes captured lines to the pattern
compiler. The dependency performs no control-file opens in the restricted path.
Discovery, coverage, watcher planning and saved-source verification share that
adapter. Native input checks still apply to global configuration and its selected
excludes file. Failed verification configuration is cached per root rather than
reread for every source. The ordinary empty-restriction path retains its adapter.
The [source-policy guide](../source-policy.md) records explicit control-byte,
directory, encoding and supported-reference limits, including unqualified
worktree/commondir indirection and ambiguous global excludes values.

Three Ingest regressions pass for ordinary/nested pattern precedence, rejection
of a configured global control inside a denied subtree, and oversized-control
refusal. Ten Windows direct policy cases pass, including a before/after native
index comparison for nested ignores and explicit credential-file whitelists,
plus positive saved-source verification. The first Windows protected-reader run
refuses the sandbox token's inaccessible default global ignore file; an explicit
metadata probe confirms permission denial. The scoped test runner now supplies
an owned empty global ignore configuration, without changing user files, ACLs or
global settings. The earlier failed observation remains in
`p9-protected-controls-kernel-r2.log`; r3/r4 record the qualified fixture results.
The initial compile errors and subsequent Clippy style findings remain identified
in their separate r1 logs; corrections preserve the behavioral assertions.

The latest actual CLI/MCP selection passes three cases / one separately gated
predecessor ignore on **`727fd1b5…` / 923 native inputs**
(`p9-protected-controls-native-r1.log`). It also positively verifies an allowed
source with Report after trim and compares the complete native, CLI and MCP
receipts. Neo R10 is collecting protected-reader, policy, watcher, verification,
coverage, actual transport and Clippy outcomes on that source, using the same
explicit owned configuration profile and unconditional Cargo cleanup. P9 remains
unaccepted; P10–P14 have not started. All earlier full-workspace and frozen runtime
receipts keep their original source identities.

### P9 independent audit closure and combined validation, 5 November 2026

Parallel independent review identified and corrected two ignore-snapshot issues:
global patterns now retain the ordinary adapter's process-CWD base, and a nested
control failure remains latched until a fresh matcher. Deterministic regressions
cover configured/default global controls and repaired-but-still-failed snapshots.

The source-I/O audit replaced split metadata/canonicalization/open sequences for
administrative inputs with authorize-original/authorize-resolved/protected-exact-
path capture. Kernel settings/usage/model and calibration adapters, semantic
inputs and helper installation retain that selected path. Store consumes bounded
asset captures and verifies references rather than reopening those source paths.
These are supported-input protections; no universal filesystem or memory claim
is added. The local full Ingest run on **`8129b98d…` / 924 native inputs** passed
272 tests, failed one and ignored six. The failure was the parser fixture's live
writable Windows `NamedTempFile` handle conflicting with protected capture.
The fixture now closes the handle with `into_temp_path`, retaining automatic path
cleanup and the protected reader's sharing requirements. The failed receipt is
preserved in `p9-combined-audit-ingest-r1`.

The lifecycle audit added a read-only exact cleanup-resume receipt independent
of the current policy operation. Native validation binds request key, caller,
index, operation, configuration and result. Workbench retains complete accepted
proof separately from later current status, so a lost response can reconcile
after a reviewed successor without replaying old work. Missing/expired/pruned
proof remains unknown. Two separate agents reviewed that change without finding
a concrete defect; another independent review found no further capture defect.
These were code reviews, not executed-check claims.

On **`8129b98d…`**, the complete extension suite passes **716 / 19 skipped**;
Windows Kernel's administrative-input and policy integration targets pass
**12 / zero failed**. The later fixture correction has a new source identity;
combined owner tests and workspace Clippy are collecting separate receipts.
Neo R10 completed all nine selected steps successfully and its exact isolated
target was removed by successful `cargo clean`; collected receipts retain its
earlier **`727fd1b5…`** source. R11 is prepared for the final combined audit delta,
with unconditional cleanup, but has not yet launched at this checkpoint.

P9 remains unaccepted. P10 and later implementation has not begun. Parallel
preparation maps and inert proposed patches stay under ignored artifacts; they
do not constitute phase acceptance. Existing broad/runtime evidence remains
bound to its original inputs and is not relabeled as final-source validation.

The corrected candidate is frozen at **`224ee433…` / 924 native inputs**.
Combined Windows owner suites pass: Ingest **273 / six ignored**, Store
**688 / three ignored**, zero failures. Workspace/all-target/all-feature Clippy
passes with `-D warnings`; repository checks pass for 1,738 public files. Python
reports 440 run, **431 passed / nine skipped**. Native/extension/lockfile version
consistency remains 1.8.8. Full workspace tests followed by one sealed release-pair
runtime/performance/relevance sequence are collecting in
`p9-combined-final-queue-r1`; that queue stops on failure and preserves each
source-bound result. It does not dispatch hosted CI. Neo R11 has launched on the
same native fingerprint; its owner wrapper always attempts Cargo cleanup and its
results/target removal remain to be collected. No phase acceptance is claimed
from queued or running checks.

Neo R11 is now collected: **all seven steps passed** on **`224ee433…`**,
including full Ingest, Kernel policy/administrative capture, Store exact-policy
receipt and calibration selections, actual native CLI/MCP permanent-trim cases,
formatting and workspace Clippy. `cargo clean` exited zero, and a separate SSH
existence check confirms the exact isolated target directory was removed.
This is focused macOS ARM64 evidence; it does not claim a full Mac workspace,
fresh runtime/performance, installed editor, signing or additional platforms.

Evidence bookkeeping correction: the current primary review generator retained
the older r2 inventory output name. Its current 158-file inventory was moved to
`p9-final-review-r3.json`; the overwritten historical r2 inventory is unavailable
and its filename now contains an explicit invalidation marker. Do not cite that
marker as review evidence. Source files and all test/runtime receipts were
unaffected; historical test results retain their original fingerprints.


### P9 feature-mode correction and isolated P10 work, 5 November 2026

The full Windows workspace/all-feature suite passed on **`224ee433…` / 924
native inputs**, exit zero in 2,242.5 seconds. Its unchanged-source receipt is
`p9-combined-final-workspace-r1.receipt.json`; no unique-test census is inferred
from nested subprocess summaries. Neo R11 passed all seven steps and cleaned its
target as recorded above.

The fresh release producer built its helper but failed lightweight-main
compilation: `OfflineTokenizer` referenced the remote tokenizer's `open` wrapper,
which the administrative-capture change had made test-only. All-feature tests
compile the other tokenizer implementation and did not expose this branch.
The failed producer/queue remain preserved as r4 / combined-final-queue-r1.
The correction is **exactly three cfg attributes** in `remote_tokenization.rs`:
include the existing import/wrappers when either testing or local inference is
disabled. Function bodies are unchanged. Primary review mechanically compared
with the R11 source archive (`p9-feature-guard-review-r1.json`). All-feature
production/test predicates are unchanged; no full-workspace rerun is justified
by this compile-only feature restoration. Its earlier result retains its source
identity rather than being relabeled.

The corrected source is **`42224990…` / 924 native inputs**. Lightweight CLI
all-target Clippy and workspace/all-target/all-feature Clippy pass with warnings
as errors; focused tokenizer/helper selections pass in both feature modes, and
formatting passes. The corrected r5 release pair and frozen gates are collecting
under `p9-combined-final-queue-r2`; Neo R12 is collecting focused feature-mode
Clippy/tokenizer checks with unconditional cleanup. P9 remains unaccepted until
these results settle. Future phase qualification will check lightweight and
helper feature compilation before long suites or release LTO.

To use the validation wait productively without altering tested root files,
P10 implementation now runs only in the ignored isolated worktree
`artifacts/v190/p10-worktree`, based on the public P9 snapshot plus the exact
feature-guard correction. Core/catalog staging, funded quota and positive-control
input work have separate owners; the primary owns native transport/coordination
and consumers. No P10 code is integrated into this P9 tree, enabled or accepted.
Acceptance remains sequential after signed P9. Prepared post-P14 deleted-source
regressions likewise remain unapplied and unexecuted.


### P9 accepted local phase and signed checkpoint, 5 November 2026

P9 is accepted for this unpublished 1.8.8 development phase. No push, PR,
publication, version promotion or normal user installation is authorized here.
The complete current change received a primary review, with separate parallel
source-I/O, ignore and receipt/lifecycle reviews as recorded above.

Final evidence retains its actual input identity:

- **`224ee433…` / 924 native inputs:** full Windows workspace/all-feature suite
  passes, exit zero in 2,242.5 seconds; combined Ingest/Store suites pass273/688
  with6/3 ignored. Python reports431 passed/nine skipped. The complete extension
  suite passes716 with19 skips on its earlier **`8129b98d…`** source; subsequent
  changes are native-only, and its receipt is not relabeled. Neo R11 passes all
  seven focused steps, including real
  CLI/MCP policy parity and Unix protected-path regressions, and cleans its target.
- **`42224990…` / 924:** exact three-attribute feature correction, both-mode
  Clippy/tokenizer checks and fresh sealed r5 Windows release pair. All12 queue
  stages pass: formatting, both tokenizer modes, producer, clients, six publication
  fault attempts, frozen Off/Report replays, all84 unchanged-bound latency gates,
  original nine-case three-trial/mode relevance, all84 metric/reference
  comparisons, and local four-core smoke/clients/install3/3. Detailed install
  results are `artifacts/ci-local/20261105-094824-b13bcd94/summary.json`.
- **`7163ef05…` / 924:** only the shared-calibration unit fixture changed after
  that runtime campaign: canonical temporary parent plus useful failure output.
  Primary byte comparison verifies the entire production source outside that
  test function is unchanged (`p9-fixture-test-only-review-r1.json`). Both Windows
  feature-mode cases, workspace Clippy and formatting pass. Neo R13 passes fmt
  and both feature-mode cases using its original default TMPDIR environment.
  Successful Cargo cleanup and a separate SSH check confirm target removal.

Neo R12's lightweight CLI Clippy passed, but its tokenizer fixture failed because
macOS exposes default temporary storage through `/var`. The fixture had passed
that unresolved spelling to an exact-path helper reader. The corrected fixture
selects a canonical temporary parent; neither the protected reader nor expected
assertion was weakened. R12's failed receipt remains preserved, with successful
Cargo cleanup/target removal; R13 supplies the corrected focused result.

The r5 runtime remains honestly bound to **`42224990…`**, not relabeled as the
later test-fixture input. Main SHA-256:
`7723276a5767cbacac1e14958cd113c0e5a99e1b8fdb1c3bcbb5b6b84c1fc808`;
helper: `ce589b2aa3b17a43a3affc771e72bf10509618edfa679db82c22d01669139526`;
install manifest: `0872d52aab2a44160906dcc296e82d9bcaaebaf9f5dbb5686f2838953255bf10`.
Its complete inventory is `p9-runtime-final-r5-seal.json`. No production code
changed after those runtime gates. The later final installer will be rebuilt
from its final accepted implementation and version.

This is bounded local qualification, not full Mac workspace/performance,
installed-editor, Linux/Intel, signing/notarization, power-loss/secure-erasure or
published-artifact qualification. The small original relevance corpus establishes
only its measured controls. LLVM coverage is unavailable. Earlier dependency,
notice and actual predecessor-handler evidence retains its recorded inputs and
limits. Fresh post-documentation repository/version/whitespace checks and the
human DCO signed phase commit complete this checkpoint. P10 implementation is
isolated until that commit; P11–P14 and deleted-source follow-up remain unaccepted.


### P10 — isolated candidate and focused Windows evidence, 5 November 2026

**Historical development checkpoint, before the frozen qualification below.**
P10 implementation is isolated in `artifacts/v190/p10-worktree`; the accepted P9
root checkpoint and its source/runtime identities above remain unchanged. Working
version **1.8.8** remains unpublished. **P10 is not accepted.** These observations
are individual checks of a changing isolated tree, not a frozen source/runtime
qualification. No P10 source freeze, Neo run, phase commit, publication or normal
installed-runtime replacement is established by these results.

The [candidate workflow](../index-splitting.md) implements two independent policy
partitions, inherited permanent denies and positive eligibility, a durable Accepted
intent before setup/funding/child effects, sealed destination build, full affected
Project review, read-only original witness and a drained restrictive cutover.
Store owns exact operation allocations and child-writer funding; ordinary opens
receive neither permit nor funding from a backing path. CLI owns metadata progress
and worker/fence dispatch; Kernel/Ingest own bounded source/control enforcement.
Membership activation creates no evidence grant or saved default. Watching uses the
reviewed choice and remains subject to the native write/finalization barrier.

Before-fence cancellation requires owned destination cleanup and funding
settlement; after-fence recovery completes the restriction. Physical cleanup,
terminal owner publication and actual per-root funding acknowledgments remain
separate from logical completion. Frozen finalization can remain `acknowledging`
after the Core receipt becomes terminal. Exact receipt/key recovery preserves
historical acknowledgments and later current attention independently. No frontend
or read-only status path automatically replays a mutation. A pending frozen
finalization refuses Cancel rather than claiming `AlreadyComplete`.

The following Windows receipts were inspected with matching log SHA-256 values.
Cargo runs used the existing `artifacts/v190/p10-target`, `CARGO_INCREMENTAL=1`,
locked offline dependencies and `--timings`; durations include command execution
and any compilation. These are observed timings, not a controlled speedup or
cross-platform comparison. Every native test row below exited zero with nonzero
tests. The formatting row ran no tests.

| Exact run label | Selected scope | Observed result | Wall time |
| --- | --- | --- | --- |
| `p10-store-split-r3` | Store library `split`, no default features | 21 passed, 0 failed, 0 ignored; 708 filtered out | 45.7 s |
| `p10-store-filtered-lookup-r1` | Store library `catalog_postings_follow_replacement_deletion_and_exact_case`, no default features | 1 passed; 728 filtered out | 2.8 s |
| `p10-cleanup-pin-r2` | Kernel `source_policy` target, `service_policy_cleanup_releases_only_its_own_pin_and_preserves_actual_error_reports`, all features | 1 passed; 12 filtered out | 9.4 s |
| `p10-kernel-policy-full-r1` | Entire Windows Kernel `source_policy` integration target, all features | 13 passed, 0 failed, 0 ignored, 0 filtered out | 22.2 s |
| `p10-native-happy-cutover-r7` | CLI library `real_empty_native_owner_completes_witness_fence_remainder_membership_and_funding_ack`, all features | 1 passed; 511 filtered out | 43.3 s |
| `p10-fmt-r2` | Workspace formatting check | Exit 0; empty error log | 6.0 s |

Receipts/logs use those exact labels under ignored `artifacts/v190/`; the native
happy-path timing report is
`p10-target/cargo-timings/cargo-timing-20261105T204536623Z-cf3e0779d011bd1b.html`.
The cleanup-pin case is included in the 13-case integration target; the two rows
are separate runs, not 14 distinct tests. Unix-only protected-path cases are
cfg-excluded from that Windows target. The filtered Store result is one posting
replacement/deletion/case regression, not a full Store suite or all native consumers.

The r7 native-owner case uses production native admission/control and private
Kernel jobs with an **actually empty worker/helper/client census**. It exercises
witness refresh, explicit metadata Resume, original fence, independent remainder,
reviewed membership and both funding acknowledgments, while preserving source
bytes and grants/defaults. It does **not** qualify live process/provider drainage,
active client revocation, managed installation or semantic-ready inference. Its
actual Rust owner is
`project_supervisor::guarded_qualification_tests::split_tests`; it is a library
fixture, not an installed-process campaign.

Workbench separately passed host/webview/Assistance compilation, real bundles and
native catalog checks, followed by **733 passed, 19 conditional skips** in its full
752-case run. After the individual-receipt limit was aligned to Core's 14 KiB
budget, compilation/bundling and **133 focused cases** passed without another broad
suite. The prior full run is not relabeled as the later receipt-boundary input.
Its complete native snapshot, historical acknowledgment, frozen finalization,
missing/expired evidence, lost-response/restart, later-latch and no-replay cases
remain frontend/controller evidence, not native transport or platform acceptance.

**Later development evidence, 5 November 2026:** `p10-abrupt-capture-r3`
passed the actual abrupt-process funded-capture recovery and unknown-staging
protection regression. `p10-cli-split-combined-r2` passed six selected CLI cases;
its one ignored helper is explicitly launched by the crash parent. These checks
use native source `afa4ffe7…` (944 files), not the later input below.

The actual process/provider test `p10-resource-debug-r6` passed on native source
`f7d6a19d…` (945 files), using the separately recorded matched debug pair built
from `0419c791…`. It includes native/CLI/MCP evidence, client drainage, original
pause preservation, concurrent queries and both final funding acknowledgments.
The cancellation process case separately passed in `p10-resource-debug-r5`.
The production pause correction aligns Rust admission with the existing SQL
classification; `p10-paused-parity-r1` passed the real SQL/Rust job-kind matrix.
Earlier failed development runs remain recorded; neither debug run qualifies
the final release pair, all platforms or total resource cost. The observer uses
canonical resource reads and sampled file lengths; unsampled peaks, physical
allocation and total CPU remain unknown.

**Historical outstanding work at that checkpoint:** the required Neo batch and its post-test `cargo clean`,
final frozen release/source/runtime gates and independent combined review remain
outstanding. No P10
Neo execution is claimed from the prepared ignored scripts or their local mocked
owner checks. Full workspace/feature-mode/native CLI/MCP/runtime gates must retain
their own actual commands and identities; neither the 13-case integration target
nor this empty-census success substitutes for them. P11–P14 and the deleted-source
follow-up remain unaccepted, and the implementation plan is not complete.


### P10 — frozen qualification passed; integration review pending, 5 November 2026

**Qualification passed; primary integration/review pending. P10 is not yet
phase-accepted, committed or published.** Version **1.8.8** remains unchanged.
P11–P14 and the deleted-source follow-up remain unaccepted.

`p10-final-qualification-r6.json` records all **22 Windows gates passed** on exact
native source **`c713b469d7d4a646a9ceab52aa7ef28ecd2b1af19a787312d2b0b9e843d4321b`
/ 945 inputs**. Every stage's actual/effective exit is zero; receipt/log hashes
and before/after source identity agree. The fresh release pair
`p10-runtime-final-r3` is sealed before/after subsequent stages. It contains the
scoped metadata correction: full-grant snapshots/coverage/readiness preserve
source policy and union filtering; narrow, stale and revoked scopes still refuse.
The exact previously failing permanent-trim native CLI/MCP case passed on that
new pair before the broader process and workspace runs.

| Frozen R6 scope | Observed result |
| --- | --- |
| Formatting; lightweight and workspace all-feature Clippy; required lightweight check; workspace build | Passed |
| Exact permanent-trim actual native/CLI/MCP regression | 1 passed |
| Actual split process/resource completion and pre-effect cancellation | 2 passed |
| Full workspace, all features, offline, `--no-fail-fast -- --test-threads=4` | 2,565 passed, 0 failed, 49 ignored; 1,710.4 s wall |
| Canonical native/CLI/MCP client checks; publication faults | Passed; six publication-fault cases |
| Unchanged Off/Report legacy latency comparisons | 84/84 passed; no bound changes |
| Nine relevance/quality cohorts, three trials; frozen comparisons | Passed |
| Local CI `SkipBuild`, smoke/clients/install suites | 3/3 passed |
| Repository checks, notices, fresh live dependency policy audit, Python checks, whitespace | Passed; 446 Python tests |

The source gates/build/test stages used locked offline inputs. Only the fresh
advisory gate used network access and retained its existing reviewed dispositions;
this is not a claim of zero advisories. Windows native-boundary fixtures used a
new owner-only inheritable test base, verified read-only before every gate. No
existing user/shared directory ACL was repaired or native policy weakened. R4's
full failed census and the unlaunched R5 preparation remain historical evidence.

**Mac Neo:** the broader `p10-native-20261105-r2` batch passed **16 steps / 66
tests** on **`a7458f86…` / 945 inputs**. The later scoped metadata correction is
separately qualified by `p10-native-20261106-r3`: formatting plus the complete
10-case Kernel scoped-read module in lightweight and all-feature modes, **three
steps / 20 tests**, on the exact **`c713b469…` / 945-input** source. Both cleanup
receipts report test exit 0, `cargo clean` exit 0, target absent and source/reports
preserved; clean logs report **4.3 GiB** and **2.2 GiB** removed respectively. The
66 earlier tests are not relabeled as tests of the corrected source. No complete
Mac workspace/runtime/performance, Intel or Linux qualification is claimed.

**Bounded release-pair resource observations:** `p10-resource-final-r6` uses the
existing three-root source fixture (four files per root, 621 bytes total), four
query clients and 250 ms sampling.
Completion retained 8 baseline, 83 during and 8 settled successful samples,
including **8 successful samples while the same split child was observed
Running**. One scope refusal remains a failed sample; read-only re-confirmation
preserved the same independent grant/selection and did not replay that read.
Sampled RSS reached **181,501,952 bytes**; separate logical file-length highs were
357,431 original / 309,124 destination / 239,144 unrelated bytes. One exclusive
destination Index span reports **328,125 microseconds CPU**. Total split CPU,
true instantaneous physical disk/RSS peaks, allocation/COW overhead and unsampled
spikes remain unknown. These observations establish neither speedup nor a large
semantic workload result.

Cancellation retained 8 baseline, 20 during and 8 settled successful samples.
Its held setup lease kept cancellation at Accepted before any child build;
contention is explicitly **not applicable for this metadata-only cancellation**,
not an active-build cancellation result. Full native responses, timings, source
hashes, cleanup and funding-ack observations remain in the ignored artifacts.
Installed-editor behavior, semantic-ready split execution, broader workload and
platform matrices, signing/notarization and publication remain outside this
qualification. Primary combined review and integration must precede phase
acceptance; this record does not complete the 1.9.0 plan.

**Subsequent review finding:** the `c713b469…` qualification above remains
historical evidence. A witness deadline/cancellation checkpoint gap inside the
bounded posting lookup is being corrected, with new focused regression/gates
pending. Primary review will record the successor source and evidence before
phase acceptance; the passed campaign is not relabeled as that correction.


### P10 — local phase checkpoint, 5 November 2026

P10 passed its local phase checks for continued development in unpublished
**1.8.8**. The broad frozen R6 qualification above remains bound to `c713b469…`;
it is not relabeled as the later source or a published artifact.

Final review found that the remainder witness checked cancellation/deadline only
around an internal posting scan. The narrow correction adds a no-data checkpoint
before each shard, decoded candidate and final artifact validation. Existing
authorization and policy filtering still precede data-bearing selection; errors
refuse the lookup instead of claiming an empty remainder. Each indivisible map
read/decode remains bounded by existing value and 4,096-path shard limits.

Successor native source is
**`fadf30fbd45c92ccca0c384eb10e4802f12d1e7af47d07741c251d140c4e45f5` / 945 inputs**.
`p10-witness-correction-validation-r1.json` records seven passed Windows stages:
formatting; all 15 Store posting tests, including two deterministic cancellation
regressions; all 13 Kernel policy integration cases; the actual native-owner
cutover/funding-ack case; the actual witness beyond 256 selected-prefix files;
minimal Store/Kernel Clippy; and workspace/all-target/all-feature Clippy with
warnings denied. These focused regressions extend the earlier broad campaign;
the full workspace/latency/runtime suites were not repeated for this callback.

Mac Neo `p10-native-20261106-r4` separately passed formatting and the same 15 Store
tests plus both native witness/cutover cases: **four steps / 17 tests** on this
successor source. Collected cleanup proves `cargo clean` exit 0, target absent,
source/reports preserved and timings retained. Earlier 66/20-pass Neo batches
retain their own source identities. Independent source/test review and primary
integration review found no remaining concrete blocker in the reviewed scope.

The frozen release pair remains the earlier `c713b469…` pair; this checkpoint
does not relabel it as containing the callback correction. The final local test
installer must be rebuilt from the completed plan. Resource and platform limits
above still apply. P11–P14, deleted-source recovery, final local test installer
and contributor PR #60 assessment remain unfinished. No push, publication or
normal installation is part of this checkpoint.


### P10 — supplemental preceding-phase latency comparison, 6 November 2026

`p10-versus-p9-gates-r1.json` compares the existing P10 frozen measurement
reports with the accepted P9 R5 reports: **all 84 gates passed**, with unchanged
limits. The earlier P10 helper retained its P8 preceding anchor; this supplemental
comparison supplies the direct P9 preceding-phase comparison. It reuses actual
saved measurements and does not rerun workloads, change limits, relabel runtime
source identities or establish a new speedup claim.

### P11a — local phase checkpoint, 6 November 2026

P11a is accepted for unpublished local development. Version **1.8.8** remains unpublished. The 27-file
feature/test tree adds opt-in metadata reporting, not Lean/Rich retained-profile
migration. Later phases and deleted-source recovery remain unfinished.

Completed observations retained separately:

- Original native source **`33e6df135da90839f409dfbba6dd2ce338af31675875e99f20d0b94af212f3f7` / 948 inputs**:
  scoped formatting/Clippy and **18 focused native tests** passed. The new
  flattened conditional display regression subsequently passed on its own
  current test source; this is a display fixture, not native transport evidence.
- Mac Neo `p11-native-20261106-r1` used that original `33e6df13…` source and
  passed **nine steps / 12 tests**: formatting, Core3, Ingest1, and Kernel2/MCP1/
  CLI1 in minimal and all-feature modes. Collected cleanup proves `cargo clean`
  exit0, target absent, source/reports preserved and timing reports retained.
  This is focused ARM macOS evidence, not a full Mac workspace/runtime campaign,
  Intel/Linux, model/OCR, installed-editor or signing qualification.
- Workbench compile/bundles passed; the full UI run reports **741 passed,
  0 failed, 19 skipped / 760 total**. Skips remain skips.
- Actual R4 process checks passed in both feature modes. They exercise complete
  canonical native, CLI tools call, real tools console, convenience info and MCP
  alias results and input catalogs; Full/Terse and absent/Lean/Rich reply choices,
  scoped unknowns, full-root policy observations, conditional validators and
  invalid boolean refusal are covered. Positive replies check outer error flags
  before unwrapping and exactly one requested index. Earlier shape/prompt fixture
  failures remain recorded; assertions were corrected to the existing flattened
  `ConditionalRead` and `Result:` console protocol, without product changes.

The current test/source identity is
**`979568465e27b37bc82538ca91e6f5ef843f20886270679ba7da1f42e40e53fc` / 948 inputs**.
The exact R1 release pair retains its original compiled `33e6df13…` source and
build receipt. Only two external test inputs differ; **946 other declared native
inputs and all production module bytes are unchanged**, independently verified.
Reuse requires equality with the original full runtime seal before/after stages;
it is not a fresh build or a relabeled producer. Current test revision and
original binary identity are recorded separately.

`p11a-final-qualification-r3.json` reports **passed**: all 12 continuation stages
completed successfully, adopting five exact passed R2 prerequisites and the two
passed R4 transport checks with their original receipts rather than claiming a
rerun. The original pinned Nextest CI-profile run on `97956846…` collected
**2,571 passed / one failed / 53 skipped** in 1,243.263 s. Its sole failure was
stale generated `docs/cli-reference.md`; canonical parser-owned regeneration then
passed three selected checks, followed by the ordinary strict one-case reference
verification. The original failed run is retained; the full suite was not rerun
or relabeled as an initially clean run. All selected cases now have passing
evidence. Four routine long campaigns and the remaining skipped/ignored cases
are not converted to passes; P14 retains its required expanded campaign.

R3 passed doctests, native clients, six publication faults, the three local
runtime smoke suites, Off/Report replay, all **84 unchanged latency gates**,
repository and license checks, fresh dependency audit, Python tests and whitespace.
The full UI rerun after flat/historical fixture correction again passed
**741 / 0 failed / 19 skipped**. Consumer fixtures verify flat canonical and
historical nested reports through actual host/session persistence and full or
selected exports; actual native parity is established separately by the R4
process receipts. No new Mac full workspace or cross-platform claim is inferred.

P11a qualification and primary review passed. P11a is accepted for unpublished local development. P11b–P14, deleted-source recovery and final
local test installer remain unfinished. No push, publication, normal installation
or completion of the full plan is claimed.


### P11b — local phase checkpoint, 6 November 2026

P11a is accepted at `f2bf2d219eda7096986565fcbc57f7ef9b1c5c7a`.
P11b is accepted for unpublished local development; working version **1.8.8** remains unpublished.
No P12 implementation or documentation is enabled by this checkpoint.

Functional source **`e38277697881592edaea8b50311cfea628f6ce70c643c1a0bc0bfcf95c5d2ef1` / 954 inputs**
passed its scoped formatting/Clippy and exact **52-case** native selection,
including migration, barriers/interruption, quota, reuse and inherited split
behavior. Current source
**`8c4fc38e85435cb5df60f3dfa618e348706b02465989518e953ed1b76ec0c773` / 954 inputs**
adds only two optional external resource-test hooks; all **952 other native
inputs and production bytes are unchanged**. Current formatting/Clippy and both
modified resource cases passed. Earlier 52-case and Mac receipts retain `e382…`;
they are not relabeled as new `8c4…` executions.

The fresh release pair was built from `8c4…` with incremental compilation off
in 802.1 s. `p11b-final-qualification-r1.json` passed all **11 scoped stages**:
one actual retained-migration CLI/MCP release case, 16-file/model native clients,
six publication faults, unchanged Off/Report frozen replays, all **84 latency
gates** with explicit preceding **P11a R3** reports and unchanged archive/control,
repository/license checks, fresh dependency audit, Python tests and whitespace.
The sole release build was adopted by exact successful receipt/source/full seal;
no duplicate build was run. Full UI compilation/bundles passed and the full run
reports **746 passed / zero failed / 19 skipped (765 total)**. Flat canonical and
historical nested reports remain covered through host/session/full/selected
exports; UI fixtures do not substitute for actual native transport evidence.

Mac ARM retained the same `e382…` source. Initial batch
`p11-native-p11b-20261106-r1` passed its first six steps / **eight tests**, then
correctly failed a wrong `--bin` selection that matched zero tests (actual Cargo
exit0, effective validation failure). The initial failure is preserved. Corrected
`p11-native-p11b-20261106-r2-supplement` passed the three intended CLI tests:
**11 passed tests across the two recorded selections**. No product fix or retry
of failed product behavior was involved. The combined
`p11b-neo-combined-verification-r1.json` binds nine passing commands / 11 tests,
while retaining the failed initial command. Both collected owners report
`cargo clean` exit 0, target absent and reports preserved. This is focused ARM
macOS evidence, not full Mac workspace, Linux/Intel, editor or default ONNX
qualification.

`p11b-resource-observed-r1/verification.json` records two actual development-build
cases with seven Kernel phases and four tiny-wordvector-helper phases. With
vectors disabled, the four-file Kernel fixture observed indexed/reused counts
of **4/0** for legacy cold, **3/1** for Lean migration, **0/4** for Lean no-op,
and **1/3** for either a one-rule override or one-source edit. The refused Rich
quota pass preserved the Lean publication and logical used bytes. Logical used
bytes were 200,425 after cold indexing; 233,242 after Lean migration, refusal
and no-op; 233,327 after the rule override; 233,314 after one edit. These include
retained history and are not pruned-format or physical-disk comparisons.

The helper fixture recorded four embedding inputs and four cache misses at
baseline, unchanged through Lean, Rich and Rich no-op. Cumulative reused inputs
were **0 → 4 → 8 → 8**; the four graph records / 816 vector bytes were preserved.
Cumulative model-load microseconds were **16,207 → 31,546 → 47,077 → 47,077**:
model-loading work grew during migrations, so this is not zero model-load work.
Phase wall/counter data remain in the receipt. One tiny development fixture has
no matched performance baseline; no speedup, total CPU, service peak RSS,
physical peak footprint or default ONNX-forward claim follows from it.

The scoped final queue intentionally does not repeat the broad workspace or the
three local runtime smoke suites. P14 retains cumulative workspace, four routine
excluded campaigns, doctests and installed/platform matrix requirements. P11b
qualification and primary review passed; it is accepted for unpublished local development. P12–P14, deleted-source
recovery and the final local test installer remain unfinished. No push,
publication, normal installation or complete-plan claim is made.


### P12 — local phase checkpoint, 6 November 2026

P11b is accepted at `aa28425d2d3031b42f358e41d6ec366822635379`.
P12's current **1.8.8** implementation passed primary review and is **accepted
for unpublished local development**. The frozen
native input identity is
`4121692a2cc0272358023f5362ae886eeed4fd06203cea7af23bbfc5f4be86b9`
(**960 inputs**). The matched Windows release pair built in **815.503 seconds**
with release incremental compilation disabled. All **12** final qualification
stages and **19** scoped ARM64 Neo tests passed; Neo cleanup exited zero and
removed the target. This is a local phase checkpoint, not publication or final
installed-editor acceptance.

The [refresh guide](../source-verification.md#explicit-reference-refresh)
describes the first wave: one existing index and at most 64 referenced sources,
subject to wire limits, with complete configured-source metadata authority even
for selected work. Partial Project/path grants and several refreshed indexes in
one operation are unsupported. Native review binds the actual backing, grant,
original publication/source identities, settings and configured policy/model.
Full fallback is a separate explicit choice. Generic configuration enqueue/retry
cannot bypass the dedicated native refresh owner. MCP evidence stays read-only.

Kernel checks the original publication, unfinished bootstrap/current work and
saved settings while holding the actual writer, before recovery or source work.
Ingest records protected matcher inputs and qualified absence; the authenticated
Current witness binds exact incarnation/sequence. Trusted source and qualified
semantic-only publication rebind it, while older writer publication invalidates
an unchanged unknown witness. Full requalification cannot no-op on identical
historical unbound bytes. Original Lean/Rich identity, source coordinates,
unselected payload, pause/manual/watch choices and semantic policy are preserved.

Exact receipt lookup remains read-only after lost responses or restart. Missing
saved evidence remains unknown; no mutation replay or reconstructed success is
introduced. Requery is separately confirmed from the original query, yields new
native evidence and leaves the old result/refresh acknowledgment historical.
Workbench rechecks host trust, selection and Stop generation across asynchronous
review and journal boundaries. Its refresh-specific originals stay in memory;
metadata/receipt export does not invent unavailable originals after reload.

**Observed focused evidence:**

- Strict Windows Clippy passed on all targets/features of Core, Ingest, Store,
  Kernel, MCP and CLI with warnings denied (`p12-focused-clippy-r8`). This is a
  scoped six-package check, not a claim of a full workspace suite.
- `p12-focused-r4` passed **39/39**, zero failures, with 2,578 tests not selected
  by its explicit filter. It includes all **18 new cases** and 21 Windows legacy
  cases. One additional legacy ignore-control alias test is `cfg(unix)` and is
  retained for Neo instead of being counted as a Windows pass.
- The actual CLI process case passed in **115.855 seconds** using Cargo's matched
  debug main/helper pair and the existing isolated component-format fixture. It
  exercises real native Lean/Rich migration, full qualification then selected
  publication, a transport closed without reading its acknowledgment, receipt-only
  reconciliation, paused Pending work, explicit unpause, stale review refusal,
  graceful restart and actual shared-helper continuation. Complete native/CLI/MCP
  FileInfo/profile/status/query responses and exact scoped members are compared.
- The full extension suite after the epoch correction passed **776**, failed
  **0**, skipped **19**, out of **795** (`p12-ui-full-epoch-r1`). These mocks and
  host-flow tests do not substitute for installed-editor qualification.

**Final source-bound qualification:**

- `p12-final-qualification-r1` passed all **12** stages: actual refresh release
  parity, retained-profile release parity, service clients, publication faults,
  Off and Report samples, unchanged latency gates, repository checks, license
  notices, fresh dependency audit, Python tests and whitespace checks. These
  adopt the existing successful focused and UI results; they do not rerun or
  relabel those earlier results. All **84** original latency gates passed with
  their limits unchanged. No P12 resource-cost improvement is claimed.
- The release producer and all native stages retain the exact 960-input source
  above. Main SHA-256:
  `1e41d1bf497eb8b092491629160e0d988cdd15c675417603a1fb30844e4543e1`;
  helper:
  `bc873940a807c772deaf0b678ca878a769d8a99023a54eb5b78a1e9f102750a1`.
  Runtime/build/install manifests and notices remain sealed together; no
  replacement producer or source identity is inferred from current timestamps.
- `p12-neo-combined-verification-r1` confirms the same source, **19** passing
  tests across **10** successful commands on ARM64 Neo. Actual native/helper and
  source/OS custody cases passed. Test exit and `cargo clean` exit were **0**;
  the target was removed, while source, reports and timing evidence survived.
  This is scoped Mac evidence, not a full workspace, installed release, Intel,
  Linux, signing, default ONNX or editor/platform qualification.

**Earlier attempts remain failed evidence.** R1 collected **31 passed / 8 failed
out of 39**, while the original queue incorrectly expected 40 because it included
a Unix-only baseline. R2 correctly selected 39 and again collected **31 passed /
8 failed**. Native validators had conflated catalog and service-request namespaces
and then rejected valid native request epoch zero. Corrections preserve those
separate authority domains and the actual registry epoch; fixtures were not made
to pretend they were equal or positive. The earlier attempts also exposed a
writer preflight that recursively acquired its already-held lock through ordinary
inspection, losing the settings observation and reporting configuration change.
The owning writer-aware digest fixed that common pipeline error before R3.
R3 then collected **37 passed / 2 failed**: identical historical-unbound witness
bytes suppressed full requalification, and the process fixture compared Pending
with Running job observations from different moments. The no-op guard now also requires exact
current witness binding; moving-state polling uses complete native observations,
while strict full native/CLI equality remains at stable receipt boundaries.
R4 passed after those corrections; no earlier result is relabeled.

This is bounded development evidence, not a published release/installer/Genesis, ONNX/default
provider, physical power-loss, arbitrary remote source or total model/resource
cost qualification. Publication-before-saved-result remains failed/unknown.
Release-runtime, source-bound scoped Neo qualification, mandatory `cargo clean`
and primary review passed. The authorized signed commit remains a separate
Git action; no human sign-off is invented here. P13 remains
conditional; P14, the separately requested missing-source isolation/cleanup fix
and final local test installer are not completed by this entry.
