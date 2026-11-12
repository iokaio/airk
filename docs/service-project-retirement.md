# Index deletion and native retirement

## Delete an index in 1.8.11

In **Indexing → Index library**, choose **Delete index**, inspect **Delete this
index?**, then confirm **Delete index and stop maintenance**. The existing native
retirement owner stops that index's maintenance, removes its current set
memberships/default references and schedules cleanup of its owned derived data.
The UI reports logical removal and physical cleanup separately. Pending cleanup
is retained and must not be described as reclaimed disk space until confirmed.
Original source files, independent indexes and shared models are preserved.

A different independent index can cover some of the same source files without a
split: use a fresh storage leaf, collection and backing identity in **Set up a new
index**. Exact retired backing remains retired; deletion does not clear permanent
source exclusions or authorize adoption of unknown storage. See
[setup and selection](service-project-setup.md) and the
[1.8.11 working-build record](releases/release-1.8.11.md).

The native mechanics and dated qualification below remain inherited evidence.
`Project`, context and grant terms describe native records; the 1.8.11 UI presents
sets and handles native scope setup through **Search scope → Apply selection**.

## Missing files, source folders and index storage in the 1.8.8 working build

**Post-P14 development behavior; selected local qualification is recorded.**
Windows recovery tests, the corrected owning CLI suite, actual 1.8.7 upgrade
process scenarios, Mac portable tests and the final optimized-runtime extension
suite have passed within their recorded scope. The [working-build
record](releases/release-1.8.8.md) distinguishes combined test acceptance from the
original failed full runs and identifies the local test package. Earlier P14
results retain their original source and package identities. Version **1.8.8**
remains unpublished; manual installed-editor acceptance and broader platform
qualification remain separate.

A missing source is not an instruction to forget its index. When storage remains,
retained evidence can still be available with verification **Off**. **Report**
labels observed source-verification failures; **Require** enforces verification.
Resume paused work and explicitly update an existing index to publish individual
file removals. Retiring the entire index is a separate reviewed choice.

Missing index storage is reported as an unavailable member. For a Project that
combines indexes, member availability **Report** keeps healthy members' evidence
and the complete failed-member details. **Require** refuses an incomplete result.
Source verification and member availability are separate controls. Neither read
path unregisters an index, recreates a missing folder or silently narrows a grant.

The earlier 1.8.8 UI exposed this as **Prepare native index-retirement review**
inside the selected index. In 1.8.11 use **Delete index** and its confirmation.
The same native reviewed request remains authoritative. Retirement cancels the selected index's work,
disables its source authority and removes its current Project memberships and
default references, including context defaults no longer backed by a Project membership. Other memberships,
associations and independent indexes remain intact. Frozen grants and historical
receipts remain unchanged: an old grant that names the retired index becomes
stale; apply a fresh **Search scope** selection for the desired remaining indexes
and index sets.
An active reviewed split can require its exact recovery to finish first.

Logical retirement and physical cleanup are reported separately. Cleanup can
settle a missing collection or a wholly missing data-root ancestor using retained
native ownership and qualified absence; it does not create replacement folders,
ledgers or locks merely to remove references. When a shared data root remains,
cleanup releases only the retired index's proven disposable allocations. A
surviving owner, unknown file, changed binding or returned foreign storage keeps
cleanup pending rather than deleting unrelated data or releasing its credit.
Interrupted cleanup retains its native proof and resumes through the existing
owner. Source files, other indexes and shared models are preserved. This is
logical deletion, not secure erasure of earlier results, exports or backups.

Registration can precede the first worker or index build. A reviewed retirement
can remove that never-started index's current references without initializing
ownership, quota or writer-advisory files just to clean up. Native cleanup checks
that no selected storage or credit needs disposal and preserves unrelated files
and other indexes sharing the data root. Releasing selected credit still requires
its actual writer advisory and retained native custody. Unknown or changed native
state keeps cleanup pending; missing ownership is not permission to adopt it.

An unavailable old index source during an upgrade from an older installation is
handled by the clean break, not by this retirement path. The explicit legacy
upgrade retirement commands and the extension's review command were removed on
9 November 2026; see [unavailable legacy index during the clean
break](service-installation.md#unavailable-legacy-index-during-upgrade).

## Historical 1.6.4 removal contract

Removal cancels work, stops monitoring and new admission, drains owned workers, then automatically deletes the ownership-matched index, usage ledger and saved project settings. Registry details and job/configuration receipts are removed after files are gone. Opaque identifiers and removal time remain as minimal history and an opt-out marker. Sources and shared model assets are untouched. Pending removal is durable and resumes after interruption; it is not reported complete until deletion finishes. Earlier inactive registrations enter this path during the schema-10 migration. Unknown entries, aliases, ownership failures and held leases defer deletion and report failure. See [1.6.4](releases/release-1.6.4.md).

## Historical two-step implementation through 1.6.1

The remainder records the earlier unregister/retained-cleanup behavior and its original validation; it is superseded by the later removal contracts above.

Development work for 1.4.0. Unregister and deletion of derived state are separate operations. This contract does not authorize automatic cleanup of 1.3 state or unknown directories.

## Unregister

From 1.5.3, **Management › Projects › Remove project…** runs a native inspection, prepares the exact unregister request and asks for confirmation. It works from retained settings when the registered source folder has been deleted and does not require opening that project in the editor. On success the view switches to unregistered projects, where **Preview / resume index cleanup** and **Delete reviewed index state** reclaim its derived storage through the existing separate confirmation. The same removal action is available in Management › Indexing. A missing source stays missing: neither operation recreates it. `root_unavailable` remains an honest watcher observation until the source is restored or the project is unregistered; it is not treated as an empty source tree.

The trusted console's project settings offer **Unregister project**. Native CLI clients prepare the same configuration with `configure --stdin` and `{"operation":"unregister"}`. The ordinary inspection/configuration guard binds the exact project, incarnation and settings before a durable request is accepted. The console presents the host, collection, source, data location and effect before dispatch. Lost trust, declined approval and stale native guards prevent submission or execution.

Preparation does not unregister a project. The registry commits successful completion, the final configuration result, disabled watching, canceled queued project work, cleared continuations and inactive registration in one transaction. A crash after recording preparation resumes this metadata transition under a new proven worker fence; it cannot recover a successful job while leaving registration active. A missing preparation result rolls the completion transaction back.

Fresh brokerage and mutation admission reject inactive registrations. The supervisor drains its own exact child and descendants before releasing resident/query allowances. The completion receipt labels worker drain as a separate observation. Idle direct transports close on deliberate shutdown; blocking work retains its inner materialization permit and process ownership until completion or proven native exit. Other projects retain their workers and indexes.

Unregister is eligible while a project is paused. Ordinary configuration and source jobs remain paused; read-only inspection, unregister and the separate inactive cleanup operations can pass that gate. The running-work control uses the same classification. All-project repair includes active registrations only. Shutdown and predecessor ownership audits deliberately include inactive bindings so a crash during drain cannot hide an owner.

Sources, saved settings, models, derived files and unrelated registrations remain intact. Inactive bindings remain in bounded registry metadata, count toward its 1,024 retained-binding limit and retain their storage/source isolation. Unregister advances the request epoch to invalidate unaccepted old proposals; already accepted keys continue returning their original receipts. Explicit reviewed registration can reactivate the same binding. Replaying an old successful unregister key after reactivation returns its original receipt and does not unregister it again.

Registry format five added bounded registration state to the existing project rows; format six adds the persistent cleanup barrier. Live opening upgrades prior development registries without changing their namespace, project bindings, job receipts or pause preference. Saved earlier-format inspection remains read-only and does not migrate. Unknown columns in an earlier schema, missing initialized state and invalid registration/cleanup flags fail closed.

Windows passes all 278 store tests, including atomic completion, paused admission, canceled queued work, independent projects, active repair membership, retained ownership audits, saved observations, explicit reactivation, replay, captured-result recovery, migration and damaged-state preservation. The real native test passes an idle direct client held across paused unregister, canceled source work, worker drain, unchanged sources/settings/indexes, independent-project reads and receipt replay after explicit reactivation. The extension suite passes 206 tests with six skips, including approval and trust loss. Final source and platform qualification remain separate gates.

## Derived state

Unregister does not reclaim disk space. The trusted console's **Unregistered projects / cleanup** view lists inactive bindings separately from registered projects and bulk repair. **Preview / resume index cleanup** accepts a durable native preparation job. Its result binds collection ownership, the exact active/retired/replacement incarnation set, saved settings, registry namespace and request epoch. File/byte counts are bounded observations; incomplete inventories explicitly report lower bounds. **Delete reviewed index state** requires a completed preview and a host confirmation naming source, data, host, counts and effect. Declining, stale proposals or lost trust prevent dispatch.

Cleanup uses a private role of the same main executable, with its own inactive project lease and fencing generation. It opens the proven collection directly, without a Kernel, source scan, query endpoint, grant or inference session. Supervision and worker-side admission both prevent cleanup from reaching the old query owner during unregister/drain races. Both cleanup operations can run while paused; other configuration remains subject to its existing gate.

A durable owner-bound intent and registry barrier precede removal. Original retired slots drain first. Abandoned replacements become explicit retirement receipts. Five saved configuration files (roots, extraction, vector/custom-model selection, compression and scan limits) copy into a fresh empty owned slot, within 1 MiB, and are verified before atomic manifest selection. The previous active slot then retires in turns of at most 4,096 known files. Existing reader gates, alias/link checks, exact path ownership and quota-grant retirement remain enforced. Unknown entries are preserved and stop cleanup. Source trees, shared installed models/calibration inputs, root usage ledgers and unrelated collections are untouched.

Cancellation/failure retains the intent and re-registration barrier. Preview returns the same resumable approval; retry can continue it. An interrupted running cleanup can recover under the existing three-attempt job bound. Success requires all selected old slots to be absent and a durable complete report. Capturing that report precedes the atomic successful-job/barrier-clear transaction; crash recovery completes both together. Explicit registration then starts with the preserved settings and requires fresh indexing. An older preview cannot select a later registration's index; retained successful receipts replay their original result.

For CLI review, save `{"operation":"prepare_derived_cleanup"}` to a small JSON file and run `airkctl service requests --runtime-dir <control> prepare-configuration --project <binding> --file <draft.json>`. This persists an unsubmitted request. Submit the exact saved nonce with `airkctl service requests --runtime-dir <control> retry <nonce>`. Read job/configuration-result metadata through `airkctl service rpc --runtime-dir <control> --request-file <request.json>`. After reviewing the returned preview, separately prepare and submit `{"operation":"delete_derived","preview":"<guard>"}`. This never requires reconnecting to an inactive project's evidence endpoint. Unknown acknowledgment outcomes retain the saved request for exact recovery.

This operation owns only the selected 1.4 collection. [Explicit legacy 1.3 retirement](service-legacy-transition.md#explicit-old-derived-state-deletion) uses a separate offline CLI preview and captured removal intent. Neither operation adopts unknown directories or uses retired direct-store commands as a fallback.

## Reviewing all retained AIRK data (1.7.6)

After split Phase 4, `runtime inventory` and `runtime purge` are not yet
`airkctl` commands; until Phase 7 they are reachable only as
`airk __airk_role setup-interim runtime inventory|purge …`.

`airk runtime inventory --install-dir <dir>` is read-only. It lists AIRK-owned per-user locations with bounded, no-follow size measurements, every recorded active and inactive project index root, and what it cannot see (custom roots outside recorded projects, manual agent configurations, exports, backups, copies sent to providers, and temporary uninstall diagnostics). Partial results are marked `incomplete`.

`airk runtime purge --install-dir <dir> --select <id>` previews the exact logs or cache locations and returns a `plan_digest`. Only a repeat of the command with `--confirm <plan_digest>` deletes the contents of those locations. It runs while the service is quiescent, keeps the location roots, never follows links, reports anything it kept, and is safe to repeat after an interruption. Project indexes are not purged this way: use the unregister operation above, which drains the project's workers first. **AIRK: Review and Remove AIRK Data** in VS Code combines both paths, each behind its own confirmation. All of this is logical deletion, not secure erasure.
