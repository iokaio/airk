# Lean and Rich replies

The 1.8.8 working build adds optional `reply_detail: "lean" | "rich"` on native reads.
It selects a projection of evidence already available in the current publication.
It changes neither retained indexing detail nor search mode, source scope,
authorization, publication, model selection or live-source verification.

Three independent controls remain separate:

| Control | Purpose |
| --- | --- |
| Retained indexing detail | Selects evidence kept during indexing; changing it can require reprocessing |
| `reply_detail` | Chooses available Lean/Rich evidence for this read |
| `response_mode` / `include_status` | Controls metadata encoding and repetition; Full supports audits |

Omission preserves existing defaults and encoding. Symbols/imports/outlines keep
their minimal navigation default; code/document facts keep their applied-profile
legacy default. Workbench keeps rich facts and focused search snippets when the
choice is omitted. Existing saved sessions retain their choices. An explicit
Lean/Rich selection is independent of the applied retained profile; a Rich reply
cannot recreate facts omitted during indexing.

| Read | Lean | Rich |
| --- | --- | --- |
| `symbols`, `imports`, `outline` | Existing minimal rows with page-local file/provenance bindings | Existing rich facts, including available native IDs/details |
| `code_facts`, `document_facts` discovery | Existing compact fact projection | Existing rich facts |
| `search` | Existing focused snippets, unless `full_chunks:true` is explicitly supplied | Existing focused snippets, unless `full_chunks:true` is explicitly supplied |
| Exact/keyed reads, references, relationships, metadata and help | Existing payload where no alternative projection exists | Existing payload where no alternative projection exists |

Both choices preserve applicable source hashes, locations, publication identity,
provenance, omissions, coverage, validators, continuations and errors. Lean is not
a completeness guarantee. Rich does not select full chunks, more hits or larger
budgets. Search text expansion remains an independent `full_chunks:true` choice;
it can fit fewer hits in a fixed budget. Durable-key lookups retain their
established rich representation. Neither choice starts extraction or indexing.

## Use it across interfaces

Public MCP search arguments, on the connection's existing authorized scope:

```json
{"query":"retry budget","mode":"lexical","reply_detail":"lean","limit":10,"max_bytes":8192}
```

CLI read aliases expose the same optional flag. Tools-call and shell use the
same JSON input through canonical handlers:

```console
airk symbol Queue --reply-detail rich
airk code-facts --name Queue --reply-detail lean
airk parse --reply-detail lean --execute "/symbols Queue"
airk tools call airk_search --args '{"query":"retry budget","mode":"lexical","reply_detail":"lean"}'
```

In Workbench, choose **Reply detail** in a tool form or use a slash option such
as `/symbols Queue --reply-detail lean`. **Default** keeps existing behavior;
editing the picker submits no query. Choices remain in the tool's session draft,
plan arguments and complete JSON/Markdown exports. Structured details and selected
exports retain a returned explicit reply choice for their own source steps.

Optional null projection controls behave as Default. `parse.reply_detail`
supplies a default to steps whose choice is absent or null. An explicit
per-step choice overrides that default. Prepared plans retain the effective step
arguments in their digest; changing a prepared plan's projection requires a fresh
preparation rather than reusing its old digest.

## Compatibility and limits

`compact` remains supported. Matching old/new choices are valid; conflicting
choices reject instead of silently choosing precedence. For example,
`reply_detail:"rich", compact:true` conflicts. Clear the legacy compact override
in a form before selecting the other reply. Search `full_chunks` is independent
and supports either value with either reply detail.
Malformed or unsupported enum values reject. There is no new retained-profile
management command or migration in this phase.

Cursors and conditional validators retain their existing effective-representation,
scope, budget and publication checks. Changing projection requires restarting
discovery; equivalent legacy flags retain their original continuation behavior.
Consume all valid pages when completeness matters. Minimal rows without symbol
IDs still need an explicit rich locator read before an ID selector; see
[plan bindings](read-interface-parity.md#regression-evidence).

All native read catalogs expose the new input; those catalogs and tool-help
schemas therefore grow intentionally. Default evidence replies do not gain a
`reply_detail:null` field. Documentation/help with no alternate projection keeps
its existing payload. Strategy conversion continues to reject unsupported native
arguments explicitly; save a native plan export to preserve such choices.

Current tests and measured comparison limits are in the
[implementation ledger](research/implementation-v190.md). Installed-editor,
external-client cache refresh and platform qualification are separate gates.

P2b independently adds [evidence guidance](evidence-guidance.md) when existing
metadata reports a limitation or an empty query. That optional metadata can grow
a reply; it does not change P2a's projection defaults or legacy evidence encoding.
