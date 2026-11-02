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
| P0b | Complete — commit recorded at next checkpoint | Synchronized 1.9.0 increment and forward-line policy; no schema migration |
| P1–P2b | Not started | Compact-plan correctness, independent replies and existing-observation guidance |
| P3–P4 | Not started | Decision records/contracts, then opt-in single-index verification |
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

## Restart handoff — stopped at the maintainer's request

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
