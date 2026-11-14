# Unix provider guardians

Owner: `airk-inference::unix_guardian`, compiled on Unix. Process foundations
own native groups; Store owns provider-drain records; Supervisor owns project
exit completion, drain recovery and resource admission. The shared-helper
guardian and retained per-worker provider guardian keep their distinct native
ownership proofs. Neither is replaced by a process-name/PID-scan cleanup rule.

The existing Supervisor is a different crate. Its live calls require these
public Rust entry points, with no `local-inference` feature restriction:

- `recover(registry, active)` reconciles exact native boot/process-scope and
  group-absence evidence against Store's provider-drain records.
- `rss(registry, pool_pid)` observes provider group members and guardian RSS,
  retaining unknown observations and existing census/deadline bounds.
- `wait_for_worker(runtime, identity)` waits for the exact worker's provider
  drains and project lease before Supervisor completes legacy process exit.

These entry points preserve the former same-crate callers after extraction;
`pub(super)` would hide them from their actual owning Supervisor. Their bodies,
native Linux/macOS branches, record shapes and frame/digest bytes are unchanged.
The native process-group census and Darwin `proc_listpgrppids` declaration stay
private and reachable through `rss`. No code is excluded, removed or silenced
to satisfy a crate-only build. An in-process owner continues to use its existing
exit path without a synthetic legacy-worker reap.

Applicable checks include Unix no-default-feature Clippy for both Inference
and Supervisor (not just the CLI dependency graph), the corresponding provisioned
local-inference build, and the existing `airk-cli` `unix_guardian` integration
target for real guardian drain behavior. Windows does not compile this module.
Visibility/source-body identity alone is not native Unix process qualification.
