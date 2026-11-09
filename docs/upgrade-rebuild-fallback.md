# Clean break and rebuild at the 1.8.16 baseline

**Approved 8 November 2026 after the installed upgrade failures.** The user
approved replacing all pre-1.8.16 index migration with a forced clean break. The
1.8.16+ development line will be stabilized and qualified before an eventual
public **1.9.0** release. Current native/extension versions remain 1.8.16.
Implementation and installed qualification of this revised contract are in
progress; earlier passing tests do not qualify it.

This contract supersedes the optional early rebuild, imported-legacy migration
requirements and unpublished reconciliation-continuation adapter described in
the historical record below. The rebuild path is the required boundary for every
recognized installation older than 1.8.16, not a fallback after migration fails.
The old index-migration path must not remain an alternative at this boundary.

## Admission and supported upgrades

A verified pre-1.8.16 installation enters native reset/rebuild admission. A
current binary copied over old state cannot convert that state into a supported
installation. Ordinary upgrades from 1.8.16 onward require both an approved
runtime version and the supported persisted state-contract identity. Unknown,
missing, crossed or unsupported state identity cannot bypass the boundary.
Fresh state records `airk-state-contract.json` under the selected control root.
The native owner atomically qualifies it at fresh Genesis. Schema 1 records
`state_contract: "airk-state/1"`, `baseline_version: "1.8.16"`, and the exact
installation, control root and namespace binding. A marker copied from another
installation or namespace cannot qualify the selected state. Future format
changes require explicit compatibility or another rebuild boundary; the numeric
application version is not sufficient authority.

The extension, standalone CLI and native installer must use the same native
boundary decision. Native lifecycle management remains authenticated local IPC;
read-only HTTPS/MCP does not gain reset or deletion authority. Equivalent status,
review and result views retain the complete canonical native observations.

## What the reset preserves and discards

Capture saved indexes from complete authoritative catalog/registry metadata,
without opening old index publications. Existing capture bounds are 64 indexes
and 128 KiB; exceeding either refuses the reset explicitly, with no truncation.
The complete predecessor/successor protected-path union also retains its
existing 16-path bound and is checked before reset admission or old-process Stop.
The 64-index roster bound does not promise support for 64 distinct physical roots.

Still-existing retained older namespace/private roots must have exact immediate
ownership under the installation, state, control/log roots or current retained
registration data roots. A saved protection path alone is not deletion authority.
An unsupported retained root refuses reset before mutation and preserves the
exact saved selection (`clean_break_retained_root_unsupported`). Already removed
older roots are not a remaining cleanup obligation. The reset does not promise
a universal purge of every unknown historical installation layout.
Preserve the exact reviewed source selections and supported pause, watch, vector
opt-out and evidence-detail preferences. Saved active catalog-root selection is
captured where supported. Old grants, history, custom models, quotas and custom
extraction rules are not transferred as successor policy.

This is a mandatory boundary for older versions, not a promise that every unknown
historical configuration can be rebuilt automatically. Include/exclude policies
or aliases whose exact source boundary would require old-format translation
refuse before mutation. Never replace them with whole-root access. Report
unsupported settings explicitly; do not infer missing sources from an open
workspace or scan disks to guess old intent. External user custom-model files
remain untouched.

Original source files and folders remain untouched. Old indexes and caches,
index IDs, grants, namespace credentials and accepted/pending operations do not
carry into the fresh state. Do not fabricate successful completion of discarded
operations or reinterpret rebuilt evidence as the old publication. Existing
clients require reviewed reconnection with fresh authority; old connection
configuration is removed or changed only where AIRK ownership is proven.

After the forced reset is admitted, predecessor installation-level User Stop and
lifecycle state are discarded with the other old installation state. They do not
prevent the fresh runtime from starting its initial builds. Only supported index
pause/watch choices are carried forward, with the initial-build behavior below.
A new Stop accepted during reset or rebuilding remains authoritative and is
never cleared by retry or resume. The extension's native-execution-disabled
setting remains a hard gate: reset does not enable native execution or bypass
workspace trust.

## Native reset and cleanup lifecycle

The owning native installer must bind a durable reset operation to the exact
installation, verified predecessor/candidate inventories, reviewed source roster
and successor state. It must verify process/manager ownership and old-owner exit
before opening or removing old AIRK state. Process names, absent endpoints,
uninstall timestamps or an extension panel are not shutdown proof.

Create a fresh namespace/catalog and fresh index storage, then submit ordinary
native indexing jobs. Every saved index receives one initial build, including
saved paused indexes; restore its saved pause/watch policy after that build.
One unavailable index must not prevent independent healthy indexes from
building. Restart resumes the same accepted reset and indexing identities; it
does not create another namespace or replay an uncertain action. Report
installation, per-index readiness, client reconnection and cleanup separately.
Cleanup waits for all replacement indexes to meet its readiness condition;
missing, failed or incomplete indexes remain visible and may retain old owned
state as pending cleanup. A timer is not evidence of progress.

Remove ownership-proven old AIRK index/state roots, superseded runtimes, launchers
and installation artifacts once their reset retirement conditions are verified.
Preserve sources, unrelated files, unowned paths, external user custom models
and still-active current state. Guard every proposed deletion against every
primary and additional source root, including inactive registrations. Redirected
links or unproven deletion scope block cleanup. Known owner-controlled private
control/log roots authorize their owned contents; that authority does not extend
to arbitrary surrounding directories. Deleting a custom parent merely because
it once held an AIRK index is not permitted. Old state may be retained temporarily for an incomplete reset, but
it is not an indefinite rollback installation or a successful-cleanup exemption.
Cleanup must resume from its durable operation and disclose each unproven or
blocked item. Client declarations and old uninstall-receipt cleanup use a
separate host-owned compare-and-swap plan whose proof is prepared before reset.
The source HTTP correction adds purpose-bound `predecessor_cleanup`
metadata proof, distinct from an ordinary live grant or credential renewal.
Before retirement, retain the complete bounded original HTTP ownership snapshot
and host client metadata durably. Native capture follows verified predecessor
exit and precedes the state fork, with a complete 128-ID/64-KiB HTTP bound;
an ID lookup must retain the full inventory,
not replace it with a selected subset. Expired/revoked historical ownership can
prove disposal only. Credential comparison uses a digest of the full original
token, never persisted token plaintext. A fresh-namespace ownership collision or
unavailable observation blocks disposal rather than implying absence.
Only the actual owning fresh HTTP service may initialize its first durable
empty credential ledger before endpoint publication, with no prior endpoint or
credential history. Metadata reads never repair a missing ledger or invent
authenticated absence. Missing/corrupt/orphan metadata remains unavailable;
a compacted retired marker without a token digest preserves an unproven
environment token. See [the native lookup contract](service-installation.md#exact-native-bootstrap-and-host-artifact-cleanup).
Qualification of this correction is in progress; the earlier r34 checks
remain historical evidence, not acceptance of these new inputs.
Native reset completion does not claim that external client configuration has
been cleaned or reconnected. Keep failed test fixtures and incomplete cleanup receipts as
qualification evidence; successful product reset is not permission to remove
those developer records.

## Qualification

Before distributing the corrected candidate, exercise native handlers and actual
CLI/extension installation paths with fictional owned fixtures. Cover clean
installation; older raw and catalog installations; prior failed activation;
running old clients/workers; paused and missing sources; unsupported or corrupt
metadata; crash/restart at each state boundary; source-byte preservation; fresh
namespace/grants; same-contract later upgrades; and rejected copied binaries or
unknown markers. Verify all old owned processes and eligible artifacts are gone,
and that unrelated sources and artifacts remain. Windows results do not qualify
macOS or Linux. The [1.8.16 working record](releases/release-1.8.16.md) and
[implementation ledger](research/one-airk-implementation.md) record actual checks
and remaining limits.

## Historical optional-rebuild design — superseded 8 November 2026

The material below records the earlier optional source implementation and its
original limits. It is retained as dated design/validation history, not current
upgrade guidance or an acceptance requirement for carrying pre-baseline indexes.

### Earlier optional automatic rebuild during upgrade

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

#### Required behavior

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

#### Initial supported configuration

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

#### Native ownership and recovery

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

#### Editor reporting and interface parity

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

#### Acceptance before shipping

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
