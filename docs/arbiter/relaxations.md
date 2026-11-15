# Relaxed limits in the 1.8.201 local candidate

Dated 15 November 2026. At the user's direction, tunable token and deadline
ceilings were multiplied by four so that a local Windows candidate can be built,
tested and installed. **This is not qualification.** No Focus or Context profile
is qualified at either the old or the new limits, and all earlier resource,
latency and numeric evidence applies only to the old limits. Restore or
re-qualify these values after manual testing; the restore order is at the end.

Line numbers drift; each row names the exact expression to search for.

## Not relaxed

Source and collection isolation, authority/admission checks, digest and
identity checks, exact-byte download verification, no replay of uncertain work,
cancellation/custody stops, read-surface parity, installer payload checks and
signing were not touched. The qualification floors (T2048/N8, 30 s p95, resource
and numeric rows in `airk-inference` `pool/arbiter/qualification*` and
`airk-service` `final_distribution*`) and `Budget::default()` (2048 tokens,
30000 ms) were deliberately left at their old values; see "Deliberately kept".

## Token ceilings (old 2048 → 8192)

| Location (search for) | Old → new | Unblocks / effect | Restore criterion |
|---|---|---|---|
| `crates/airk-core/src/arbiter/mod.rs` `CompilerProfile::validate` `(1..=2048).contains(&self.max_tokens)` | 2048 → 8192 | Compiler profile may declare up to 8192 tokens | Evidence that the exported graph and tokenizer work at the width, plus a qualified profile |
| same file, `max_tokens` check in the Context profile validation (`(1..=64).contains(&self.max_candidates)`) | 2048 → 8192 | Context profile width ceiling | same |
| same file, `Budget::validate` `(1..=2048).contains(&self.input_tokens)` | 2048 → 8192 | Callers may request an 8192-token shared budget | Measured latency/memory at that budget |
| `crates/airk-core/src/arbiter/focus.rs` `FocusProfileLimits::validate` (`pair_input_tokens`, `max_input_tokens`) | 2048 → 8192 | Wider Focus shared allocation | Focus re-export and qualification |
| same file, `focus_token_allowance` `local_parent_tokens` range | 2048 → 8192 | Local parent allocation | same |
| same file, `validation_budget()` `input_tokens` | 2048 → 8192 | Structural outer-delivery ceiling only; never a dispatch allocation | same |
| `crates/airk-core/src/arbiter/context_action.rs` `reserved_input_tokens` range | 2048 → 8192 | Reserved Context tokens | same |
| `crates/airk-core/src/combined_context.rs` `resources.input_tokens` range, and the inline `Budget { input_tokens: 2048, … }` used for packet delivery bindings | 2048 → 8192 | Per-partition allowance and delivery validation ceiling | same |
| `crates/airk-core/src/native_focus.rs`, `native_context_action.rs` `remaining_input_tokens` range | 2048 → 8192 | Native request remaining tokens | same |
| `crates/airk-service/src/service_runtime/arbiter/joint_ledger.rs` `MAX_INPUT_TOKENS` | 2048 → 8192 | One-use ledger reservations | same |
| `.../arbiter/scoring.rs` `request.remaining_input_tokens <= 2048` | 2048 → 8192 | Next-read request admission | same |
| `.../arbiter/final_distribution/composed_validation.rs` `event.binding.tokens <= 2048` and `budget.input_tokens <= 2048` | 2048 → 8192 | Composed-validation ceilings | same |
| `crates/airk-service/src/service_runtime/arbiter.rs` `max_input_tokens: 2048` (two places) | 2048 → 8192 | Focus shared ceiling given to `FocusProfileLimits` | same |
| `crates/airk-inference/src/inference_engine/pool/arbiter/execution_observer.rs` `(1..=2048).contains(&self.binding.tokens)` | 2048 → 8192 | Dispatch observer binding | same |
| `crates/airk-retrieval/src/arbiter/compiler/focus.rs` `validate_profile` `(1..=2048)` | 2048 → 8192 | Focus compiler profile ceiling | same |

## Deadline ceilings (old 30 s → 120 s)

| Location (search for) | Old → new | Unblocks / effect | Restore criterion |
|---|---|---|---|
| `crates/airk-core/src/read_limits/mod.rs` `MAX_DEADLINE_MS` | 30_000 → 120_000 | Shared by saved-byte verification, combined reads, Workbench and the MCP schemas | End-to-end Answer at 30 s on an optimized pair, or a reasoned new bound |
| `crates/airk-core/src/arbiter/mod.rs` `Budget::validate` deadline range | 30_000 → 120_000 | Request budget deadline | same |
| `crates/airk-core/src/arbiter/focus.rs` `validation_budget()` deadline; `ordinary_search_allocation` `.min(30_000)` | 30_000 → 120_000 | Focus allocation clamps to the original attempt | same |
| `crates/airk-core/src/combined_reads.rs` `(1..=30_000).contains(&self.deadline_ms)` (message "1..120000 ms"); `combined_reads/read_retry.rs` allocation range | 30_000 → 120_000 | Combined read deadline | same |
| `crates/airk-core/src/combined_context.rs` inline validation `Budget` `deadline_ms` | 30000 → 120000 | Delivery binding ceiling | same |
| `crates/airk-core/src/native_focus.rs`, `native_context_action.rs` `remaining_ms` range | 30_000 → 120_000 | Native requests | same |
| `crates/airk-ipc/src/control.rs` `arbiter_remaining` `(1..=30_000)` and `.min(Duration::from_secs(30))` | 30 s → 120 s | Original-clock intersection for native Arbiter exchanges | same |
| `crates/airk-mcp/src/response.rs` `verification_deadline_schema` and its doc comment | max 30000 → 120000 | MCP tool schema | same; regenerate help |
| `crates/airk-mcp/src/lib.rs` Answer `budget.deadline_ms.min(30_000)` | 30_000 → 120_000 | Answer's own serialized ceiling (not the generic 125 s read maximum) | same |
| `crates/airk-kernel/src/workbench.rs` `verification_deadline_ms` schema | max 30000 → 120000 | Workbench plan schema | same |
| `crates/airk-frontend/src/combined_frontend.rs` total `SharedBudgets` `deadline_ms`, `remaining_deadline_ms.unwrap_or(…).clamp(1, …)`, `observation_budget` | 30000 → 120000 | Combined default/ceiling | same |
| `crates/airkctl/src/evidence.rs` `verification_deadline_ms` range | 30000 → 120000 | CLI flag range | same |
| `crates/airk-https/src/runtime_http_commands.rs` HTTP read context | 30 s → 120 s | HTTPS reads | same |
| `crates/airk-supervisor/src/runtime_metadata/arbiter.rs` four `.min(30_000…)` clamps | 30 s → 120 s | Focus/joint/next-read callback remaining time | same |
| `crates/airk-kernel/src/arbiter.rs`, `arbiter/joint.rs` `.min(30_000)` (three places) | 30 s → 120 s | Kernel native remaining time | same |
| `crates/airk-service/src/service_runtime/arbiter/scoring.rs` `remaining_ms <= 30_000` and `.min(30_000)`; `joint_ledger.rs` `budget_window_ns` `.min(30_000)` | 30 s → 120 s | Service admission and ledger window | same |
| `crates/airk-mcp/src/arbiter_calibration_capture.rs` collector deadline and `remaining_ms`/`remaining_input_tokens` | 30 s → 120 s, 2048 → 8192 | Private capture collector | same |
| Tests: `crates/airk-cli/tests/combined_reads.rs` three `30000` ceiling assertions | 30000 → 120000 | Same ceilings, still upper bounds | Restore with the production values |

## Download part deadline (1.8.203)

The per-part download deadline was a fixed 900 s. With 1 GiB catalog parts that
required about 1.19 MiB/s sustained, so slower connections could never finish
Provision. It is now 900 s or `ceil(part bytes / 256 KiB/s)` seconds, whichever
is longer (`crates/airk-service/src/service_runtime/arbiter/download.rs`
`PartBudget::for_part`). The budget is still per part, never reset across
sources; the 30 s response/idle limit and four redirects are unchanged. To
restore: re-split the catalog into parts that fit 900 s at a qualified rate.

## Deliberately kept (not relaxed)

- `Budget::default()` stays 2048 tokens / 30000 ms. Many retained fixtures embed
  it. Raising it requires regenerating `fixtures/split/arbiter/*.json` and
  `fixtures/split/i1/combined_context_allowance.json`.
- The 5 s I1 metadata callback timeout,
  `MAX_GRANT_TTL_MS`, window TTL, OCR, installer and unrelated 30 s timers.
- Qualification floors and evidence validators (`qualification.rs`,
  `qualification/context.rs`, `final_distribution.rs`): `T2048/N8`, 30 s p95,
  `unchanged_floors`, numeric rows `(17,3),(128,8),(2048,8)`. These describe the
  earlier captures. Qualification at the new widths needs a new reviewed policy.
- Profile-producing widths (`runtime/qualification.rs` `max_tokens: if focus
  { 512 } else { 2048 }`, `publication.rs` test profiles) and the prompt
  excerpt byte allowance in `tasks.rs`.
- **Focus width 512 cannot be multiplied.** `runtime/input.rs`
  `selected_graph_shape` requires the Focus graph input to be statically
  `[1, max_tokens]`; the exported graph is 512 wide. Only a new export can widen
  it. Context graphs use dynamic `[1, -1]` token axes, so they can run wider, but
  memory and latency at 8192 are unmeasured.

## Tests changed

New literal boundary tests (expected numbers are literals, not read from the
constants): `read_limits/tests.rs` (120_000), `arbiter/tests.rs`
(`request_budget_ceilings_are_literal_8192_tokens_and_120000_ms`),
`arbiter/focus/tests.rs` (`focus_profile_token_ceilings_are_literal_8192`,
`8193` refused), `native_focus/tests.rs` (120_000/120_001 and 8192/8193),
`native_context_action/tests.rs` (8193 refused), `airk-ipc` control tests
(120_001 refused, 120_000 accepted). The Context request "maxima" case now uses
8192/120_000. None of these were compiled or run when written.

## Packaging and version-series relaxations (1.8.201 series)

| Location (search for) | Old → new | Unblocks / effect | Restore criterion |
|---|---|---|---|
| `packages/vscode/scripts/build-bootstrap.cjs` `verifyReleaseBundle` / `finalSeries()` | qualified native publication capture always required → required only for 1.9.0 and later | The 1.8.x local VSIX can be packaged with an empty release catalog; optional downloads show as unavailable | Restore automatically when the final series is built; qualification must produce `fixtures/arbiter/publication_catalog.json` |
| `packages/vscode/src/test/installerBootstrap.test.ts` release-bundle capture assertion | always throws without the capture → throws only in the final series | Matches the gate above | same |
| `scripts/package-native.py`, `scripts/build-service-runtime.py`, `scripts/package-portable.py` version gate | `>= (1, 9, 0)` → `>= (1, 8, 201)` | The 1.8.201 candidate follows the current notice/legal and launch contracts instead of the historical ones (stricter, not looser) | Revert to 1.9.0 only if the series numbering changes |
| `scripts/optional-publication.py` `current()` | unchanged at `(1, 9, 0)` | Publication custody stays a final-series check | none |
| Workspace/extension version | 1.9.0 → 1.8.201 | 1.9.0 is reserved for the final release | Next local build increments to 1.8.202 and so on |
| `crates/airk-retrieval/src/arbiter/runtime.rs` `release_qualification_gate` / `local_test_profile_for` | a compiled profile needed a qualified target row and a qualification digest → a profile whose `profile_id` starts with `unqualified-local-test` skips only those two checks, and only when the package version parses to below 1.9.0 | Lets the 1.8.x candidate admit the three selected models without qualification so they can be downloaded and tried; `cpu_fit`, asset, download, ABI and policy-digest checks still apply, and `publication.rs` `project()` stays strict | Remove the prefix path (or let it fail at 1.9.0, where it already refuses) once qualified profiles replace them |
| `crates/airk-service/src/service_runtime/arbiter/lifecycle.rs` status reasons | "qualified" wording → local-test profiles report "Unqualified local test profile (not release-qualified)." | Honest labeling in Admin/Workbench status | none; keep |
| same file, `user_failure_reason` | full error chain as the reason → fixed "Try again later." text for source-class and deadline download failures | The user sees a retry note; transport detail and signed URLs are never shown | none; keep |

## Restore order

1. `MAX_DEADLINE_MS` and the Answer ceiling, once an optimized end-to-end Answer
   with the independent Focus and Context stages has been timed.
2. Token ceilings, once a Context profile is measured at the intended width.
3. The calibration-collector values.
4. Re-qualify under the same limits; do not relabel earlier evidence.
