# Current agent assignment

These instructions are mandatory for the active assignment. Standing rules remain in [AGENTS.md](../AGENTS.md). Historical receipts and prior decisions remain in the ledger; this file records the effective campaign scope.

All unlinked filesystem paths below are repository-relative unless absolute or prefixed with `~/`.

Active campaign: Arbiter final implementation (execution start 2026-11-16T05:29:42Z).

| Need | Source |
|---|---|
| Ordered implementation plan and D01–D08 user decisions | [docs/arbiter/final-plan.md](arbiter/final-plan.md) |
| Known issues and historical candidate evidence | [docs/arbiter/knownissues.md](arbiter/knownissues.md) |
| Weight-experiment rationale | [docs/arbiter/weights-v2-plan.md](arbiter/weights-v2-plan.md) |
| Progress ledger | [docs/simplify/one-process-ledger.md](simplify/one-process-ledger.md) |
| Existing split decisions and contracts | [docs/split/split-plan.md](split/split-plan.md), [docs/split/contracts.md](split/contracts.md) |
| Standing simplification decisions | [docs/simplify-plan.md](simplify-plan.md) |

## Authority and deadline

- Branch: feat/arbiter-final, created from feat/advanced-split. Commit locally; no push to this new branch, PR,
  merge, publication or deployment is authorized.
- Current task: implement all steps of docs/arbiter/final-plan.md, authorized
  by the user on 15 November 2026. Execution T0 is recorded in the ledger.
  The user authorized continuing past the earlier 72-hour checkpoint.
  The current 48-hour FP16 deadline is **Friday, 20 November 2026 at
  9:27 p.m. America/Denver** (`2026-11-21T04:27:11Z`). It runs across the
  extension restart and includes time spent closing the Windows Large FP32
  prerequisite. Original T0 and per-run limits apply.
  [FP16 schedule](arbiter/final-plan.md#5-time-budget-and-unattended-scheduling)
- The final plan supersedes the old advanced-split execution deadline and
  campaign ordering. Preserve prior ledger entries as historical evidence.
- The Windows Large FP32 CUDA prerequisite passed on 18 November. Qualify
  efficient FP16 on Windows CUDA first. After Windows FP16 passes, start FP16-only
  Linux CUDA qualification directly on the existing WSL2 instance and
  report genuine blockers. Linux CPU checks continue independently.
- After both CUDA FP16 gates pass, qualify FP16 only on macOS using native
  Core ML. The Datagram weekend has a separate manual kickoff. The future
  Radeon FP16 campaign uses Azure exclusively after those three gates pass,
  in order of Windows DirectML and Linux ROCm. The current execution remains
  local. No cloud teachers are authorized.
## Models and qualification

- Retain the Windows FP32 implementation and measured evidence to inform FP16.
  Keep each variant's identity, evidence and qualification separate, and
  qualify efficient attention and automatic admitted CPU fallback. The release
  goal is FP16 as the sole GPU precision if it passes the measured quality,
  performance and memory gates on target developer machines.
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
  2048-token/30-second evidence remains historical; Focus's graph remains
  512 tokens per pair unless separately re-exported and qualified.
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
## 18 November extension restart checkpoint

The FP16 deadline is Friday, 20 November at 9:27 p.m. America/Denver.
Use the existing deadline when resuming in a new context.

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
  Keep GEMM2 as the baseline. Check whether retaining FP32 projection
  outputs through accumulation reduces the discrepancy with the same
  half operands. Matching raw parity still gates wider Windows scoring.
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
