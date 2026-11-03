# Understanding missing or limited evidence

The 1.9.0 candidate adds optional `guidance` to native read results when returned
metadata identifies a limitation, or a query returns empty primary rows. It explains
observations and suggests a next read. It is not a confidence score, a complete
availability audit or proof that source bytes match the index.

CLI aliases, tools-call/shell, MCP and Workbench receive the same native advice.
Workbench displays **Evidence guidance** without executing the suggestions.
Sessions, structured details and complete JSON/Markdown exports retain it.
Selected exports retain the original source request's guidance beside its coverage;
that context does not become a claim about every selected record.

## Read the reason and its subject together

Each observation contains `reason`, `subject`, `basis`, `message` and `next_read`.
The basis names metadata in the original full response. Terse mode can omit an
unchanged status field under its existing receipt rules; Full retains that context.
Multiple observations can coexist. They do not identify the cause of an empty
query unless the returned metadata actually establishes that claim.

| Reason | Existing observation used in this phase | Meaning and useful next step |
| --- | --- | --- |
| `unsupported` | One selected adapter has explicit false capability flags | Inspect that adapter's flags. This says nothing about other adapters or the effective profile of every file. |
| `unavailable` | Saved source availability is unavailable | Inspect the reported access reason. Retained evidence can remain readable while source access fails. |
| `not_retained` | Published indexing level is Essential | Native facts/relationships are not retained. Explicit imported analysis is separate. Rich replies cannot recreate omitted native evidence. |
| `reply_omitted` | Minimal navigation representation | Some details and durable symbol IDs are omitted from the reply. Restart selected discovery with `reply_detail: rich` before an ID-based follow-up. |
| `pending` | Discovery, semantic or source-work state is pending, partial, running, paused or blocked | Inspect that subject's status when needed. Retained evidence may still be readable; advice does not resume or queue work. |
| `failed` | Discovery, semantic or source-work state is failed | Preserve the failure and revision. Inspect status and review an explicit retry; never replay an uncertain operation automatically. |
| `stale` | Observed requested source watermark exceeds completed watermark | Source work has not caught up. The subject is **source work**, not proof that a particular cited file differs on disk. Retain the observation's original age. |
| `excluded` | File vector-selection decision is ineligible with an exclusion rule | Inspect the saved decision. This is vector exclusion, not removal from the index or proof that lexical evidence is unavailable. |
| `truncated` | Existing truncation, cursor or page-more metadata | Continue the same request when a cursor exists; otherwise select a cited passage for a bounded read or review limits explicitly. |
| `unknown` | This saved query returned empty primary rows | Supplemental pointers can still be present. Empty primary results alone do not establish unsupported extraction, omitted retention, exclusion, source absence or live freshness. Inspect selected file metadata/capabilities before widening scope. |

The capability response exposes the vocabulary under `read_guidance`, including
its version, limits and inert next-read actions. This is a shared vocabulary over
existing observations. Adapter/profile family inventory and effective-profile
reporting remain later phases; absence of an observation is not proof of support.

## Choose a next read deliberately

`next_read` is an advisory action name, **not an executable tool name or new input**.
Keep the existing authorized project, path scope, publication and budgets. Use the
current tool catalog before constructing a request.

| Action | Supported route |
| --- | --- |
| `inspect_capabilities` | Native `capabilities`; select an existing adapter ID to inspect its flags. CLI: `airk tools call airk_capabilities --args '{"adapter":"text"}'`. |
| `inspect_file_info` | `file_info` for an already selected path; CLI `airk info <path>`. Preserve exact location/hash/revision metadata. |
| `inspect_source_status` | Lightweight `status` / `airk status` when a new observation is needed. Respect typed retry and original age; do not repeatedly run diagnostics or inventories. |
| `inspect_saved_policy` | Explicit read-only settings, such as `airk settings extraction_policy` or `airk settings vector_selection`. Changing policy/indexing remains a separate management action. |
| `request_rich` | Restart the selected discovery with `reply_detail: rich`. Clear a contradictory `compact` override; do not reuse a cursor from another representation. |
| `continue_page` | Same tool and original filters/representation with its exact cursor. Respect `required_record_bytes`, scope/revision fences and total budgets. |
| `read_selected_passage` | `read_file` or `context` at a returned citation, within original document-location and source-scope limits. |

For a named type/function, use `symbols` before broad concept search. Matching is
exact by default; suffix/contains modes are explicit. Use rich lookup when an ID
is needed, then scoped `references` and `read_file`/`context`. References are textual
candidates; indexed lexical boundaries can limit their coverage. Written call
relationships and suggested targets do not certify resolved callers, inheritance
or complete struct-to-method associations. Preserve reported coverage, precision,
original locations and all required pages. See [discovery](discovery.md) and the
[developer guide](mcp-developer-guide.md).

A wider search is a user/agent decision within existing authorization. Do not
respond to an empty result by searching every collection, changing filters,
reindexing, enabling a provider or selecting another retained profile implicitly.

## Cost and presentation boundaries

Advice checks a fixed roster of fields already in the native result. It examines
array lengths and, for one selected adapter, nine existing capability flags. It
does not walk evidence records, enumerate files/adapters, hash sources, query the
registry, perform coverage scans, run extraction/models or start work. Healthy
results without a qualifying observation gain no `guidance` field.

At most **eight observations** are emitted. `guidance.truncated` discloses omitted
advice; original readiness, failure, coverage and continuation fields remain
authoritative. Messages, subjects and basis labels are fixed text; advice copies
no query, source, path, cursor, rule ID or failure string. Existing evidence budgets
and publication checks still apply; small advice metadata is additional envelope
content. Responses with limitations can therefore grow. The phase ledger records
measured byte/latency effects instead of treating that growth as free.

`parse` retains advice in each native step and preserves its aggregate warnings,
truncation and completeness fields. It does not infer a second availability verdict
from the combined plan or recursively inspect its results. Fatal read errors and
typed retry receipts retain their existing contracts. Help has no evidence
availability to classify. Workbench's guidance section is text only; reading or
copying it sends no tool request and changes no selection or setting.

## Folder policy and freshness

VS Code `files.exclude` and `search.exclude` are not automatic AIRK indexing-policy
conversion. Inspect AIRK's existing ignore rules, configured roots and saved
coverage observation; an empty search cannot diagnose an ignored path. A new
inventory is explicit work, not a prerequisite for a read. See
[index coverage](index-coverage.md) and [source watching](service-source-watching.md).
Window-scoped settings belong at their supported window/user/workspace scope,
rather than assuming folder settings override them in a multi-root window.

This phase adds no Project/index hierarchy, cross-project scope, profile migration,
permanent trim or source verification. Unsaved editor buffers remain outside saved
source evidence. Existing freshness warnings stay until a later explicit
verification phase supplies an applicable receipt. Current validation and limits
are in the [implementation ledger](research/implementation-v190.md).
