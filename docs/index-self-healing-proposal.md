# Proposal: bounded automatic recovery for indexing

27 October 2026. Original design proposal, retained as design history. The user subsequently authorized implementation in 1.5.6 on the same branch. See [implemented automatic recovery](index-self-healing.md) for the actual contracts, limits and validation links; the optional artifact-quarantine idea remains deferred.

AIRK should recover common temporary failures without requiring users to know about workers, helpers or inspection jobs. Recovery belongs to the native service and must continue without VS Code. The default experience should be one status and one useful action, with the last usable publication retained whenever native integrity rules allow it.

## Existing foundations

The native service already owns durable jobs, request keys, source/semantic continuations, worker identity/fencing, model selection and publication identity. Worker/helper startup has bounded retries. Source watching records requested/completed revisions, retry exhaustion and canceled revisions; root traversal retries can observe restored access. Pauses, explicit helper stop and project removal have distinct meanings.

The 1.5.6 work adds durable initial setup, model preparation before semantic enablement, and automatic cached-inspection refresh after successful publication work. These address incomplete setup and stale displayed state. They do not make every native failure self-healing.

The remaining gap is visible in the implementation: [project supervision](../crates/airk-cli/src/project_supervisor.rs) keeps worker/helper retry counters in memory and stops automatic startup after three failures. [Background continuations](../crates/airk-store/src/service_background.rs) persist the next phase, but do not explain or repair every exhausted prerequisite. [Source watching](../crates/airk-store/src/service_watch.rs) separately preserves exhausted and canceled source revisions. Extend these contracts together rather than creating a competing recovery mechanism. The reported remote startup failure's underlying cause remains unknown.

## Remember the requested result

Treat Search mode, Updates and Evidence detail as durable user intent. Track requested settings, applied settings and published coverage separately. A bounded native reconciliation pass should compare that intent with verified observations and either join existing work, schedule one missing authorized phase, wait for a prerequisite, or report a specific intervention. Run it on relevant transitions and occasional bounded checks, not by repeatedly enqueuing jobs from UI polling.

For example, Text + semantic with an available text publication but no semantic coverage should explain whether model preparation, source publication or embedding is outstanding. If an accepted continuation was interrupted, finish that stage without asking the user to discover Install model, Inspect and Index now in the correct order. Do not infer unfinished intent from a stale cached inspection: first verify current settings, publication, jobs and cancellation markers. Manual updates permit completing accepted work, not starting new source updates indefinitely.

## Proposed recovery policy

| Observation | Automatic response | Stop condition / user decision |
| --- | --- | --- |
| Metadata lock contention or disconnected UI | Retry the bounded read with backoff, then refresh the exact project's status. Show the previous sample's age meanwhile. | Authentication, protocol or ownership mismatch; do not guess another endpoint. |
| Unknown mutation acknowledgement | Look up the original request receipt and reconcile the outcome. Add a read-only receipt lookup where needed. | Expired or missing receipt, changed registration, or an unknown effect. Never create a replacement request key automatically. |
| Missing source directory or temporary access failure | Periodically check the same registered root. After access returns, request one reconciliation for automatic mode or resume already accepted work. | Changed canonical identity, redirected root or protected boundary. Never adopt another folder. Manual mode must not acquire unsolicited new source work. |
| Quota pressure or full disk | Wait without consuming repeated job attempts. Recheck bounded capacity observations; resume when native admission can reserve the required space. | No automatic quota increase, data deletion, cache purge or storage migration. Explain what capacity is needed and what is unknown. |
| Helper exits or transient model-load failure | Restart only the verified owned helper, reload the same verified model and retry eligible semantic work with a small budget. Keep published text usable. | Explicit helper stop, model checksum/path mismatch, repeated failure, or memory pressure that remains unresolved. Never replace a custom model silently. |
| Worker exits or startup temporarily fails | Reacquire the exact project owner and recover native captured work, with exponential backoff and a persistent per-episode budget. | Repeated crash with the same cause/runtime, ownership ambiguity, or unavailable storage. Never restart all projects to repair one. |
| Worker connected but no real advancement | First distinguish pause, scheduler/resource wait and legitimate expensive work. On confirmed stall, request cooperative cancellation; after the native grace period, restart the owned worker and recover captured work once. | No automatic restart based on heartbeat age alone. Stop after repeated no-progress recovery. |
| Settings prerequisite is still queued | Keep the user's bounded desired setting as a native durable request and complete inspect → guarded apply → source → semantic stages. | A stale settings/engine guard or conflicting later user choice requires review. This closes the current pre-acceptance preparation timeout gap. |
| Missing default model while semantic mode is requested | Continue the accepted setup operation using verified bundled assets, preserving any existing custom selection. | Invalid/custom assets or repeated provisioning failure. Show text availability separately. |
| Settings request remains incomplete despite no running job | Reconcile the durable request against current model, publication and continuation state; join or schedule only its missing authorized stage. | User cancellation, superseding settings or an exhausted unchanged failure must not be overridden. |
| Index integrity/recovery failure | Run bounded metadata verification and only the existing native recovery path when its captured state and guards authorize it. | Full rebuild, destructive cleanup, changed roots, missing captured evidence or broken integrity require an explicit reviewed plan. |

## One native recovery record per episode

Use the existing registry and owning service components; do not introduce another scheduler, external database or extension polling loop that controls work. Key the bounded record by project, operation, requested source revision, settings/engine identity and failure class. Store original job/receipt, attempts, next eligibility, first/last failure and last verified advancement. Retain a capped transition history, not repeated polling logs or source contents.

Budget proposal: at most three startup/helper attempts with exponential backoff and jitter, and one stall-recovery attempt per unchanged source/configuration episode. Use a longer cooldown after exhaustion. Do not reset the budget merely because VS Code or the service restarts. A material condition change can reopen eligibility; repeated unrelated events must not erase evidence of a persistent fault. Exact budgets should be chosen from fault-injection measurements, not treated as already qualified defaults.

Also cap repeated failures across episodes for the same project/runtime and failure cause. Otherwise a continuously edited file could create new source revisions and bypass the crash-loop limit. A cooldown permits another condition check; it must not by itself grant unlimited retries of the unchanged operation.

A paused project stays paused, a canceled revision stays canceled, a manually stopped helper stays stopped, and an unregistered project cannot be resurrected. Cleanup barriers and retained revision/source boundaries remain authoritative. Shared-helper recovery is coordinated once across projects, with isolated outcomes for each collection.

Internal recovery interruption must have a distinct reason from user cancellation. Before a replacement attempt starts, drain or fence the old owner and recheck user intent atomically; a concurrent Cancel, Pause or Remove wins. Runtime/protocol incompatibility should offer the existing reviewed runtime repair path rather than repeatedly restarting incompatible executables.

Recovery success requires evidence: the replacement worker has a new valid fenced identity, the intended job makes actual progress, and the appropriate publication or semantic readiness advances. A successful restart RPC or process heartbeat is insufficient. Keep source freshness, text availability and semantic coverage separate throughout.

## User experience

Use the existing health banner, not another Management tab or set of knobs:

- Waiting for source folder. AIRK will check again; the saved index is available.
- Waiting for disk space. Indexing will resume when storage admission succeeds.
- Recovering semantic indexing, attempt 2 of 3. Text search is available.
- Recovered after 54 seconds. Expand Recent activity for the cause and action.
- Needs attention after repeated failure. Show one specific next step and the last native reason.

Offer Retry now only when useful, plus existing Pause and advanced diagnostics. A bounded transition history should explain what AIRK tried, why it stopped and what would allow another attempt. Do not claim search is ready just because recovery was accepted.

For the reported experience, the intended sequence is: "Semantic setup queued" → "Preparing model" → "Building semantic index; text search available" → "Text and semantic search available." A temporary helper failure inserts "Recovering semantic indexing, attempt 1 of 3" into that sequence. A persistent failure ends with "Text search available; semantic indexing needs attention" and one relevant action. It never falls back to an unexplained Disabled label.

## Later option: isolate a repeatedly failing artifact

If evidence reliably identifies one unchanged file as the cause, consider a bounded retry exclusion keyed to its content hash and extractor version so unaffected files can progress. This needs a separate partial-publication contract and precise attribution; a worker crash alone is not enough. Show the affected path and missing/stale coverage, retry when the file or extractor changes, preserve original source bytes and never label incomplete coverage fully ready. Defer this until the basic recovery policy is qualified.

## Delivery and validation

1. Add typed recovery reasons, read-only receipt reconciliation and durable desired-setting preparation/reconciliation. Keep unknown failures manual and preserve current guards.
2. Add condition-based source/storage recovery and bounded verified helper/worker retries, using persistent episode budgets.
3. Add stall recovery only after injected long-running extraction, pause, contention and pressure cases reliably distinguish waits from dead work.

Exercise faults before/after acceptance, model capture, source publication and receipt acknowledgement; restart the service and close every editor between stages. Include missing/restored folders, redirected roots, disk exhaustion, corrupt/custom model assets, helper exit, worker crash, repeated stall, simultaneous projects, pause/cancel/remove during recovery and expired request receipts. Assert no duplicate effective mutation, no cross-project work, preserved source bytes, no silent model change, stable old publication when valid, and a bounded number of recovery attempts.

Record local measurements for time to verified recovery, exhausted attempts, repeated failures and false stall detections. No remote telemetry is required. The main acceptance criterion is that an eligible temporary failure reaches the requested usable state without another user click, even with every editor closed; an ineligible or repeated failure stops with an accurate reason and one actionable next step.

Qualify Windows, Linux and macOS separately. Begin with fault-injection tests and measured transitions; do not enable destructive fallback as a convenience. The original other-machine startup failure still needs its native diagnostic cause before attributing it to a specific platform defect.
