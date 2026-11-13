# Offline service installation and lifecycle

> **Split Phase 4 (non-shippable interim).** The installer and the extension's
> management of the service are being reworked under the
> [split plan](split/split-plan.md) (Phases 6–7); this page still describes the
> pre-split installation. What is true now: `airk` is the service only and
> `airkctl` is the CLI. `airk` with no arguments, or `airkctl service start`,
> starts the installed service; `airkctl service stop`, `restart` and `status`
> replace `runtime lifecycle` and `runtime inspect|diagnose`. Agent connection
> commands moved to `airkctl agents …`, and `runtime http-trust` to
> `airkctl service trust`. The other pre-split `runtime …`, `setup`,
> `uninstall`, `host` and `service legacy …` commands on this page are
> reachable only as `airk __airk_role setup-interim <arguments>` until Phase 7.

**Current installer policy (10 November 2026):** installations through 1.9.0
inclusive must fully uninstall the owned predecessor and start with an empty
catalog. Installing the same build also requires full uninstall first. Source
files remain untouched; indexes, indexing jobs and watching are not restored.
Reconnect waits for explicit Index library setup. Ordinary editor reopening
only reconnects. The [candidate record](releases/release-1.8.23.md) retains its
original evidence; source version remains 1.8.23. The delivered Windows debug package passed its
package checks, but manual installation reported the HTTP cleanup refusal.
The subsequent native eligibility fix is untested, with no replacement package;
repair, builds and qualification are stopped at the user's direction.
The [manual-test correction ledger](simplify/one-process-ledger.md#manual-test-installer-correction-10-november-2026)
records implementation and validation; the historical rebuild policy below does
not authorize automatic indexing on a new installation.

**Inherited source implementation; unpublished and not yet platform-qualified.**
The current sources implement a shared localhost HTTPS service, one distributed
native executable with embedded default inference, direct manager entries and
guarded lifecycle operations. Final Windows installer, actual client/OS trust
and Mac Neo acceptance are separate pending gates. Use the
[1.8.23 candidate record](releases/release-1.8.23.md) and its linked task ledger
for preparation and source validation; older release evidence is historical.

## Split installer and setup role (Phase 7, in progress)

**Status (12 November 2026): source only. Nothing below has been built,
installed or qualified on any host.** It is the split replacement for the
pre-split installer flow described in the rest of this page
([split plan](split/split-plan.md) §9; contracts §5.5–§5.7).

- **Setup role.** `airk __airk_role setup --operation <file> [--plan |
  --prepare]` (U33) consumes one owner-private `airk-setup-operation/1` file
  and records every phase in `<state>/setup/<operation_id>.json`
  (`airk-setup-receipt/1`). It retires receipt-proven pre-split installations
  through the existing replacement code, drains the running service with
  exact-image evidence, verifies the placed pair against
  `payload-inventory.json`, writes the install record and then the install
  pointer, registers login startup (Windows Task Scheduler, Linux systemd user
  unit; macOS records `login_unsupported`), starts the service through the
  installed `airkctl`, and applies certificate trust with consent. Uninstall
  removes login startup, owned trust, the pointer and the record; index data
  is deleted only on an explicit purge. Details:
  [`setup_role/CONTRACT.md`](../crates/airk-service/src/setup_role/CONTRACT.md).
- **Login startup** runs `airk __airk_role service-login --install-id <id>`,
  admitted only for the pointer's installation and the record's exact image.
- **`airkctl service uninstall --yes [--purge-indexes]`** writes an
  uninstall operation, starts the setup role and exits (U20).
- **Windows installer** `installers/windows/airk-service.iss`: per user, no
  elevation, default `%LOCALAPPDATA%\AIRK Service\program` (`bin\airkctl.exe`,
  `service\airk.exe`, `notices\`), a data-location page (index data and state;
  cache and logs), a disclosure page listing earlier AIRK installations found
  by `setup --plan`, a preflight (`--prepare`) before files are replaced
  (it refuses while VS Code runs a pre-split AIRK extension: update the
  extension first or close VS Code; from 1.8.30 open VS Code windows with an
  up-to-date extension release AIRK while setup replaces it, through the I4
  replacement notice, and reconnect afterwards, so VS Code can stay open),
  the "AIRK Admin" Start-menu shortcut and `bin\` on the user `PATH` under its
  ownership receipt. Unattended: `/DIR=`, `/DATA=`, `/CACHE=`,
  `/ACKNOWLEDGE-RESET` (required when an earlier installation will be
  removed) and `/TRUST` (otherwise trust is skipped). Locations must be
  printable ASCII.
- **Local build** `pwsh -NoProfile -File scripts/build-local-service-installer.ps1 -ForceBuild`
  (U37). `scripts/sign-native.py` has no local signing adapter yet (U70), so
  the build stops unless `-Unsigned` is passed; an unsigned installer is a
  labeled packaging check that setup refuses at placement.

**Known blockers before a Windows installation works:** `airkctl` refuses a
record that names a Windows signature until Authenticode verification lands
(step 3b.1c-sig), so `airkctl service start` (and therefore setup's start
step) fails on Windows; the local signing adapter (U70) is missing; the
service does not yet use the record's data, cache and log locations (U89).

## Required clean break before 1.8.16

The following records the superseded 8 November rebuild policy. The current
installation rule above starts empty through 1.9.0 inclusive; this historical
section does not authorize recreating saved indexes.

The user approved the [clean-break contract](upgrade-rebuild-fallback.md) on
8 November 2026. Every earlier installation must rebuild into fresh AIRK state.
Preserve only authenticated, reviewed source selections and supported preferences;
original source files remain untouched. Old indexes, IDs, grants, credentials and
pending operations are not reused. Native shutdown, durable reset recovery and
ownership-checked retirement of old index/state roots, runtimes and launchers are
mandatory. Incomplete rebuilding or cleanup remains visible.

Fresh Genesis atomically qualifies `airk-state-contract.json` in the selected
control root: schema 1, state contract `airk-state/1`, baseline version 1.8.16,
and exact installation/control/namespace binding. Missing or copied markers do
not qualify old state. Capture uses complete catalog/registry metadata with
explicit 64-index/128-KiB bounds; no old index publication is needed. The complete
predecessor/successor protected-path union must also fit the existing 16-path
bound before admission/Stop; 64 logical indexes do not imply 64 distinct supported
physical roots. Still-existing retained older namespace/private roots outside
exact immediate ownership refuse before mutation with
`clean_break_retained_root_unsupported`. A recorded protection path does not
authorize deletion; already absent old roots are not outstanding cleanup.

A historically staged but never-started payload may lack optional process-role
aliases. On Windows, missing alias files alone do not prove process exit when
the native process audit is incomplete. That known unproven layout refuses
before reset admission or Stop. The installer does not create aliases to make
inspection pass, invent retirement proof, or omit a known process obligation;
canonical retirement guards still apply. Not every historical layout qualifies
for automatic reset.

Unsupported
include/exclude or alias translation refuses before mutation instead of widening
the selected sources. Supported pause/watch/vector/detail preferences survive;
custom policies, quotas, models, grants and operation history are not migrated.
External user custom-model files remain untouched.

Admitted clean reset discards predecessor installation-level User Stop and
lifecycle state, so the fresh runtime can start its initial builds. This does
not discard a new Stop accepted during reset/rebuild; that newer intent remains
authoritative through retry/resume. Native-execution-disabled and workspace-trust
gates remain enforced by the extension. Index pause/watch preferences retain
their separate supported preservation contract.

Every saved index gets one initial build before its saved pause/watch policy is
restored. An unavailable index does not block independent builds, but final old
state retirement waits for all required replacement readiness. Native cleanup
guards every deletion against all primary/additional source roots, including
inactive registrations; links and unproven ownership block it. External client
declaration/uninstall-receipt cleanup is a separate host-owned compare-and-swap
plan, prepared before reset. Native completion does not claim client cleanup.

From 1.8.16 onward, the supported persisted state contract and approved runtime
version govern ordinary upgrades. A new binary or changed version string cannot
label old state as compatible. The 1.8.16+ line is to be qualified before a future
public 1.9.0 release. Implementation and installed acceptance of this revised
policy are in progress. The previous narrow reconciliation-continuation adapter
was superseded before installed qualification.

The installed 1.8.15 refusal happened in preflight before shutdown, despite its
earlier progress text. Explorer failure publication and progress corrections
remain part of the new candidate: failure must be visible while an unrelated
connection read is pending, and progress must not claim process exit before it
is verified.

Management → Service → Runtime → **Complete runtime diagnostics** opens, copies
or exports the complete cached native bootstrap and diagnostic observation as
JSON. The capture timestamp identifies its age. The extension host owns these
actions; they make no new native read and do not prove current health, process
absence or upgrade completion. Service **Copy details** remains an explicitly
selected summary and points to this complete cached JSON route.

## Exact native bootstrap and host artifact cleanup

The HTTP disposal lookup accepts the exact selected predecessor, including an
initialized modern installation, or its retained exact reset snapshot. Saved
state identity validation still applies; a pending baseline reset is no longer
an eligibility condition for disposal-only ownership proof. This source fix
addresses the reported automatic-installation refusal; it is untested and no
replacement installer was built, at the user's direction on 10 November 2026.

Host request tracking is being moved from Memento into a shared private
per-request journal; preferences stay in Memento. Immutable preparation is
persisted before native dispatch, and post-response local tracking failures
retain the full native reply without authorizing replay. After authenticated
fresh Genesis establishes the exact operation/store/root/control/namespace
retirement fence, fully validated predecessor-namespace Memento rows, including
late nonces, are never imported or replayed and no longer consume active quota
or block a new Stop. This is not a cross-window writer drain: bounded
exact-observed cleanup separately reports late removals and remaining rows,
without claiming the original capture included them. Before the fence,
nonretired/foreign/malformed/wrong-namespace rows remain preserved and blocking;
uncaptured physical journal files remain blocked and untouched.

Predecessor request retirement requires exact captured scope/physical proof,
the matching accepted native reset, selected successor control/candidate and a
distinct activated Genesis namespace. That proves old request authority was
revoked; it does not prove all indexes ready or old index/state/runtime files
removed. Physical cleanup and overall installation completion still require
all fresh index, client and native cleanup gates. Ordinary same-contract
upgrades preserve uncertain requests. Interrupted-retirement handling remains
under source review; this correction has no new backend qualification yet.

An additional HTTP disposal correction is implemented in source; qualification
is in progress. The administrative
`predecessor_cleanup` proof is separate from ordinary live-grant lookup: it can
identify expired/revoked historical ownership for disposal without renewing
credentials or authorizing reads. The complete bounded original HTTP snapshot
and host client metadata must be durable before retirement. Fresh-namespace
ownership collisions and unavailable observations block disposal. Only the
owning fresh HTTP service can initialize its first durable empty ledger before
publishing an endpoint; metadata reads cannot repair a missing ledger or treat
it as proof of absence. See the [reset contract](upgrade-rebuild-fallback.md).
Earlier r34 checks do not qualify these new inputs.

After verified predecessor exit and before the state fork, native reset captures
the complete original private HTTP ownership inventory, bounded to 128 IDs and
64 KiB. The durable journal retains its original control root, namespace,
predecessor and operation identity after old-control deletion. It stores neither
token plaintext nor live grants. Missing, corrupt or orphan metadata reports
`unavailable`; it does not prove an empty inventory. A compacted retired marker
without a token digest cannot prove ownership of an environment token, so that
unproven environment entry is preserved.

`runtime lookup-http-connection --purpose predecessor-cleanup --predecessor
<originalSHA> --expected-active <CURRENTselectedSHA>` returns the disposal-only
Core DTO with the full original snapshot. Ordinary modern lookup and enrollment
remain separate. `runtime inspect-legacy-launchers --purpose predecessor-cleanup`
allows an exact selected predecessor-pair ownership read for pre-reset client
capture; ordinary HTTPS inspection keeps its existing gate. The journal retains
verified predecessor main/stable/universal-launcher metadata, and explicit
`command_images` excludes the staged candidate. The fresh service persists its
first empty ledger under its `ServiceLease` before TLS startup, without allowing
reads to create or repair an absent ledger.

The reset's final bootstrap probe now uses an opaque native permit issued only
for the exact admitted clean-break operation in `Starting`, its matching saved
activation intent, fresh Genesis marker and verified candidate health. The
transport is fixed to the candidate's request-epoch read and revalidates the
operation, instance and namespace. Ordinary reads, enrollment, checkpoints and
other lifecycle callers retain their version/state gates; this is not a general
pre-baseline bypass. The actual narrow transport regression passed locally.

Host artifact retirement is separate from native index/state removal. The
extension captures predecessor-native-proven HTTP environment entries and saved
connection metadata, with exact preimage checks before removal. Public trust
copies additionally require native CA provenance, this host's creator proof,
matching public bytes and full-width device/file identity. The creator handle
stays open through the awaited publication observer, and the caller checks the
original file identity; post-return adoption is rejected. Modified, unproven,
shared or newly referenced namespace copies remain preserved and visible
blockers. Cleanup plans retain digests/references, not bearer-token plaintext.
The existing terminal PATH mutator is preserved.

Stop-related metadata, PATH and completion bookkeeping do not ensure or start a
runtime. Host cleanup recognizes Windows path aliases and bare-ID saved
connection metadata while retaining ownership checks. Its flat managed
`http-trust` census is bounded to 1,024 entries and 2 MiB total read work,
including failed-read and overflow attempts. An old CA copy without proven
ownership remains preserved with an explicit blocker.

Only the current window's live Assistance creator can retire and positively
refresh its endpoint. Refresh failure stays visible and retry rechecks current
ownership; other windows are untouched. This does not enroll a new evidence
connection automatically. Native completion, host cleanup and client
reconnection remain separate observations.

## Standalone Windows EXE and user PATH

The standalone Inno package, separate from VSIX terminal integration, adds the
literal existing `{app}\runtime` directory to HKCU user `PATH` only after
`NativeSetupSuccess`. It preserves the other raw entries and existing `REG_SZ`/
`REG_EXPAND_SZ` type. Variable-bearing entries stay raw during ownership
comparison; the comparison does not apply expansion indiscriminately across
registry kinds. Pre-existing manual paths are not claimed as owned.

A durable `user-path-owner.txt` receipt records pending intent before the write
and ownership only after verification. After uninstall confirmation, successful
native uninstall is required before removing the exact owned literal. Native or
PATH cleanup failure aborts Inno's uninstall before package-file deletion,
preserving the receipt, PATH entry and bootstrap files for diagnosis. Cancellation
does not run native removal or change PATH. Existing terminals keep their
inherited environment; an application may need restarting before its new
terminals observe the updated user PATH.

The [Windows installer guide](../installers/windows/README.md) owns the detailed
lifecycle. Thirteen static/mocked PATH/package checks passed locally; they do not
execute Pascal or a registry. ISCC was unavailable for this local qualification,
so actual Inno compilation, registry/type/Unicode round-trips and signing remain
unqualified. No installer-ready claim follows from those tests.

<a id="checkpoint-history-capacity-correction"></a>

## Historical checkpoint history capacity correction

The r28 manual-test VSIX can refuse an upgrade from 1.8.11 with
`checkpoint_complete_roster_limit: serialized snapshot` when retained completed
jobs and request receipts overflow the 448 KiB checkpoint. Retrying unchanged
state repeats the refusal; deleting history is not the recovery procedure.

The r29 correction uses checkpoint snapshot schema 2. Every
retained table row still contributes to its complete count and authenticated
digest, while expanded job and request details describe pending/running work.
It preserves the existing size and row limits and does not delete retained
history. Saved schema-1 checkpoints are reobserved using their original format
for immutable packet and journal verification. An older checkpoint-capable
1.8.14 predecessor still executes its own checkpoint capture and may encounter
its original limit; candidate-side compaction does not change that executable.
The replacement r29 VSIX is package-verified; installed acceptance remains
pending. See the
[working record](releases/release-1.8.14.md).

## Approval and immutable staging

The installed platform VSIX or explicitly approved native distribution pins the
complete runtime inventory. Verification checks version/platform, every allowed
file and digest, physical file identity and bounded topology before execution.
Workspace settings do not approve arbitrary code. A changed payload or unknown
file fails closed; inspection and reviewed repair remain distinct from launch.

Modern `airk-runtime.json` is **schema 2** and binds only `airk`/`airk.exe`.
The separate complete `airk-install.json` file inventory remains schema 1; that
inventory schema is not the historical two-role runtime contract. On macOS the
main stays at `AIRK.app/Contents/MacOS/airk`, inside its sealed signed bundle.
Modern payloads contain no separate embedding executable, launcher image/copy,
dynamic ORT runtime or external default model assets. Legal files, signed app
resources and build/inventory receipts remain fully inventoried. Historical
schema-1 main/helper payloads are decoded strictly for predecessor handling;
they are not rebuilt or republished as modern packages.

Activation, rollback and `runtime repair-launcher` never create or replace a
launcher for a schema-2 payload. The repair command reports
`launcher_repaired: false` for that layout. A retained predecessor launcher and
its receipt remain unchanged migration state until explicit owned retirement;
an unresolved historical repair transaction must still be settled explicitly.

A successful modern installer must then migrate recognized owned MCP
configurations to HTTPS and remove the old launchers. After guarded
configuration saves and connection verification, it calls
`airk runtime retire-legacy-launchers --install-dir PATH --expected-active SHA`.
The native transaction verifies the selected schema-2 payload, the per-install
launcher receipt and any universal pointer for that installation. It stops only
verified legacy image owners, removes the exact owned launcher files and
receipts, and returns an absence receipt. It does not stop or restart the modern
service, change User Stop, remove source/index data, or retain a launcher fallback
after successful completion. Incomplete migration or cleanup is an explicit
installation failure, with owned transaction state retained for retry.

`airk runtime inspect-legacy-launchers --install-dir PATH --expected-active SHA`
provides read-only, hash-verified old command lineage for migration; a basename
alone is insufficient. Changed files, partial/unknown universal publications
and foreign ownership are preserved and reported. A valid universal entry owned
by another installation is outside this transaction. Windows/Linux termination
uses held native process identities. The macOS adapter authenticates native
audit-token execution identity before signaling an owned predecessor; unsupported
or incomplete native observations block completion. Raw observations retain unknowns and do not claim a
complete whole-machine census. Windows records the exact image path/hash roster
and a durable witness while write/image-exclusion handles are held, before
unlinking. A partial retry cannot reuse that witness for another image; an
already-missing historical image without it needs a complete native observation
or reports cleanup incomplete. Fresh installations have no historical image
roster to retire. Modern Ensure never republishes a universal launcher.
Ordinary activation preserves predecessors until this required post-migration
cleanup finishes. An explicitly admitted [rebuild](upgrade-rebuild-fallback.md)
separately protects its recorded predecessor payloads and old state as recovery
evidence; that retained set is not ordinary obsolete-payload cleanup.

For a selected schema-2 runtime, `airk runtime retire-previous --install-dir PATH
--expected-active SHA` then retires **every eligible approved nonselected runtime** in the
managed installation, including approved orphans no longer named by `previous`.
It supports skipped version numbers and repeated schema-2 replacements through
the same inventory contract. A successful receipt has `complete: true`, the
full `retired_runtimes` list and native process observations. It does not stop or
restart the selected service, clear User Stop, or modify source/index state.
The pre-1.8.16 clean-break lifecycle must retire its ownership-proven old
payloads and state once reset retirement is admitted; the older optional-rebuild
preservation rule is not an exemption. Historical optional rebuilds reported
preserved payloads separately and could retain A while retiring intermediate B
in a later upgrade. Those retained receipts remain historical evidence, not
proof that current baseline cleanup is complete.
Historical schema-1 selected runtimes keep the older one-previous command
behavior; current installation orchestration uses the modern branch only after
configuration migration and verified launcher absence.

The native transaction freezes the complete original manifests and process-image
roster before removal. Missing files in an interrupted cleanup remain part of
that roster; a reduced list, changed inventory, newly appeared payload or unknown
file blocks completion. Removal resumes under the same selected-runtime binding.
Since 1.8.18 the number of retained runtime payloads does not limit staging,
payload repair, uninstall or cleanup; an installation where earlier updates
failed can stage its replacement before predecessors are removed, and cleanup
retires every superseded payload it finds. At most four interrupted staging
reservations may exist; resume or abort them first. A crash resumes the same
candidate. Startup must finish the current migration/cleanup before another
upgrade. A reviewed payload repair's reserved restore cannot be shared with an
unrelated staging. Repair restores its exact preserved active payload without
starting the service or deleting the damaged original.

Modified/unknown repair quarantines remain explicit
`preserved_damaged_payload_requires_review` blockers to complete obsolete-image
removal. An older interrupted cleanup that lost both its original payload and
sealed ownership inventory reports `superseded_ownership_manifest_unavailable`
with recovery guidance. Neither case invents process ownership or silently
declares completion. Arbitrary user-downloaded flat executables are outside the
owned runtime and editor-cache inventories; their names do not authorize deletion.

## Pre-managed saved indexes and owned editor caches

For the current pre-1.8.16 boundary, old index publications and imported authority
are not migrated. Capture only reviewed source/preference inputs that the reset
owner can authenticate, then rebuild fresh state. The older migration/adoption
mechanisms described below are retained historical implementation context and do
not bypass the [required reset](upgrade-rebuild-fallback.md). Cache/process
ownership checks still govern retirement; unknown files are preserved and
reported rather than classified by name.


For the official standalone families beginning at 0.2.1, the native origin adapter
captures the actual original MCP declaration and complete configuration digest,
native user/file identities and full executable hash. Generated editor-cache
layout and digest-prefix consistency establish cache ownership, not distribution
approval or permission to execute those bytes. Current, future, altered, aliased
and unknown cache entries are preserved with an explicit blocker.

`runtime prepare-legacy-caches --install-dir PATH --expected-active SHA
--global-storage GLOBAL_STORAGE [--origin-locators-file PRIVATE_JSON]` freezes
the exact ownership inventory before configuration changes. Actual default
installations include known native Code/Insiders/VSCodium user profiles; custom
installations use the actual current editor storage and storage roots proven by
original declarations. Other custom profiles without native context or recorded
lineage cannot be discovered automatically. VS Code owns its extension-package
directories; this operation does not delete those or unrelated downloads.

**9 November 2026 (simplification S2, decision Q1 and step 3):** standalone
adoption is no longer offered. The extension leaves original standalone agent
entries unchanged for review and keeps their helper caches. Both adoption
commands refuse every operation with `standalone_adoption_unsupported`; the
adoption implementation is removed. An interrupted adoption is never resumed.
Its leftovers are settled automatically, correcting where possible and deleting
otherwise:

- Cache commands (`inspect`, `prepare`, `reprepare` and `retire-legacy-caches`)
  first move a withheld helper image back to its empty original path, or remove
  a withheld duplicate when the original is present, then clear the withdrawal
  from `legacy-cache-retirement.json`.
- Activation and manager changes (login install/uninstall, managed configure
  and detach) clear the adoption's own native admission fence only while the
  service is stopped under installer exclusion. A fence owned by another
  operation is never cleared. The service is restarted with the currently
  selected runtime only if the adoption had stopped it while it was running and
  not user-stopped, and starting is still allowed.
- They delete `legacy-maintenance*.json`, `legacy-index-adoption-batch-*.json`
  and `<state>/legacy-adoption/` (only when its `owner.json` names this
  installation and state and it contains no link or reparse point).
- They keep catalog rows, adopted index data (`airk-adopted-indexes-*`),
  original standalone data and sources, agent configurations and the cache
  retirement journal. Catalog rows of an adoption interrupted before Ready are
  reported and kept: no rollback API survives, and changing persisted catalog
  state needs the plan's D8 decision. Each such row is reported with code
  `interrupted_adoption_rebuild_required` and this guidance: rebuild the index
  by setting it up again from its original sources in Indexing, retiring the
  inert entry if needed, and keep the machine and AIRK running (no shutdown or
  sleep) until that rebuild completes.

Settlement reports what it restored, deleted and left, with reasons, as one
`legacy_adoption_settled` JSON line on standard error. If the adoption's fence
cannot be cleared because the service is running, activation and manager
changes refuse with `legacy_maintenance_pending_before_manager_change` until
the service is stopped. The following describes the historical adoption
behavior for reference only.

`runtime adopt-legacy-indexes --install-dir PATH --expected-active SHA
--operation-id BATCH_ID --locators-file PRIVATE_JSON_ARRAY
--expected-cache-inspection-sha256 DIGEST` adopts saved evidence from the exact
original declarations. Native grouping verifies physical collection identity,
historical layout, saved publication and source inventory. Codex and Claude
declarations for the same saved index share one archive, one Current publication,
one adopted index and one immutable grant containing every authenticated original
configuration claim. Client credentials remain distinct and independently
revocable. A file can contain several aliases; exact server/container/origin
identity remains separate from its whole-file digest. Grouping is never inferred
by the frontend from folder strings.

The preparation retains the original typed locator vector for interrupted
configuration-save retries. The native batch operation, each derived group
operation and the catalog's canonical operation remain explicitly distinguished.
The singular `adopt-legacy-index --locator-file PRIVATE_JSON` is the one-element
form of this same implementation. Native maintenance checkpoints accepted work, drains the
selected service and holds its owner leases across catalog writes. It withdraws
the full accepted cache-image roster, disables the ownership-proven original
scheduler, stops exact old owners, and holds the existing historical writer
lease while archiving the selected collection. The original tree stays intact;
validation and compatibility conversion operate on separate private copies.
Only actual Current publication, native catalog completion and imported original
authority permit success. No source extraction, watcher, indexing or model job is
created. `source_availability: not_observed_saved_only` deliberately does not
certify current source availability, freshness or semantic readiness.

Enrollment uses the group's original registration and the selected original
configuration digest. `http-legacy-connection --origin-sha256 DIGEST` additionally
selects the exact native configuration claim when one original file/logical
registration refers to different historical physical indexes. An unknown claim
or omitted ambiguous selector cannot fall back to another imported grant.

After immutable adoption and authority completion, exact predecessor images are
restored only for the unfinished guarded configuration transition. Successful
installation still requires all recognized configurations to use HTTPS, then
`runtime retire-legacy-caches --install-dir PATH --expected-active SHA
--expected-inspection-sha256 DIGEST`. Native retirement checks saved original
configuration containers for remaining accepted stdio references, stops exact
image owners and removes only captured owned images and empty generated leaves.
It retains full process/absence receipts and preserves sources and old index data.

A newly appeared cache prevents completion. Explicit bounded
`runtime reprepare-legacy-caches` with the previous inspection digest archives the
original proofs and returns a new accepted inventory; configuration review and
guarded saves must run again against that inventory. An interrupted
adoption's withdrawal is settled first (see above); changed file identities and
unknown entries require recovery rather than silent inventory widening. Final real predecessor, process, Windows and Mac acceptance
remain separate qualification gates recorded in the working release ledger.

The source owners are [runtime verification](../crates/airk-installer/src/runtime_install.rs),
[the native builder](../scripts/build-service-runtime.py), and
[extension verification](../packages/vscode/src/airkctl/verify.ts).

## Process lifetime and recognizable names

A per-user native service owns the loopback listener, native grants and shared
in-process MiniLM engine. Model status identifies its service instance, approved
execution digest, provider generation, admission and outstanding work. A null
inference process ID does not mean the engine is missing.

Since 1.8.22, the service is the only resident `airk` process of an
installation. Project owners, cleanup owners and the source watcher run inside
it. An owner that does not drain by its deadline is quarantined. It keeps its
project lease, and that project reports a typed stuck-owner status. Other
projects keep running, and an explicit service restart recovers. The
temporary processes are:

- one window helper per VS Code window (`airkctl session` after split Phase 6);
- CLI and installer one-shots;
- at most one PDF parser child;
- a user-opened `tools shell`.

Stable, preview and custom installations are separate installations, each with
its own service. See [process ownership](process-model.md).

MCP clients connect directly to the saved native HTTPS URL. They do not spawn an
AIRK process, Node proxy or launcher for each connection/request. Claude Code
and Codex credential helpers run the one-shot `airkctl agents credential`,
which prints the bearer header and exits. Verified setup,
inspection and administrative CLI commands remain bounded one-shot operations.
Interactive terminal tools execute the sealed main directly, retaining the exact
installation and native selection. The optional terminal wrapper is a shell/PATH
convenience, not an MCP transport or another engine copy.

Optional OS startup uses owned per-user definitions bound to the sealed main.
Windows Task Scheduler, Linux user systemd and macOS login management retain
separate platform checks. No modern manager entry is generated through a copied
launcher. External organization supervisors retain their ownership policy.
See [process ownership](process-model.md) and [native signing](native-signing.md).

## Native user defaults

`airk runtime location [--channel stable|preview]` reports defaults without
creating an installation. Native OS account/known-folder lookup, rather than a
workspace-supplied home path, identifies the user. Linux accepts supported
absolute XDG overrides. Stable and preview retain separate locations and manager
identities. Existing installations use their recorded layout; selecting a
version does not silently create another service or relocate an index.

Install/payload, writable state/control, index data, cache, logs and client
configuration are distinct locations. Explicit custom installations require
reviewed ownership and state binding. See [storage formats](storage-formats.md).

## Writable state binding

Installation/layout and initialization receipts bind the original private state
and control namespace independently of the payload. Missing, redirected or
conflicting initialized state is a recovery condition, not permission to create
an empty registry. Unknown directories and partial receipts are preserved.
Source/index isolation also protects proven private installation/state paths.

Canonical databases, journals and committed publications remain the state
owners. A checkpoint packet records bounded identities, digests and safe
progress references; it does not copy source text, private keys or live model
pointers into an installer backup. Index-data privacy remains separate from
control/credential privacy: inventory can report existing public storage and
its limits without claiming that every ancestor or ACL was audited.

## HTTPS connection and certificate trust

The service binds the saved IPv4 loopback port, preferring 57421 only for initial
selection. If that port is occupied it holds a newly bound ephemeral listener
and publishes the actual endpoint; clients must use the returned URL. The old
port is not assumed safe or available. Namespace, installation/user identity,
service instance, generation and certificate fingerprints remain native-bound.

Enrollment uses an explicitly reviewed single/combined selector, or genuine
owned predecessor configuration/provider lineage plus a live imported grant.
A multi-root predecessor retains all original members with no default project.
Missing, revoked, authority-zero or ambiguous state requires review. Setup
returns protected connection references, never a raw bearer or private key.

TLS validates normally against the exact loopback IP. Install the reviewed public
issuer through supported additive client trust or per-user OS trust; preserve
existing enterprise/public roots. Do not disable TLS validation or replace the
endpoint with plain HTTP. Normal CA validation is not a unique-issuer pin.

`runtime http-trust --install-dir <installation> --action inspect` observes the
owned trust state. Install/remove require `--expected-ca-sha256 <reviewed-digest>`;
mutation rechecks that fingerprint. Windows uses CurrentUser Root; macOS uses the
native user's login keychain. The ownership receipt distinguishes preexisting
issuers from AIRK additions. Interrupted or changed outcomes remain for review,
not blind retry. Verified saved identity permits inspecting/removing owned trust
after Stop without starting a service. Uninstall's guarded cleanup preserves
preexisting or unproven trust and reports incomplete cleanup explicitly.

Automatic per-user Linux OS-store enrollment is unsupported. Linux VS Code MCP
needs explicit administrator provisioning in its distro system store and user
NSS store; see [official VS Code certificate guidance](https://code.visualstudio.com/docs/setup/network#_ssl-certificates).
An internal AIRK HTTPS check does not qualify that external host. Client formats,
protected VS Code input resolution, remote/container limits and actual connection
checks are documented in [agent integration](agent-integration.md).

The source owners are [HTTP setup](../crates/airk-https/src/runtime_http_commands.rs),
[private identity/credential storage](../crates/airk-endpoint/src/http_endpoint.rs) and
[owned trust](../crates/airk-endpoint/src/http_endpoint/trust.rs).
Issuer replacement uses an explicit receipt-backed prepare/trust/commit/finalize
transaction, with prepared-only abort and an ordinary 24-hour overlap. Expiry
does not remove trust automatically. See [issuer rotation](local-https-api.md#explicit-issuer-rotation)
for exact review inputs, owned client-bundle CAS and remaining host qualification.

## Selection and lifetime

Apply the version/state-contract boundary before selecting the lifecycle below:
pre-1.8.16 uses fresh-state reset, and supported 1.8.16+ state uses the ordinary
compatible-line path. A legacy checkpoint or preserved predecessor cannot grant
permission to retain pre-baseline indexes. Cleanup remains a separate verified
outcome; the reset contract does not authorize deleting original sources.


Explicit Stop and pause preferences remain native. Observation is not authority
to start a stopped owner or replay a request. Newer runtime versions cannot
implicitly widen a configured search selection. Accepted source/index work
retains its native request identity and durable status independently of Explorer.

Upgrades journal `staged_verified`, `quiescing`, `checkpoint_ready`, `old_stopped`,
`activation_prepared`, `candidate_started`, `committed` and `retiring` boundaries,
with an explicit `aborted` terminal path. The journal binds the installation,
control namespace, candidate/predecessor inventories, manager definition and
checkpoint digest. Modern owners close admission and reach their canonical
checkpoint boundary before replacement. Provider callbacks/delivery and worker
ownership participate; an immediate pending/unknown drain is not completion.

Older multi-process installations cannot implement a new graceful protocol
retroactively. Their adapter captures committed-store state only after verified
old-image/manager ownership and exit observations, and records that weaker
boundary explicitly. Unknown or foreign processes are preserved. PID reuse,
missing observations and unsupported saved jobs are not interpreted as safe
completion. Original request keys, publications and source bytes remain intact.

Resume and rollback use the same operation/selection fences and actual durable
outcome; they do not submit an uncertain mutation again. The implementation is in
[checkpointed activation](../crates/airk-installer/src/runtime_install/activation/checkpoint.rs) and
[service checkpointing](../crates/airk-service/src/service_runtime/checkpoint.rs).
Actual predecessor/version/OS campaign coverage remains a release gate.

## Registry 13 and downgrade compatibility

The current registry contract is **13**: it retains schema-12 physical catalog
DDL while requiring in-process provider and embedded-default-source semantics.
A version string, a copied SQLite file or unchanged DDL does not establish
compatibility. Required registry/authority/policy capabilities are read from
retained state and compared with the candidate's native self-test before an
unsupported target can take over. `runtime_state_contract_unsupported` preserves
the current selection and requires a compatible runtime or explicitly validated
snapshot restoration. Executable rollback does not downgrade metadata.

See [capability declarations](../crates/airk-core/src/index_catalog.rs) and
[persistent requirements](../crates/airk-store/src/service/registry/runtime_contract.rs).
An older main may remain diagnostic-only; that is not permission for old
workers/model semantics to reopen modern state.

## Organization-managed read-only payloads

Managed installations pin an approved provider-owned payload and separate user
metadata/state. `external` leaves service election to the organization;
`user-on-demand` uses its explicitly allowed lifecycle. Unknown/changed payloads
are refused. Modern clients use direct HTTPS enrollment and the sealed main;
`prepare-client` does not manufacture a modern launcher. User detach/uninstall
preserves provider files and external ownership. Detachment requires the exact
managed configuration's native catalog initialization to be settled; an
unfinished or changed reservation is preserved and refused. User-on-demand
detachment authenticates and drains its saved runtime and proves native owner
exclusion. External detachment leaves the organization's supervisor running.

The durable detach intent supports interrupted retries, including after user
policy or selection references have been removed. Saved index data and the
immutable managed reservation remain intact. Detachment does not convert the
installation to an unmanaged runtime, and Ensure or Start cannot recreate the
removed references. Reattach explicitly with the same approved managed
configuration; changing its policy requires a separately supported transition.
Saved state capabilities and checkpoint requirements still apply; read-only
payload policy is not bypassed by a new transport.

## Startup registration and removal

Since 1.8.22, every installation, replacement and upgrade registers login
startup through `runtime login install`. The VS Code installer and standalone
`airk setup` share that native implementation. 1.8.20 registered it only on a
fresh installation.

- **Opt-out.** `runtime login uninstall` writes `startup-opt-out.json` in the
  installation folder, reported as `startup_manager.user_opted_out`. An
  upgrade honors that record and does not register again. `runtime login
  install` clears it. The record is a new installation-folder file, not an
  `airk-state/1` format. A replacement removes the old installation folder,
  so a replaced installation is registered again.
- **No idle exit.** The registered entry runs `runtime managed-serve`. In
  HTTPS mode no service exits while idle, whether it is managed or on demand.
  It keeps running after the editor closes and starts at the next sign-in.
- **One service.** A second `serve`, `managed-serve`, `foreground` or `host`
  start is refused with a `service_already_running:` message. The JSON `code`
  is still `operation_failed`.
- **CLI starts.** With login startup registered, `runtime ensure` and any CLI
  command that needs the stopped service start it through the OS manager.
  They never spawn `serve` themselves. Without a registration, the CLI starts
  the on-demand service.
- **Refusals.** A refused registration (OS policy, or the unqualified macOS
  shared adapter) keeps on-demand operation and is reported. It never fails
  installation. Explorer then starts a service that ended without a Stop
  request once for that stopped instance, and never after an explicit Stop.
- **Windows console.** The sign-in entry detaches from a console that no other
  process shares, so Task Scheduler's console window does not remain open.

Replacement removes the replaced installation's owned registration before
moving its folders; see the next section. The CLI start through the OS manager
still needs the installed process-topology check.

## Replacing an installation through 1.9.0

Through 1.9.0 inclusive, installation fully uninstalls the owned predecessor
and starts with an empty catalog. Installing the same build also requires full
uninstall first. Original sources are preserved. Reconnect cannot create an
index. Ordinary editor reopening reconnects without reinstalling. The
[manual-test correction ledger](simplify/one-process-ledger.md#manual-test-installer-correction-10-november-2026)
retains the failures and validation; 1.8.22 did not satisfy this policy.

1. **Ownership first.** Every location must be provably AIRK's own before
   anything changes. A layout folder outside the default locations needs an
   AIRK owner record. An installation record that exists but cannot be read is
   a typed refusal (`remove_installation_refused`).
2. **Replacement lock.** A `.<leaf>.replace-1.lock` beside the installation
   refuses a concurrent replacement before any side effect.
3. **Login registration.** The owned registration is removed.
4. **User CA.** The owned user CA is retired through its trust receipt.
5. **Folders.** Processes must be proven stopped before folders are moved aside and
   deleted. If a move fails partway, the installer attempts to restore moved
   directories, newest first. Executable names may already have been permanently
   retired; restoring directories does not restore a runnable predecessor.
   Resume the same recorded installation operation. If directory restoration
   fails, every folder left aside is reported as residue. Owned custom index
   storage is included after its native ownership
   and source separation checks. Unproven shutdown or incomplete removal blocks
   successor installation; moving a folder is not shutdown proof.

The command is `airk runtime replace-installation --install-dir <dir>
--candidate-sha256 <sha256> [--reinstall --operation-id <sha256>
[--generation-id <sha256>]] [--apply]`. Without `--apply` it only reports the
plan. Explicit installation uses an operation identity; an ordinary connection
does not force a reinstall. Destructive `--apply` refuses without
`--reinstall --operation-id`; a missing identity cannot create an unrecoverable
cleanup attempt. Native installer receipts beside the install root
deduplicate an accepted operation across windows, retain interrupted cleanup,
and consume a fresh-install operation even when no predecessor exists. A host
installation generation is separate from the verified payload identity; explicit
reinstall consumes its current generation so a later reopen cannot repeat the
uninstall. These are installation metadata, not a change to `airk-state/1`.
Plan and result retain `client_references`, including proven historical launcher
references whose files are already missing, the old endpoint and its CA.

Before `--apply`, the VS Code extension captures the affected agent entries in
project and user-level configuration files into a reconnect ledger
(`airk.reconnect.ledger.1`). After installation, Explorer offers **Reconnect**
for them; see [agent setup](agent-integration.md#reconnecting-after-a-replacement).
Standalone `airk setup` uses the same replacement boundary. Source folders are
never touched.

Definition/image ownership is checked before changing a registration. Uninstall removes only recorded owned native/terminal/client
integration with the required preimage/identity checks; sources and unrelated
or unowned data are preserved. Proven predecessor-owned index storage is removed
by replacement. A trust or process cleanup uncertainty is reported and
retained rather than converted into success. Stop, logical index deletion,
physical derived-data cleanup and full data purge are distinct operations.
Unknown historical or unowned residue is reported and preserved; it cannot
establish completed predecessor cleanup. Replacement does not claim a universal
purge of every historical layout or silently ignore an unresolved owned target.

`runtime inventory` is read-only. Purge requires its exact reviewed plan/digest
and excludes unknown or redirected locations. Index deletion uses the native
retirement contract. See [index retirement](service-project-retirement.md) and
[extension cleanup](vscode-extension.md#removing-the-terminal-command-and-uninstalling).

<a id="unavailable-legacy-index-during-upgrade"></a>

## Unavailable legacy index during the clean break

The current reset never needs to read an old index publication to carry it
forward. It still requires authoritative reviewed source/preference metadata and
proven cleanup ownership. A missing source is reported as unavailable and cannot
be replaced by guessing another folder. Unowned or unreadable legacy artifacts
remain reported cleanup work. The repair/adoption behavior below is historical
migration context, superseded for the pre-1.8.16 boundary.


**Approved follow-up, pending implementation:** automatic fresh-index recovery
with notification after a positively identified incompatible migration must
preserve the previous state and original source boundaries. It is a distinct
native recovery transaction, not the explicit retirement commands below. See
the [automatic rebuild policy and acceptance contract](upgrade-rebuild-fallback.md).
No generic automatic rebuild fallback is currently implemented or qualified.

**9 November 2026:** the in-place catalog upgrade was removed. The
legacy-retirement commands now refuse with `catalog_upgrade_unsupported`, and
the extension no longer contributes the **AIRK: Review unavailable legacy
upgrade index retirement** command; the following paragraph is historical. The explicit legacy-retirement review/resume
commands were administrative
recovery for a candidate-owned unavailable predecessor index. They preserve the
original request and complete receipt, and do not grant evidence access or
recreate missing source/storage. Use the candidate that owns the saved installer
identity. See [the historical recovery contract](reviews/service-installation-before-http-2026-11-07.md#unavailable-legacy-index-during-upgrade)
and current native help for the exact bounded candidate/receipt operations.

## Explicit payload repair

Inspect/diagnose preserve damage and ownership failures. Repair requires the
trusted complete distribution and exact native review, retains unknown files,
and cannot select another ledger or reset sources to manufacture readiness.
The [predecessor repair record](reviews/service-installation-before-http-2026-11-07.md#explicit-payload-repair)
retains its original qualification, not a modern installed-platform claim.

## Development evidence

The [1.8.14 record](releases/release-1.8.14.md) owns current validation and gaps.
The [archived pre-HTTP installation guide](reviews/service-installation-before-http-2026-11-07.md)
retains earlier helper/launcher formats, commands and dated local/hosted results.
Those results do not qualify the new runtime, trust setup or installer.

## 1.8.15 legacy preflight lock correction

Legacy upgrade preflight observes registry capacity and accepted-work compatibility
without opening collection publications while the predecessor runs. The opaque
preflight observation is never a complete checkpoint. Full publication capture
and compatibility verification still require native exclusion after verified
predecessor exit. See the [candidate record](releases/release-1.8.15.md) for evidence
and remaining installed/platform qualification.
