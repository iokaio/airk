# Make detached uninstall completion and recovery easy to verify

Extension removal hands native cleanup to a detached worker. Users need a reliable way to distinguish removal of the extension from successful shutdown and removal of the shared native runtime.

## Current behavior and evidence

Source: `c62c1e6936500801dbf41dffdd6e21453e48335d`, 1.7.2 candidate. [The hook](https://github.com/iokaio/airk/blob/c62c1e6936500801dbf41dffdd6e21453e48335d/packages/vscode/src/uninstallHook.ts#L67) writes `cleanup.log` and `result.json` to a temporary `airk-uninstall-*` directory and prints that location before returning. The worker records `complete` or `needs_attention` and deletes its copied executable/script on the normal completion path. This is useful existing reporting, not a completely silent cleanup flow.

[Native removal](https://github.com/iokaio/airk/blob/c62c1e6936500801dbf41dffdd6e21453e48335d/crates/airk-cli/src/runtime_retirement.rs#L666) can refuse unknown ownership or defer removal of held/changed files. An abrupt worker exit can also occur before a final result is written. The checked-in detached-worker test covers survival beyond the hook deadline; this review did not execute that test or run a real installed-editor uninstall on any platform.

The runtime is shared by CLI and agent clients, so successful removal can disconnect those clients as well as the editor. The intended scope of uninstall should be visible.

## Proposed acceptance criteria

- **AC1:** A documented, discoverable status/recovery path remains usable after extension removal and identifies the exact cleanup attempt and installation(s).
- **AC2:** Started, complete, needs-attention and interrupted/unknown outcomes are distinguishable; lack of a final receipt is never treated as success.
- **AC3:** Recovery instructions work when the extension and temporary worker executable have already been removed, without telling users to delete unverified directories or kill unrelated processes.
- **AC4:** Installed-editor tests cover success, a deliberately held file, an ownership refusal and worker interruption, verifying both the result and actual remaining processes/files.
- **AC5:** Documentation identifies shared-runtime effects and retention of temporary diagnostics. It does not promise all-platform cleanup based only on mocked or single-platform tests.

This is an observability and qualification proposal; no universal uninstall failure was demonstrated.
