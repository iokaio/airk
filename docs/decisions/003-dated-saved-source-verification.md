# ADR 003: dated saved-source verification

Date: 2 November 2026.

Status: implementation design for P4/P7/P12 of the approved 1.9.0 plan. This is
not a shipped feature, independent certification or claim of platform qualification.

## Context

Authenticated index identity establishes which saved revision supplied evidence.
It does not establish that current saved source bytes still agree. Reindexing each
hit would mutate publication/model state and cannot validate the old snippets.
Coverage inventories are broader than a response; metadata/watcher silence is
not source equality. Current Inspector/Checker behavior is useful precedent,
but native CLI/MCP/Workbench need one complete contract.

Relevant live paths are [conditional reads](../../crates/airk-kernel/src/current/conditional.rs),
[coverage](../../crates/airk-kernel/src/coverage.rs),
[source access](../../crates/airk-ingest/src/lib.rs),
[source freshness](../service-freshness.md) and
[Inspector/Checker](../../packages/vscode/src/explorerInspect.ts).

## Decision

### Guarantee and controls

Use **Saved source verified at [time] against indexed revision [revision]**.
The guarantee is full saved-byte agreement for the identified references during
the observation interval. It does not establish extraction/OCR accuracy, truth,
complete search, resolved calls, unsaved-buffer agreement or continued freshness.
Serialized receipts are observations, never transferable authority or signatures.

`verification: off | report | require` defaults to **off**. Keep it independent of
reply/retention detail, response encoding, search mode and
`member_availability: report | require`. Off adds no source opens/hashes, identity
lookups, jobs or receipt placeholders. Explicit checks start no extraction,
embedding, writer lock, source update, publication or model provisioning.

Canonical enabled requests carry a resolved explicit budget:
`verification_max_sources`, `verification_max_bytes` (aggregate raw bytes), and
`verification_deadline_ms`. All three are required with report/require; clients
can expand a measured, published P4 preset before dispatch, but may not supply
unqualified silent defaults. Keep fields flat for equivalent native schemas,
CLI flags and Workbench numeric controls. Metadata/help reads do not acquire a
whole-index verification meaning; advertise controls only on evidence reads and
parse. The contract pack fixes safety ceilings; P4 measures presets before use.

### Complete reference inventory

Collect every returned saved-source reference, including compact shared files,
definition pointers, mapped/document locations, qualified graph endpoints and
conditional/not-modified targets. Never reinterpret source text or arbitrary
metadata as a locator. Preserve all original result/provenance/coverage/budget
qualifiers. A known file reference lacking a valid source identity is unsupported,
not silently dropped. Unresolved written targets remain explicitly unverified
language assertions; their host-source match does not resolve them.

Deduplicate known files by index/backing/root identity, logical source and expected
raw digest, not chunk count or equal bytes across independent indexes. A shared
index selected through several Projects is checked once. Reference associations
remain exact and response-local. Each source gets an outcome; if metadata/count
limits prevent a complete inventory, disclose that incomplete accounting and
return non-verified rather than inventing a denominator or hashing a hidden subset.

Use authenticated identity from the same selected read. Where a tool normally
omits a hash (paths-only listing/not-modified), an enabled producer supplies bounded
identity metadata from that same publication before capture. This is not another
canonical tool call, full inventory or lookup against a newer head. Off retains
its existing projection/work. Historical change endpoints cannot become current
source-read authority; unsupported/expired references remain explicitly unchecked.

### Read-only capture and races

Resolve only through the current authorized roots and source/policy predicates.
An indexed stale path is not permission to open an excluded path. Validate alias,
root, containment and no-link/reparse rules, including policy before hashing.
Native verification uses the same future policy hook as [ADR 002](002-projects-indexes-and-query-scope.md).

Open read-only and stream **the complete raw source** through SHA-256 with a
64-KiB working buffer and initial concurrency **one**. Size/time metadata can
reject obvious changes, never prove equality. Charge attempted full-file bytes
to the aggregate budget. Use handle identity/size/change observations before and
after reading, then revalidate the pathname/root binding. Detectable replacement,
write or identity loss yields `changed_during_check`, not matched. Windows uses
native handle identity; Unix uses device/inode/change metadata. Unsupported stable
identity/containment evidence gives unsupported, not a guessed match.

Check cancellation/deadline between reads and stop further source admission.
Ordinary filesystem calls can block beyond a cooperative deadline; do not claim
forcible interruption or release actual read capacity while a blocked operation
still owns it. Other status/read capacity must retain existing bounds. No global
atomic filesystem snapshot or exhaustive protection against adversarial concurrent
writers is promised. Cross-platform and remote-filesystem behavior need evidence.

Compare with the original expected hash and validate the original current-only
publication/scope fence before completion. A changed head/grant expires the
attempt; never substitute a new hash/publication into old evidence. Unchanged
bytes reuse retained extraction, with no index/model/writer work. Do not cache a
later fresh-check claim by size/mtime or watcher state. Within-response dedup is
safe; a later request needs a new check.

### Receipt and failure semantics

Receipt schema **`airk.saved-source-verification/1`**, method
**`sha256-full-saved-source/1`**, includes requested mode, response-selection and
enforced-scope digests, budgets/limits, observation interval, per-origin publication,
per-source expected/observed hashes, location associations, profile/provenance
binding and all counts/bytes/outcomes. Sources retain self-contained origin/root/
path identity. Shared associations must resolve within the complete response;
selected copies materialize/rebind them, with no dangling table indices/pointers.

P4 includes nullable stable index ID and Project/member-association slots from day
one, plus the explicit legacy binding/collection/incarnation identity. Null means
the new metadata hierarchy is not activated, not unknown source authority. P7
fills those slots without changing the saved-byte guarantee or inventing a global
publication. Data-root/source/grant identities remain distinct.

Per-source outcomes are `matched`, `mismatch`, `missing`, `unreadable`,
`out_of_scope`, `changed_during_check`, `budget_exhausted`,
`publication_expired` and `unsupported`. Counts distinguish reference inventory,
known sources, stable whole-hash comparisons, matches, mismatches and unchecked
subjects. Checked means matched/mismatch; raced/partial reads remain unchecked even
when bytes were consumed. Incomplete inventory/metadata is explicit and prevents
aggregate verification. Zero references is **not_applicable**, never all verified.

Report retains indexed evidence plus all applicable outcomes. Require gives a
typed non-success with the complete attempted evidence/receipt when any required
file reference cannot be checked/matched; it must not become a complete certified
subset. Use an MCP error result carrying the complete structured JSON for direct
reads, and preserve its nonretryable `verification_required_unmet` code through
CLI tools/shell/convenience aliases. CLI prints the structured failure before its
nonzero exit. Existing parse envelopes can retain their established failed-domain
status while carrying complete step/aggregate receipts; extend typed mapping and
UI storage rather than collapsing them to a generic string. Only actual typed
publication/admission refusals can authorize retry; never replay an entire plan.

Parse inventories/verifies its final response once under its outer budget and
deduplicates repeated references across steps. Do not apply a global mode by
independently hashing every inner step. An explicit per-step check, if supported,
must remain separately budgeted/identified and cannot silently duplicate a global
check. Prepared digests include verification policy/budgets; changing them needs
preparation. P4 may initially reject simultaneous global/per-step enabled checks
with a clear conflict, while supporting each separately through the same handler.

### Presentation, refresh and performance

Replace only the corresponding freshness warning after an applicable match. Keep
scope, source integrity, extraction, relevance and unresolved-language qualifiers.
Mixed receipts say N of M referenced sources verified (or unknown denominator),
with per-source reasons. Keep collection `live_source_verified: false`: a checked
subset is not global freshness. Native CLI/MCP knows no complete editor-buffer
state. Workbench retains an independent unsaved-buffer observation and labels
saved disk agreement accordingly.

All Lean/Rich, Terse/Full, native/CLI/MCP/Workbench, parse, session, cards, JSON/
Markdown and selected exports preserve receipts and uncertainty. Reopening an
old receipt displays its original date, never a newly current verification badge.
Not-modified validators do not renew check times. Follow-up edits need current
guards. Keep existing warning behavior when off.

Refresh is a separate **P12** reviewed management action using existing jobs,
admission, receipts, quotas and pause/manual policy. It must disclose full-scan
fallback before admission, then requery one new publication and verify every new
reference. Bound churn/requery attempts. Do not attach new hashes to old snippets
or infer whole-workspace completeness from selected refresh.

P4 compares off against frozen/preceding gates and instruments exact hash bytes,
source attempts, extraction/model/publication counts (all zero on unchanged
checks), large/slow/raced file budgets, cancellation and pipeline resources.
Measure real native capture before choosing enabled presets. Retain physical-I/O,
platform and installed-editor unknowns; do not treat them as passing qualification.

## Alternatives

Reindexing/re-extracting every hit changes publication and adds unnecessary model
and writer work. Metadata-only checks and cached coverage cannot prove fresh
source equality. Re-extraction with the same parser is not independent accuracy
certification. Always-on checking adds unchosen large-file/remote-I/O cost. Keeping
only an editor implementation breaks agent/CLI parity. None is selected.

## Consequences and acceptance

Cost is proportional to distinct referenced raw bytes, not snippet size. A small
PDF hit may require a large read; explicit budgets and unchecked outcomes make
that cost visible. Off remains the legacy fast path. Stronger dated agreement is
useful without promising perfect extraction, exhaustive retrieval or future truth.

[Schemas/fixtures](../research/contracts-v190/README.md) are design acceptance,
not implementation evidence. P4 must pass all receipt/race/budget/actual transport
and export cases before public source verification; P7 extends member identities;
P12 implements refresh. See the [phase ledger](../research/implementation-v190.md).
