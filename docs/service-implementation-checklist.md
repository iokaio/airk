# AIRK 1.4.0 implementation completion checklist

This is the working completion record for the [service plan](airkservice-architecture.md). A stage commit records progress. It does not close the implementation task. A checked item requires implemented behavior and the named validation evidence; development fixtures do not establish installed-platform acceptance.

User authorization: implement the entire plan, make human DCO-signed local stage commits, do not push, and build a new VSIX only after implementation, testing and review. The approved release is 1.4.0.

## Implementation and validation

- [ ] Phase 0: baseline and selected authenticated publication contract, including bounded readers, durable disk admission and crash/tamper evidence. Four Windows/Linux mechanism experiments, ordinary publication and sibling handoff integration now pass; the complete baseline/qualification matrix stays open.
- [ ] Phase 1: service skeleton, trusted discovery, concurrent launchers, authenticated control and direct worker endpoints, bounded handlers and reconnect. Native transport and brokerage fixtures pass; installed discovery remains open.
- [ ] Phase 2: durable requests, idempotency, fencing, supervision, pause, cancellation and recovery. Native/store fixtures cover acceptance and ownership; running pause/cancellation is now implemented and native-tested; captured-recovery jobs, persisted retirement observations and bounded automatic retirement are integrated; service-owned directory watching, durable source-intent capture and automatic reconciliation now have native coverage; detailed recovery receipts and the complete maintenance fault matrix remain open.
- [ ] Phase 3: integrated coherent publication, request/object leases, reserved replacement builds, atomic manifest handoff and ownership-proven recovery/cleanup. Ordinary format-140 publication now passes five integrated Windows/Linux store regressions, six native process tests and the Windows shared MiniLM smoke; sibling replacement/retirement, exact rebuild receipts and manifest crash tests now pass on Windows/Linux; complete maintenance integration and the remaining qualification matrix stay open. Isolated WAL experiments are not completion evidence.
- [ ] Phase 4: fair service-wide resource policy, observed pressure, finite extraction/inference/query grants, presets, compatible model reuse and complete drain accounting. The executable split, direct pool and renewal fixtures pass; exact service-issued job grants, fair resource admission, cached RSS observations, presets and running pause/cancel are implemented; pooled inference now pins bounded immutable shared files independently of collection retirement; global provisioning/calibration-file cutover, workload and installed-platform qualification remain open.
- [ ] Phase 5: immutable verified offline runtime pair, trusted user launcher, on-demand startup, optional user startup, upgrades, rollback and uninstall. Native installer/stable-launcher and bounded idle-lifecycle code is integrated; the Windows offline stage/elect/upgrade/rollback/uninstall fixture and short-socket Linux native suite pass. Interrupted staging has durable reservation, bounded resume/abort and process-death coverage. Shared login mutations require OS-held configuration proof; Windows native-schema and Linux typed local-bus adapters are integrated. Durable activation coordinates drain, stopped-owner launcher repair, manager-owned startup and registry readiness before public selection, with resumable phases, independent receipts and explicit pre-commit abort. Native user/XDG state binding and managed read-only payload references are integrated, including external-supervisor ownership and resumable user detachment. Native LaunchAgent proof, rotated logs, extension policy presentation and installed-target qualification remain open.
- [ ] Phase 6: trusted service console, bounded status/events, restart, one/selected/all repair, closed/missing projects, stale plans, cancellation and resumable partial-failure receipts; actual multi-window editor acceptance. Fenced durable native job-progress samples, phase timing and pause-aware stall handling are integrated; console presentation and detailed failure/event history remain open.

Missing-source integration: native workers now serve existing ownership-matched retained evidence after a registered root disappears, report source availability separately and reject new scans/rebuilds with `source_unavailable`. The console still needs to present these native observations.

Source-isolation integration: registrations and fenced additional-root updates now serialize overlap checks against all registered data, control and verified installer-private paths. Workers validate pending source configuration before capture. Windows/Linux process regressions preserve committed evidence and private bytes on rejection; new cache/log locations introduced at cutover must join these protections.
- [ ] Phase 7: default extension, CLI, container and read-only MCP cutover; fresh 1.4 namespace, old-helper rejection and explicit legacy agent configuration review; no direct-store fallback or service payload forwarding. Audit separate 1.4 quota/lifecycle metadata ownership as well as collection filenames, so legacy-root cleanup cannot release service allowances.
- [ ] Phase 8: synchronized native/extension/lockfile 1.4.0 versions, exact artifact receipts, repository/native/extension checks, workload and installed-platform matrix, review and release documentation.

## Token-efficiency additions

Implement and validate these during Phase 7 MCP/extension cutover, after the worker/publication/resource contracts are integrated. Plan changes do not mark their code or acceptance gates complete.

- [ ] Terse by default, extension-panel Full selection, per-call override and agent guidance in MCP instructions/tool descriptions.
- [ ] One revision token, changed/requested status only, preserved candidate/import distinctions, no repeated static provenance, and measured bounded payload comparisons.
- [ ] Minimal outline/symbol records with shared original file bindings and consistent `items` across fact tools, empty and conditional results.
- [ ] Committed-generation reads during publication plus structured retry hints on safe temporary rejection.
- [ ] Terminal method-name postings for receiver/chained calls, explicitly labeled candidates.
- [ ] Authenticated bounded multi-generation/time-window change baselines and explicit `index-current` validation outcomes.

## Final gate

- [ ] Reconcile every requirement and exit criterion in the full plan against implementation and evidence, including requirements inside sections preceding the phase table.
- [ ] Finish relevant native, extension, packaging, license, repository, formatting, Clippy and whitespace checks. Identify unavailable qualification explicitly; never count it as passed.
- [ ] Review the complete change, fix findings and rerun affected checks.
- [ ] Verify README, documentation index, release notes and Marketplace description agree with the implemented 1.4.0 behavior.
- [ ] Build the new VSIX only after the preceding implementation, test and review gates pass; record its exact path and digest for manual installation.
- [ ] Verify local stage commits have the authorized contributor's DCO trailer and no push occurred.

If a concrete blocker cannot be resolved within existing authorization, record the affected requirement and required input. Completing a stage, a long test run or a convenient subset is not a stopping condition.
