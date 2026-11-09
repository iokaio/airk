# One AIRK completion stop — 8 November 2026

Local completion has **not** been reached. This note retains the original stop
and the subsequently authorized repair cycle below. The approved completion plan's
two-strike rule stopped validation after the second actual modern checkpoint
preflight failed. At that stop, no third attempt, later platform batch, commit,
push, PR, merge or publication had been performed.

## Completed work and evidence

The four planned repairs and their full combined diff received primary and
independent review. Final mocked extension validation passed 1,089 tests with
21 gated skips; full Python validation passed 854 tests with 13 skips.
Workspace all-feature Clippy and targeted native regressions passed. Additional
cheap tests cover complete HTTP refusal evidence, container input isolation,
actual bundled Workbench controls, full exports, saved-config refresh contracts,
held native inference/HTTPS status and canonical receipt validation.

The original Windows r26 debug runtime built in 63.785 seconds. Its embedded
ORT 1.22.1 CPU smoke reports 384 dimensions and repeat equality. This is debug
build/model evidence, not installed, optimized or cross-platform acceptance.

- Runtime: `artifacts/local/one-airk-1.8.14/windows-debug-r26/runtime`.
- Manifest SHA256: `2069c2d6e3acc48bba07a808fae4b160c46def14b76b03cb12a768357cf2138a`.
- Original compiled source: `ac2620f07a33fa096349197001edaefa5370caacb18310270181b062fbda42ef`, 1,116 inputs.
- Test source at the original stop: `821c1c969f46815270e99410e4ecdaf7ca958971ba4640d01263f14b813e24f0`, 1,116 inputs.
- Only two external cfg(test) modules differ from the original binary's source;
  every other frozen input and all 811 runtime file seals remain unchanged.

The [implementation ledger](one-airk-implementation.md) records exact commands,
scopes, timings, source identities and all intermediate failures.

## The two failures

Receipts below are under `artifacts/local/one-airk-1.8.14/`; their complete logs,
JUnit and Cargo timings remain alongside them.

1. `completion-modern-checkpoint-r26-r1.json`: one selected test failed before
   fixture allocation, 0.028 seconds execution / 24.085 seconds runner. The new
   fixture confused install manifest schema 1 with executable metadata schema 2.
   A pure regression reproduced it before correction. Typed install/build/runtime
   receipt validation and an exact read-only check of the sealed r26 candidate
   then passed 2/2 T0 tests, with scoped Clippy passing. The independent reviewer
   approved the exact two-file test-only correction and reuse proof.
2. `completion-modern-checkpoint-r26-r2.json`: one selected test failed,
   791 excluded, 3.177 seconds execution / 4.403 seconds runner. It reached real
   worker inspection, then failed model preparation with
   `source-bearing preparation requires its native administrative owner`.
   Model configuration, held embedding and checkpoint assertions were not reached.

The second reuse proof reports all inputs and artifact seals unchanged before
and after. The original r26 binary was reused, never rebuilt or relabeled.

## Second failure: root cause and affected callers

`NativeFixture::configure_model_and_publish` in
`crates/airk-cli/src/project_supervisor_guarded_qualification_tests.rs` still
calls `Draft::InstallModel { directory }.prepare()` directly. The current
production contract deliberately rejects every source-bearing draft through
that pure helper (`crates/airk-cli/src/service_configuration.rs:160`). Model
preparation instead requires the native administrative owner to capture a
policy-checked, bounded input pin. The async production preparation path at
`:222` preserves the original path, requests `PrepareInputFile`, validates the
returned pin and constructs `InstallModel` from its canonical parent and SHA.

The wrong assumption was that a legacy qualification fixture could still
prepare source-bearing configuration locally. Reusing that shared helper for
the modern preflight inherited the stale API. The same helper serves the legacy
guarded qualification fixtures, so correcting only the modern call would leave
the affected behavior incomplete. Read-only caller audit found this direct
source-bearing preparation call in the shared fixture; production refusal is
intentional and must remain intact.

The missing cheap test was shared fixture preparation coverage. The existing
`source_bearing_drafts_cannot_bypass_the_native_preparation_owner` unit test
already proves the required refusal. Metadata, semantic planner and held-engine
T0 tests did not exercise the fixture's provisioning path.

## Bounded proposal at the original stop

At the original stop, no correction for this second failure had been implemented
or run. The subsequent authorized cycle is recorded below.

1. Replace the shared fixture's direct preparation with its existing native
   owner: `catalog::Operation::AdministrativeRead` → opaque
   `NativeAdministrativeRead.connect` → `NativeRead::PrepareInputFile` for the
   original `model.json` spelling, with the existing 16,384-byte bound.
2. Drop the RefCell owner borrow before awaiting the read. Require native
   success and validate the typed `InputFile`. Preserve its canonical path/SHA,
   actual inspection incarnation/settings and candidate-engine job identity.
   Do not locally hash source inputs or bypass source-policy capture.
3. Add an injected-observer T0 seam covering the original path, one exact native
   request, byte bound, returned pin and refusal propagation. Add a cheap fixture
   contract against direct source-bearing `.prepare()` calls; audit the shared
   modern and legacy callers together.
4. Run scoped T0/T1 and independent review, then one authorized setup-only
   actual worker preflight before the accepted multi-batch checkpoint case.
   Retain the original r26 artifact identity and separately record current test
   sources if all production inputs/seals remain unchanged.

The fixture has no foreground control listener, so simply calling the async
CLI preparation wrapper would not supply its native owner. Its existing opaque
administrative capability is the bounded route.

## Retained state and unrun gates

Failed fixture: the existing protected fixture base's
`airk-guarded-workers-MNXo1l` directory. It remains preserved. A read-only exact
fixture process observation at 22:38:28 UTC found zero matching `airk.exe`
processes; this is not a native cleanup receipt. Cleanup remains unqualified.
No stop, restart, deletion or recovery operation was used to manufacture success.

The r26 VSIX, enabled-native extension campaign, six installed editor scenarios,
1/10/64-client load, actual raw API clients, issuer rotation and actual Code trust
run remain unrun. No OS trust mutation occurred. Neo, Docker, final full
workspace/doctests, ci-local, optimized packaging and release gates remain unrun.
The approved limitations and original historical receipts retain their scope.

At the original stop, further repair/validation required the user's authorization
for a new cycle under the [completion plan](../one-airk-completion-plan.md#02-run-rules).

## Authorized fixture repair cycle — 8 November 2026

The user subsequently authorized continuing this proposal. The two external
test modules now route shared modern/historical model preparation through the
opaque native administrative capability and one `PrepareInputFile` read. The
original manifest spelling and 16,384-byte bound are retained. Native success
checking precedes typed pin validation; the returned parent/hash become the
configuration, with actual inspection settings/incarnation and candidate engine
identity unchanged. Structured refusals retain their complete typed body and
include it in failure diagnostics. No local source hashing or job replay was added.

Injected-observer regressions check the exact request/count, original spelling,
distinct returned canonical pin, transport errors, complete refusals and malformed
pins. A fixture contract catches direct source-bearing draft preparation. A
separate ignored actual setup preflight checks both workers' semantic readiness,
vector reads and unchanged sources before held/checkpoint qualification. Both
actual tests distinguish setup failures from checkpoint failures.

The targeted T0 selection passed **4/4**, 792 excluded, **0.056 seconds** execution
and **17.387 seconds** wrapper (`completion-model-preparation-t0-r2.json`). The
first sandbox attempt failed at linking because its identity could not create a
temporary file in the existing protected fixture base; no tests ran. The retained
contributor-identity rerun passed without changing ACLs or fixture policy.
Independent combined review found no blocking issues. Actual setup/checkpoint
results and final scoped checks are recorded in the implementation ledger;
these changes alone do not qualify cleanup, platforms or release completion.

The separate actual setup preflight then passed **1/1**, 795 excluded,
**13.709 seconds** execution / **14.907 seconds** wrapper
(`completion-model-setup-r26-r1.json`). The held/checkpoint preflight passed
**1/1**, 795 excluded, **17.818 seconds** execution / **18.967 seconds** wrapper
(`completion-modern-checkpoint-r26-r3.json`), including **66 actual embedding
calls**, exact accepted-job success, native worker sealing and provider completion.
Both fixtures remain retained. All **811** original r26 seals and all consumed
inputs were unchanged before/after; only the two external test modules differ
from the original compiled source. No rebuild or new artifact identity is claimed.
The repaired test source is
`9335cf1dbff5f128719382049e7db7e146df9d2db98a56bbe2e5d15eae21a735`,
1,116 inputs; the original compiled source remains `ac2620f0…`.
The fixture blocker is resolved for these Windows cases; the earlier failures
remain historical evidence and broader local/release completion remains open.

## Recurring fixture failures: systemic analysis — 8 November 2026

The user requested deeper analysis after the next legacy-editor producer failed.
No repair or new expensive run followed that request. This analysis distinguishes
the prior corrected defects from the shared reason that later fixture defects
keep escaping review.

The failures are different native boundaries, not one unchanged native error:

| Attempt | Wrong fixture assumption | Boundary actually required |
| --- | --- | --- |
| Checkpoint r1 | Modern executable implies install-manifest schema 2 | Independent install schema 1, runtime schema 2 and build schema 1 |
| Checkpoint r2 | A fixture may locally prepare a source-bearing model draft | Native administrative input capture and returned bounded pin |
| Legacy editor producer r26 | Runtime schema 1 implies raw legacy registration and imported-snapshot scope | Executable topology and catalog authority must be classified independently |

The first two corrections were substantive within their scopes: setup and held
checkpoint tests now pass against the original sealed r26, including 66 actual
embedding calls. They did not validate the independent Python/editor historical
producer. Describing that as a generally resolved fixture blocker would overstate
the evidence; only those selected native component cases were resolved.

### Current failure: confirmed chain, not just the error label

`prepare-native-editor-legacy.py:84` admits a schema-1 predecessor, then `:136`
calls shared `Fixture.register`. `check-service-clients.py:356` derives
`single_image` from runtime schema; `:502–525` also uses that topology flag to
choose registration and evidence authority. A schema-1 runtime therefore takes
`prepare-registration` followed by raw `Register`.

The selected authentic predecessor is 1.8.12, install manifest `be73a42f…`.
An immutable, read-only SQLite observation of the stopped failed fixture found:
registry user_version **12**, catalog authority_contract **1**, activated **1**,
accepted_new_writes **1**, **0 projects**, **0 index definitions**. No service
was restarted and no database writes were performed. Historical source at
`3c6e81c`, `service_registry.rs::register_project_in`, calls
`service_catalog_setup.rs::require_registration_admission`. Under authority 1,
raw Legacy registration requires an already ready canonical index mapping.
This empty catalog has no such mapping. That source/state combination explains
the observed generic `invalid_registry_request`; it is not evidence of malformed
JSON, exhausted retries or a current r26 migration failure.

The already-proven sibling `check-runtime-upgrade.py:737–739` explicitly selects
reviewed setup for this predecessor, using `reviewed_fixture` at `:489`. Its
catalog grant, evidence scope and original launcher arguments are preserved
separately from pre-catalog legacy imports. This distinction was available in
the repository and was missed in the producer implementation and review.

### Why the cheap checks and review missed it

`test_native_editor_fixture.py:41–49` replaces the entire Fixture, including
registration with unconditional success and Index with a synthetic succeeded
job. Its Session at `:95–105` synthesizes the expected citation. Consequently,
these tests verify producer bookkeeping and failure retention, but never execute
the shared registration decision, old protocol, real argument construction or
retrieval authority. The positive test supplies the behavior it is meant to
prove. `test_service_clients.py:23–46` likewise models only the two topology
cases, omitting schema-1 with authority-1.

Primary and independent review checked immutable seals, cleanup safety, available
branches and passing test totals but failed to trace the selected predecessor
through its actual authority contract. That review clearance was too narrow.
Hashes prove stable bytes, not semantic compatibility; test count does not prove
the boundary under investigation. The same review weakness let setup assumptions
survive the earlier metadata and model-preparation reviews.

### Why changing only registration would be another symptom fix

The producer next constructs a raw service-project launcher configuration.
`native-editor-legacy.cjs:98` requires that selector and `:127` requires imported
legacy lineage. `installedEditor.ts:267–300` requires native
`httpLegacyConnection`, `imported_legacy_snapshot` and retrieval under the exact
original imported authority. Reviewed 1.8.12 catalog/grant migration exercises a
different lineage and cannot establish those assertions. The existing sibling
upgrade script handles that distinction explicitly. Agent config recognition in
`agentMigration.ts:36–52` also needs an ownership/scope audit if supported input
forms change; widening recognition merely to accept a fixture is not justified.

Changing shared `single_image` globally would be incorrect: it also controls
runtime startup, lifecycle, transport, reporting and retirement. Those decisions
must retain their actual independent contracts.

### Required correction strategy before another expensive run

1. Specify the scenario's required authority lineage and choose an authentic,
   sealed compatible predecessor. Retain the required pre-catalog imported
   migration case; do not substitute reviewed catalog migration and call it the
   same coverage. If no suitable artifact exists, report that gate blocked.
2. Separate topology, registry authority, input-preparation ownership and
   connection scope in fixture decisions. Derive them from verified contract
   evidence; reject unsupported combinations before activation. Reuse proven
   owning helpers instead of reimplementing their setup protocol.
3. Add a cheap compatibility matrix: schema-1/legacy authority,
   schema-1/catalog authority and schema-2/catalog authority. Exercise real
   classification and setup/argument functions; mock process I/O, not the
   decisions being verified. Include exact launcher forwarding, management vs
   evidence scopes, grant/provenance and producer-to-consumer compatibility.
4. Audit the complete producer chain and shared helper callers, including
   registration, index setup, pause, launcher config, original read, migration,
   retirement, editor enrollment and Stop. Add negative cases for incompatible
   authority, crossed grants/namespaces/revisions/paths and unsupported inputs.
5. Only after the cheap tests and whole-chain review pass, run one matched
   setup preflight. Retain failed inputs and immutable identities; no native
   rebuild follows from harness-only changes. Preserve the existing stop rules.

Current receipts: native packaging passed **17.471 s**, VSIX **20.498 s**,
enabled-native extension **1,108 passed / 3 skipped / 0 failed**, **162.923 s**
wrapper. Producer `completion-editor-fixture-r26.json` failed **30.091 s**;
all frozen inputs and 811 runtime seals remained unchanged. Failed fixture
`artifacts/ef26-9af3cdfd/legacy` is retained. Its exact native Stop completed,
and subsequent health was unavailable; no uninstall or broader cleanup claim
follows. Candidate activation, installed legacy migration and later ladder
stages were not reached. No production migration correction has been proven
necessary by this failed fixture; those production paths remain unqualified.
