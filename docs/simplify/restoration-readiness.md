# Interrupted-adoption and installed-fixture readiness

Owner: CLI restoration task, integrated by the primary release-readiness owner.
This is focused release stabilization; persisted formats and service behavior
remain unchanged. Source review is not installed qualification.

## Investigation and invariants

Source reviewed: `runtime_install/legacy_maintenance.rs` and its tests,
`lifecycle.rs`, `activation::start_selected`, runtime inventory/selection checks,
`quiescent`, `ServiceManager::require_stopped_for_maintenance`, state-contract
admission, installed layout/registry binding, and the two `service_runtime`
installed-loss fixtures. The checkpoint, install-layout and install-files
contracts govern the reused boundaries.

- Settlement runs under the installer lock. It clears only its own checkpoint
  fence, after proving manager shutdown and acquiring all native owner leases.
  Foreign fences survive. An unreadable journal with a fence survives.
- Restart applies only when the adoption observed a running service, did not
  record restoration or an original explicit user stop, and no foreign fence
  exists. Current lifecycle stop state is checked again before verifying the
  currently selected runtime. Journal-saved runtime/health never selects code.
- Quiescence leases are released before starting. The actual starter retains
  its native ownership, exact-image and baseline checks. A failed restart is
  reported as a stopped service; existing behavior removes the maintenance
  journal afterward. Repeated settlement therefore does not replay that start.
- Existing settlement fixtures record `cycle_health: null`; none exercises the
  restart observer. Missing coverage is a source-review finding, not a
  reproduced production defect.
- The two installed process fixtures manufacture pre-baseline layout files but
  no state-contract Genesis or selected image. Admission correctly refuses
  them before their intended log/control-loss observations. A marker alone is
  insufficient: service admission also requires the exact selected executable.
  Repair must use the existing private payload/stage/activation test machinery,
  preserve the loss/recovery assertions, and start that selected executable.
- Top-level `uninstall` lacks the public-command test mapping. Its standalone
  completion helper already has explicit success/incomplete/deferred receipt
  tests. CLI noninteractive refusal must happen before any installation change.

## Evidence and planned selections

The retained handoff records baseline failures for both installed fixtures and
the missing command mapping. The primary agent's new retained-artifact diagnostic
selected both installed fixtures: 2/2 failed with `clean_break_required`. Its
command-mapping diagnostic selected one test but stopped earlier at the moved
`runtime abort-layout` owner; that run does not reproduce the uninstall omission.
The artifact's compiled-source identity is not established, so neither run
qualifies current source. Receipts: `artifacts/readiness-1817/fixture-diagnostic.json`,
`service_runtime-diagnostic.log` and `cli_commands-diagnostic.log` in that directory.
No build or test was run by this delegated task.

Cheap coverage will exercise restoration eligibility, selected-image observer
success, refusal, explicit stop, foreign fence, held-owner exclusion, recovery
after releasing the owner, and repeated settlement without another start.
The observer replaces only process launch; real layout, journal, fence,
selection, inventory and lifecycle observations remain in the tested path.
Matched runtime/process restoration remains a separate qualification gate.

Golden expectations: existing report fields/reason strings and journal/fence
formats retain their bytes; installed namespace and loss receipts retain their
existing assertions. No public read, control or persisted contract changes.

Validation commands, selected identities and outcomes will be recorded here
after source review and the primary agent's serialized batch.

## Completed source and review

- A private observer replaces only the final process launch. Current lifecycle
  permission, selected identity, payload verification, journal/fence ownership
  and quiescence remain in the path. The production observer calls the same
  `activation::start_selected` with the same arguments and reports the same
  values and reason strings.
- Four new deterministic tests cover selected-image restoration success after
  releasing leases, eligibility refusals, deferred recovery after native owner
  release with a starter refusal, and a newly recorded explicit user stop.
  Existing no-restart fixtures are unchanged. Observer tests qualify decisions
  and reporting; they do not qualify actual service launch.
- Installed process fixtures now use the existing sealed-payload machinery,
  actual native staging/activation/Genesis and the exact selected executable.
  Original control/registry loss and namespace restoration assertions remain.
  The log test creates its safe mapping through canonical IndexSetup, then
  compares matched read-only setup drafts that differ only in source root to
  prove the private log source is refused. Each fixture preserves its directory
  on failure and enables removal only after all owned child exits.
- Current-source mapping inspection found the stale `runtime abort-layout`
  entry as well as the missing top-level `uninstall`. The former now names the
  existing abort regression; the latter names a new actual CLI noninteractive
  refusal regression. Existing native completion receipt tests remain intact.
- Explicit source review checked observer ordering, owned-path limits, retained
  formats/reports, selected-image admission, matched safe/private draft inputs,
  fixture failure retention and child exit assertions. Compilation remains the
  primary agent's next gate.

Selections (shipping configuration, serialized by the primary agent):

```console
cargo test -p airk-cli --no-default-features --features local-inference --lib runtime_install::legacy_maintenance::tests:: --locked
cargo test -p airk-cli --no-default-features --features local-inference --test cli_commands standalone_uninstall_ --locked
cargo test -p airk-cli --no-default-features --features local-inference --test cli_commands every_public_command_has_a_test_and_prints_help --locked
cargo test -p airk-cli --no-default-features --features local-inference --test service_runtime installed_ --locked
```

Expected selections: eight restoration library tests, one uninstall CLI test,
one public-command mapping test and two installed-loss process tests. Record
the actual selected identities and outcomes; zero selected tests is a failure.

## Focused batch and follow-up fixture investigation

The primary agent's first compiled focused library batch passed all eight
restoration regressions. Its complete receipt is
`artifacts/readiness-1817/focused-native-libs-r2.log`: 204 selected, 199 passed,
five failed. Four CLI failures were delegated back together; the fifth is owned
by the MCP security task. These were actual current-source reproductions.

The four CLI cases are two distinct classes:

- `registry_busy_retry_retains_full_mutation_and_instance_on_all_request_variants`
  and `standalone_launch_plan_reads_owned_token_into_child_environment_only`
  passed an already-created `tempfile::TempDir` as the native private root.
  `tempfile` 3.27.0 uses platform-default directory creation on Windows, while
  `local_ipc_windows::validate_existing_root` requires exactly one owner ACE.
  An isolated parent temporary base alone cannot prove that a newly created
  default-ACL directory satisfies that invariant. Each fixture now uses its
  temporary directory only for cleanup and lets `local_ipc::prepare_root`
  create a fresh private child. Existing protocol, mutation, instance, secret
  containment and inherited-certificate assertions remain unchanged. No user
  ACL or native security rule changes.
- The completed-activation and external-callback cases were stale baseline
  expectations. Public activation checks clean-break admission and executes
  candidate qualification before acknowledging an old reviewed receipt.
  Managed startup requires initialized state before invoking its callback.
  The inert, uninitialized fixtures could not satisfy those requirements;
  their failures were not host qualification failures.

The completed-receipt unit case keeps pure receipt acknowledgment and every
unchanged-state/startup-marker assertion, and now explicitly proves public
prebaseline refusal without changing any durable bytes. Its identity changes
from `completed_reviewed_activation_acknowledges_without_starting_or_reinitializing_state`
to `completed_reviewed_receipt_acknowledges_but_public_prebaseline_replay_refuses_without_changes`.
Successful public replay moves to the actual process boundary:
`initialized_completed_reviewed_activation_replay_preserves_stopped_service_and_all_state`.
That integration test creates real fresh Genesis, stages and activates a
separately approved inventory containing the same current image, records a
canonical exact-instance Stop, and retains native exclusion across replay.
It compares the entire fixture's file inventory and content identities before
and after, including state, registry namespace, selection, receipts, payloads,
logs and an invalid startup sentinel. It preserves failed fixtures and does
not restart solely to manufacture a cleanup receipt.

The external callback's positive case now creates real owned component
Genesis/state-contract state before calling the external health path, retains
the callback count of one, and checks the specific unavailable-supervisor
error. Its identity changes from
`external_legacy_callback_runs_once_before_health_without_election` to
`initialized_external_callback_runs_once_before_health_without_election`.
A new `uninitialized_external_policy_refuses_before_callback_or_election`
sibling preserves policy, layout and provider bytes, records zero callbacks,
and proves that no control namespace, public selection or bootstrap appears.
The component readiness producer substitutes executable qualification only;
these metadata cases do not qualify an actual managed provider process.

Source review covered the full admission/callback order, exact selection and
instance identities, native ACL creation versus validation, all four request
variants, secret cleanup, durable Stop ownership, replay snapshots and failure
retention. No production change was needed for these four fixtures.

Next serialized T0 selections: the renamed completed-receipt case, both
external callback/refusal cases, the existing full registry-retry module and
the standalone launch module. The new successful public replay is a separate
T2 process check after the parent reviews the combined source and cheap results.
All follow-up validation is pending; the delegated owner ran no build or test.

## First actual-process preflight: epoch observation gap

The primary agent's first T2 process-preflight batch selected 52 tests: 49 ran,
48 passed, the new initialized completed-replay case failed, and three tests
were not run after the stop condition. The new case reached successful actual
reviewed activation and then failed at `reviewed candidate request epoch missing`
after 21.788 seconds. This is the first process-preflight strike for the current
component. The failed fixture and its unproven service cleanup are retained.
Passed process/CLI/MCP/HTTPS evidence retains its original input identities;
there is no reason to rerun that complete batch for a test-observation change.

The failure message discarded the actual `ControlResponse`, so it does not
establish whether the refusal was checkpoint admission, registry contention,
catalog failure or another response. The missing cheap regression is an
observation case proving that an unexpected epoch response is reported in full
with the phase being tested. A known-no-admission retry contract cannot be
inferred from this message alone, and RequestEpoch prunes metadata rather than
being a generic evidence read.

Source review traced ordinary activation through checkpoint capture, recovery
startup, committed selection, catalog compatibility, `complete_selected`,
native admission release and removal of the activation intent. Successful
activation returns only after `ReleaseUpgradeAdmission` acknowledges. Health
bypasses the metadata actor; it does not establish that RequestEpoch succeeded.
Native request helpers already retry only `registry_busy` within their original
bounded contract. The shared evidence `read_retry` helper does not authorize
replaying RequestEpoch.

A separate source-review hypothesis remains to verify: the metadata actor can
enter its recovery-frozen branch and block on its channel while a native
admission-release command clears the fence independently. That already-entered
branch unconditionally rejects its next received request with
`runtime_upgrade_in_progress` before rechecking the fence. A subsequent actor
iteration re-observes the cleared fence. This could explain a single immediate
epoch refusal after release acknowledgment; it is not yet an observed cause of
this retained failure. Common reviewed fixtures and service/upgrade harnesses
also request epochs directly, so the boundary must be investigated as a shared
flow before changing any retry or readiness assumption.

No production or test correction has been made for this new failure. Retained
native observations and a complete serialized reply are required before deciding
whether it is a fixture observation issue or a production release-boundary defect.

Follow-up retained evidence now identifies the native refusal code:
`artifacts/readiness-1817/replay-retained-diagnosis.json` contains the candidate
instance's `control_failed` log with `runtime_upgrade_in_progress` at Unix
milliseconds 1794288101582. The same instance started at 1794288100712; the
selected receipt is 1794288101108 and committed checkpoint is 1794288101116.
The later read-only observation found no checkpoint fence or activation intent.
The candidate naturally idle-exited after 180 seconds; it was not restarted to
obtain these observations. Later `service_unavailable` health/epoch replies are
separate observations after that exit, not the original failure. The failed
fixture remains retained under its original owner and ACL.

Sibling audit: the VS Code `ServiceClient.key`, common reviewed `Rpc::key`,
service-client and runtime-upgrade harnesses issue RequestEpoch directly.
`ServiceClient.rpc` surfaces this failure rather than retrying it. Native
registry retries accept only `registry_busy`; evidence retry classifiers do
not include this code or RequestEpoch. `Projects` is a supported metadata read
implemented by SELECT-only `registry.list`; a same-instance Projects response
can establish the metadata observation required before the test's single
epoch mutation. This is a proposed fixture precondition, pending a deterministic
T0 demonstration of release during the actor's already-entered frozen receive.
The cached logs prove the refusal, but do not identify its exact emitter; the
actor ordering remains a source-backed explanation until that seam is exercised.

The coherent follow-up is now ready for the primary agent's review and batch:

- A private helper extracts the existing frozen receive without changing its
  timeout, disconnect handling, acknowledgment ordering, active/missing charges
  or exact refusal. Its production callback is a no-op. The new deterministic
  `frozen_metadata_receive_retains_unexecuted_refusal_after_recovery_fence_release`
  case enters that canonical receive, clears a real owned recovery fence after
  the frozen observation, and submits an epoch afterward through a real channel.
  It expects the unchanged exact refusal and proves no registry was created.
  The callback controls this interleaving without clocks, sleeps or processes.
  This isolates the actor; it does not pretend to perform native Coordinator
  release or change the frontdoor's independently cached admission state.
- The process fixture first observes a bounded one-item read-only Projects page
  from the actual health instance under the existing five-second control
  deadline. The deadline wraps the complete native observation future, including
  connection, send and response. Only the exact typed checkpoint refusal is
  pending; unknown transport outcomes, instance changes and every other reply
  stop immediately. Projects uses the canonical typed/deny-unknown-fields page
  schema, and the helper checks its one-item bound and continuation consistency.
  Health before and after that observation identifies the same actual candidate
  and executable. Only then does the fixture call the existing canonical epoch
  transport, with no added mutation retry.
- `metadata_readiness_observer_repeats_only_known_unexecuted_projects_refusal`
  and `metadata_readiness_deadline_and_epoch_failure_keep_complete_phase_observations`
  cover known pending versus terminal/unknown replies, unchanged observation
  budgets with a manual clock, and complete unexpected epoch JSON plus phase.
  All original public replay, durable Stop, exclusion and full state/content
  identity assertions remain unchanged.

This corrects the fixture's assumption that health plus admission release also
synchronizes the metadata actor. It adds no production retry or lifecycle
behavior and does not qualify an immediate first client metadata request without
the observed readiness precondition. The actor's existing transitional refusal
remains visible; no synchronized-release guarantee is added to current guidance.
The source and test changes are frozen for the primary agent's formatting,
Clippy and T0 batch. No tests or builds were run by the delegated task.
