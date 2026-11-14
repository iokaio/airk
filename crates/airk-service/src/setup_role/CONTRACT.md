# `setup_role` contract (I4 writer; split Phase 7)

**Owner:** `airk-service` (`crates/airk-service/src/setup_role/`). **Introduced:**
split plan Phase 7 ([split plan](../../../../docs/split/split-plan.md)
§9.1–§9.6, §10.2; decisions U13–U16, U20, U30–U33, U36, U39, U40, U50, U78,
U82; [contracts](../../../../docs/split/contracts.md) §5.5–§5.7).

## Purpose

The private roles of the signed `airk` image that install, repair, upgrade
and remove an AIRK Service installation (U33: no third executable):

- `airk __airk_role setup --operation <absolute path> [--plan | --prepare]`
  consumes one `airk-setup-operation/1` file and runs the §5.6 transaction.
- `airk __airk_role service-login --install-id <32 hex>` is the entry the
  login registration starts.
- `airk __airk_role setup --operation <original-operation-request.json>
  --seal-inno` seals finalized Inno EXE/DAT custody after native completion and
  wrapper PATH success. This Windows-only mode requires the original successful
  closed receipt, settled active owner and exact current I4 pair/root/image
  proof. It writes Core's private `wrapper-handoff.json`, holding both actual
  EXE/DAT read handles without write/delete sharing while hashing/signature
  checks and the seal write run. On failure it preserves the original request,
  active/main receipts and files and writes `error.txt` beside the request;
  it never emits a valid new handoff or changes prior receipts to success.
- `setup --operation <original operation> --admit-removal` verifies private
  snapshots of the original canonical I4 pointer/record beside the request
  (`expected-current.json`, `expected-install-record.json`). Under the native
  install lock they must equal the exact current original I4 image. Native
  admission persists the Active removal owner and returns `status: admitted`
  before any drain/removal. The CLI then launches the verified wrapper/native
  completion with that same operation. This closes the handoff gap without a
  new SetupOperation field. Existing `--prepare` still refuses removal.

## Interface

| Mode | Runs | Writes |
|---|---|---|
| `--plan` | Read-only disclosure (U13): predecessors found, items that will be left, location problems, the existing installation | `plan.json` and `plan.txt` beside the operation file; stdout = `plan.json`. No lock, no receipt; the operation is not consumed |
| `--prepare` | Before file placement: lock, location checks, predecessor retirement, service drain, wait for installed images to exit | Receipt (open); stdout `airk-setup-result/1` with `status: awaiting_placement` |
| (none) | Every remaining step | Receipt (closed); stdout `airk-setup-result/1` with `status: complete` |

Removal kinds (`uninstall`, `purge`) take no `--prepare`.

The payload inventory `payload-inventory.json` (`airk-service-payload/1`,
written by `scripts/build-local-service-installer.py` for Windows and by the
split modes of `scripts/package-standalone-linux.py` and
`scripts/package-standalone-macos.py`) lists every placed
file (`/`-separated program-relative path → `sha256`, `bytes`), the two
executables, the admin asset digests, the signing identity (or `null`),
version, channel and target. Its SHA-256 is the operation's
`payload_inventory_sha256`. With `--plan`/`--prepare` it is read beside the
operation file; otherwise from the program folder.

## Platform wrappers (§9.4, §9.5)

Each wrapper verifies its payload before the first native execution, writes
one operation per step into a fresh owner-only folder (with the inventory
copy), runs `--plan` (U13 disclosure; acknowledgement prompt or flag),
`--prepare`, places the files, verifies the placed `airk` and runs the
complete mode from it with the same operation.

| Wrapper | `requested_by` | Verified first | Layout (program-relative) | `PATH` |
|---|---|---|---|---|
| `installers/windows/airk-service.iss` | `inno` | pinned `airk.exe` digest | `bin\airkctl.exe`, `service\airk.exe`, `notices\` | user `PATH` entry, `user-path-owner.txt` |
| `installers/linux/install.sh.in` | `install-sh` | pinned roster, inventory and `airk` digests; no links, hard links or special files | `bin/airkctl`, `service/airk`, `notices/`; signing `{platform: linux, signed: false, integrity: record-sha256}` | `~/.local/bin/airkctl` link when that folder exists, `airkctl-link-owner.txt` |
| `installers/macos/Install AIRK.command.in` | `dmg` | the same digests and the `AIRK.app` Developer ID signature | `AIRK.app/Contents/MacOS/{airk,airkctl}`, `notices/` beside the bundle (the signed runtime seals `AIRK.app` before packaging), signing `{platform: macos, team_id}` | same as Linux |

The link receipt `airkctl-link-owner.txt` is three lines:
`airk-airkctl-link/1`, the absolute `…/.local/bin/airkctl` link path, and its
target (this installation's `airkctl`). No wrapper edits a shell startup file.

Admin asset digests (I4 `admin_assets`) frame every embedded asset in name
order with `airk_core::identity::framed_digest`: `inventory_sha256` over
`airk/admin-asset-inventory/1` and each name, SHA-256 and content type;
`bundle_sha256` over `airk/admin-asset-bundle/1` and each name and bytes
(`admin_plane::embedded_asset_digests`). This is the interim definition until
C7's `admin-assets.json` producer lands.

## Authority (before any state is opened)

1. The operation file is a regular, unlinked, owner-private file in a folder
   without links; at most 16 KiB; canonical `airk-setup-operation/1`.
2. First admission: within its lifetime (≤ 10 minutes) by the injected clock.
   Resume: the receipt `<state>/setup/<operation_id>.json` binds the exact
   operation bytes (`operation_sha256`); the operation is decoded against the
   admission time the receipt recorded. A closed receipt refuses a replay
   (`setup_operation_consumed`); other bytes are `setup_operation_changed`.
3. Own image: its SHA-256 equals the inventory's `airk`; in the complete
   mode it runs from `<program>/<executables.airk>`; for a removal with an
   installation at the same locations, it is that record's `airk`.
4. One operation per user at a time: the owner-private lock
   `install-pointer/install.lock` beside the fixed pointer
   (`installation_in_progress`).

Single use: the operation file (and its folder's inventory and plan copies)
is deleted when the receipt closes. A failure closes the receipt with
`failure`; an interruption leaves it open for a resume.

### Private transaction custody and recovery

The reviewed Core `setup_recovery::active::ActiveSetup` is the durable owner
beside the fixed pointer. Setup holds the native installation lock before
loading it. Another operation cannot replace an unsettled owner, including
after expiry. A matching owner supplies its original admission time even
when interruption preceded the first main receipt write. The ordinary I4
reader remains strict: only the exact admitted operation can classify prior
pair, candidate-record/prior-pointer and complete candidate-pair bytes.
Third records and pointer-before-record states refuse without restoration.

Before effects, the native adapter captures nofollow, owner-private directory
handles and native IDs for the selected and original roots. Missing roots
are created in fresh nonce staging, recorded with their parent/object IDs,
then published with a no-replace rooted rename and pinned directory flush.
Unclaimed interrupted staging is preserved; an appearing unbound directory
is never adopted. The candidate record/pointer, including timestamps, are
fixed in the active owner before publication. Every resumed candidate must
still match the exact operation and approved payload inventory.

Keep-data removal saves the original I4 pair, exact uninstall and four native
root observations in the separate Core retained receipt before removing the
pointer. Reinstall additionally requires the matching successful main receipt,
the original four locations and exact live root IDs. Foreign nonempty roots,
incomplete/failed removals, changed roots and ambiguous retained evidence
refuse. Purge uses the held root object on Unix and a name-pinned exact-ID
adapter on Windows, never follows descendant symlinks, and retains root IDs.

A failed active operation retains its original request, payload inventory and
setup image custody. An exact owned retry archives the complete original
failed main receipt bytes at `state/setup/failures/<operation>-<digest>.json`
before reopening that same operation. It removes only the terminal Complete
step from the live receipt; the archived failure is immutable evidence.
Ordinary replay of a closed receipt remains refused. Success settles only
after the terminal main receipt, exact publication/removal and root continuity
checks; a terminal tag or an expired process alone does not free custody.
Before replacing a settled fixed owner, its exact canonical bytes are retained
at `<pointer parent>/setup-history/<operation_id>.json`. This uses the existing
ActiveSetup schema. Later removal verifies the current inventory digest against
that original completed placing operation when this evidence exists. An
installation predating these private records has no invented history proof;
the surviving legacy ownership boundary must be qualified separately.

Candidate build/state preflight runs before drain or placement. Same-build
repair and upgrades with the exact supported state contract preserve state.
A downgrade, unknown version precedence or incompatible state requires both
explicit reset acknowledgement and purge. A placing reset purges after drain
and before placement verification/publication; the four root IDs survive.
The inventory version must equal the setup executable's build before effects.

Native program cleanup uses the original opened program object and nofollow
ancestors, retaining changed/aliased payload files. Windows opens each file
with read/delete access without write/delete sharing, checks the exact-handle
privacy/alias policy and expected payload hash, then disposes that same handle.
After releasing its own program name pins, empty-root deletion reacquires a
DELETE handle under the pinned parent and compares its original native ID
before disposition. A replacement is preserved. Unix uses original dirfds
for approved tracked names and never follows renamed roots or symlinked
ancestors. It does not claim atomic content comparison plus unlink against
arbitrary uncooperative same-user writers; current wrappers honor the native
transaction fence. Empty Unix program roots/directories are preserved and
reported because no equally strong identity-bound directory removal has been
qualified. No pathname-removal fallback invents continuity evidence.

Installer::Portable uses native program cleanup on Windows too. It never
delegates to Inno or consumes PATH, login or trust integration ownership merely
from portable locations. Its retained external recovery runtime includes the
full approved payload inventory/bundle, so removing tracked program files does
not erase the setup executable's dependencies or original request.

## Transaction (§5.6)

Pure in `plan.rs`, tested against `fixtures/split/i4/interruptions.json`.

Placing kinds (`install`, `repair`, `upgrade`): `lock` → `retire_predecessors`
→ `drain` → [`--prepare` stops] → `verify_placement` → `write_record` →
`write_pointer` → `register_login`? → `start`? → `apply_trust` → `close`.

Removing kinds: `lock` → `drain` → `unregister_login` → `remove_trust` →
`remove_pointer` → `remove_record` → `purge_data`? → `close`.

Purge/reset removes contents while retaining the native directory identities
of data, state, cache and logs. State additionally retains `setup/` receipts.
Empty roots remain deliberate ownership/recovery evidence; a third directory
at the same path is never adopted as the original root.

- `lock` validates locations every run. Placing kinds (U39/U50): absolute,
  below a top-level folder, no links, no nesting between the five locations,
  no overlap with any pre-split root (both channels and layout families of
  `user_paths`, the pre-split Inno `standalone` folder) or the pointer folder;
  an existing location folder is owner-private (`owned_folder`); the nearest
  existing ancestor of one that does not exist yet is not writable by others
  (`safe_parent`; Windows `require_not_writable_by_others` accepts read-only
  grants to other accounts, U96; Unix the `owned_folder` rule); in
  `--prepare` on Windows the program folder is then created owner-only
  (`prepare_root`) before placement, as the I4 readers require it private;
  each chosen folder is absent, empty, the
  existing installation's, or (program, complete mode) holds only inventory
  files, the record, the inventory, Inno's `unins###.{exe,dat,msg}` and the
  wrappers' `user-path-owner.txt` or `airkctl-link-owner.txt`.
  Removing kinds: the operation's locations equal the installation's.
- `retire_predecessors` (U13/U14): every existing pre-split install root is
  planned and retired through the existing receipts-only
  `runtime_install::replace_installation` (operation identity
  `framed_digest("airk/split-setup-retirement/1", operation_id)`, candidate =
  the payload inventory digest). U13 acknowledgements are required when any
  root needs removal. A root that cannot be retired is a leftover with its
  reason; a leftover that can restart an old service is listed first. The
  pre-split Inno `standalone` folder is reported, not removed (U74 pending).
  A pre-split shared-manager receipt names the pre-U93 command
  `runtime managed-serve --install-dir <root>`; `service_manager` accepts it
  (with the interim form) for ownership and removal only, never for launch.
  Each setup operation has its own retirement identity, and the installer's
  operation folder does not outlive the installer, so an earlier operation
  cannot be resumed. A replacement journal (`.<root>.replacement-1`) left
  incomplete before its freeze (nothing frozen, no executable admitted, every
  location at its original path with its recorded identity and owner seals)
  is taken over by the next operation, which plans afresh and keeps the
  earlier identity consumed. A journal past its freeze resumes only under its
  own operation and otherwise stays `replacement_in_progress`.
  On close, leftovers are written to `leftovers.txt` beside the operation
  (`leftover_notice`), which the Inno installer shows after placement.
- `drain`: for the operation's and the existing record's control roots, a
  `Health` from this user's service whose reported image SHA-256 is the
  existing record's or the payload's `airk`, then `Shutdown` fenced to its
  instance and the exit of its process (60 s). First, when installed images
  exist and the step places or removes files, setup writes the I4
  replacement notice (`replacing.json` beside the pointer, ten minutes) so
  editor windows end their `airkctl` sessions; the notice stays through
  placement and is removed when the receipt closes (complete or failed).
  Before placement or removal,
  every process running exactly an installed image (except this one) must
  exit by itself within 15 s (`installation_files_in_use`); nothing is
  terminated. Re-observed until `placed` (or `login_unregistered`) is durable.
  Process observation (`legacy_processes`) counts as unobserved only a
  process that could be this user's: on Windows an `OpenProcess` denial (5,
  another account) or invalid parameter (87, exited) is not a miss, nor is a
  failed image or owner query on a process whose open handle reports an exit
  code other than `STILL_ACTIVE`. An incomplete scan is observed again within
  the same 15 s; at the deadline a running image is reported
  (`installation_files_in_use`), otherwise `process_observation_incomplete`.
- Old editor extensions: for placing kinds, `--plan` reports and `--prepare`
  refuses (`old_extension_running`) while any process runs the native `airk`
  of a pre-split AIRK editor extension (`plan::pre_split_extension_image`:
  `…/extensions/iokaio.airk-*/…/airk[.exe]`), because that extension sets the
  pre-split runtime up again after retirement. `plan.json` carries them in
  `blockers`; the Inno data page does not advance while either
  `location_problems` or `blockers` is non-empty and warns to update or close
  VS Code first (a closed editor cannot be observed). The Unix installers stop
  at the `--prepare` refusal.
- `verify_placement`: every inventory file matches size and SHA-256; version
  equals this `airk`; the placed `airkctl --version` reports the same
  version; admin digests equal the embedded bundle's; the inventory names a
  signing identity (`payload_unsigned` otherwise) and both placed images pass
  the I4 readers' signature check (`airk_process::signature::verify`, the one
  implementation `airkctl` uses: Windows Authenticode / macOS `codesign`
  against the C10 identity, Linux digest only); a refusal keeps its code
  (`signature_unverified`, `signing_identity_mismatch`,
  `signing_platform_mismatch`).
- `write_record`: `install_id` is kept for the same program folder, otherwise
  derived from the operation (`framed_digest("airk/install-id/1",
  operation_id)`), so a resume rewrites the same record. Owner-private,
  temporary file + rename.
- `write_pointer`: last of the placement steps, only for a record the I4
  reader accepts; owner-only folder, temporary file + rename + folder sync.
- `register_login` (Windows Task Scheduler, Linux systemd user unit) through
  `ServiceManager::for_installation` (`airk-service-<hash>`, definition and
  receipt under `<state>/login/` on Windows): registered without starting.
  macOS uses the exact placed I4 bundle's SMAppService adapter described below;
  approval requirements and refusals are reported. A failure is recorded
  (`login_failed`) and makes completion unsuccessful.
- `start`: the installed `airkctl --output json service start` (U94, one
  start implementation). A failure is recorded and reported when the receipt
  closes (`service_not_healthy`, non-zero exit), never hidden.
- `apply_trust` after the start (the HTTPS issuer exists only after the
  service's first start; a recorded deviation from the §5.6 table order):
  `skip` → `trust_declined`; Linux → `trust_unsupported`; otherwise
  `http_endpoint::trust::manage(Install)` with the OS consent dialog; no
  issuer yet or a failure → `trust_declined` with the reason.
- Removals: owned login registration, owned trust (its receipt) and the
  pointer (only when it names this installation) and the record. On macOS
  and Linux, where `airkctl service uninstall` is the uninstaller, the
  `record_removed` step also deletes the placed files: each inventory file
  that still matches its size and SHA-256, then the inventory (matching the
  operation's digest), then the running `airk` last; the link named by
  `airkctl-link-owner.txt` only while it points at this `airkctl`, then the
  receipt; then the folders left empty and the program folder. A changed or
  foreign file is left and listed as a leftover (`plan::program_removal`,
  `plan::owned_link`, `plan::link_action`). An interruption after the running
  image is deleted cannot resume; its receipt stays open. Windows leaves the
  files to the Inno uninstaller. Index data,
  state, cache and logs are deleted only by `purge` or `delete_indexes:
  purge` (U32; `ask` keeps); the state folder keeps `setup/`; a location
  containing the program or pointer folder is refused.

## Remaining qualification and preserved recovery limits

The U40 first-launch fallback inside `AIRK.app` remains separate from the
implemented SMAppService adapter. Actual Mac registration, OS consent and
signed-package notice placement require their platform/artifact qualification.
Moved-program cleanup retires only matching proven payload entries and retains
the old exact I4 record/root for recovery. The root-independent predecessor
roster now discovers orphan registrations, standalone/editor/PATH remnants and
saved schedulers; observation never authorizes name-based deletion. Unproven
ownership remains explicit preserved leftovers, as detailed below.

## Service locations (U89)

`service-login` adopts the record's locations before serving
(`service_locations::adopt`); `service-detached` and `record-error` adopt them
when their control root is the record's `locations.state`
(`service_locations::adopt_for_control`). The log rings of that control root
are then in `locations.logs`, and `airkctl service logs` reads them there
(`airkctl::logs::locate`). State is the control root; data roots are chosen
per registration by the client (`airkctl` defaults them to `locations.data`;
AIRK Admin to a folder named after the collection inside it);
the service derives no cache folder. `service-foreground` adopts nothing.

The record step (`WriteRecord`, every placing kind) first makes the record's
`data` and `cache` folders exist, created owner-only like the state folder
(`files::prepare_locations`). An index setup review needs the data folder;
a fresh install, and a reinstall after "delete data", have neither (1.8.32
installed without them, so every setup draft failed).

## Login startup from the service (U101)

The serving service manages its installation's login registration (M21,
`service_runtime::login`) only through this facade: `login_target(control)`
names the installation whose `locations.state` is the service's control root
(else `None`); `login_registered` reads `owned_registration_exists`;
`login_install` and `login_uninstall` hold the setup lock (`install.lock`,
so they never interleave with a setup or removal) and call the same
`effects::register_login`/`unregister_login` with the same
`ServiceManager::for_installation` identity. Both converge
(`already_registered`, `absent`). `login_uninstall` refuses
(`login_startup_in_use`) while the registration's own instance runs, because
removing it ends that instance, which is the serving service.

## T0

```console
cargo test -p airk-service --lib setup_role --locked
cargo test -p airk-cli --lib role_admission --locked
cargo test -p airk-host --lib service_locations --locked
cargo test -p airk-core --lib install_record
```

Each Cargo selection must report nonzero selected tests. Record selected test
identities and outcomes; compatibility re-exports do not include dependency
unit tests. Platform-only cases require their native host.


Active owner, terminal history, retained-root proofs, failed receipts, program
record/inventory, fixed pointer and main receipt publication use shared rooted
Process operations through IPC. The owner passes its original authority/program/
state directory handles for the final writes; pathname rechecks are not used
as a replacement for those handles. Root staging creation is relative to its
held parent. On Unix rebinding writes remain within the original directory;
subsequent live-root validation refuses changed names. Windows retains ancestor
name pins. Owner-private staging files are synced before rename and parent
sync errors propagate. Arbitrary uncooperative same-user changes outside the
operation fence remain outside the cooperative installer serialization proof.


## I4 macOS login adapter

Split signed plists invoke `__airk_role service-login-current`; this derives the
installation ID only from strict current I4 and verifies the executing service
path, bytes and C10 before automatic startup. Historical explicit-ID
`service-login` syntax is unchanged. The static signed plist does not contain a
per-install random ID. `service-login-control --install-id <id> --action
status|register|unregister [--operation <absolute-original-request>]` runs only
from the exact placed I4 AIRK.app. A setup request must match the durable active
owner and original receipt/roots; ordinary service mutations acquire install.lock
and refuse any unsettled owner. Status cannot authorize mutation. The same
SMAppService Objective-C bridge validates the exact split plist dictionary;
legacy managed plists keep their original branch. RequiresApproval is reported
as consent-required; registration never claims Enabled from API success alone.


A moved-program setup drains both exact old/new image sets and unregisters the
old I4 login target before publishing the new pair. Failure to remove that
registration halts placement. After publication, cleanup uses the original
pinned prior program root and the previous settled operation's approved raw
inventory digest. Only matching listed files/inventory are deleted; changed,
foreign, aliased or unprovable files remain explicit leftovers. The old exact
I4 record and old directory root remain as recovery custody because the current
publication contract requires their continued identity. They do not authorize
startup or make the old location the current installation. No recursive old
program deletion or unverified metadata adoption occurs. Missing historical
inventory proof leaves the old payload intact and reports manual recovery.

Before removing any moved prior payload entry, the exact approved raw existing
payload inventory is retained at selected-state/setup/prior-inventories/
<current-operation-id>.json through the private rooted writer. This is the
existing payload format and original digest, not a reconstructed inventory.
Crash resume reads that copy before considering any absent old program file.


Predecessor discovery enumerates both historical and migrated runtime roots
unconditionally for stable/preview, plus every verified fixed universal-launcher
selection (including custom roots). A layout preference is not an absence proof.
Discovery returns paths only; existing native replacement plans must still prove
ownership of every data/state/log/settings/launcher/trust/registration artifact.
Malformed or missing selected launcher targets are reported, never guessed.

The [predecessor orphan roster](predecessor_roster/CONTRACT.md) also runs when no
install root survives. Native registration and saved receipt/path observations
locate candidates; names never authorize deletion. Unknown, refused and unproven
items remain explicit preserved leftovers. Bounded merging prioritizes restart
risks and records an omission marker rather than silently dropping evidence at
Core's existing limit. A predecessor step with remaining observations reports
`leftovers_preserved`, rather than claiming complete retirement.

A legacy I4 image without completed private inventory history grants cleanup
only for its exact two record-bound executable paths/digests/sizes. An editable
unbound inventory cannot add deletion targets. Support files and the unproven
inventory remain visible leftovers for original-installer recovery. Current
settled-history proof enables the full exact listed-file cleanup path.


Wrapper recovery uses exclusive `--recovery-field <closed-selector>` with the
original operation. Native Core canonical/private decoding, original admitted
key/time and live roots, actual native image/inventory/C10 authorize projection.
Fresh unadmitted valid-TTL requests are reviewable but still require Prepare's
full preflight before placement; competing unsettled owners refuse. Selectors
include requested-by, kind, operation-id, inventory-sha256, all five locations,
login-startup, trust, start, placement-required and completion-required. Each
returns one exact UTF-8 scalar (no BOM/newline/control) on stdout and atomically
writes fixed owner-private recovery-<selector>.txt beside that same request.
These derived files grant no independent authority; wrappers read only after
successful verified helper exit. Completion-required is false only for exact
settled successful original receipt and current pair/removal reconciliation;
placing success additionally verifies full actual C7/C10/inventory. Read-only
Active observation classifies interrupted metadata without archiving/retiring
an owner or weakening ordinary I4 reads. Wrappers preserve their outer cleanup.

Core Close durably retains the exact original canonical operation bytes at
operation-request.json (digest/equality checked, conflicting backup refuses)
and retains its original raw inventory outside tracked program files. Successful
core completion cannot retire those inputs before outer PATH/seal/tracked
cleanup succeeds. A resume using the backup never deletes that backup. Normal
operation.json remains consumed; no new operation/key or reconstructed inventory
is created. Recovery projection can therefore reconcile successful removal even
when the outer uninstaller already removed the program's inventory.

Outer producer exclusion uses an actual kernel lease, not an expiry or env
boolean. Unix --wrapper-lease-script execs /bin/bash with only the original
--resume request (+--silent when --wrapper-silent); the exclusive private
wrapper.lock open-description is inherited by shell copy/mv descendants. Native
setup children verify the inherited file identity/privacy/lock and mark their
own descriptor close-on-exec, so their resident service descendants cannot
retain it. Windows wrapper-lock-handle creates/validates an exact-user protected
Global mutex, duplicates an unowned handle to the kernel-captured birth-checked
launcher. The Inno producer thread must acquire it, hold through placement and
outer finalization, then release on that same thread. Kernel abandonment follows
actual owner death, never a supplied PID or time limit. The environment FD value
is only a lookup hint and never authority. Fresh requests still need full native
Prepare admission; same-key recovery uses the exact admitted original bytes.

The Windows opaque HANDLE scalar is create-new only in a fresh per-producer
private projection directory. Inno copies the exact original request and raw
inventory bytes there, retaining their hashes; these diagnostic copies share
the original key and grant no new admission. Every scalar query uses that
producer's projection context. Prepare, Complete and final seal retain the
original operation path. Concurrent same-original producers cannot overwrite
each other's process-local numeric HANDLE rendezvous.

Inno NativeComplete seals finalized EXE and DAT custody immediately after
PairPublished and before login registration or StartRequested. This pending
writer requires the typed active owner, identical original operation and
published candidate, unchanged roots, full C7/inventory and actual C10 proofs.
Ordinary and candidate-continuation startup both require the matching private
handoff plus actual EXE/DAT hashes and uninstaller signature. The outer
--seal-inno revalidates the same record after PATH; it does not mint a new key.
A pending handoff alone never grants removal: completed private inventory
history remains mandatory. Missing or sibling-operation custody refuses start.

Interrupted placing resumes that already recorded PointerWritten repeat the
same typed-owner seal before each remaining RegisterLogin/Start action. They
reverify full payload and actual original candidate/root proofs before invoking
the existing verified writer. A missing custody record can therefore recover
under the same operation; changed/foreign I4 and sibling operations never reach
the writer. The existing startup custody refusal remains mandatory.

## Platform placement of removal/seal helpers

The removal directory-order snapshot is compiled only on Windows, alongside
its exact-handle empty-directory cleanup consumer. All hosts still inspect
and remove the same proven file-entry tuples and retain the original root
identity validation. Unix preserves unsupported empty-root removal as an
explicit leftover; a path recheck is never substituted for an atomic proof.

transaction::completed_for is private Windows-only reconciliation for the
Inno final sealer, its only production caller. Its full settled-owner, root,
original operation and receipt validation is unchanged. Shared admission_time,
startup_allowed, Active observation, interrupted owner recovery and scalar
completion reconciliation remain compiled for all supported hosts. These cfg
placements do not remove live macOS or Linux recovery duties.
