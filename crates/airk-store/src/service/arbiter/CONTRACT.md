# Independent Arbiter stage state and asset custody

Owner: airk-store::service::{arbiter,arbiter_assets}. New state decision recorded
15 November 2026 under the approved Focus/Context contract. This is not an
airk-state/1 modification.

Current configuration and assets live in private arbiter-v3 under the actual
ServiceLease. Reads, missing-state inspection and disabled startup create nothing.
Existing arbiter-v1/v2 trees, profile identities, asset files and incomplete native
receipts are never reinterpreted, moved, adopted or deleted. Existing valid v3 is
authoritative; no old consent activates either current stage. If v3 is absent and
a legacy root exists, ordinary configuration mutation refuses with explicit
initialization needed; ordinary evidence remains usable.

Explicit Initialize uses the native service lease, writer lock, descriptor-relative
configuration lock and original management liveness. It requires the exact bounded
legacy configuration digest, or absence when no legacy configuration exists. Two
legacy configuration files are ambiguous and refuse. It atomically publishes only
new canonical configuration/3 revision1 with BOTH stages disabled and empty.
Legacy bytes remain unchanged. Conflicting interrupted initialization is retained;
no overwrite manufactures successful recovery. Configuration CAS remains one
monotonic common revision, with exact installation/catalog identities and all
independent stage validation supplied by Core.

New assets/3 and partial/3 receipts bind configuration_schema=configuration/3 and
explicit Focus/Context stage in addition to existing installation/catalog/manifest,
configuration revision, declared asset bytes and original native file identities.
A current Profile must declare the matching stage. Prior receipts cannot mint
missing stage custody; old schema bytes are preserved and refused at the new root.
Download, staged partial recovery, pinning, draining and removal preserve original
ServiceLease/directory/file ownership, bound asset inventories and exact hashes.
A withdrawn-profile cleanup observation remains native custody only, never an
execution/profile qualification or enrollment capability.

Independent stage configuration shares the same writer/file-lock boundary and
common revision. A change in either stage invalidates stale composed delivery.
The service may not drop native asset pins or resident reservations until actual
FFI/drain completion. Reads and restarts never provision absent assets. Store does
not grant source read authority, select a model or infer hardware qualification.

Focused controls retain exact v2 fixture bytes and unfinished assets through
explicit initialization, wrong-digest and expired-authority refusals, no implicit
CAS migration, independent Focus-only consent, foreign stage receipt refusal and
existing alias/object-identity/partial-recovery/removal checks.

Detailed asset publication/removal fences remain owned by
[the asset transaction contract](../arbiter_assets/CONTRACT.md).


A current directory, asset subtree or interrupted configuration lock is never
proof of initialization. Whenever the current configuration FILE is absent,
load, ordinary CAS and final-delivery reads still refuse retained legacy state,
regardless of whether the v3 directory already exists. Explicit Initialize alone
may publish the new disabled configuration after its original legacy digest and
liveness checks. A valid existing v3 configuration remains authoritative.

## D08 atomic configuration3-to4 migration

DeviceConfigurationStore borrows the original ConfigurationStore; it shares its actual ServiceLease, writer mutex, descriptor root and OS configuration.lock. The authoritative CAS path remains arbiter-v3/configuration.json so old schema3 decoders REFUSE after schema4 publication instead of granting stale parallel consent. Before atomic replacement under the prior file HANDLE, migration creates configuration-v3-<rawSHA>.retained with exact canonical schema3 bytes, fsyncs it and its directory, rechecks original management liveness, then publishes canonical4 via the existing staged descriptor-relative replace. The retained copy is evidence, NEVER live authority. Existing retained/staged files reopen only through owner-private no-follow native guards and must match exact bytes; conflicts are preserved/refused. Failure before publication leaves active3 unchanged. Recovery against already-published4 requires the exact retained3 digest/revision and exactly the expected migration result, never overwrites later4 mutations.

Automatic native upgrade may invoke load_or_migrate with authenticated compiled Profile resolution and live source/configuration/deadline checks. It preserves model slot, enablement, consent, all enrollments and original source-catalog namespace, chooses Auto/Auto and advances common revision once. Device-catalog digest/source is a separate required binding supplied by the owning native catalog, not a copied meaning for the old namespace. Actual CPU candidate admission and assets3 custody are separate native gates, never inferred from a digest or successful data migration.

Current apply_checked retains installation/sourcecatalog/devicecatalog identities, exact revision CAS, same writer/OS lock, original check callbacks, canonical staging, prior-handle replacement and readback equality. with_current retains the nonwaiting final-delivery read fence and checks original liveness around bounded I/O; it refuses unmigrated schema3 rather than pretending it is4. Legacy Initialize/CAS cannot overwrite4 because their strict schema3 decoder refuses it. Missing-state inspection is read-only; older v1/v2 requires explicit disabled initialization and exact legacy digest, with old bytes untouched. Initializing valid3 must use migration rather than discard its consent. No new model/source/role capability is minted.

## Device catalog/source upgrades within configuration4

Ordinary configuration4 decode stays strictly bound to the expected compiled DeviceCatalogBinding; it never accepts arbitrary source/catalog changes. A native-only upgrade may rebind4->4 using authenticated expected OLD and NEW compiled catalog snapshots/descriptor resolvers, exact original raw configurationSHA/revision, unchanged installation and SOURCE catalognamespace, and the live owning authorization predicate. Both catalogs must independently validate their exact manifest/execution references and bound source; unknown ancestors/rows or changed bindings refuse. The transition clones all stage settings (model, enabled state, enrollment/consent, device policy and GPU precision), changes only device_catalog and revision+1, and uses the same existing writer/mutex/OSlock and prior-HANDLE CAS. Prior exact4 bytes are create-new retained and fsynced before atomic replace; no profile/device/task promotion is inferred. No public request accepts old/new catalog objects or an authorization predicate. Native StatusDevice startup must use the owning compiled transition inventory automatically, otherwise return explicit unsupported-upgrade rather than reinterpret stale configuration or require routine manual selection.

ConfigurationBindingObservation is diagnostic metadata only for locating an authenticated old compatibility snapshot. It is not active configuration, source permission or device/model qualification. Native caller must authenticate the old/new snapshots independently before CatalogRebindRequest/rebind_checked; ordinary load stays strict and an unknown snapshot refuses. Once4 publishes, stale expected revision/binding cannot overwrite later settings. Already-committed rebind is observed by loading the new expected binding, not replaying the old CAS.

Retained archive AND staged configuration files execute one common file.sync_all fence after either fresh creation/write OR reopening matching existing bytes. Recovery opens ReadWrite for the Windows FlushFileBuffers capability, preserves exact byte/object custody, then fsyncs the file before directory.sync or configuration replacement. Matching bytes or a directory fsync never proves file-data durability. A failed sync propagates before publication; the original active configuration and retained objects remain. Only a later explicit authorized recovery can retry the file fsync. The injectable retention-sync seam is private; public native methods always use real File::sync_all, never a metadata success bit.

initialization_status(original_live_check) shares the original writer fence and returns Some ONLY after positively observing absence of the current configuration FILE and presence of retained v1/v2 state, using the original bounded legacy digest. Any current file exists => None, with later typed parsing/error handled normally; corrupt/current3 is never replaced by a disabled placeholder. Ambiguous legacy configurations refuse. Ordinary absence has no initialization marker and remains read-only disabled/configurable. Native service must additionally prove no actual active/warming/pending work before emitting a disabled initialization placeholder.
