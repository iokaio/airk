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

## 2. Optional models download from one GitHub prerelease with no backup

- **What:** The three models are 44 parts (24,130,010,313 bytes) in the public
  prerelease [`models-1.8.201`](https://github.com/iokaio/airk/releases/tag/models-1.8.201).
  The app downloads them on explicit Provision. There is **no backup source**: if
  GitHub is unreachable or returns an unexpected response, Provision fails with
  a message that says the download failed and to try again later, and nothing is
  enabled. Already-downloaded verified whole assets are kept. Partially
  downloaded parts of an unfinished asset are kept on disk but are not resumed;
  the next Provision restarts that asset. Use Remove to reclaim that space
  before retrying (the failure message says so from 1.8.203).
- **Part deadline (1.8.203):** each part's deadline is 900 s or enough time
  for the part at 256 KiB/s, whichever is longer (4096 s for a 1 GiB part).
  Stalls are still bounded by the 30 s idle limit.
- **Verified:** GitHub's computed digest and size match the staged manifest for
  all 44 parts, and one 1 GiB part (`arbiter-focus-qwen3-reranker-06b.model.onnx.data.part01`)
  was downloaded anonymously and its SHA-256 matched.
- **Not verified:** A full download-and-load of all three models through the
  app (Provision, join, ONNX load, ABI) has not been run. Load behavior,
  latency and memory with these exports in the service are unknown.
- **Scale risk:** One host serves every download. Large is about 16 GB per user.
  GitHub does not guarantee release-asset bandwidth and may throttle heavy use.
  An independent mirror would need a separate hosting decision.
- **Decision history:** Hugging Face was considered and rejected because the
  tested derived exports (W8 Focus graph, FP32 Compact/Large with AIRK's readout)
  are not hosted there and public ONNX variants differ from them. Seven of 23
  raw assets match public Git/LFS metadata; the rest exist only as these parts.

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

## Conditions for replacing the local-test models

The three models in the 1.8.201 candidate (Focus Qwen3-Reranker-0.6B, Compact
Qwen3-1.7B, Large Qwen3-4B) are `unqualified-local-test` profiles whose files
are parts of the GitHub release `models-1.8.201`. They are a manual-testing aid.
A replacement is a new profile with a new manifest digest and a new release
tag. **A published release is never edited or overwritten**, because earlier app
versions may still point at its URLs. The old parts are removed only after every
supported app version that references them is retired.

### Required before any replacement is accepted

1. **Qualification.** All of these pass on actual evidence, with no tuning on
   confirmation data:
   - Fixed CAL100 capture: thirteen ordered phases, each with its own natural
     ownership closure.
   - Two fixed raw Context captures.
   - Native fit with a safe positive generic Read margin and zero UNKNOWN. If
     the fit is unsupported, report that and do not create a qualified profile.
   - Resource checks and the ten composed scenarios, including MiniLM
     coexistence, joint memory, cold/warm lifetime, disable/drain, settlement and
     real set-membership revocation.
   - Held-out A6 goal comparisons with Focus enabled throughout, against the
     ordinary follow-up and deterministic-packet baselines. Existing floors are
     .75 completion, .02 gain and .10 turn reduction.
2. **Limits restored or re-qualified.** The relaxed token and deadline ceilings
   in [relaxations.md](relaxations.md) are restored, or each model is qualified
   at the values it ships with. Evidence measured at the old limits is not
   relabelled.
3. **Rights and provenance.** Source revisions, licenses, notices, derivation
   records and target proofs are complete for the exact bytes being published.
4. **Native catalog.** The qualified profiles are produced by the native final
   promoter, the catalog is compiled, and the native publication capture
   (`fixtures/arbiter/publication_catalog.json`) is emitted from it.

### What forces a replacement

| Trigger | Replacement |
|---|---|
| A profile qualifies | A qualified profile replaces its local-test one (new manifest, new release tag). The 1.9.0 series refuses `unqualified-local-test` profiles outright. |
| The exported graph, weights, tokenizer or ABI change (for example a Compact W8 export that now meets its numerical tolerance, or a wider Focus graph than 512 tokens) | New profile, new parts. |
| The Context prompt or task policy changes again (it already changed once for the compact prompt) | New task-policy digest and profile metadata; the weights may be reused only if their digests are unchanged. |
| An upstream revision is withdrawn, relicensed or found defective | New export from a reviewed revision. |
| A part fails verification, or the release assets are removed or throttled | Re-host under a new tag (and an independent backup if one is approved). |
| A hardware requirement is measured and differs from the shipped profile | New profile with updated `cpu_fit` and reservations. |

### How a replacement is delivered

New parts go to a new `models-<series>` release; the new profile and source map
are compiled into the next candidate; unchanged assets keep their digests and
URLs and need no re-upload. A candidate whose profile is replaced is built under
a new version number, and the test report records the old and new manifest
digests side by side.

Local-test catalog run (three Arbiter profiles embedded; `artifacts/v190/test-r5.log`,
`test-r6.log`): airk-core/ipc/supervisor/store/retrieval/service/kernel/mcp/frontend/
airkctl/https/inference/host ran 3184 tests, 3149 passed, 35 failed (98.9%).
airk-cli/installer/endpoint/catalog-control/ingest/process ran 1434 tests, 1417
passed, 17 failed (98.8%). Accepted at the 90% threshold, not edited: the 31
failures listed above plus
`airk-supervisor project_worker::admin::tests::actual_singleton_focus_worker_refuses_foreign_clock_and_expired_attempt_before_storage`
(flaky: passed 2 of 3 reruns, variant 3, `remaining_ns = 0`); and in airk-cli the
three `native_owner_recovery` cases, `service_only::no_arguments_without_airkctl_reports_it_and_starts_nothing`,
two `service_runtime` cases (`native frame read deadline exceeded`), the
`tools` parity cases (`airkctl_*_matches_airk`, `control_tool_calls_answer_what_the_cli_printed`,
`airkctl_agents_connect_list_repair_remove_on_a_live_service`,
`airkctl_reads_print_what_airk_printed`, `airkctl_resolves_the_project_its_folder_names`,
`airkctl_session_opens_and_closes_its_window_on_a_live_service`; the catalog case
shows the comparison binary is an older `airk` with ORT 1.22.1 / `minilm-l6-v2-u8-v1`,
so these compare against a stale pair, not this build) and two airk-installer cases
(`replacement::tests::original_control_is_reauthenticated_even_when_custom_storage_is_already_aside`,
`service_manager::direct_tests::split_mac_job_requires_exact_static_role_and_placed_bundle`).

## 8. Other platforms were not built

macOS (Mac Neo has 8 GiB and older isolated evidence only) and Linux were not
rebuilt or tested. Nothing in this candidate supports a cross-platform claim.

## 9. Explorer Service view and Arbiter controls are not user friendly

Reported by the user on 1.8.205 (Explorer > Service, "Native management").
The layout makes no sense to a user and offers no useful features in a usable
form:

- Two rows of tabs (Service / Storage / Recovery, then Resources / TLS and trust
  / Start at sign-in / Restart or stop) with no explanation of what each does
  or which one to use.
- "Resources" shows only a bare **Load** button; nothing is shown until it is
  pressed, and nothing explains what it loads.
- "Optional local Arbiter" is a collapsed block of internal wording ("Read
  native stage status", "No current native stage status has been read. No
  model, enrollment, hardware fit or readiness is assumed.", "Initialize
  separate disabled stages"). It does not say whether the Arbiter is on, which
  models are installed, or how to turn Focus or Context on.
- "Native management · 69cc6133" and "Close management session" expose
  implementation details instead of the user's task.

Needs a user-task redesign (status first, plain actions such as "Download and
turn on Arbiter Focus"); not addressed in this candidate.

## 10. Explorer navigation rail: duplicate icons and the "Explore evidence" group

Reported by the user on 1.8.205 (Explorer webview, `packages/vscode/src/explorerWebview/app.tsx`
`Rail`, icons in `components.tsx` `ICONS`).

**What is wrong**

- When the rail is collapsed to icons, several destinations share one icon, so
  you cannot tell what you are looking at:
  - **Activity, Agents and Service** all show the same sliders icon. The rail
    picks the icon from the route's workspace (`manage`), not from the
    destination.
  - **Indexes** (workspace `indexing`) has no icon of its own and shows the
    same grid as **Overview**.
- The collapsible **Explore evidence** group heading breaks the collapsed rail:
  its label wraps into "Explore / evidenc" under a bare disclosure triangle,
  and the triangle and partial text look like a rendering error.
- The group label does not say what its five items (Code, Documents, Evidence,
  Review, Strategies) have in common in terms a user would recognize, and the
  group opens only when one of them is current, so the items are easy to miss.

**Proposed improvements**

1. **One unique icon per destination, keyed by destination id** (`overview`,
   `search`, `indexes`, `activity`, `agents`, `service`, `help`, `code`, `docs`,
   `evidence`, `review`, `strategies`), not by workspace. Suggested glyphs:
   Overview — dashboard tiles; Search — magnifier; Indexes — stacked database
   cylinders; Activity — pulse/clock line; Agents — plug or robot head;
   Service — power/server; Help — question mark; Code — angle brackets;
   Documents — page; Evidence — shield with check; Review — checklist;
   Strategies — branching flow. Add a test that every rail destination maps to
   a distinct icon so a new destination cannot silently reuse one.
2. **Remove the "Explore evidence" group heading.** Show all twelve
   destinations as one flat list, split into two visually separated sections
   by a thin divider (no text label). Top: the workspace tasks (Overview,
   Search, Indexes, Activity, Agents, Service). Bottom: reading saved evidence
   (Code, Documents, Evidence, Review, Strategies). Put Help last, pinned to
   the bottom of the rail.
3. **Collapsed rail = icons only, with tooltips.** Each button keeps its
   `title`/`aria-label` (already present) so hovering or a screen reader names
   it. No text wraps in the collapsed state, and the current destination keeps
   its highlight.
4. **Name the reading section in the expanded rail only, if at all**, with a
   short caption that cannot wrap (for example "Read" or "Saved index"), shown
   only when labels are visible and hidden when collapsed.
5. **Page titles match the rail labels**, so the heading confirms which
   destination is open ("AIRK WORKSPACE · Agents" today; keep the destination
   name prominent and drop the generic caption).

Not addressed in this candidate.

## 11. Agent access without a token (U110): follow-ups

U110 (15 November 2026, user decision) removed the MCP token: `/mcp` and the
`/api/v1` tool routes take no credential, scope is a tool parameter
(`index_ids`, `index_set_ids` or `workspace`, else `scope_required`), and the
Explorer's applied scope is stored as a persisted workspace default. This
resolves the earlier HTTP 401 (`credential_refused` / `credential_required`)
problem, where Explorer snippets carried a `<connection-id>` placeholder.

- **Accepted exposure:** any local process of any OS user that can reach
  127.0.0.1 can read every index. HTTPS (AIRK CA), Host and `Origin` checks
  remain, so browser pages cannot reach it through DNS rebinding.
- **MCP roots are deprecated upstream:** the automatic `workspace` fill uses
  `roots/list`, which SEP-2577 deprecates. Clients on newer MCP protocol
  versions may not offer roots; the explicit `workspace` argument is the
  supported path.
- **Unused credential machinery:** credential issuance, the ledger, `agents
  credential/info/verify/launch/default/revoke` and the grant-bound HTTPS code
  paths stay compiled but unused (marked legacy in CLI help). Their removal is
  a separate, evidence-backed step.
- **Harnesses still enrolling credentials:** `check-http-port-relocation.py`,
  `check-issuer-rotation.py`, `benchmark-service.py`,
  `benchmark-real-corpora.py`, `benchmark-mcp-investigation.py`,
  `check-retrieval-quality.py`, `check-usage-inputs.py`,
  `check-runtime-upgrade.py`, `check-service-install.py`, `check-container.py`
  and `setup-local-test.py` still enroll a connection. The shared HTTPS client
  ignores the token and fills in the enrolled index as scope, so they still
  run, but they should move to the tokenless endpoint read.
- **Terse status elision on the open path:** terse MCP sessions used to omit
  an unchanged `freshness` on repeated status calls. Open reads build a fresh
  backend per call and repeat it. Pre-U110 open connections behaved the same;
  U110 makes it the only HTTPS path. `airk-cli mcp
  default_terse_https_supports_full_and_requested_status_without_changing_revision`
  fails on this assertion (not weakened).
- **Multi-index capabilities plan over the open path:** a two-member
  `parse` plan with a `capabilities` step is refused
  `combined_read_unavailable` identically through MCP and the pipe; the
  one-member plan passes. Grant-bound sessions served it before U110. Cause
  not investigated (the error chain goes to the service's private log).
  `airk-cli combined_reads capabilities_plans_keep_complete_native_cli_and_mcp_results_above_read_frame`
  fails here.
- **`read_scope` on multi-index results:** a delivered multi-index body is bound
  byte for byte by its delivery receipt, so it carries no `read_scope`; its
  `selection.scope_identity` names the open scope. Single-index results carry
  `read_scope`.
- **Unrelated failure seen in the U110 runs:** `airk-cli service_runtime
  installed_log_binding_places_the_ring_outside_control_and_missing_ownership_never_falls_back`
  (`index_setup_paths_overlap`) exercises index-setup drafts, not U110 code; not
  run on an unmodified tree. `an_installed_first_start_creates_the_index_catalog_and_serves_admin`
  failed once on a native frame deadline and passed on rerun.
- **Admin UI-pilot fixture drift (not U110):** `airk-https
  admin_plane::tests::ui_pilot_cases::ui_pilot_i3_semantic_cases` reports
  `usage_empty` missing from `fixtures/split/ui_pilot` (the native side emits
  the case, the retained fixture lacks it). Not regenerated without review.
- **Pre-existing test failures found during U110:** `airk-mcp`
  `arbiter_handler_tests::policy_withdrawal_during_joint_wait_refuses_stale_answer_without_replay_or_primitive_loss`
  and `arbiter_handler_tests::optional_callback_failure_keeps_canonical_evidence_but_typed_scope_refusal_stops_delivery`
  fail identically on the unmodified `98c368f4` tree (they passed in the
  earlier `test-r2`/`test-r5` runs; a commit between those runs and
  `98c368f4` broke them). Not investigated.

## 12. Explorer Agents tab: Connect is refused and there is no manual fallback

Reported by the user on 1.8.206 (Explorer > Agents, repository on `D:`).
History: before U110, the Codex and Claude Code entries copied from the Agents
tab had a literal `<connection-id>` placeholder, so `/mcp` returned 401
`credential_required`. U110 fixed that. On 1.8.206, **Connect** now fails
every time for this repository with:

> Automatic config editing was refused; apply the edit manually: parent grants
> foreign mutation

Emptying or deleting `.mcp.json` does not help, because the check is on the
folder, not the file.

**Cause.** `airkctl` reviews the target before an atomic replace with a
retained recovery copy
(`crates/airkctl/src/agents/config/retention.rs`, `review`). It calls
`airk_ipc::endpoint::require_protected_parent_handle` on the containing folder
(`crates/airk-ipc/src/endpoint/handle_policy.rs`). That check refuses if any
other account has write access to the folder. `D:\code\Github\airk` inherits
Modify for `Authenticated Users`, which is the Windows default on a secondary
drive. It also grants Modify to `CodexSandboxUsers` and to two Codex sandbox
SIDs. So almost every repository outside the user profile is refused. This is
the same failure behind the roughly 26 `airkctl` `agents::config::*` test
failures listed above.

**The Explorer leaves the user stuck.** The refusal is shown as raw text.
Nothing on the page shows what to edit by hand:

- **Copy** copies only the endpoint URL
  (`packages/vscode/src/explorerWebview/agents.tsx`, the endpoint card). It
  does not copy a config entry.
- The ready-to-paste entries exist only on AIRK Admin > Agents
  (`packages/admin/src/agents.tsx`, `ManualCard`). The Explorer mentions them
  only in small print inside the collapsed **Details** section.
- "parent grants foreign mutation" is never explained, and the target file is
  never named.

The workaround used on 1.8.206 was to hand-write the entry. For Claude Code,
`.mcp.json` gets the URL and the `X-AIRK-Response-Mode` header. For Codex,
`.codex\config.toml` gets `[mcp_servers.airk]` with `url` and
`[mcp_servers.airk.http_headers]`. The old token and helper fields are removed.

**Proposed fix (1.8.207):**

1. **Native writer: no hard refusal for credential-free entries.** Since U110,
   AIRK entries hold no secret, only a URL and a header. When the parent folder
   grants write access to other accounts, the writer would still write the
   entry, with these safeguards:
   - It keeps the same-parent identity, basename and size checks.
   - It still preserves the user's other servers.
   - It reports a typed warning, for example
     `recovery_copy_unprotected`, that names the folder and the
     principals that have write access.

   A hard refusal stays in place where it is still needed: a legacy entry that
   carries a credential, or a target that is not a regular file. This changes
   the agent-config writer contract (I2 / `airkctl` `src/contract/CONTRACT.md`).
   It needs a recorded decision and updated tests. The `agents::config::*`
   failures should then pass without any assertion being weakened.
2. **One canonical manual entry, from Rust.** `airkctl agents connect` (and
   the `agents.read` / `agents.connect` views) would return a `manual` object
   for the selected agent and folder with three fields:
   - `file`: `.mcp.json`, `.codex\config.toml` or `.vscode\mcp.json`;
   - `format`: `json` or `toml`;
   - `text`: the exact entry, filled in with the live endpoint URL.

   AIRK Admin's `claudeJson`/`codexToml` builders would render this text
   instead of building it in TypeScript. Today they are handwritten mirrors of
   the writer, which the repository rules prohibit.
3. **Explorer "Set up manually" section** under **Connect**, always available.
   It shows the target file path, the entry for the selected agent, and a
   **Copy** button.
4. **Refusals in plain language.** When **Connect** is refused, the Explorer
   opens that section and replaces the raw error with a plain-language message,
   for example: "AIRK can't safely edit files in `D:\…` because other accounts
   can change files there. Paste this entry into `D:\…\.mcp.json` instead." The
   raw error stays available under **Details**. Every refusal from
   `retention.rs` needs a short message like this. The writer would return a
   typed refusal code instead of one prefixed string, so the Explorer and Admin
   don't have to parse message text.
5. **Endpoint port changes.** The manual entry carries the current port. Next
   to it, the section says that if the service has to pick a new port, the
   entry must be updated. It points to **Copy** or `airkctl agents endpoint`.

Checks:

- Rust: scoped `airkctl` dev-check and the `agents::config::*` regressions,
  run in a folder that grants `Authenticated Users` Modify.
- Explorer and Admin: typecheck and behaviour tests.
- Parity: the CLI, Explorer and Admin show the same manual entry for each
  agent.

Not addressed in this candidate.

## Operational notes

- `AGENTS.md` and `CLAUDE.md` intentionally differ: `AGENTS.md` carries a rule
  against using a Codex sandbox user account, which does not apply to Claude
  Code. `scripts/check-repo.ps1` enforces byte equality, so it will fail that
  one check until the check or the files are reconciled. This is a deliberate
  divergence, not a defect in the candidate.
- `cargo clean` removed the default `target/local-vector-model` bundled-model
  copy. Builds now set `AIRK_BUNDLED_MODEL_DIR` to
  `artifacts/advanced-split/bundled-model-v3` (exact U8/S8 v3 models).
- The sandbox-owned `artifacts/advanced-split/models/selected-native-goal-driver-r1`
  directory stayed unreadable (ownership change needs an elevated shell), so the
  Context prompt was rewritten from source rather than from the private
  proposals.

## Verification summary and candidate inventory (1.8.202)

Built from commit `a650b156` (the tree was dirty only because of the user's
uncommitted `AGENTS.md` line).

| File | SHA-256 | Bytes | Signing |
|---|---|---|---|
| `artifacts/local-service-installer/20261115-150454/airk-service-1.8.202-x86_64-pc-windows-msvc-installer.exe` | `84c2aae5a26a2ced5cd2be2f42610d110dd22596184dc7d427d525a331b43fd2` | 75,672,608 | Authenticode valid, Ioka LLC |
| `artifacts/local-vsix/20261115-151937/vsix/publish/airk-1.8.202.vsix` | `e697ac85c274df345c3e003260b431e271e8eeedafcc4fec2c055227ff9b7354` | 1,700,297 | none (Marketplace signing happens only at publication) |
| Model parts: prerelease `models-1.8.201`, 44 files | per file in the release notes | 24,130,010,313 | none |

Ran: whole-workspace Clippy (clean, `-D warnings`), the test batches described
above, a signed release-local build, and `check-service-clients.py --files 16
--model` against the 1.8.202 runtime pair (exit 0, 120 s, source bytes preserved,
managed stop complete; report `artifacts/v190/check-service-clients-1.8.202.json`).

Not run: installing the installer or VSIX, upgrade/repair/uninstall journeys,
silent mode, an end-to-end Provision of the three models, loading them in the
service, doctests, the full Nextest remainder, real-transport parity suites,
macOS and Linux.


## Resolved execution hold — 17 November 2026: CUDA compiler child lifetime

The current checkpointed Flash section has completed its compiler work, but its
owned process tree remains open. Read-only observations at09:37Z and09:43Z find
only the pinned MSVC VCTIP telemetry child (PID42204); no nvcc/cicc/ptxas/cl/Ninja
process remains. This census is not original parent HANDLE exit proof and does
not qualify the section. The original cutoff and complete-tree requirement stay
unchanged; no helper exemption, forced-success cleanup, registry/IDE policy or
tool-binary mutation is introduced. The prior R12 cutoff failure remains failed.

The bounded investigation finds no verified invocation-local control for the
installed MSVC toolset. Embedded upload/timeout options do not establish a
supported graceful shutdown of the running child. This blocks a successful
CUDA producer/composition and actual GPU model execution while it persists.
Preserve warm outputs and continue independent product, metadata and platform
work. Current evidence: artifacts/v190/final-plan/exec-20261116T052942Z/
gpu-flash-lifetime-observation-r1/observation.json; the actual section/outer
receipts remain under the approved human-owned cu8-checkpoint roots.

### Resolution of the first Flash lifetime hold — 17 November 2026

The child settled naturally within the original cutoff; Flash completes09:45Z
with48 verified outputs, no timeout/force and finalJob0. This was observed10:07Z;
the receipt SHA is12e9054e6a39f96585cb49ec33c3000ce47998ab0c50275a78a7e34332affb8d.
No registry/tool/ACL change or descendant exemption was used. LLM then passes
43 outputs (da0eda4c) and final provider closure passes1860 outputs (5bf52327),
both naturally. Final composition passes with explicit checkpointed-warm scope
(f9fac8eb). The original R12 failure remains failed; model qualification is pending.

## Open execution input — 17 November 2026: protected GPU proof metadata closure

First actual native assembly refuses GPU custody, exits101 naturally, and leaves
an empty owned Job. The error is masked as an owner mismatch by the parent-policy
fallback; no unsupported runtime or model is admitted. Exact source tracing finds
the historical warm baseline under the writable D: campaign directory missing
from the9-entry protected metadata-copy map. The unchanged Source correctly refuses
that original parent. Complete the exact read-closure audit and use the existing
Rust-owned metadata-copy mechanism in fresh inputs; preserve prior native outputs
and this failed attempt. No ACL/owner change or loader/proof-policy broadening is
used. Actual receipt: approved human C anchor/gpu-first-fp32-assembly-owned-r1/
execution.json, SHA70b85630acc8b3a2e9e6b9b535b164cc09b578c806ff81514a6dfc6e04ba065d.
First FP32 neural execution remains pending while this input correction proceeds.

### Metadata assembly resolution — 17 November 2026

R2 closes the valid warm-baseline custody refusal through the existing protected
copy mechanism, without security changes. It then refuses checkpoint composition
timing (receiptcaa02366): overall first-attempt04:08:52Z was incorrectly compared
to later R12 start05:27:32Z. Native now authenticates the original first-start
FilePin and retains overall <=R12 <=first section, all original clocks/limits and
settlement requirements. Scoped Clippy and81/81 regressions pass, including actual
unaltered raw timing arithmetic. No receipt, threshold or clock is reset.

Fresh corrected-image R3 assembly passes12.868392s, PID3848 exit0 naturally,
original Job0/no drift; receiptb0d3dbbd. Native manifestaf6d1ec5 authenticates13 DLLs
and static buildf9fac8eb, retaining fits=[] and unapproved status. This resolves
metadata assembly only. First FP32 inference and GPU qualification remain open.
Automatic approval review separately blocks new component-runner file creation;
the clarification request remains pending.
