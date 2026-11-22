# Current agent assignment

These instructions are mandatory for the active assignment. Standing rules remain in [AGENTS.md](../AGENTS.md). Historical receipts and prior decisions remain in the ledger; this file records the effective campaign scope.

All unlinked filesystem paths below are repository-relative unless absolute or prefixed with `~/`.

The user resumed the Windows and Linux FP16 campaign after D: cleanup. Keep new Windows work on D:, reuse the existing checkout target with one Cargo owner, and apply the standing disk budget and fixture disposal rules. Retained receipts remain evidence under their original identities. Removed fixture installations must be restored or replaced by a verified compatible installation before reuse.

| Need | Source |
|---|---|
| Ordered implementation plan and D01–D08 user decisions | [docs/arbiter/final-plan.md](arbiter/final-plan.md) |
| Known issues and historical candidate evidence | [docs/arbiter/knownissues.md](arbiter/knownissues.md) |
| Weight-experiment rationale | [docs/arbiter/weights-v2-plan.md](arbiter/weights-v2-plan.md) |
| Progress ledger | [docs/simplify/one-process-ledger.md](simplify/one-process-ledger.md) |
| Existing split decisions and contracts | [docs/split/split-plan.md](split/split-plan.md), [docs/split/contracts.md](split/contracts.md) |
| Standing simplification decisions | [docs/simplify-plan.md](simplify-plan.md) |

## Authority and deadline

- Branch: feat/arbiter-final, created from feat/advanced-split. Commit locally. The user authorized pushing
  this branch to origin on 20 November 2026, Denver time. No PR, merge, publication or deployment is authorized.
- **Current task from 21 November 2026:** the Answer evaluation plan,
  [docs/arbiter/final-plan-fp16.md](arbiter/final-plan-fp16.md). Its
  execution started on the user's word on 21 November 2026. For this campaign it
  replaces the qualification gates below (the raw-score ceilings, the 98%
  paired evidence gate and held-out confirmation) as acceptance evidence;
  their receipts stay as history. Agent sessions run in headless Claude Code
  on Haiku and judge sessions on Sonnet, both at medium effort (changed by
  the user on 21 November from Sonnet and Opus; the first round runs on
  airk alone); each series is one commit; three
  iterations run back to back, then work stops for the user's manual
  inspection, where the user may change the number. Private campaign folder:
  `artifacts/v190/final-plan/exec-20261116T052942Z/answer-eval-r1/`. Every
  trained weight candidate is trained on pairs from both the airk and
  bookbuilder repositories.
- **Redirected on 22 November 2026:** the user stopped the evaluation runs
  during change set 3 and directed that Answer become a local multi-round
  inference loop (plan section 12: Split a compound request into parts; per
  part Plan, Retrieve per sub-need, Read and check coverage, Follow up,
  Compose; then Union the parts). The goal is unchanged. Large runs on the GPU
  only, falling back to Compact on the GPU and then Compact on the CPU; Focus
  and Compact GPU routes become product priorities. Answer's time allowance
  rises above 120 s. Implementation (plan section 12.8, L1 onward) starts only
  after the user has reviewed the plan revision and its commit. No backward
  compatibility: the new Answer version replaces v2 at once. The pending
  one-turn evaluation work is committed and pushed as it stands (L0); its
  results are history, and the evaluation and weight refinement start over on
  the loop (L7).
- **D: allowance doubled, 22 November 2026.** The user doubled the D: growth
  allowance for the Answer-loop campaign: "Allow more disk if needed. Just
  watch available space." The D budget against the original Windows baseline
  1233246265344 bytes rises from 195421011968 to 390842023936 bytes, so the D:
  free-space floor falls from 1037825253376 to 842404241408 bytes. The 100 GiB
  physical floor, per-job reservations, cleanup owners and monitoring of
  actual free space still apply.
- Previous task: implement all steps of docs/arbiter/final-plan.md, authorized
  by the user on 15 November 2026. Execution T0 is recorded in the ledger.
  The user authorized continuing past the earlier 72-hour checkpoint.
  The prior 48-hour FP16 deadline was **Friday, 20 November 2026 at
  9:27 p.m. America/Denver** (`2026-11-21T04:27:11Z`). It runs across the
  extension restart and includes time spent closing the Windows Large FP32
  prerequisite. Original T0 and per-run limits apply.
  The previous overall delivery target was Friday, 20 November at 10:10 a.m.
  America/Denver (17:10 UTC). Tyler set that target aside on 20 November and
  authorized continuing. Preserve the qualification gates, sequence, approved
  storage limits, and the original clocks and receipts of individual attempts.
  [FP16 schedule](arbiter/final-plan.md#5-time-budget-and-unattended-scheduling)
- The final plan supersedes the old advanced-split execution deadline and
  campaign ordering. Preserve prior ledger entries as historical evidence.
- The Windows Large FP32 CUDA prerequisite passed on 18 November. Qualify
  efficient FP16 on Windows CUDA first. After Windows FP16 passes, start FP16-only
  Linux CUDA qualification directly on the existing WSL2 instance and
  report genuine blockers. Linux CPU checks continue independently.
- Apply the user-approved provisional 0.07 raw-score ceiling to this FP16
  version's development runs. The user subsequently froze 0.007 maximum
  absolute raw-score error versus the independent matching-recipe reference
  for this Windows/Linux Context Compact and Large FP16 CUDA release, before
  held-out confirmation. Retain the mass, winner, calibration and 98% paired
  evidence gates and review consequential close calls.
- The user approved separate Linux ELF runtime manifests for scientific and
  product runs, with pinned Linux dependencies and a Linux compatibility
  digest. Preserve the existing Windows manifest and compatibility identities.
- After both CUDA FP16 gates pass, qualify FP16 only on macOS using native
  Core ML. The Datagram weekend has a separate manual kickoff. The future
  Radeon FP16 campaign uses Azure exclusively after those three gates pass,
  in order of Windows DirectML and Linux ROCm. The current execution remains
  local. No cloud teachers are authorized.
## Models and qualification

- The user confirmed FP16-only delivery on 20 November Denver time, on CPU
  and GPU, including explicit CPU inference and automatic CPU fallback. Stop
  new FP32 work. Use the already completed FP32 baseline only for comparison
  with the final FP16 artifacts.
- Qualify the exact FP16 CPU and GPU artifacts, efficient GPU attention, and
  a separately qualified FP16 automatic CPU fallback. Preserve the measured
  quality, performance, memory, and platform gates.
- On 18 November the user approved an exact 32 GiB Windows Compact host
  process cap. Use a 32 GiB aggregate Job cap so the process can use that
  allowance. Preserve the 24 GiB device budget, original 120-second deadline
  and strict placement. Keep the earlier 16/24 GiB
  attempt evidence under its own identity.
- The approved FP32 Context matrix for Compact and Large is 32, 128, 1024
  and 8192 tokens. Reuse exact independent results for unchanged 128 and
  8192 inputs. Generate independently pinned references for the new 32 and
  1024 inputs. Retain the 17-token evidence under its original identity.
- Run the approved bounded-hybrid experiment with full final graph and CPU
  tensor bounds checked before scoring. Measure actual runtime profiling
  and verify scores against both independent references. Use one loaded
  session for the first four-case profiling pass within the original
  deadline. Collect required cold and warm qualification separately.
- A measured Large FP32 host-cap constraint authorizes a fresh 48 GiB
  process and 48 GiB aggregate Job with the full host reservation and
  actual headroom checks. Run one Large FP32 process at a time with
  exclusive use of the GPU. Observe empty Job and host reservation
  settlement before starting the next process.
- Preserve the selected bundled MiniLM plus independent optional Focus
  (Qwen3-Reranker-0.6B) and Context Compact/Large (Qwen3-1.7B/4B). Terminal
  classes stay unavailable until their own evidence passes. Use suitable
  existing human-reviewed labels; missing labels block only dependent
  acceptance/promotion, while independent work continues.
- The approved qualification target is a new default of 8192 shared input tokens
  and 120 seconds, including all client timeouts, with a measured p95 limit of
  120 seconds. Report actual component and end-to-end latency. Historical
  2048-token/30-second evidence remains historical. The user selected a
  static 1,024-token query/passage pair for Focus on 19 November Denver
  time. Re-export and qualify the 1,024 graph and compiler under fresh
  identities. Use the existing 8,192-token shared budget and 120-second
  deadline. Existing 512-token runs retain their historical artifact identity.
## Recovery and execution

- Automatic agent-config edits require a reviewed safe policy for the entire
  file and recovery path; otherwise provide the complete canonical manual
  entry. No account, ACL or retention-policy workaround.
- Current-version index regeneration is authorized when upgrade cannot preserve
  the current contract. Preserve source files and unrelated user work. DiskANN
  remains the selected disk-backed vector engine. The superseded installed-editor
  repair does not resume.
- Resume from live source, the final plan, ledger and exact receipts. Keep
  contract-first ordering, explicit parallel file ownership, one Cargo
  owner per host/target and primary review before expensive integrated runs. A failing model
  or unavailable host does not stop independent product checks.
## 20 November Windows and Linux FP16 checkpoint

The original three Compact Flash recipes exceeded 0.007 on the 1,014-token
development input. The later Windows Compact graph with protected dense
projections and compensated CUTLASS attention passed that input at 0.000145
and the exposed TRAIN_Q543 close call at 0.000081. All four canonical widths
passed the matching error, mass and winner checks. The 8,192-token maximum
was 0.000183106 and its synchronized forward took 1.915 seconds. Continue
unprofiled warm timing, fit, calibrated Answer and held-out checks. Product use of
the distinct attention arithmetic awaits the user's execution-identity decision.

Windows Large's original T32 error was 0.011738. Protecting just its Q/K/V
projections produced 0.014718 against that recipe's independent reference.
Both chose the matching winner. The next treatment widens all seven body
projections per layer and retains the original learned half weights and Flash
attention. The Ubuntu static CUDA core and provider now link, and the first
verified GNU service pair has zero product model requirements. Native Linux
model qualification still follows Windows. The 98% paired comparison remains
unmeasured. See the ledger for the exact source, image and run receipts.

Use the checked CAL restart procedure to reuse the installed models. The
Linux provider continues in its existing Ubuntu virtual disk on C:. Check
physical C: and D: headroom plus guest ext4 space before admitting growth.
Keep new Windows outputs on D:. Exact run identities, active deductions
against the original time limits, and later results belong in the
[execution ledger](simplify/one-process-ledger.md).

## 18 November extension restart checkpoint

This checkpoint used the earlier Friday 9:27 p.m. deadline. See the current
Windows and Linux CUDA target under Authority and deadline.

- Resume on `feat/arbiter-final` with verified Windows user and default
  owner, using the checked execution procedure. The existing desktop
  AIRK service and CLI may be present. The qualification process and
  Cargo target were idle at handoff. Recheck the GPU and host headroom
  before an expensive run. The WSL LLM keep-alive task was disabled and
  Ubuntu and Docker were idle. Preserve that condition during the
  Windows Large FP32 run.
- The corrected Large FP32 scored 32, 128, 1024 and 8192 tokens and
  passed both independent comparisons and placement. Reuse the exact
  artifact, the equivalent-predecessor reference lineage and the
  existing passing receipts. Under the private campaign root
  `artifacts/v190/final-plan/exec-20261116T052942Z`, use
  `large-fp32-batch-view-scoring-r1/execution.json` with SHA-256
  `ea9aefbf7e9cefb91bca8384924237f95655131cb49ac571e6900e55f3d6d820`.
  [Windows Large scoring ledger](simplify/one-process-ledger.md#2026-11-18--corrected-large-fp32-passed-all-four-cuda-scoring-widths)
- The corrected Large Windows Resource20 completed with one cold load, one
  warmup and twenty unprofiled 8192-token samples. Both comparisons, the
  120-second operation deadlines, original 48 GiB process and Job caps,
  44 GiB device reservation and settlement passed. Retain the evidence under
  `large-fp32-batch-view-resource20-r1/execution.json` at SHA-256
  `8ff0d3ed5152ea533a46ab1dc258b96b536aa7df2948b49400f275aabda8fc7b`.
  [Windows Large Resource20 ledger](simplify/one-process-ledger.md#2026-11-18--windows-large-fp32-resource20-passed)
- Windows G5 readiness, Compact conversion, independent graph and weight
  verification, and native metadata passed. The exact Compact T32 candidate
  has a 0.012271881 raw difference between independent FP16 and FP32 references.
  The 19 November acceptance makes that comparison's raw error and argmax
  diagnostic on fresh Context FP16 runs. Require matching-recipe parity,
  original-FP32 mass error at most 0.05, and the accepted 98% task-quality
  gate for qualification. Preserve the original receipts. The strict startup
  log on 19 November confirmed CUDA-capable, CPU-preferred nodes and reached
  its cap. The complete disposable FP16 graph diagnostic then found all 28
  attention nodes on CUDA and only fixed-size integer and Boolean CPU data.
  The bounded A6000 T32 scoring probe admitted the exact graph and reached
  its first forward, which exited with a Windows access violation. The
  converted Context graph omits both internal present tensors required by
  the pinned CUDA GroupQueryAttention implementation. Correct those outputs
  in the maintained exporter and independent verifier, retain the public
  score interface and authenticate fresh conversion, metadata and placement.
  The corrected graph and independent T32 proof passed their own gates.
  The next A6000 probe synchronized and logged all 28 FP16 FlashAttention
  layers with complete typed execution, but matching-half raw error was
  0.0248556137 against 0.001. Both mass bounds and the winner checks passed.
  Modeling the pinned CUDA probability rounding in a separate CPU
  reference yielded raw error 0.0303249359 against the same sealed scores.
  The disposable layer-0 capture kept the complete optimized graph and
  reproduced all eight CUDA scores exactly. The source-derived Flash CPU
  replay matched 65,288 of 65,536 captured output values bit for bit on
  identical Q/K/V inputs. Independent prefix Q, K and V all differ before
  attention. With identical captured input and verified weights, the
  direct K/V/Q projections still differ. The shared normalized input
  differed in five of 65,536 values. Standalone GEMM2 reproduced all
  131,072 CUDA projection values. Exact integer dot products found
  1,633 half-rounding mismatches for GEMM2 and 53,141 for FP32 pedantic.
  Keeping FP32 projection outputs until a host half cast reduced
  misses to 1,182 with GEMM2 and 92 with pedantic. The stock CUDA float
  MatMul call with TF32 disabled missed 176 exact half results using
  upcast half operands. The independently verified Compact candidate
  promotes 84 Q/K/V MatMuls with byte-identical FP16 storage. The first
  A6000 load retained the FP32 matmuls but exposed the Level 4 initializer
  optimizer expanding half weights. Disable that specific optimizer for
  this private probe. The corrected A6000 load passed with all 84
  FP32 projections, explicit CUDA weight casts and 374 bounded CPU
  shape controls. The distinct independent CPU reference completed
  one full T32 forward. The owned A6000 score returned and synchronized
  with all 28 FP16 FlashAttention layers. Matching raw error fell from
  0.0248556137 to 0.0085411072. Both mass bounds and the winner checks
  passed. Raw error still exceeds 0.001. Hold wider scoring and isolate
  the remaining arithmetic before another full GPU forward. The completed
  CPU replay of CUDA RMSNorm and QuickGelu ordering increased matching
  raw error to 0.0201478004 against the sealed GPU vector. Keep the
  0.0085411072 candidate result. The isolated attention output projection
  showed 823 half-rounding mismatches with GEMM2 and 117 with stock
  FP32 MatMul on its retained half operands. The separate Q/K/V/O
  candidate completed one synchronized A6000 T32 forward. Matching
  raw error increased to 0.0263910294 while both mass checks and the
  winner check passed. Retain Q/K/V as the lead development candidate.
  The authorized additional comparison is complete. Near ties,
  threshold-adjacent decisions and 8,192-token quality remain unmeasured.
  [Full output-projection comparison](simplify/one-process-ledger.md#2026-11-19--compact-qkvo-t32-increases-matching-raw-error)
  [Output projection comparison](simplify/one-process-ledger.md#2026-11-19--compact-output-projection-benefits-from-stock-fp32-matmul)
  [Source-order CPU comparison](simplify/one-process-ledger.md#2026-11-19--source-order-compact-cpu-reference-does-not-close-raw-parity)
  [Scored Q/K/V result and blocker](simplify/one-process-ledger.md#2026-11-19--compact-qkv-t32-raw-error-improves-and-still-exceeds-the-gate)
  [Corrected CUDA observation and matching CPU reference](simplify/one-process-ledger.md#2026-11-19--corrected-compact-qkv-cuda-load-and-matching-cpu-reference-pass)
  The approved 0.010 Windows CUDA development batch completed on the
  Q/K/V candidate. T128 passed with 0.006991 raw error. T1024 and T8192
  missed at 0.068153 and 0.046494. All three kept the full score order
  and passed both mass limits. This historical batch used a 0.001
  same-recipe qualification limit. The current Windows/Linux Context release
  uses 0.007 and retains the 98% judged-evidence rule. The T1024 layer-zero
  capture reproduced all eight sealed CUDA score words. On identical
  Q/K/V inputs, the multi-tile CPU replay matched 98.54% of attention
  output words, with 0.000976563 maximum error. Avoid widening
  layer-zero attention on this evidence. The next qualification work
  is exact-artifact calibration and a powered, native-visible paired
  judged cohort. Resource20 remains gated.
  [T1024 attention capture and replay](simplify/one-process-ledger.md#2026-11-19--compact-t1024-identical-operand-flashattention)
  [Windows CUDA development measurements](simplify/one-process-ledger.md#2026-11-19--compact-qkv-cuda-development-screen-at-three-widths)
  [Approved development screen](arbiter/final-plan.md#approved-windows-compact-cuda-development-screen)
  For judged task quality, the original local TechQA DEV archive
  provides 77 provisional answerable cases with complete archived
  menus, publisher offsets and no menu overlap with original TRAIN
  gold IDs. The blind ContextBench complement supplies 14 provisional
  cases across eight literal repository clusters. Exact shared menu
  documents connect the 77 TechQA cases into 33 groups. Check wider
  source exposure and native-visible spans, freeze paired cluster-aware
  power and obtain additional independent judgments as needed before
  confirmation.
  [Dependency audit and confirmation gaps](arbiter/software-v2-data-audit.md#techqa-menu-dependence-and-confirmation-readiness)
  The approved supplemental screening roster contains 25 IBM Cloud
  and 65 AWS technical-support candidates plus 50 MultiLang repository
  candidates. Preserve their separate identities. Check source rights
  and family/model exposure, join native-visible support spans, obtain
  independent sufficiency judgments where needed, and freeze complete
  paired menus and long-context coverage before admission.
  [Supplemental recruitment outputs](arbiter/software-v2-data-audit.md#supplemental-90-and-50-candidate-roster)
  Source validation matched 76 original TechQA answer spans, resolved
  all 60 publisher pointers for 25 IBM Cloud cases, mapped 41 unique AWS
  ranges, and found 39 MultiLang candidates with the scoped chronology,
  root-license and known-overlap checks. Preserve
  refusals. Complete the remaining source and human reviews and
  capture exact native-visible evidence before scoring a judged arm.
  [Confirmation source validation](arbiter/software-v2-data-audit.md#source-validation-results-for-the-confirmation-candidate-pools)
  Replacement screening yielded 20 passes across 32 repositories. Keep
  the first 11 passes with the original 39 and the other nine as reserves.
  Of those 50, 35 have bases later than the issue and need pre-fix review.
  All 485 repeated AWS occurrences are mapped. Exclude the 24 affected cases
  from the active comparison per user direction. The publisher CSV
  provides no occurrence offsets. The 98% comparison remains unmeasured.
  The remaining 41 AWS cases have an authenticated private corpus. The 76
  verified TechQA cases have their complete ordered publisher menus frozen.
  Git parent paths place all 35 later repository bases before the linked merge.
  [Active source cohort](arbiter/software-v2-data-audit.md#active-source-cohort-after-the-aws-exclusions)
  [Replacement screening and repeated AWS answers](arbiter/software-v2-data-audit.md#replacement-screening-and-repeated-aws-answers)
  [Local confirmation-source inventory](arbiter/software-v2-data-audit.md#initial-context-fp16-confirmation-source-inventory)
  [Stock FP32 projection comparison](simplify/one-process-ledger.md#2026-11-19--stock-cuda-fp32-projection-is-a-compact-candidate)
  [FP32 projection output comparison](simplify/one-process-ledger.md#2026-11-19--fp32-projection-output-improves-half-rounding)
  [Full projection rounding audit](simplify/one-process-ledger.md#2026-11-19--exact-projection-rounding-rules-out-cublas-pedantic)
  [Identical-input projection comparison](simplify/one-process-ledger.md#2026-11-19--identical-half-inputs-isolate-compact-projection-rounding)
  [Independent operand comparison](simplify/one-process-ledger.md#2026-11-19--independent-compact-prefix-differs-before-attention)
  [Layer-0 capture and replay](simplify/one-process-ledger.md#2026-11-19--compact-cuda-layer-0-capture-and-identical-operand-replay)
  [Single-tile comparison](simplify/one-process-ledger.md#2026-11-19--single-tile-flash-reference-still-misses-compact-t32-raw-parity)
  [Corrected T32 execution and numerical blocker](simplify/one-process-ledger.md#2026-11-19--corrected-compact-fp16-cuda-forward-settles-and-matching-raw-parity-fails)
  Leave Focus GPU off the critical path. The 25% plus one-second warm Answer p95
  improvement target and the 2 GiB discrete-VRAM reserve are approved for
  release confirmation where applicable. [Accepted FP16 rule](arbiter/final-plan.md#accepted-context-fp16-task-quality-qualification)

## Scoped ownership exceptions

- User exception approved during implementation: reviewed in-scope tracked-file
  content edits may proceed while preserving each existing owner and ACL exactly.
  This does not authorize writing foreign-owned ignored artifacts, caches or
  private state, or changing ownership/access policy.
- Read the final 17 November restart checkpoint and subsequent entries in
  docs/simplify/one-process-ledger.md before resuming. The permanent checked
  execution helper is implemented and reviewed as scripts/dev/assert-human-execution.ps1. Dot-source it through the
  verified human shell, call Assert-AirkOutputParent before writes and
  Assert-AirkCreatedOutput before consuming outputs. Enumerate child/cache outputs;
  the guard is a read-only metadata preflight, not atomic confinement.
- The user explicitly authorizes repairing foreign ownership in this repository
  to TYAI\tdotn, SID S-1-5-21-3678370870-3858642417-826968105-1002. The user is
  handling existing owner/access repair through the Windows UI. This supersedes
  the earlier ownership-preservation restriction for that repair only. No
  alternate account, new sandbox account, global permission change or agent-led
  access-policy workaround is authorized. Reobserve affected metadata before
  consuming historical custody; retain the original receipts unchanged.
## Build targets and private state

- Retained training data, checkpoints, calibration/reference inputs and receipts
  under artifacts/ are not disposable Cargo caches. The current Cargo target is
  target/; an explicitly selected artifacts/v190/p10-target clean affects that
  target only. Never delete all artifacts/ as a build cleanup. Preserve exact
  research outputs and use reviewed build-output cleanup when space requires it.
- Build target for this execution: target/ (human-owned checkout target).
  The user requested Cargo cleanup on 19 November due to low D: space. The
  target was removed after the retained build and diagnostic receipts were
  verified. The next necessary Rust build will recreate it. Preserve the
  designated foreign-owned artifacts/v190/p10-target root and keep one Cargo
  owner per target.
- Mac Neo: existing SSH setup, POSIX sh batches, checkout ~/code/airk;
  connection details in ignored artifacts/v190/neo-connection.local.json.
  Verify the host's logged-in user before any batch. Preserve evidence outside
  its target and clean that target after each batch, including failures.
- Private state includes temp/, .airk/, artifacts/one-process/native-fixtures*/,
  artifacts/editor-qualification/, artifacts/advanced-split/models/weights-v2/,
  artifacts/v190/final-plan/ and any fixture/state/cache/data roots under
  campaign artifacts. Never stage or package private state, credentials,
  indexed source copies or training examples. Verified distributable outputs
  belong in separate fresh package directories.

On 20 November UTC the user approved binding exact graph, optimizer settings and measured GPU/CPU placement for Windows Compact, Windows Large, Linux Compact and Linux Large wherever mixed execution is needed. Each model and OS requires independent qualification. The versioned service-wide allow_gpu setting defaults to enabled for existing configurations. Both decisions are recorded in the final plan.

### Windows Compact memory pressure, 20 November 2026 after 11:02 UTC

The A6000 reproduced all eight original 8192-token score words at both measured availability tiers. The 24 GiB call took 1.872 s and both Jobs exited with zero members (`compact-capacity-protected-24-gib-pressure-fit-r1/execution.json`, SHA-256 `dd82bcac500fb4734dfeaa170dd31a2bcb4f6f710b3acbe53d0cb6ecc98dc0f1`). At 16 GiB the synchronized call took 1.899 s. The actual process/aggregate host cap was 12 GiB, the measured peak was 7,747,698,688 bytes and all 120 ballast allocations were released. The parent originally rejected the unprofiled stdout after exit. The offline reconciliation reopened the original stdout, device and host grants, immutable score controls and native journal. It verified the candidate PID, original deadline, typed graph and literal score words. The issued receipts remain `140ec1cc6a99838c2af21a3940c2dfa5f671fb62fff9c28009833a26189b42a6` (ballast) and `a8931ef78662c4ac00e3d97ffe59e68242e4053a5345dd10826f05b017cbdaae` (native). The separate reconciliation is `compact-capacity-protected-16-reconciliation-r1/execution.json`, SHA-256 `cda1dfea8e03310c1286ccd78ede74928388e59e6ac63fd39c457ba27c8d22d7`.

Both tiers showed 79,691,776 bytes of process shared GPU memory, also observed in the graph-only and unconstrained controls. At 16 GiB the samples overlapping the forward interval recorded 6,706,245,632 local bytes and 79,691,776 nonlocal bytes. Allocation-level paging remains unresolved, so neither tier qualifies for a dedicated-memory or fixed20 capacity claim. The remaining original 12, 8 and 6 GiB probes also ran. The 12 and 8 GiB cases reproduced all eight words in 1.903 s and 1.896 s, with the same 79,691,776-byte shared/nonlocal peak and no dedicated-only admission. At 6 GiB the native model returned exact words in 2.742 s, but 106 ms into its forward the ballast shared counter rose to 211,812,352 bytes and the continuously resident envelope failed. The separate native Job exited cleanly. The 6 GiB ballast log ended without an original natural-release record. None of the five tiers qualifies for the candidate-floor fixed20. The referenced capacity report records all original receipt identities and limitations. The 16 model-free controls cover the tightened fit-only Job cap, original OS reserve, checked native grant, sizing handshake, unprofiled-output regression and positive nonlocal observations when the shared counter reads zero. See [capacity observations and reproduction](../scripts/cuda-component-execution/CAPACITY-SWEEP.md).

### Windows and Ubuntu checkpoint, 20 November 2026 at 11:02 UTC

Windows Compact passed the four development widths up to 8192 tokens. Its 32-card matching-reference raw errors had median 0.00001067, p95 0.00015640 and maximum 0.00018311. The 20 warm full-width model forwards had p95 1.7721 s. The original first 56 calibration requests closed, the independent compiler reproduced all 56 inputs, and seven CUDA jobs returned all 56 score vectors. In the first 16 requests, every CUDA winner matched the independent matching-recipe winner. First eight also matched the original publisher winners. Complete the independent CPU panels and preserve final lifetime closure before fitting any rule. The R5 capture reached 72/100. No production rule or paired Answer-quality result has been established.

Windows Large passed the 32-token full-model test with direct FP32 intermediate calculations and its original half-stored weights. Matching raw error 0.00003052, original publisher raw error 0.00003195, both winner 1. The predecessor with half-rounded intermediates exceeded the 0.007 release limit. Continue the 128-, 1024-, 8192-token and exposed near-tie controls on the new graph. Windows CPU Focus separately completed formal original and matching comparisons on 14/14 cases and 20 full-width resource samples at 1024 tokens. CUDA product arithmetic, complete installed latency/fallback and the 98% paired evidence gate remain open.

Ubuntu passed its installed-service GPU opt-out control and frozen-source compiler check. Linux CUDA model qualification follows Windows. The Ubuntu VHD is on C: and stayed at 79086747648 bytes through the first host alert. C: crossed the original 24 GiB growth allowance at 10:24 UTC with 2644164198400 bytes physically free. Ubuntu is stopped. Keep C:-backed writes held, retain the original baselines in host-volume-watch-r2.jsonl, and use D: for the Windows campaign. Starting WSL may provision its separate 16 GiB swap file on C:.

### Windows calibration and release-path checkpoint, 20 November 2026 at 11:21 UTC

The original Compact collection closed ten service batches (80 of 100 questions). The first 56 native CUDA observations remain complete. Independent matching CPU arithmetic covered the first 32 questions (224 scores) with 32 matching winners and maximum raw difference 0.0003395081. The publisher CPU reference covered the first 24 with 24 matching winners and maximum raw difference 0.0003929138. All are development measurements. The active production rule count and paired Answer-quality outcome count are both zero.

Large at 128 tokens passed the matching-reference control with raw difference 0.0000116825104. Original publisher raw difference was 0.0000171661377 and both reference winners agreed with CUDA. The original native receipt is b5a0d90cfa65aa67f446ffd81d4a273515450b43216ab69afe433d0c59397256. The next shared CPU reference slot is assigned to Large at 1024 tokens. The Windows installed Answer and latency collectors passed 20 focused tests. Actual installed comparisons require qualified models and the managed campaign packages.

The single D: offline Cargo home resolved the real locked dependency tree. Its original receipt 25cc58193438fbef6882cedc7640ef6111cc83452c5ae40b1f5d2c7024f3d72a records 294797312 allocated upper bytes and an unchanged original C: Cargo database and locks. All Windows target producers share it serially. Ubuntu remains stopped under the existing host budget. An independent source audit reproduced the single historical configure-guard hash from the retained Git object. Original stock CUDA reconfiguration requires the historic C: root and an expired original cutoff. A fresh D: CUDA build would need a new build identity. The original product manifest and production arithmetic decisions remain open.

### Windows and Ubuntu checkpoint, 20 November 2026 at 12:12 UTC

The R5 service stopped with 98 complete questions. Question 99 has an Answer and four of its five follow-up probes. Question 100 was never issued. The service, Job and exclusive installation lock closed. No complete 100-question capture exists at Focus 1024. The 40 Compact requests with both independent reference panels had 40 matching winners. They support development comparisons only. The next calibration step is a timed comparison of the last four original questions on the one retained installation. Preserve the frozen 120-second request and 125-second admission requirements.

Large passed at 32, 128 and 1024 tokens. At 1024 the maximum matching error was 0.0001831055 and both reference winners agreed. A two-part CPU run reproduced all eight uninterrupted 1024-token score words. Independent 8192-token reference computation is proceeding under the same per-Job limits. Compact returned all eight expected 8192-token scores at the five pressure tiers. The 6 GiB ballast lost residency during the forward and the other tiers showed unresolved nonlocal memory. No dedicated-only capacity floor is admitted.

The verified Windows CPU runtime build finished. The stock verifier passed again after reclaiming 1467202536 bytes of compiler intermediates. The original D: Cargo target and offline cache are shared serially. Ubuntu remains stopped with an unchanged 79086747648-byte backing file. Its separate default swap file is absent while stopped. Linux CPU references are prepared on D:. Native Linux CUDA qualification, installed Windows and Linux Answer latency, the operative release calibration and the 98% paired evidence gate remain open.

### Windows campaign checkpoint, 20 November 2026 at 12:49 UTC

Windows Compact and Large both passed the matching raw limit of 0.007, mass and winner checks at all four development widths. Large at 8192 had matching maximum 0.00038146973. The original seven-option near-tie remains under examination. Independent width controls do not complete model fit, calibration or paired evidence qualification.

Two timing passes on the original last four CAL questions completed eight independently granted Answer windows. The observed tail requires 133.14 seconds beyond the original three-hour capture allowance. The 98 complete original R5 questions remain development evidence only. A prospective three-hour-fifteen-minute fresh capture awaits a decision. Commit ee56afc2 identifies the readers and ledger.

The stock Windows 1.8.207 service completed 15 real installed device-policy events. It restarted the same staged runtime, found its original index, retained model selections across both global GPU setting changes, refused an unqualified activation, accepted explicit same-model CPU selection and stopped naturally. The new private fixture used 114688 allocated bytes and made zero runtime copies. Receipt fb150732e567f830321423ef69f5c5c8b42affebd17d0de433b2ddbca9c0a64b records a settled Job with exit zero. The recorded audit script SHA 8f0c2bb73e859b6426fda1aea4c7ca8d11d9e1d99c0a4f09ff4e4cd68eea0434 includes a trailing blank line. The normal Windows installer artifact, actual calibrated fallback and installed latency are separate open items. Ubuntu remains stopped under the C: growth hold.

### Extension restart checkpoint, 20 November 2026 at 13:54 UTC

The user requested a stop for an extension restart. No campaign Job, CUDA build, service, ballast, Cargo target owner, WSL session or host-volume watcher remains running. Preserve the uncommitted pressure, R6 raw-score and AWS observer work. Resume only on new direction. The Windows and Linux FP16 releases are unqualified. The 98% held-out paired comparison remains unmeasured.

The user raised the C: backing volume growth allowance to **120 GB (120000000000 bytes)** from the original free-space baseline 2669960904704 bytes. Windows work stays on D:. Budget and floor on D:, the 100 GiB physical floors and original separate guest/VHD growth checks remain. The revised monitor ran, validated the original alert and both original baselines, and closed for restart. Last r3 log SHA f9d6743c267340da098fa239c7931238d3b6c10c966dcd0017ecb1f0ef4885e1. Before starting writers again, use watch-campaign-volumes.ps1 with a fresh host-volume-watch-r4.jsonl, the original Windows baseline 1233246265344, D budget 195421011968, C budget 120000000000, floor 107374182400, cutoff 17:10 UTC and -RevisedBackingLogSha256 9cfbf2ed1877be97f859d5a5a696a3aa9aa90dcb06989f477c642e75fa452cda. Ubuntu 24.04.4 actually booted as UID 1000, with unchanged 79086747648-byte VHD and zero of 16 GiB guest swap used. It then stopped naturally before R7 compilation. Ubuntu boot/stopped receipt 0bf665a9d2ce6d19c68eb7c47f4f7b191b45306089591c7abee0749f23aabe12. Resume independent GNU CPU work with the distinct C and guest allowances. Linux CUDA follows Windows.

Windows Compact/Large passed 0.007 matching raw at all four widths. Fixed20 model p95s were 1.7721 and 4.793535 seconds. Large native cold+21 finished in 115.850005 seconds, combined custody and native run exceeded the original combined limit at 153.544792. No dedicated-only floor established. Large pressure native image and operand passed but none of its five capacity trials ran. The debug Windows CPU installer built locally; its unsigned artifact was never executed or published.

R6 started on the optimized native Windows service with the original three-hour clock. It stopped before its first question when the native catalog selection revision was 8 and the retained initial scope required 4. Its independent scope identity had also changed. Preserve the guards and authenticate the new post-restart selection, then use the genuinely closed R6 as the next physical predecessor. R6 produced zero questions; original R5 stays 98/100. Native lease recovered, outer exit 1/no forced termination/no remaining members, mutex released. Original outer c29f7db5040ef9d4face6a6fea6365de36aaf51adf8ad940bb6f8cb39fd359db, release d976fbe1a642530b6e57ef470a44827cb89df7d0bf03ddeb12a04b086543c74e, custody c805f6228ad84ec4aff40b9699231be83507a40fe78d88ec16a3922000db543f. Search optimization seven-case timed equivalence and 28 scores are development controls only. Fresh D CUDA producer R1 stopped before configure with an inherited two-process cap; corrected signed supervisor 542a7a89 passed genuine negative/positive process-tree controls. Its R2 has not launched. Preserve the original 13:44 start and 16:44 UTC build cutoff.

AWS R5 verified all 27 positive files and 41 eligible source ranges in the original publication. It observed 15800 files through one service cursor before fresh brokerage was required. R5 206021 checked immutable entries remained intact. The separate AWS R2 witness was stopped on request before native census; original owned 11c6d55c5c79a8c9bf129a6bb1b4946590febbafe1bcc8687bbc099be2862bad, zero surviving children. Resume with a fresh witnessed census and pretested whole-guide queries. The 24 ambiguous selectors remain excluded.

### Live resume checkpoint, 20 November 2026 at 15:58 UTC

R7 calibration resumed under the original common clock with plan SHA
`c04be2262a32d8318f9aa222e73ea9bf078ad80d7975d907eebefe486895a2eb`.
The first R7 lifetime closed naturally, and its exact native compiler
replayed all 56 captured inputs for both Context models with zero forwards.
Reobserve the second native lifetime and eventual outer terminal before
opening the full population. The 16:59:12.847 UTC common deadline includes
the original setup debit. Inspect the R4 volume watcher before starting
writers and retain the original cutoffs.

The first 56 Context Compact raw GPU vectors settled in seven clean owned
batches. Their exact receipts, original inputs and zero-forward compiler
receipts are in the ledger. Independent protected and publisher comparisons
still need an R7-aware reference reader and affinity admission. Reobserve
the full capture and original terminal before projecting the remaining
population. The original CAL R5 rows remain historical and R6 has zero
questions.

The CU18 R2 build is settled and failed after an MSVC helper prevented the
original configure tree from closing. It retained the generated graph and
original failure custody. A reviewed direct-Job preflight can reopen that
evidence under the original 16:44 UTC cutoff. It has no active compiler.
GNU CPU attempt 4 passed 173 selected tests and skipped 163 with an
unchanged signed source census, a verified private Nextest store and no
remaining Cargo processes. The attempt 3 census failure remains a separate
historical receipt. Linux CUDA qualification follows Windows.

The R7 CPU references need an affinity check that authenticates the new
service image before inference. Historical serial timings put the final
44 dual-reference rows beyond the original cutoff at the observed capture
pace. A concurrent two-role run would need separate memory admission and
disjoint observed core placement. Neither release is ready, and no artifact
was published.

### Isolated Windows phase 11 completed, 20 November 2026 at 19:31 UTC

The same-image 8-question timing replay closed naturally with all 56 ordered
native calls matching. The 787.790-second worker was 398.896 seconds faster
than the original. Fresh setup through closure took 837.948 seconds. Treat
this as sufficient timing evidence and proceed to the next full Windows
capture. Use the newest timing retirement linked in the ledger as physical
handoff evidence alongside the original R3 restart anchor. Bind a distinct
full-capture launcher and volume monitoring for the complete original
10,800-second common budget, and reserve Windows against competing heavy
work. Review completion after all four final questions and fixture shutdown.
R7 remains at 96 closed rows. The timing run has settled and no full
successor is running.

### Full Windows R8 capture completed, 20 November 2026 at 23:08 UTC

The full isolated capture closed all 100 original questions across the 56/44
native lifetimes. Both leases returned and the outer Job settled naturally.
The disk observer remained healthy. Use the [terminal receipt checkpoint](simplify/one-process-ledger.md#20-november-2026-at-2308-utc--full-windows-r8-calibration-capture-closed)
for the exact 100-row input identities. Continue with the independent R8
certificate and projection, then fitting and the remaining Windows FP16
qualification gates.

### Windows Compact full CUDA CAL panel completed, 20 November 2026 at 23:52 UTC

The R8 certificate and both original compiler censuses are complete. The
scientific Compact raw panel has all 100 score vectors. It reuses the 56
verified R7 GPU vectors and contains 44 new R8 vectors. Forty rows have
independent matching and publisher references at this checkpoint. Preserve
the completed reference batches and original timeouts. The new FP16-only
decision governs all subsequent work. See the [Compact panel checkpoint](simplify/one-process-ledger.md#20-november-2026-at-2352-utc--complete-compact-cuda-cal-panel-and-recover-16-cpu-comparisons)
for the exact pins and successor volume observer. Large needs its own input
selector on the existing scientific graph before raw scoring. Fitting and
later gates still require their own evidence.

### FP16-only CPU and GPU delivery, 20 November 2026 Denver time

The last CPU reference batch settled. Keep the completed FP32 baseline for
the final comparison. The native catalog currently has FP32 local-test
Context Compact and Large CPU profiles. Qualifying the FP16 CPU path is
part of the delivery work, including automatic fallback. Retain the 100
Compact scientific GPU vectors and the original qualification gates.

### Windows CPU FP16 operator placement, 20 November 2026 at 5:48 p.m. Denver time

The pinned 1.30.0 Windows CPU provider test build widens literal FP16 MatMul
and Softmax to FP32 on load. The check exercised no inference. Verify the
exact static AIRK CPU runtime before admitting a release artifact. Keep
the prior CPU and Large GPU arithmetic evidence attached to their original
graphs. Compact's 100 scientific score vectors remain available. See the
[CPU placement receipt checkpoint](simplify/one-process-ledger.md#21-november-2026-at-0048-utc--windows-cpu-fp16-placement-diagnostic).

### Weights-only FP16 CPU recipe selected, 20 November 2026 at 6:29 p.m. Denver time

The user selected the weights-only FP16 CPU recipe for CPU inference and
automatic CPU fallback. The CPU graph is the model's FP32 graph with its
learned embedding and projection weights read from the GPU candidate's FP16
weight file, and its arithmetic is FP32. The plan records FP16 as the stored
and delivered weight precision on that basis. Record the recipe in the
exporter contract before conversion, and keep qualification flags false until
the existing gates pass. See the
[selection ledger entry](simplify/one-process-ledger.md#21-november-2026-at-0129-utc--weights-only-fp16-cpu-recipe-selected).

### Ten delivery priorities and the approved FP16 delivery plan, evening of 20 November 2026, Denver time

The user stated ten priorities for this delivery. Support Windows, Linux and
macOS. Support NVIDIA and Radeon GPUs. Use FP16 for smaller downloads and
reasonable resource requirements. Prefer simpler and good over maximal
optimization. Choose a solution that reaches all platforms. Enhance retrieval
so a more capable agent receives the information it needs, without requiring
an advanced answer. Specialize weights without limiting existing abilities.
Examine MiniLM at low priority. CPU inference must be available whenever the
GPU cannot be used. Explore faster, lower-quality CPU options.

The FP16 delivery plan was rewritten around them that evening. Six items
amended recorded decisions, and the user approved all six at 02:33 UTC on
21 November:
[approvals A1–A6](arbiter/final-plan.md#fp16-delivery-plan-approvals-20-november-2026-americadenver).
Where an earlier section of this file conflicts with them, the approvals
govern. The user's evaluation direction of 21 November, in the next section,
retires the two bullets below that are marked so, and amends the ladder for
Large. In short:

- No weight file larger than FP16 ships. MiniLM INT8 and the retained Focus
  CPU export stay.
- CPU fallback follows a ladder: full budget, a 2,048-token budget, the next
  smaller model, then retrieval only. Every downgrade is reported. No CPU
  tier is guaranteed. The target is a processor with at least 8 cores that is
  no more than six years old, so measure the tiers at 8 cores.
- WebGPU becomes the GPU route for Radeon and macOS when its probe passes on
  the rented hardware. A platform where it fails falls back to CPU. The user
  dropped specialized code for Radeon, so do not start DirectML, MIGraphX or
  ROCm work, and do not treat Core ML as a fallback. The A6000 smoke test
  stays early and must show WebGPU within five times CUDA. The memory targets
  are a 16 GB card for Compact and a 24 GB card for Large.
- Retired on 21 November: calibration and judged task evidence established
  once per model, with other routes admitted by offline drift checks with a
  guard band and a confirmation set.
- Retired on 21 November: scoring the frozen FP32 artifact once per model
  for evaluation.
- Device memory fit is judged against a model-free control.
- GPU use is opt-in on every platform, chosen in the Service Admin
  configuration through the service-wide setting. CPU is the default route.
  The source defaults the setting to off since commit `0b5fe97c`. The
  earlier statement in this file that the setting defaults to enabled is
  superseded. A configuration that already stores GPU use as enabled is
  reset to off once, and the user is told: Service Admin shows the notice
  with "Allow GPU use" and "Keep GPU off", and the VS Code extension shows
  the reset state.
- Retired on 21 November with the paired comparison: a person reviewing its
  discordant cases. The user now reviews the judging packet of the Arbiter
  plan.
- The user authorized multiple agents and asked for the whole plan to be
  carried to completion. One agent owns Cargo on a target and one owns the
  GPU at any time. Each agent owns named files and does not commit. The
  primary agent reviews, commits and pushes every result.

The work order of that plan is superseded by the
[work order of the Arbiter plan](arbiter/final-plan-fp16.md#6-work-order).
The user withdrew an unfinished sentence from an earlier answer as
unimportant. See the
[rewrite entry](simplify/one-process-ledger.md#21-november-2026-between-0129-and-0233-utc--fp16-plan-rewritten-around-ten-stated-priorities)
and the
[approval entry](simplify/one-process-ledger.md#21-november-2026-at-0233-utc--user-approves-a1-to-a6-of-the-fp16-delivery-plan).

The user named the test hardware for proving the plan out: a rented Mac with
a 32 GB GPU and 128 GB of RAM, and an Azure V620 VM with a 32 GB Radeon card
under Windows and under Linux. Booking stays a manual user action. Rehearse
each rented batch on local hardware first. See the
[later revision entry](simplify/one-process-ledger.md#21-november-2026-between-0129-and-0233-utc--fp16-plan-revised-for-review-feedback-and-named-test-hardware).

At 03:29 UTC on 21 November the user directed the work to run to completion
and added three rules. Do not assume the existing GPU work was done right:
fix or improve it where it is not. Measure and report the paired comparison
against FP32 on each iteration, and do not stop the work at 98%. Review the
results with the user after the work finishes, and decide then whether to
proceed or try again. Push the branch after each coherent set of commits so
that nothing exists only on this machine.

The user directed that the macOS rehearsal on the Mac Neo start after the
Windows and Linux CUDA work is complete. It is step 11 of the work order. Do
not start macOS builds before then. See the
[sequencing entry](simplify/one-process-ledger.md#21-november-2026-between-0129-and-0233-utc--mac-neo-rehearsal-sequenced-after-windows-and-linux-cuda).

### Evaluation direction, 21 November 2026 (America/Denver)

The user redirected the work after the first measured results. I had reported
results from public question sets (a 100-row calibration panel and 76 TechQA
cases) as if they answered the product question. They do not, and the user
dropped that approach. The purpose, in the user's words, is to learn whether
"the 'answer' tool improve[d] substantially the results for an AI agent
tasked with a common set of ten software development instructions that one
might encounter including architectural, writing, documentation, testing,
and coding."

The governing plan is the rewritten
[Arbiter plan](arbiter/final-plan-fp16.md). Read it before any Arbiter work.
The direction is also recorded in the
[parent plan](arbiter/final-plan.md#evaluation-direction-21-november-2026-americadenver).

- **Do not start step E1 or later until the user explicitly says to start.**
  The user read the plan, corrected it and approved its main readings on
  21 November (reading 4, then readings 1, 3, 5, 8 and 9). That approval was
  not the word to start: the primary agent took it as one, the user stopped
  it before any agent ran, and said "do not execute on the plan yet". No
  service, index, scenario, harness or test exists. The other readings of
  section 9 stand as written unless the user corrects them.
- The user then passed on an outside review and the plan adopted its four
  points: the benchmark's own files are excluded from the airk index, with a
  leak probe; a turn count ends at a correct finish (evidence and brief both
  right), with the headroom reported at the baseline; every report shows
  what each model adds over the stage below it; and shared search fusion is
  held back from the first change set.
- Two new indexes are built from the branch as it builds today, for
  `D:\code\github\airk` and `D:\code\github\bookbuilder`. The user's
  running service is not used, and nothing is written into either
  repository.
- Ten scenarios in each repository: three coding, two writing, two test or
  QA and three architecture. Five results per scenario: Answer with Focus
  only, with Focus and Compact, with Focus and Large, the `parse` path and
  the `search` path. Answer configurations run on CPU and GPU.
- The agent judges with the plan's two rubrics. First priority: fewer turns
  for an AI agent to get the context for the task. Lower priority: useful
  detail for a person.
- Weights are adjusted after each test, from what the scenarios show, as
  comprehensive change sets: the set of changes most likely to improve
  results, built together and then iterated. Do not alter one thing at a
  time; the user said time is short. Do not present results from public
  question sets or calibration panels as evidence about product value.
- "Weights" and "adjustments" cover every adjustable value, including how
  Answer gathers evidence. The user confirmed this reading.
- Every series of changes and tests gets a report in
  `docs/arbiter/answer-evaluation/`, with the fixed sections its README
  lists, including what was good and what was bad. A step is not done until
  its report is committed. Reports are written for a reader who was not
  there, and quote none of the bookbuilder repository's source.
- Large never runs on CPU in production. Without GPU opt-in, Large is not
  offered and Compact is the only Context option. In tests Large on CPU is
  measured on one scenario in five with a 180-second maximum.

State when the plan was rewritten. Resume from the ledger entries of
21 November and not from this summary.

- Done and pushed before the redirection, and still valid as engineering:
  the opt-in GPU default with its Admin and VS Code notice, the Core route
  planner with typed reasons, the portable graph proof and its load on the
  static CPU runtime, the capacity rule under A6, the Linux storage policy
  and boot wrapper, and the WebGPU padding measurements.
- Retired as acceptance evidence: everything measured on the calibration
  panel and on TechQA. The stage-arm tools and the slot diagnosis remain in
  the repository as tools.
- Private runbooks and the Core design are in
  `artifacts/v190/final-plan/exec-20261116T052942Z/agent-runbooks-r1/`. The
  Windows CUDA runbook's calibration and fit steps are retired.
- Known failing or blocked checks, each with an owner in the ledger: the
  `module_cycles` boundary check in `airk-retrieval`, seven pinned files in
  the formatting quarantine, the Admin browser-teardown errors with Edge 158,
  and the CUDA capacity trials, which wait for the desktop's GPU memory
  state to recover.
- Linux: the campaign distribution is `Ubuntu` with user `tyler`; never run a
  bare `wsl` command. Platform work waits for step E9 of the Arbiter plan.
- Open user decisions: readings 2, 6, 7, 10, 11, 12 and 13 of the Arbiter
  plan's section 9 are followed as written and are the user's to correct.
  Still open from before and not urgent: the Linux guest and VHD caps, the
  Linux `install.sh` per-user pointer, and a release tag for catalog rows.
