# Original same-host canonical read allowance

Core owns ReadAttemptBudget schema1 and the optional ToolsCall field. This
contract changes no protocol number, persisted state, digest framing, public
MCP input, I2 Invoke frame or existing125-second read ceiling.

## Wire and compatibility

ToolsCall appends read_attempt: Option<ReadAttemptBudget>, defaulting to None
and omitted when None. All seven retained M3 literals in
fixtures/split/i1/control_methods.json must serialize byte-identically with
unchanged field order and frame bounds. Explicit JSON null follows the existing
optional-budget pattern and decodes as None; a carried Some parent is never
replaced by None to recover from refusal or transport failure.

A Some carrier uses the existing schema, original request_id, local monotonic
clock, absolute expires_monotonic_ns, remaining_ns ceiling and optional retry
expiry. Existing remaining_at validates schema/clock/125s bounds and returns
the smaller of absolute time left and the sender ceiling. Zero means exhausted;
adapters must refuse it before connect, backend work or read/model dispatch.
A retry expiry later than the outer expiry is invalid. Ordinary queries may
retain allowances above30s; only optional NativeNextRead has its existing30s
ceiling. Answer intersects its requested ceiling with its earliest parent.

Worker Query retains its existing positive peer feature/version gate. The new
M3 carrier uses a distinct compatibility choice: older ControlMethod and
ControlRequest readers have deny_unknown_fields and must reject Some before
execution. No service capability field currently exists in the closed Health
shape. Same version identity or the worker Query gate is not proof of this new
M3 support. A supplied parent is sent once; failure never causes a None fallback.
EOF/lost response remains nonretryable/unknown, not proof that nothing ran.
Clients and services carrying this extension require a matched rebuilt pair;
old same-version development images may refuse honestly.

The Core literal cases establish owner type/byte/strict-reader semantics only.
They do not qualify the service, OS endpoint, scorer or cancellation lifetime.
Consumer tests must establish validation before effects and one original
attempt through every Window/Open/Project/combined routing branch.

## Original lifetime and nonread boundaries

I2 retains its existing complete-frame monotonic receipt time, original request
ID and cancellation state across actor/worker queues and one permitted scope
reselection. Core's typed absolute expiry reaches the actual ToolsCall read
surface; internal CLI invocation does not mint another duration. Parent expiry
and cancellation prevent a new read dispatch. Existing foreground mutation
custody, uncertain outcomes and no-replay rules remain unchanged: generic Invoke
also supports nonread commands, and this carrier does not authorize mutation
retry or automatic cancellation of accepted effects.

For Some, transport connect/encoding/write/read consume one absolute original
deadline; existing shorter connect/write caps remain. Service validates before
scope/backend work and checks the same context again before dispatch and result
delivery. Canonical reads, Kernel, native callback, IPC and scorer may narrow
that deadline but never refresh it. Legacy None retains its existing125s
canonical/135s response behavior. Actual accepted-capacity/cleanup owners stay
held until their existing completion/settlement proof.

SDK fractional remainder is floored once before assigning sent state or writing.
A nonpositive floored remainder uses the existing not-sent deadline refusal,
never max(1,...) to grant extra time. This changes the compiled SDK scientific
input: require a new protocol freeze and retain prior failed development calls,
including the unknown ninth Kev call, under their original identities.

Canonical Answer admission intersects an existing original allowance with the
validated Answer request ceiling before selection, setup or member work. The
MCP owner parses its existing AnswerArgs and Core ContextRequest validator;
consumers do not mirror JSON fields. Sampling later may only narrow that same
context. Non-Answer reads retain their supported original ceiling. Invalid
arguments keep the canonical handler's existing error projection; early budget
admission does not invent a second malformed-argument envelope.
