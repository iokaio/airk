# Qualification evidence reconciliation

Readiness investigation on 9 November 2026, against committed source
`08e94c96`. This is a source and retained-receipt review. No compilation,
test, runtime execution or installed journey was run for this investigation.
The earlier [handoff](handoff-2026-11-10.md) retains its original dated results.

## Shipping inference test identities

The S4 comparison cited in the handoff used **no default features**. The
`inference_engine` module is gated by `local-inference`, so that comparison
does not establish shipping-feature identity equality. Source comparison
of `117133f6^` with the current files finds the same 19 leaf test names:

| Module before and after, unless a mapping is given | Leaf test names |
| --- | --- |
| `inference_engine::pool::tests` | `registration_replaces_live_authority_at_capacity_without_revoking_siblings`; `invalid_or_colliding_registration_preserves_consumed_and_sibling_authority`; `repaired_owner_survives_delayed_old_release_without_losing_sibling_inference`; `compatible_projects_share_verified_inference_and_foreign_assets_are_rejected`; `equal_legacy_weight_digest_does_not_alias_distinct_pipeline_descriptors`; `interleaved_incompatible_models_return_only_their_scoped_vectors`; `bundled_provision_is_global_persistent_and_does_not_unload_compatible_inference`; `custom_provision_captures_once_and_shared_sessions_reject_other_runtime`; `calibration_sessions_use_global_inputs_and_preserve_exact_identity_across_projects` |
| `inference_engine::pool::retirement::tests` | `native_compute_context_is_exact_and_cleared_after_failed_and_successful_calls`; `revocation_during_model_selection_never_starts_native_inference`; `actual_engine_barrier_waits_for_inflight_compute_and_cancels_expired_auth_waiters`; `full_engine_queue_keeps_pending_ticket_and_old_waiters_stay_revoked_after_forget`; `drain_membership_bounds_and_foreign_full_identity_or_generation_refuse` |
| `inference_engine::pool::accounting::tests` | `completion_requires_actual_compute_delivery_and_cache_barriers`; `canceled_queued_work_releases_only_its_own_worker_charges` |
| **Before:** `inference_engine::pool::owner_tests`; **after:** `inference_engine::pool::owner::tests` | `held_inference_keeps_native_status_responsive_and_checkpoint_charged`; `actual_owner_failure_remains_visible_after_native_join_and_is_not_reap`; `established_native_connection_cannot_admit_after_its_grant_expires` |

Sixteen complete identities remain unchanged; the three owner identities
have the explicit module mapping above. This is source reconciliation, not
a retrospective compiled before/after outcome receipt. The handoff reports
a later 19/19 shipping-feature execution, but does not link its raw receipt.
Capture the current identities and outcomes during the next necessary
shipping-feature batch, retaining the record outside the Cargo target:

```console
python scripts/dev/test_identities.py record --output artifacts/release-readiness/cli-shipping-identities.json -- -p airk-cli --no-default-features --features local-inference --lib --locked
cargo test -p airk-cli --no-default-features --features local-inference --lib inference_engine::pool --locked -- --nocapture
```

The recorder lists the entire library; compare its inference subset with
the reviewed names above. Its `--run` mode runs entire selected binaries,
so it is not a filtered-test execution wrapper. The separate Cargo command
executes the inference subset. Check every identity and outcome; zero
selection is a failed check. Do not relabel source reconciliation as
execution proof.

## Mechanical scripts and S5 scope

`scripts/dev/s4_unwrap_embed_macros.py` is an **apply-only reconstruction
script**. Its first positional argument is the source root; it expects the
parent's macro sources after the documented file moves. It has no `--check`
mode and should not be invoked with that flag on the current tree. The
handoff's statement that every listed script supports `--check` is too broad.
The script remains reviewable evidence of the transformation; adding a
checker is separate work, not a prerequisite for claiming a checker exists.

`scripts/dev/boundary_check.py` enforces three implemented S5 gates: module
honesty, allowed normal workspace crate dependencies, and the top-level
module-cycle ratchet. It explicitly reports registration completeness and
declared state ownership as inapplicable because the corresponding registry
and declarations do not exist. Do not report these as five enforced gates.
The exact current command is `python scripts/dev/boundary_check.py --json`.

## Kernel quota flake: evidence and remaining uncertainty

The handoff reports intermittent failure of
`current::indexing::tests::optional_projection_quota_refusal_still_publishes_original_values_and_scalars`
about 1 in 7 isolated runs, including before S7.4. It does not retain a linked assertion/error
receipt. This investigation did not reproduce that failure and cannot
assign a confirmed root cause from frequency alone.

The test observes `CurrentWriter::required_publication_allowance`, drops
that temporary writer, and configures quota with the observation, the
original file's metadata byte length and a fixed 16 KiB ledger margin. It
then publishes different-path metadata through `CurrentUpdater`.
The allowance API explicitly says its observation does not reserve capacity
or authorize a later transaction. Actual preparation recomputes allowance
and optional admission counts only missing content-addressed resources.
Mandatory metadata, deduplication and the missing optional-resource byte
total therefore belong in the diagnostic record. These are **source-review
hypotheses**, not a demonstrated explanation of the retained failure.

Preserve these invariants: mandatory source values and scalars publish;
optional budget refusal produces `ProjectionOmission::Budget`; the original
projection remains readable; publication never writes the source tree.
Do not widen quota, retry the test to green, or change its assertions to
obtain a pass. The smallest useful diagnostic is an observed preparation
seam reporting actual mandatory allowance and optional missing-byte demand,
with the complete failure chain retained. An injected admission decision
would allow deterministic refusal without relying on a ledger-size margin.

The release-readiness correction temporarily quarantines this one test with
owner Tyler Jensen and expiry **23 November 2026**. The `#[ignore]` reason and
`docs/testing/ignore-triage.md` carry the same owner, expiry and unresolved
quota-margin assumption. Every assertion remains unchanged. This is an
explicit coverage exclusion, not a passing result or a confirmed production
defect. Resolve the observation/admission seam and remove the quarantine by
the expiry; retain the complete original diagnostics if the failure recurs.

The existing scoped diagnostic selection is:

```console
cargo test -p airk-kernel --no-default-features --lib current::indexing::tests::optional_projection_quota_refusal_still_publishes_original_values_and_scalars --locked -- --exact --ignored --nocapture
```

Run after investigation/observation changes are complete, under the primary
agent's single Cargo ownership. Retain the failing fixture and full output.

## Host-dependent fixtures

`CONTRIBUTING.md` supports `AIRK_PRIVATE_CENSUS_BASE` pointing to an isolated,
owner-controlled directory with inheritable owner permissions. Store
`service/collection/tests.rs::integrity_fixture` and
`tests/native_owner_recovery.rs::fixture` honor it. The actual-process
fixture creates children with prefix `airk-native-owner-`; Store uses
`airk-census-fixture-`. Neither automatically fixes an unsuitable parent.
Never modify a user's existing ACL or accept foreign write grants to get a
positive boundary classification.

The four CLI library fixtures named in the handoff use ordinary
`tempfile::tempdir()` (activation fixture, managed catalog fixture,
`service_runtime/registry_retry_tests.rs`, and standalone launch tests).
`AIRK_PRIVATE_CENSUS_BASE` alone does **not** redirect those fixtures.
Qualify them in a process whose temporary directory is an isolated owned
namespace, or add a narrowly owned fixture seam after confirming their
actual failure. Preserve the initial capability failures separately from
any qualified rerun. The handoff's callback/selection failures also require
their actual diagnostics; owner-only errors do not explain every failure.

Batch explicit test filters through Nextest after choosing the qualified
fixture root. Include both Store
failures, all four CLI cases and the three parent
`native_owner_recovery` tests. The fourth test in that integration binary
is a subprocess helper and is invoked only by its owning parent.

## Retained candidate identities

The following values are read from retained receipts/checksum lists, not
freshly rebuilt or independently rehashed in this investigation:

| Input | Recorded identity |
| --- | --- |
| `artifacts/ladder-2026-11-10/service-runtime/airk-build.json` | debug; source SHA-256 `a95fae98888e01a3efa68e0e9aa87066d45a690e9eb7afdefb11a79f0684dd12`, 1,132 source files |
| Runtime executable, Windows x64, schema 2, version 1.8.16 | SHA-256 `319fd1599f5325a4b7ab57e85c0af31aba73bc57f2ac354ddc91bc9ab1718cae` |
| `artifacts/ladder-2026-11-10/airk-ladder.vsix/publish/airk-1.8.16-win32-x64.vsix` | SHA-256 `29cd9c684549bccbd2509e853fcbb79b96a3c6d93f0af110ad8069e1219dce32` |
| `artifacts/ladder-2026-11-10/native/publish/airk-1.8.16-x86_64-pc-windows-msvc.zip` | SHA-256 `065337fac2bc2cb508d23a3c6504b99c096a0c985a0cdab2e69140d5202e08a1` |

The debug runtime records deterministic embedded-model smoke, 384 dimensions,
ORT 1.22.1 and no external ONNX Runtime imports. Its evidence remains bound
to these original inputs. A dependency/version change requires a newly
sealed runtime and package; these archives do not qualify that candidate.

Installed acceptance, optimized payloads, macOS, all-features Linux,
container and hosted CI remain outside this investigation. Current
authorization to continue the supplied readiness plan supersedes the old
handoff's statement that installed qualification lacks a go-ahead; it does
not turn the earlier unrun gates into passes.
