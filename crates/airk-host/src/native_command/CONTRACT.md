# `airk_host::native_command` contract

**Owner:** `airk-host` (forwarder); implementation owner: `airk-process`. **Introduced:** step S7.1 of the
[S6/S7 untangling plan](../../../../docs/simplify/s6-s7-plan.md), 9 November
2026.

## Purpose

`native_command` is the leaf module that runs one bounded native command:

- it starts one contained child (`native_process::ContainedChild`);
- it enforces a fixed deadline on the child, its input and its output;
- it caps input at `MAX_CONFIG` bytes and each output stream at 64 KiB;
- it locates Windows system tools by their native system directory.

Before S7.1, these items lived in `service_manager`. `http_endpoint` (user
trust) and the macOS login adapter (`runtime_install::macos_login`) imported
that OS lifecycle hub just to run one command, and `native_process` imported
it for argument quoting. That put `native_process` and `http_endpoint` in the
CLI module cycle. Callers now depend on this leaf instead.

## Interface

| Item | Visibility | Contract |
|---|---|---|
| `MAX_CONFIG` | `pub` | 128 KiB. Bounds command input here, and service configuration files in `service_manager`. One value, so any accepted definition can be passed as input. |
| `Invocation { program, args, env, input }` | `pub` | A command description. `Invocation::new(program, args)` sets an empty environment overlay and no input. `env` adds to the inherited environment; it never clears it. |
| `Output { success, text }` | `pub` | Exit success and the lossy UTF-8 text of stdout and stderr, concatenated in the order the two streams closed. |
| `execute(&Invocation) -> Result<Output>` | `pub` | `execute_with_timeout` with a 30-second deadline. |
| `execute_with_timeout(&Invocation, Duration) -> Result<Output>` | `pub`, tests or `test-support` | Fails, rather than truncating, when input exceeds `MAX_CONFIG`, an output stream exceeds 64 KiB, or the child, its input or its output outlives the deadline. Standard input is null unless `input` is set. Windows children get `CREATE_NO_WINDOW`. |
| `execute_native_program(program, args) -> Result<bool>` | `pub`, macOS, tests or `test-support` | Exit success only. |
| `execute_native_observation(program, args) -> Result<(bool, String)>` | `pub`, Windows, macOS and tests | Exit success and text. |
| `windows_tool(name) -> Result<PathBuf>` | `pub` | Windows: `GetSystemDirectoryW` joined with `name`, which must be an existing file. Other hosts: always an error. |

Windows argument quoting (`windows_argument`) is not part of this module. It
lives in `native_process`, which uses it for detached service command lines.

## Owned state and invariants

- **No owned state.** There are no statics, caches or locks. Each call owns
  its child and its helper threads until the call returns.
- **Deadline covers everything.** A slow child, a blocked stdin writer or an
  open output pipe all fail the call before the deadline plus scheduling slack.
  The child-deadline error tells the user to inspect OS service status before
  retrying.
- **Containment.** `native_process::ContainedChild` owns the child (a Job
  Object on Windows, an `airk_process::GroupChild` process group on Unix) and
  drains its descendants when the call returns, including on error.
- **Error strings are observable.** Callers surface them. They are kept
  byte-identical to the pre-S7.1 text.
- The clock is `std::time::Instant`; there is no injected clock yet. Do not add
  decision logic here without one.

## Dependencies (allowed edges)

- This host module only re-exports `airk_process::bounded_command`.
- The implementation uses `std`, `anyhow`, `windows_sys` and process
  containment in its owning crate.
- **Forbidden:** service or installer policy. In particular, never import
  `service_manager`: that recreates the cycle S7.1 removed. Re-measure with
  `python scripts/dev/simplify_metrics.py` (the library module graph table).

## Conformance applicability

These are not registered read, control or job operations, so properties P1–P9
of the plan's §4.6 do not apply. Reason: a local process adapter with no wire
format, request identity or read budget.

## Tests

T0, run from the repository root:

```console
cargo test -p airk-installer --no-default-features --lib service_manager::tests::manager_command --locked
```

The run must select at least two tests on every host
(`manager_command_fixture` and
`manager_command_deadline_covers_blocked_stdin_and_output_is_not_silently_truncated`).
Linux adds `manager_command_completion_drains_descendants_with_closed_pipes`.

The tests stay in `service_manager::tests`. Moving them would change their
identities, and several test modules re-run the fixture by its exact name,
`service_manager::tests::manager_command_fixture`.

## Entry points

- Source: `mod.rs`, beside this contract.
- Callers: `service_manager` (OS lifecycle), `http_endpoint::trust` (user
  trust store) and `runtime_install::macos_login` (signature and login job
  observation).

## Moved (split U90)

The items above now live in `airk_process::bounded_command` (with
`native_process::ContainedChild`, `spawn` and `attach_owned`), moved by
`scripts/dev/split_u90b_forwarders.py` so `airkctl` runs the same bounded
commands. This module re-exports them with the visibility and cfg gates
listed here; their behavior and bounds are unchanged.

Each Cargo selection must report nonzero selected tests. Record selected test
identities and outcomes; compatibility re-exports do not include dependency
unit tests. Platform-only cases require their native host.
