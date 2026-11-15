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
