# `airkctl` contract module (I2)

Owner of the I2 contract between `airkctl` and its callers: the VS Code
extension, scripts and users (split plan §4.2; draft in
[contracts](../../../../docs/split/contracts.md) §3).

## Interface

`contract::agent_entry` owns the I2 Entry, Agent, Scope, Class and Transport
data and their unchanged strict decode/schema rules. Agent configuration keeps
classification, finding/label behavior and editing, reexporting those same
types for existing callers. Schema generation depends on contract data alone.

Session ingress has sixteen queued messages (twice the existing eight running
requests), plus at most one decoded reader frame awaiting admission. Queueing
backpressures the input reader; the actor never waits on output or a full queue.
Clock wakes are coalesced when the message lane is full. Reply buffering has
eight frames and `8 * (16MiB + 4)` bytes, plus one active OS write, separately
from the existing bounded event buffer. A saturated reply lane closes this
session's authority and retains its queued replies; it cannot promise delivery
of further results or undo accepted native effects. No request is automatically
replayed. Timers, cancellation and worker completions remain actor events;
workers recheck cancellation and original deadline before first dispatch.

The native Admin client adds `admin-management/1` without changing protocol2 or
the old frame limits. Core owns the reviewed
[native Admin contract](../../../airk-core/src/native_admin/CONTRACT.md).
AdminCall carries operation, raw JSON body and original bounded deadline_ms;
AdminRead and AdminCancel name that original outer request ID; AdminClose ends
only this client's cached native session. AdminChunk uses the Core-generated
typed chunk; AdminClosed confirms its captured close. Cookie/CSRF never enter
I2 fields, native values, stdout/stderr or debug output.

RawValue is retained from the framed input before any Value conversion so its
64KiB body budget/depth is not changed by intermediate canonicalization. The
same-host deadline starts at complete frame receipt, before the bounded input
queue, and is preserved through actor/worker admission and subsequent reads.
Cancellation is shared with workers and propagated via the dedicated native
cancel call. Failure after native dispatch is nonretryable
`admin_outcome_unknown`; it never authorizes reopen-and-replay. An actual native
`admin_session_invalid` reply instead produces nonretryable
`admin_session_ended_outcome_unknown`, including after earlier chunks. This
ends the captured cached context, preserves uncertainty about effects and
cannot automatically open another session. Other native errors retain the
ordinary unknown outcome; no HTTP status or I3 body is fabricated. Close/rebind
fences pending opens and stale deliveries; unknown close keeps the cache closed
until a verified new context. Recovery handles remain limited to the current
native cookie: process death/expiry cannot pretend to resume that authority.

`service uninstall --output json` returns a pending launch observation, never
proof of removal: `started`, original `operation_id`/`kind`/`delete_indexes`,
`process_id`, native `receipt`, plus `operation` (the exact retained private
request) and `recovery_program` (the verified runtime retained outside tracked
program files). The caller preserves those paths after failure and resumes
that original operation; it never reconstructs a new request to hide failure.
Native admission first binds the exact original canonical I4 snapshots before
the detached handoff. Windows Installer::Inno selects one signed uninstaller
whose actual EXE and finalized DAT match Core's private original-operation
handoff receipt. Missing legacy evidence or ambiguity refuses automation.
Portable ownership always uses native removal and owns no Inno/PATH/login
integration. Native receipt closure alone does not prove wrapper removal;
the wrapper owns its separate completion observation.

Usage/Help pilot projections are retained in the Rust-emitted
[semantic cases](../../../../fixtures/split/ui_pilot/README.md). I2 preserves the
complete native document or MCP result and its failure streams; the fixture
contract distinguishes those bytes from the I3 envelope. The live command and
the cases use the same private output formatters.

Online commands share one admission gate after their existing target/input
resolution: probe native health; only a missing service can start automatically,
using the verified matching installation and `StartMode::Automatic`. Saved Stop,
pair mismatch, native refusal and uncertain transport errors are preserved.
Tools/catalog/open/window reads, library, folders, projects and storage reads
use this gate, as do the existing project evidence/control paths. Admission
does not renew a window lease, change a selection, replay a mutation or reset a
request deadline.

`version`, parser help, `service status`, `service health`, `service shutdown`,
explicit lifecycle actions, login-startup operations, setup-receipt observation
and offline diagnostics remain outside automatic admission. Explicit Start or
Restart keeps its existing intentional lifecycle owner. Root selected-index
`status` remains an online evidence read. Pure target resolution remains usable
by nonstarting observations; online helpers never replace its ownership or
explicit runtime-directory rules.

The `test-support` feature exposes only a Rust-scoped installation-discovery
fixture. Its factory is thread-local and restored by RAII, including nested
panic unwinds; normal discovery remains unchanged and there is no environment,
command-line or wire override. In-process CLI fixtures explicitly capture a
NoInstallation witness into their worker when testing missing-service refusal,
so a user's real installed pair cannot change those fixture preconditions.

The `online_admission` unit regressions cover running/exited/stopped/mismatch
outcomes, an independent parser-accepted command-family/override roster and
window authority preservation. These injected controls do not establish a real
installed-process startup journey; that qualification uses the matched pair.

- **Exit classes** (`ExitClass`): codes 0–8 — success, refused, usage,
  service_missing, incompatible_pair, unsupported_capability, outcome_unknown,
  installation_unverified, internal. The numeric codes are part of the contract.
- **Error line** (`ErrorLine`): the last stderr line under `--output json`,
  `{"airkctl_error":1,"class","code","retryable","retry_after_ms"?,"message","native"?}`;
  message ≤ 4,096 characters; `deny_unknown_fields`; `validate()` rejects
  another line version, non-snake-case codes and the success class.
- **Version document** (`VersionDocument`, schema `airkctl-version/1`): local
  only, never contacts the service. Readers ignore unknown fields.
  `capabilities` lists only served operations of this exact pair.
- **Exact agent-entry removal input:** the existing
  native agent-config `Entry` is the I2 schema root for a repeatable
  `agents remove --entry <JSON>` argument. Its camel-case report representation
  stays unchanged. Unknown fields, missing identity fields/status, unknown enums
  and noncanonical connection IDs are refused. The explicit `status: null`
  observation remains valid. Native custom keys and container paths are carried
  unchanged, including custom keys recognized through a legacy AIRK credential
  marker (U110) or a verified launcher.
  The contract landed before the handler. The served handler advertises
  `agents-entry-remove/1`; consumers require it from the verified CLI.
- **Agent connect (U110):** `agents connect --client <claude|codex|vscode>
  [--project <folder> | --user] [--server-key <airk…>]` writes an entry naming
  only the live loopback endpoint URL and `X-AIRK-Response-Mode: terse`, and
  prints `{"schema":1,"client","scope","file","serverKey","url","changed"}`.
  It enrolls no connection: `--connection-id` and `--selection-file` are
  refused, `--open` is accepted and is the default. `agents repair` rewrites
  entries that name another endpoint or carry legacy credential fields to the
  URL-only form and ignores `--connection-id`. Its actions and `agents remove`
  actions keep their existing fields, `connectionId` included (the id a legacy
  credential field names, else null).
- **Workspace defaults (U110):** `scope default --workspace <absolute folder>…
  [--index <id|name>]… [--set <id|name>]…` stores the default scope of each
  folder in the service (no selector clears it); `scope defaults` lists them.
  Both print `{"schema":1,"entries":[{"folder","index_ids","index_set_ids",
  "applied_unix_ms"}]}` with every stored default. The documents are not
  schema roots of the generated I2 schema.
- **Session protocol 2** (`SessionRequest`/`SessionResponse`/`SessionEventFrame`):
  4-byte big-endian length then JSON (the shared codec). Replies keep the
  protocol 1 `{id, outcome}` shape; events are `{event:{kind,…}}`. Budgets are
  the `MAX_*` constants. Cursors are scoped to one service incarnation and one
  window. No frame carries a token, grant, lease or launch code.

## Invariants

- Extension consumers observe every session child retirement/replacement as a
  single `session_lost` boundary. Subscription and Assistance acknowledgment
  callbacks are fenced to that generation; late old-child callbacks cannot
  suppress a replacement subscription or acknowledge its sequence numbers.
- Ordinary Workbench project reads use the active W4 window authority before
  the project fallback. Explicit named and reviewed session selections retain
  precedence. Client, status and late-result identity include effective implicit
  window authority and admission state; stored turn/session formats stay unchanged.
- Contract values change only with a reviewed fixture change. Expectations in
  the fixtures are written independently of this code.
- Window-plane frames (`scope_select`, `scope_release`, `assistance_ack`,
  `handoff_publish`) follow `fixtures/split/i1/window_plane.json`; a service
  refusal of one is `refused {code, retryable}`, distinct from session-level
  `failed`. The window lease never appears in a session frame.
- Native scope mutations share one session-local gate: explicit W4/W4b and
  the one automatic W4 reselection cannot pass one another. Committed scope
  and generation live in shared state and publish immediately after native
  success, before the completion event; failed/cancelled reply delivery never
  rolls back a completed native mutation. A failed selection preserves the
  last successful scope. Completion effects cannot restore an older scope.
- Lock order is mutation gate then short scope-state lock. Catalog lookup,
  ordinary reads and automatic retry never hold the mutation gate. Automatic
  reselection observes the catalog outside that gate, then checks its captured
  plane epoch and committed generation under the gate before sending W4.
  A retry checks its committed generation before execution and before accepting
  its output; if superseded it returns the original refusal. This does not
  change the initial read's live-window routing contract.
- Restart/plane reset advances the epoch and clears shared scope without
  waiting for a native mutation call; an old-plane completion cannot publish
  state into the new plane. Session close advances the scope generation,
  clears scope and closes further scope mutation/commit without changing the
  ordinary invocation drain/exit rules. The scope-state lock never spans
  native calls. A poisoned mutation gate admits no further scope mutation.
- The session `control` relay forwards exactly the operation-table rows
  (`fixtures/split/i3/admin_operations.json`) that list the `session`
  surface (contracts §3.8, U103); others are `session_command_refused`.
- `airkctl` depends only on `airk-core`, `airk-process` and `airk-ipc` (U27).
- Exact-entry removal requires `agents-entry-remove/1` from the
  verified CLI. It conflicts with `--connection-id`, refuses missing, duplicate,
  changed or out-of-scope selected rows, and never falls back to all entries.
  Compare the complete observed `Entry` again against live source inside the
  existing config-edit boundary before deletion. Equality covers observed
  identity/classification, not arbitrary declaration bytes. Existing unrelated
  bytes and the actions/blockers result shape remain intact; per-action errors
  must remain visible. Existing invocation argument/frame limits apply to raw
  selectors; a row too large for those limits is refused, never reduced.

## Fixtures

`service setup-receipt` is an offline observation of the receipt named by the
current verified I4 record's `installed_by.operation_id`. It accepts no arbitrary
receipt path and performs no start, setup, retry or mutation. Its generated I2
`SetupReceiptObservation` retains the full Core-decoded receipt. `successful`
requires a matching placing operation, a closed receipt without failure, and
ordered successful record/pointer publication followed by terminal completion.
Health or the presence of installed files cannot substitute for this proof.

Private agent-edit retention records have a separate
[config contract](../agents/config/CONTRACT.md). Existing I2 shapes remain
unchanged. Connect file errors use nonretryable `agent_config_edit_refused`,
`agent_config_edit_conflict_retained`, or `agent_config_edit_outcome_unknown`;
the latter uses the existing outcome-unknown exit class. Conflict explicitly
discloses that proposed bytes may be published. Under U110 connect names no
connection; the private manual-recovery location remains visible on later
failure.
Repair/remove preserve per-action errors. No restore/discard capability is
advertised; record fixtures do not qualify native publication by themselves.

`fixtures/wire/airkctl_session_frames.json`, `fixtures/split/i2/exit_codes.json`,
`fixtures/split/i2/error_lines.json`, `fixtures/split/i2/version.json`.
`fixtures/split/agents/entry_remove_cases.json` pins the new input's exact
shape, strict decoding and existing transport budgets. Actual subprocess,
session and configuration-edit tests cover selector custody and revalidation;
extension composition tests cover reviewed scope and complete action errors.
The compatibility matrix and offline-operation fixtures land with their first
consumer.

## T0

```console
cargo test -p airkctl --lib contract
cargo test -p airkctl --lib cli::session::tests --locked
```


`service setup --portable --program <absolute-runtime>` is an explicit offline
setup operation. Optional `--data`, `--state`, `--cache`, and `--logs` select the
native operation locations; an existing portable repair retains its four roots.
The front door validates the full C7 candidate, current I1/I2 versions, the raw
outer payload inventory, actual listed files and C10 before running native setup.
Unlisted program files are refused. Existing canonical I4 metadata can identify a
damaged installation for repair without manufacturing a `Verified` discovery.
The native owner admits the exact inventory/root proofs and publishes its copy of
the inventory under the setup lock. The source is `airkctl-portable`; login
startup and automatic start are disabled (there is no trust option since local
trust plan C). Predecessor removal and
index reset require their separate supported review flow. Success returns the
existing generated `SetupReceiptObservation` for the exact operation after full
I4 discovery and successful receipt observation. Failure retains and reports the
original private request; the command does not reconstruct a new operation.

Portable relocation may select a newly extracted approved runtime while keeping
the four original data roots; native setup waits only prior-program sessions,
so it does not wait for its own CLI in the new runtime. Full native invocation
has the operation's bounded ten-minute lifetime allowance (the native drain
alone can take sixty seconds). After Complete/admission is sent, loss of proof
or handoff failure returns nonretryable OutcomeUnknown and the original
recovery path. Pre-admission refusals retain their original error class/code.
No timeout or disconnect silently retries a new operation.


Public portable recovery is `service setup --resume <absolute-original-private-
operation-file>` (optional redundant `--portable`). It conflicts with program
and location overrides and accepts no PID/kill/adopt control. It never creates a
new operation. Canonical Core bytes, private custody, portable source/options,
exact Active key/digest/original admission time and every live selected/prior
root are checked before same-op native recovery. Expired requests resume only
under that exact Active owner. A consumed successful operation reconciles its
current full C7/C10/inventory and receipt without replay. A changed request,
unknown owner or consumed unproven request refuses. The original raw inventory
is used, never reconstructed. Native completion uses the existing R10 detached
launcher and exact launched-child observation; timeout never kills admitted
setup. OutcomeUnknown carries the original operation path and observed child
PID as diagnostic context only. Repeated native invocations converge under the
same durable owner/lock. Service, receipt and custody identity, not PID, prove
completion; no arbitrary process is attached or stopped by this command.


## Invoke canonical read lifetime (A6 correction)

The D06 session transport envelope is at most 125000 ms: the unchanged
120000 ms native operation ceiling plus 5000 ms for transport/delivery. Invoke
and AdminCall admission use that same envelope. Their one deadline starts at
complete frame receipt, including actor/worker queueing; delivery overhead is
added once by the caller, never on retry, scope reselection or native dispatch.
The historical `airkctl_session_frames.json` keeps its literal 120000 ms control;
`fixtures/split/i2/transport_budget.json` records the new native-observed
125000 accepted / 125001 refused boundaries for both methods.
Its original request ID, native monotonic expiry and cancellation state survive
actor/worker queues and the one already permitted predispatch scope reselection.
The internal Io/command seam carries that parent only into actual canonical
ToolsCall reads through Core's optional ReadAttemptBudget; it does not rewrite
caller argument JSON or manufacture a new allowance after selection.

The new M3 field follows [Core's owner contract](../../../airk-core/src/read_attempt/CONTRACT.md):
missing/None old bytes remain identical, Some uses old strict refusal without a
fallback or fabricated capability gate. Same-version identity alone does not
prove an older development service supports the field. A sent/lost response
retains existing nonretryable/unknown classification. Core's125s read maximum
and ordinary budgets above30s remain supported; Answer intersects its original
parent remaining with its serialized120000ms ceiling (relaxed from 30000ms in 1.8.201), and NativeNextRead keeps
the approved 120s operation cap. None retains125s canonical/135s transport
response behavior; the 125s I2 envelope does not extend native work. It is a
bound on waiting/delivery, not a fresh per-stage allowance.

Generic Invoke can perform nonread operations too. Do not interpret this read
carrier or session timeout/cancel as proof an admitted mutation did not run,
permission to replay it, or authority to discard foreground custody. Expiry or
cancellation before a new read submission forbids that submission; already
accepted work retains its owning completion/settlement obligations.

The shared SDK floors one sampled remaining duration before sending Invoke or
AdminCall. A zero or negative floor uses the existing not-sent
session_deadline_exceeded path; it never grants a minimum1ms after exhaustion.
A changed compiled SDK requires a new scientific protocol/input freeze. Earlier
calls, samples and unknown failures remain immutable under their old identities.


Arbiter management consumes Core configuration/3 and status/2. Configure files
carry one explicit stage and complete native per-stage choices/enrollments.
Cancel and Recover require `--stage focus|context` and the actual common expected
revision; profile-bound Provision/Remove derive the stage from native admitted
profile or original retained custody. `arbiter initialize` is explicit and
accepts `--expected-legacy-configuration SHA` when retained legacy configuration
exists; omission means actual absence, never permission to migrate or overwrite.
Status/startup never initializes optional state or enables/downloads a model.


`arbiter import --expected-revision N --profile SHA --directory ABSOLUTE` uses
only the private native ArbiterImport request and required typed status response.
The CLI passes the exact original UTF-8 native absolute directory string through
Core validation; it never pre-canonicalizes alias/traversal evidence. The owning
service opens and verifies that local bundle read-only against the actual
compiled qualified profile. A caller path supplies no profile/code/consent or
network authority. This operation has no public Admin management counterpart.

## Final-plan D04 additive agent setup output

`agents endpoint` retains the checked endpoint fields and adds `manual`, three
Rust-owned project-relative `AgentManualEntry` values. This presentation field
is outside the persisted endpoint descriptor, its schema and its digest.
`agents manual --client <claude|codex|vscode> [--project <folder>] [--user]
[--server-key <airk…>]` returns one entry naming the exact resolved target;
it is read-only and valid through the ordinary session command path.
`AgentManualEntry` and `AgentConfigRefusal` are generated I2 schema roots,
shared directly from Core with I3. Connect success adds `manual`; failures keep
the existing error class/code and expose typed recovery details as described in
the agent-config contract. Full original native errors remain available.

The verified CLI advertises `agents-manual/1` only when the read-only manual
command and typed connect recovery projection are served. Older clients can
ignore the additive field; consumers can report unavailable support without
attempting an unrecognized command.

`library set-create --index` parses every supplied index identity and refuses
duplicates before automatic service admission. Invalid local selectors never
probe or start a service; valid selectors keep the original admission and
reviewed catalog mutation path. CLI parser/output fixtures use thread-local
missing-installation discovery; separate installation/admission regressions
retain exact pair/signature and refused-start checks.

U110 `agents connect --selection-file` without a connection identity is also a
retired-source refusal. It is checked before installation discovery, target
resolution or service admission, and never falls through to URL-only editing.
Existing connection/selection combinations keep their explicit retired-source
refusal; modern `connect` omits both flags.


## D08 additive native device-management transport

The current command `arbiter status` dispatches Core StatusDevice and returns
complete Status/3 with configuration/4, actual native selected execution and
precision/fallback facts. `arbiter status-legacy` retains Status/2 and the exact
configuration/3 CPU interpretation. The old Profile selected_profile never
implicitly grants GPU execution. New InitializeDevice and MigrateDevice are
explicit mutations; migration binds the exact old configuration digest and
revision under native ownership. Automatic upgrade uses that same authenticated
owner transaction and is not a read effect.

Configure files are complete Rust ManagementRequest values. ConfigureDevice
carries an explicit stage, expected_revision and complete DeviceStageConfiguration,
including independent device_policy and gpu_precision. CLI passes every native
field intact and refuses a foreign stage or unknown precision before admission.
Retained Configure remains its original CPU request; it is never converted to
a device request or given invented consent. Status/3 is accepted only for the
new device operations, and legacy operations retain Status/2. Full native errors
and original unknown-outcome/nonreplayable mutation semantics remain unchanged.

The shared Core Arbiter schema and Rust semantic fixtures own all device DTOs.
I2 session responses carry their complete native result, without an alternate
handwritten view or a new clock/token allowance. Device selection data is not
qualification, source authority, a reservation or proof of device settlement.

The complete current family also includes ProvisionDevice, CancelDevice,
RemoveDevice and RecoverDevice, preserving their original revision/profile/stage
payload fields. Current private ArbiterImportDevice retains the exact native
offline-directory input and returns ArbiterDevice Status/3; it is forbidden over
HTTP. Ordinary CLI/I3 operations use current device variants. Explicit CLI
--legacy (status-legacy for reads) and I3 *_legacy operations retain old CPU
requests only. After device configuration/4 is active, those old operations
require an updated client and cannot write or project a GPU result into CPU
Status/2. Versioned configuration migration creates and fsyncs an exact archived
configuration/3 before atomic replacement; no read/missing-state inspection
performs migration or model acquisition. Core/Store own the migration, actual
DeviceCatalog/source binding and assets/4 plus partial/4 custody.


## Answer's Invoke envelope (final plan 12.4, step L2, 22 November 2026)

An Invoke whose command is `tools call airk_answer` (or `answer`), after the
session's value options, may ask for up to `MAX_ANSWER_INVOKE_DEADLINE_MS`
(605,000 ms: Answer's 600,000-ms operation plus the same 5,000 ms for
delivery; [Core's read-attempt contract](../../../airk-core/src/read_attempt/CONTRACT.md)).
Every other Invoke and every AdminCall keeps `MAX_INVOKE_DEADLINE_MS`
(125,000 ms). The retained frame fixtures and `transport_budget.json` are
unchanged; `invoke_deadline_limit` is tested directly. The VS Code session
client applies the same rule to the argv it builds (`tools call <tool>`).

## Connection commands removed (23 November 2026, local trust plan B, change-log entry 68)

`agents info`, `credential`, `revoke`, `verify`, `launch` and `default`,
`agents list --connection-id`, and `tools list|help|call --connection` are
removed. A removed subcommand answers the typed `not_yet_served` refusal
(exit 5) as any unknown command does; a removed argument of a served command
is a usage error (exit 2). Neither reaches the service
(`cli::tests::open_agent_arguments_are_checked_before_the_service`).
`docs/cli-reference.md` and `airk-mcp`'s `help/cli_commands.json` are
regenerated from the parser.
