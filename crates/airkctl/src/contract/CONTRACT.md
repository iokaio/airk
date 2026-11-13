# `airkctl` contract module (I2)

Owner of the I2 contract between `airkctl` and its callers: the VS Code
extension, scripts and users (split plan §4.2; draft in
[contracts](../../../../docs/split/contracts.md) §3).

## Interface

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
- **Session protocol 2** (`SessionRequest`/`SessionResponse`/`SessionEventFrame`):
  4-byte big-endian length then JSON (the shared codec). Replies keep the
  protocol 1 `{id, outcome}` shape; events are `{event:{kind,…}}`. Budgets are
  the `MAX_*` constants. Cursors are scoped to one service incarnation and one
  window. No frame carries a token, grant, lease or launch code.

## Invariants

- Contract values change only with a reviewed fixture change. Expectations in
  the fixtures are written independently of this code.
- Window-plane frames (`scope_select`, `scope_release`, `assistance_ack`,
  `handoff_publish`) follow `fixtures/split/i1/window_plane.json`; a service
  refusal of one is `refused {code, retryable}`, distinct from session-level
  `failed`. The window lease never appears in a session frame.
- The session `control` relay forwards exactly the operation-table rows
  (`fixtures/split/i3/admin_operations.json`) that list the `session`
  surface (contracts §3.8, U103); others are `session_command_refused`.
- `airkctl` depends only on `airk-core`, `airk-process` and `airk-ipc` (U27).

## Fixtures

`fixtures/wire/airkctl_session_frames.json`, `fixtures/split/i2/exit_codes.json`,
`fixtures/split/i2/error_lines.json`, `fixtures/split/i2/version.json`.
The compatibility matrix and offline-operation fixtures land with their first
consumer.

## T0

```console
cargo test -p airkctl --lib contract
```
