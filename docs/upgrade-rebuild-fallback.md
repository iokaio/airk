# Optional automatic rebuild during upgrade

**8 November 2026, scope refinement: implemented in source; installed and frozen
qualification pending.** The user retained automatic rebuilding and requested an earlier
upgrade option without an exhaustive classification of incompatible failures.
There is no retained actual incompatible-failure receipt, and the installation
population is fewer than ten. The first implementation therefore accepts an
explicit native `runtime activate --rebuild-indexes --expected-active <inventory>`
choice before attempting migration, then automatically rebuilds from authenticated
saved source definitions. The early editor upgrade choice uses that same
native operation. Choosing it still requires native authentication and eligibility
checks before mutation. This document describes the bounded source implementation;
it does not qualify an installation. The
[fixture compatibility matrix](testing/fixture-compatibility.md) retains the
separate imported-legacy and reviewed-catalog migration cases.

The earlier 8 November policy required a positively identified incompatible
migration failure before recovery admission. This refinement replaces that
trigger with an explicit earlier choice. It does not introduce a catch around
arbitrary activation errors, a failure taxonomy, or an unattended rebuild of
every installation. The authentic 1.5.12 schema-8 metadata bridge passed its
scoped regression using the original DDL; original publication, imported
authority, process ownership and installed producer acceptance remain separate.
That bridge remains independent of rebuilding.

## Required behavior

Migration remains the normal upgrade path. When the user chooses automatic
rebuild early, bind admission to the exact expected active inventory and the
candidate. Authenticate the complete saved source/configuration roster, preserve
the old installation state and build replacement indexes in fresh compatible
state. Once this choice is accepted, native recovery resumes automatically and
notifies the user of progress and outcome. An incompatible migration failure is
not a prerequisite, and no second confirmation is required to start indexing.

Preserve the original runtime, state, index roots, configuration and any existing
failure receipt as recovery evidence. Recreate supported saved settings exactly;
refuse before changing state when a setting cannot be preserved. Do not infer
source folders from open workspaces, scan directories to
guess missing definitions, widen an existing connection scope, or silently
enable watching or semantic indexing. A paused index remains paused; report
its pending rebuild honestly. Sources remain read-only.

## Initial supported configuration

The first implementation is bounded to an authenticated raw managed,
pre-catalog registry. It preserves the exact saved source aliases and collection
boundaries, pause and watch preferences, vector opt-out, simple evidence-detail
settings and the default bundled model policy. This is not support for every
historical configuration or all saved settings.

Native inspection refuses the operation **before any state change** for modern
catalog memberships or grants, custom quotas, usage settings, custom models,
scan or compression settings, vector selection, extraction rules, source policy,
split state, semantic overrides, or unsettled configuration. Unsupported or
incomplete authoritative metadata also remains an explicit refusal. The editor
may offer the early choice when a predecessor is installed; selecting it still
requires native authentication and eligibility checks. Unsupported installations
retain their old state and the refusal details.

Original client configuration and credentials remain preserved, but their old
namespace does not authorize the successor. Re-enrollment is explicit after
rebuilding, with reviewed source scope; existing connections are not silently
retargeted. A rebuild does not manufacture imported-legacy provenance or grant
the rebuilt installation an old namespace's authority.

Installation, indexing acceptance and usable search are separate outcomes.
Report rebuild started, pending/paused, ready, failed or unavailable from durable native job
observations. Do not say the index was preserved or migration succeeded after
rebuilding it. Missing sources, denied access or exhausted storage can still
prevent readiness; retaining those failures is part of the contract.
`Ready` records successful native Index jobs for replacement evidence; it is not
a claim of complete semantic/vector readiness. Inspect the separate semantic
coverage/readiness observation, including disabled or still-pending vectors.
An unavailable or expired original job receipt does not prove execution failed
or succeeded. Report historical job observation as `Unavailable` and retain its
identity; current native readiness is a separate observation. Do not resubmit a
rebuild or manufacture a terminal outcome merely because that receipt cannot
be read.

## Native ownership and recovery

The existing `runtime_install` activation/checkpoint owner must admit and persist
the recovery before acting. An extension catch around a failed activation cannot
establish that the failed transaction is safe to replace. Use a durable operation
identity tied to the exact installation/control namespace, candidate and
predecessor inventories, explicit rebuild choice and bounded source roster.
Retain an existing original failure without fabricating one for an early choice.

The installation layout and registry owner deliberately pin the control path and
namespace. Recovery therefore needs an explicit successor-state transition or a
separately owned successor installation. It must not erase, reuse or silently
retarget the preserved control root. Verify old process/manager exclusion before a
new owner serves, then use canonical fresh catalog Genesis, reviewed setup and
native Index jobs in fresh storage. A restart resumes the same accepted recovery;
it does not create another namespace, repeat an uncertain mutation or enqueue a
second rebuild.

New credentials/grants require explicit re-enrollment with reviewed source
boundaries; automatic grant or catalog-membership migration is outside the
initial supported configuration. Fresh rebuilt evidence must carry distinct rebuild
provenance, never `imported_legacy_snapshot` or a claim of retained publication
continuity. Old state is retained through this recovery rather than passed into
ordinary successful-migration retirement. Eventual removal is a separately
owned cleanup operation.

The retained predecessor is an explicit exception to ordinary obsolete-payload
cleanup. For a rebuild from A to B, A and the recorded earlier predecessor remain
protected recovery evidence. A later ordinary B-to-C upgrade retires B and other
unprotected obsolete payloads, while its complete cleanup receipt reports the
preserved rebuild evidence separately. The old rebuild receipt is historical;
its candidate B does not have to remain selected for C to complete ordinary
cleanup. Preserved evidence must not be silently deleted or treated as an
unexplained cleanup failure.

Do not convert arbitrary activation failures into rebuild admission. Ownership or
selection changes, pending/unknown migration outcomes, failed drain, corruption,
missing authoritative definitions, permission errors, transient busy responses,
client-configuration conflicts, TLS/trust failures and budget exhaustion retain
their existing failures. A rebuild cannot repair those prerequisites by deleting
state or resetting the namespace.

## Editor reporting and interface parity

Native CLI, MCP status/configuration reads and Workbench must retain the complete
recovery result, any failure reason, preserved-state locations, operation/job identity,
scope/provenance and readiness. The extension consumes those observations; it
does not reconstruct source definitions or implement indexing itself.

The automatic setup path must distinguish a migrated installation from rebuilt
replacement indexes. Persist notification identity by recovery operation and
terminal stage so restart/reconnect resumes progress without repeated rebuilds
or repeated success notices. Suggested initial wording:

> AIRK is rebuilding replacement indexes from your saved source folders.
> Your previous state is preserved. Search will become available as indexing
> completes.

Use the actual paused, completed, failed or unavailable observation when applicable. Provide
native details for the preserved state and recovery, and avoid promising that
every installation or rebuild will succeed.

## Acceptance before shipping

Selected development evidence is retained in the
[implementation ledger](research/one-airk-implementation.md#early-rebuild-option--8-november-2026-implementation-in-progress).
The Store selection passed 8/8, including the authentic schema-8 bridge. The
native selection passed 32/33, retaining one MCP fixture budget failure; the
corrected MCP selection then passed 2/2 without raising the 48 KiB limit.
The real Store-backed interruption regression passed fourteen pause/phase
subcases. These separate scoped runs are not a clean whole-suite result or
installed recovery acceptance. The original r26 artifacts remain unchanged.

First establish the supported authoritative metadata reader for the selected
saved configuration. Refuse unsupported or incomplete definitions. Add T0
regressions for explicit admission and expected-active checks, exact saved
scope/settings, fresh storage, full provenance, transition/restart identity,
unknown outcomes, pause/watch policy and complete failure reporting. Exercise
the real canonical handlers and actual CLI/MCP transport where responses change;
renderer mocks cannot qualify parity.

Then qualify interruption at every accepted recovery boundary and one authentic
installed rebuild with the exact preserved old state, original source hashes,
new native index jobs and eventual retrieval. Require primary and independent
review, T0/T1 and a separate source/artifact freeze before native acceptance.
Keep migration acceptance independent; rebuild success does not satisfy the
installed imported-legacy migration requirement. Production changes require a
new build identity; the original r26 runtime and VSIX remain immutable evidence.
