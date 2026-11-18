# AIRK final implementation plan

Planning date: 15 November 2026 (America/Denver).
Status: execution underway on feat/arbiter-final; D01–D08 recorded.
Scope update: deliver FP32 GPU execution first, then FP16 with efficient
attention, with both options available for version 1.9.0 and automatic CPU
fallback; see section 11. The user authorizes continuing beyond the original
72-hour checkpoint with T0 and per-run limits preserved.
The original planning request authorized this runbook only. The user authorized
implementation on 15 November 2026; public publication requires separate
authorization. Current execution evidence and blockers are in section 10.

This plan combines [knownissues.md](knownissues.md) and
[weights-v2-plan.md](weights-v2-plan.md). It orders product repairs, reliable
measurement, bounded model experiments and final delivery so a later execution
can proceed with explicit decisions and little reinterpretation. The research
rationale remains in weights-v2; the historical failures remain in knownissues.
Neither source is rewritten by this planning task.

Success is a working, tested product and an honest disposition for each selected
model. Training is not guaranteed to improve a model. A rejected adaptation
retains the original weights; those original optional weights still need
qualification. Missing qualification is a blocker, never a passing fallback.
All-three qualified Focus/Compact/Large profiles remain mandatory for the final
native publication catalog. A useful local candidate is a separate milestone.

## 1. Recorded authority and decisions

### User answers for this plan

The user answered D01–D06 during this planning conversation. These are recorded
planning decisions for later implementation, not claims that checks have passed.
D07 records the follow-up latency decision. D08 records the execution-time GPU
expansion and explicit automatic GPU/CPU selection requirement. Detailed new
contract encodings must be recorded before implementation; see section 11.

| ID | Decision | Answer/status | Dependent work |
| --- | --- | --- | --- |
| D01 | Delivery platforms and stopping point | APPROVED: Windows candidate first, then all supported-platform release gates; report genuine blockers. | Steps 15–18 |
| D02 | Training/evaluation compute, elapsed-time cap and teachers | APPROVED: “Just use this machine and the Neo. Max duration for this work is 48 hours. Target is 24.” Execution updates: the user first set 36 hours total, then explicitly approved 72 hours total on 17 November UTC for FP32 followed by FP16 with efficient attention. On 18 November the user authorized continuing past the 72-hour checkpoint. Original T0 and per-run limits apply. No paid/cloud compute or cloud teachers. Inventory: Threadripper PRO 5975WX, 32 physical cores, approximately 128 GiB RAM, NVIDIA RTX A6000; driver/free VRAM/training availability remain to verify. | Steps 7–14 |
| D03 | Human-reviewed labels for unattended work | APPROVED: use suitable existing human-reviewed labels; finish other work and block only unsupported promotion when labels are missing. Agent judgments are not human gold. | Data acceptance, terminals and FINAL |
| D04 | Shared-folder automatic agent-config editing | APPROVED: allow automatic edits only when the entire file and recovery path meet a reviewed safe policy; otherwise provide the complete manual entry. Token-free AIRK fields alone do not qualify a path. | Step 2 |
| D05 | Context terminal-class contract extension | APPROVED: include the extension; leave Sufficient, Clarify and NoJustifiedRead unavailable unless each class's evidence passes. | Steps 2, 11, 13 |
| D06 | Shipping read budget | APPROVED: qualify 8192 tokens / 120 seconds as the new default, including all client timeouts. The 2048/30 configuration remains a historical/control population. Focus stays a static 512-token pair unless separately re-exported. | Steps 2, 4, 7, 12–15 |
| D07 | p95 latency limit for the new profile | APPROVED: set the new profile's measured p95 qualification limit to 120 seconds and report actual component and end-to-end latency. Historical 30-second measurements remain historical. | New resource-policy freeze and qualification |
| D08 | Automatic local GPU execution and CPU fallback | APPROVED during execution: detect whether each user's GPU can safely run the selected model, use a qualified GPU variant when it can, otherwise automatically select a qualified fitting CPU variant for that same model. Report the device/reason; preserve consent, budgets and qualification gates. LATEST APPROVED: get FP32 working first, then implement FP16 with genuinely efficient attention; both options are part of the version 1.9.0 goal. Preserve selectable FP32, FP16 and Auto GPU precision with admitted CPU fallback. CUDA first on available NVIDIA hardware. DirectML/CoreML capability and applicable platform evidence follow section 11. Users can disable GPU use service-wide across all backends. Windows Radeon is a separate opt-in experiment with qualified same-model CPU fallback. | Additional Step 2 contracts; Steps 6–7, 12–18; G1–G4 |

### Settled rules; do not ask again

1. Keep bundled MiniLM, independent optional Focus (Qwen3-Reranker-0.6B),
   and optional Context Compact/Large (Qwen3-1.7B/4B). No replacement encoder
   family, cloud inference, new serving process or new crate is planned.
2. Use the logged-in user for every shell, build and test. Default tool execution
   was observed as a sandbox account during planning; the explicit logged-in
   execution path was verified as `tyai\tdotn`. Never use a sandbox-account
   baseline or change ACLs/global Git trust to obtain a pass.
3. Work on `feat/arbiter-final`, created from `feat/advanced-split` at the
   user's subsequent instruction. The opening local commit includes the pending
   plans, issue notes, ledger and synchronized agent instructions. Use the
   configured human identity, `git commit -s` and verify the DCO trailer.
   No push to this new branch, PR, merge, public upload, release publication or
   deployment is authorized. Later implementation still needs its task instruction.
4. Preserve unrelated work, source files, immutable published model releases and
   historical receipts. Current-version index regeneration is allowed when
   needed; it never authorizes deleting unrelated user data.
5. Keep AGENTS.md and CLAUDE.md byte-identical, including the logged-in-user
   rule. The historical intentional-divergence note is superseded by the current
   standing instruction. Reconcile them during implementation; do not weaken
   the repository equality check.
6. The completion gate is at least .75 completion and at most .02 completion
   regression against the selected strongest baseline, plus at least .10 mean
   external-turn reduction on paired common-success tasks. The ".02 gain"
   wording in knownissues is incorrect; the frozen protocols govern.
7. Historical 90% test acceptance and local-test profile bypasses are local
   candidate history. They do not waive final tests or qualify any model.
8. Use GitHub's existing approved immutable source map unless a later hosting
   decision authorizes another origin. No mirror, anonymous community export,
   fabricated download URL or overwrite of an existing release is implied.
   GitHub-only availability remains disclosed.
9. Keep the old advanced-split deadline as history. This plan cannot make a
   missed deadline current or use it to waive gates. The user superseded the original
   24-hour target/48-hour cap first with 36 hours total, then explicitly with
   72 hours total for the expanded FP32-first/FP16 goal on 17 November UTC.
   On 18 November the user authorized continuing past that checkpoint.
   T0, correctness requirements and frozen per-run limits still apply.
10. Public publication is a separate final action on concrete reviewed artifacts.
    Prepare artifacts, rights/source-map proofs and a readiness report first.
    Missing final upload authorization does not stop independent local work.

### Source identity and conflict resolution

The planning checkout was `76d84c533437c4934314e1b10b8319af34ebddb6`,
with live version 1.8.206. knownissues was already modified and weights-v2 was
already untracked; preserve both. The source SHA-256 values read for this plan:

| Source | SHA-256 |
| --- | --- |
| knownissues.md | `cab7328dd2f5d9c09f61b83f1fce078a36d2ae9bdd9ac6ee51dd0cf258cbe026` |
| weights-v2-plan.md | `283248d07beebbf6a289c3957c6ca460bab11d48caa907b3779aa792e44a9429` |

Use later user answers first, then recorded contract decisions, then this
runbook. Live code and original receipts establish implementation/evidence
status. A dated issue is a reproduction lead, not proof it still fails.
Do not restart completed work merely because an older handoff proposes it.

## 2. Execution rules and ownership

At each step, read its affected source, tests and owning CONTRACT.md. Keep
behavior changes distinguishable from mechanical moves. Record contract,
digest-input or persisted-format decisions before dependent implementation.
Rust owns contracts, generated client types, model views and semantic cases;
Python consumes native exports, and TypeScript renders native results.

Use the existing [execution ledger](../simplify/one-process-ledger.md).
Maintain a small step table there with status, exact input identity, owner,
receipt path, result, next action and blocker. Allowed statuses are
`not_started`, `in_progress`, `passed`, `retained_original`,
`unsupported`, `blocked` and `not_applicable`; the last four need a reason.
A model experiment can end retained_original while qualification remains blocked.

Private model/data outputs:
`artifacts/advanced-split/models/weights-v2/<experiment-id>/`.
Other run receipts:
`artifacts/v190/final-plan/<run-id>/`.
These are private scratch/evidence roots, never staged or packaged. Verified
distributable packages use separate fresh output directories.

Every receipt records exact command/arguments, source commit and dirty-input
hashes, consumed fixture/model/policy identities, environment/hardware, start/end
and elapsed time, selected/passed/failed/skipped/refused counts, output hashes,
diagnosis and cleanup/settlement. Cargo receipts also record crate/features,
target directory and incremental setting. Preserve original failed attempts.

| Owner | Files/outcome | Scheduling rule |
| --- | --- | --- |
| Primary/native integration | Ledger, contracts, shared Rust owners, runtime custody, final review and all Cargo processes | One Cargo owner per host/target; Windows and Mac Neo may build concurrently. Hand off each target only after its compiler descendants exit. |
| Transport/config | airkctl agent configuration and I2, open-read adapters, affected harnesses and real-transport tests | Contract decisions first; coordinate shared Core/Service edits with primary. |
| UI | Explorer/Admin Service, Agents and rail, generated consumers, behavior tests and Help | Can implement against frozen Rust fixtures while native work proceeds; do not handwrite DTOs/config builders. |
| Data/evaluation | Rights, splits, labels, native alignment, rubrics, evaluator and untouched FINAL reservation | Keep scoring/labels independent of model implementation; honor D03. |
| MiniLM | Existing encoder trainer/exporters, original controls, U8/S8 and regeneration evidence | No Cargo outside primary; one GPU-heavy experiment at a time unless measured safe concurrency is frozen. |
| Focus | Exact-input relevance training, prompt control, export and teacher validation | Prepare alongside MiniLM; teacher transfer waits for measured superiority. |
| Context | Compact/Large whole-card learning, replay, terminals and calibration inputs | Compact pilot first; Large preparation in parallel, qualification separately. |

The primary independently reviews the combined changes before expensive runs or
corrective pushes. Agents may prepare independent outcomes in parallel; assign
exact files before edits. Do not create an agent-orchestration product.

Cheap checks first. A failing filtered run selects at least one test and reports
the count. On expensive failure stop that batch, preserve diagnostics, reproduce
cheaply and correct the cause. A repeated failure requires a revised diagnosis
and cheap regression before the next attempt. Never retry unchanged failed
inputs, broaden limits, relax assertions, fabricate cleanup or tune on FINAL.

Continue independent work when a branch is blocked. At the 72-hour checkpoint,
retain results, identify remaining work and continue under the user's 18 November
authorization. Report observed outcomes against the release criteria.

## 3. Dependency order

| Step | Outcome | Must follow |
| --- | --- | --- |
| 1 | Reconciled inventory and runnable logged-in environment | Planning decisions recorded where needed |
| 2 | Native contract/fixture decisions frozen | 1, D04–D06 |
| 3 | Reliable U110 reads, setup and agent configuration | 2 |
| 4 | Answer deadline/settlement repair and native input census | 2; 3 for integrated transport reproduction |
| 5 | Task-oriented Service/Agents UI and usable navigation | 2; integrates with 3–4 |
| 6 | Full model acquisition/lifecycle preflight | 3; existing profiles can be diagnosed while 4–5 proceed |
| 7 | Corrected baseline and additive experiment freeze | 3–4, 6, D02/D06 |
| 8 | Audited software corpus and evaluation reservation | 7, D03 |
| 9 | MiniLM M1 pilot and native exports | 8 |
| 10 | Focus prompt/SFT pilot and native export | 8; may run before/alongside 9 within hardware limits |
| 11 | Context useful-read learning and conditional terminals | 8; initial frozen upstream baseline, then selected 9–10 output |
| 12 | Triggered refinements, exact export/resource freeze | 9–11; early precision diagnostics may start after 7 |
| 13 | Exact-artifact CAL/confirmation and composed custody evidence | 12 |
| 14 | Held-out goal evaluation and final model disposition | 13 |
| 15 | Regeneration/recovery, qualified trio and publication capture | 14 |
| 16 | Integrated workspace, transport and platform gates | Ordinary gates: reviewed 3–5 and identified current artifacts; model/catalog-specific gates additionally require 15 |
| 17 | Local packages and installed-product qualification | Applicable independent gates from 16; provisional Windows candidate does not wait for 15; final-series package requires 15 |
| 18 | Documentation, milestone commits and delivery report | All applicable prior steps; explicit blocker inventory |

## 4. Step-by-step runbook

### Step 1 — Reconcile the live starting point

1. Verify logged-in identity, branch, worktree changes and configured contributor.
   Save the starting manifest; do not overwrite the two source documents.
2. Reconcile AGENTS/CLAUDE using the standing rule, then run the repository check.
   Inventory the current version instead of copying stale 1.9.0 prose.
3. Verify the persistent Cargo target `target/`, pinned tools,
   static ORT inputs, and `AIRK_BUNDLED_MODEL_DIR` pointing to the verified
   `artifacts/advanced-split/bundled-model-v3` assets. Do not clean Windows
   targets or reuse the same target simultaneously across checkouts.
4. Inventory every knownissues failure and every relaxation row: current
   source/fixture, historical receipt, present reproduction status and owning
   step. Include the two private-boundary tests, all agent-config failures,
   setup/recovery, source watcher, measurements, singleton deadline, stale
   comparison pair, installer/mac placement, U110 and fixture drift.
5. Baseline only relevant failures against their original/unmodified source
   where attribution needs it, under the same logged-in account. Use isolated
   output identities and serialized Cargo ownership; avoid the prior worktree
   artifact-poisoning mistake. No unrelated rebuild merely to confirm a claim.

**Exit:** environment/source receipt, complete issue inventory, no missing user
decision hidden as an assumption. Failure families may still be open; schedule
their cheap reproductions in Steps 3–4 and their owning platform checks later.

### Step 2 — Freeze contracts before consumers

1. Record D04 in I2 and
   [agent-config CONTRACT.md](../../crates/airkctl/src/agents/config/CONTRACT.md).
   Existing replacement retains the entire displaced file, including unrelated
   server secrets and concurrent editor bytes. Tokenless AIRK fields or a secret
   scanner alone cannot establish safe retention. Preserve regular-file,
   identity, basename, size, no-clobber, concurrent-change, exclusion, durability
   and uncertain-outcome fences. No ACL mutation to make a path eligible.
2. Add Rust-owned manual configuration for the selected client/folder with
   `file`, `format` and exact `text`; define typed refusal reasons/details
   rather than parsing an English prefix. Freeze JSON/TOML escaping, endpoint,
   agent/scope and port-change cases. Preserve unrelated configuration bytes.
   Generate I2/I3/UI consumers and semantic cases through the existing pilot.
3. If D05 includes terminals, record the Core/qualification/final-promoter
   extension and independent refusal fixtures now. Define each whole-card
   class, supported/unsupported metadata, raw-margin acceptance and no
   runner-up fallback. Schema changes need explicit recorded decisions; enum
   presence alone does not authorize deployment.
4. Record D06 across Core read limits, compiler/profile widths, joint ledger,
   IPC, callbacks, transports and qualification policy. Inventory every
   [relaxations.md](relaxations.md) row; preserve original fixtures and create
   new native-emitted cases for any chosen changed contract. Separate default,
   structural ceiling, actual allocation and shipping profile.
5. Prompt, chunk recipe, terminal policy and calibration changes each receive
   their own identity/fixture decision before use. No silent change to
   `airk-state/1`, wire digests, vector compatibility or native calibration
   format. Keep the all-three final catalog requirement.

**Exit:** reviewed contract decisions and independent native fixtures; consumers
may proceed without waiting for a separate Git commit.


### Step 3 — Repair operational and transport failures

1. Reproduce the complete Step 1 failure inventory with focused tests and the
   current matched runtime. Distinguish code defects, private-parent refusals,
   stale binary comparisons, generated-fixture drift and environmental failures.
   The expected result for unsafe shared-parent editing is a typed refusal plus
   usable manual instructions, unless D04's full safety contract is proven.
2. Implement Rust manual-entry emission and typed refusal details from Step 2.
   Cover Claude Code, Codex and VS Code/GitHub Copilot in supported scopes:
   exact filename/text, JSON/TOML escaping, current port, unrelated servers,
   stale credential removal, ordinary safe writes and shared-folder fallback.
   Remove Admin's handwritten `claudeJson`/`codexToml` mirrors in favor of
   generated native outputs.
3. Repair U110's repeated terse-status elision and two-index
   `parse`/`capabilities` failure at their canonical owner. Compare complete
   native, pipe and HTTPS/MCP results; preserve `read_scope` for single-index
   responses and receipt-bound `selection.scope_identity` for multi-index
   bodies. Never inject a field into an already bound body.
4. Migrate the eleven named harnesses: `check-http-port-relocation.py`,
   `check-issuer-rotation.py`, `benchmark-service.py`,
   `benchmark-real-corpora.py`, `benchmark-mcp-investigation.py`,
   `check-retrieval-quality.py`, `check-usage-inputs.py`,
   `check-runtime-upgrade.py`, `check-service-install.py`,
   `check-container.py`, and `setup-local-test.py`.
   Use the tokenless endpoint and explicit index/set scope. Test workspace
   defaults separately, missing scope, and clients that never offer MCP roots.
5. Preserve U110's accepted local-process exposure, TLS trust and Host/Origin
   ingress protections. Do not silently reintroduce credentials. Test real
   Codex/Claude/VS Code connection behavior where an existing client is available.
6. Triage the retained canonical Answer mismatch; the two MCP callback/policy
   withdrawal failures reproduced on `98c368f4`; setup-path overlap; frame
   deadline failures; and `usage_empty` fixture drift. Regenerate fixtures
   only from reviewed Rust producers and compare complete expected semantics.
   Repair races with observers/clocks, not sleeps or retries.
7. After harness migration, separately inventory legacy credential operations
   and callers. Delete only code proven uncalled after clean compilation/Clippy
   or covered by a reviewed surviving-owner compatibility row. Preserve live
   predecessor discovery, shutdown and recovery duties; unresolved code stays
   labeled with evidence, not removed because its name says legacy.

**Checks:** scoped Rust dev-check/focused regressions, script unit tests and
affected real transports. Report exact selected counts and before/after failure
inventory. Keep the shared-folder refusal assertion where the policy requires it.

**Exit:** no known operational defect is allowed to confound the baseline.
Independent platform defects may remain scheduled with an explicit blocker.

### Step 4 — Repair Answer and implement the larger default end to end

1. Locate R154's actual call 0019 and its original inputs. If inaccessible,
   retain that fact and build a faithful focused reproduction from live owners;
   do not claim an exact replay. Trace retrieval, compiler, queue, metadata
   callback, each forward, additional read, packing and transport time.
2. Repair the failure class, including sibling callers, clipped remaining time,
   original-clock intersections, source/policy revocation and cancellation.
   Prove actual native settlement/drain; a response timeout or socket close is
   not proof of stopped work. Keep a cheap deterministic regression.
3. Apply D06: default 8192 shared input tokens and 120000 ms. Update the Rust
   default and each applicable validation, allocation, callback and transport
   owner from the relaxation inventory; regenerate all affected native
   fixtures, schemas, Help and clients. Include VS Code host, MCP SDK,
   CLI/session, pipe, HTTPS/Admin and Workbench transport deadlines.
4. Distinguish the 120-second operation deadline from client transport overhead.
   No transport may prematurely truncate a valid operation; any transport
   overhead must be explicit and must not grant fresh native work time.
   Retain unrelated TTL, OCR, installer and callback safety semantics unless
   their actual dependency requires a separately recorded change.
5. Preserve an explicit 2048/30 historical-control request rather than letting
   new defaults alter old experiment identities. Keep old qualification rows as
   dated evidence; define new 8192 profile resource/numerical shapes and D07
   latency rules before scoring. No old 30-second/2048 result is relabeled.
6. Measure the actual native tokenizer census for the compact Context prompt,
   using current task-policy bytes and complete retained cards. Preserve every
   read, excerpt, hypothesis, gap and reason. Measure missing decisive spans,
   identifier loss, overflow, prompt/padding charges and useful legal options.
7. Keep Focus pair width 512 and left padding unless an explicitly triggered
   wider-export experiment succeeds. Context's dynamic axes do not themselves
   prove 8192 resource fit. Shared budget charges include every Focus and
   Context input; there is no per-stage reset.

**Checks:** literal new default/upper-bound/refusal fixtures; full request/result
parity; no early client expiry; native cancellation and stale-authority
regressions; small actual end-to-end timing probes after review.

**Exit:** diagnosed Answer behavior, honest settlement, exact 8192/120 request
propagation and a native input/token/menu report. Poor fit leads to a separately
identified recipe repair, never hidden truncation or an invented model gain.

### Step 5 — Make Service, Agents and navigation usable

1. Present current Service/resource/Arbiter status on opening the view, with
   loading, failure, stale and retry states. Replace the unexplained Load-only
   view and two-level tab maze with clear user tasks and short descriptions.
2. Show Focus and Context independently: installed/download state, on/off state,
   selected Compact/Large model, index enrollment, fit/readiness and failure.
   Offer plain actions such as “Download and turn on Arbiter Focus,” showing
   model size, explicit consent and enrollment before mutations. Preserve
   separate stages, default-off behavior and real native state.
3. Remove internal session IDs and custody wording from routine screens. Keep
   useful diagnostics in Details. Show measured requirements with their profile
   and platform; label provisional requirements until measurements exist.
4. Agents always offers “Set up manually” with the Rust-provided target path,
   full entry and Copy button. On automatic refusal, open it, explain the typed
   reason plainly, and retain raw detail. Explain that a port change requires a
   fresh entry and point to Copy/`airkctl agents endpoint`.
5. Give all twelve rail destinations a distinct destination-keyed icon:
   Overview, Search, Indexes, Activity, Agents, Service, Code, Documents,
   Evidence, Review, Strategies and Help. Remove the Explore evidence group.
   Use a flat list with a divider; pin Help last. Collapsed mode is icons only,
   with tooltips/aria labels and active highlight. Match page titles to labels.
6. Use existing code-native icons/styles. Test icon uniqueness against an
   independently authored destination set, keyboard/screen-reader names,
   expanded/collapsed layout, initial status loading, typed refusal/manual
   copy, consent, stale replies, retry and independent stage operations.

**Checks:** extension compile/tests; Admin build/check/tests; generated asset
verification. Compare the full native manual entry with both rendered/copied
surfaces for every supported agent. Review the rendered UI at narrow/wide sizes.

**Exit:** usable tasks, complete native results and sealed assets. UI mocks alone
do not qualify transport or installed product behavior.

### Step 6 — Prove acquisition and model lifecycle before training

1. Use fresh owned test storage and explicit provisioning consent in the
   fixture. Exercise the actual app/service Provision path for all three
   selected models: download every part, verify size/hash, join, verify logical
   assets, load ONNX, check tokenizer/ABI and make a bounded native call.
2. Exercise unreachable source, interrupted part, corrupted part/whole asset,
   deadline/idle stall, cancellation, retry, Remove and verified whole-asset
   reuse. Preserve sanitized errors with no signed URLs or secrets.
   Current incomplete-asset retries restart rather than resume; retain partial
   bytes honestly and explain Remove. Do not claim resumable parts.
3. Preserve the existing size-dependent part deadline (at least 900 seconds or
   size at 256 KiB/s), 30-second idle bound and fixed redirect limit unless a
   separately measured source-map change justifies different part sizing.
4. Verify disabled/offline behavior, independent enrollment, decline consent,
   load failure/OOM recovery and unchanged ordinary MiniLM progress.
   Test the pre-1.9 local-test profile path and its hard refusal at 1.9+.
5. Retain existing public release assets unchanged. A new model/profile needs
   new immutable identity/tag and reviewed exact source map; stage new bytes
   locally until separately authorized publication. A locally staged download
   fixture proves lifecycle mechanics, not public availability of future URLs.

**Exit:** exact provision/load/lifecycle receipt per model. Failure blocks its
expensive training/qualification path until reproduced and corrected; other
product work continues. Record GitHub-only hosting as a retained limitation.

### Step 7 — Establish a corrected baseline and bounded experiment program

1. Freeze a matched runtime pair, source/dirty-input manifest, original MiniLM
   U8/S8 controls, all selected Qwen inputs, native recipes, policies, sources,
   GPU/CPU environment and exact index IDs. Use fresh experiment indexes.
2. Run a small service-client preflight on the new U110 and 8192/120 path.
   Compare exact dense search with DiskANN on identical vectors and queries;
   measure semantic-only, lexical-only and production hybrid separately.
   Preserve extraction/alignment refusals, fallback, unavailable/partial
   graphs and source identity in the original task denominator.
3. Re-score old development diagnostics only to attribute prior errors.
   Historical failed/partially consumed FINAL sets remain consumed and retain
   their evaluation-only restrictions. DiskANN improvements are infrastructure
   gains, not evidence that weights improved.
4. Obtain same-visible-input/menu oracle decisions and a separate full-evidence
   oracle. Record representation/proposal misses independently. If the menu
   cannot offer the needed evidence, fix the recipe with frozen weights before
   scaling Context learning.
5. Create an additive software-v2 experiment freeze, leaving the original
   A0/encoder/goal-baseline protocols unchanged. Bind D06/D07's new budget and
   resource policy; state how old gates map to new software support definitions.
   Preselect metrics, critical strata, negative masks, controls, sample/power
   rationale, seeds, caps and exact stop/selection rules before scores.
6. Profile inference/export-only resource probes on already authorized inputs;
   do not take gradient steps before Step 8's data acceptance. Draft M1, F1/F2
   and C1 cards with hypothesis, primary change, controls, optimizer proposal,
   metric, limits and diagnostic. Step 8 binds actual accepted data hashes and
   freezes executable cards before even a short gradient probe.
7. Freeze terminal risk/sample/coverage criteria before CAL/raw task scores:
   zero observed false-sufficient cases and one-sided 95% upper bound below 1%
   false accepted Sufficient decisions. Roughly 299 independent zero-error
   accepted cases is an elementary-binomial guide; use repository-cluster
   analysis and an adequate sample rationale. Insufficient existing human
   labels leaves the class unsupported; never lower this target after scores.
8. Use only this machine and Neo. No cloud teacher or paid compute. An existing
   local teacher may label TRAIN only after its identity, suitability and cost
   are recorded; teacher review is not human gold. Use one pilot seed and
   three finalist seeds. No giant Cartesian/hyperparameter sweep.

**Exit:** trustworthy baseline, failure taxonomy, token/menu census, weight-
addressable residuals, pre-score acceptance criteria and a draft experiment register. A non-neural failure
blocks only dependent neural experiments until its cheap regression passes.

### Step 8 — Freeze licensed, aligned software supervision

1. Target an initial 2,000 query-group pilot across at least 20 unrelated
   repository families; expand toward 5,000 only if the measured D02 budget
   permits. Any justified size change happens before scoring and preserves a
   meaningful stratum/power rationale. The 25,000–50,000 scale-up is conditional
   future work unless a learning curve and remaining cap support it.
2. Start with mutually assigned 40% code/tests, 35% docs/contracts/config,
   15% cross-file diagnostics and 10% general-document replay. Cover Rust,
   TS/JS, Python, Go, C/C++, shell/PowerShell, Markdown/config and supported
   technical-document extraction. Avoid single-repository/language dominance.
3. Split TRAIN, DEV, CAL, policy-confirmation and FINAL by repository/fork,
   issue/symbol family and chronology before synthesis, mining or teacher
   scoring. Deduplicate clones, copied docs, fixes and paraphrases; reserve
   unseen projects and task/language slices. FINAL's source closure is excluded
   even from unsupervised training and mining.
4. Pin per-source rights, revisions and notices; a benchmark wrapper's license
   does not grant rights to underlying files. Use the weights-v2 benchmark
   shortlist selectively, with exact benchmark identity and overlap audit.
   Hotpot remains evaluation-only. Do not use private workspace/customer/live
   requests for training without separate explicit consent.
5. Keep exact path/spans, original context, native-unit mapping, question origin,
   relevance grades, uncertainty, annotation authority and alternative coherent
   support sets. Adjudicate file-level labels to visible spans. Never leak a
   future patch, revealed filename or later documentation into a pre-fix task.
6. Ground synthesized TRAIN questions in real evidence and verify claims.
   Mine lexical/dense negatives; include wrong overload/platform/version,
   missing constraints and different-failure tests. Unjudged is unknown.
   Counterevidence can be positive; mask all alternative positives and clones.
   Reject invisible positives instead of teaching tiny prefixes unseen facts.
7. Apply D03 to every final critical-evidence/terminal label, teacher conflict
   and stratified TRAIN review. Reuse appropriate existing human judgments only
   when they cover the exact claim and visible evidence. Native span projection
   or agent annotation is not new human review. Prepare a clear missing-label
   packet; continue tooling/data/nondependent experiments, blocking unsupported
   training acceptance or promotion as the applicable gate requires.
8. Audit concentration, native alignment and refusal denominator; keep
   multi-instance weak parent bags a separate arm with equalized bag-size
   controls. Freeze sample sizes from repository-cluster variance before
   one-shot scoring; inadequate power yields no promotion.
9. Only after that acceptance, bind exact data/split/rights/native-visible-example
   hashes, starting checkpoints, seed/config, metric, controls and maximum
   steps/compute to executable M1/F1/F2/C1 cards. Freeze them before short bounded
   gradient probes; measure those probes and admit longer pilots only within
   their recorded caps and the remaining campaign budget.

**Exit:** rights/split/alignment/label audits, frozen executable experiment cards,
general-document regression panel,
frozen rubrics and untouched FINAL reservation. Never turn 20 evaluable tasks
out of 64 into an all-64 pass.


### Step 9 — Run MiniLM's smallest useful adaptation

1. M0 is the exact original publisher controls on the corrected native pipeline.
   Keep 256 WordPieces, 384 dimensions, tokenizer, masked-mean/L2 and batch-one
   deployed ABI fixed for the first weight experiments.
2. M1 uses multi-positive contrastive training with general replay; compare
   easy negatives against screened lexical+dense negatives on the same rows.
   Give each required evidence unit/need its own positive loss. Pool only
   genuinely interchangeable alternatives; one easy implementation hit must
   not satisfy the loss for a required test and contract.
3. Start AdamW with one epoch, learning rate `1e-5`, temperature .05 and
   effective contrastive batch 32. Test `5e-6` or `2e-5` sequentially only
   if the frozen diagnostic justifies it. Memory-permitted larger batches
   (up to 128) require explicit negative groups or verified cached contrastive
   learning; gradient accumulation alone does not enlarge negative pools.
4. Export promising checkpoints immediately to both U8 and S8. Compare reference
   and actual native exact/ANN/hybrid Recall@10, complete coherent evidence sets,
   nDCG/MRR, actual Focus-pool recall, semantic rescue, general-document and
   language/repository strata. Float-only improvement is insufficient.
5. If Step 7 found a chunk/title/path/identifier/fusion defect, test that recipe
   with unchanged weights separately, then use the original/adapted weights ×
   original/revised recipe 2×2 control. Record the new pipeline identity.
6. Select by the preregistered DEV rule and confirm finalists across three
   training seeds. If the critical strata/native exports/gates fail, retain
   originals for downstream work rather than blocking Focus/Context.

**Exit:** selected original or candidate MiniLM, exact U8/S8 identities and
attributed quality/cost evidence. Final promotion still waits for Step 14.

### Step 10 — Improve Focus on actual admitted pairs

1. F0 fixes publisher weights, current prompt and candidate pool. F1 changes
   only the software-evidence instruction, with native recipe/fixture identity.
   Select the instruction on DEV before attributing F2's weight effect.
2. F2 performs continued relevance SFT on exact escaped, left-padded 512-token
   native query/window pairs. Rust-refused overflows stay refusals; Python must
   not silently crop. Train response-position full-vocabulary yes/no loss,
   mask the prompt, preserve deployed no=2152/yes=9693 logit scoring.
3. Start LoRA rank16, learning rate `5e-5`, effective batch32, one epoch and
   the frozen replay fraction. A second epoch or one changed rank/rate is a
   separately bounded follow-up. Full tuning requires demonstrated adapter
   capacity limits. Merge the adapter before native ONNX comparison.
4. Compare original/adapted under the same instruction on a fixed pool, then on
   Step 9's selected encoder pool. Measure nDCG and necessary/complementary/
   exception evidence retention. Report original, admitted, scored and
   delivered counts and overflow; ranking cannot recover absent candidates.
5. Keep diversity/overlap controls separate. No learned relevance cutoff or
   probability interpretation is added merely because a score is normalized.

**Exit:** selected original or adapted Focus with valid export and useful-evidence
ordering. It becomes an M2 teacher only after independent DEV superiority.

### Step 11 — Train Context and implement conditional terminal support

1. C0 scores unchanged Compact and Large on the same native states, current
   prompt, complete cards and upstream snapshot. Begin C1 with Compact;
   prepare Large's data/export path concurrently and carry forward the winning
   recipe, with separate controls/results.
2. Preserve nonthinking chat, six-input graph, final-prefix A–H vocabulary-row
   readout and `scores[1,N]`. The current view contains at most four newest
   192-byte hit prefixes, five verified Read choices and eight total cards.
   Whole-card correctness includes action, gap, reason and every source-role/
   relevance hypothesis. It is not free-form answer generation.
3. C1 starts useful-Read LoRA with rank16, learning rate `5e-5`, dropout .05,
   effective batch32, one epoch and allowed-symbol decision loss. Distribute
   targets across equivalently useful complete cards. Attention projections
   first; add MLP or rank/rate variants only for measured underfit. At most
   three pilot epochs with frozen DEV early stopping.
4. Balance positions through separately emitted native fixture variants;
   never reorder a frozen production roster. Test alias/path/evidence-order
   sensitivity. Score the complete roster, choose its global winner/margin,
   then apply supported class calibration. An unsupported terminal winner
   abstains; never substitute a lower-scoring Read. No abstain token is added.
5. Keep complete-support, duplicate-only, ambiguous and no-useful-choice states.
   During read-only C1, terminal-needed examples are refusal/evaluation probes,
   not fabricated Read targets.
6. C2 executes eligible counterfactual reads from identical pre-action TRAIN
   states; measure required support/counterevidence gained, duplicate content
   and real token/read/time cost. Freeze utility weights on DEV. Limited
   two-step lookahead prevents labeling bridge reads useless. Future outcomes
   may label utility but never enter student inputs.
7. C3 gathers actual learner-visited TRAIN mistakes/recovery states, obtains
   valid labels and compares against equal-size/equal-compute additional
   expert-state SFT. This is offline replay, not runtime learning.
8. Implement D05's terminal applicability across Core/task/fitter/promoter with
   independent fixtures, even if real semantic labels remain unavailable.
   Sufficient requires all task-required visible support; Clarify requires
   genuine ambiguity; NoJustifiedRead requires a complete verified menu with
   no justified choice and cannot mean global unanswerability or timeout.
9. Train a separate terminal-bearing arm only with D03-compliant labels.
   Include missing constraints, stale APIs, conflicting tests, platform
   qualifications, duplicate choices and natural missing-evidence cases.
   Missing gold leaves each class unsupported and the runtime fail closed.
   Do not project QASPER answerability or generic CAL Read certificates into
   terminal human judgments.

**Exit:** independently measured Compact/Large useful-read behavior, bounded
trajectory coverage, missing-menu/input rates and per-class supported or
unsupported disposition. Include Focus-off cases. Terminal extension code
does not establish qualified terminal functionality.

### Step 12 — Run only justified refinements; freeze practical native artifacts

Use the following branch table mechanically. Each branch gets one registered,
capped attempt plus a corrective attempt only after changed inputs and cheap
diagnostic evidence. Skip branches that cannot finish before the reserved
qualification window; record the reason rather than treating them as passes.

| Branch | Required trigger | Comparison/stop rule |
| --- | --- | --- |
| M2 teacher distillation | Focus/teacher demonstrably better on relevant DEV pairs; same inferable content | Contrastive-only, score-difference/distribution distillation-only and small combined arm; no cross-model vector matching. |
| M3 / F4 refreshed negatives | Measured remaining ranking misses with valid labels | One TRAIN refresh versus equal extra training on old negatives; stop on more false negatives or lost evidence diversity. |
| M4 short denoising | Demonstrated residual code-representation gap | Equal compute on extra retrieval supervision; no broad pretraining by default. |
| F3 preference/soft-target objective | F2 residual and independently grounded teacher preference | One added objective; mask ties/conflicts, retain pointwise loss, compare equal compute. |
| C4 preferences/distillation | Useful-Read SFT leaves a defined decision residual | Same-state/menu preference or audited teacher distribution, SFT/reference regularization; no online GRPO/general agent search. |
| Large → Compact distillation | Large measurably stronger on the same visible state | Compare native-card distributions; no generated chain-of-thought supervision. |
| Wider Focus/changed Context evidence view | Input census shows consequential visibility loss | New recipe/export/profile identity and unchanged-weight control; fit the shared 8192/120 budget. |
| W4 or quantization-aware training | Reproduced failure of cheaper valid W8/mixed-precision approaches | Pinned native operator support and all original numerical/task gates; otherwise retain valid full precision. |

1. Separate reference→FP32 ONNX, FP32→quantized ONNX and native→task comparisons.
   Preserve tokenizer/prompt bytes, positions/masks, output rows and identities;
   include difficult real inputs and near-tied margins.
2. Diagnose Compact W8's historical .290778875 raw-logit drift against the .25
   limit with matched intermediate tensors. Test sensitive-layer exclusions or
   scaling before W4. Check the actual operator/provider before applying an
   ORT-issue setting. Argmax agreement does not waive numeric failure.
3. Preserve MiniLM's mixed-precision protections and test both U8/S8 ISA paths;
   distinguish quantization loss from ANN loss. Quantized training techniques
   do not automatically qualify a quantized ONNX deployment.
4. Measure complete resident combinations: download bytes, staging/disk peak,
   load peak, steady/peak process memory, warm/cold p50/p95, throughput, CPU
   fit/threads/provider, actual widths and MiniLM coexistence. Do not sum
   isolated RSS figures and present that as measured joint memory.
5. Retain old shapes and add native 8192/N8 boundary/numerical/resource evidence
   under the new frozen policy. Apply D07 to measured latency; report actual
   component and end-to-end costs, overflow/failure and low-memory refusal.
   Reservations and old CPU1/4/8/16 observations are not fresh measurements.
6. Recheck OOM/recovery, cold/warm lifetime, disabled/excluded indexes,
   independent stages, offline inference, cancellation/drain and authority
   changes with the actual final exports.
7. Freeze the selected original/adapted checkpoint, precision, graph/external
   data, tokenizer, compiler/task policy and resource fit for every component.
   Finalist seeds and DEV selection are finished before CAL/FINAL.
   Any later upstream/model/recipe change invalidates dependent calibration
   or task evidence; no post-hoc relabeling.

**Exit:** final artifact set with valid numerics and measurable fit, or explicit
unpromoted components. Compression failure keeps a valid larger artifact only
if it also meets actual resources; no automatic precision switch.

### Step 13 — Collect exact-artifact calibration and custody evidence

1. Review current selected-model qualification contracts and the private
   runbook anchors in Section 7. Port needed maintained driver code into the
   existing developer-tool ownership with cheap tests before running it.
   Do not execute old sandbox-owned packets, stale binaries or historical
   release/decision hashes. Missing private source is a blocker or a reason
   to reconstruct a tested harness from its owning contract.
2. Freeze a fresh CAL100 population, ownership wrapper and U110 scope bindings.
   The thirteen chunks are phase-00 through phase-11 with eight ordered
   questions each, then phase-12 with four. Preserve ordering and every
   natural ownership closure; a partial capture is not CAL100.
3. Historical chunk limits were 1800 seconds/phase, one fixture's absolute
   10800-second lifetime, 500 total probes and 30 seconds/read. D06 changes
   the read configuration to 120 seconds; it does not silently extend fixture
   custody or other protocol caps. Review/freeze a new compatible execution
   protocol before capture. If full capture cannot fit the retained custody
   caps, diagnose and record the needed protocol decision; do not refresh
   deadlines in-place or stitch expired evidence into a passing run.
4. CAL gathering uses Focus ON and no Context task forwards. Aggregate all
   thirteen original closures and independently establish reference
   certificates before looking at Context raw task scores.
5. Run two complete serial raw Context captures, one Compact and one Large,
   under current 8192 policies and bounded owned execution. Before either run,
   separately inventory its historical 3360-second whole-census bound; 100
   permitted 120-second calls do not fit that bound in the worst case. Record
   and review the new whole-census custody cap using measured projections,
   actual fixture lifetime, per-call semantics and remaining campaign time.
   This is an implementation protocol decision, not an automatic multiplication
   or a new user gate. Preserve original task denominators, all legal choices
   and source/recipe identities.
6. Fit raw-margin acceptance in the native owner on CAL, using the exact final
   export. The generic Read fit needs at least one certified positive and
   zero accepted UNKNOWN. No supporting cutoff means unsupported/no qualified
   profile. Terminal fitting uses the new reviewed class-specific path and
   actual human semantic gold; old reference certificates have none.
7. Confirm the fitted policy once on disjoint policy-confirmation data.
   Report accepted precision, risk/coverage, false sufficiency, unnecessary
   clarification, useful-read rejection and every invalid/unavailable outcome.
   Keep whole-roster winner/margin semantics. Temperature diagnostics remain
   offline unless separately added to the native calibration contract.
8. Evaluate the already frozen Step 7 terminal risk/sample/coverage criteria.
   Report bounds and effective independent coverage; do not change the rule
   after raw scores, fitting or confirmation. Missing human labels or inadequate
   sample support keeps that class unsupported.
9. Run final fitted resource and ten actual composed cases:
   `baseline_identity`, `focus_only_search`, `dual_compact`,
   `dual_large`, `noninterference`, `cancel_drain`, `source_revoke`,
   `remove_recover`, `stage_independence`, `normal_shutdown`.
   Include MiniLM progress, joint reservation/memory, natural exit, empty
   ownership jobs and settlement. `source_revoke` removes real membership;
   retiring the entire set is not an equivalent test.

**Exit:** final-artifact CAL/fit/confirmation/resource/composed receipts, complete
closure and supported-class inventory. A valid export or fitted metadata file
alone cannot pass this step.

### Step 14 — Run held-out goals once and select final dispositions

1. Freeze finalists, prompts, thresholds, corpus, rubrics, fixed local capable
   agent/scaffold, scope and total read/token/byte/deadline budgets. The local
   scaffold must be reproducible on D02's allowed machines; unavailable
   capability is reported, not replaced with an unapproved cloud model.
2. Preserve the required selected-Qwen A6 comparison with Focus enabled for all
   its arms: ordinary follow-up, deterministic packet, Compact and Large.
   Use the native-selected driver, not the legacy public four-model CLI. Its
   --split confirmation flag is a runner mode: bind the campaign to untouched
   v2 FINAL tasks, disjoint from policy-confirmation, rather than reusing the
   calibration-confirmation population under another name.
3. Add v2 attribution controls: original versus adapted MiniLM with optional
   stages off; lexical-only/lexical+Focus/original hybrid/hybrid+Focus; selected
   encoder with original/adapted Focus and Context off; each Context size with
   Focus off; original/adapted Compact and Large in the composed path.
   DEV reduces the matrix; no full Cartesian sweep.
4. Give the ordinary baseline legitimate follow-up reads and equal total
   budgets. Choose the strongest baseline using baseline results only:
   higher completion, then fewer mean external turns on each baseline's
   completed tasks, then deterministic packet on an exact tie.
5. Score coherent complete support alternatives, citations/source identity,
   critical evidence/counterevidence, unsupported claims, completion, visible
   turns, every internal canonical read, inference work and resource cost.
   Localize implementation/test/contract correctly; any fixed-agent repair
   subset supplements the independent rubric rather than replacing it.
6. Report paired query deltas, repository-macro/task/language strata,
   repository-cluster uncertainty, all-task failures/costs and paired
   common-success outcomes. An empty common-success set cannot pass.
7. Apply Section 6's frozen gates to each required profile/mode.
   If FINAL fails, retain the failure, keep original weights where applicable
   and do not retune/reselect on that FINAL. A later experiment needs a new
   hypothesis and fresh held-out reservation.

**Exit:** adapted/original/unsupported disposition per component with exact
evidence. Large needs measured benefit for its cost. If any required optional
profile cannot qualify, final catalog promotion is blocked even if another
component improves.


### Step 15 — Prove regeneration/recovery and produce the native catalog

1. A changed MiniLM encoder or semantic input recipe gets a new embedding/
   pipeline identity, invalidates incompatible caches and regenerates vectors.
   Never mix old document vectors with new query embeddings. Preserve lexical
   availability and disclose semantic readiness throughout.
2. Test interrupted regeneration, stale-graph refusal, resume and rollback.
   Rollback selects original weights with matching original vectors or rebuilds
   them; restoring only a model file is insufficient. Use experiment indexes
   first; preserve source bytes and unrelated user indexes.
3. Focus/Context-only changes leave semantic vectors unchanged but invalidate
   model/policy/calibration-dependent advice caches. Exercise independent
   enable/disable/enrollment, stale advice refusal and recovery.
4. Complete exact-byte rights/provenance for checkpoints, derivations, graphs,
   tokenizer and notices, plus actual target/fit proofs and approved immutable
   download-source maps. Retain per-part/logical-file hashes and new/old
   manifest comparisons. Do not invent URLs for unpublished bytes.
5. Use the native final promoter only with all three selected qualified profiles.
   Rebuild the compiled catalog, then emit genuine
   `fixtures/arbiter/publication_catalog.json`. Validate catalog/publication/
   offline projections against exact staged inputs. No handwritten capture,
   empty catalog bypass or partial trio.
6. The final series must reject `unqualified-local-test` profiles and require
   the native capture. Preserve the bounded local series only for an honestly
   labeled provisional candidate. New profiles/assets use new identities;
   existing releases are never edited. Remove old public assets only under
   separately authorized retirement, outside this plan's local work.

**Exit:** regeneration/recovery proof, qualified trio, compiled catalog and native
capture; or a precise promotion blocker. Local staging is not remote publication.

### Step 16 — Qualify the reviewed integrated candidate

1. Primary reviews the full combined patch, contracts, fixtures, generated
   assets, dependency boundaries and receipt identities. Freeze all consumed
   source/model/toolchain/runtime inputs before expensive gates. Run ordinary
   workspace/transport/platform checks on identified retained-original/local-test
   artifacts even if Step 15 is blocked. Only checks that require qualified
   models, the final catalog or publication capture wait for those inputs.
2. Run format, affected scoped Clippy/regressions and script/UI checks first.
   Then complete full workspace Clippy, all-feature tests, doctests, routine-
   excluded campaigns where Nextest is used, ignored vector-performance and
   final publication-input cases. No 90% pass threshold or ignored failure
   inventory counts as a full pass.
3. Exercise current [per-surface read parity](../read-interface-parity.md):
   CLI/session, MCP, Admin and Workbench through real canonical/native handlers
   and transports. Compare full inputs/results, copy/export, provenance,
   readiness, coverage, continuations, error/retry, omissions, scope identities,
   fallback and all work accounting. Document inapplicable operations.
4. Build/reuse an exact matched runtime pair and run service-client integration
   preflight; never compare against the old ORT 1.22.1/stale MiniLM binary.
   Reuse unchanged artifacts only with their original proven identities.
5. Qualify Windows first. Batch Mac Neo through existing SSH/POSIX sh, preserve
   receipts outside its target, and clean its target after every batch including
   failures. Keep single Cargo ownership. Check its actual architecture, disk,
   descriptor limits and memory; an 8 GiB host cannot demonstrate a larger
   resource fit merely because the program compiles.
6. Attempt Linux/container and other required platform gates only with existing
   suitable environments on the allowed Windows/Neo machines. No third host,
   paid runner or cloud CI is authorized by D02. Missing host/ISA/physical
   capacity or required hosted-release evidence is an explicit blocker;
   cross-compilation does not establish target execution.
7. On failure stop the batch, collect exact diagnostics and follow the cheap
   reproduction rule. Re-run only invalidated checks; unchanged passing
   evidence retains its original identity.

**Exit:** per-platform gate matrix, actual test counts, failed/skipped/unavailable
checks and complete release-blocker inventory. Windows success alone never
establishes cross-platform release readiness.

### Step 17 — Deliver Windows packages, then test installed behavior

1. For a useful early Windows milestone, package reviewed Steps 3–6 under the
   next unused local 1.8.x candidate version with unqualified models clearly
   labeled. This can proceed while model research/qualification is blocked.
   It does not require or imply a final 1.9.0 catalog.
2. Keep native/extension versions and lockfiles synchronized. Discover the next
   unused version from live state; do not hard-code historical proposed 1.8.207.
   Reserve 1.9.0 for final delivery after its applicable gates. Perform each
   accepted bump once per milestone, including generated version/Help assets.
3. Build/stage the exact matched runtime after static ORT/model provisioning
   and sealed Admin assets. Package only tested native artifacts and compiled
   universal VSIX. Use fresh outputs, verified notices/signatures and complete
   hashes/census. Optional models remain explicit separate downloads; never
   include training checkpoints, optimizer state, credentials or source corpora.
4. Use isolated installed-product fixtures under the logged-in user. Exercise
   install, upgrade, interrupted setup/recovery, repair, silent mode, uninstall,
   real service process ownership, sign-in behavior, Explorer and agent setup.
   Use safe automatic edits or manual entries according to D04. Preserve the
   user's actual installation/configuration unless a later instruction
   explicitly authorizes changing it.
5. Exercise actual model Provision/import/load/enable/disable/enrollment/Remove
   from the built product and normal MiniLM-only operation. For new unpublished
   assets, use approved local staging/import tests and clearly mark actual
   public-source availability as pending; never call this a successful public
   download. Verify full published downloads only after separately authorized
   immutable publication.
6. Recheck the installed 8192/120 timeout chain, resource display, status-first
   controls, collapsed rail, manual Copy/config equivalence and recovery. Source
   read-only behavior and complete process settlement are acceptance criteria.
7. Collect installer/VSIX/native-package paths, hashes, signatures, version,
   source/model identities and exact installed-journey receipts.

**Exit:** concrete Windows candidate(s) and tested/unavailable installed behavior.
A packaged file without installed checks is a candidate, not a qualified release.

### Step 18 — Reconcile documentation and hand off exact outcomes

1. Update current Help/operator documentation with model roles/stages, consent,
   independent enrollment, download/staging/RAM requirements, measured latency,
   8192/120 defaults, offline behavior, limitations and recovery.
2. Correct current README/documentation index/release notes/Marketplace/VSIX
   claims and version references. Keep historical receipts recognizable;
   explicitly retire the .02-gain wording, sandbox-user advice and obsolete
   known-issue statements only after their fixes/evidence exist.
3. Update the existing ledger and this plan's outcome table. Summarize each
   original known issue as fixed with evidence, retained by decision,
   unsupported, unavailable or still blocked. Link the exact receipts.
4. Review and commit coherent milestones only on the authorized branch/remote
   with the human DCO identity. Preserve the user's unrelated starting changes;
   never stage private data or broad artifact directories. No PR.
5. Deliver Windows artifacts first, then the platform/qualification report.
   Distinguish implementation, measured model gains, rejected experiments,
   compilation, installed outcomes and public availability. At the 72-hour
   checkpoint, record remaining blockers and continue under the user's
   18 November authorization.
6. Only after concrete artifacts and all required release gates are ready may
   public publication be proposed for separate approval. Do not request a
   blanket release approval during this planning task.

## 5. Time budget and unattended scheduling

The original planning allocation was 24 hours with a 48-hour maximum, followed
by a user-approved 36-hour total deadline. On 17 November UTC, the user approves
**72 hours total** for FP32 GPU execution first and then FP16 with efficient
attention, both available for version 1.9.0. Use the same Windows machine and
Mac Neo, with quality prioritized. Original T0 remains
2026-11-16T05:29:42.2081757Z. The 72-hour checkpoint is
**2026-11-19T05:29:42.2081757Z (Wednesday, 18 November at 10:29:42 p.m.
America/Denver)**. On 18 November the user authorized continuing beyond it.
Keep the original campaign clock, acceptance rules and per-run limits.
The early rows below remain historical scheduling targets, not completed outcomes.

| Elapsed target | Main outcome | Parallel independent work |
| --- | --- | --- |
| 0–4h | Steps 1–4 contracts, focused failures and timeout repair | UI Steps 5, source/rights inventory, download preflight |
| 4–6h | Steps 6–8 baseline/input census and audited pilot freeze | UI completion; Windows candidate preparation if reviewed |
| 6–12h | First M1, F1/F2, C1 pilots with early exports | Label audits, refusal/recovery tests; Large export preparation |
| 12–15h | Finalist seeds and exact artifact/resource freeze | Windows candidate checks and maintained qualification harnesses |
| 15–20h | CAL, fit, composed and held-out runs that fit their frozen caps | Documentation and available noncompeting platform preparation |
| 20–24h | Windows delivery, integrated gates and status at target | Report remaining qualification blockers and justified extension work |
| 24–36h, updated | Native FP32 GPU integration, actual model/placement/Auto-fallback proof and grouped technical gates | Complete independent CPU/platform repairs and candidate checks |
| 36–48h, updated | FP16 graph/initializer path and genuinely efficient attention after FP32 works | Static mask/equivalence controls and generated device/precision consumers |
| 48–60h, updated | Separate precision numerical/resource/recovery and composed gates; CAL/task work only with admitted inputs | Measured joint residency and applicable hardware/provider checks |
| 60–68h, updated | Integrated workspace/real transports, signed packages and applicable installed/platform gates | Review current source, rights/notices and exact artifacts |
| 68–72h, updated | Final justified checks, settlement and review handoff | Reserve time to report every unmet criterion honestly |

These are scheduling targets, not claims that a six-hour CAL allowance or human
labeling fits automatically. Before each long run, estimate measured duration
plus recovery/settlement and confirm it fits both its own protocol and remaining
campaign time. Review the entire inventory at least every four hours. Every
two hours ask: am I completing final-plan.md, or getting sidetracked? Report
actual progress against the plan and the next critical outcomes to the user.

At hour 12, stop optional exploration if it threatens core export/qualification.
At hour 24 and beyond the 72-hour checkpoint, continue toward unfulfilled
acceptance criteria under the user's 18 November authorization. Finish FP32,
then advance to the separately qualified FP16 path.
Missing human labels, unavailable hosts or a failing profile cause a local
blocker and continued independent work, not a weaker claim.

## 6. Frozen acceptance rules

Bind these to the additive v2 freeze before scoring. The original protocols are
[A0](../../tools/retrieval-train/a0-freeze.json),
[encoder controls](../../tools/retrieval-train/encoder-baseline-freeze.json) and
[baseline selection](../../tools/retrieval-train/goal-baseline-selection-freeze.json).
The old budget/resource rows remain historical; D06/D07 explicitly amend the new
profile's policy only.

| Boundary | Required result |
| --- | --- |
| MiniLM | Recall@10 >= .85; coherent complete support alternative >= .70; >= .03 absolute paired complete-recall improvement against each original unsigned control; mean-recall regression <= .02. Both U8/S8 native exports, critical strata and actual delivered path pass. |
| Source identity | Citation/source precision 1.0; no invented path/span, hidden omission or mixed embedding identity. |
| Optional quality | Required evidence/completion floor >= .75 where applicable; completion no more than .02 below the strongest baseline; no citation/critical-evidence regression. Independent Compact/Large results and preserved Focus-only/Context-only modes. |
| Follow-up effort | >=10% mean external-turn reduction on nonempty paired common-success tasks; also disclose all-task failures, internal reads, compute and resources. |
| Generic Context fit | >=1 certified positive; zero accepted UNKNOWN; exact whole-roster winner and margin before class acceptance. |
| Terminals | Class-specific human-backed evidence, zero observed false sufficiency and frozen risk/coverage criteria; unsupported stays unavailable. Generic Read success is not terminal qualification. |
| Approximate export | Raw-logit drift <= .25; normalized-mass diagnostic drift <= .05; identical fixture argmax under the frozen rule. New real near-boundary cases supplement these gates. |
| New default | Shared 8192 input tokens / 120000 ms; compatible actual graph/profile; complete client timeout propagation; no work-budget reset. Focus remains 512 per admitted pair. |
| New latency | Measured p95 <= 120 seconds for the new profile; report actual component and end-to-end cold/warm latency and failure costs. Preserve the frozen sample/quantile protocol and original 30-second evidence separately. |
| Resources | Measured actual CPU/ISA/provider/threads, memory/disk/load peaks and resident coexistence at shipped inputs; no unsupported transfer from old reservations or hosts. D08 additionally requires measured device memory/placement and automatic qualified CPU fallback; section 11. |
| Catalog/package | Exactly three qualified selected optional profiles, native emitted catalog/capture, complete rights/source maps, final-series local-test refusal and verified payload census. |
| Release claims | All applicable workspace, transport, platform, installed and artifact gates actually pass. Missing platform/public availability remains explicit. |

A proposed statistical target is frozen before scoring; it cannot be weakened
after results. Publication of new weights remains separate from scientific and
local artifact readiness.

## 7. Verified commands and qualification entrypoints

Use the pinned repository tools, current live selectors and owning contracts.
Replace placeholders in a retained run receipt before starting; never execute
the literal placeholders. List/inspect a focused selector and require nonzero
selection. Do not run cargo check before equivalent Clippy.

| Purpose | Command/owner |
| --- | --- |
| Format | `cargo fmt --all -- --check` |
| Scoped Rust | `python scripts/dev-check.py -p <crate>` with the actual needed features/targets |
| Focused tests | `cargo nextest run --profile ci -p <crate> --locked --timings -E '<verified expression>'` |
| Final workspace | `cargo clippy --workspace --all-targets --all-features --locked --timings -- -D warnings`, then `cargo test --workspace --all-features --locked --timings` |
| Doctests | `cargo test --doc --workspace --locked --timings` |
| Complete Nextest alternative where selected | Add `--ignore-default-filter` to restore routine-excluded campaigns; run doctests separately; do not duplicate a completed equivalent full run without cause. |
| Extension | `npm run compile --prefix packages/vscode`; `npm test --prefix packages/vscode` |
| Admin | `npm run build --prefix packages/admin`; `npm run check --prefix packages/admin`; `npm test --prefix packages/admin` |
| Matched pair | `python scripts/build-service-runtime.py --destination <pair-dir> --profile debug` after provisioned inputs/sealed assets |
| Preflight | `python scripts/check-service-clients.py --runtime-dir <pair-dir> --files 16 --model --report <receipt.json>` |
| Version agreement | `python scripts/release-version.py check --expected <chosen-version>` |
| Native package | `python scripts/package-native.py --target <native-target> --runtime-dir <tested-pair-dir> --output artifacts/<fresh-output>` |
| VSIX package | `python scripts/package-extension.py --channel stable --output artifacts/<fresh-output>` |
| Repository | `pwsh -NoProfile -File scripts/check-repo.ps1`; `git diff --check` |

Use persistent `target/` and Windows incremental on; preserve
Cargo timings. Read [CONTRIBUTING](../../CONTRIBUTING.md) and
[CI/build instructions](../ci-and-builds.md) for provisioning, vector-performance,
signing and installed commands. Select real platform/features from live source.

### Maintained harness owners and required operational transitions

Use the [maintained Section 7 source mapping](../../scripts/arbiter/evaluation/research_history/SECTION7.md)
for the exact historical CAL, certificate, raw Context, selected A6 and composed
source versions. The original artifact paths are provenance references only:
never execute or import ignored historical implementations for a remaining run.
The source-rescue checkpoint does not make archived programs current adapters.

| Operation | Maintained owner and remaining prerequisite |
| --- | --- |
| CAL chunks and aggregation | `scripts/arbiter/evaluation/research_history/selected_native_goal/` preserves the mapped packet/controller/capture/aggregation sources. Implement reviewed operational owners under `scripts/arbiter/evaluation/selected_goal/`; keep the capture child inside its owned controller and bind all thirteen phase-execution/hash pairs. |
| Independent certificate and raw Context | The same mapped historical family preserves the exact preparation and owned execution sources. A current adapter must bind capture/corpus/rubric/source/task-policy inputs and separately reviewed Compact/Large profiles. Inspect the live `inference_owner::context_action_tests::selected::selected_context_cal_complete_readout_raw_scores` selector before use. |
| Generic native fit | Inspect `inference_owner::context_action_calibration_tests::selected::fit_selected_context_generic_read_calibration` and its current `AIRK_CONTEXT_ACTION_FIT_INPUT`, `_INPUT_SHA256`, `_OUTPUT`, `AIRK_SELECTED_CONTEXT_PROFILES_SHA256` and `AIRK_SELECTED_CONTEXT_CORPUS_SHA256` inputs. Terminal support still needs its reviewed native path. |
| Selected A6 | Adapt the mapped selected **confirmation** `run_goal_condition` and its complete helper closure under `scripts/arbiter/evaluation/selected_goal/`. `tools/retrieval-train/run_goal_condition.py` has a distinct development/final interface and is not interchangeable. |
| Composed | `scripts/arbiter/evaluation/research_history/composed_service_r1/` and `composed_orchestration/` preserve distinct historical/current-policy source versions. Implement `scripts/arbiter/evaluation/composed_service/` against the current Rust-emitted plan, SDK, native images and maintained process observer; review the entire selected helper closure. |
| GPU diagnostic and incremental core | Current entrypoints are `scripts/cuda-component-execution.py`, `scripts/cuda-static-incremental.py` and `scripts/cuda-static-checkpointed.py`, with their owning contracts. The committed collector patch is `scripts/cuda-static-runtime/final-graph-metadata/collector.patch`. Current graph integration and an executable, reviewed input recipe remain prerequisites; these paths alone authorize no run. |
| FP16 preparation/execution | `scripts/arbiter-fp16-execution.py` owns bounded preparation and execution. Its genuine FP32-success and reviewed native G5 prerequisites must pass; placeholders or renamed historical receipts cannot satisfy them. |
| Native final promotion | Inspect `service_runtime::arbiter::final_distribution::tests::emit_final_selected_distribution_from_recorded_proofs` and current `AIRK_FINAL_DISTRIBUTION_INPUT`, `_INPUT_SHA256`, `_OUTPUT` inputs; require actual approved proofs/source maps/rights and all three profiles. |

The proposed `selected_goal/` and `composed_service/` operational directories
are not implemented at this checkpoint. Before each remaining run, finish and
review the needed adapter in maintained source, pin its current source epoch and
every consumed helper, native image, model/profile, SDK and private input, and
emit a fresh plan/packet through the owning producer. Rescue maps recover exact
historical bodies; they do not establish API compatibility, private-input
availability or a current executable seal. Never repair a seal by retagging an
old receipt or substituting a same-named helper from another experiment.

Do not reuse historical packet deadlines, fixture lifetimes, approvals, selected
IDs or hashes. Reconcile historical composed 30-second request/event caps through
the approved 120-second native/client contract before execution; simply changing
a Python constant does not qualify the path. Check maintained tool availability
under the logged-in user and retain the full exact command and source closure in
each new receipt. Rust emits native profiles, schema/semantic cases and publication
projections; no Python protocol mirror. Historical receipts and private research
outputs stay preserved under their original identities.

## 8. Coverage of both input documents

| Input requirement | Implementing steps |
| --- | --- |
| knownissues 1, 2a: unqualified models/local-test bypass | 6–15; final-series refusal in 15–17 |
| knownissues 2: full model acquisition/no backup | 6, 12, 15, 17; GitHub-only limitation retained by decision |
| knownissues 3: missing publication capture | 15–17 |
| knownissues 4, relaxations: larger limits and stale resource evidence | 2, 4, 7, 12–16; D06/D07 |
| knownissues 5: R154 Answer timeout | 4, 7, 13 |
| knownissues 6: prompt fit/task identity | 4, 7–8, 11–13 |
| knownissues 7: failed/incomplete tests, stale comparison pair | 1, 3–4, 16–17 |
| knownissues 8: unavailable platforms | 16–18, D01/D02 |
| knownissues 9: Service/Arbiter UX | 5, 17–18 |
| knownissues 10: rail/icons/grouping | 5, 17 |
| knownissues 11: tokenless scope, roots, legacy code, harnesses, parity/fixtures | 2–3, 16 |
| knownissues 12: shared-parent config refusal/manual fallback | 2–3, 5, 17; D04 |
| knownissues replacement rules/operational notes | 1–2, 6, 15–18 |
| weights-v2 Stage 0 | 3–4, 6–7 |
| weights-v2 Stage 1 | 7–8 |
| weights-v2 Stages 2/3 | 9–10, triggered branches in 12 |
| weights-v2 Stages 4/5 | 11–13; D03/D05 |
| weights-v2 Stage 6 | Early export probes in 9–11; exact artifact/resources in 12 |
| weights-v2 Stages 7/8 | 13–18 |
| weights-v2 research/benchmark provenance | Retained rationale in source plan; exact rights/source/split audit in 8 |
| weights-v2 stopping, ownership, numeric gates | Sections 2, 5–7 and every step's exit |

## 9. Planning validation and execution handoff

This document records the user's seven decisions and the current implementation
order. Source review, contract/gate checks and independent read-only reviews
support the plan; historical test/model results were not rerun here. No model
training, runtime implementation, installation or qualification is performed by
this planning task.

Planning validation: all relative Markdown links resolve; the new file's
whitespace and git diff --check pass after final review. The repository-wide
scripts/check-repo.ps1 check stops at the pre-existing AGENTS.md/CLAUDE.md byte
mismatch; no later repository-wide check is claimed. The planning outcome is
recorded in the existing execution ledger. Only this plan and that ledger entry
are written by this task; both input documents and agent instruction files are
preserved. No builds or runtime tests are appropriate to this documentation-only
change. Do not convert planned checks into claimed passes.

### Branch and agent-instruction handoff

After planning, the user requested feat/arbiter-final from the current
feat/advanced-split branch and an opening commit of all pending files. AGENTS.md
and CLAUDE.md now contain identical mandatory execution-identity rules for
Codex and Claude Code: use the verified logged-in user for shells, edits,
children, temporary/cache/build outputs, delegated work and remote batches.
An unverified or foreign identity blocks the operation before file creation;
no sandbox-account, ACL/ownership or global Git-trust workaround is permitted.
This handoff updates branch/instruction state without starting implementation.

## 10. Current execution outcomes

Execution started 15 November 2026 at 22:29:42 America/Denver
(2026-11-16T05:29:42.2081757Z). The user now sets a 72-hour total deadline from
original T0: 18 November2026 at10:29:42 p.m. America/Denver
(19 November05:29:42 UTC). The earlier36-hour deadline is superseded. The
[existing ledger](../simplify/one-process-ledger.md) retains exact commands,
source/artifact identities, failed attempts, test counts and private receipt
paths. No source-only checkpoint grants missing runtime qualification.

### Source checkpoint and remaining execution work — 17 November 2026

The reviewed broad source rescue is committed locally as
`5c238473f7921c5dd295a9ab0daa9bcd56c16641`; no push or publication follows.
The [source-rescue catalogue](source-rescue.md) links maintained owners and
historical families with their receiving READMEs. Exact maps/inverse proofs and
the staged-byte audit support this committed scope. Neither that scope nor the finite 830-document review proves that all
artifact contents have been inventoried, rescued or safely removed.

Aggregate reconciliation/performance work remains open: R1 was interrupted after
repeated immutable Git lookups, and R2 reached its original 900-second deadline
before candidate workers. Retain both outcomes. Continue with reviewed phase
diagnostics and parallel verification of independent inputs, not an unchanged
retry, broader timeout or invented aggregate pass. Cleanup also remains open:
no originals were deleted at the source checkpoint. Default preservation tests
must transition to committed inverse/hash/length verification, with original
custody established separately by present originals or bound pre-removal/removal
proofs. Historical live helper dependencies, mixed evidence containers, private
inputs and retained research outputs are not disposable build caches.

GPU source preservation is not GPU qualification. The final-graph collector patch
and Rust diagnostic/provenance source are committed, but the new C++ collector
has not been compiled into a fresh core. The
`scripts/cuda-component-execution/graph_provenance.py` draft remains unwired and
excluded from executable authority. The earlier native source group
`1a271ce38dcbf874bda463bdf737b3575b4dfd5cdda5dae472bb804ff109fcb2`
passed scoped Retrieval/Inference Clippy and 161 focused regressions using the
old static core. Those checks establish compilation/synthetic controls for that
source, not the new collector, actual final-graph capture, inference or GPU
qualification. The prior FP32 component refused CPU-assigned nodes and did not
produce passing inference rows; efficient FP16 and its native G5 transition
remain dependent work.

Next: complete maintained current adapters/helper seals from section 7 and the
reviewed graph provenance integration; freeze a genuine fresh incremental-core
recipe and its producer identity, build/relink only after combined review, then
attempt bounded graph diagnosis and FP32 execution under the original gates.
New static core, native manifest, profile and image identities must remain
separate from the old qualified or unqualified artifacts. Do not silently admit
CPU nodes, promote a model or mark a plan step complete to bypass missing proof.
The shared 8192-token/120-second requirement, Focus's 512-token graph limit,
D01–D08, original T0 and current 72-hour deadline remain unchanged.

### Prior execution snapshot — historical evidence, not current source qualification

The following detailed snapshot predates the maintained-source rescue. Its
results and blockers retain their original dates/source identities; its pending
claims, approval blockers and scheduled update times are not a current execution
instruction. The checkpoint above and latest ledger entries govern remaining
work. No row below qualifies the new collector/core or marks the rescue, cleanup
or final delivery complete.

The delivered signed Windows R3 candidate is the c547e654 build (C7 a8d7983b/1941).
Model results retain their native 198f and 5d37 source/image identities and original
build ancestry. Native c9fe, committed in 059a2a89, now has full-workspace
Clippy, 4,825/4,825 restored-filter regressions and three separately provisioned
vector controls passing. Doctests compile nineteen suites with zero cases.
Fresh e306 R154 R8 reaches full753-source readiness and delivers one canonical
packet in62.964776s with original HANDLE exit0/lease release. Empty unsupported
paragraphs and UNVERIFIED source checks remain; this is delivery evidence,
not Answer quality or p95. Subsequent Mac alias and Core/Store foundations have
separate scoped results. Device selection/GPU qualification remain incomplete
(section 11).
Earlier R2 and later diagnostics retain their identities; no artifact is retagged as another.

| Step | Status | Observed outcome and remaining work |
| --- | --- | --- |
| 1 | passed | Clean starting source/branch/contributor recorded; human Windows identity verified; existing human-owned warm target audited; issue/input/platform inventory retained. Foreign and unreadable inputs preserved. |
| 2 | in_progress | Original D04–D06 CPU/manual/terminal/budget contracts and Rust fixtures pass their recorded controls. D08 Core/Store device-metadata foundations are applied; scoped Clippy, native generation2/2 and repaired166/166 regressions pass. Forty-nine device cases and native/editor generated types are adopted with qualification=false. The77-file higher-consumer change is applied and eleven-crate all-target/all-feature Clippy passes33.958642s; grouped native regressions now pass105/105. G3 actor and G4 scoped diagnostics now pass186/186 grouped native regressions, both client typechecks and sealed assets; device lifetime/selection/revocation and per-scope observation source is committed18cf7f0e. Actual GPU qualification remains incomplete. |
| 3 | in_progress | Rust-owned manual configuration, whole-file shared-parent refusal, eleven tokenless harness migrations and per-session status repair are implemented. Current c547 genesis-only bootstrap repair preserves authority refusals. Signed R3 preflight passes144.210s; the subsequent matched e306 Windows R5 pair builds and passes its16-file/model preflight126.466879s. These identified candidates retain separate source and process proofs; earlier failed signed reads remain retained. |
| 4 | in_progress | Native8192/120 defaults and125-second delivery remain. Reviewed D06 removes55/60-second connection-admission clamps from the accepted parent without extending TTLs or weakening authority; scoped Clippy/106 regressions pass. Fresh rebuilt e306 R8 reaches full753-source readiness and delivers one canonical packet in62.964776s. Seven NN calls/29 finite scores and original service HANDLE/lease settle. Paragraphs=[], unsupported, stopbudget and unassessed/basis_omitted remain. Native verification is UNVERIFIED0/18 under the existing64KiB frame/3283-byte receipt allowance. This is delivery evidence, not useful Answer quality, p95, CAL or promotion; original R154 is not replayed. |
| 5 | passed | Task-oriented Service/Agents, canonical manual Copy, twelve-icon navigation and the compact Workbench composer are implemented. Signed R3 Workbench retains sixteen full comparisons/two Answer routes; Admin retains eight full comparisons and wide/narrow navigation/title/icon checks. Both native processes have Stop/original HANDLE exit 0 proof. Real Code 1.146.0 activates the production R3 VSIX installed in a private profile and passes Explorer singleton/close-reopen/same-version CSS handoff in 8.109072s. Code parent exits 0 and a later CIM snapshot finds no Code using that private profile; no all-descendant HANDLE proof or native I4 claim follows. Historical observation gaps remain uncounted. Current device-UI source checks additionally pass110/110 tests and Admin/editor typechecks; these do not retag signed R3 artifacts or qualify native device behavior. |
| 6 | in_progress | The original acquisition failure remains failed; the independent 23-asset census and repaired Large finalization/cached verification passes are retained. Public Focus R7 now passes its complete isolated U93 journey in 82.614s: Provision/Import, scoring, removal, explicit restart, A/B MiniLM reads, two actual Stops and original HANDLE exit 0. Both native and owned-runtime receipts pass. Earlier R4/R5 failures remain historical. This proves the exercised public Focus lifecycle, not modern I4, all-three public Context availability, task quality, CAL or release qualification. |
| 7 | in_progress | The earlier identified matched-runtime baseline captures 27/27 lexical/vector/hybrid results across nine original queries with actual Stop/PID exit. Eighteen vector/hybrid verification receipts remain partial under the original source budget. Exact/ANN same-vector diagnostics accept the current canonical singleton without weakening scope/source fences; scoped Clippy, six ANN and five human-visibility controls pass. Genuine native pins and the nine-query comparison retain exactly nine uncached forwards, exact/ANN order agreement on all ten live vectors, natural exit/empty Job and original 120s/8GiB bounds. Population equals k10; no quality or scale improvement is claimed. Later optimized CoSQA visibility has its own offline source/image identity, not Debug product p95 evidence. |
| 8 | in_progress | Public human labels are acquired and audited: CoSQA/CodeSearchNet joins 1,955 judged pairs across 38 repository families; ContextBench retains 150 source-complete multifile TRAIN cases; original TechQA provenance leaves 290 positive pairs/254 documents, only fifteen complete original menus. After two failed Debug censuses, the optimized full 1,955-row census passes 34.137539s under unchanged 120s/4GiB bounds, retaining 16,386 native units across 316 files. Only 21 strict whole-input MiniLM matches (five functions/four repositories) pass visibility; 1,934 stay unknown. Focus fits 1,953 original full-function pairs with two positive overflows; within the supported private rights scope, 1,867 pairs fit (932 positive/935 negative, 37 repositories/437 functions). Eighty-six unknown-rights rows remain unknown. These are visibility/fit observations, not semantic training admission. A new exact existing-byte freeze closes35 family/clone components, native512 binding and historical-exposure quarantine: TRAIN400/10 families, DEV85/3, CAL55/2, confirmation139/2, research939/15, quarantine249/3. Seventeen literal and sixteen real-data checks pass. This supports a narrow exploratory binary-relevance pilot; DEV3 clusters cannot establish two-sided .05 superiority. Broader native/replay/label acceptance remains open; unknown pairs are not negatives. |
| 9 | blocked | M1 gradients depend on Step8 acceptance. Original MiniLM weights remain in use; no adaptation or measured gain is claimed. |
| 10 | in_progress | Original Focus full512 numerical/resource component gates pass, p95 2.116398s below4GiB. Existing1867 human exact question/function binary pairs support that relevance claim independently of21-row MiniLM visibility or missing terminal labels. Existing-byte native/split/clone/rights/power/card work is now frozen for a narrow binary target; missing binary human labels are not its blocker. F1/F2 native compilation/census and the owned full-vocabulary LoRA worker are implemented;28 pure,4 actual toy CPU-gradient and13 native Focus controls pass. The parent final-hash clock correction is reviewed. The optimized compiler-only census passes4.160160s with original natural exit/Job/HANDLE proof: all1955 publisher views/refusals match; both recipes retain2000rows with1998 compiled/2 refused. The exact census-emitted CPU source row passes Clippy and21/21 grouped controls, preserving the full legacy descriptor. The loss population remains the preregistered400 eligible code IDs plus45replay. First DEV-score launch fails before Torch/model load on inherited historical native-context seals; a full13274-file audit finds exactly8 native-context drifts, all model/runtime/math/data pins unchanged. The separately reviewed8-path source epoch and fresh-bytecode-prefix guard pass their controls. A fresh score run passes strict physical GPU identity and loads all310 official weights, then refuses before scoring on an implicit system-site startup module outside the frozen runtime. Natural originalPID/HANDLE exit1, empty Job and35.140643s are retained. The no-site correction then passes52 controls and authenticates2279 actual imported modules without model/CUDA initialization. Fresh SCORE passes85/85 DEV pairs/170 GPU forwards in38.513423s with natural exit/empty Job and final hashes. The frozen exploratory rule selects software_evidence_v1 (macrocomponent AUROC .480313 to .554258, no component fixed-zero balanced-accuracy regression); three DEV clusters do not establish significance or quality promotion. The original120s one-update/32-draw probe passes41.304089s with finite gradients, discarded adapter and measured allocator/host bounds. The full445-draw/14-update pilot then passes184.803123s under its original600s/16GiB-host/8GiB-allocator limits, with natural originalPID/HANDLE exit0, empty Job, final input hashes and a merged full checkpoint. The saved checkpoint is separately sealed; adapted comparison, real export and exact numerical/task/resource qualification remain unrun. Source-policy identities change explicitly; no old CAL/quality result transfers. Full Step8 and promotion requirements remain unmet. These labels do not supply unjudged alternatives, necessary/complementary/exception roles or terminal gold. No training or quality promotion is claimed. |
| 11 | blocked | Terminal /2 contracts, global-winner/no-fallback behavior and per-class refusal fixtures pass native checks. Training and terminal promotion remain blocked by unresolved Step8 data/rights/split/label-scope acceptance and exact CAL/confirmation; terminal classes stay unavailable. |
| 12 | in_progress | Focus and Compact retain their separate numerical/resource component passes; Compact formal 17/128/8192 checks have maximum raw error 0.000190735 and 20-sample p95 86.049136s below 120s/16GiB. Large Flash remains numerically refused at 0.001331329 > 0.001. The separate Dense graph-only transformation passes; native NN0 metadata/template emission and six tiny 17/128 controls also pass, with maximum error 8.34465e-7. Full Dense 8192 fails in 191.893830s: load returns in 39.093627s but score does not return within its original 120s; the watchdog terminates owned PID 58052, exit 77, with settled handles/empty Job/no drift. Peak RSS 33,826,385,920B and Job 33,872,953,344B remain below 40GiB. No Dense numerical result, Large qualification or extra Large branch is claimed. Task/CAL/composed/promotion gates remain separate. |
| 13 | blocked | Exact-artifact CAL/confirmation and composed qualification require the unresolved baseline/profile/data gates. |
| 14 | blocked | No current held-out promotion comparison is admitted; consumed historical FINAL populations stay evaluation-only. |
| 15 | blocked | Exactly-three qualified optional profiles, rights/source maps and genuine final publication capture are unavailable. Final-series refusal gates remain strict. |
| 16 | in_progress | Historical c9fe/059a full Clippy/4825 regressions retain their identities. Current D06 rebuilt R5/R8 delivery evidence remains separately bounded. Applied Mac alias correction passes Windows Clippy/25 tests, Mac Clippy/one metadata case and all37 continuation tests, including exact refresh. The continuation batch remains nonpass on automatic cleanup; separately reviewed manual cleanup removes6426 isolated target entries with unchanged sources and reobserved absence. Core/Store generation2/2 and repaired166/166 pass. Higher77-file consumers pass eleven-crate all-target/all-feature Clippy33.958642s; grouped native regressions now pass105/105. UI110 tests/typechecks pass under source-only scope. Original R12 provider batch remains failed at08:27Z. Separately bounded Flash/LLM/final target sections, warm composition and the actual96-input static aggregate now pass naturally. Native checkpoint/custody bridge passes all-target/all-feature Clippy and78/78 regressions against the actual CUDA archive; all compiler/test-only fixture corrections retain original assertions. Exact CUDA-linked images are preserved (de8afa51). Actual native custody/request emission passes. The first valid metadata-custody refusal is closed through the existing protected-copy mechanism. A second assembly exposes an overall-origin/R12-clock comparison defect. The reviewed native correction passes scoped Clippy and81/81 regressions, preserving original clocks/limits. Fresh R3 assembly passes12.868392s with original exit0/Job0, authenticating13 DLLs and the actual checkpointed static build; manifestaf6d1ec5 remains unqualified with fits=[]. First GPU model execution is pending. No-inference three-crate all-target Clippy passes49.778134s. The early Windows CUDA math default additionally passes CLI all-target/all-feature Clippy76.541376s and5/5 native regressions115.478912s including build; direct/login artifact qualification remains separate. No GPU model, final-workspace, installed or release pass follows. |
| 17 | in_progress | Version 1.8.207 and the signed c547 R3 EXE/native ZIP/VSIX delivery retain their verified byte, signature, notices and client/UI proofs. Real Code 1.146.0 production VSIX Explorer activation/close-reopen/CSS checks pass on a private profile in 8.109072s; native_execution is false. Code parent exit 0 and a private-profile CIM observation are retained without claiming every descendant HANDLE. No native production activation or I4 installed journey follows. The user's 1.8.206 installation/configuration remain preserved; no qualified final-series release or publication is claimed. |
| 18 | in_progress | Current delivery, verification limits, platform failures and Core/Store/GPU progress retain their exact identities. Signed c547 R3 remains the delivered CPU candidate; e306 R5/R8 are separate current evidence. FP32 then efficient FP16 and remaining gates stay open; no old-candidate packaging expansion, score-floor change or release claim. Outside analyses remain ignored as implementation input. Deadline2026-11-19T05:29:42Z; latest14:30Z self-check2 passed/11 in_progress/5 blocked, next16:30Z. Preparation has consumed too much time; first FP32 remains critical. Automatic approval review blocks new component-runner files; clarification is pending while existing metadata and independent source work continue. No publication authorization. |

The user approved reviewed in-scope tracked-file content edits while preserving
existing ownership and ACLs. This resolves the source-edit constraint and does
not authorize foreign ignored output/cache writes. The UI ownership incident
and exact preserved objects are recorded in the ledger. Fresh human-owned
outputs are used; no account, ACL or ownership workaround qualifies a check.

### Original known-issue disposition during execution

This table supersedes current-status interpretations of the dated 1.8.201 issue
record; its original measurements, failures and receipt identities remain history.
The candidate version is now1.8.207, discovered from live official inventories
and synchronized once across the four native/extension manifest/lock owners.

| Original issue | Current disposition and evidence limit |
| --- | --- |
| Unqualified optional profiles | Retained unqualified; no training/promotion without accepted labels and exact native CAL/confirmation/resource/composed evidence. |
| GitHub-only model source | Original release assets and the independent 23-asset census remain intact. Public Focus R7 and repaired Large finalization/cache components pass. Private Compact 8192 numerical/resource checks pass. Historical eager failures, Large Flash numerical refusal and Large Dense score timeout remain failed. No qualified all-three public catalog or new Context URL/publication is invented. |
| Pre-1.9 local-test admission | Retained as a disclosed local candidate mechanism; final-series/native publication gates stay strict. Current terminal source changes require new Context metadata and invalidate old policy identities. |
| Missing qualified publication capture | Blocked by absent qualified three-profile catalog. No synthetic capture or fabricated public URL is substituted. |
| Larger default/client timeout chain | Native defaults 8192/120000 and the Rust-emitted 125000ms delivery chain pass literal/refusal and transport cases. Signed c547 R3 cold reads pass after the bounded genesis repair. Compact now has formal 17/128/8192 numerical and 20-sample resource passes, including p95 86.049136s. Those component results do not establish task quality, CAL or composed Answer latency. |
| R154 Answer timeout | R7 remains failed near56s. After D06 removes55/60-second connection-admission clamps from the accepted120-second parent, genuine rebuilt e306 R8 delivers one canonical packet in62.964776s and settles original service HANDLE/lease. Paragraphs remain empty/unsupported, stopbudget and native verification UNVERIFIED0/18. This resolves the exercised delivery cutoff, not task quality, p95, CAL or promotion; original R154 is not replayed. |
| Context prompt fit | Complete native126-request census passes with121 shrinks/5growths/0refusals, all within original2048. The original shrink-only test remains failed. This does not qualify new8192 graphs or reconstruct missing R154 bytes. |
| Historical accepted failures/90% threshold | Superseded by this plan; no acceptance waiver. Native private-boundary tests pass under verified human identity; config49/49 pass with only disposable fixture GIT_PAGER removed. Other reported failure classes are repaired or awaiting current grouped native checks. |
| Completion criterion wording | Frozen gate is at least.75 completion, at most.02 completion regression, and at least.10 paired common-success external-turn reduction. Historical '.02 gain' prose is not the acceptance rule. |
| Platforms and installed artifacts | Signed R3 client/UI/Code Explorer and historical c9fe4,825/4,825 retain their CPU candidate scope. Mac alias Clippy/metadata and37 continuation tests, including exact refresh, pass; the automatic-cleanup batch stays nonpass. Separate authorized manual cleanup removes the exact isolated target while preserving source/evidence. Linux96-archive aggregation passes; no Linux application or modern Windows I4 pass follows. Core/Store and higher-consumer scoped checks pass; grouped native regressions pass105/105; provider compilation remains active. GPU model execution and final qualification remain pending. |

## 11. Automatic GPU execution and CPU fallback (D08)

Added during execution on 17 November 2026 UTC at the user's request.
D08 expands implementation scope; the subsequent instruction explicitly requires
AIRK to choose a usable GPU for each user's machine and otherwise use CPU.
This section supersedes CPU-only execution assumptions for optional Arbiter
profiles. The user's latest instruction supersedes the intervening FP32-only
direction: **FP32 working first, then FP16 with efficient attention, with both
options available for version 1.9.0**. These are separately identified and
qualified variants. Existing MiniLM CPU INT8 paths remain. Native device-selection and CPU-fallback source are implemented; selected GPU
model execution and qualification remain unproven. Efficient-FP16 native policy
is still unapplied; source preparation and cheap tests do not qualify a model.

### Required user behavior and scope

After the user enables/enrolls an optional stage, its default device policy is
**Auto: prefer a qualified GPU; otherwise use a qualified CPU path for the same
selected model**. Device selection requires no routine user intervention.
Optional stages stay off by default and existing consent/enrollment remains.
The service, CLI, Admin and Workbench expose the actual selected device and the
reason for CPU selection. A GPU being present is not evidence of model fit.

Keep MiniLM's existing CPU U8/S8 paths and vector identity unchanged in this
extension. First establish FP32 with TF32 disabled, then the separate FP16
efficient-attention variants. Prioritize Context Compact/Large, where CPU time
and GPU memory are consequential; measure Focus variants independently.
Preserve independent Focus/Context selection; Focus keeps its CPU path unless a
separately qualified GPU variant offers a measured benefit. Do not silently
substitute Compact for Large, change precision, reduce input width or reset the
operation budget to make a device fit.

CPU fallback is conditional on a qualified, currently fitting CPU execution
variant. If neither device can run the selected model safely, return a typed
unavailable reason and preserve ordinary MiniLM operation. Large's existing CPU
numerical/deadline failures are not qualified fallbacks.

Provide a service-wide **Allow GPU use** setting. Auto remains the default
when GPU use is allowed. Turning GPU use off takes precedence over Auto,
explicit GPU precision, and experimental Radeon opt-in across all backends,
including CUDA. With the setting off, select the qualified CPU variant for
the same model when it fits. Otherwise return typed unavailability.

On disable, stop admitting new GPU loads, reservations and work. Settle
existing owned GPU work under its original deadlines, then unload GPU
sessions and release reservations. Report disable in progress until
settlement is proven. Persist the setting across restart and expose the
actual device and reason through the service, CLI, Admin and Workbench.
The existing per-stage CPU policy remains available for narrower control.

### Verified starting point and backend order

Read-only Windows inventory at 2026-11-17T01:57–02:00Z found an NVIDIA RTX A6000,
compute capability 8.6, driver 597.06, 49,140 MiB total and approximately
38,400 MiB free VRAM. Free memory is a transient observation, not a reservation.

Provisional discrete-VRAM planning targets, not measured minima or admission
thresholds: Focus6–8GB at512 tokens; Compact24GB and Large48GB at8192 tokens.
Dense FP32 attention can require a4GiB matrix for Compact or8GiB for Large,
with multiple intermediates plus weights/runtime/headroom. The actual G4
measurements determine requirements; never hard-code these guesses as fit proof.
These estimates assume one selected Context model, not both Context models or
unmeasured joint GPU residency. Apple unified memory also pays host/OS costs.
Architecture references:
[Compact](https://huggingface.co/Qwen/Qwen3-1.7B/blob/main/config.json),
[Large](https://huggingface.co/Qwen/Qwen3-4B/blob/main/config.json).
CUDA_PATH names Toolkit 12.8; compatible CUDA/cuDNN libraries, their provenance
and actual native execution remain unverified. Mac Neo has an Apple A18 Pro and
8 GiB unified RAM. It cannot be assumed to fit either Context model.

| Priority | Platform/backend | Work and acceptance limit |
| --- | --- | --- |
| 1 | Windows NVIDIA / CUDA | First bounded compatibility and native qualification target on the actual A6000. Pin compatible runtime, driver requirements and dependent libraries. |
| 2 | macOS / CoreML | Inspect exact graph coverage and available unified memory on Neo; run only fitting bounded probes. Distinguish GPU, Neural Engine and CPU placement; report a genuine fit/coverage blocker. |
| 2 | Windows DX12 / DirectML | Evaluate available Intel hardware after a bounded graph-coverage probe. Scope results to the tested adapter and driver. Follow the separate opt-in addendum for experimental Radeon. |
| 3 | Linux x86_64 / CUDA on WSL2 | Qualify FP32 and efficient FP16 on the existing same-user WSL2 instance after Windows qualification. Verify device access and the compatible Linux stack. CPU Linux gates continue independently. |

DirectML remains supported but is in sustained engineering, with new development
directed to WinML. Its sessions require sequential execution and disabled memory
patterns. CoreML compute-unit selection does not prove that every operation ran
on the GPU. These constraints inform the implementation; neither provider is
assumed to support the selected transformer graphs.
Sources: [DirectML](https://onnxruntime.ai/docs/execution-providers/DirectML-ExecutionProvider.html),
[CoreML](https://onnxruntime.ai/docs/execution-providers/CoreML-ExecutionProvider.html).

TensorRT, WebGPU, OpenVINO and QNN are deferred beyond this
extension. Do not introduce a new process, cloud compute, driver installation,
global environment changes or a runtime selected through an ambient DLL path.

### G1 — Record native device and compatibility contracts

Reopen the device-dependent part of Step 2 before dependent code changes.
Core currently binds CpuFit and onnxruntime-cpu/1.30.0; this is more than a
session option. Record the precise additive/versioned device-fit, resource,
profile, execution-identity, status and qualification decisions in the owning
contracts. Preserve historical CPU descriptors/receipts and prove their retained
fixture bytes. Do not silently reinterpret them as GPU-qualified.

Rust owns Auto/Cpu policy, device capability/selection outcomes, typed reasons
and generated client views. Bind provider/build identity, OS/architecture,
device compatibility, precision/math mode, graph assets, memory requirements,
placement policy and relevant provider options. Bind the actually selected
execution identity into dependent caches and qualification evidence. Keep
existing model identity and CPU vector compatibility unchanged. If a persisted
or wire shape must change, first record its exact version/migration/refusal
decision; this scope approval does not approve an unspecified encoding.

The selection contract must cover:

1. Enumerate actual adapters and providers, choose a compatible device
   deterministically, and compare the exact model variant with runtime/driver,
   operator/shape support and validated device requirements. Do not treat
   adapter name, a successful allocation or a provider list as sufficient.
2. Evaluate usable free memory and measured load/workspace peaks with headroom,
   existing reservations, both resident optional stages, MiniLM and other work.
   Unified memory is one shared pool. Free-memory observations can race; retain
   real admission/ownership and handle allocation failure.
3. In Auto, try only qualified compatible GPU variants. When absent, unsupported,
   out of memory, inaccessible or unable to initialize before dispatch, select
   a qualified fitting CPU variant and report the precise reason. Recheck
   authority, capacity and the original deadline before dispatch.
4. Hold device/resident charges until actual native and device work settles.
   No fallback replays uncertain work, refunds unknown work or grants a new
   timeout/token budget. An in-flight GPU failure permits same-request CPU work
   only after proven settlement and admission within the original shared budget;
   otherwise report that failure and select CPU for subsequent fitting work.
   Avoid repeatedly retrying a known broken provider; invalidate its diagnostic
   state on a relevant device/driver/runtime/configuration change.
5. CPU remains usable when optional GPU dependencies are missing. Forced CPU
   selection is available for diagnosis/reproducibility. Keep nonselected
   devices and unrelated applications untouched.

Owners: Core arbiter/resource-policy and inference-provider contracts,
Retrieval runtime/publication/qualification, Inference pool, Service admission,
native schemas and the approved generated I2/I3/UI consumers.

### G2 — Prove runtime and graph compatibility cheaply

Inspect pinned ORT 1.30.0 and ort 2.0.0-rc.10 rather than assuming current web
examples match the native build. CUDA supports FP32 generally; FP16 conversion
is not inherently required. However, the pinned CUDA GroupQueryAttention kernel
registers F16/BF16 activation types, whereas the retained GQA artifacts use FP32.
Registering CUDA alone therefore cannot qualify those graphs as all-GPU.

Use the existing original unfused FP32 Compact (c79c6725...) and Large
(f8ba5d3e...) graphs first. The approved FP32 Context matrix is 32, 128, 1024
and 8192 tokens, with unchanged math and actual placement. Use exact retained
independent reference scores for unchanged 128 and 8192 inputs, and obtain
independently pinned scores for 32 and 1024. Retain earlier 17-token evidence
under its original identity. Use one loaded session for the initial profiled
pass within its original deadline. Preserve
causal/padding masks, positions, projections, decision rows and the FP32 output
ABI. This first milestone does not change precision or use FP16 to conceal an
FP32 failure. Preserve a failed FP32 result under its original identity.
After actual FP32 native success and demonstrated Auto/CPU fallback, implement
the separately registered FP16/efficient-attention milestone below. Do not block
independent FP16 preparation on unrelated missing scientific labels, but neither
variant is promoted without its own complete gates. Any necessary alternative
FP32 operator decomposition needs a reviewed identity and unchanged gates.

Use error_on_failure for provider registration. For a strict GPU session,
disable implicit CPU EP fallback and do not also register CPU EP explicitly:
the pinned runtime rejects that combination. CPU fallback means a separately
selected qualified CPU session, not invisible CPU placement inside a GPU claim.
If shape/control operations require CPU, define and qualify an explicit bounded
hybrid placement policy with full profiler coverage; reject unexpected CPU
compute and disclose the placement. An enabled provider is not utilization proof.
See [ort registration behavior](https://docs.rs/ort/2.0.0-rc.10/ort/execution_providers/struct.ExecutionProviderDispatch.html).

Freeze TF32 and other numerical/provider options explicitly. CUDA defaults can
change float32 arithmetic. Keep applicable numerical tolerances unchanged,
including the existing stricter selected-model reference gates; no threshold
is loosened after a failure. Profile complete optimized graphs/subgraphs.
Measure copies and synchronization; use I/O binding where beneficial rather
than assuming the small token input/output must always remain on-device.
Provider arena limits do not cap all GPU allocations: measure actual device
peak, workspaces and joint residency separately.
Source: [CUDA options and requirements](https://onnxruntime.ai/docs/execution-providers/CUDA-ExecutionProvider.html).

### G3 — Build authenticated runtime variants and recovery

Keep the CPU static build, its failed-attempt history and reusable outputs.
Do not weaken its existing version/engine checks into accepting arbitrary ORT.
Extend the provision/build identity and verifier for explicit GPU-enabled
variants at the same pinned ORT API/source, or record a separate reviewed
version decision if compatibility requires it.

Decide the single-process runtime/link/load strategy before implementation.
Do not co-load incompatible CPU/GPU ORT copies. Demonstrate that the selected
strategy preserves MiniLM and CPU fallback on machines without GPU libraries.
Authenticate application-owned provider libraries and dependencies through
explicit protected package paths; validate compatibility and reject ambient
DLL substitution. Drivers remain system prerequisites, never bundled or
automatically installed.

Record exact source/download hashes, dependency closure, redistribution
permissions, notices, signing, installer ownership and uninstall/recovery.
Provider binaries and CUDA/cuDNN dependencies make a single-static-binary claim
conditional; do not invent a mandatory packaging layout before the link audit.
Extend the native runtime/package metadata and maintained builders, not private
qualification bypasses. Retest installation with missing, mismatched, corrupted
and removed provider dependencies while preserving a working CPU service.

Recorded native contract decisions (17 November): product CUDA requirements use
RuntimeManifest/2, distinct from the immutable /1 scientific UUID/driver fit.
The initial compatibility class remains Windows x64/SM86. Other architectures
use an admitted fitting same-model CPU path or report unavailable. A native,
non-deserializable eligibility witness requires one actual management load,
complete placement observation, independent synthetic numeric roster and
synchronization/capacity/authority recheck under the existing original120-second
management cutoff. Reaccept affected retained GPU sessions within that same
cutoff when their joint residency changes; never renew it per session. Explicit
CPU performs no GPU check. This admission does not transfer reference-hardware
p95/peak or semantic qualification. Public provenance is a Rust-owned, explicit
Notice-asset projection; raw private producer receipts never ship.

For G4 read parity, keep scalar status lightweight and add optional native
scoped_device_status/1 to canonical diagnostics. Reuse complete stage/device/
precision/selection reasons plus explicit scoped index application, bound to the
original scope/publication/read budget. No global enrollment list, configuration
migration, CAS, model load, device probe or new deadline may arise from an
evidence read. Missing/old/corrupt configuration remains explicitly unavailable.
CLI/session, MCP, Admin and Workbench inherit the same canonical observation;
Search/Answer schemas remain unchanged. Both additions require owning contracts
and independent literal/refusal cases before consumers are applied.

### G4 — Qualify selection, each model/device variant and delivery

Group cheap policy/contract/initializer/provider tests before expensive native
runs. Cover no GPU, unsupported driver/operators, low memory, allocation races,
multiple adapters, both resident stages, missing libraries, registration failure,
CPU fallback, explicit CPU, stale configuration and source withdrawal.
Exercise service-wide GPU disable before startup and during pending or
active work, including explicit FP32/FP16 and experimental opt-in. Verify
cancellation or natural settlement, released sessions and reservations,
and disabled state after restart. Exercise Radeon opt-in, manual disable,
recoverable provider failures, and uncertain GPU settlement separately.
Mocked capability results establish decision logic only.

For every supported model/backend/precision/OS/architecture combination:
run actual numerical checks at 32, 128, 1024 and 8192 tokens for FP32 Context,
the G5 matrix for FP16 Context, and 512 tokens for Focus. Collect actual
placement tracing, cold/load and twenty-sample warm resource measurements with
the existing nearest-rank p95 <=120 seconds, device/host memory and coexistence.
Measure composed Answer end-to-end latency separately. Preserve original
8192/120000 and 125000ms transport delivery limits and actual settlement.
Exercise cancellation, device failure/OOM, disable/Remove/restart and CPU
recovery with native owners. Report unsupported combinations explicitly.

Any changed graph, precision, runtime or execution identity needs its own exact
artifact CAL/confirmation and task evidence before promotion; CPU component
passes do not transfer. Existing missing labels, consumed FINAL populations
and terminal-class restrictions remain blockers. GPU speed cannot manufacture
data acceptance or qualify Large by itself.

Generate full device/status/reason results for CLI/session, MCP, Admin and
Workbench. The normal UI shows a concise device choice; details and manual CPU
selection belong in Service/settings, with no new permanent composer notice.
Compare complete canonical results and exercise real transports.

Package only tested variants with verified legal notices in the installer,
base README and Arbiter README. New GPU variants receive new artifact identities
and candidate qualification; retain the delivered R3 CPU candidate unchanged.
No new immutable model URL, publication, deployment or user-install mutation
is authorized by D08.

### Integration into ongoing work and schedule

| Existing steps/work | Change after D08 |
| --- | --- |
| 2 | CPU/terminal/budget decisions remain evidenced; additional G1 device contract work is pending before GPU consumers. |
| 3–6 | Continue operational repairs, real composed readiness/Answer, lifecycle and client parity. Add device/refusal/fallback cases when G1/G2 are frozen. |
| 7, 12 | Add a separately identified GPU baseline and exact runtime/graph/resource freeze; prioritize one bounded CUDA Context path over further CPU Large attention experiments. |
| 8–11 | Rights, supervision, split and acceptance blockers are unchanged. GPU work does not restart open-ended data acquisition or invent human labels. |
| 13–15 | Require variant-specific numerics/resources/CAL/task/promotion proofs. Keep the selected Focus/Compact/Large requirement; device variants are not extra selected model families. |
| 16–17 | Finish useful CPU platform/candidate work on frozen inputs; extend applicable native, real transport, signed package and installed gates for GPU variants. Do not discard completed Linux CPU compilation or retag R3/R4 as GPU-tested. |
| 18 | Report implementation, automatic CPU fallback, tested adapters/backends and remaining unsupported combinations separately. |

Execution order now: retain and settle current owned work; G1 plus a cheap CUDA
dependency/graph probe; implement reviewed G2/G3; run grouped G4 gates and package
only if they pass. Nonconflicting CPU/Linux/Mac repairs continue, with one Cargo
owner per host/target and one GPU-heavy job at a time. The R154 fresh CPU route retains its
original fixture identity; it is not retrospectively a GPU comparison.

The prior 72-hour deadline was 2026-11-19T05:29:42Z. At the dual-precision
scope decision (approximately17 November03:05Z), the engineering estimate is
36–48 additional hours of work; the extension provides roughly50 calendar hours.
Target both implemented paths, applicable native qualification and a reviewed
Windows candidate by Wednesday18 November10:29p.m. Denver. This is an estimate,
not a claim of future passes or a guarantee that external label/rights/platform
blockers disappear. FP32 integration is the next milestone; FP16 efficient
attention follows, then both-variant selection/recovery/package qualification.
The user authorized continuing beyond that checkpoint on 18 November.
Do not promise universal GPU support or the attachment's unmeasured10–50x
speedups, few-second latency or12–16GB fit. If a gate cannot finish, retain its
evidence and exact unmet criterion rather than weakening it.

Current D08 status: reviewed Core/Store device contracts and migration are
implemented with scoped Clippy and166/166 regressions passing; native schema,
49 semantic cases and generated types are current. Higher runtime/service/
transport/UI foundations are committed with grouped scoped checks; product
eligibility and scoped diagnostics pass grouped native regression checks. Actual GPU model execution
and qualification remain pending. The original R12 full provider batch remains failed at its cutoff. Separately
bounded warm Flash, LLM and final provider sections now settle naturally, and
checkpoint composition and the genuine96-input static aggregate pass. Native
assembly and actual GPU model qualification remain pending. No GPU model has executed.
Existing CPU results retain their original identities.

### G5 — FP16 with efficient attention, after FP32 works

This is now part of version1.9.0, not deferred work. Keep the FP32 variant
available throughout. The GPU setting offers Auto, FP32 and FP16 precision;
explicit CPU remains a separate device choice. Initial Auto preference is a
qualified fitting FP32 GPU variant, then a qualified fitting FP16 GPU variant
of the same selected model, then its separately admitted CPU path. An explicit
GPU precision preference restricts GPU candidates only; CPU fallback uses the
existing admitted CPU precision and reports it plainly. Never silently substitute
a model, turn on a stage, narrow input or reset the shared budget.

1. Register the exact half-precision conversion, protected FP32 operations,
   output FP32 ABI, native initializer loading and attention algorithm before
   numerical scoring. Keep the original FP32 weights/graphs and their receipts.
   Every FP16 graph/external-weight/runtime-option/profile/calibration identity
   is separate; a dtype label is not evidence.
2. Implement genuinely memory-efficient attention. Merely casting the current
   biased GQA graph still selects an unfused CUDA path. Preserve the exact
   causal/left-padding and any supported internal-mask semantics, positions,
   scale, projections and decision rows. Use independent literal/tensor controls
   including padding holes, all-masked/edge cases and17/128/8192 boundaries.
   If an efficient kernel cannot represent an input, declare/refuse that shape
   or use a predeclared qualified alternative before dispatch; do not drop masks.
3. Preserve the FP32 numerical gates. Freeze matching-precision reference-to-
   native parity and FP32-to-FP16 precision-loss comparisons independently,
   using the existing applicable approximate-export/task rules before observing
   results. New near-boundary cases, full output comparisons and exact-artifact
   CAL/task/terminal constraints remain. No tolerance is loosened after failure.
4. Repeat actual placement, host/VRAM peaks, load/cold/warm twenty-sample p95,
   joint residency, cancellation/device settlement, memory pressure/OOM recovery,
   Auto/explicit-precision/CPU fallback and real composed120-second behavior.
   An arena limit or lower weight size is not complete memory-fit evidence.
5. Prefer one authenticated runtime build supporting both precisions when pinned
   source/build inspection justifies it, avoiding an unnecessary later native
   rebuild. Compiled kernel availability does not admit the FP16 model before
   its separate gates. Bind exact build options and model variant in receipts;
   never mix core/provider binaries from different builds.
   Windows CUDA builds establish an absent process-local GEMM2 default as the
   first synchronous main operation, before ORT and workers, covering direct/login
   entries. Preserve explicit values; leave debug logging optional for product
   execution. Retrieval captures/checks the immutable startup value without late
   mutation. No global environment or service-manager setting changes.
6. Generate native device/precision status and settings for CLI, Admin and
   Workbench, retaining full copy/export results and no permanent composer
   notice. Package both tested variants with truthful fit/availability and
   notices. Keep the selected three model slots; variants do not create extra
   selected model families or waive final promotion/publication requirements.

Provisional efficient-FP16 planning targets remain about4GB for Focus,8GB for
Compact and12–16GB for Large, not measured hardware minima. Without genuinely
efficient attention, half precision might still require12–16GB for Compact and
24GB for Large at8192 tokens. These guesses never grant native admission.
The actual available-memory and qualified per-variant policy decides Auto.

## Addendum — Linux CUDA qualification on WSL2

After Windows qualification is complete, proceed directly to FP32 and FP16
Linux qualification in the existing WSL2 instance on this machine. This step
can start without another kickoff. Run Windows and WSL2 GPU-heavy jobs in
sequence on the shared NVIDIA GPU.

Verify the Windows human account and the corresponding WSL2 Linux UID before
the batch. Record the distribution, WSL kernel, mapped NVIDIA driver, GPU,
available memory and compatible Linux CUDA and cuDNN versions. Verify actual
CUDA device access and the loaded library paths. Use the existing Windows
host driver and the provisioned Linux user-space toolchain. Report missing
access or dependencies with the exact affected gate.
[NVIDIA CUDA on WSL](https://docs.nvidia.com/cuda/wsl-user-guide/index.html)

Build and seal the Linux x86_64 runtime and its CUDA provider in an isolated,
same-user WSL2 target. Use the reviewed source revision and exact model graph
and weight identities. Run the existing Compact and Large FP32 graphs at 17
and 128 tokens, then at 8,192 tokens after the initial probes pass. Repeat
with the separately pinned FP16 variants and verify efficient attention,
required mask behavior and the FP32 output ABI.

Apply the existing G4 and G5 gates for each applicable variant, including
actual placement, numerical parity and precision loss, peak memory, cold and
warm p95 latency at or below 120 seconds, and Auto and admitted CPU fallback
behavior. Retain the exact model, runtime, driver, command, placement and
timing receipts. Report qualification for the recorded Linux x86_64 WSL2
configuration and preserve any variant-specific refusal or failure.

## Addendum — Datagram macOS FP32 and FP16 weekend

Start this separate macOS test only after the Windows candidate has completed
the qualification required by this plan. The user will manually book and
kick off a 72-hour Datagram weekend pass on a dedicated Apple Silicon
Mac Studio with M4 Max, a 40-core GPU and 128 GB of unified memory.
[Datagram weekend pass](https://datagram.net/day-pass)

Datagram confirmed full access to the dedicated Mac. We can install Xcode
or ask Datagram to do so during setup. At kickoff, verify SSH access in
the same authorized human account, record the macOS, Xcode and SDK builds,
and confirm Core ML and Metal profiling in that session. Freeze the
reviewed source revision, runtime build, graph and weight identities, and
use only the inputs authorized for the rented host.

Build the native macOS arm64 Core ML path. Test the existing Compact and
Large FP32 graphs first, then test their separately pinned FP16 variants
with efficient attention. For each precision and model, start at 17 and
128 tokens and advance to the 8,192-token boundary after the initial probes
pass. Check that the FP16 path uses an efficient attention implementation
on the Mac and preserves the required masks and FP32 output ABI.

Record actual GPU and CPU placement, numerical parity, peak unified
memory, and cold and warm latency for each variant using the existing G4
checks and 120-second p95 target. Apply the G5 FP16 precision-loss and
memory checks. Exercise Auto, explicit FP32 and FP16 selection, and the
admitted CPU fallback where applicable. Preserve and report each graph
coverage, fit, numerical or latency failure with its exact variant identity.

Before the pass ends, copy the permitted receipts, traces and results to
the local campaign evidence root. Record the qualification scope and any
remaining blockers in the ledger. Datagram wipes the machine after the
pass. [Datagram weekend pass](https://datagram.net/day-pass)


## Addendum — Experimental Windows Radeon GPU support

Offer Windows Radeon acceleration through a separately labeled, off-by-default
DirectML opt-in. Start with Context Compact FP32 and build on the Windows
development workstation. Verify compile, packaging, and the available
DirectML smoke path. Radeon hardware testing is unavailable for this work,
so disclose the tested adapters and label the Radeon path experimental in
the setting, device status and release notes. The observed approximately
25× A6000 speedup for the 8192-token Compact model call motivates the
experiment. Record Radeon performance only when measured.
[Windows DirectML provider](https://onnxruntime.ai/docs/execution-providers/DirectML-ExecutionProvider.html)

When opted in and GPU use is allowed, check the actual adapter, provider,
operator coverage and device fit before running the selected model.
Missing provider bits, unsupported operators, insufficient memory or a
settled execution failure can trigger automatic fallback to the qualified,
fitting CPU variant of that same model within the original request budget.
If GPU work has uncertain settlement, stop the request and keep the GPU
path unavailable until safe recovery. Turning off the Radeon experiment
returns Radeon-only systems to CPU selection. The service-wide GPU switch
takes precedence over this experiment.
