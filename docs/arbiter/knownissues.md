# Known issues in the 1.8.201 local candidate

Dated 15 November 2026. This is the local Windows candidate (service installer
and VSIX) built for manual testing on `feat/advanced-split`. It is **not** a
release, not qualified, and not an approval to publish. Version 1.9.0 is
reserved for the final release; the local candidate series starts at 1.8.201
and increments until a release candidate is published.

Every limit relaxed to reach this candidate is recorded, with its original value
and restore criterion, in [relaxations.md](relaxations.md). Statuses below are
what was observed in this build; "not run" means no evidence exists, not that a
check passed.

## 1. Arbiter models are not qualified (blocks all Arbiter claims)

- **What:** The release catalog (`crates/airk-retrieval/src/arbiter/release-catalog.json`)
  still has no qualified profiles. Arbiter Focus (Qwen3-Reranker-0.6B) and
  Arbiter Context Compact (Qwen3-1.7B) and Large (Qwen3-4B) cannot be enabled
  from a qualified profile in this candidate.
- **Why:** Qualification needs a fixed CAL100 capture (thirteen phases of up to
  30 minutes), two raw Context captures, native fitting, the ten composed
  scenarios and the held-out A6 goal comparisons. These take far longer than the
  three-hour delivery window and were not run.
- **Not run:** CAL100, raw Context captures, fit, resource and composed
  scenarios at the new limits, held-out A6, fitted-asset rights and target
  proofs.
- **Next:** Follow `docs/arbiter/whatsleft.md` section 4 after manual testing.

## 2. Optional model weights cannot be downloaded

- **What:** The app contains the on-demand acquisition, source-fallback and
  restart paths, but no verified public source exists for the optional model
  files. Provisioning reports the models as unavailable.
- **Why:** Seven of 23 raw assets match public Git/LFS metadata. Sixteen
  derived assets, including all six model graph/data files, have no verified
  public location, and the public ONNX variants differ from the tested exports.
  Hosting them needs a publication decision, verified rights and your explicit
  authorization. No namespace, URL or upload was invented or performed.
- **Next:** Decide and authorize hosting (and an independent backup, if any),
  stage the local parts, verify complete bytes through the native transport and
  bind the actual source maps into the final compiled catalog.

## 2a. Unqualified local-test profiles (pre-1.9.0 only)

- **What:** A compiled profile whose id starts with `unqualified-local-test` is
  admitted without a qualified target row or qualification digest, so Focus,
  Compact and Large can be downloaded and tried in this candidate. Status text
  reads "Unqualified local test profile (not release-qualified)."
- **Safety:** The rule is fail closed. It applies only when the package version
  parses to below 1.9.0, so a 1.9.0 or later build refuses these profiles, and
  release publication (`project()`) is unchanged and still demands qualification.
  CPU fit, assets, downloads, ABI and source policy digests are still checked.
- **Download failures:** A source-class failure or expired deadline now ends with
  a fixed message saying the download failed, nothing was enabled and to try again
  later. No URL or signed query string is shown. There is no automatic backup
  source in this candidate unless the profile lists one.
- **Why unqualified:** see section 1. Results from these models say nothing
  about retrieval quality, latency or memory at the raised limits.

## 3. Packaging omits the qualified native publication capture

- **What:** The VSIX packages without `fixtures/arbiter/publication_catalog.json`.
  `build-bootstrap.cjs` now requires that capture only for the 1.9.x-and-later
  series; the 1.8.x local series skips it.
- **Why:** Native code emits the capture only after all three profiles qualify
  (and refuses to emit it for an empty catalog). No genuine capture can exist
  yet, and none was fabricated.
- **Effect:** The resulting VSIX is a local test candidate and must not be
  published. The 1.9.0 gate is unchanged.

## 4. Limits were raised and no evidence exists at the new values

- Token ceilings were multiplied by four (2048 to 8192) and the shared read
  deadline from 30 s to 120 s. Full list: [relaxations.md](relaxations.md).
- All earlier resource and latency evidence (Compact p95 about 19.5 s, Large
  about 23.1 s, the 4/16/40 GiB reservations, Focus about 2.2 s) was measured at
  the old limits and **does not apply**. Hardware requirements shown in Help are
  preliminary.
- **Focus width is still 512.** The exported Focus graph has a static
  `[1, 512]` input, so it cannot use the larger ceiling without a new export.
- `Budget::default()` stays at 2048 tokens and 30 s because many retained
  fixtures embed it. Callers must request larger budgets explicitly.
- Client-side timeouts (VS Code host, MCP SDK, CLI transports) that are shorter
  than 120 s were not changed and can end a long read first.

## 5. The Answer timeout (R154) is not proven fixed

- The cause of canonical Answer call 0019 exceeding 30 s was never isolated.
  This build adds deadline clipping for the application and Focus metadata
  callbacks and relaxes the deadline to 120 s. That is a hypothesis-driven
  repair plus a larger allowance, not evidence of a fix.
- Call 0019 was not replayed and its partial capture is not CAL evidence.

## 6. Context prompt fit is unmeasured

- The Context prompt now interns repeated paths and hypothesis sets and parses
  SourceLiteral targets through their Rust owner; every card, read, excerpt,
  hypothesis, gap and reason is still present.
- The actual native tokenizer census needs the private tokenizer and retained
  requests, and was not run. Whether the prompt fits the old 2048 or the new
  8192 profile on the retained real request is unknown.
- The Context task-policy digest changed. Earlier Context calibration and
  profile evidence is not applicable to it by identity and was not relabelled.

## 7. Final gates not run or not complete

See the "Verification summary" below for what did run. Not run in this window:
the full workspace Nextest remainder (the earlier R144 run executed 294 of 4573
selected tests), restored routine-excluded campaigns, doctests on the final
source, the full CLI/session, MCP, Admin and Workbench real-transport parity
suite, the ignored vector-performance cases and the final publication-input
cases.

### Accepted test failures (counted as passing at your direction)

Two `airk-store` tests fail on this host with `native private boundary is
unavailable; strict reads required` / `native Windows boundary policy
unavailable`. They are not in the Arbiter paths this work changed, and the tests
were not edited, ignored or loosened.

- `service::collection::tests::native_original_data_root_handle_blocks_owned_path_replacement`
- `service::collection::tests::untouched_windows_bind_and_retained_producer_layout_qualifies_native_boundary`

I did not prove they also fail on the unmodified source (no baseline run). The
likely cause is the Windows private-boundary policy for this checkout's
location or account, not the candidate code. Investigate before release: run
both on a clean checkout under the same account, then under the sandbox
identity. Core/IPC/Supervisor/Store/Retrieval run: 1565 tests, 1563 passed,
2 failed, 25 skipped (`artifacts/v190/test-r1.log`).

Service/Kernel/MCP/Frontend/Airkctl/HTTPS/Inference/Host run
(`artifacts/v190/test-r2.log`): 1609 tests, 1574 passed, 35 failed, 64 skipped
(97.8%). Four of the 35 were caused by this candidate's relaxed limits and
version and were fixed by regenerating the native-emitted fixtures
(`help_content_manifest_matches_native_owners`, `ui_pilot_help_native_cases`,
`public_answer_sdk_preserves_advertised_schema_full_utf8_question_and_budget`,
and the allowance fixture's malformed-deadline boundary, now 120001). The
remaining 31 are accepted at the 90% threshold and were not edited:

- `airk-mcp arbiter_handler_tests::disabled_native_answer_expands_real_scoped_evidence_and_preserves_full_canonical_results`
  (retained canonical `airk_search` result differs; cause not investigated; may
  involve the relaxed limits or version in the retained record).
- `airk-service setup_role` (2): `the_record_step_creates_missing_data_and_cache_locations`
  (`location_unavailable` on a temp path) and
  `transaction::tests::os_management_refuses_unsettled_and_wrong_operation_or_stage`.
- `airkctl` (about 26): the `agents::config::*` retained-writer/eligibility
  tests (git and ignore-file controls), `cli::session::tests::real_command_invoker_carries_exact_agent_entries_through_the_session`,
  `cli::tests::{a_missing_installation_or_service_exits_three,set_create_indexes_are_checked_before_the_service}`,
  `cli::tests::version_answers_locally_in_both_formats` (actual capabilities list
  contains `admin-management/1`, which the test's expected list does not; not
  caused by this work), three `agent_entries` tests and
  `error_outcomes::service_refusal_is_backtrace_independent_in_the_real_process`.
  Most look host-environment related (git/safe.directory, temp locations,
  process spawn); not baselined.
- `airk-kernel` (2): `source_watch::tests::native_nested_events_are_acknowledged_without_changing_sources_and_missing_roots_are_independent`
  (15 s) and `integration persistent::source_measurements_are_small_authenticated_values_independent_of_graph_bytes`.
- A later targeted run of the MCP help/UI-pilot/arbiter tests: 27 run, 26 passed,
  1 failed (the `disabled_native_answer` case above).

None of these were investigated against an unmodified baseline. Treat the list
as the first thing to triage before the 1.9.0 release gates.

## 8. Other platforms were not built

macOS (Mac Neo has 8 GiB and older isolated evidence only) and Linux were not
rebuilt or tested. Nothing in this candidate supports a cross-platform claim.

## Operational notes

- `cargo clean` removed the default `target/local-vector-model` bundled-model
  copy. Builds now set `AIRK_BUNDLED_MODEL_DIR` to
  `artifacts/advanced-split/bundled-model-v3` (exact U8/S8 v3 models).
- The sandbox-owned `artifacts/advanced-split/models/selected-native-goal-driver-r1`
  directory stayed unreadable (ownership change needs an elevated shell), so the
  Context prompt was rewritten from source rather than from the private
  proposals.

<!-- Verification summary and artifact inventory are appended after the build. -->
