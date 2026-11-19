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
  On 18 November the user authorized continuing past the 72-hour checkpoint
  at 10:29:42 p.m. America/Denver. Continue the campaign across sessions
  unless the user changes direction. Original T0 and per-run limits apply.
- The final plan supersedes the old advanced-split execution deadline and
  campaign ordering. Preserve prior ledger entries as historical evidence.
- For the current implementation, use this Windows machine, its existing WSL2
  instance and Mac Neo. Complete Windows qualification before beginning the
  authorized Linux FP32 and FP16 gates on WSL2. Start those gates directly
  once Windows is complete and report genuine blockers.
- The Datagram weekend has a separate manual kickoff after the Windows and
  Linux CUDA campaigns. If CUDA FP16 passes on both on this workstation, test
  FP16 only on macOS using Core ML. Azure is the sole provider for the future
  Radeon FP16 campaign, after Windows and Linux CUDA and macOS qualification
  are complete. Current execution remains local. No cloud teachers are authorized.
## Models and qualification

- Finish the current CUDA FP32 work to inform CUDA FP16, then qualify FP16 with
  efficient attention and automatic admitted CPU fallback. Keep each variant's
  identity, evidence and qualification separate. The future release decision may
  select FP16 as its only GPU precision if the measured value is sufficient.
  Radeon evaluation is deferred to that FP16 plan after successful CUDA FP16.
- On 18 November the user approved an exact 32 GiB Windows Compact host
  process cap. Use a 32 GiB aggregate Job cap so the process can use that
  allowance. The 24 GiB device budget, original 120-second deadline and all
  numerical/placement criteria still apply. Keep the earlier 16/24 GiB
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
- Build target for this execution: target/ (human-owned warm checkout target).
  The designated artifacts/v190/p10-target root is foreign-owned and remains
  preserved. Do not clean targets or share them across concurrent checkouts.
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
