# Guarded project retirement

Development work for 1.4.0. Unregister and deletion of derived state are separate operations. This contract does not authorize automatic cleanup of 1.3 state or unknown directories.

## Unregister

The trusted console's project settings offer **Unregister project**. Native CLI clients prepare the same configuration with `configure --stdin` and `{"operation":"unregister"}`. The ordinary inspection/configuration guard binds the exact project, incarnation and settings before a durable request is accepted. The console presents the host, collection, source, data location and effect before dispatch. Lost trust, declined approval and stale native guards prevent submission or execution.

Preparation does not unregister a project. The registry commits successful completion, the final configuration result, disabled watching, canceled queued project work, cleared continuations and inactive registration in one transaction. A crash after recording preparation resumes this metadata transition under a new proven worker fence; it cannot recover a successful job while leaving registration active. A missing preparation result rolls the completion transaction back.

Fresh brokerage and mutation admission reject inactive registrations. The supervisor drains its own exact child and descendants before releasing resident/query allowances. The completion receipt labels worker drain as a separate observation. Idle direct transports close on deliberate shutdown; blocking work retains its inner materialization permit and process ownership until completion or proven native exit. Other projects retain their workers and indexes.

Unregister is eligible while a project is paused. Ordinary configuration and source jobs remain paused; only read-only inspection and this metadata operation can pass that gate. The running-work control uses the same classification. All-project repair includes active registrations only. Shutdown and predecessor ownership audits deliberately include inactive bindings so a crash during drain cannot hide an owner.

Sources, saved settings, models, derived files and unrelated registrations remain intact. Inactive bindings remain in bounded registry metadata, count toward its 1,024 retained-binding limit and retain their storage/source isolation. Unregister advances the request epoch to invalidate unaccepted old proposals; already accepted keys continue returning their original receipts. Explicit reviewed registration can reactivate the same binding. Replaying an old successful unregister key after reactivation returns its original receipt and does not unregister it again.

Registry format five adds bounded registration state to the existing project rows. Live opening upgrades prior development registries without changing their namespace, project bindings, job receipts or pause preference. Saved format-two through format-four inspection remains read-only and does not migrate. Unknown columns in an earlier schema, missing initialized state and invalid registration flags fail closed.

Windows passes all 278 store tests, including atomic completion, paused admission, canceled queued work, independent projects, active repair membership, retained ownership audits, saved observations, explicit reactivation, replay, captured-result recovery, migration and damaged-state preservation. The real native test passes an idle direct client held across paused unregister, canceled source work, worker drain, unchanged sources/settings/indexes, independent-project reads and receipt replay after explicit reactivation. The extension suite passes 206 tests with six skips, including approval and trust loss. Final source and platform qualification remain separate gates.

## Derived state

Unregister does not reclaim disk space. Explicit selected derived-state deletion still needs its own preview, owner/fence proof, bounded resumable inventory, quota accounting and preserved settings. Unknown entries and source trees must remain protected. That implementation remains open; retired direct-store removal commands are not a fallback.
