# AIRK feature review

**8 November 2026.** This is a review of the unpublished **1.8.14** working tree
on branch `feat/finetuning`, which sits on commit `f54fb4d` and has uncommitted
changes. It is not a release record.

It combines three sources:

- [One AIRK](one-airk.md) and the [One AIRK completion plan](one-airk-completion-plan.md);
- the current [architecture](architecture.md), [README](../README.md) and the
  feature references linked below;
- a direct reading of the source in `crates/` and `packages/vscode/`.

Every important code claim was checked against the live files. Where a document
and the code disagree, this review follows the code and lists the disagreement in
[section 19](#19-inconsistencies-found-during-this-review).

## How to read this review

AIRK keeps four facts separate:

- what is implemented;
- what has passed a local check;
- what has been qualified on an installed platform;
- what has been published.

This review does the same. Each feature carries one of these labels:

| Label | Meaning |
| --- | --- |
| **Implemented** | Production code and unit or integration tests exist in the working tree |
| **Locally checked** | Also passed a recorded local gate, cited from the [implementation ledger](research/one-airk-implementation.md) or a release record |
| **Unqualified** | Implemented, but installed-platform, client or release acceptance is still pending |
| **Library only** | Implemented and tested in a crate, but not reachable from the current CLI, MCP or extension |
| **Planned / partial** | Designed or scaffolded, with limits the code itself declares |

Nothing in 1.8.14 is release-qualified yet. The latest published stable release is
**1.7.6**. See [section 18](#18-qualification-state-and-known-gaps).

---

## 1. What AIRK is, and what it is not

AIRK (Artifact Indexing and Retrieval Kernel) builds a **local, derived index** of
a project's code and documents. It serves **bounded, cited, revision-identified
evidence** to:

- coding agents, over MCP;
- local tools, over a JSON HTTPS API;
- people, through the CLI and the VS Code Explorer and Workbench.

Its scope is limited on purpose, and that limit is part of the design:

- **No reasoning or generation.** AIRK does not answer questions, write code,
  orchestrate agents or route model calls. The Workbench "uses no generative AI".
  The only learned component is a small intent classifier, and its output is a
  proposal that a person must review before it runs.
- **Local only.** There is no cloud provider, no API key, no external database and
  no network telemetry. The MiniLM embedding model is compiled into the executable,
  and nothing is downloaded at runtime.
- **Sources are authoritative and read-only.** All index state is derived from the
  source files. Discovery, extraction, verification and every provider path read
  source files without following links and never write to them. The LSP client
  even refuses `workspace/applyEdit`
  ([`providers_lsp.rs`](../crates/airk-ingest/src/providers/lsp.rs)).
- **No claim of understanding.** Calls, references and imports are reported as
  *written candidates*. AIRK does not claim resolved references, a complete
  understanding of any language, or semantic search in the
  understanding-the-meaning sense.

What makes AIRK distinctive is less any single feature than the **evidence
discipline** that runs through all of them. Every result can be traced to an
exact publication. Every limit is disclosed. Every uncertain outcome stays
uncertain rather than being rounded to success.

## 2. Distinctive features at a glance

| # | Feature | Status |
| --- | --- | --- |
| 1 | Exact revision tokens bound to every result, cursor and conditional read | Implemented |
| 2 | Saved-source verification (`off` / `report` / `require`) with dated receipts | Implemented, locally checked (P4) |
| 3 | Typed refusals and evidence guidance; unknown stays unknown | Implemented |
| 4 | About 100 native extraction adapters over 56 pinned tree-sitter grammars, plus custom Office, PDF and specialist parsers | Implemented |
| 5 | Composite documents (HTML, Vue, MDX, notebooks, Razor and others) with mapped original coordinates | Implemented |
| 6 | Crash-safe incremental publication: SQLite FTS5 plus a mutable DiskANN provider in one journaled head | Implemented |
| 7 | Embedded INT8 MiniLM profiles chosen by CPU feature, with exact pipeline identity | Implemented; ORT 1.22.1 static smoke passed on Windows debug r26 |
| 8 | Hybrid retrieval by equal-weight reciprocal rank fusion, with requested and served mode disclosed | Implemented |
| 9 | Combined reads across up to 32 indexes with shared, non-multiplied budgets | Implemented, locally checked (P7) |
| 10 | One 29-tool MCP catalog shared by MCP, the CLI and the HTTPS JSON API | Implemented |
| 11 | Terse/Full response modes and Lean/Rich reply detail that never change what is indexed | Implemented |
| 12 | Read-only evidence planner (`parse`) with a deterministic parser and a review-only learned classifier | Implemented |
| 13 | CLI, Workbench, MCP and HTTPS parity with complete results | Implemented; full transport parity unqualified |
| 14 | One per-user service, one executable, in-process inference and truthful completion proofs | Implemented, unqualified |
| 15 | Loopback HTTPS with a per-installation CA, verifier-only bearer credentials and strict Host/Origin rules | Implemented, unqualified |
| 16 | Endpoint relocation when the port is taken, without probing or killing the other listener | Implemented, unqualified |
| 17 | Checkpointed upgrade through a digest-chained journal and an immutable checkpoint manifest | Implemented, unqualified |
| 18 | Ownership-proven predecessor migration and cleanup | Implemented; Windows predecessor cases passed |
| 19 | Reviewed catalog management: index sets, grants, permanent exclusions, splitting, retirement | Implemented, locally checked (P6f–P11b) |
| 20 | VS Code extension with sealed hot-swap updates, preimage-checked config edits and a phased issuer rotation flow | Implemented, unqualified |
| 21 | Content-free local usage ledger with measured, not invented, savings | Implemented |

---

## 3. The evidence integrity model

This section describes AIRK's central contract. Most later features depend on it.

### 3.1 Revision tokens and framed digests

Every published index state has a `RevisionToken` of the form
`airk-r1:<incarnation>:<generation>:<sha256>`
([`identity.rs:76`](../crates/airk-core/src/identity.rs)).

- **Parsing is strict.** A token is at most 126 bytes, and the generation must be
  written in canonical decimal.
- **`validate_against` refuses three kinds of mismatch:**
  - a token from another index incarnation;
  - a generation from the future;
  - a fork, meaning the same generation with a different revision.

  It explicitly does *not* claim to prove ancestry.
- **Where tokens come from.** In the store, the current head (artifact root,
  lexical commit, graph head) hashes to a `CurrentToken`.
- **Stale tokens fail.** A read that names an old token is refused with "current
  revision expired; repeat the request"
  ([`current/read.rs`](../crates/airk-store/src/current/read.rs)). There is no
  silent fallback to whatever happens to be current.

All identities use one domain-separated hash, `framed_digest`. It is SHA-256 over
the prefix `AIRK-DIGEST-1\0`, a domain name and length-framed fields. A test checks
it against an independent .NET implementation. Because each kind of digest has its
own domain, a digest made for one purpose cannot be passed off as another.

### 3.2 Conditional reads

Evidence reads accept these options:

- `revision`
- `if_evidence`, a representation validator returned by an earlier read
- `known_hash`, a remembered source hash
- `follow_moves`
- `artifact_id`

The kernel computes a separate validator for each representation. It covers the
payload, evidence, profile, root, path, selection and representation options.
When `if_evidence` still matches, the payload is left out and the status is
`NotModified` ([`current/conditional.rs`](../crates/airk-kernel/src/current/conditional.rs)).
The other read statuses are `MoveCandidate`, `RemovedFromIndex`, `Ambiguous`,
`HistoryUnavailable` and `Unknown`.

`known_hash` validates **source bytes only**. It is documented as never standing
in for an evidence validator. An agent that has already seen a passage can
therefore confirm it is unchanged without paying for the bytes again, and can
never confuse "the file is the same" with "the extracted evidence is the same".

### 3.3 Saved-source verification

Every evidence read accepts a `verification` mode (`off`, `report` by default, or
`require`) and three budgets
([`airk-core/src/verification.rs`](../crates/airk-core/src/verification.rs)):

| Budget | Default | Maximum |
| --- | --- | --- |
| Sources | 8 | 256 |
| Bytes | 16 MiB | 1 GiB |
| Deadline | 2 s | 30 s |

The method, `sha256-full-saved-source/1`, re-hashes the saved bytes behind each
cited file. It does this read-only and without following links
([`airk-ingest/src/source_read.rs`](../crates/airk-ingest/src/source_read.rs)).

Each file gets one of these states:

- `Matched`
- `Mismatch`
- `Missing`
- `Unreadable`
- `OutOfScope`
- `ChangedDuringCheck`
- `BudgetExhausted`
- `PublicationExpired`
- `Unsupported`

The receipt records when the check happened, the publication fence and the limits
that were hit.

The published catalog states what verification does **not** cover:

- it checks saved bytes only;
- it does not certify extraction;
- it does not look at unsaved editor buffers;
- it does not certify the whole index.

Two failure paths are worth noting:

- **`require` mode fails cleanly.** If the check cannot pass, the result is the
  non-retryable `verification_required_unmet`, and the attempted evidence is still
  returned beside it.
- **An oversized receipt is refused.** If the receipt cannot fit in the reply, the
  result is a typed refusal with `verification_performed: false`.

### 3.4 Continuations are tied to the request

A continuation token only works for the exact request that produced it.

- Discovery cursors (`airk-d1:`) bind the revision, the tool and a digest of the
  filters, and carry a checksum
  ([`discovery.rs`](../crates/airk-core/src/discovery.rs)).
- Change cursors bind the target and baseline tokens, the request digest and a
  staged position ([`changes.rs`](../crates/airk-core/src/changes.rs)).

Changing a filter, the representation or the head expires the cursor. The code
describes cursors as "consistency selectors, not authorization tokens", so a
cursor can never be used to widen scope.

### 3.5 Typed refusals, evidence guidance, and unknown stays unknown

Every read that fails returns a typed receipt:

- **Argument errors** carry `{code, field, message, retryable}`.
- **Admission refusals** carry `query_submitted: false`, a `retry_after_ms` hint
  and `fresh_brokerage_required`.
- **Uncertain catalog writes** are reported with the note "Missing observation is
  not evidence that no effects occurred".

Results can also carry [evidence guidance](evidence-guidance.md)
([`guidance.rs`](../crates/airk-core/src/guidance.rs)). This is inert advice built
only from metadata already in the response, at most eight observations. Each
observation has a reason, a subject, a basis, a message and a suggested next read.

| Reasons | Next reads |
| --- | --- |
| `unsupported`, `unavailable`, `not_retained`, `reply_omitted`, `pending`, `failed`, `stale`, `excluded`, `truncated`, `unknown` | `InspectCapabilities`, `InspectFileInfo`, `InspectSourceStatus`, `InspectSavedPolicy`, `RequestRich`, `ContinuePage`, `ReadSelectedPassage` |

Guidance is declared `starts_work: false`. An empty result is labelled `unknown`
and is never explained by a guessed cause.

### 3.6 Indexed text is treated as untrusted

Returned document text, including agent instruction files, is treated as data.
The MCP server instructions start with "Untrusted index; verify live bytes before
edits". `AGENTS.md`, `CLAUDE.md` and `.github/copilot-instructions.md` have their
own adapter. It records written constraints as *untrusted* cues, and never follows
the instructions or the literal paths they mention
([`instruction_documents.rs`](../crates/airk-ingest/src/instruction_documents.rs)).

---

## 4. Discovery and source safety

**Status: implemented.**

- **Ignore rules.** Discovery uses the `ignore` crate with `follow_links(false)`
  ([`source_scope.rs`](../crates/airk-ingest/src/source_scope.rs)). It honours
  `.gitignore`, `.ignore`, `.airkignore`, `.git/info/exclude` and the Git
  global/system exclude files.
- **Ignore files are read like sources.** They go through the same protected
  no-follow reader, with bounds of 64 KiB per file, 1 MiB per snapshot and 32,768
  cached directories. A control file that is ambiguous or redirected is refused
  rather than guessed
  ([`policy_ignore.rs`](../crates/airk-ingest/src/policy_ignore.rs)).
- **Hard exclusions** apply to every path component and cannot be re-included
  ([`lib.rs` `excluded`](../crates/airk-ingest/src/lib.rs)): `.git`, `.airk`,
  `node_modules`, `target`, `.venv`, `__pycache__`, `.aws`, `.ssh`, `.env`,
  `.env.*`, `*.pem` and `*.key`.
- **Credential defaults** can be re-included with `!` in `.airkignore`: `.npmrc`,
  `.netrc`, SSH private keys, `*.pfx`, `*.jks`, kubeconfig, `*.tfstate`,
  `.docker/config.json` and others. The code calls this "a convenience default,
  not secret detection".
- **[Permanent exclusions](source-policy.md)** are deny rules owned by one index.
  - They take precedence over includes, ignore negation and grants.
  - On Windows and macOS, any uncertainty about Unicode or case-folded names
    counts as a refusal ("Unknown Unicode collation is never proof of disjoint
    physical names").
  - Applying a deny removes the indexed evidence through a resumable publication.
    The source files are untouched.
- **Isolation.** Each project has a canonical source root, its own data root and
  its own collection. Sources cannot overlap registered data, runtime state,
  control or log directories, and storage is reserved before it is created.
- **Priority ordering.** Agent-relevant files are processed first: README,
  `AGENTS.md`, `CLAUDE.md`, docs, then manifests. Vendor, generated and fixture
  files come last. Every eligible file is still visited
  ([`priority.rs`](../crates/airk-ingest/src/priority.rs)).
- **Watching.** File-system notifications are treated only as *hints*. Durable
  intents and periodic reconciliation decide what is indexed. A separate
  supervised source-watcher process isolates stalls in the file system.

## 5. Native extraction

**Status: implemented.** Platform qualification for some adapter families is
recorded separately in the [v2 ledger](architecture-v2-ledger.md).

### 5.1 The adapter registry

Extraction is built on **tree-sitter 0.25.10 with 56 grammar crates pinned to
exact versions** ([`airk-ingest/Cargo.toml`](../crates/airk-ingest/Cargo.toml)),
plus custom parsers where no suitable grammar exists. The baseline registry
([`adapters/registry.rs`](../crates/airk-ingest/src/adapters/registry.rs)) has
about 80 grammar-backed adapter IDs and about two dozen custom ones.

Each adapter records:

- its exact `grammar_identity`;
- a capability matrix: definitions, imports, call candidates, structural facts,
  embedded regions and continuity;
- its written limitations.

The adapter and dialect choice are part of the file's effective profile identity.
The full matrix is in [formats](formats.md).

### 5.2 Coverage

| Group | Examples |
| --- | --- |
| General code (about 30) | Rust, Python, JS/TS/TSX, Go, Java, C, C++, C#, Ruby, PHP, Bash, PowerShell, Lua, Swift, Kotlin, Dart, Scala, Elixir, Objective-C/C++, R, Perl, Groovy, Nix, Zig, Julia, Clojure, OCaml, Haskell, Erlang, F#, SQL (generic, PostgreSQL, MySQL) |
| Specialist | Fortran (fixed and free), COBOL (fixed grammar and a custom free-form adapter), VB.NET, Delphi, PL/SQL, T-SQL, ABAP, Apex, Solidity, VHDL, Verilog/SystemVerilog, CUDA, WGSL, GLSL, Assembly (generic, x86 AT&T and Intel, AArch64) |
| Repository and build | HCL, Dockerfile, Protobuf, GraphQL, CMake, Make, MSBuild/`.sln`, GitHub Actions, Compose, Bazel/Starlark, Gradle (Kotlin and Groovy), Kubernetes, OpenAPI, GitLab CI, finite manifests (`package.json`, `Cargo.toml`, `pyproject.toml` and others) |
| Structured text | Markdown, JSON/JSONC, YAML, TOML, XML/SVG, CSV/TSV, INI, CSS/SCSS |
| Composite hosts | HTML, Vue, Svelte, Astro, MDX, Razor/Blazor, Jupyter, R Markdown, literate Haskell |
| Office and PDF | DOCX, XLSX, PPTX (read in memory with zip and quick-xml; external relationships and DTDs refused), PDF text layer |

### 5.3 How the design differs from typical indexers

- **Nothing is executed or evaluated:**
  - no build files are run;
  - no macros or preprocessors are expanded;
  - no spreadsheet formulas are recalculated (stored results are labelled
    separately);
  - no notebook cells are run;
  - no shader is validated;
  - no linked resource is fetched.

  Module headers state this. For example, the Actions adapter says "action
  references and run bodies are never executed".
- **Ambiguity is settled by evidence or refused.**
  - `.h` files default to C. C++ is chosen only from an explicit `-x c++` in a
    declared `compile_commands.json`, and conflicting entries are refused
    ([`header_context.rs`](../crates/airk-ingest/src/header_context.rs)).
  - Objective-C headers are recognised from a masked 4 KiB prefix.
  - `.cls`, `.v` and `.pl` need a strong visible marker or an explicit policy.
- **Composite documents keep their original coordinates.** Child regions, such as
  the script in a Vue file or a cell in a notebook, are parsed with their own
  grammar. Their facts map back to `DocumentLocation::Mapped` spans in the host
  file, and child budgets cannot reset the host's shared budget
  ([`composite.rs`](../crates/airk-ingest/src/composite.rs)).
- **Damaged syntax is suppressed, not guessed.** A node that contains a parse
  error never becomes a fact, and the file is marked `partial-syntax`. PHP, Ruby
  and SQL get conservative repairs over the same tree.
- **Citations point to the original structure.** Locations can be lines, PDF
  pages, DOCX blocks, PPTX slides, table rows or worksheet cells. Table rows keep
  their cell coordinates, and numbers and units do not run across cell boundaries.

### 5.4 Isolation and limits

- **PDF parsing runs in a separate worker.** The AIRK binary re-executes itself
  with a private argument, and the worker runs `pdf-extract` on a temporary copy
  ([`pdf_worker.rs`](../crates/airk-ingest/src/pdf_worker.rs)). Its limits are
  512 MiB of memory, 10 CPU-seconds, a 15 s wall clock and 64 MiB of output. On
  Windows it is contained by a Job Object; on Unix by a private process group with
  `RLIMIT_AS` and `RLIMIT_CPU`.
- **Default limits** ([`ScanLimits`](../crates/airk-ingest/src/lib.rs)):

  | Limit | Default |
  | --- | --- |
  | Per file | 256 MiB |
  | Corpus | One-eighth of physical memory, capped at 4 GiB |
  | Files | 100,000 |
  | Discovery entries | 1,000,000 |

  Saved per-collection overrides can raise these within validated ceilings. On
  Linux the memory figure follows cgroup limits.
- **Per-file parse budget.** Each file may visit at most 500,000 syntax nodes and
  produce at most 10,000 facts. The tree is walked iteratively, so deep nesting
  cannot overflow the stack. A UTF-8 file over 4 MiB keeps its exact text but
  produces no structural facts.
- **Bounded extraction pool.** One to four workers run with a 16 MiB in-flight
  allowance and deliver results in source order. A worker panic is caught and the
  previous generation is kept.

### 5.5 Detail levels

Collections choose one of four levels of retained detail:

- **Search Only** (`essential`)
- **Agent Lean** (`agent_lean`, the default for new collections)
- **Rich Structure** (`balanced`)
- **Detailed**

[Retention profiles](retention.md) choose which fact families are kept per file
type. The semantic-unit projection is separate from the retained detail, so a
lean index can still embed well-bounded declaration and heading units.

**Opt-in per-file family reporting**
([profile families](profile-family-reporting.md)) reports 18 fact families. For
each one it gives three values: whether the adapter supports it, whether it was
retained, and whether it was included in this reply. It does not rescan the
source.

## 6. Code and document facts, reported as candidates

**Status: implemented.**

- **Fact kinds.** `definition`, `import`, `key`, `call` (the written call-target
  text), `exception_handler` and `inherits`
  ([`syntax.rs`](../crates/airk-ingest/src/syntax.rs)). Qualified names, scopes,
  signatures, modifiers and comments are kept, each capped at 1 KiB.
- **Relationships.** Call and inheritance edges are exposed as
  `syntax_candidate`. Written targets such as `db.query` are never resolved to a
  symbol ID.
- **References** are textual FTS5 occurrence candidates. They are paged at 100 per
  page under a 250 ms cooperative budget and tied to one publication
  ([`references.rs`](../crates/airk-kernel/src/references.rs)).
- **Framework cues** are fixed rules for:
  - route registration;
  - JS test registration;
  - database calls;
  - Rust, Java and C# test, route and injection attributes.

  Each cue is labelled `StructuralRule` with the coverage text "framework binding,
  security and runtime behavior unverified".
- **Document facts** come from keyword and pattern rules in
  [`prose.rs`](../crates/airk-ingest/src/prose.rs):
  - sections, both explicit and heuristic headings;
  - dates, normalised to ISO;
  - requirements, from modal verbs, ID labels and numeric bounds with units;
  - architecture, design and feature statements, including user stories;
  - decisions;
  - given/when/then acceptance criteria.

  The module states: "Classification never proves an obligation or design."
- **Continuity across edits** ([`continuity/`](../crates/airk-ingest/src/continuity),
  [`reconciliation.rs`](../crates/airk-kernel/src/reconciliation.rs)):
  - **Files** are matched by source hash, profile and size. A move is claimed only
    when the match is unique; ambiguous content stays `Unknown`.
  - **Declarations and document statements** are matched in passes: explicit
    identity, then name, then signature hash, then body hash.
  - **Change dimensions** are recorded separately: path, content, name, signature,
    body, documentation, statement, quantity, date and modality.
  - **Similarity suggestions** (shingles, at least 80%) are optional and "never
    assign identity".

**Library only.** The kernel has three more paths, all integration-tested in
[`code_graph.rs`](../crates/airk-kernel/tests/integration/code_graph.rs) and
[`limits.rs`](../crates/airk-kernel/tests/integration/limits.rs):

- **SCIP import** (`Kernel::import_scip`), labelled "hash-bound, not
  compiler-certified";
- **bounded LSP 3.17 and universal-ctags providers** (`Kernel::run_provider`),
  which run on a private workspace copy and are rejected if sources change during
  the run;
- **local Tesseract OCR** (`extract_ocr`).

Nothing in `airk-cli` or `airk-mcp` calls any of them today (see
[section 19](#19-inconsistencies-found-during-this-review)).

## 7. Storage and crash-safe publication

**Status: implemented.** Power-loss durability is not qualified on any platform.

- **Content-addressed objects.** Sources and extractions are immutable
  content-addressed objects (SHA-256) in the derived store. They are compressed
  with LZ4 in independently checked 1 MiB blocks.
- **One current head per collection.** The head binds three things: the artifact
  map, the SQLite FTS5 lexical commit, and the DiskANN graph head.
- **Journaled publication**
  ([`current/journal.rs`](../crates/airk-store/src/current/journal.rs)). The
  journal moves through `Ready`, `Prepared` and `Applying`.
  - **Prepare** applies the captured lexical changes inside a transaction, seals
    the FTS5 shadow rows, and rolls back.
  - **Apply** repeats the same changes. It must produce an identical seal before
    lexical, map, graph and head rows commit together.
  - **Recovery** replays only the captured inputs, never the live sources.

  The cost of doing the incremental work twice has not been measured.
- **Readers stay consistent during publication.** Readers hold shared request
  gates and object leases for the length of a request (120 s for ordinary
  snapshots). A prepared publication leaves the previous head readable.
- **Atomic file replacement.** The file is fsynced, renamed (on Windows with
  `MoveFileExW` `REPLACE_EXISTING | WRITE_THROUGH`), and then the parent directory
  is synced ([`durability.rs`](../crates/airk-store/src/durability.rs)).
- **Space accounting.** Quotas are logical reservations that survive a process
  crash; they are not OS quotas. WAL and shared-memory allowances scale with the
  admitted database image.
- **[Storage formats](storage-formats.md)** carry their own version identities,
  independent of release versions. A new format is adopted by rebuilding derived
  state, never by silently migrating old evidence.
- **Integrity checks.** The store verifies authenticated FTS5 shadow tables, runs
  SQLite `integrity-check`, and offers resumable physical Verify and Repair jobs.
- **Relocation** of the data root is explicit and runs only when the data is
  quiescent.
- **No history.** Historical search is unsupported: "only the current index is
  retained". The `changes` tool keeps bounded metadata for up to 64 publications
  or 24 hours, so a remembered revision can still be validated.

## 8. Retrieval

### 8.1 Lexical search: SQLite FTS5 with deterministic boolean matching

**Status: implemented.**

- **Engine.** `sqlite-fts5/current-lexical-1` with `detail=none` and
  `columnsize=0` ([`current/lexical.rs`](../crates/airk-store/src/current/lexical.rs)).
  It replaced Tantivy in 1.3.0
  ([decision](index-sharding-review.md#decision-replace-tantivy-with-sqlite-fts5)).
- **Query terms.** A query term expands to the whole identifier plus its
  underscore and camelCase parts, lowercased, with no stemming. Terms are joined
  with AND.
- **Ordering.** Results are ordered by `stable_id`, a hash of path and chunk, not
  by relevance score. The MCP `search` description says so: "Lexical candidates
  use stable-id order, not relevance ranking."

The ordering choice is deterministic, reproducible and honestly disclosed, but it
means lexical-only rank carries no relevance signal. Scope filters are applied in
SQL before the limit, and every candidate's identity and citation are re-checked.

### 8.2 Vector search: Microsoft DiskANN through an authenticated provider

**Status: implemented; this is the production vector path.**

- **Provider.** `MutableDiskIndex`
  ([`vector/mutable.rs`](../crates/airk-retrieval/src/vector/mutable.rs)) runs
  `diskann` 0.59.0 over graph records that the store authenticates. The kernel
  reports its backend as `microsoft-diskann-0.59.0/airk-current-provider`.
- **Features:**
  - tombstones through per-node `live` flags;
  - one `SearchLimits` budget covering record reads (65,536), candidates (4,096),
    distance and adjacency reads;
  - refill passes;
  - a `partial` flag on results;
  - exact scoped `search_subset` for path-filtered queries.
- **What is not the engine.** The in-memory `LexicalIndex` and `MemoryStore` are
  bootstrap adapters only; no production path uses them. The older single-file
  `DiskIndex` survives only in tests.

### 8.3 Embedded MiniLM with exact pipeline identity

**Status: implemented.** The static ORT 1.22.1 CPU smoke test passed on the
Windows debug r26 build. Native INT8 platform qualification is pending.

- **Two bundled models.** Both are all-MiniLM-L6-v2 exported to INT8 ONNX, with
  384 dimensions, 256 tokens, mean pooling and L2 normalisation. SHA-256 is pinned
  for model, tokenizer and license
  ([`model_catalog.json`](../crates/airk-retrieval/src/model_catalog.json)).
- **CPU-based selection.** Unsigned INT8 (`minilm-l6-v2-u8-v1`) is used on x64
  without VNNI. Signed INT8 (`-s8-v1`) is used on x64 with VNNI and on ARM64.
- **One text per forward pass.** Dynamic quantisation therefore never depends on
  unrelated texts in a batch, which keeps exact-input caching sound.
- **Separate identities.** Model and pipeline digest, approved execution identity
  and provider incarnation are kept distinct
  ([One AIRK §6.1](one-airk.md#61-separate-model-identity-execution-identity-and-runtime-ownership)).
  Vectors are reused only when the full model, tokenizer, preprocessing,
  quantisation and dimensions all match.
- **No fallback vectors.** No vector is invented for unknown text, and nothing is
  downloaded.

### 8.4 Hybrid ranking and readiness policy

**Status: implemented.**

- **Fusion.** Hybrid search fuses lexical and vector branches with equal-weight
  reciprocal rank fusion, `airk.rank-fusion.rrf-equal/1`, scoring each item
  `1 / (61 + zero_based_rank)`. It deliberately ignores raw scores from either
  backend ([`combined_reads.rs`](../crates/airk-core/src/combined_reads.rs)). A
  passage that spans several chunks is emitted once.
- **Readiness states.** Semantic readiness is `Unavailable`, `Disabled`,
  `Pending`, `Partial`, `Ready` or `Failed`.
- **Saved search policy.** A collection chooses one of:
  - `PartialHybrid` (the default);
  - `LexicalWhilePending`;
  - `RequireComplete`.
- **Disclosure.** Every search records the requested mode, the mode actually
  served, and any `IncompleteReason` (helper unavailable, coverage incomplete,
  upgrade pending) ([search execution](search-execution.md)). A Default search can
  fall back to lexical during an inference outage and says so. An explicit vector
  or hybrid search, or `RequireComplete`, refuses instead.
- **[Vector selection](vector-selection.md)** rules keep lockfiles and minified
  assets out of the vector index while keeping their lexical and fact evidence.

### 8.5 Combined reads across indexes

**Status: implemented and locally checked (P7).** See
[combined reads](combined-reads.md).

- **Selection.** A reviewed grant can select up to 32 indexes. Each unique index
  runs once, even when several selected projects contain it.
- **Shared budgets.** Result count, bytes, time and verification allowances are
  split across members, never multiplied. A zero allocation means "not queried",
  not "zero hits".
- **No global head.** Members are ranked by RRF over each member's local rank. The
  result never claims a global corpus top-k or an atomic global head; the
  `PublicationVector` is per-member and time-bounded.
- **Member availability.** `member_availability` is `report` (keep partial
  results) or `require` (fail if any member cannot answer).
- **Complete member responses.** Each member's native response is kept, and the
  ranking points into it by JSON Pointer rather than copying evidence text.
- **Limits.** Combined search has no continuation yet.

## 9. Agent interface: one unified MCP catalog

**Status: implemented.**

### 9.1 The 29 tools

The catalog is built in [`unified.rs`](../crates/airk-mcp/src/unified.rs).

- **19 native tools** (`#[tool]` handlers in
  [`lib.rs`](../crates/airk-mcp/src/lib.rs)):
  - status and help: `status`, `diagnostics`, `capabilities`, `help`;
  - scope and planning: `query_scope`, `parse`;
  - discovery: `search`, `list_files`, `file_info`;
  - code navigation: `outline`, `symbols`, `references`, `imports`,
    `code_facts`, `relationships`;
  - documents: `document_facts`;
  - source reads: `read_file`, `context`;
  - change validation: `changes`.
- **10 Explorer and proposal tools** added by the unified layer:
  - drafts: `draft_search`, `draft_strategy`, `draft_plan`, `suggest_view`;
  - draft checks: `validate_draft`, `proposal_status`;
  - templates: `list_templates`, `get_template`;
  - handoff: `get_handoff`;
  - projects: `projects`.

The same catalog serves:

- stdio MCP (historical);
- HTTPS MCP;
- the HTTPS JSON API;
- `airk tools list/help/call/shell`.

Old `airk_`-prefixed names still work as aliases, and the JSON API publishes the
canonical name and its aliases. Every scoped tool needs an explicit 64-hex
`project` whenever a connection has no single default. It never follows the
Explorer's dropdown.

### 9.2 Response shaping that never changes what is indexed

These options are described in [response modes](mcp-response-modes.md) and
[reply detail](reply-detail.md).

- **Terse** (the default) sends status and readiness once per connection, and
  again only when they change or `include_status=true`. It replaces repeated
  publication tokens with a reply-local `{ref: publication}` and moves repeated
  provenance into a local table. **Full** restores everything for audits.
- **`reply_detail` lean/rich** chooses which *already retained* facts appear in a
  reply. Rich cannot recreate facts that were never retained, and a
  contradictory `compact` flag is rejected.
- **`status`** is a lightweight observation. It can be cached for 5 s and
  discloses `observed_unix_ms`, `cache_age_ms` and `cached`. Cold setup gets 10 s
  plus a 2 s observation budget, after which it reports unknown readiness with
  null evidence. **`diagnostics`** carries the heavy detail: process identities,
  jobs, watchers and sampled waits.

### 9.3 `parse`: a bounded, read-only evidence planner

[`parse`](../crates/airk-mcp/src/lib.rs) (`airk.parse.request/1`) runs in
`prepare` or `execute` mode. It takes exactly one of `text`, `recipe` or `plan`.

- **Plan limits.** A plan has at most 6 steps, 12 reads, a 30 s budget and
  512 KiB. Steps can bind selectors from earlier results, such as
  `unique_definition_path` or `hit_locations`.
- **Same handlers.** Inner reads re-enter the same MCP handlers under the outer
  admitted lifetime. They take no recursive transport and no second permit, and
  they share one usage record.
- **Recipes.** `definition`, `impact`, `tests`, `documents`, `component`,
  `readiness`.
- **Deterministic text parsing** handles slash commands and anchored English
  templates. These "neither answer questions nor infer runtime behavior"
  ([`workbench/natural.rs`](../crates/airk-kernel/src/workbench/natural.rs)).
- **Learned fallback.** When deterministic parsing gives up, a fixed **linear
  intent classifier** proposes a plan
  ([`intent.rs`](../crates/airk-retrieval/src/intent.rs)):
  - 65,536 hashed buckets × 13 labels with int8 weights, about an 850 KB embedded
    asset;
  - features from word n-grams, character 3–5-grams and identifier-shape flags;
  - no embeddings, no network and no text generation;
  - a prediction is accepted only above a logit threshold and a top-two margin.

  **A learned proposal never runs without an explicit plan submission.** A test
  enforces this. Training data, its catalog, attribution and a byte-identical
  replay receipt are public under [`tools/parser-train`](../tools/parser-train)
  ([parser training](parser-training.md)).
- **Never** writes, runs commands or nests plans.

### 9.4 Drafts for review, not actions

`draft_search`, `draft_strategy`, `draft_plan` and `suggest_view` put a draft
(at most 16 KiB, unknown fields refused) into the **Suggestions Inbox** of an
attached, trusted Explorer window. Nothing runs until the user applies it.

- **Inbox bounds.** The inbox is in memory only, with at most 50 pending drafts,
  256 KiB, 30 per minute, and a TTL.
- **Plain text only.** Agent text is rendered as plain text, never HTML.
- **Status.** `validate_draft` checks a draft without admitting it.
  `proposal_status` always reports `executed: false`.
- **Strategies.** Strategy drafts may use only a closed operator list, with no
  shell, write, network or recursion operators.
- **Handoff.** `get_handoff` returns a Studio prompt or Planner task only when the
  user shared it. The token is opaque, lasts 10 minutes and is bound to the
  binding, the draft revision and the publication.
- **Headless.** `airk explorer …` validates drafts against the same generated
  schemas with no editor running.

### 9.5 Admission and the single safe retry

- **Shared, configurable capacity.** Evidence concurrency is **4, 8, 16, 32 or
  64** (default 16), plus one reserved status/control slot. All sessions, CLI
  channels and project workers share it
  ([`service_resources.rs`](../crates/airk-core/src/service_resources.rs)).
- **Permits outlive cancellation.** An accepted request keeps its permit through
  transport cancellation until the native work really finishes.
- **One 50 ms retry.** A typed `selection_busy` refusal gets exactly one internal
  retry, at one canonical owner, within the original deadline. After the wait the
  original authority is re-checked
  ([`read_attempt.rs`](../crates/airk-mcp/src/read_attempt.rs)). Mutations,
  uncertain exchanges and `publication_busy` are never replayed, and retries are
  not stacked across layers.

## 10. Parity across CLI, Workbench, MCP and HTTPS

**Status: implemented.** Full transport parity across installed hosts is
unqualified. The contract is in [read-interface parity](read-interface-parity.md).

AIRK treats parity as a compatibility requirement, not a convenience. The
mechanism:

- `LocalTools` runs the real MCP server over an in-memory duplex transport. CLI
  convenience commands and `airk tools call/shell` therefore use the same
  handlers and schemas as stdio and HTTPS
  ([`local.rs`](../crates/airk-mcp/src/local.rs)).
- The Workbench planner's read operations call those same handlers inside the
  planner's admitted lifetime.
- The `/api/v1/tools` catalog is derived from the canonical catalog, not
  maintained separately by hand.
- Every UI has a **Complete response** view. JSON and Markdown copies and exports
  keep the full native result, and selected-evidence exports say that they are a
  subset.
- Regression tests compare complete results across paths (for example
  [`tests/combined_reads.rs`](../crates/airk-cli/tests/combined_reads.rs)
  `assert_parity`, and `cli_tool_calls_keep_all_fields_and_share_mcp_capacity_observations`
  in [`tools.rs`](../crates/airk-cli/src/tools.rs)). Only documented transport
  identities and timing are excluded from the comparison, and unknown fields are
  never stripped to make results match.

Management stays on authenticated native IPC for the CLI and Workbench. Evidence
MCP and the HTTPS API never receive mutation, lifecycle, enrollment or deletion
authority.

## 11. The One AIRK service runtime

**Status: implemented. Final native, installed-platform and release qualification
are pending** ([§18](#18-qualification-state-and-known-gaps)).

### 11.1 One executable

The modern payload is one native `airk` executable per target: Windows x64,
Linux x64, macOS ARM64 and macOS x64.

- **What is embedded.** The executable embeds the target's MiniLM weights,
  tokenizer and configuration, checked by digest at build time
  ([`build.rs`](../crates/airk-cli/build.rs),
  [`bundled_assets.rs`](../crates/airk-cli/src/bundled_assets.rs)). It also links
  ONNX Runtime CPU 1.22.1 statically.
- **What is gone.** There is no `airk-embed` helper, no separately deployed ORT
  library and no launcher.
- **What it does not mean.** One executable is not one process. Source and index
  workers still run as supervised processes from the **same approved image**,
  started with explicit role arguments.
- **Role copies are historical.** Copied role executables (`airk-service`,
  `airk-index`, `airk-source`) belong only to predecessor two-image payloads; the
  schema-2 single image refuses alias files
  ([`process_roles.rs`](../crates/airk-cli/src/process_roles.rs)).
- **macOS.** The main executable stays inside its sealed, signed `AIRK.app`
  bundle and is never copied out of it, which preserves the fixes for issues #66
  and #67.

### 11.2 Process model and ownership

- **One service per user and machine.** It hosts independent collections, chosen
  by each workspace's MCP configuration. Custom, test and managed installations
  get isolated namespaces and must not take over the default installation's
  endpoint.
- **Metadata actor.** A bounded actor keeps registry I/O off the health handlers.
  Evidence does not queue behind it: HTTP reads go through scoped native worker
  connection pools.
- **Containment** ([`airk-process`](../crates/airk-process)):
  - **Windows.** Job Objects with `KILL_ON_JOB_CLOSE`, never requesting breakaway.
  - **Unix.** Private process groups. The group leader is never reaped early, so
    its PID cannot be reused while AIRK still refers to it.
  - **Ownership proofs.** An allocation survives a timeout until the child's exit
    is proven.
- **Measured reason for the change.** A Windows machine was found running ten MCP
  chains, each with two resident launchers and a frontend: 20 launchers and 11
  `airk.exe` processes in total. The 1.8.14 target for ten clients is **zero
  launchers, zero per-client frontends and zero embedding helpers**.

### 11.3 In-process inference with honest completion proofs

[`InferenceOwner`](../crates/airk-cli/src/inference_owner.rs) runs:

- one owner thread with a local IPC listener (64 slots);
- a separate compute thread, configurable from 1 to 4 threads.

The model is created before any grant is accepted. Shutdown stops the engine in
order and joins every thread.

Indexing workers still reach inference through an authenticated, bounded route.
HTTP admission, worker reads and inference continuations use **separate permits**.
That prevents a deadlock on the return path HTTP → worker → service inference.

What is distinctive is that **proofs cannot be faked**:

- A `NativeCompletionWitness` can only be built from the real owner barrier and
  service lease, and its constructor is private. A serialized observation cannot
  forge one.
- The code states that "an in-process barrier can never manufacture a
  process-reap fact"
  ([`service_inference.rs`](../crates/airk-cli/src/service_inference.rs)).
- Old helper drain tickets can still be decoded for migration, but are never
  accepted as in-process proofs.
- A caller that times out does not abort an FFI computation. Capacity stays
  charged until the computation actually finishes.

The plan records the trade-off openly: an inference crash can now take down the
service, which is then recovered by its OS manager.

## 12. Local HTTPS: endpoint, trust and credentials

**Status: implemented.** Actual client and platform qualification is incomplete
([§18](#18-qualification-state-and-known-gaps)). See the
[local HTTPS API](local-https-api.md).

### 12.1 Endpoint and relocation

- **Address.** The listener binds IPv4 loopback only, at
  `https://127.0.0.1:57421/mcp` by default
  ([`service_http.rs`](../crates/airk-core/src/service_http.rs),
  [`http_endpoint.rs`](../crates/airk-cli/src/http_endpoint.rs)). It never binds
  `0.0.0.0`, `::` or a hostname. IPv6 is off.
- **When the port is taken.** If the preferred or saved port returns
  `AddrInUse`, the service binds port 0 and **keeps that socket** through TLS
  startup and publication. It never checks a port, closes it and hopes. It never
  probes, adopts or kills the other listener, and never sends it credentials.
  Other bind errors report their real cause.
- **Descriptor.** A versioned, owner-protected `EndpointDescriptor` records
  installation, user, namespace, URL, generation, service instance, executable
  digest, CA and leaf digests, state and the predecessor's digest. It contains
  no secrets.
- **Relocation transaction.** Moving to a new port goes through
  `bound_candidate` → `descriptor_published` → `configs_reconciled` →
  `complete` or `awaiting_host_reconnect`, with crash recovery from observed
  state. A service that is ready and clients that have migrated are reported as
  separate facts.
- **Bootstrap.** Discovery and management use owner-checked named pipes on
  Windows and Unix sockets elsewhere. There is no unauthenticated HTTP discovery.

### 12.2 TLS identity

Each installation creates its own private CA
([`http_endpoint_tls.rs`](../crates/airk-cli/src/http_endpoint/tls.rs)):

| Certificate | Key and lifetime | Constraints |
| --- | --- | --- |
| CA | ECDSA P-256, 3,650 days | Path length 0; KeyCertSign and CrlSign only |
| Leaf | 30 days | SAN `IP:127.0.0.1`; ServerAuth |

Private key files are immutable and owner-only. No CA is shared between
installations or shipped in artifacts. The TLS keys have nothing to do with
Developer ID or release signing.

### 12.3 Client trust (frozen decision)

Clients use normal CA validation, and **their existing public and enterprise
roots stay in place**. Nothing ever disables validation.

| Client | Mechanism | Evidence |
| --- | --- | --- |
| VS Code / Copilot | Per-user OS trust, installed by **AIRK: Trust Local HTTPS Issuer**: Windows `CurrentUser\Root` or the macOS login keychain ([`http_endpoint_trust.rs`](../crates/airk-cli/src/http_endpoint/trust.rs)) | The r25 Code run failed TLS on the extra-CA path. The OS-trust path is approved for one owned Windows run but **not yet exercised** |
| Claude Code | Additive `NODE_EXTRA_CA_CERTS` bundle | Claude 2.1.118 passed scoped reads, reconnect, additive trust and wrong-issuer rejection on Windows debug |
| Codex | `CODEX_CA_CERTIFICATE` bundle and `bearer_token_env_var` | A real Codex experiment confirmed the local CA is trusted and an untrusted issuer is rejected before HTTP |
| Linux OS trust | Unsupported in 1.8.14; needs an administrator | Documented limitation |
| The other 12 catalog clients | Guide text only; automatic setup returns `http_client_compatibility_unverified` | Blocked, not faked |

The plan states openly what this does and does not authenticate. An additive
bundle authenticates the endpoint under the client's trusted-CA policy. It is
not an installation-specific pin. A CA that the client already trusts and that
mis-issues a certificate for `127.0.0.1` falls outside this guarantee.

### 12.4 Credentials

[`http_endpoint_store.rs`](../crates/airk-cli/src/http_endpoint/store.rs):

- **One bearer credential per reviewed client connection.** It is random, at
  least 256 bits, in the form `airk1.<connection_id>.<64 hex>`, and can be revoked
  on its own.
- **Verifier only.** The service stores a SHA-256 verifier, never the token, and
  compares in constant time (`subtle::ConstantTimeEq`).
- **Ledger bounds.** At most 128 records in 1 MiB, with a default lifetime of 365
  days. Revocation and proven retirement reclaim ledger space while keeping ID
  reuse fenced.
- **Enrollment returns references, never secrets.** Tokens never appear in URLs,
  command lines, repository files, diagnostics or examples.
- **The grant decides access.** Each credential maps to the original reviewed
  grant, namespace and permitted indexes. A URL, workspace path, Explorer
  selection or session ID cannot widen it.

### 12.5 Gateway defences

[`http_gateway.rs`](../crates/airk-cli/src/http_gateway.rs):

- **Host.** The Host header must be exactly the allowed value. A missing,
  duplicated or wrong Host gets `host_refused`, which blocks DNS rebinding.
- **Origin.** **Any** `Origin` header is refused with `origin_refused` before
  credentials are processed (line 373). This is stricter than the plan's
  "validate when present", and it shuts out every browser page.
- **Bearer.** Exactly one `Bearer` header is required. Authentication runs as
  bounded blocking work, with `AUTH_TASKS = 16` each for authentication and
  validation. When saturated, the gateway refuses with `authentication_busy` or
  `native_validation_busy` before any work is admitted.
- **Bounds.** 128 connections, 32 handshakes, 32 MiB of output and 1 MiB frames.
  Sessions are limited to 128 in total and 8 per credential, with a 15 s
  initialisation limit and a 300 s idle limit.
- **Per-request checks.** The live native grant is re-validated on every request.
  Delivery fences cover bytes already buffered below TLS, so a revocation takes
  effect even mid-delivery.

### 12.6 Routes

| Route | Contract |
| --- | --- |
| `POST/GET/DELETE /mcp` | Standard MCP Streamable HTTP (rmcp `StreamableHttpService`). SSE `Last-Event-ID` replay is not supported; clients re-initialise instead |
| `GET /api/v1/tools` | `airk.local-api/1` catalog with complete schemas, canonical names and aliases |
| `POST /api/v1/tools/{name}` | Calls one advertised read tool with its exact argument object. Returns the complete canonical result as JSON, with no JSON-RPC wrapper |

- **Request-local context.** The JSON API has no durable session. Revocation and
  lifecycle admission are checked on every request.
- **Error split.** Transport errors use documented HTTP statuses (`401`/`403`,
  `400`, `413`, `429`/`503`) with a bounded, credential-free body. Canonical tool
  results, including typed native refusals, are returned as `200`.
- **No management.** There are no shutdown, upgrade, enrollment or file-write
  routes.
- **Examples.** Python, Node and PowerShell examples need no MCP SDK.

### 12.7 Issuer rotation

Rotation is a phased, receipt-backed migration
([`http_endpoint_rotation.rs`](../crates/airk-cli/src/http_endpoint/rotation.rs)):

1. prepare, which starts a 24-hour overlap;
2. publish bundles;
3. trust the new issuer;
4. commit;
5. finalize or abort;
6. clean up.

Each step requires digest-pinned expected values. A staged certificate never
authorises service startup. Renewing the leaf under the same issuer does not
force clients to re-enroll.

## 13. Lifecycle: OS managers, checkpointed upgrade and migration

**Status: implemented.** Windows cases are locally checked. Mac and Linux
lifecycle qualification is pending.

### 13.1 Direct OS supervision

The OS manager starts the approved image directly. No launcher, foreground
wrapper or shell sits in between
([`service_manager.rs`](../crates/airk-cli/src/service_manager.rs)).

- **Windows.** A user-owned Task Scheduler entry, with no administrator service.
- **Linux.** A `systemctl --user` unit. libsystemd is loaded only from protected
  OS paths.
- **macOS.** The signed `SMAppService` login-item adapter
  ([`service_manager_macos.m`](../crates/airk-cli/src/service_manager_macos.m)).
  `RequiresApproval`, revoked approval and unverifiable jobs are reported as
  separate states.
- **IPC peer checks.** On Unix the peer UID must equal `geteuid()`. On Windows the
  pipe ACL is user-only and the token SID is checked. Both happen **before** any
  request is decoded ([`local_ipc*.rs`](../crates/airk-cli/src)).
- **Stop and crash are different.** An explicit Stop stays stopped. Reconnecting
  cannot clear it, and crash restart policy treats a crash differently from a
  Stop or an upgrade.

### 13.2 Checkpointed upgrade

An upgrade stages, quiesces, checkpoints, stops, replaces and restarts the
service. It does not do a live cutover.

- **Upgrade journal** (`upgrade-journal.json`,
  [`runtime_checkpoint.rs`](../crates/airk-cli/src/runtime_install/activation/checkpoint.rs)). It is
  digest-chained and saved with compare-and-set. Its stages are:
  1. `StagedVerified`
  2. `Quiescing`
  3. `CheckpointReady`
  4. `OldStopped`
  5. `ActivationPrepared`
  6. `CandidateStarted`
  7. `Committed`
  8. `Retiring`, or `Aborted`
- **Checkpoint manifest.** It is a small, **immutable** manifest that refers to
  committed native state. It is not a second database snapshot and not a
  serialized process. It is written owner-only and atomically, and validated at
  startup. If the roster of indexes and jobs would not fit, the manifest is
  refused rather than truncated.
- **Recovery-only start.** The candidate starts in recovery-only mode under a
  durable upgrade fence. Public admission reopens only at `Committed`.
- **Inference drain.** Accepted indexing jobs keep a bounded allowance to finish
  their next embedding batch before the checkpoint. If inference cannot drain,
  the state is recorded as incomplete or forced. A graceful checkpoint is never
  faked.
- **Accepted jobs stay bound to their engine.** Each pending job kind gets exactly
  one recorded outcome:
  - finish under the predecessor;
  - continue through an explicit, versioned compatibility adapter;
  - reviewed replacement.

  A matching model digest alone never authorises moving a job to a new engine.
- **Fixed deadline.** The checkpoint deadline is 180 s.

### 13.3 Predecessor migration and mandatory cleanup

This is described in [One AIRK §10](one-airk.md#10-upgrade-path-for-older-multi-process-installations).

- **Starting points.** Upgrades come directly from any earlier official version,
  without intermediate installs. Adapters are chosen by validated state format,
  not by a version allow-list.
- **What is captured.** Process identity is captured by executable hash, install
  association, ancestry and creation time, never by PID or basename alone. The
  host editor or agent is never terminated just because it is a parent process.
- **Config rewrites.** Recognised Codex, Claude Code and VS Code/Copilot entries
  are rewritten with exact preimage checks. Comments, unrelated servers, disabled
  states and tool policies are preserved.
- **Shared archives.** Claims for the same old index share one immutable archive
  and one converted index, while each client gets its own HTTPS credential.
- **Mandatory cleanup.** After migration, the VSIX flow must remove ownership-proven
  launchers, universal pointers, old helpers and every superseded runtime.
  Unknown files, sources, indexes and shared or custom models are preserved.
- **Outcomes.** `complete`, `awaiting_host_reconnect`, `blocked_unknown_owner`,
  `blocked_client_compatibility` or `recovery_required`. A retained fallback
  executable means the transaction is unfinished, not that the upgrade succeeded.
- **Recorded evidence.** Windows predecessor migrations from 1.3.1, 1.0.0-build.7
  and 1.8.12 passed. Mac migrations, universal launchers and many-client cases
  remain open.

## 14. Index management and the catalog

**Status: implemented and locally checked** in phases P6f, P7, P8, P9, P10, P11a,
P11b and P12 of the 1.8.8–1.8.11 working builds. Installed-editor and
cross-platform qualification are recorded separately.

- **Typed catalog.** Projects (`prj_`, shown in the UI as **Index sets**), indexes
  (`idx_`), source roots (`src_`) and grants (`grt_`)
  ([`index_catalog.rs`](../crates/airk-core/src/index_catalog.rs)). Registry
  contract 13.
- **Grants.** A reviewed grant pins memberships to a policy epoch and digest, so
  relaxing a policy later cannot widen an existing grant. An empty scope means
  "no authority", never "all sources". The code repeats this principle: "IDs and
  context choices grant no access".
- **Reviewed setup.** Opening a folder does not index it or grant anything.
  **Indexing → Index library** reviews sources, storage and watching before any
  index is created.
- **Two-phase writes.** Every catalog mutation is prepared, reviewed and then
  applied against a `preview_digest`. An unknown outcome is reconciled by looking
  up its receipt, never by replaying the request.
- **[Index splitting](index-splitting.md).** Part of one index can be moved into a
  new independent index. The flow:
  - a full census of the projects affected;
  - a sealed destination build;
  - a restrictive fence on the original;
  - separate observations for logical completion, physical cleanup and funding.

  Cancellation before the fence restores the original. After the fence, recovery
  completes the restriction.
- **[Permanent exclusions](source-policy.md)** and reviewed
  [retirement](service-project-retirement.md) (**Delete index**) keep sources
  intact and show logical and physical cleanup separately. A missing source root
  is retired only after two confirmed observations at least 10 s apart.
- **[Recommendations](recommendations.md)** (`query_scope` view). This is
  metadata-only advice: "Help me choose indexes". It runs no evidence query, no
  source scan and no model load. Coverage, overlap and cost that have not been
  observed are reported as **unknown**.
- **[Explicit reference refresh](source-verification.md#explicit-reference-refresh)**
  re-indexes up to 64 selected source locators through a reviewed native
  Configure job.
- **[Self-healing](index-self-healing.md).** The service owns recovery. Accepted
  settings finish without the editor, temporary source or storage failures wait
  for conditions to recover, and retry budgets survive restarts. Stalls are
  detected from real progress.
- **Indexing schedule.** Discovery prioritises docs and instructions. Source
  batches grow progressively, from 32 files or 2 s up to 4,096 files. Semantic
  batches drain from a durable queue after source publication. Each collection has
  its own worker lock, so separate projects can index at the same time.

## 15. The VS Code extension

**Status: implemented and extensively unit-tested. The final mocked suite passed
1,089 tests with 21 gated skips. Installed-editor acceptance of 1.8.14 is
pending.**

### 15.1 Surfaces

- **Views.**
  - an activity-bar **Explorer** webview, with a fallback tree;
  - a secondary-sidebar **Workbench**;
  - about 60 commands;
  - a walkthrough;
  - a `vscode:uninstall` hook.
- **Trust gating.** Native commands appear only when the workspace is trusted,
  `airk.runtime.nativeExecution` is enabled and the runtime is available.
  `airk.runtime.nativeExecution` is a single switch that turns off every native
  process.
- **Organisation-approved runtimes.** `airk.service.managedSource`,
  `managedSha256` and `managedLifecycle` support a read-only managed runtime.
- **Explorer routes** form a closed set
  ([`explorerRoutes.ts`](../packages/vscode/src/explorerRoutes.ts)):
  - Overview;
  - Search;
  - Code: Symbols, References, Outline;
  - Documents;
  - Evidence: Inspector, Checker, Changes;
  - Review: Changes, Impact, Tests;
  - Strategies: Library, Studio, Planner;
  - Indexing: Search scope, Index library, Index sets;
  - Management: Overview, Activity, Storage, Recovery, Service, Agents, Usage,
    Logs;
  - Help.

  Unknown routes are refused. The "not available" screen says "Nothing here is
  simulated".
- **Review** joins the editor's Git changes with indexed facts and builds
  candidate impact and test checklists. It never runs tests.
- **Workbench**
  ([Workbench implementation](workbench/implementation.md)):
  - slash commands for the read tools;
  - 18 tool forms;
  - recipes;
  - editable `parse` plans;
  - continuations re-planned within the remaining call and byte budgets;
  - per-turn **Complete response** with JSON and Markdown copy and export;
  - sessions guarded by a lock, which can be forked or exported;
  - "Search Selection in Workbench".

### 15.2 Distinctive engineering

- **Updates without reloading the window**
  ([`extensionUpdateLoader.ts`](../packages/vscode/src/extensionUpdateLoader.ts)).
  - **Checks.** A sealed `airk-update.json` inventory is checked file by file:
    - SHA-256 for every file;
    - a name allow-list;
    - 8 MiB per file and 32 MiB in total;
    - no symlinks and no escape from the install root;
    - `package.json` must match;
    - versions must agree.
  - **Loading.** The code runs from the captured bytes through a private
    `vm.Script` loader that never touches Node's require cache. UI assets are
    published atomically into a content-addressed cache.
  - **Fallback.** If the new version fails to activate, the previous interface is
    restored. The Explorer panel sits outside the replaceable module graph, so
    exactly one panel survives.
- **Native runtime bootstrap**
  ([`serviceBootstrap.ts`](../packages/vscode/src/serviceBootstrap.ts)). The
  installed VSIX is the trust source.
  - The manifest's schema, version and target are checked, along with an
    inventory hash and a bounded walk of the payload. Symlinks, hard links and
    unknown files are refused.
  - From 1.8.14 a single-image runtime is required.
  - On Unix, an install directory owned by someone else or open to group/other is
    refused with a `chmod 700` hint. The extension never changes permissions
    itself.
  - A first-use review shows the persistent effects (PATH, storage) before
    anything is written.
- **MCP definition provider** (VS Code and Copilot). It returns one
  `McpHttpServerDefinition` with the bearer header and response mode.
  - Its version string includes the connection ID and the endpoint generation, so
    rotating the credential or moving the port refreshes the definition.
  - It returns nothing when the workspace is untrusted, native execution is off,
    an update is pending, or a manual `.vscode/mcp.json` entry already owns the
    connection.
  - Resolving re-checks the generation and the scope.
- **Editing agent configuration safely**
  ([`agentConfig.ts`](../packages/vscode/src/agentConfig.ts),
  [`agentSetup.ts`](../packages/vscode/src/agentSetup.ts)).
  - Only a regular, unlinked, strict UTF-8 file of at most 1 MiB is accepted, and
    the inode must not change between check and open.
  - A merge only inserts missing entries. A conflicting server of the same name
    aborts the edit with the file untouched.
  - Saves go through a `WorkspaceEdit` and require a clean document, the expected
    preimage bytes, unchanged disk bytes and the same document version. The disk
    is read again after saving to confirm.
  - Migration of user-level configs reads them with a TOML scanner that runs no
    code. It is bounded to 64 files, 8 MiB and 512 declarations.
- **Trust delivery for Claude and Codex.** These clients get trust and tokens
  through the terminal `environmentVariableCollection`:
  - a content-addressed PEM bundle that keeps the existing roots;
  - an `AIRK_MCP_TOKEN_<id>` variable.

  No secret is written into a repository file.
- **Fencing.** Several counters keep late replies from overwriting newer state:
  - `selectionEpoch`, the catalog owner key and a separate advice sequence;
  - `generation`, `observationIdentity` and `localScopeEpoch`.

  A stale reply fails with `REQUEST_SUPERSEDED` or `STALE_BINDING`, so it cannot
  overwrite the results of a newer selection.
- **Error classification**
  ([`explorerErrors.ts`](../packages/vscode/src/explorerErrors.ts)). Each error
  code belongs to one plane: assistance, host, service or webview. Native codes
  are passed through only if they match `^[a-z0-9_]{1,128}$`. Unknown outcomes are
  shown as "do not assume the action had no effect".
- **Data hygiene.**
  - **Review and Remove AIRK Data** runs an inventory, then a purge confirmed by
    plan digest.
  - The uninstall hook starts a detached worker so cleanup can finish after VS
    Code's 5 s hook deadline. It writes a result and README, preserves modified
    agent entries, and keeps sources and indexes.
  - The diagnostic log is bounded: a 200 MB high-water mark, with FIFO purge to
    100 MB.

## 16. Usage reporting and observability

**Status: implemented.** See [usage reporting](usage-reporting.md).

- **Content-free daily aggregates per collection by default.** There is no query
  or source content, no network telemetry and no external database. Modes are
  `disabled`, `aggregates-only` and `diagnostic-events`, and
  `AIRK_USAGE_MODE=disabled` bypasses recording entirely.
- **Measured, not invented, savings.**
  - **Context reduction** is computed from per-source full-context byte counts
    stored in the catalog, compared with the bytes actually delivered.
  - **Price scenarios** are explicit user inputs.
  - **Provider token use and billing** are labelled as not observed.
  - **Feedback** (`useful`, `partly-useful`, `not-useful`) is a bounded sample,
    not a relevance score.
- **Calibration.** Transformer tokenisation can be calibrated against a supplied
  tokenizer.
- **Exports.** JSON, CSV and Markdown, and a session import for paired-task
  comparison.
- **Runtime provenance.** `status` includes the compiled versions, process
  identities and executable and payload hashes for the service, worker and
  frontend. Missing observations stay explicit.
- **Diagnostics.**
  - a bounded process diagnostic ring;
  - retained `job_failed` events with safe failure codes;
  - a local raw job-error log that is never sent anywhere;
  - a diagnostic report that redacts paths and users.

## 17. Engineering process as a product property

Several practices in [`CLAUDE.md`/`AGENTS.md`](../CLAUDE.md) and the
[completion plan](one-airk-completion-plan.md) directly shape the product's
trustworthiness:

- **Cost-tiered testing.** T0 tests take seconds, T1 1–2 minutes, T2 2–12 minutes
  and T3 13–40 minutes. Defects must be reproduced at T0 or T1. T2 and T3 runs
  only confirm code that has already been reviewed, against frozen inputs.
- **Frozen inputs and receipts.** Receipts record whether native, script and
  extension inputs were unchanged. A filtered run that selects zero tests counts
  as a failure.
- **Two strikes, then stop.** If an expensive step fails twice for the same
  component, all runs stop and a root-cause note goes to the user.
- **Checks are never weakened to pass.** Assertions, limits, queues and retries
  are not loosened. Skipped or failed coverage is never relabelled. Failed
  fixtures and incomplete cleanup receipts are kept.
- **Decision logic is testable on any host.** Classification, deadlines and retry
  eligibility live in pure functions outside `cfg(target_os)` FFI modules, so
  Windows can exercise macOS logic. The Mac retirement classifier of workstream B
  is an example.
- **Separate identities for every artifact.** A reused build is never labelled as
  fresh, and every Cargo run keeps its `--timings` evidence.
- **Licensing.** DCO sign-off, SPDX headers and dependency license notices are
  preserved, and versions stay synchronised across native code, the extension and
  their lockfiles.

## 18. Qualification state and known gaps

Taken from the completion plan and the latest ledger entries, 8 November 2026.

**Passed (selected, local):**

- Windows r20 receipts: 3,224 workspace tests passed (52 skipped), plus workspace
  doctests and retrieval acceptance (two workloads, nine cases).
- Completion freeze: native source `ac2620f0…`, 1,116 inputs. Integrated
  formatting and workspace all-feature Clippy passed. The mocked extension suite
  passed 1,089 tests (21 gated skips). The Python suite passed 854 tests (13
  skips).
- Original Windows debug r26 build passed in 63.785 s, with the embedded ORT
  1.22.1 smoke test at 384 dimensions and profile `minilm-l6-v2-u8-v1`.
- Actual Claude Code 2.1.118, Windows debug: scoped reads, reconnect, additive
  trust and wrong-issuer rejection all passed.
- Windows predecessor migrations from 1.3.1, 1.0.0-build.7 and 1.8.12.

**Failed or open:**

- **r26 native checkpoint preflight.** It failed on a harness assumption: the
  harness expected install manifest schema 2, but the canonical installer
  manifest is schema 1. This is the first strike for that component.
- **Workspace tests on r25** were interrupted; there is no completed result for
  the current tree.
- **Actual VS Code 1.145.2** failed TLS before tool discovery. Only the extra-CA
  path was tested. The approved owned `CurrentUser\Root` enrollment has not yet
  run.
- **64-client campaign.** At 64 clients the r25 run stopped on bounded 503
  refusals. A typed, documented refusal with zero failures or uncertain outcomes
  is the accepted rule.
- **Installed `different` journey** timed out on r25. **Neo r13** uninstall
  failed with `ESRCH`; workstream B corrects this.
- **Linux** static build is not claimed. **Linux OS trust** is unsupported.
- **Not yet done:** Neo GUI Code, a second OS account (the two-user matrix),
  universal-launcher and many-client migrations, performance thresholds, the
  MiniLM fault matrix (CPU contention, out-of-memory, FFI failure), hosted CI,
  Developer ID / notarised artifacts, and publication.

The local finish line is defined in
[§8 of the completion plan](one-airk-completion-plan.md#8-completion-and-handoff).
Commits, pushes, PRs and publication remain unauthorised.

## 19. Inconsistencies found during this review

These are observations only. No source or documentation outside this file was
changed.

1. [`explorerWebview/manage.tsx:467`](../packages/vscode/src/explorerWebview/manage.tsx)
   says "One connection provides 27 tools". The unified catalog and the README
   have 29.
2. Two doc comments still say DiskANN's mutable provider is "not selected by the
   normal indexing/search runtime yet":
   [`vector/mutable.rs:3`](../crates/airk-retrieval/src/vector/mutable.rs) and
   [`vector.rs:32`](../crates/airk-retrieval/src/vector.rs). The kernel's search
   and update paths do use it.
3. [`code-facts-and-relationships.md`](code-facts-and-relationships.md) documents
   `airk analysis prepare`, and [formats](formats.md) describes explicit local
   OCR. The kernel entry points (`import_scip`, `run_provider`, `extract_ocr`)
   have no callers in `airk-cli` or `airk-mcp` today.
4. [`process-model.md`](process-model.md) still describes `airk-embed` as a live
   process. The README and the 1.8.14 single-image runtime say it is gone.
5. [`agent-integration.md:53`](agent-integration.md) still says no actual Claude
   TLS or scoped-read acceptance is retained. The completion plan already
   schedules that fix for its final documentation pass.
6. One AIRK §4.1 says to "validate Origin when present". The gateway refuses
   every `Origin` header, which is stricter. The plan text could be aligned with
   the implemented behaviour.

## 20. Source map

| Area | Primary owners |
| --- | --- |
| Contracts (identity, verification, guidance, catalog, combined reads, service, HTTP) | [`crates/airk-core/src`](../crates/airk-core/src) |
| Discovery, extraction, adapters, continuity, PDF worker | [`crates/airk-ingest/src`](../crates/airk-ingest/src) |
| Storage, FTS5, journal, quotas, catalog SQL, integrity | [`crates/airk-store/src`](../crates/airk-store/src) |
| DiskANN provider, ONNX inference, model catalog, intent classifier | [`crates/airk-retrieval/src`](../crates/airk-retrieval/src) |
| Publication, search, conditional reads, Workbench planner, policy | [`crates/airk-kernel/src`](../crates/airk-kernel/src) |
| MCP handlers, unified catalog, HTTPS adapter, response shaping | [`crates/airk-mcp/src`](../crates/airk-mcp/src) |
| Service runtime, gateway, TLS, credentials, inference owner, installers, OS managers | [`crates/airk-cli/src`](../crates/airk-cli/src) |
| Process containment | [`crates/airk-process/src`](../crates/airk-process/src) |
| VS Code extension, Explorer, Workbench | [`packages/vscode/src`](../packages/vscode/src) |
| Parser training data and tools | [`tools/parser-train`](../tools/parser-train) |
