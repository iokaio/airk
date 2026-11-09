# Open issue analysis

Analyzed 2 and 3 November 2026 against `feat/finetuning` at commit `b72fbc8`
(1.9.0 candidate). Nine issues are open, filed by two outside testers against
the published 1.8.7 build: #58 and #59 on Windows x64 (Marketplace package),
#61 to #67 on macOS 26.4.1 Apple Silicon (GitHub release package, some also
checked against the Marketplace package). Every native, extension, script and
workflow file cited below is byte-identical between the `v1.8.7` tag and this
branch except the version field in `packages/vscode/package.json` and the
regenerated `docs/cli-reference.md`, so the findings apply to the shipped
build and to the current candidate. Code positions are live line numbers
verified with `rg`/`sed`; the AIRK index consulted for discovery was a
1.8.7-era snapshot and was not treated as authoritative.

Evidence used: the issue bodies and comments, the health report attached to
#58 (captured 2026-11-02 20:32:00Z), the tester's `codesign`/`spctl` output in
#66 and #67, and the source tree. Times are UTC.

## Issues by severity

| Rank | Severity | Issue | Verdict |
| --- | --- | --- | --- |
| 1 | High | [#66](https://github.com/iokaio/airk/issues/66) GitHub release 1.8.7 VSIX never starts on macOS 26.4.1 (Apple Silicon): service killed with SIGKILL, `airk-launcher` is a bare copy of the signed binary | Confirmed from code. The installer copies the bundle's sealed main executable to a path outside `AIRK.app`; the Developer ID signature is bound to the bundle's `Info.plist`, so the copy fails signature validation and macOS refuses to run it. The extension cannot start from the release package. No user-side workaround passes the installer's integrity checks. |
| 2 | High | [#67](https://github.com/iokaio/airk/issues/67) Marketplace darwin-arm64 1.8.7 is a different build from the GitHub release asset (ad hoc signed, not notarized) | Confirmed and documented as a design choice in `docs/release-workflow.md`; the Marketplace workflow promotes the unsigned CI candidates and the GitHub release workflow signs the same candidates separately. Two binaries carry the same version, the published verification procedure cannot apply to the Marketplace package, and the ad hoc build is the only one that works on macOS because of #66. |
| 3 | High | [#58](https://github.com/iokaio/airk/issues/58) Default search fails with "read failed" while the inference helper is down (no restart or lexical fallback) | Confirmed. Four separate defects compound: a per-service-lifetime helper restart budget that only an explicit Start resets, no lexical degrade on the default read path, an untyped error that the Workbench collapses to "read failed", and a Management surface that never clears a stale helper recovery row. The reason the helper keeps exiting is not recorded anywhere and remains unknown. |
| 4 | Medium-high | [#64](https://github.com/iokaio/airk/issues/64) New or edited files are not searchable until the running vectors job finishes | Confirmed. The supervisor re-queues the next 32-artifact vectors turn before it checks for due source work, and both admissions require an idle project, so source reconciliation is starved for the whole semantic backlog. The in-kernel preemption check is bypassed in service mode. Lexical freshness is proportional to embedding time. |
| 5 | Medium | [#63](https://github.com/iokaio/airk/issues/63) `airk mcp` exits silently with code 0 on a JSON-RPC message larger than about 65 KB | Confirmed. The stdio reader enforces a 65,536-byte line bound by returning a read error; rmcp treats that as a closed transport, the frontend returns success, nothing reaches stderr. The bound is not documented for stdio. |
| 6 | Medium | [#59](https://github.com/iokaio/airk/issues/59) "From an older index" banner appears on default search results that are current | Confirmed. The Workbench compares each turn's publication with a readiness snapshot taken once when the session was selected and never refreshed after a run. After a new publication, current results are labeled old. The `model_failed` correlation is coincidental. |
| 7 | Low-medium | [#65](https://github.com/iokaio/airk/issues/65) Untrusted workspace: commands fail with the raw code `runtime_client_no_longer_authorized` instead of asking for trust | Confirmed. The refusal is correct; the code is shared with two unrelated conditions, the command wrapper shows it verbatim and rethrows into a modal, and no command is hidden by a trust `when` clause. The right sentence already exists in the Explorer views. |
| 8 | Low | [#61](https://github.com/iokaio/airk/issues/61) `service register` fails when `--data-dir` does not exist yet | Confirmed. `register` canonicalizes the data directory while `prepare-registration` accepts a planned path, and the headless quick start omits the `service prepare-storage` step the extension uses to create it. |
| 9 | Low | [#62](https://github.com/iokaio/airk/issues/62) `airk files` help says "under --prefix" but the prefix is positional | Confirmed. One doc comment and three generated/maintained documentation lines. |

Each issue below has the reported behavior, the root cause with file positions,
ordered recommendations, and regression coverage to add. Cross-cutting notes and
the documentation to update with the fixes follow at the end.

---

## Issue #66: release VSIX never starts on macOS 26.4.1 (launcher copy is killed)

### Reported behavior

With the GitHub release `airk-1.8.7-darwin-arm64.vsix` (Developer ID signed,
notarized) in a fresh VS Code profile, **AIRK: Set Up** never reaches a
connected state; a toast says "airk failed: exit code 1" and no service
process remains. Outside the editor, `airk --version` works inside
`AIRK.app/Contents/MacOS`, but a byte-identical copy of the same file exits
with 137 (SIGKILL) and `codesign --verify --strict` reports
"invalid Info.plist (plist or signature have been modified)". The installer
creates exactly such a copy at
`~/Library/Application Support/AIRK/runtime/airk-launcher` (same SHA-256 as
the bundle executable). `airk runtime activate` reports
`approved service exited before health: signal: 9 (SIGKILL)`. With the
service started from inside the bundle, every path that goes through the
launcher (collection status, MCP connection check) still fails with SIGKILL
or `write EPIPE`. Replacing the launcher with a symlink into the bundle makes
the connection check pass, but the installer rejects symlinks
("runtime file is not a regular unlinked file") and the extension then reports
`selected_runtime_payload_unverified`. The Marketplace package (ad hoc signed,
see #67) works on the same machine.

### Root cause analysis

**The stable launcher is a copy of the sealed bundle executable.**
[`prepare_launcher`](../../crates/airk-cli/src/runtime_install.rs#L1089-L1135)
copies the approved payload's main executable (`AIRK.app/Contents/MacOS/airk`)
byte for byte to [`launcher_path`](../../crates/airk-cli/src/runtime_install.rs#L775-L780)
(`<install>/airk-launcher`), sets mode 0700 and records its digest in
`launcher.json`. The launcher then runs as `airk runtime launch --install-dir …`
([`RuntimeCommand::Launch`](../../crates/airk-cli/src/runtime_install.rs#L309-L314))
and forwards to the selected payload. The extension, agent configurations and
the universal launcher all enter through this file (release 1.5.0 record,
[docs/releases/release-1.5.0.md](../releases/release-1.5.0.md) lines 43 and 69).

**Release signing binds that executable to the bundle.**
[`sign-native.py`](../../scripts/sign-native.py#L254-L260) first signs each
executable standalone with Developer ID and hardened runtime
([line 338](../../scripts/sign-native.py#L338)), then seals the enclosing
`AIRK.app` with another `codesign --force --sign … --options runtime`. Sealing
a bundle re-signs its `CFBundleExecutable` with the bundle's `Info.plist`
digest in the code directory (the plist is emitted by
[`build-service-runtime.py`](../../scripts/build-service-runtime.py#L176-L190)).
A copy of that executable outside the bundle has no `Info.plist` next to it,
so its signature no longer validates, which is exactly the tester's
`codesign --verify` output. On Apple Silicon the kernel refuses to execute code
whose hardened-runtime signature fails validation, hence SIGKILL at launch
with no stderr. The development/CI build signs ad hoc with runtime options
cleared ([lines 205-217](../../scripts/build-service-runtime.py#L205-L217)),
which is why the Marketplace copy survives being moved. The exact point at
which macOS 26.4.1 enforces this (versus earlier versions) is not established
by the repository; what is established is that the signed-copy tests in
[docs/validation.md](../history/validation.md) line 78 ran without actual OS signing,
and [docs/release-verification.md](../release-verification.md) line 52 records
macOS packages and installed-editor behavior as "not verified".

**Every integrity check then correctly refuses the workaround.**
[`safe_file`](../../crates/airk-cli/src/runtime_install.rs#L631-L641) rejects
symlinks and hard links (`nlink == 1`), the extension's CLI installer applies
the same rule ([cliInstall.ts:252](../../packages/vscode/src/cliInstall.ts#L252)),
and the bootstrap refuses an unverified payload
([serviceBootstrap.ts:308](../../packages/vscode/src/serviceBootstrap.ts#L308)).
These are deliberate anti-aliasing guards, so the fix belongs in how the
launcher is produced, not in the guards.

**The failure is reported as an ordinary exit.** The setup toast is built from
`error.message` at [serviceExtension.ts:811](../../packages/vscode/src/serviceExtension.ts#L811)
and disappears; the status bar stays on "AIRK: setup". `bootstrap.json`
records `exited_before_health` with `exit_code: null`, and the signal is only
visible to someone who runs `runtime activate` by hand.

Side observations in the report (Workbench "Open a trusted workspace" in a
trusted window, Explorer "No registered AIRK project is selected", "27 tools"
versus "28 tools") are consistent with the launcher-based MCP client failing,
but were not proven; re-check them after the launcher fix.

### Recommendations

1. **Stop shipping a bare copy of the sealed executable as the launcher.**
   Preferred: embed the bundle's `Info.plist` in the executable at link time
   (`-sectcreate __TEXT __info_plist`, matching the plist written by
   `build-service-runtime.py`) so the standalone signature carries the plist
   and a copy validates anywhere. Then sign the executable once as standalone
   hardened-runtime code and seal the bundle without re-signing the main
   executable's plist slot (or verify the copied launcher after sealing).
   Alternative with less packaging change: build a separate small
   `airk-launcher` executable, sign it as nested standalone code inside
   `Contents/MacOS`, and have `prepare_launcher` copy that instead of `airk`.
   A verified symlink into the approved payload is the quickest unblock but
   requires relaxing `safe_file`, `verified_launcher`, the extension's
   `nlink` check and the launcher receipt on macOS only; treat it as a
   fallback, not the fix.
2. **Add the failing path to the signing gates.** In
   [github-release.yml](../../.github/workflows/github-release.yml) and
   [macos-signing.yml](../../.github/workflows/macos-signing.yml), after
   notarization, copy `AIRK.app/Contents/MacOS/airk` to a temporary directory
   on the macOS runner, run `codesign --verify --strict` and `--version` on
   the copy, and run `airk runtime activate` against a staged install so the
   actual `airk-launcher` is exercised. Record the result in the release
   provenance; fail the release when the copy is killed.
3. **Report the real reason.** Map `exited_before_health` with a recorded
   signal to a persistent notification and a Management line: "macOS
   terminated the AIRK launcher (signal 9); the installed launcher failed
   signature validation. Reinstall from a corrected package." Keep the exit
   signal in `bootstrap.json` (`signal` field) and in the health report.
4. **Record macOS installed-editor qualification for the signed package**
   before the next release; 1.8.7 shipped with only Windows evidence
   ([release-1.8.7.md](../releases/release-1.8.7.md) line 87).

### Regression coverage to add

- A Rust test in `runtime_install` that installs from a payload whose main
  executable differs from its launcher source (simulating a nested launcher)
  and asserts the launcher receipt and `verified_launcher` still agree.
- A macOS-only CI step (hosted Apple Silicon runner) that executes the staged
  `airk-launcher` with `--version` after Developer ID signing; without the
  signing environment it should be skipped and reported as skipped, not
  passed.
- Extension test for the new `exited_before_health` + signal rendering.

---

## Issue #67: Marketplace darwin-arm64 package is a different build from the release asset

### Reported behavior

The Marketplace package for `iokaio.airk` 1.8.7 (darwin-arm64) and the GitHub
release asset for the same version contain different `airk` and `airk-embed`
binaries and a different `install_manifest_sha256`. The release bundle is
Developer ID signed with hardened runtime and notarized (`spctl` accepted);
the Marketplace bundle is ad hoc signed with no team (`spctl` rejected). Both
report `airk 1.8.7`. The verification procedure in
[docs/release-verification.md](../release-verification.md) cannot succeed for
a Marketplace install.

### Root cause analysis

This is how the two workflows are built today, and the repository says so:

- [extension-release.yml](../../.github/workflows/extension-release.yml#L162-L200)
  downloads the tested native package from ordinary CI
  (`ci-candidates.py download`), repackages it with `package-extension.py`
  and publishes the result to the Marketplace
  ([lines 201-256](../../.github/workflows/extension-release.yml#L201-L256)).
  Ordinary CI signs macOS candidates ad hoc with runtime options cleared
  ([build-service-runtime.py:205-217](../../scripts/build-service-runtime.py#L205-L217))
  and does not sign Windows binaries at all.
- [github-release.yml](../../.github/workflows/github-release.yml#L131-L210)
  downloads the same pinned candidates, signs them in the `native-signing`
  and `macos-signing` environments (Azure artifact signing for Windows,
  Developer ID plus notarization for macOS) and publishes the signed VSIX
  files as release assets.
- [docs/release-workflow.md](../release-workflow.md) line 49: "The Marketplace
  workflow … promotes unsigned native candidates into the requested VSIX
  channel." [docs/native-signing.md](../native-signing.md) line 46: "Ordinary
  PR CI and Extension release continue to create unsigned candidates."
  [docs/release-verification.md](../release-verification.md) line 69: "The
  Marketplace copy was not compared in this record."

So the difference is intentional in the pipeline but not visible to users:
[README.md](../../README.md) line 152 describes a "signed app bundle" without
qualifying the channel, the extension README makes no statement, and the
Marketplace listing is where most users install from. Consequences:

- Two different binaries share one version, so bug reports and provenance
  checks are ambiguous (the tester had to hash both to find out).
- Marketplace macOS users run an ad hoc-signed bundle that Gatekeeper rejects;
  it works only because the extension launches it without quarantine. Windows
  Marketplace users run unsigned executables.
- The `install_manifest_sha256` differs, so switching channels on one machine
  is treated as a runtime change and reactivation.
- Because of #66, the unsigned Marketplace build is currently the only one
  that works on macOS 26.4.1. That inversion is the strongest argument for
  publishing one artifact set.

### Recommendations

1. **Publish the same signed artifact set to both destinations.** Either have
   the Marketplace publish job consume the signed VSIX files produced by the
   GitHub release workflow for the same commit (restore by run ID, the same
   way `rehearsal_run_id` restores rehearsed files, and verify with
   `verify-extension-release.py`), or run the two signing jobs inside
   `extension-release.yml` behind the same environments. Require a per-platform
   SHA-256 match between the two destinations before either is marked
   published, and record both in `release.json` and `SHA256SUMS`.
2. **Extend verification to the Marketplace.** Add a Marketplace download and
   hash comparison to [verify-published-release.py](../../scripts/verify-published-release.py)
   and to the procedure in docs/release-verification.md; the current record
   explicitly skips it.
3. **Until that lands, state the difference where users look.** README,
   extension README and docs/release-verification.md should say that the
   Marketplace package is an unsigned CI build, how to verify it (publish its
   hashes), and that the GitHub release is the signed, notarized package.
   Do not describe the Marketplace package as signed.
4. **Decide the channel policy explicitly** in docs/release-workflow.md and
   the release checklist: one build per version, or distinct version suffixes
   per channel. Mixed builds under one version should not continue.

### Regression coverage to add

- Workflow-level: a release gate that fails when the Marketplace and GitHub
  VSIX hashes for a platform differ (scripts/tests next to the existing
  `test_ci_*_signing.py`).
- Documentation check: the repository check should flag "signed" claims in
  README/extension README unless both channels are signed.

---

## Issue #58: default search fails while the helper is down

### Reported behavior

Default `/search` (Text + semantic) failed immediately with
`Incomplete: query: read failed` and zero records twice in about 80 minutes;
the real cause, `shared embedding helper temporarily unavailable`, was only
visible in "Complete response". Management > Service showed the service live
while the Inference helper panel showed "Inference process available: No".
`--mode lexical` worked. The helper did not restart on its own during outages
of about 22 and at least 10 minutes; clicking **Start inference helper** fixed
both. Seven `model_failed` records carried no reason. The 20:32 health report
still showed "helper_unavailable, retrying, attempt 2 / 3, next check 20:04:14"
while the helper was available.

### Timeline reconstructed from the health report

| Time | Observation | Source |
| --- | --- | --- |
| 18:23:48 | `service_started`, instance `4022850d…`, same instance through 20:32 (uptime 7,690 s) | process log |
| 18:29:32, 18:41:48, 18:47:49, 19:14:28 | `model_failed` (no code) | process log |
| 19:25:26–27 | vectors job `426ddab6…` running at attempt 1, then returned to pending (deferred) | events 130–131 |
| 19:25:28 | `job_failed` code `helper_unavailable` for that job | process log |
| 19:25:28–19:47:25 | no job events; helper panel "No" at 19:46 | issue text, events |
| 19:47:25–28 | same vectors job runs at attempt 2 and succeeds, reconcile and verify publish revision 13 | events 132–139 |
| 19:53:37, 20:04:09 | `model_failed` | process log |
| 20:32:00 | report captured: `inference.available = true`, `loads = 1`; recovery row `helper_unavailable`, `retrying`, attempts 2/3, `started 19:53:37`, `next_check 20:04:14` | report |
| 20:33:45 | `model_failed` (from the issue; after the capture) | issue text |
| 20:44:07 → 20:53:11 | helper "No" until Start; "Yes" with a new worker ID | issue text |

### Root cause analysis

**1. Automatic helper restart stops after three failures and only an explicit
Start resets the budget.** The supervisor's in-memory
[`PoolRecovery`](../../crates/airk-cli/src/project_supervisor.rs#L381-L408)
admits a restart only while `attempts < 3` and not suppressed. Every helper
failure site (`Pool::exited` at
[project_supervisor.rs:937](../../crates/airk-cli/src/project_supervisor.rs#L937),
start failure at
[line 988](../../crates/airk-cli/src/project_supervisor.rs#L988) and control
renewal failure at
[line 1112](../../crates/airk-cli/src/project_supervisor.rs#L1112)) calls
[`ServiceRegistry::recovery_failed("helper", …)`](../../crates/airk-store/src/service/registry/recovery.rs#L532-L557),
which persists `failures = min(failures + 1, 3)` and a bounded delay of 1 to 8
seconds. While no pool exists, the loop reloads that row every iteration
([line 770](../../crates/airk-cli/src/project_supervisor.rs#L770)) so the cap
survives in-memory state. The only code that deletes the helper row is the
`StartEmbedding` control message
([line 1317](../../crates/airk-cli/src/project_supervisor.rs#L1317)); the
Management **Retry semantic indexing** action reaches the same path through
`start_embedding`
([serviceConsoleController.ts:736-739](../../packages/vscode/src/serviceConsoleController.ts#L736-L739)).
There is no reset on a successful start and no time-based decay.
[docs/service-embedding.md](../service-embedding.md) line 59 documents this as
"three helper start attempts per service lifetime".

The report fits this model. The recovery row's `started_unix_ms` is set only on
insert, and it reads 19:53:37, so the row was deleted (explicit Start) between
19:14:28 and 19:53:37, consistent with the tester's first Start around 19:47.
After that reset the failures at 19:53:37, 20:04:09 and 20:33:45 exhaust the
budget again and produce the second outage. The first outage (19:14 to 19:47)
requires the budget to have been exhausted by 19:14; the four earlier failures
achieve that only if a reset also occurred between 18:47:49 and 19:14:28,
which the report cannot show (process-log history is truncated to eight
records). Ask the tester whether Start or Retry was used in that window.

**2. The helper keeps exiting, and the service records no reason.** All three
failure sites call `log.record(LogKind::ModelFailed, None, None)` with no code,
although [`Log::record_code`](../../crates/airk-cli/src/service_logs.rs#L423)
exists and `LogRecord.code` is already part of the schema
([service_logs.rs:43](../../crates/airk-core/src/service_logs.rs#L43)). No
`model_started` record exists, so successful automatic restarts are invisible
and the log reads as seven unexplained failures. Candidate causes, in the order
worth testing:

- *Renewal misclassified as helper failure.* Every 30 seconds the supervisor
  renews each worker's pool grant
  ([lines 1082-1112](../../crates/airk-cli/src/project_supervisor.rs#L1082-L1112)).
  [`Pool::renew`](../../crates/airk-cli/src/embedding_pool_process.rs#L210-L224)
  treats any reply other than `Renewed` within 3 seconds as failure, and the
  supervisor then drops a possibly healthy helper. The helper answers
  `Failed` when it has no live, unexpired grant for that exact worker
  ([embed_pool.rs:647-650](https://github.com/iokaio/airk/blob/v1.8.7/crates/airk-cli/src/embed_pool.rs#L647-L650)). Worker
  replacement (the tester saw worker IDs change on their own) and the helper's
  one-second grant sweep are the obvious ways to reach that state.
- *Fatal error in the helper's main loop.* A listener accept error other than
  `PeerRejected`, or a stdout frame write failure, returns from the loop and
  ends the process
  ([embed_pool.rs:673](https://github.com/iokaio/airk/blob/v1.8.7/crates/airk-cli/src/embed_pool.rs#L673),
  [line 670](https://github.com/iokaio/airk/blob/v1.8.7/crates/airk-cli/src/embed_pool.rs#L670)).
- *External termination* (antivirus, job-object cleanup when a worker exits).
  Only an exit status in the log can distinguish this from the above.

**3. The default read path has no live-helper degrade.** Mode resolution in
[`search_current_read_with_filters`](../../crates/airk-kernel/src/current/search.rs#L200-L240)
turns `Default` into `Hybrid` whenever saved semantic readiness is `Ready`. It
consults the saved `DefaultSearchPolicy` (`PartialHybrid`,
`LexicalWhilePending`, `RequireComplete`) only for pending or disabled
coverage. The vector branch is then mandatory:
`branches.push(vector_candidates(…)?)` at
[line 270](../../crates/airk-kernel/src/current/search.rs#L270), and
[`vector_candidates`](../../crates/airk-kernel/src/current/search.rs#L512-L528)
embeds the query through the worker's remote model session, which returns
`HelperUnavailable` from
[`Hub::client`](../../crates/airk-cli/src/embedding_pool_client.rs#L350-L428)
while no pool access exists. The lexical branch has already been computed at
that point and is discarded. An explicit `--mode lexical` succeeds because the
vector branch is skipped.

**4. The failure is untyped and the Workbench hides it.** MCP
[`error_result`](../../crates/airk-mcp/src/response.rs#L15-L34) only emits a
structured hint for the typed contention errors recognized by
[`read_retry_hint`](../../crates/airk-kernel/src/retrieval_retry.rs#L18-L38);
everything else becomes the bare `error.to_string()`. The Workbench step
executor reduces any step error to `"{step}: read failed"` in `completeness`
([workbench.rs:2420](../../crates/airk-kernel/src/workbench.rs#L2420)) and
keeps the message only in `report.error`, which the webview shows under
"Complete response". The user-facing summary therefore never names the helper
or the remedy. `HelperUnavailable` is defined in `airk-cli`, so the kernel and
MCP crates cannot downcast it today; classification has to happen at the
worker read boundary or the marker type has to move to `airk-core`.

**5. Management keeps a stale "retrying" row and the Service summary does not
mention the helper.**
[`recovery_observations`](../../crates/airk-store/src/service/registry/recovery.rs#L667-L750)
always emits the persisted `helper` budget row with state `retrying` while
`failures < 3`, regardless of whether a pool is currently running. Nothing
writes that row on a successful start, so after the 20:04 restart the report
still showed "attempt 2 / 3, next check 20:04:14" 28 minutes later. The
Explorer badge
([explorerStatus.ts:113-114](../../packages/vscode/src/explorerStatus.ts#L113-L114))
reacts only to index availability, freshness and `semantic === 'failed'`; the
`airk_status` payload has no helper-availability field; and the project health
view ([explorerManagement.ts:123-145](../../packages/vscode/src/explorerManagement.ts#L123-L145))
raises the helper only through a `needs_attention`/`stopped` recovery row or a
job-scoped wait. With the row stuck at `retrying`, the summary stays
informational while semantic search is actually unavailable.

**Working as designed, but worth clarifying:** the vectors job that "failed"
at 19:25:28 was in fact deferred and retained pending by
[`defer_job_failure`](../../crates/airk-store/src/service/registry/recovery.rs#L571-L642);
it resumed on the same receipt at attempt 2 once a pool existed. The process
log still records it as `job_failed`, which is what the tester read. The
22-minute wait is entirely explained by item 1: `check_recovery_conditions`
([project_supervisor.rs:533-566](../../crates/airk-cli/src/project_supervisor.rs#L533-L566))
polls every second and resumes within about 30 seconds of a pool appearing.

### Recommendations

Ordered by user impact. Items 1 to 3 remove the outage; 4 and 5 make the
remaining cases legible.

1. **Make default search degrade to text-only when the helper is unavailable
   and say so.** In `search_current_read_with_filters`, for `Default` mode
   only, treat a helper-unavailable failure of the vector branch as an
   incomplete lexical result instead of an error: keep the lexical branch,
   set `incomplete = true`, and add a bounded disclosure to the response (for
   example `semantic: { state: "ready", helper: "unavailable", served:
   "lexical", reason: "helper_unavailable" }`). Explicit `mode=vector|hybrid`
   must still fail with the typed error from item 2. Requires a kernel-visible
   error marker: move `HelperUnavailable` to `airk-core` (or add an
   `airk_core::inference::HelperUnavailable` marker that the `airk-cli`
   `RemoteSession` adapter attaches as context), respecting the dependency
   direction in [docs/architecture.md](../architecture.md). Parity: the same
   disclosure must appear in CLI `search`, MCP `search`/`context`, Workbench
   turns (render "Semantic search unavailable; text results only" with a
   Start-helper link), Workbench Markdown/JSON copies and exports, and
   `response_mode=full`. Add the field to the versioned MCP catalog and to
   [docs/mcp.md](../mcp.md) / [docs/service-retrieval.md](../service-retrieval.md).

2. **Type the helper failure on every transport.** Extend `read_retry_hint` (or
   a sibling classifier) so helper unavailability yields
   `{ status: "retry", retryable: true, code: "helper_unavailable",
   retry_after_ms: 2000, action: "repeat_same_request", alternative:
   "mode=lexical", remedy: "start_embedding" }`. Carry the code into the
   Workbench `completeness` entry (`query: read failed (helper_unavailable)`)
   and render `report.error` inline on the failed step rather than only in
   "Complete response". CLI JSON error envelopes
   ([docs/operations.md](../operations.md) line 96) need the same code.

3. **Let the helper restart budget recover on its own.** Two changes in the
   supervisor and store: (a) on a successful `Pool::start` whose `Ready`
   observation arrives, record `model_started` and call a new
   `recovery_recovered("helper")` that marks the row `recovered` (retain
   `started_ms` for history; clear `failures`), so a later isolated failure
   starts a fresh three-attempt budget; (b) add time-based decay so a budget
   exhausted by three failures becomes eligible again after a bounded cool-off
   (for example 10 minutes, then 30, capped) instead of waiting for a human,
   unless `suppressed` (explicit Stop) is set. Keep the existing per-explicit-Start
   reset. Update [docs/service-embedding.md](../service-embedding.md) line 59
   and the recovery wording in [docs/index-self-healing.md](../index-self-healing.md)
   if it restates the lifetime cap.

4. **Record why the helper died.** Use `record_code` at the three failure sites
   with distinct codes (`helper_exited:<exit status or signal>`,
   `helper_start_failed:<cause class>`, `helper_renew_failed`) and add
   `model_started` / `model_recovered` kinds. The Management activity view and
   the health report already print `code`, so no renderer change is needed
   beyond the label table in
   [serviceConsoleView.ts](../../packages/vscode/src/serviceConsoleView.ts).
   While there, make the helper's listener accept path non-fatal for transient
   errors (log and continue; exit only on repeated failure), and change the
   renewal path so a `Failed` reply for a worker that no longer holds a grant
   re-registers that worker instead of dropping the whole pool. Re-test the
   worker-replacement scenario (idle worker exit followed by a new worker)
   against a live helper on Windows; that is the most likely trigger for the
   observed cadence and cannot be confirmed from the report.

5. **Surface helper availability where the tester looked.** Add
   `inference: { available: bool, worker: <incarnation|null> }` to the
   `airk_status` payload from the worker's in-process `Hub` access state (no
   process probe, so it stays within the status tool's budget); feed it into
   `semanticExplanation` and the badge
   ([explorerStatus.ts](../../packages/vscode/src/explorerStatus.ts)) as
   `warn`/"semantic unavailable"; add a Service-summary line "Inference helper
   unavailable: default search is text-only" with the existing
   `startEmbedding` action whenever `model.available === false` and the
   project wants semantic coverage, independent of recovery rows; and relabel
   the deferred-job log so a retained job shows as `job_deferred` rather than
   `job_failed`.

### Regression coverage to add

- Kernel: a `search_current_read_with_filters` test with a `RemoteSession` that
  fails with the new marker: `Default` returns lexical evidence, `incomplete`
  and the disclosure; `Hybrid`/`Vector` return the typed error.
- MCP/CLI/Workbench parity test comparing the full `search` result and the
  error envelope for the helper-unavailable case across
  [crates/airk-cli/tests/mcp.rs](../../crates/airk-cli/tests/mcp.rs), the CLI
  JSON path and `airk_parse` execute (per the parity rule in CLAUDE.md).
- Supervisor unit tests next to the existing
  [`helper_recovery_is_bounded_and_manual_stop_remains_stopped` test](../../crates/airk-cli/src/project_supervisor.rs#L2018-L2047):
  successful start resets the budget; exhausted budget becomes due after the
  cool-off; explicit Stop still suppresses.
- Store test for `recovery_observations` showing `recovered` after
  `recovery_recovered("helper")`.
- Extend [scripts/check-service-pool.py](../../scripts/check-service-pool.py):
  terminate the helper process externally three times, assert default search
  still returns lexical records with the disclosure, assert the helper restarts
  after the cool-off without Start, and assert the typed error on explicit
  hybrid mode.
- Extension tests in
  [explorerManagement.test.ts](../../packages/vscode/src/test/explorerManagement.test.ts)
  for the new summary line and badge state when `model.available === false`.

---

## Issue #64: new or edited files are not searchable until the vectors job finishes

### Reported behavior

Headless service, source watching on, default vector settings. With a 1 MB
text file being embedded (`active_job.kind == vectors`), a tiny new file with
a unique word was invisible to `search --mode lexical` for about 2 minutes 15
seconds, until the vectors job ended; `watch.requested_revision` stayed ahead
of `completed_revision` (14 against 11) the whole time. With 6 MB of text the
wait was about 7 minutes and covered a new file, an edit, a delete and a
rename. Health stayed ok and no error was reported.

### Root cause analysis

**Semantic work runs as a chain of short jobs, and the chain is re-queued
before source work gets a turn.** One vectors job is one 32-artifact batch
with a 30-second soft deadline
([`service_enrichment_turn`](../../crates/airk-kernel/src/service_enrichment.rs#L51-L66),
[`resume_current_enrichment_batches_bounded`](../../crates/airk-kernel/src/current/enrichment.rs#L813-L822)).
After each turn the worker sets `semantic_pending` while readiness is
`Pending` or `Partial`
([project_worker.rs:1150-1160](../../crates/airk-cli/src/project_worker.rs#L1150-L1160)).
The supervisor loop (250 ms cadence) then calls
[`due_background_jobs`](../../crates/airk-store/src/service/registry/background.rs#L121-L140)
and enqueues the next vectors continuation
([project_supervisor.rs:1371-1382](../../crates/airk-cli/src/project_supervisor.rs#L1371-L1382)),
and only afterwards calls
[`due_source_projects`](../../crates/airk-store/src/service/registry/watch.rs#L150-L166)
to enqueue source reconciliation
([lines 1388-1397](../../crates/airk-cli/src/project_supervisor.rs#L1388-L1397)).
Both queries admit a project only when it has no pending or running job
(the `NOT EXISTS(… service_jobs … state IN ('pending','running') …)` clause,
with an exception only for automatic audits). Because the continuation is
evaluated first in the same iteration, the idle gap between two vectors turns
is always taken by the next vectors turn, and the source reconcile is admitted
only when `semantic_pending` clears. The tester's wait therefore scales with
the embedding backlog, which matches both measurements.

**The kernel's own preemption check is bypassed in service mode.**
[`drain_current_enrichment`](../../crates/airk-kernel/src/current/enrichment.rs#L913-L946)
breaks out of the batch loop when the scheduler queue reports
`requested_revision > completed_revision`, but that probe is skipped when
`self.service_collection.is_some()` ([lines 934-939](../../crates/airk-kernel/src/current/enrichment.rs#L934-L939));
it reads the legacy extension-era queue, not the service watch table. The
statement in [docs/vscode-extension.md](../vscode-extension.md) line 260 that
"an edit during a drain sends one durable queue notification so source work
can take the next turn" describes that legacy path and does not hold for the
service.

**Job ordering offers no priority.**
[`next_pending_job`](../../crates/airk-store/src/service/registry/jobs.rs#L212-L224)
orders by sequence; even if a reconcile were queued behind a continuation it
would wait for it.

**Nothing discloses the lag.** `airk_diagnostics` shows the requested and
completed revisions, but `airk_status` and search responses do not, so a
client cannot tell that lexical evidence is behind the source tree.

### Recommendations

1. **Give due source work priority over semantic continuations.** In
   `due_background_jobs`, exclude the `Vectors` continuation for a project
   whose watch row has `requested > indexed` and is otherwise admissible
   (`enabled`, not paused, attempts < 3, `next_ms` reached, no source budget
   block); or evaluate `due_source_projects` before `due_background_jobs` and
   skip the continuation for projects that just received a reconcile. The
   SQL condition is the robust form because it survives loop reordering.
   Keep the existing rule that a vectors turn never runs concurrently with
   source work on the same project.
2. **Restore in-turn preemption for the service.** Replace the legacy queue
   probe in `drain_current_enrichment` with the service watch observation
   (requested versus completed revision) so a multi-batch turn stops at the
   next batch boundary when source changes arrive. With `max_batches = 1` this
   is a small win; it matters if the turn size is raised later.
3. **Disclose source lag on reads.** Add `freshness.source_updates_pending`
   (requested and completed revisions, or a boolean plus age) to
   `airk_status` and to the `include_status` block of search results, and
   show it in Management and the Workbench readiness panel. Parity: CLI
   `status`, MCP `status`, Workbench readiness, Markdown/JSON copies.
4. **Document the scheduling rule** in [docs/service-retrieval.md](../service-retrieval.md)
   or [docs/index-management.md](../index-management.md) and correct
   docs/vscode-extension.md line 260.

### Regression coverage to add

- Store test: with `semantic_pending = 1` and a watch row `requested > indexed`,
  `due_background_jobs` returns nothing for that project and
  `due_source_projects` returns it; after the reconcile completes, the
  continuation becomes due again.
- Service runtime test (extend [service_runtime.rs](../../crates/airk-cli/tests/service_runtime.rs)
  or [check-service-clients.py](../../scripts/check-service-clients.py)): queue
  a multi-turn semantic backlog, add a marker file, assert lexical visibility
  within one turn (bounded by the 30-second soft deadline plus admission),
  and assert the backlog finishes afterwards.
- Status parity test for the new freshness field across CLI, MCP and
  Workbench.

---

## Issue #63: `airk mcp` exits silently with code 0 on a message over about 65 KB

### Reported behavior

Tool calls up to about 65,000 bytes get the normal `query exceeds 4096 bytes`
tool error. From about 70,000 bytes the server stops answering, closes the
pipe and exits with code 0; nothing is written to stderr. A client cannot tell
this from a normal shutdown. The service stays healthy and a new session works.

### Root cause analysis

The frontend bounds stdio lines deliberately:
[`BoundedRead`](../../crates/airk-mcp/src/remote.rs#L521-L553) counts bytes
per line and returns `io::ErrorKind::InvalidData` ("MCP request exceeds frame
bound") once a line exceeds
[`MAX_FRAME_BYTES`](../../crates/airk-core/src/service.rs#L8) = 65,536. The
wrapper [`ObservedRead`](../../crates/airk-mcp/src/remote.rs#L496-L518) fires
the "input closed" signal on any read error. rmcp 3.2.0 reports a transport
read error as `QuitReason::Closed`, the same as end of input, so
[`serve_io_with_input_lifecycle`](../../crates/airk-mcp/src/remote.rs#L436-L456)
returns `Ok(())`, `unified_mcp::serve` runs its cleanup successfully, and
[`main`](../../crates/airk-cli/src/main.rs#L1009-L1021) exits with success.
No stderr line is written anywhere on this path. The 64 KiB bound is
documented only for `airk tools call --args-file`
([docs/mcp.md](../mcp.md) line 169), not for MCP stdio sessions or in the
capability payload.

The bound itself is sound (the service frame limit); the defect is that
exceeding it is indistinguishable from a clean close.

### Recommendations

1. **Answer, then keep serving.** On overflow, switch `BoundedRead` to a
   discard-until-newline state, then emit one JSON-RPC error response
   (`-32600`, message naming the 65,536-byte bound and the request's byte
   count, `id: null` since the id cannot be recovered without parsing the
   whole line) and continue the session. This matches how smaller invalid
   requests already behave.
2. **If the session must end, say so.** Where the transport cannot recover,
   write one stderr line (`airk mcp: request exceeded the 65536-byte frame
   bound; closing session`) and exit non-zero, distinct from EOF.
3. **Document the bound** in docs/mcp.md (stdio section), in the
   `capabilities` limits block
   ([lib.rs:1272-1284](../../crates/airk-mcp/src/lib.rs#L1272-L1284)) and in
   agent guidance, together with the existing 4,096-byte query bound.

### Regression coverage to add

- `remote.rs` test module: a line of 70,000 bytes yields an error response
  and the next well-formed request still succeeds; a line exactly at the
  bound is accepted.
- CLI transport test in [crates/airk-cli/tests/mcp.rs](../../crates/airk-cli/tests/mcp.rs):
  oversized request produces the documented reply or a non-zero exit with the
  stderr line, never exit 0 with no output.

---

## Issue #59: "From an older index" on current results

### Reported behavior

Two default searches at 19:53:57 and 19:58:53 showed the banner; two later
searches at 20:28:18 and 20:53:04 did not. All four returned the same
publication, revision 13 (`airk-r1:14759d59…:13:f8c46278…`). Nothing explains
what the banner refers to.

### Root cause analysis

The banner is computed in the webview at
[main.tsx:78-82](../../packages/vscode/src/workbenchWebview/main.tsx#L78-L82):

```tsx
const publication=object(state.status?.publication).token,
      old=typeof publication==='string'&&typeof envelope.publication==='string'&&publication!==envelope.publication;
{old&&<p class="warning">From an older index</p>}
```

`state.status` is the host's readiness snapshot. The host fetches it with one
`airk_status` call when a session is selected
([workbenchView.ts:121](../../packages/vscode/src/workbenchView.ts#L121)) and
again only on the explicit **Refresh readiness** action
([line 236](../../packages/vscode/src/workbenchView.ts#L236)). It is cleared,
not refreshed, on session, project and client changes. Executing a turn never
updates it. The comparison is a plain inequality, so it cannot tell "this turn
is older than the index" from "the snapshot is older than this turn".

Applied to the report: revision 13 was published at 19:47:26-27 (vectors
succeeded, reconcile, verify) after the tester's Start click. A Workbench
session opened before that holds a revision-12 snapshot, so the 19:53 and
19:58 turns, which correctly ran against revision 13, were flagged as old.
Something between 19:58 and 20:28 (reopening the view, switching sessions, or
Refresh readiness) replaced the snapshot and the banner disappeared. The
`model_failed` records at 19:53:37 and 20:04:09 are unrelated; they do not
change the publication.

Supporting facts: the publication digest is a framed hash of the canonical
current head ([current/mod.rs:169-175](../../crates/airk-store/src/current/mod.rs#L169-L175)),
so one generation always maps to one token; a token mismatch at the same
generation is not possible, which is why the tester saw identical tokens on
the result side. The design contract
([docs/workbench/design.md](../workbench/design.md) line 529) says the banner
appears "if the index has moved on" and that unobservable readiness must be
stated, not implied; the implementation checks neither direction nor snapshot
age. [workbenchDisplay.test.ts](../../packages/vscode/src/test/workbenchDisplay.test.ts)
covers status updates but has no case for the banner direction.

### Recommendations

1. **Refresh the snapshot after every executed turn.** When a turn's envelope
   carries a publication whose incarnation matches the snapshot and whose
   generation is higher, adopt it as the newest known publication immediately
   (no round trip), then issue a bounded `airk_status` (the service caches it
   for five seconds) to pick up readiness fields. Keep the explicit Refresh
   action.

2. **Make the comparison directional and explicit.** Parse both tokens as
   `airk-r1:<incarnation>:<generation>:<digest>` (the host already has the
   `RevisionToken` grammar from [identity.rs](../../crates/airk-core/src/identity.rs#L76-L110)).
   Show the banner only when the turn's generation is lower than the
   snapshot's at the same incarnation, or when the incarnation differs
   (rebuilt index). Replace the text with
   "From an older index: this result used revision 12; the current index is
   revision 13." and keep **Re-run**. When the turn is newer than the
   snapshot, show nothing and refresh. When `state.status` is absent or the
   publication is unparseable, show "Current readiness not checked" per the
   design contract instead of nothing. Explain the banner in the Workbench
   help topic and in [docs/workbench/design.md](../workbench/design.md) line 39.

3. **Parity.** The Markdown/JSON copies and exports
   ([workbenchRender.ts:182](../../packages/vscode/src/workbenchRender.ts#L182))
   already record the exact publication per turn; add the current revision
   observed at copy time only if the banner is shown, so exports carry the
   same statement as the view.

### Regression coverage to add

- `workbenchDisplay.test.ts`: turn generation < snapshot → banner with both
  generations; turn generation > snapshot → no banner and snapshot adopted;
  different incarnation → banner; snapshot absent → readiness-not-checked
  note.
- `workbenchClientLifetime.test.ts`: a completed run is followed by exactly one
  `airk_status` call and the posted status carries the newer token; failures
  of that status call do not alter the turn.
- Fixture-level check that the helper-failure events in #58 produce no banner
  when the publication is unchanged.

---

## Issue #65: untrusted workspace commands fail with a raw error code

### Reported behavior

In Restricted Mode, `AIRK: Index Now`, `Set Up`, `Install or Repair CLI`,
`Check AIRK MCP Helper` and `Connect Coding Agent…` all fail with the
notification "AIRK: AIRK service: runtime_client_no_longer_authorized" plus a
modal "Command … resulted in an error". Nothing says the folder must be
trusted. All commands stay in the palette.

### Root cause analysis

[`ServiceExtension.trusted`](../../packages/vscode/src/serviceExtension.ts#L170-L174)
throws `ServiceError('runtime_client_no_longer_authorized')` when
`vscode.workspace.isTrusted` is false. The same code is thrown for an untrusted
window in [serviceConsole.ts](../../packages/vscode/src/serviceConsole.ts#L140)
(lines 140, 158, 187, 199), [serviceWorkspace.ts:64](../../packages/vscode/src/serviceWorkspace.ts#L64),
[serviceConsoleController.ts:70](../../packages/vscode/src/serviceConsoleController.ts#L70)
and [serviceBootstrap.ts:199](../../packages/vscode/src/serviceBootstrap.ts#L199),
and it is also the code for "controller disposed" and "generation changed",
so one code covers three unrelated conditions. The command wrapper `safe`
([serviceExtension.ts:1049-1050](../../packages/vscode/src/serviceExtension.ts#L1049-L1050))
shows `AIRK: ${error.message}` (the `ServiceError` message is
`AIRK service: <code>`, [serviceClient.ts:22-24](../../packages/vscode/src/serviceClient.ts#L22-L24))
and rethrows, which is what produces the modal. `package.json` declares
`untrustedWorkspaces.supported: "limited"` with a good description
([lines 76-79](../../packages/vscode/package.json#L76-L79)) but none of the 59
`airk.*` commands carries a `commandPalette` `when: isWorkspaceTrusted`
clause; the only four `when` rules are `false` for tree commands. The
Explorer views already render the right sentence
([overview.tsx:46](../../packages/vscode/src/explorerWebview/overview.tsx#L46),
[manage.tsx:44](../../packages/vscode/src/explorerWebview/manage.tsx#L44)), and
[`onDidGrantWorkspaceTrust`](../../packages/vscode/src/serviceExtension.ts#L1101)
already resumes installation when trust is granted.

### Recommendations

1. **Introduce a distinct `workspace_untrusted` code** for the trust checks
   (keep `runtime_client_no_longer_authorized` for disposal and generation
   change) and map it in `safe` to the existing sentence, "Trust this
   workspace in VS Code before AIRK can connect or read evidence", with a
   **Manage Workspace Trust** button (`workbench.trust.manage`). Do not
   rethrow for this code, so no modal appears; log the code to the output
   channel.
2. **Hide native commands while untrusted.** Add `"when": "isWorkspaceTrusted"`
   to the `commandPalette` entries of every command that calls `trusted()`;
   keep informational commands (open Explorer, help, logs, show status)
   visible. Make the status bar item "AIRK: untrusted" open trust management.
3. **Audit the other throw sites** listed above so the Explorer, Workbench and
   console paths use the new code and sentence; add the sentence to the
   extension README's Restricted Mode note.

### Regression coverage to add

- Extension test for `safe`: `workspace_untrusted` shows the sentence with the
  trust action and does not rethrow; other codes keep current behavior.
- Script contract test over `package.json` that every command whose handler
  calls `trusted()` has the trust `when` clause (the existing manifest checks
  under `scripts/` are the natural home).

---

## Issue #61: `service register` fails when `--data-dir` does not exist yet

### Reported behavior

Headless flow from `docs/cli.md`: `service prepare-registration` succeeds for
`--data-dir /tmp/airk-data/demo`, then `service register` fails with
`operation_failed: resolve service data root: No such file or directory`. The
parent exists; only the leaf is missing. Creating it first makes `register`
work. The help for `serve` says it creates a missing control directory.

### Root cause analysis

The two commands validate differently. `service register`
([main.rs:1672-1683](../../crates/airk-cli/src/main.rs#L1672-L1683)) calls
[`service_projects::registration`](../../crates/airk-kernel/src/service_projects.rs#L194-L213),
which canonicalizes the data path and therefore requires it to exist.
`service prepare-registration` calls
[`planned_registration`](../../crates/airk-kernel/src/service_projects.rs#L230-L248),
which resolves a planned location through `data_scope::canonical_location`
and accepts a missing leaf. The extension never hits this: it reserves the
project and then runs `service prepare-storage`
([serviceBootstrap.ts:447](../../packages/vscode/src/serviceBootstrap.ts#L447)),
which creates the reserved directory chain with mode 0700
([service_registration.rs:13-60](../../crates/airk-cli/src/service_registration.rs#L13-L60))
after re-validating the planned identity. That step is documented in
[docs/service-project-setup.md](../service-project-setup.md) line 14 and in
the CLI reference, but the headless quick start in
[docs/cli.md](../cli.md) lines 55-56 shows only `prepare-registration` then
`register`, and the `mkdir -m 700` advice on line 66 is presented as a privacy
tip rather than a prerequisite. The error message names neither the path nor
the remedy.

### Recommendations

1. **Make `service register` accept a planned data directory**, matching
   `prepare-registration`: build the registration with `planned_registration`
   (plus `validate_planned`), submit the reservation, then run the same
   `create_reserved_storage` step the extension uses, so one command completes
   the documented flow. Keep the 0700 creation, the parent-must-exist rule and
   the canonical re-checks.
2. **Name the path and the fix in the error** for any remaining missing-path
   case: `data directory /tmp/airk-data/demo does not exist; run "airk service
   prepare-storage --runtime-dir … --project …" or create it with mode 700`.
3. **Fix the quick start** in docs/cli.md, docs/headless-cli.md and
   docs/cli-reference.md so `prepare-registration`, `register` and storage
   creation are shown as one sequence with the data directory requirement
   stated.

### Regression coverage to add

- CLI test: `service register` with an existing parent and missing leaf
  succeeds and creates the leaf with mode 0700 on Unix; a missing parent is
  refused with the path in the message.

---

## Issue #62: `airk files` help says "under --prefix" but the prefix is positional

### Reported behavior

`airk files --prefix src` is rejected (`unexpected argument '--prefix'`); the
usage line is `airk files [OPTIONS] [PREFIX]`, yet the help text says "List
indexed files under --prefix, with continuation cursors".

### Root cause analysis

The doc comment on the subcommand
([main.rs:347](../../crates/airk-cli/src/main.rs#L347)) is the clap help
source. The same sentence is repeated in
[docs/cli-reference.md](../cli-reference.md) lines 154 and 990 and in
[docs/cli-developer-guide.md](../cli-developer-guide.md) line 408. No script
generates `cli-reference.md`, so all three documents need the same edit.

### Recommendations

1. Change the doc comment to "List indexed files under an optional path
   prefix, with continuation cursors." and update the three documentation
   lines to match. Do not add a `--prefix` alias; the positional form is the
   documented contract and an alias would create two spellings to keep in
   parity with MCP's `path_prefix`.
2. Add a repository check that the CLI reference help lines match the clap
   help output (the Python contract tests already exercise CLI help; one
   assertion comparing `airk --help` subcommand summaries with the reference
   table would have caught this).

---

## Cross-cutting notes

- **Versions.** All nine issues reproduce on 1.8.7 and on the 1.9.0
  candidate; the P0 to P3 work on this branch does not touch the helper
  lifecycle, search mode resolution, status payload, Workbench banner,
  scheduling, stdio bound, trust gate, registration or launcher/signing
  paths. The uncommitted verification work modifies
  `airk-kernel/src/workbench.rs` and `airk-mcp/src/response.rs`; the #58 fix
  lands in the same files, so sequence it after that work is committed.
  The 1.9.0 bump is already requested for this branch; per CLAUDE.md do not
  bump again for these fixes unless they ship in a separate PR.
- **Suggested fix order.** #66 and #67 together (one packaging and release
  change: identical signed artifacts whose launcher survives being copied),
  then #58, then #64, then #63, #59, #65, #61, #62. #66 blocks every macOS
  user of the release package and should not wait for the rest.
- **Documentation to update with the fixes.** [docs/service-embedding.md](../service-embedding.md)
  (restart budget, new log kinds), [docs/mcp.md](../mcp.md) and
  [docs/service-retrieval.md](../service-retrieval.md) (typed
  `helper_unavailable` hint, lexical degrade disclosure, stdio frame bound,
  source-lag freshness field), [docs/vscode-extension.md](../vscode-extension.md)
  (Service summary line, Restricted Mode behavior, line 260 scheduling
  claim), [docs/workbench/design.md](../workbench/design.md) (banner
  semantics), [docs/cli.md](../cli.md), [docs/headless-cli.md](../headless-cli.md),
  [docs/cli-reference.md](../cli-reference.md) and
  [docs/cli-developer-guide.md](../cli-developer-guide.md) (registration flow,
  `files` help), [docs/release-workflow.md](../release-workflow.md),
  [docs/native-signing.md](../native-signing.md) and
  [docs/release-verification.md](../release-verification.md) (channel policy,
  launcher gate, Marketplace verification), the README and extension README
  (signing claims), and [docs/releases/release-1.9.0.md](../releases/release-1.9.0.md).
- **Replies to the testers.** For #58, confirm the three-failure budget as the
  cause of the missing restart and ask whether Start or Retry was used
  between 18:47 and 19:14, plus a fresh health report captured during an
  outage with the Management activity list visible. For #66 and #67, confirm
  both reports and say that the Marketplace package is the unsigned CI build
  until the channels are unified. For #64, confirm the scheduling cause and
  that lexical results are not expected to wait for embeddings.
- **Open questions.** The root cause of the helper exits in #58 is unknown
  until the log carries a code; do not describe the restart-budget fix as a
  fix for the exits themselves. The exact macOS version at which the sealed
  executable copy started being killed is not established; the gate in #66
  recommendation 2 answers it per release. The default search fallback (#58)
  and the freshness field (#64) change read contracts and need the parity
  audit described in CLAUDE.md before they ship.
