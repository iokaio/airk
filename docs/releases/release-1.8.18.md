# AIRK 1.8.18 local VSIX candidate

**Unpublished Windows x64 manual-test candidate.** On 9 November 2026 the user
requested the accepted simplification closeout plan be completed through a new
VSIX for manual testing. The working version was first synchronized to 1.8.17;
no 1.8.17 package was ever built. Later that night the user asked for the work
to be finished and the build number raised again, giving **1.8.18**. Package
verification, installed-editor acceptance and broader release qualification
have separate receipts. Merge, publication and release-branch actions require
separate authorization.

The [release-readiness ledger](../simplify/release-readiness.md) owns the blocker
roster, input identities, exact selections, commands, receipt paths and collected
results. Source findings, reproduced failures, passing hypotheses and final
qualification are recorded separately. Earlier debug packages cannot qualify
changed dependency or version inputs.

## Compatibility and scope

The persisted clean-break baseline remains **1.8.16**, with
`state_contract: "airk-state/1"`. Every earlier installation creates fresh AIRK
state and rebuilds indexes from reviewed source selections and supported
preferences. Original source files remain untouched. Old index IDs, credentials,
grants and accepted operations do not carry authority into the new state.
Ownership-proven predecessor shutdown and cleanup remain required; uncertain or
incomplete cleanup stays visible.

Supported ordinary upgrades in the 1.8.16+ line retain independent runtime,
state-contract, format, checkpoint and authority checks. This patch-version
adjustment changes no digest input, persisted format or catalog layer. The user
approved a narrow D8 exception for the security SDK update: modern MCP catalog
replies (protocol 2026-07-28 and later) add `ttlMs: 0` and
`cacheScope: "private"`; legacy catalogs and tool-call data keep their existing
contracts. The retained schema fixtures prove their documented layer coverage;
they do not establish compatibility with every historical installation.

The [1.8.16 record](release-1.8.16.md) and
[9 November stop record](../research/installer-stop-2026-11-09.md) retain the
earlier package-verified VSIX's failed installed-editor acceptance. The later
authority-based binding correction has deterministic regressions; installed
acceptance of the new candidate still needs its own receipt. The
[simplification handoff](../simplify/handoff-2026-11-10.md) retains the structural
work and its original qualification scopes.

## Focused closeout changes

- The service-client harness uses the reviewed single-image runtime version
  boundary after strict version and metadata validation, preserving historical
  artifact/source admission pins.
- Interrupted-adoption restoration has deterministic success, refusal and
  recovery observers. State-contract fixtures use initialized owned state, and
  the public uninstall command has explicit coverage mapping and confirmation
  refusal coverage.
- The security update pins `rmcp` and its macros to 3.9.0, with exact source and
  license review, dependency-policy dispositions and transport/lifecycle
  regressions. The narrow modern catalog wire exception is described above.
- Shipping-feature inference test identity mappings and the actual script and
  boundary-gate scopes are documented. One kernel quota test remains quarantined
  through **23 November 2026**, owner **Tyler Jensen**, because its intermittent
  failure and quota-margin assumption lack a confirmed root cause. All its
  assertions remain intact; this is an unresolved coverage exclusion. See the
  [investigation](../simplify/qualification-evidence.md#kernel-quota-flake-evidence-and-remaining-uncertainty)
  and [ignore inventory](../testing/ignore-triage.md).
- Host-dependent qualification uses supported owner-controlled fixture
  directories. Initial capability failures and failed fixtures retain their
  original identities.

Corrections made while qualifying the installed candidate (10 November 2026):

- **Ordinary updates no longer fail on the startup cooldown.** An approved
  update starts its candidate, deliberately drains it for catalog admission and
  starts it again. The second start fell inside the durable on-demand restart
  cooldown and was refused, leaving a committed selection with a pending
  activation. The explicit activation, probe and rollback route now waits for
  the existing 2/4-second admission boundary inside its one 60-second startup
  deadline. Ordinary on-demand starts keep their immediate refusal; the
  three-attempt circuit, backwards-clock refusal and receipt history are
  unchanged, and no start is repeated after an unknown outcome.
- **Index setup review refuses AIRK-private locations.** A setup draft whose
  source or data root lies inside AIRK's install, state or log locations is now
  refused at review, using the same read-only check the reservation repeats.
- **The saved search scope is restored after a reload or restart without
  opening the Explorer.** The MCP definition provider performs the same
  read-only revalidation of the existing saved grant, and an in-flight read
  closed by a replaced control connection is read again a bounded number of
  times. Changed or unavailable metadata is never retried and still needs
  Apply selection.
- **Per-index Usage administration opens again.** That view still launched the
  runtime through the launcher route retired in 1.8.14 and was refused with
  `stdio_mcp_retired`; it now uses the direct service route.
- **An unterminated final request on the bounded stdio component is answered
  again.** After the MCP SDK security upgrade a last request without a trailing
  newline at end of input received no reply; the bounded reader now terminates
  that frame once. This component is not the runtime HTTPS transport.
- A routine installation-lock contention during a service restart no longer
  raises the "saved HTTPS connections still need refreshing" warning unless
  the scheduled retries keep failing.

The local candidate keeps native manifests, lockfiles, extension metadata and
the generated catalog at 1.8.18. Public 1.9.0 remains a future release target.
No publication, signing or cross-platform success is inferred from this record.

## Qualification evidence

The [release-readiness ledger](../simplify/release-readiness.md) identifies
reviewed input freezes, native and extension artifact hashes, exact commands,
selected test identities, completed results, retained failures and remaining
gates. Its process-preflight result and subsequent fixture investigation remain
visible alongside passing component evidence. A package-only pass does not
establish installed acceptance or cross-platform qualification.

Manual testing uses the exact VSIX identified by its package receipt. Earlier
debug packages and results remain bound to their original compiled inputs;
changed dependencies, versions or production inputs require their own artifact
identity and relevant qualification. The kernel quarantine above remains an
explicit exclusion even when other selected checks pass.
