# Durable service maintenance plans

The opt-in 1.4 service now accepts native maintenance plans through its authenticated control endpoint. The extension console and default client cutover remain under implementation. This is local index maintenance; the service never supplies agent reasoning or carries retrieval payloads.

## Preview and approval

`plan_create` accepts `verify`, `recover`, `reconcile`, `vectors`, `rebuild`, or `auto`, with one/selected projects or all currently registered projects. Selected requests contain at most 256 distinct bindings. All-project membership is frozen at acceptance, with at most 1,024 members across four retained plans. Later registrations do not silently join an approved operation. The same accepted request key returns the original plan, including across restart; changing its request fails.

Planning queues distinct read-only inspection jobs. A preview binds sorted project membership, proposed actions, executable/pair identity and captured incarnation, publication, settings and recovery guards. Auto proposes captured recovery when needed, an explicitly confirmed fresh rebuild when the index is unavailable, and source reconciliation otherwise. Missing sources/settings produce explicit skipped members where the action needs them. Recovery failure never silently substitutes live sources or changes the approved operation into a fresh rebuild.

`plan_items` returns at most 32 items and 48 KiB per page. Each item carries its inspection, action, fallback requirement and eventual receipt. Inspection storage summaries disclose current logical use/reservations, limits and point-in-time physical headroom. The replacement estimate uses existing charged collection bytes plus a fresh database/WAL allowance; it excludes unknown changed-source growth. It is an estimate, not an operating-system reservation. Actual writes retain existing durable quota and sidecar admission; planning does not double-reserve those allowances.

`plan_execute` requires the exact preview digest. A proposed destructive fallback additionally requires explicit approval. Workers acquire the normal exclusive project lease and must match the approved engine, incarnation, publication and settings again before executing. A changed guard produces `maintenance_plan_stale`; a newly unavailable source produces a skip. Unrelated project owners continue under service-wide resource admission.

## Execution, cancellation and recovery

The metadata actor dispatches at most four candidates per plan and 16 operations per turn, without opening indexes. Project workers execute the existing bounded verify/recover/reconcile/vector/rebuild paths. Plan jobs do not join another client's or another plan's work, so plan pause/cancellation cannot stop unrelated requests.

`plan_pause` stops new admission and pauses running work at existing safe checkpoints. Planning has a ten-minute admission/inspection budget, excluding explicit plan pause. Failed inspections retry at most three times with finite backoff. Typed quota, filesystem-headroom and storage-full failures use at most three action attempts with ten/twenty-second backoff. Every retry revalidates the original preview. Source changes captured during a repair remain independent durable desired work.

`plan_cancel` persists intent, cancels queued owned jobs and signals running owners. Atomic commits remain indivisible. Restart resumes accepted plan metadata and safely queued work. Ambiguous interrupted repair actions fail with a retained receipt instead of being silently replayed. A matching durable rebuild-switch receipt can establish that publication succeeded; it cannot prove retirement completed.

Items preserve bounded fenced progress independently of ordinary job/progress retention. Cleanup is recorded from this action's actual observation. An old cached sample cannot certify new cleanup; a recovered rebuild without that proof remains cleanup-pending. Results distinguish succeeded, failed, stale, skipped, canceled and cleanup-pending members. Partial failure is never reported as complete success.

Only finished plans rotate. Rotation advances the shared request epoch before acknowledging another plan, so a deleted receipt's old key cannot repeat maintenance. Active plans are never evicted to create capacity. Source trees, saved settings and unknown directories remain protected by the existing owner-verified storage contracts.

## Validation

Windows and Linux passed all 13 native service process tests, 45 service storage regressions and physical-capacity checks. Reviewed maintenance regressions additionally exercised restart/stale settings, successful verify/reconcile/rebuild, bounded independent-plan dispatch and retained progress after job rotation. Windows workspace all-feature Clippy and Linux lightweight CLI Clippy passed; repository checks covered 901 public files, and the Python suite passed 129 tests with four platform-specific skips. Linux review image: `sha256:7e7302263928ecb087f6ae207c05e17f5f3fe3bb9b4986fd5aa46315c063cfec`. Installed-editor acceptance and the complete maintenance fault/qualification matrix remain open in the [completion checklist](service-implementation-checklist.md).
