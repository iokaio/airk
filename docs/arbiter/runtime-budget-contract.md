# Arbiter runtime budget contract

Decision owner: Core's `arbiter::Budget`, with the existing read-attempt,
combined-context, IPC and qualification owners. This additive decision records
final-plan D06/D07 (15 November 2026). It changes no wire field, digest recipe,
persisted state format or source authority rule.

Implementation status: the coordinated native default/test patch is applied.
The shipping Rust default is 8192 shared input tokens and 120 seconds; native
fixture emission and end-to-end qualification are recorded separately. This
contract decision is approved by the final plan; it is not a qualification pass.

## Shipping request and retained controls

| Boundary | Approved value | Meaning |
| --- | --- | --- |
| `Budget::default().input_tokens` | 8192 | One shared input-token budget across Focus and Context, every round and additional read. |
| `Budget::default().deadline_ms` | 120000 | One original operation deadline; no stage or transport resets it. |
| Other default fields | calls 8; rounds 3; fanout 8; candidates 32; max bytes 262144 | Existing ceilings and accounting semantics are unchanged. |
| Structural token/deadline ceiling | 8192 / 120000 ms | Admission limits, not proof of graph width, actual allocation or resource fit. |
| Client transport allowance | operation deadline plus 5000 ms, at most 125000 ms | Explicit response/transport overhead; it does not authorize extra native model/read work. |
| Selected Focus graph | 512 tokens per pair | Static exported graph width. Shared input-token availability does not widen it. |
| Historical/control population | explicit 2048 tokens / 30000 ms | Original native fixtures and receipts retain their exact input identities. |
| New-profile measured latency gate | p95 at most 120 seconds | Requires new exact-artifact/platform measurements under the frozen sample/quantile policy. |

The original same-host read-attempt ID, clock and absolute expiry accompany
native callbacks. Conversion samples conservatively; caller-supplied remaining
time can only shorten that expiry. Connect/write safety caps stay at five
seconds. A timeout or disconnected response does not prove execution settlement,
authorize replay or replenish any work budget.

## Fixture and evidence identity

The Rust-owned additive `airk.arbiter.budget-cases/1` metadata fixture exports the
shipping default, explicit historical control, literal maximum/refusal cases,
operation/transport allowances and selected Focus pair width. Its
`qualification: false` marker is mandatory: these are contract cases and UI
metadata, not model resource or scientific evidence. The same Rust producer
emits inert `DEFAULT_CONTEXT_BUDGET`, `READ_OPERATION_DEADLINE_MS`,
`READ_TRANSPORT_OVERHEAD_MS` and `READ_TRANSPORT_DEADLINE_MS` constants in
`packages/vscode/src/generated/arbiterBudget.ts`. This is a native runtime-value
export; the general schema-to-TypeScript pilot stays declaration-only. Without
explicit emission environment variables, the producer checks the retained JSON
semantics and TypeScript bytes. TypeScript renders those native values; it does
not maintain a second default.

Historical contract fixtures keep their bytes. Their Rust test producers specify
the old population explicitly instead of inheriting a future `Budget::default`.
The new native tests independently assert literal 8192/120000 values and reject
8193 tokens, 120001 ms and zero token/deadline inputs. They also assert the old
control's exact serialized bytes and the 125000-ms generic transport ceiling.

The 1.8.201 relaxation table remains dated history. Its original T2048/N8,
30-second p95, numerical/resource rows and CAL/confirmation receipts are not
relabeled. New graph/profile widths, memory, resident coexistence, latency and
numerical cases need new policy/artifact identities and measured qualification.
All-three optional-profile publication requirements remain unchanged.

## Validation

Core's focused selector is `test(arbiter::budget_tests::)`; its emitter is
`arbiter::budget_tests::default_budget_native_fixture_emitter`, with
`AIRK_WRITE_ARBITER_BUDGET_CASES` and `AIRK_WRITE_ARBITER_BUDGET_TYPESCRIPT`
naming the reviewed JSON and TypeScript outputs during explicit regeneration.
The native fixture destination is `fixtures/split/arbiter/budget_cases.json`. Historical
producer reruns must prove byte identity before any old fixture is replaced.
Core/IPC checks alone do not establish native settlement, transport parity,
model fit or installed-product qualification; the final plan requires those
owners' actual tests and receipts separately.

## Accepted reads and connection admission

Connection reuse/admission windows authorize a new submission; they do not
shorten an already accepted read's original absolute deadline. The frontend
55-second reuse window and worker 60-second admission window remain unchanged.
Accepted work retains the original read-attempt identity, cancellation, clock,
partition and 120-second operation bound, with the existing 125-second delivery
and hard-reap ceiling. Source/revocation fences and every-poll authority checks
still apply. Expired connections refuse new admission. Unknown settlement does
not authorize replay or release of execution custody.

The correction removes two independent connection-expiry clamps and an
impossible partition reconnect condition. Three-crate scoped Clippy and 106
grouped frontend/worker/installer regressions pass on the corrected source.
The rebuilt complete Answer journey remains a separate required observation.
