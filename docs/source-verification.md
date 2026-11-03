# Dated saved-source verification

AIRK 1.9.0 development evidence reads default to **Report**. A bounded native
check compares the complete saved bytes of each admitted, distinct referenced
file with the source hash that supplied the indexed evidence. Implementation
qualification is tracked in the [phase ledger](research/implementation-v190.md).
This is a dated observation, not independent certification or a guarantee that
the file will remain unchanged.

| Mode | Behavior |
| --- | --- |
| `report` (default) | Return the indexed evidence and a complete bounded verification receipt, including mismatches and unchecked references. |
| `off` | Return indexed evidence without source verification, hashing or receipt placeholders. |
| `require` | Return a nonretryable `verification_required_unmet` failure unless every returned reference is accounted for and its saved source matched. Preserve the attempted indexed evidence and complete receipt in the failure. |

The mode is chosen by explicit request arguments or the native default. AIRK does
not infer it from client identity, question wording, search mode, reply detail or
retained indexing profile. Status, diagnostics, capabilities and help do not
verify source files. An empty reference inventory reports `not_applicable`, never
a successful certification of a whole index.

## Controls and examples

The documented Report preset admits **8 distinct sources**, **16 MiB of aggregate
raw bytes**, and a **2,000 ms cooperative deadline**. Each override is optional:

| Native/MCP field | CLI flag | Safety ceiling |
| --- | --- | --- |
| `verification` | `--verification` | `off`, `report`, `require` |
| `verification_max_sources` | `--verification-max-sources` | 1–256 sources |
| `verification_max_bytes` | `--verification-max-bytes` | 1–1,073,741,824 raw bytes |
| `verification_deadline_ms` | `--verification-deadline-ms` | 1–30,000 ms |

Supplying a budget without a mode retains Report. Explicit Off with any
verification budget is rejected. Null, zero and out-of-range controls are
rejected before source checks. These limits are independent of ordinary result
limits and snippet/output byte budgets.

```console
airk info src/queue.rs
airk info src/queue.rs --verification off
airk read src/queue.rs --start-line 1 --end-line 40 --verification require
airk tools call airk_file_info --args '{"path":"src/queue.rs","verification":"require"}'
```

For public MCP, call `file_info` with
`{"path":"src/queue.rs","verification":"require"}`. Compatibility names such as
`airk_file_info` retain the same inputs and complete result. CLI shell/tools and
convenience reads share the native implementation. Workbench's live tool forms
expose the same mode and numeric controls; an empty field retains the native
default. Reading or exporting an old Workbench receipt performs no new check.

## Plans and selected exports

An executed evidence plan normally verifies its final projected response once,
deduplicating repeated file references across steps. Inner reads explicitly skip
their own checks. Preparation performs no source hashing. The resolved policy,
budgets and `verification_scope` are included in the prepared plan's digest;
changing them requires preparing again.

To select different modes per step, use `verification_scope: "steps"` on the plan.
Top-level budgets then provide defaults for evidence steps, with explicit
per-step overrides and Off supported. Receipts belong to each step and its
returned projection; no aggregate verification claim is invented. An explicit
response-wide check combined with per-step choices is rejected unless step scope
is selected. Metadata-only plans perform no source checks, including when run
from a prepared plan containing the resolved default controls.

Complete JSON/Markdown copies and exports retain all receipt fields. A selected
evidence export carries `airk.saved-source-selected-observations/1` observations:
associations are rebound to selected citations, while original outcomes, dates,
limits and uncertainty remain. It reports that no new verification occurred and
does not promote a subset of a failed/partial request into a successful check.
Inspector and Checker also reuse native observations, retaining an independent
unsaved-editor observation instead of hashing the file a second time.

## Reading the receipt

`source_verification` uses schema `airk.saved-source-verification/1` and method
`sha256-full-saved-source/1`. It retains original publications, source roots and
relative paths, expected/observed hashes, extraction profile identity, exact
reference associations, observation intervals, requested/effective limits and
counts. Future Project/index identity slots remain nullable until those features
are implemented; current single-index binding and collection identities remain
explicit.

Per-source outcomes distinguish `matched`, `mismatch`, `missing`, `unreadable`,
`out_of_scope`, `changed_during_check`, `budget_exhausted`, `publication_expired`
and `unsupported`. A whole receipt says `verified` only when all references are
accounted for and all distinct referenced sources matched under the publication
and scope fence. An unknown denominator remains unknown. Metadata or output
limits cannot silently turn an incomplete inventory into a verified subset.

Only a corresponding saved-source freshness warning is replaced by a dated match
observation. Scope, search coverage, extraction/OCR, unresolved-language and
relevance qualifications remain. Collection `freshness.live_source_verified`
continues to be false: checking returned files does not verify the entire index.
Recheck live files before later edits; serialized receipts are not signatures or
transferable source-read authority.

## Work and limits

Verification starts no extraction, inference, indexing job, publication or writer
operation. Each file is streamed through SHA-256 with a 64-KiB buffer and source
concurrency one. Deduplication is within a response; later reads require new
checks. A tiny snippet from a large document still requires hashing the complete
raw file, and Report can therefore add measurable I/O and latency.

The raw-byte budget reserves one byte of prospective EOF headroom: a file whose
reported size equals the remaining budget is unchecked. An empty file can be
checked with one byte of available budget without charging a successful EOF as
data. Attempted data reads, including detected growth, are charged. Policy/index
metadata reads are separate from the raw-source byte count.

Native capture checks no-follow containment, current source eligibility, handle
identity and detectable changes before/after reading. Historical publications
cannot become current capture authority. Deadlines and cancellation are checked
between reads; a blocking filesystem operation cannot be forcibly interrupted,
and its admitted capacity remains occupied until it returns. A metadata match or
quiet watcher never proves source equality. Saved-byte agreement does not check
unsaved buffers, extraction correctness, truth, resolved call targets, omitted
search matches or completeness of the workspace.

Response-wide plan verification requires at least an 8,192-byte output budget,
reserved before source checking; explicit Off retains the existing smaller-output
contract.

Reference inventory and receipt storage are bounded to 4,096 references and
32 KiB, further constrained by the existing transport frame and plan output
budget. If receipt metadata cannot fit, a pre-check
`verification_reply_budget` refusal explicitly reports
`verification_performed: false`; narrow the result or raise the plan's output
budget. A late projection that removes checked evidence cannot retain a dangling
verification claim and returns `verification_projection_changed`. These are
nonretryable requests to revise the inputs, not automatic replay instructions.

Selected refresh and multi-index verification are later phases; P4 does not
refresh sources or extend query scope. See [ADR 003](decisions/003-dated-saved-source-verification.md)
for the contract and the [implementation ledger](research/implementation-v190.md)
for measured costs, tests and remaining qualification boundaries.


Per-step verification requires the raw plan output view, which retains each step's
complete receipt. An evidence-only view must use response-wide verification;
an enabled per-step/evidence-only combination is rejected before source checking.
Response-wide Require failures use the same nonretryable MCP/CLI error envelope
as direct reads, preserving the entire attempted plan and its native receipt.
Per-step failures remain identified within their individual step outcomes.


The selection digest identifies the producer's canonical evidence projection
before receipt attachment, verification-result presentation and Terse compression;
transport/request identities and changing readiness observations are normalized.
It is not a checksum of the final serialized envelope, a representation validator
or a signature. Reference pointers and original hashes/publications remain the
explicit associations to the returned evidence. If late transport observations
force removal of a checked location (including a whole result becoming null),
the response refuses delivery with nonretryable `verification_projection_changed`.
Increase the output budget or narrow the plan before another request; a clipped
response cannot retain the removed evidence's verification claim.


On Windows, capture retains the existing pinned-handle sharing guards: a held
source excludes writes and ancestor-directory replacement during that capture.
An incompatible existing writer makes verification unchecked rather than waiting
for a write or overriding it. Report adds that sharing window to evidence reads;
a blocking filesystem call can extend the cooperative deadline. The local
latency campaign does not qualify concurrent editor-save responsiveness.


The fixed native tool routes and input schemas are shared immutably across
client sessions to avoid repeated schema construction. Kernel/source scope,
permissions, admission and dated response observations remain session-local.
P4 passed local qualification with the final source-bound runtime. Its Off path
passed the frozen/preceding latency gates, and relevance scores stayed exact.
Report adds intentional saved-byte and metadata work: the final warm-cache small
file median was 135.76 ms versus 66.67 ms with Off. The phase ledger retains all
earlier failures, final measurements and remaining platform/physical-I/O limits.
