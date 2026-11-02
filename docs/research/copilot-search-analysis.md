# AIRK and Copilot search: implementation comparison

**Research date:** 2 November 2026. **AIRK:** version 1.8.7, live source at
`0dbd2f0729d158a73686ce726a424555a9bbb3f8`. This is a source review, not a
retrieval benchmark, security audit or cross-platform qualification.

## Findings

AIRK has a stronger fit for **offline, auditable retrieval across code and
business documents**. It exposes source hashes, indexed publication identity,
coverage, bounded results and conditional reads as part of the retrieval contract.
Copilot's selected client code has a stronger fit for **interactive code discovery
inside an editor**: it incorporates editor text, reconciles remote indexes with
local changes, offers several search tools, and uses model-assisted selection to
turn candidate passages into useful locations. Neither finding establishes which
system returns more relevant results on the same queries.

The most useful lessons for AIRK are better candidate ranking, optional result
diversity, actionable coverage explanations, and end-to-end relevance evaluation.
AIRK already has token-aware semantic units, incremental indexing, caches, scoped
retrieval and semantic readiness; these should be improved rather than rebuilt
under different names. Copying Copilot's cloud services, chat prompts or agent
loop would conflict with AIRK's role as an independent local retrieval kernel.

A concrete AIRK weakness is visible without a benchmark: the current lexical
branch requires all normalized query terms and orders candidates by stable ID,
not lexical relevance. Its position in that ordering then contributes to hybrid
rank fusion. Improving that branch is a more direct first experiment than adding
an LLM to search. [A1] [A2]

## Evidence and limits

The local `docs/copilot-research/` directory is ignored research material. All
79 copied files were verified against their manifest's SHA-256 and byte counts
before this review. It is a selected, non-buildable snapshot, with shared services,
backend implementations and other extension components intentionally omitted.
The report cites pinned public upstream files so its evidence remains navigable
without committing the local copies.

- Copilot source: `microsoft/vscode-copilot-chat` at
  `5863f5a7088958050792b5dccbe8b46c6e13eccc` (the archived repository).
- Supplemental feature documentation: `microsoft/vscode-docs` at
  `8f7dd621e3f8e2450d9048455d787688f2de5bf3`.
- AIRK: current source, relevant contracts, and test assertions read directly from
  disk. AIRK MCP supplied scoped discovery at publication generation 5878; indexed
  observations were navigation evidence, not a substitute for live source reads.

**Implemented** below means a path exists in the inspected source. **Documented**
means the feature guide describes it, possibly through omitted components.
**Proposed** means an original recommendation for AIRK. Tests cited here were
inspected, not executed for this analysis. Documentation and source have separate
revisions; neither is proof of the current deployed Copilot service.

The Search view feature guide also describes manual or automatic semantic search
and optional keyword suggestions. Those are editor interaction policies, distinct
from the underlying retrieval capability. AIRK can study the separation without
adding a model-generated search mode to its kernel. [C15]

The comparison is intentionally asymmetric: AIRK's local implementation is
available, whereas Copilot's server-side index, model training and backend
operating guarantees are not. An absent field in this client response does not
prove that the Copilot service lacks a corresponding internal guarantee.

## What the inspected pipelines actually do

**AIRK:** saved source files pass through bounded extraction and original-location
mapping. Citation chunks and token-aware semantic units serve different purposes.
The current search path selects lexical and/or vector candidates from one indexed
publication, fuses their ranks, materializes verified evidence, and applies result
and text budgets. Filters are applied before branch limits. Definition hints are
supplemental navigation, not a relevance reranker. [A1] [A3] [A4] [A5]

The production vector path opens `MutableDiskIndex` and calls its DiskANN-backed
search or bounded exact subset search. This is stronger evidence than a dependency
name, but it does not establish production scale or platform qualification. The
simple in-memory lexical implementation in `airk-retrieval/src/lib.rs` is not the
current FTS-backed search path reviewed here. [A1] [A2] [A6]

**Copilot workspace retrieval:** the workspace service prepares repository search,
starts query embedding work early, obtains remote candidates and local-change
candidates, reconciles them, applies exclusions and ranks the resulting passages.
Its optional remote reranker can reorder results when requested and available;
otherwise the workspace service uses embedding-based ordering. Remote ingestion
also covers workspaces/files not handled by repository code search. A locally
stored index or cache must not be mistaken for fully offline embedding or search.
[C1] [C2] [C3] [C4]

**Copilot Search view:** the provider retrieves workspace chunks, starts keyword
suggestions, asks a chat endpoint to choose file/query pairs, and combines those
choices with chunk similarity. It then resolves locations through text search
and editor parsing. This is distinct from the `#codebase` tool's presentation of
workspace context. [C5] [C6] [C7]

One detail matters when assessing the reranker: in this Search view provider,
results are reported **before** a second `enableRerank` search. That second call
feeds comparison telemetry, not replacement results in the displayed search.
The separate workspace service does support returning reranked results. Treating
all these paths as one universal ranking pipeline would overstate the evidence.
[C1] [C5]

## Feature and ability comparison

| Area | AIRK implementation | Copilot snapshot / documentation | Assessment |
| --- | --- | --- | --- |
| Execution and service dependence | Local extraction, bundled embeddings and local indexes; no cloud account needed for native retrieval. [A5] [A7] | Authenticated repository search, remote embedding/chunking clients and optional remote reranking; some indexes/caches are local. [C1] [C2] [C3] | AIRK offers stronger deployment independence. The copied Copilot code cannot reproduce the full service. |
| Evidence identity | Citations include source content hashes and original locations; responses carry publication and readiness data. Conditional reads reject mismatched hashes or expired revisions. [A4] [A8] | File URIs, ranges, text and embedding types are central. Repository state tracks indexed commits, but the selected provider/tool contract does not expose AIRK's common publication/validator envelope. [C2] [C5] [C7] | AIRK is better suited to automated evidence reuse with explicit validity checks. This is a contract comparison, not a claim about backend integrity. |
| Freshness during editing | Retrieves an indexed snapshot of saved files. Coverage distinguishes stale, missing and removed sources; it does not index unsaved editor buffers. [A5] [A9] | Workspace file representations read editor text and listen for document changes. Remote results can be replaced by local-change results. [C2] [C8] | Copilot has the broader interactive editing path. AIRK's snapshot boundary is clearer but can lag the current editor. |
| Lexical retrieval | FTS5 candidates match normalized terms with AND and are ordered by stable ID. Separate navigation APIs support other bounded reads. [A2] [A10] | Dedicated file/text tools accept globs, literal/regex inputs and result limits; the text tool can retry the opposite regex mode when no results are found. [C9] | Copilot offers more forgiving search interaction. AIRK's lexical branch is predictable but comparatively weak for relevance ranking. |
| Semantic ranking | Local 384-dimensional MiniLM embeddings; hybrid search uses reciprocal-rank contributions of `1 / (61 + zero_based_rank)`. [A1] [A7] | Remote embedding types and chunk similarity, model-selected snippets, and an optional remote reranker are visible. [C1] [C3] [C6] | Copilot exposes richer selection mechanisms. Better ranking quality is plausible, not demonstrated; larger dimensions alone prove nothing. |
| Result diversity | Search removes duplicate vector occurrences but has no per-file cap in the inspected final result loop. [A1] | Search view combination defaults to five files and three chunks per file, with containment checks for model-selected chunks. [C6] | Copilot has a concrete diversity policy worth testing. Fixed caps can also hide useful concentration in one file. |
| Chunking and context | Exact citation partitions preserve source bytes/locations. Separate semantic units target 256 tokens with a 32-token context allowance and hard document/region boundaries. [A3] [A11] | A local naive chunker targets 250 tokens, removes blank lines by default, dedents text and clips long line input. Remote chunking is another path whose full behavior is unavailable. [C10] [C3] | AIRK already has a strong separation of retrieval inputs from evidence. The naive chunker is not proof that all Copilot chunking is naive. |
| Documents beyond code | PDF text-layer, DOCX, PPTX, XLSX and other adapters preserve page, block, slide, table or worksheet locations; failures are reported. [A4] [A12] | Workspace-index documentation describes text files and explicitly excludes binary files such as PDFs. Remote document-search client code is also present. [C11] [C12] | AIRK has broader demonstrated native workspace artifact extraction. This does not prove Copilot cannot use documents through attachments, other tools or remote services. |
| Symbol and usage navigation | Native written-definition facts, textual references and candidate hints; not general compiler-resolved cross-file understanding. [A1] [A10] | Documentation describes usages through references, implementations and definitions supplied by editor language intelligence. The relevant implementations are not all in this copy. [C11] | Copilot documents stronger editor-integrated navigation; that wider ability is not independently validated by this subset. |
| Scope and exclusions | Collection/root authorization plus literal prefixes, language/evidence-kind filters and ignore policy; metadata filtering precedes top-k. [A1] [A5] [A13] | Multi-folder globs, editor exclusions and repository authorization. The text tool supports explicitly including ignored files; documentation says opened/selected ignored files can enter editor context. [C5] [C9] [C11] | Copilot offers more flexible editor discovery. AIRK's configured source boundary is more appropriate for strict retrieval isolation. Ignore patterns and authorization are different controls. |
| Incomplete or unavailable search | Discovery and semantic readiness, byte/result truncation, explicit policies while vectors are pending, and separate coverage observations. [A1] [A4] [A9] | Strategy state, warnings, timeouts and cancellation. Missing local-diff results can produce a remote-only warning. [C1] [C2] [C7] | Both handle partial availability. AIRK has more structured evidence completeness; Copilot often supplies more direct next-action wording. |
| Interfaces | CLI, MCP and Workbench share native handlers/full response data; actual-handler parity tests exist. [A14] | Search view and language-model tools consume shared workspace search but reshape output for their particular use. [C5] [C7] | AIRK supplies a reusable evidence service outside one editor. The copied subset is insufficient to judge all Copilot transports or parity. |
| Scale and storage | Local disk-backed retrieval, incremental publications, shared embedding occurrences and bounded subset ranking. Local CPU, disk and scan budgets still limit work. [A6] [A15] | Remote repository indexes amortize work across clients; local caches and bounded diff search reduce repeat work. Server index internals are absent. [C2] [C4] [C11] | Copilot has a structural distribution advantage for hosted repositories. No throughput, memory or large-repository winner is established. |
| Evaluation and observability | Contract, scope, extraction, continuity and semantic-unit assertions; local status/readiness reporting. [A9] [A11] [A13] [A15] | Ranking insights, search timing, strategy and fallback telemetry; unit/scenario test scaffolds. [C5] [C13] [C14] | AIRK can learn from stage-level relevance diagnosis. Neither inspected test set is a matched comparison of retrieval quality. |

## What AIRK can learn and use

### 1. Improve the lexical branch before changing the semantic architecture

The current FTS query uses all normalized terms and `ORDER BY d.stable_id`.
Consequently, a long natural-language question can have no lexical candidates,
and a broad exact query can select its first bounded candidates by identity
rather than usefulness. Hybrid fusion cannot recover a lexical candidate excluded
before fusion, and it gives stable-ID order a relevance-like vote. [A1] [A2]

**Proposed:** evaluate a deliberately ranked lexical strategy, such as bounded
exact-identifier/path boosts and an independently implemented term-relevance
model. Do not assume BM25 is a drop-in change: current FTS uses `detail=none` and
`columnsize=0`, so ranking inputs, storage cost, authentication and update behavior
need design and measurement. Keep the existing explicit lexical semantics where
callers rely on them; make any relaxation or alternate ranking a documented input.
Use stable IDs for deterministic ties and identity, rather than as a relevance
signal. This can remain entirely local.

### 2. Make diversity and redundancy measurable options

Copilot's file/chunk caps address a real retrieval problem: a single large or
repetitive file can consume the result budget. AIRK's occurrence deduplication
solves identity duplication, not necessarily repetition or cross-file diversity.
[C6] [A1]

**Proposed:** test optional per-file caps or a bounded diversity-aware ordering
against the current baseline. Use original citation identity and exact spans to
collapse duplicates. Retain a way to inspect concentrated results when one file
really contains the answer. Any selected subset must be explicit in responses,
exports and CLI/MCP/Workbench inputs; a hidden UI cap would violate AIRK parity.

### 3. Separate candidate generation, selection and presentation

The Copilot code makes these stages visible, which is valuable even without
adopting its models. A retrieval miss, a poor reranker, a missing local index and
an awkward preview require different fixes. AIRK already separates extraction,
semantic inputs and evidence materialization; preserve that structure. [C1] [C5]
[A1] [A3]

**Proposed:** add bounded local diagnostics describing candidate counts by branch,
selected mode, filtering, redundancy removal and truncation cause. First determine
which observations already exist so this does not create a second status system.
Keep full evidence available, including when a renderer presents a subset. If a
future local reranker is justified, place it inside the owning retrieval component
and evaluate its model cost separately. Generative selection and iterative agent
reasoning belong outside AIRK.

### 4. Treat freshness as a user-visible capability, not a vague promise

Copilot searches remote and local-change paths in parallel and replaces remote
hits for changed files when local results exist. If the local path times out, it
can retain remote results with an accuracy warning. In this snapshot the local
search timeout is 15 seconds and diff-size admission is explicitly bounded.
These are concrete fallback policies, not guaranteed freshness. [C2]

AIRK has a different advantage: stale hashes and indexed revisions can be named.
**Proposed:** improve the explanation of what is indexed, what changed on disk,
and what action restores coverage, using existing coverage/service observations.
Keep retrieval reads free of indexing side effects. An unsaved-buffer overlay
would be a separate future contract: editor-owned, explicitly opt-in, separately
identified and never silently mixed into a saved-source publication. It is not a
routine change to AIRK search. [A8] [A9]

### 5. Improve recovery guidance without silently changing the question

Copilot's text tool validates inputs, bounds result counts, supplies timeout
hints and can try another regex interpretation. Its codebase tool names an
unavailable semantic index rather than claiming that no relevant code exists.
[C7] [C9]

**Proposed:** distinguish no matches from missing coverage, excluded scope,
unsupported extraction, unavailable vectors and exhausted budgets in every AIRK
surface. Offer concrete next reads, such as `coverage`, a narrower prefix or an
explicit lexical query. Do not silently broaden the configured source, search
ignored files, or reinterpret a literal query as regex. Suggestions can remain
inert until the caller selects them.

### 6. Retain AIRK's semantic-unit design; test its remaining tradeoffs

Both systems recognize that embedding input size and context matter. AIRK already
has token-aware planning, bounded source context and separate citation membership.
Its tests assert tail preservation, token bounds, UTF-8/CRLF handling and native
location boundaries. Replacing it with the copied naive chunker would discard
those properties. [A11] [C10]

**Proposed:** compare existing unit sizes/context allowances on original fixtures
covering long functions, sibling definitions, identifier-heavy code, tables and
mixed code/document queries. Include the cost of indexing and repeated context.
Do not infer a superior embedding model from vector dimensions or a handful of
successful examples. The pinned MiniLM catalog itself records qualification
limits. [A7]

### 7. Reuse lifecycle patterns, not cloud infrastructure

Copilot overlaps query embedding with other preparation, groups/caches embedding
work and invalidates its workspace cache when the embedding model or schema
changes. These are useful engineering patterns. AIRK already shares model work,
binds vectors to a pipeline identity and supports incremental publication; it
should first locate duplicated or serialized work through measurements. [C1]
[C4] [A1] [A5] [A15]

**Proposed:** measure query embedding, candidate retrieval, authenticated reads and
response materialization separately, then optimize the observed bottleneck.
Preserve worker admission, finite queues, cancellation and revision checks.
Network quota throttling and hosted repository ingestion are not features AIRK
needs to import.

### 8. Build a relevance evaluation that complements contract tests

Copilot records where model-selected chunks appear in an existing ranking. That
helps diagnose disagreement, but an LLM's choice is not ground truth. The copied
`ranking.spec.ts` tests `combineRankingInsights`, not the complete combined-ranking
policy. The Search view scenario suite explicitly returns without running cases
when no input path is supplied. It must not be counted as evidence of default
end-to-end coverage. [C13] [C14]

AIRK's tests demonstrate detailed intended contracts, including pre-limit metadata
filters and stale-token rejection. They do not prove superior answers to human
search questions. **Proposed:** add a small original, versioned relevance corpus
with expected evidence ranges, useful negative queries and human review of
ambiguous relevance. Keep it separate from integrity and transport tests. [A13]
[A15]

## Where AIRK is better, and where it is weaker

### Demonstrable advantages for AIRK's intended use

1. **Offline operation and controllable data flow.** Native retrieval does not
   require sending source passages to a cloud model or repository search service.
   This is an architectural advantage when offline availability or keeping evidence
   processing local is a requirement. It does not imply that evidence later passed
   to an external coding agent stays local. [A5] [A7]
2. **A stronger public evidence contract in the reviewed paths.** Original
   locations, source hashes, exact-current publication checks and conditional reads
   let a caller verify what evidence it received. Indexed identity does not certify
   that a live file is unchanged, and expired tokens are not historical reads.
   [A4] [A8] [A15]
3. **Broader native artifact handling.** Searchable source and extracted document
   evidence can share a collection while retaining format-specific coordinates.
   PDF support here concerns a text layer, not general image understanding or
   guaranteed OCR. [A12]
4. **Reuse across tools.** AIRK's evidence API is available to CLI, MCP and its
   Workbench, with shared handlers and complete response data. This is useful for
   multiple coding clients and investigations that do not need a chat model.
   Existing parity tests provide evidence of intent, not a fresh transport
   qualification result from this review. [A14]
5. **Explicit limits rather than implied completeness.** Readiness, extraction
   failures, result bounds and incomplete coverage can be inspected separately.
   This is valuable when someone must explain why evidence is missing. [A1] [A9]

### Present weaknesses or narrower capabilities

1. **Relevance selection is less expressive.** Stable-ID lexical ordering,
   rank fusion and local embeddings do not provide Copilot's query-conditioned
   generative selection or optional remote reranking. This establishes a mechanism
   gap, not a measured accuracy deficit. [A1] [A2] [C5] [C6]
2. **Less current editor context.** Saved-file indexing cannot answer from a change
   that exists only in an unsaved document. Copilot's editor representation and
   change tracker cover a broader interactive scenario. [A5] [C8]
3. **Less language-service assistance.** AIRK's textual references and native syntax
   facts are not interchangeable with compiler/language-server resolved usages.
   Copilot documents access to those editor capabilities. [A10] [C11]
4. **No hosted repository search.** AIRK does not search an arbitrary GitHub/ADO
   repository without local collection setup. Copilot's repository clients and
   `githubRepo` integration cover that use case. This is a deliberate scope
   difference, not a recommendation to add a cloud provider. [C2] [C11]
5. **Local operational cost and unproven quality at scale.** AIRK must provision
   models, extract files, maintain indexes and honor disk/CPU budgets on the user's
   machine. Distributed index reuse can spare Copilot clients some of that work.
   The copied clients cannot establish Copilot's backend costs, speed or reliability.
   AIRK's model/platform qualification limits also remain relevant. [A6] [A7] [C11]
6. **More work remains in search ergonomics.** AIRK offers a Workbench and a bounded
   request parser, but it intentionally does not autonomously choose tools,
   rewrite questions, follow references and generate answers as a Copilot agent
   can. Agent orchestration is a consumer responsibility, not missing kernel logic.
   [A5] [C11]

## Cautions from the Copilot code

These are reasons to study the design critically, not to assume its choices are
correct because they are upstream.

- **A diversity cap is not completeness.** `combinedRanking` reduces the selected
  set by file/chunk limits, while this provider completes with `limitHit: false`.
  AIRK should preserve explicit subset/truncation reporting if it adopts a similar
  presentation policy. This observation concerns this provider, not every Copilot
  search surface. [C5] [C6]
- **Normalization is not calibration.** Combining normalized similarity with
  binary model selections does not establish comparable probabilities. Constant
  score arrays are normalized to ones in this implementation. Any AIRK alternative
  needs tests for ties, empty selections and ranking stability. [C6]
- **Model-proposed locations still need validation.** The provider parses generated
  JSON into a typed array and matches file suffixes plus query text. AIRK should
  use validated citation identities and bounded schema checks if an external
  consumer proposes selections; model output must never confer source authority.
  [C5] [C6] [A8]
- **Additional work is not automatically a better result.** The Search view's
  second reranking search feeds telemetry after reporting results. Measure whether
  an extra stage improves delivered evidence before paying its compute cost.
  [C5]
- **A cache is not a durable evidence protocol.** Copilot's SQLite embedding cache
  and repository commit tracking are useful, but do not by themselves provide the
  revision-bound read semantics AIRK requires. AIRK should optimize its existing
  storage ownership rather than introducing a parallel cache/index authority.
  [C4] [A5] [A8]

## Suggested AIRK priorities

All items below are proposals. None is implemented by this research change.

| Priority | Original AIRK work | Success evidence and constraints |
| --- | --- | --- |
| P0 | Establish a compact relevance baseline and diagnose the current lexical candidate order. | Versioned original queries and expected evidence; separate lexical/vector/hybrid Recall@k and MRR or nDCG; no change to source authorization or exact identity. |
| P1 | Improve lexical ranking and evaluate optional file diversity. | Better relevance on the same corpus and byte/result budgets, with stable ties and no regression in narrow-scope recall. Explicit inputs and full-result parity across CLI, MCP and Workbench. |
| P1 | Explain missing evidence and fallback choices consistently. | Contract tests distinguish zero matches, missing coverage, exclusions, pending vectors and truncation; complete details survive exports and copies. |
| P2 | Tune existing semantic units and investigate bounded local reranking only if baseline results justify it. | Relevance gain measured against added latency, model size, memory and indexing work; preserve native document boundaries and offline operation. |
| Separate design | Consider editor-buffer evidence only if users need it. | Separate source/version identity and explicit opt-in; saved-source publications and configured collection scope remain authoritative. |

For a fair comparison with a running Copilot installation, use the same source
revision, relevant file set and human questions. Record the deployed Copilot
version, policy, account/index availability and whether results include unsaved
text. Run code-only and mixed-document tracks separately: a PDF extraction
advantage must not be reported as a code-ranking victory. Record cold and warm
latency, coverage/fallback state and result payload sizes, and label missing
service measurements as unavailable. Do not infer backend token costs from
client snippets alone.

Keep deterministic safety regressions separate from relevance scoring: expired
revision rejection, no cross-collection evidence, filters before top-k, correct
UTF-8/native locations, byte bounds and transport parity must remain passing
requirements even if a new ranker looks better. Review the complete diff and
validation evidence before any eventual implementation push.

## Validation of this research artifact

The analysis changes no AIRK implementation and reuses no Copilot implementation
code. The ignored research snapshot remains local and unchanged. Links below
point either to AIRK files or to immutable upstream source/documentation URLs.
Validation for this artifact passed: repository checks, Rust formatting,
whitespace checks, all 30 evidence-reference targets, and hashes/byte counts for
all 79 copied research files. An explicit final review checked the conclusions
against the cited paths; this was not an independent review. Native Clippy and
workspace tests were not rerun for this documentation-only change.

The source assertions and tests were inspected; no new retrieval, performance,
security or cross-platform success is claimed. The earlier capture attempt's
Clippy/workspace-test limitation (missing bundled INT8 assets) is historical;
this documentation review does not turn it into a passing result.

## Evidence index

AIRK links are relative to this report and refer to the reviewed worktree. Upstream
links pin the two Copilot research revisions; they do not follow a moving `main`.

[A1]: ../../crates/airk-kernel/src/current/search.rs
[A2]: ../../crates/airk-store/src/current/lexical.rs
[A3]: ../../crates/airk-ingest/src/chunking.rs
[A4]: ../../crates/airk-core/src/lib.rs
[A5]: ../architecture.md
[A6]: ../../crates/airk-retrieval/src/vector/mutable.rs
[A7]: ../../crates/airk-retrieval/src/model_catalog.json
[A8]: ../../crates/airk-kernel/src/current/conditional.rs
[A9]: ../../crates/airk-kernel/tests/integration/coverage.rs
[A10]: ../../crates/airk-core/src/references.rs
[A11]: ../../crates/airk-retrieval/src/vector_units.rs
[A12]: ../../crates/airk-ingest/src/documents.rs
[A13]: ../../crates/airk-kernel/tests/search_filters.rs
[A14]: ../../crates/airk-mcp/src/workbench.rs
[A15]: ../../crates/airk-kernel/tests/current_runtime.rs
[C1]: https://github.com/microsoft/vscode-copilot-chat/blob/5863f5a7088958050792b5dccbe8b46c6e13eccc/src/platform/workspaceChunkSearch/node/workspaceChunkSearchService.ts
[C2]: https://github.com/microsoft/vscode-copilot-chat/blob/5863f5a7088958050792b5dccbe8b46c6e13eccc/src/platform/workspaceChunkSearch/node/codeSearch/codeSearchChunkSearch.ts
[C3]: https://github.com/microsoft/vscode-copilot-chat/blob/5863f5a7088958050792b5dccbe8b46c6e13eccc/src/platform/chunking/common/chunkingEndpointClientImpl.ts
[C4]: https://github.com/microsoft/vscode-copilot-chat/blob/5863f5a7088958050792b5dccbe8b46c6e13eccc/src/platform/workspaceChunkSearch/node/workspaceChunkAndEmbeddingCache.ts
[C5]: https://github.com/microsoft/vscode-copilot-chat/blob/5863f5a7088958050792b5dccbe8b46c6e13eccc/src/extension/workspaceSemanticSearch/node/semanticSearchTextSearchProvider.ts
[C6]: https://github.com/microsoft/vscode-copilot-chat/blob/5863f5a7088958050792b5dccbe8b46c6e13eccc/src/extension/workspaceSemanticSearch/node/combinedRank.ts
[C7]: https://github.com/microsoft/vscode-copilot-chat/blob/5863f5a7088958050792b5dccbe8b46c6e13eccc/src/extension/tools/node/codebaseTool.tsx
[C8]: https://github.com/microsoft/vscode-copilot-chat/blob/5863f5a7088958050792b5dccbe8b46c6e13eccc/src/platform/workspaceChunkSearch/node/workspaceFileIndex.ts
[C9]: https://github.com/microsoft/vscode-copilot-chat/blob/5863f5a7088958050792b5dccbe8b46c6e13eccc/src/extension/tools/node/findTextInFilesTool.tsx
[C10]: https://github.com/microsoft/vscode-copilot-chat/blob/5863f5a7088958050792b5dccbe8b46c6e13eccc/src/platform/chunking/node/naiveChunker.ts
[C11]: https://github.com/microsoft/vscode-docs/blob/8f7dd621e3f8e2450d9048455d787688f2de5bf3/docs/agents/reference/workspace-context.md
[C12]: https://github.com/microsoft/vscode-copilot-chat/blob/5863f5a7088958050792b5dccbe8b46c6e13eccc/src/platform/remoteSearch/node/codeOrDocsSearchClientImpl.ts
[C13]: https://github.com/microsoft/vscode-copilot-chat/blob/5863f5a7088958050792b5dccbe8b46c6e13eccc/src/extension/workspaceSemanticSearch/node/test/ranking.spec.ts
[C14]: https://github.com/microsoft/vscode-copilot-chat/blob/5863f5a7088958050792b5dccbe8b46c6e13eccc/test/e2e/semanticSearchView.stest.ts
[C15]: https://github.com/microsoft/vscode-docs/blob/8f7dd621e3f8e2450d9048455d787688f2de5bf3/docs/editing/copilot-smart-actions.md#semantic-search-results-preview
