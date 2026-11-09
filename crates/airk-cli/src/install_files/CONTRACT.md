# `airk_cli::install_files` contract

**Owner:** `airk-cli`. **Introduced:** Milestone S1.4 of the
[simplification plan](../../../../docs/simplify-plan.md), 9 November 2026.

## Purpose

`install_files` is the leaf module for two kinds of installed-file facts:

- the payload-relative **names** of installed executables and launchers;
- the **regular-file identity check** (`safe_file`) applied before AIRK reads,
  copies, hashes or executes an installed or owned file.

Before S1.4, these lived in `runtime_install`. Each module that needed them
imported that 2,600-line installer hub. `install_files` lets callers use them
without depending on installer state, locks or lifecycle code.

## Interface

| Item | Visibility | Contract |
|---|---|---|
| `safe_file(path) -> Result<()>` | `pub(crate)` | `Ok` only for an existing regular file that is not a symlink and has exactly one name. On Unix it also requires `nlink == 1`. On Windows it refuses the reparse-point attribute (`0x400`) and requires a link count of 1 from `GetFileInformationByHandle`. It never creates, opens for write, repairs or deletes anything. |
| `main_name() -> &'static str` | `pub` | `airk.exe` (Windows), `AIRK.app/Contents/MacOS/airk` (macOS), `airk` (other). |
| `MAC_LAUNCHER_IMAGE` | `pub(crate)` | `AIRK.app/Contents/MacOS/airk-launcher-image`, the separately signed Mac launcher image name. |
| `launcher_path(root) -> PathBuf` | `pub` | `root/airk-launcher.exe` (Windows) or `root/airk-launcher` (other). Pure path join with no filesystem access. |

`pub` items have real external callers: the `airk` binary and the
integration tests in `crates/airk-cli/tests/`. Everything else stays
`pub(crate)`.

## Owned state and invariants

- **No owned state.** There are no statics, no clock and no caches. Each call
  observes the filesystem at that moment and returns.
- **Names are persisted identity.** `main_name()` and `MAC_LAUNCHER_IMAGE` are
  keys in sealed runtime inventories (`airk-install.json` `files`). Changing a
  value changes packaged and persisted formats, so it needs plan decision D8.
- **Error strings are observable.** Callers and diagnostics surface them. They
  are kept byte-identical to the pre-S1.4 text.
- **`safe_file` is a check, not a guard against races.** Callers that act on a
  file after checking it must recheck at their own use boundary, as
  `runtime_install::launcher_image` does with its digest. This module does not
  hold a handle across the call.

## Dependencies (allowed edges)

- Allowed: `std`, `anyhow`, `windows_sys` (Windows file information only).
- **Forbidden:** any other `airk-cli` module and any `airk_*` crate. Adding one
  would put this leaf back into the CLI module cycle. Re-measure with
  `python scripts/dev/simplify_metrics.py` (the library module graph table).

## Related boundary: user path discovery

S1.4 also inverted `user_paths` → `runtime_install`.

- `user_paths::UserPaths::discover(channel, probe)` takes an
  `InstallationProbe`, which carries two functions:
  - `saved_control`: the claimed layout's control root;
  - `universal_selection`: the universal launcher's configured target.
- The installer supplies `runtime_install::INSTALLATION_PROBE` and the
  convenience `runtime_install::discover_user_paths(channel)`.
- `user_paths` itself has no production dependency on any other `airk-cli`
  module.

## Conformance applicability

These are not registered read, control or job operations, so properties P1–P9
of the plan's §4.6 do not apply. Reason: they are local filesystem checks with
no wire format, request identity or budget.

## Tests

T0, run from the repository root:

```console
cargo test -p airk-cli --no-default-features --lib install_files
```

The run must select at least three tests. The Unix-only symlink case adds one
more on Unix. Callers' own regression suites cover their composition with
these helpers. Examples: `runtime_install_tests`, `runtime_launcher_repair_tests`,
`runtime_legacy_launchers_tests` and the `user_paths` tests.

## Entry points

- Source: `mod.rs` (items) and `tests.rs` (unit tests), both beside this
  contract.
- The S1.4 call-site rewrite is reproducible with
  `python scripts/dev/s1_4_install_files.py --check`.
