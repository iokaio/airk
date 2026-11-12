# Recommendations and agent query scope

Recommendations were introduced in the unpublished **1.8.8** P8 working build
and remain available in **1.8.11** through the canonical `query_scope` read.
Review an existing index, a focused task or a purposeful combination without evidence queries, source scans or model loads.

In **Indexing → Search scope**, expand **Help me choose indexes**. Leave both
selectors blank to inspect the active connection, or enter explicit index-set and index subsets.
The native response and API fields retain `Project` terminology for compatibility. Available,
selected and omitted indexes are separate. Shared indexes appear once; selecting
fewer Projects preserves only their own prefix grants. Each situation explains
its user-stated reason, expected benefit and uncertainty. **Keep this layout /
Dismiss** is always available.

**Use this query scope** deliberately prefills the selectors in a temporary
Workbench Search form, preserving existing query, filters, limits and availability
choices. Review it before running. It does not change the active grant or agent definition.
To organize a reusable selection, open **Indexing → Index sets → Open index set**
and review its memberships and defaults. Apply the desired search selection from
**Search scope**. Set defaults do not silently replace a window's explicit
selection. Reuse opens the existing set's membership review. Split advice can open the selected index's controls when its
native split capability is available; [splitting](index-splitting.md) still requires
a separate complete native review and explicit apply. [Permanent exclusions](source-policy.md)
likewise require their own reviewed operation; advice cannot apply them. Query
omission differs from membership removal, index deletion and permanent denial.

The current catalog has no dated, prefix-qualified coverage, overlap or cost
receipt. These observations remain **unknown**, including source lag and semantic
availability. Whole-index counters cannot establish a selected prefix's coverage.
Advice claims no completeness, measured savings, physical overlap or safe split.
A selected situation is a user statement, not a detected fact. Request existing
observations separately; Recommendations adds no polling or worker probes.

## Shared native contract

With an explicitly reviewed combined session, request:

```json
{
  "view": "recommendations",
  "situation": "focused_task",
  "selection": {
    "project_ids": ["prj_01010101010101010101010101010101"],
    "index_ids": ["idx_02020202020202020202020202020202"]
  },
  "max_bytes": 65536
}
```

These IDs are fictional; use current discovery. `selection` is optional; an
explicit selection requires both nonempty unique arrays and only narrows the
frozen connection and selected Projects' authorized union. `situation` accepts
`keep` (default), `reuse`, `focused_task`, `shared_dependencies`,
`different_lifecycles`, `overlap`, `irrelevant_path` and `never_index_path`.

A selected Project may contribute no queried index when the explicit subset
omits its members; Recommendations retains that Project exactly as retrieval
does. Single-Project scopes can prefill that Project's defaults review. For a
combined scope, use each Project's existing defaults form so another Project's
index is never proposed as its default merely because it belongs to the union.

CLI uses `airkctl --combined-session <reviewed-session-json> tools call
airk_query_scope --args <arguments-json>`. MCP uses `query_scope`, or native
`airk_query_scope` where advertised. Workbench Tool forms and explicit `parse`
plans share that schema. Structured details and complete JSON/Markdown copies
preserve the native body, recipes, limits and delivery receipt. Indexing JSON
copy/export retains its entire Recommendations response too.

The slash form accepts the same nested JSON selection, for example
`/query_scope recommendations --situation focused_task --selection '{"project_ids":["prj_01010101010101010101010101010101"],"index_ids":["idx_02020202020202020202020202020202"]}' --bytes 65536`.
These IDs are also fictional. CLI and MCP `parse` transports exercise this form
against the same native result; it does not accept a selection as an opaque string.

At most 32 authorized candidate indexes and eight advisory rows are allowed;
the current projection emits three cards. `max_bytes` bounds the complete reply,
including receipt (default 16,384; maximum 65,536 bytes). This view accepts no
cursor or page `limit`. Insufficient budgets fail rather than silently dropping
members or boundaries. Library coverage is explicitly connection-only. Other
discovery views retain bounded pages and revision-bound continuations.

Unknown/inaccessible IDs, stale grants and changed authority fail without dropping
members or widening scope. Delivery checks catalog/scope again. Copy and Use
revalidate grants and reject changed observations. Legacy connections return
`configuration_required`; legacy `project` bindings never silently become new
Project IDs.

## Agent instructions

Copy emits inert text derived from the catalog actually served by the installed
frontend, without a fixed tool count. It includes generation time, catalog digest,
scope identity, Project names/IDs, selected source/prefix boundaries, omissions,
active/pending state and `tools/call` JSON examples. Native validation checks
inputs; actual transport fixtures execute the examples. Missing capabilities
fail instead of emitting guessed fields.

Instructions preserve explicit `project_ids` **and** `index_ids`, shared-index
deduplication, origin-qualified follow-ups, exact publications/hashes/locations,
member outcomes, shared budgets, continuations and typed retries. Availability
and saved-source verification are separate. Zero returned matches do not prove
absence; widening remains deliberate and reported. Older catalogs require their
supported single-index binding and disclosure that combined selection is absent.

Launcher and client paths belong to each user/machine. Copying edits no AGENTS.md,
CLAUDE.md, configuration or sources and transfers no grant. Lifecycle actions
remain explicit reviewed management operations. No credentials or source contents
enter the block.

See [combined reads](combined-reads.md), [agent setup](agent-integration.md),
[architecture](architecture.md) and [phase evidence and limits](research/implementation-v190.md#p8--recommendations-and-harness-guidance).
