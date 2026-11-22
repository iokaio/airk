# Arbiter and release: what is left

Handoff at the user's explicit stop request, 15 November 2026, approximately
18:45 UTC. Implementation, qualification and packaging are stopped. This is an
unfinished-work inventory, not a release approval or a claim that the remaining
work fits an earlier estimate. The earlier delivery deadline has passed.

## Read this first

**There are no completed installers for the requested three-model candidate.**
The selected Context models have not completed task calibration, held-out goal
evaluation or final composed qualification. The on-demand download changes are
private proposals, not integrated production code. A complete final workspace
test pass is also outstanding.

The next agent should fix the two concrete blockers below, integrate and test
the prepared download changes, and then finish qualification and packaging.
Do not restart model selection, encoder training or the whole split project.

The working branch is `feat/advanced-split`; the latest committed and pushed
milestone is `3d5f88473e449ccff35833be55d79dba33729f3f`. It contains the native
Context scope-identity correction and on-demand-delivery documentation. Version
1.9.0 is already selected. The native source identity used by the latest passed
R148 checks is
`13dc85349c783077aec1ad4eae194e17542a3205639c0fe7d927b165a6a0cf25`.

Use these existing owners for requirements and evidence:

- [Implementation/release plan](../split/codex-plan.md), especially sections
  8–11 and the P0–P11 acceptance table.
- [Recorded decisions](../split/split-plan.md).
- [Execution ledger](../simplify/one-process-ledger.md), including the
  15 November 17:16 UTC checkpoint and subsequent R150 note.
- [Arbiter guide](user-guide.md), [concept](concept.md), repository `AGENTS.md`,
  `CONTRIBUTING.md` and owning component contracts.

Paths below starting with `artifacts/advanced-split/` are local ignored handoff
material. They will not arrive in a fresh Git clone. Preserve this checkout and
its private artifacts. They contain private fixtures/state and must not be
staged, bundled with installers or published.

## The user's decisions remain fixed

| Component | Selected model and behavior |
| --- | --- |
| Baseline retrieval | Retain original MiniLM; the replacement development experiment tied. |
| Arbiter Focus | Qwen3-Reranker-0.6B, independent of Context. |
| Arbiter Context — Compact | Qwen3-1.7B. |
| Arbiter Context — Large | Qwen3-4B; one Context size selected at a time. |

- All three optional choices belong in the first candidate. The user rejected
  deferring Context and delivering only core/Focus.
- **Do not package optional Arbiter weights in an installer or VSIX.** The app
  must download them from their usual public source, with a verified exact-byte
  backup where one exists. Bundled baseline MiniLM remains an installer input.
- Use explicit app-owned acquisition. Do not substitute a large manual ZIP set
  for the requested download experience.
- Test this selected path; further model bakeoffs or training are not requested.
- Preserve budgets, source isolation, current authority checks and quality
  gates. Graded reporting is useful but does not turn failed qualification into
  a pass.
- Keep the architecture Word guide deletion. Do not restore it to fix a link.
- Milestone signed commits/pushes to this branch are authorized. Public model
  uploads, releases, deployments, merges and PRs are not authorized by that.

## 1. Unblock the Context prompt before another CAL capture

### Observed failure

The current Context prompt does not fit the selected models' existing
2048-token profile limit on a retained real request. It also has to fit the
remaining shared input budget: Focus had already consumed 1024 tokens on that
request, leaving 1024. Neither Context model has a successful task-score result
from this CAL path yet. Raw export/resource checks are separate evidence.

The request contained seven whole action cards, four source units and five
scored reads. Most cards repeated the same hypothesis set. The prompt also
included opaque literal-read provenance identifiers and repeatedly escaped JSON
strings through the generic Qwen compiler.

Owning production file:
`crates/airk-retrieval/src/arbiter/context_action.rs`. The generic compiler is
`compiler/qwen.rs`; its policy digest is separate from the Context task-policy
digest. Changing Context source changes the task-policy digest and dependent
metadata even if weights and generic compiler remain identical.

### Prepared, unfinished correction

Private directory:
`artifacts/advanced-split/models/selected-native-goal-driver-r1/context-prompt-fit-r1/`.

- `context_action.rs.proposed`, SHA
  `823084e06c3ce73ed650fb1bdc1a0c01d12fec7b1eb85b80b8557ecbb974d3f0`.
- `qwen-tests.rs.proposed`, SHA
  `c81f34b72e133f4bcf0c0f888c0747b6bb44ac4a42ef9b7882bfdd3f701fd9bb`.

The proposal interns repeated paths/hypothesis sets and uses compact tuples.
It parses native SourceLiteral targets through their Rust owner, preserving
semantic query/path/line/span while leaving machine identities in native alias
mappings. It must preserve every offered card, read, source excerpt, hypothesis,
gap and reason. No truncation, subset selection or budget increase was approved.

**This patch is mid-work and unverified.** No formatting, compilation, codec
tests, exact tokenizer census, new policy digest or profile regeneration ran.
Review the newly added backslash/Unicode fixture escaping in particular. Three
codec tests and an ignored native OLD/NEW tokenizer census are proposed. The
separate structural-count file reports cardinalities, not prompt token counts.
No completed install manifest or impact matrix exists.

The preceding recorded decision is
`models/context-prompt-fit-root-decision-r1.json`, SHA
`508f4c4cac7f9c59c16a64d1095f73c63230ec676f4c5ae19ddb30faee45ee32`,
under `artifacts/advanced-split/`.

Remaining steps:

1. Review/finish the codec and meaningful round-trip/alias/Unicode tests.
2. Use the actual native tokenizer to measure old/new prompts against both
   the 2048 profile cap and actual remaining shared budget. Do not raise limits
   to obtain a fit.
3. Regenerate affected task-policy/calibration/profile metadata through native
   owners. Record which old component checks remain applicable by exact
   identity; do not relabel old receipts as new-profile evidence.
4. Compile and run focused regressions before a new expensive capture.

## 2. Resolve the end-to-end Answer timeout

R154 CAL phase 00 failed on canonical Answer call 0019 after 30.009779 seconds.
The caller reported `session_deadline_exceeded`, sent=true, outcome unknown,
retry=false. Do not replay that uncertain call or reuse its partial capture as
successful CAL evidence.

Two Focus forwards succeeded and settled, consuming 1024 tokens and about
7.014 seconds of inference. Context performed zero task forwards. The passive
Context observer wrote `record0004.json` with roughly 19.6 seconds remaining.
Kernel then continued deterministic evidence gathering after Context became
unavailable. Neighboring successful Answers already took approximately
21–28 seconds. The specific remaining bottleneck has not been proven.

Established source finding: application/focus_application metadata callbacks
do not clip their waits to the original read's remaining deadline. Their I1
requests already have a five-second timeout; they are not unbounded. A proposed
private `*_before(deadline)` adapter plus production `timeout_at` should retain
that ceiling and existing retry rules. **No deadline-fix patch or test was
written.** This omission is real, but its role in R154 is still a hypothesis.

A non-neural CPU diagnostic was prepared but never mounted/run:
`models/post-observer-cpu-profile-r1/mount-manifest.json`, SHA
`038cf0359e6a334d74e1ae642d94c0022cc4af966d2b8f58fe9ed8446650c85b`.
It profiles local observer processing, not actual IPC or later fallback waits.
Use only if it answers a remaining concrete question.

The last fixture and collectors closed naturally. Closure evidence is
`stage-service-preparation-r1/cal-execution-r102/R154-closure.actual.json`, SHA
`8c2f91b8ae8cb6cd13a1ffde111f4a5c6019283a081b96657ef0f89eb7f8cab6`.
Actual journal: 40 Focus forwards, zero Context task forwards, no dropped journal
records, no active owned processes, lease reacquired. This is cleanup evidence,
not successful CAL100 evidence.

The last build started the existing optimized `release-local` test profile to
avoid judging shipping timing from unoptimized debug fixtures. It was stopped
at the user's request. Its partial log is
`receipts/optimized-foundation-r155/foundation.log`; no completed receipt or
optimized fixture qualification exists. Warm compiled dependencies remain.
No Cargo/rustc/Python process was observed after the stop. Preserve the target.

## 3. Integrate and qualify on-demand downloads and backups

Existing installer/VSIX allowlists exclude optional weights. The missing work
is the native acquisition/source-map/fallback path and real source availability.

Private bundle:
`model-download-origin-preparation-r1/combined-consumer-readiness-r1.json`, SHA
`fec1551d62dfc8d4319abc085a3ae67aead05424ff0b33a8fa1759afc809e9f4`.
It enumerates 20 proposed source/contract/test targets, plus an append-only
Service contract change. **It is not ready to apply unchanged.**

| Part | State at stop |
| --- | --- |
| Core contract | `core-contract-r1/manifest.json`, SHA `9ab6150bc84cfbc01aa00f33694f79b8bfdf6b4524e9705e1783fc3bc94cef95`; passed isolated R150 gates, not mounted in main. |
| Store restart | `store-restart/r2/manifest.proposed.json`, SHA `d7574a22170093303db47e67371248166ec956e7a03b704eee191ecd18c38f9f`; eight prepared controls, uncompiled. |
| Service fallback | `service-consumer-r6/manifest.frozen-r6.json`, SHA `aead19726d9a3d0fb02ede9d357e66973783a89f5c9915ceeca2a5ddd89671fb`; uncompiled, known test syntax defect below. |
| Publication | `publication-consumer-r2/manifest.json`, SHA `fdf90eed63f27d1512890b5fcdf5dba369833bc304a8f58da95a707113658178`; five new controls plus retained coverage, uncompiled. |

Known defect: Service R6 references
`service-consumer-r5/source_attempt_tests.rs.proposed`. Two `whole_asset` calls
inside `assert!` lack a comma between `&mut request` and `None` (around lines
222/265). No corrective version exists. Rustfmt returning success did not parse
those opaque macro expressions; compiler validation is still required.

The recorded design permits up to two exact-byte backup rosters, immutable
reviewed Hugging Face URLs and official AIRK GitHub release parts. Every part
retains its length/SHA, original 900-second deadline and four-redirect budget
across attempts. Only source-class failures permit whole-asset fallback.
Cancellation, local custody failure and deadline expiry stop. Store must prove
ownership before replacing an unfinished asset; completed cache is retained.
Recover remains local; only explicit Provision acquires over the network.
Existing bounded logs record static failure classes, never signed URL queries.

Root decisions under the same private directory:
`root-contract-decision-r1.json`, `root-final-source-map-decision-r1.json`, and
`root-store-admission-refinement-r2.json`.

Core's actual Rust-emitted fixtures and schema are in
`receipts/core-contract-r150/native-emitted/`:

- `download_origin_cases.json`, SHA
  `9f4a7253da9b80c9b70fba49370493bee157ef57dc9d71bf63b52222db95ac77`.
- `arbiter.schema.json`, SHA
  `0de80d341527681bd9a1f6ba3a525d5bac7c0fafdfc9795b49bcd4e2f3705a0f`.

The detached Core gate checkout is
`artifacts/advanced-split/core-contract-gate-r149`. Reuse its authenticated
generated outputs after reviewing the mapping; do not overwrite newer main
source with its older checkout. R150 passed Clippy, six focused tests, emitter,
schema comparison and contract-types write/check. An earlier LF/CRLF fixture
failure is preserved; main fixture bytes were not weakened.

A draft one-off mount script exists:
`model-download-origin-preparation-r1/combined-mount-r1/mount_optional_sources.py`.
**It was not run and its manifest references the defective old bundle.** Fix
and freeze the proposals first, review the complete change, then regenerate
all-input/base-hash preflight and mount mapping. Preserve the newer R147 Service
contract text. Run scoped Clippy and the nonzero consumer regressions, regenerate
shared types/assets, and test the complete real acquisition/recovery path.

### Public sources are a real unfinished dependency

`asset-origin-matrix.raw-r1.json` under the source-preparation directory records
seven of 23 raw assets matching public Git/LFS metadata. Sixteen derived assets,
including all six model graph/data files, have no verified matching public
location. Public ONNX variants differ from the tested exports; they cannot be
silently substituted. HEAD/LFS metadata is not a complete download proof.
One small official license GET was verified; no complete model transfer passed.

The exact qualified derived exports need an actual hosting/publication decision,
verified rights and explicit user authorization before upload. Do not invent a
Hugging Face namespace, claim unpublished GitHub URLs work, or label two routes
on one host as independent outage protection. A backup may be absent.

Finish a legitimate local part-inventory/staging sequence before final approved
source maps; the final promoter no longer mints URLs. The private tiny-file
part stager has ten passing tests, but no large payload staging was performed.
After authorized publication, verify complete primary/backup bytes through the
real native transport, then bind actual maps into the final compiled catalog.

### JS, Python and packaging consumers

The Rust publication helper `github_part_filenames` produces the existing
`Publication.assets` GitHub-only subset. It adds no new wire field. HF-only
maps may have an empty GitHub asset map; the complete compiled catalog SHA
still binds all source authority.

Read-only inspection found the current VS Code bootstrap already handles
empty/subset maps and compares the full opaque native projection. No JS patch
or test was created. Add native-fixture-based empty/mixed-source regressions
and contract clarification; do not handwrite TypeScript Profile/source mirrors.
The Python audit likewise found existing loops appear to accept empty/subset
maps while authenticating the full native projection. No Python patch or test
was written. The actual Rust source-case emitter/fixtures must be completed
before meaningful JS/Python regressions; no proposed extra fixture field was
implemented. URL-free local part staging before final source maps remains
unresolved and unimplemented.

## 4. Complete actual model qualification

The current release catalog still has no qualified profiles. Raw profiles with
empty downloads/qualified targets are not release selections.

| Model | Useful existing component evidence; not final qualification |
| --- | --- |
| Focus | W8 block32 export; 12 numerical and 20 resource checks passed; CPU4, 4 GiB reservation, 512 tokens, K4; observed resource p95 about 2.17 seconds. |
| Compact | Exact FP32 export; W8 failed its numerical tolerance and remains rejected. Three numerical and 20 resource checks passed; CPU4, 16 GiB reservation, 2048 tokens, K8; p95 about 19.46 seconds. |
| Large | Exact FP32 export, approximately 16.09 GB weights; CPU16, 40 GiB reservation, minimum 48 GiB host; three numerical and 20 resource checks passed, p95 about 23.13 seconds. CPU8's timeout remains failed. |

Do not advertise these as final supported hardware requirements until fitted,
composed and target evidence is complete. Changing prompt-policy metadata needs
an explicit evidence-impact review even when the neural weights stay exact.

Required remaining sequence:

1. Obtain a successful fixed CAL100 capture after the prompt/deadline repairs.
   The prepared collector uses thirteen ordered phases (12×8 then 4), one
   original fixture lifetime of 10800 seconds, 1800 seconds per phase, 30 seconds
   per read and the unchanged 500-probe bound. It explicitly renews the same
   selected scope between completed questions. All thirteen natural ownership
   closures are required; failed/partial older rows stay excluded.
2. Independently validate the aggregate certificate against actual source,
   artifact, request, scope and ownership identities. Check capture-size bounds
   before scoring. Freeze consumed inputs; do not edit pinned harness files.
3. Run the two fixed raw Context captures serially, with their already recorded
   3360-second whole-census bounds. This does not extend per-call budgets.
4. Fit through the native owner. A safe positive generic Read margin and zero
   UNKNOWN are required. If the fit is unsupported, report that result; do not
   create a nominally qualified profile or tune on confirmation data.
5. Exercise the fitted profiles' resource and ten composed scenarios, including
   MiniLM coexistence, joint memory, cold/warm lifetime, disable/drain,
   settlement and actual set-membership revocation between admission/return.
   Retiring a set is not the membership-revocation scenario.
6. Run the fixed held-out A6 goal comparisons: ordinary follow-up,
   deterministic packet, Compact and Large, Focus enabled throughout, same
   external agent/revision/budgets. Retain completion, turn reduction,
   source/citation, safety and resource outcomes. Existing floors include .75
   completion, .02 gain and .10 turn reduction. Do not tune on confirmation.
7. Complete fitted-asset rights, source, provenance and target proofs, then
   produce all three qualified profiles through the native final promoter.

Latest failed capture:
`models/actual-composed-service-r2/CAL-capture-chunked-r154/phase-00/`.
Retained fixture state is under the same service root; R154 closure identifies
the actual original control/lease/source identities. A successor fixture must
use a fresh work directory and genuine same-owner restart, not copied/adopted
index state, reseeding or a fabricated Configure. Source state is private.

The corrected chunk collector's manifest SHA is
`7ef151f004f514a7a93c788098dcb0e3c0efdc3ed34e464ad15693902562dabf`;
15 Python and six compiled-SDK controls passed. Certificate consumer support
was installed privately. These checks do not prove actual CAL100 completion.
Use the retained R152/R154 controller packets and source pins as historical
templates; regenerate actual bindings after new source/images/start times.

The graded reporter is private at
`models/selected-native-goal-driver-r1/graded-quality-report-r1/quality_report.py`.
Nine controls passed. It reports partial lexical F1, each original human
alternative, evidence/citation coverage, costs and missingness. Lexical overlap
does not establish semantic correctness. Its historical nine-task scores are
not Qwen scores; current selected-Qwen100 is explicitly not run.

## 5. Finish integrated source, clients and release gates

Do not redo completed implementation solely because older plan checkpoints say
it was pending. Compare actual source and receipts with each acceptance row.
The following is the known unclosed evidence backlog, not a claim that every
other P0–P11 item has been accepted:

- Final workspace Clippy/tests on the integrated final source. R148 Clippy
  passed, six focused regressions passed and the actual combined read parity
  case passed. R144 full Nextest selected 4573, ran 294, passed 293 and failed
  one; 4279 were unrun. R148's fix does not make R144 a complete pass.
- Required doctests and the explicitly restored routine-excluded campaigns.
  `final-gate-preparation-r1/run-final-native-gates.py` records the prepared
  sequence, including the separate ignored vector-performance cases. Confirm
  selectors against live source and report nonzero selected counts.
- A current matched shipping `airk`/`airkctl` pair, then the documented
  `check-service-clients.py --runtime-dir ... --files 16 --model` preflight.
  The older R104 pair was reused only as a private fixture client distribution;
  it is not the current qualified shipping pair.
- Full applicable CLI/session, MCP, Admin and Workbench native/real-transport
  parity, refusal/recovery and installed journeys on final source/assets.
  A renderer unit pass or older debug artifact does not close those gates.
- Refresh sealed shared Admin assets and extension outputs through their owners.
  Existing Admin 246/VS Code 1122 passes retain their original source identities;
  the default `artifacts/admin/admin-assets.json` was stale at the last audit.
- Reconcile outstanding UI/DOM/host, split/set lifecycle, installer recovery,
  silent mode, portable use and predecessor ownership acceptance against P0–P11.
  The plan contains historical pending rows; use the ledger's later receipts
  rather than either assuming completion or reimplementing everything.
- Recheck README, documentation index, release notes, Marketplace/VSIX text and
  task Help against the final implemented version. Explain independent Focus,
  mutually exclusive Context sizes, downloads/backups, disk/RAM/CPU needs,
  consent, unavailable hardware, failures and offline recovery accurately.
- Run repository, whitespace, dependency/license and affected packaging checks.
  The script-union checkpoint had three publication-input tests deferred;
  final real publication inputs must close those cases.

## 6. Produce the requested local installers

No candidate set is ready. Packaging must consume tested matched binaries and
current sealed assets, without optional weights or private state.

### Windows

The existing canonical signed local owner is:

```powershell
python scripts/build-local-service-installer.py --force-build --iscc 'C:/Users/tdotn/AppData/Local/Programs/Inno Setup 6/ISCC.exe'
```

Read `CONTRIBUTING.md` and the script's current prerequisites first. It refreshes
Admin assets, builds a matching `release-local` pair, performs runtime/model and
notice checks, and signs native components, uninstaller and outer installer.
Signing tool/account preflight passed earlier; **no current candidate was
actually signed**. Do not use `--unsigned` to bypass the authorized signed path.
Build native/portable packages and the universal VSIX from the verified outputs,
not separately assembled binaries. Exercise install, exact `--silent`, upgrade/
repair, health/read, shutdown, uninstall/retained-data and recovery boundaries.

### macOS and Linux

Mac Neo has retained useful arm64 evidence from an older isolated source:
`receipts/mac-recovery-r131/`, including its retrieved evidence and instructions.
Scoped Clippy, six bridge tests, a `release-local` pair, actual bundled MiniLM
smoke and mandatory cleanup passed. This is not a final-current-source installer
or Arbiter qualification. The host has only 8 GiB RAM and cannot qualify the
selected Context reservations. Developer ID/notary configuration exists; no
final candidate signing/notarization was done. Preserve the dirty original Mac
checkout; use the documented isolated checkout and clean its target after each
batch with evidence retained outside it.

The final Linux platform/installer/portable acceptance also remains to be
established against the plan. There is no current all-platform completion claim.
Do not infer Linux success from Windows or Mac component checks.

Deliver an inventory of actual candidate files, SHA256s, source identities,
signing status, exercised platform/journeys and known limitations for the user's
local test. Local packaging is not public release completion. Publication needs
the separate authorized workflow and verification of downloaded published bytes.

## Operational handoff

- Root was the sole Cargo owner. The active build was stopped; no owned model
  fixture remains live. Agents were instructed to stop. Do not restart work
  until the user authorizes the successor to continue.
- Persistent Windows target at the time of this note:
  `artifacts/v190/p10-target`. Approximately 375 GiB was free at the preceding
  disk check. That target was deleted on 22 November 2026 on the user's word;
  the current Cargo target is `target/`
  ([current assignment](../agent-current-work.md#build-targets-and-private-state)).
- Static ORT 1.30 environment owner:
  `artifacts/advanced-split/ort130-x64-owned-r5/cargo-aggregate-env.json`.
  Load its five environment values through the existing build/receipt owners.
- Bundled-model parent: `artifacts/advanced-split/bundled-model-v3`;
  U8 test model: its `minilm-l6-v2-u8-v3` subdirectory. Keep exact U8/S8 metadata
  and pipeline identities; do not substitute the old v1 fixtures.
- Windows development uses incremental1; release-local leaves the environment
  override unset. Retain `--locked --timings`; do not run redundant cargo check
  before equivalent Clippy. Avoid concurrent Cargo and local NN measurements.
- Protected fixture/evaluation state belongs to sandbox SID1012. Human SID1002
  is used for Git/signing/SSH. Mixing these identities has caused access failures.
- For Git use process-local `safe.directory=D:/code/Github/airk`, not a global
  configuration change. Signed commits use Tyler Jensen's configured identity
  and DCO `git commit -s`; do not invent agent signatures.
- AIRK MCP tools were unavailable in this session; local source/tools were used.
  A successor should follow AGENTS.md's one-time discovery/status rule.
- No PR, merge, release, model upload or deployment was performed by this handoff.
  Do not mistake a prepared plan, private proposal, resource check or compiled
  binary for demonstrated quality or a usable installer.
