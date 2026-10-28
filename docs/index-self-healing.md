# Automatic indexing recovery in 1.5.7

28 October 2026. Development behavior on `feat/manageindices`; published and platform qualification are recorded separately in [the release record](release-1.5.7.md). The [1.5.6 record](release-1.5.6.md) retains its original validation scope.

The native service owns accepted setup, scalar indexing preferences, job continuations and recovery. Closing every editor does not discard them. Management observes this work and presents its reason, attempt budget and next condition check. A usable prior publication remains subject to the existing native integrity rules; recovery never edits original source files.

## Settings and uncertain acknowledgements

Search mode and Evidence detail are accepted as bounded scalar preferences before inspection. The exact registry/project binding and accepting engine are recorded with the request. The fenced owner inspects current settings and applies that one preference; root changes, custom model selection, imported configuration, cleanup and other advanced operations retain their explicit preparation guards. Pause delays execution. Cancellation wins over continuation. A saved result settles an interrupted configuration; an unfinished scalar preference may resume idempotently under the same accepting engine and retry budget.

Model preparation precedes enabling semantic policy. Existing custom models and text-only choices remain authoritative. Source publication and semantic processing follow as durable phases. Automatic projects with verified enabled-but-disabled semantic state receive one setup reconciliation per engine/settings identity when no work or cancellation blocks it. Manual projects require accepted work; opening a query cannot silently opt them into automatic updates.

Management reconciles lost job/configuration acknowledgements through a read-only lookup of the original caller and request key. A missing or expired receipt remains unknown; it never generates a replacement request. Lifecycle and maintenance-plan requests retain their separate existing recovery contracts. Eligible metadata-lock failures receive at most two additional reads with short backoff. Mutations are not automatically resubmitted by the extension.

## Recovery behavior

| Failure or wait | Native response |
| --- | --- |
| Worker startup/exit | Persist up to three failures per exact project/runtime with backoff and bounded jitter. Preserve pending work while startup remains eligible. A successful indexing publication or explicit retry clears the applicable budget. |
| Shared helper exit/unavailable | Persist bounded shared-helper retries. Resume eligible semantic work only after a helper is available. Explicit helper Stop survives service restart. Invalid model assets and identity/protocol errors do not authorize a fallback model. |
| Missing source root | Keep the same accepted job pending. Check the registered root and configured additional roots; resume after canonical directory access returns. The worker revalidates its full source boundary before indexing. |
| Physical storage/quota pressure | Keep the exact job pending while capacity is insufficient. Use bounded native capacity metadata to decide when to try admission again; the actual reservation remains authoritative. Never increase quotas or delete data automatically. |
| Publication/contention wait | Retry eligible work within its persisted job budget. Native resource waits and project pauses do not advance the stall clock. |
| Shared quota-ledger contention | Wait at most thirty seconds for lock admission during a service job, with pause/cancellation checkpoints and a native-lock observation. Only lock acquisition retries; a ledger mutation is never replayed. Optional accounting keeps its shorter deadline. Exhaustion records `quota_ledger_busy`. |
| Repeated terminal source/recovery failures | Persist a project budget across new source hints, worker exits and service restarts. Three terminal failures stop automatic source retries. Explicit retry/index/recovery or a successful source/publication recovery clears the applicable budget. |
| Confirmed no-progress stall | Keep the existing ten-minute advancement threshold and thirty-second cooperative grace. Permit one recovery of a stalled job, with a project work budget preventing source edits from replenishing repeated recovery attempts. Heartbeats and phase-label changes do not count as progress. |

Condition checks are bounded to eight due jobs per pass, at most once per second; unavailable prerequisites are checked again after thirty seconds. Waiting does not consume another job attempt. Such active waits keep the service from declaring itself idle merely because no worker is currently executing. Removed projects and paused/canceled work cannot be released by a condition check.

Initial model setup may wait while its independent text-indexing continuation proceeds. This keeps text usable on machines with a missing or unavailable helper. Recovery of the same job keeps the same durable receipt. Existing finite queue, source-revision, cleanup, process ownership, resource admission and fencing contracts remain in force.

Recovery eligibility checks the unfinished runtime recipe (`current-runtime-bootstrap.json`) and publication transaction phase. The permanent database initialization receipt (`current-bootstrap.json`) is not unfinished work. Confusing those markers in 1.5.6 caused healthy publications to be repeatedly classified for recovery. Successful recovery, including an approved Auto Repair whose chosen action is Recover, allows outstanding source changes to be retried without marking them indexed. Pause, manual updates and explicit cancellation remain authoritative. Later successful publication/recovery supersedes old attention banners without rewriting failed job receipts.

## Management

The existing page shows Waiting for source folder, Waiting for storage capacity, Recovering indexing, or Semantic indexing needs attention. Recent activity includes the reason, attempts, next check and recovered/canceled/failed state. A stopped or exhausted helper offers Retry semantic indexing. Worker startup exhaustion retains its scoped restart action. Published search availability is separate from request acceptance and current recovery.

The registry migrates format 7 to 8 transactionally while preserving registrations, request receipts and namespace. Older supported formats migrate through the same path. Read-only saved-state inspection does not migrate or create metadata; missing initialized format-8 tables are rejected.

Recovery-stage, SQLite and filesystem codes retain known causes without storing raw exception text or source contents. Control failures include their safe code in the finite service log. Read-only MCP execution-context worker counts do not describe machine capacity; the service scheduler grants background jobs their own work allowances.

## Limits

Full rebuilds, destructive cleanup, changed source boundaries, corrupt/custom model assets and unknown failures still require an explicit decision. The optional artifact-quarantine idea in the [original proposal](index-self-healing-proposal.md) remains a separate design: ordinary attributable extraction skips already allow other files to progress, while an unattributed worker crash must not silently exclude a file.

Advanced configuration preparation still has its bounded client wait; the three everyday indexing preferences do not depend on that inspection window. Native source, helper and storage probes are local observations, not guarantees about future admission. Windows test evidence does not establish Linux/macOS or installed-editor qualification. The repeated-recovery marker bug is reproduced locally; the original other-machine two-second operation failure remains unattributed because its 1.5.6 logs did not retain the underlying cause. Quota contention is a separately reproduced failure path, not proof of that remote cause.
