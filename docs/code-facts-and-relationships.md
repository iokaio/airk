# Code facts, relationships and SQLite

AIRK uses bundled embedded SQLite for collection revisions, exact facts and relationships. Runtime installation still requires only the native helper. SQLite complements persistent Tantivy lexical retrieval and DiskANN vector retrieval through AIRK's portable disk provider. Vector/hybrid search additionally requires deliberately provisioned local model assets; see [local models](local-models.md).

## Native evidence

Language profiles now emit qualified display names, containing scopes/owner IDs, signatures before grammar body nodes, written parameters/return annotations, selected modifiers/attributes and nearby comments. Explicit declaration/occurrence and signature spans use zero-based UTF-8 byte offsets with exclusive ends in retained source. Syntax IDs include path, span and node kind, separating repeated/nested/overloaded declarations without claiming stable compiler identity after edits. Absent fields mean selected grammar fields did not provide them. Individual details are capped at 1024 UTF-8 bytes and scope walks at 128 ancestors. This is an initial extraction slice, not complete language analysis or inferred typing.

Written calls have per-language profiles and fixtures for Rust, Python, JS/TS, Go, Java, C/C++, C#, Ruby, PHP, Bash, PowerShell, Lua, Swift and Kotlin. Calls create `syntax_candidate` edges from their containing declaration or `file:<path>` owner to the written expression, such as `db.query`. That expression is not converted into a resolved symbol ID. Selected base/inheritance, import and containment syntax also produces candidate edges. Embedded Vue/Svelte/HTML code and SQL call graphs remain unsupported.

Bundled `airk-framework-cues/1` rules recognize selected app/router route calls, db/conn/pool database calls, JS test registrations, Python Depends, exception handlers and Rust/Java/C# route/test/injection attributes. Rules operate on parsed nodes, excluding comment/string examples. Familiar spellings are structural cues; framework identity, aliases, versions, endpoint security, database behavior and actual test coverage remain unverified.

`Fact.code` records basis (`syntax`, `structural_rule`, `imported`), producer/version, identity, scope, signature and coverage. Document evidence remains separate. Response/citation metadata supplies collection, generation, source hash and original location. File info still exposes partial syntax, and error-containing nodes are suppressed.

## Queries

~~~console
airk code-facts --name "Auth::authenticate" --kind definition
airk code-facts --kind route_registration --path-prefix src/
airk relationships db.query --kind call --direction incoming
~~~

Use actual `code.symbol_id` values returned by facts for identity lookup. Syntax call queries use exact written targets. Imported SCIP IDs have producer prefixes, and local identities include document paths. External targets remain identifiers and never authorize source reads outside the collection.

The MCP additions `airk_code_facts` and `airk_relationships` bring the server to fourteen read-only tools. Exact facts accept names, qualified names or IDs plus kind/language/path filters. Incoming call edges find syntax caller candidates; incoming implements edges find optional producer assertions; outgoing edges inspect dependencies/containment. Exploration allows 1–3 hops, 100 visited symbols, 100 results and 65536 payload bytes. Traversal/SQLite instructions have a 250 ms deadline; instruction interruption returns an error, while traversal/result/byte bounds set truncation. Missing edges do not prove absence of impact. `airk_references` remains explicitly textual.

Fact budgets count names, paths and document/code details; citation/envelope framing is additional. Relationship budgets count each serialized edge including its citation. SQL indexes support exact facts and incoming/outgoing edges directly without loading the complete source snapshot. Lexical/text operations still load retained snapshot data and use the in-memory map.

## Optional SCIP import

The adapter accepts supplied TypeScript/TSX/JavaScript, Rust and C# SCIP documents. Original fixtures qualify the format adapter, not every upstream indexer/build environment. AIRK does not install or certify an indexer. Producers may be compiler-backed or heuristic; imported resolution is `provider_asserted`. Ordinary SCIP references never become call edges automatically.

Capture a manifest **before** producing the index. Declare build files, lockfiles, generated inputs and a file containing effective compiler flags/target/environment fingerprints. A Git commit alone does not bind dirty files. Preparation requires indexed inventory to match current sources. Keep output outside indexed sources or inside the excluded AIRK data directory.

~~~console
airk index
airk analysis prepare --producer scip-typescript --producer-version <actual-version> --build-input tsconfig.json --build-input package-lock.json
# Save the returned manifest outside indexed sources BEFORE running the producer.
# Run the chosen producer; set index_sha256 in that captured manifest to its output digest.
airk analysis import /indexes/project/index.scip --manifest /indexes/project/binding.json
~~~

[The PowerShell helper](../scripts/import-scip.ps1) automates index → prepare → explicitly selected producer → digest → validated import. Supply `-Airk`, `-Root`, `-DataDir`, `-Collection`, `-Producer`, `-ProducerVersion`, `-ProducerExecutable`, `-ProducerArguments`, `-BuildInput` and `-IndexPath`. Producer arguments must create that output path. The helper refuses existing output/binding and output inside indexed sources. No shell command string or automatic download is used. A producer that changes generated sources requires a fresh index from the final inventory; mismatched revisions deliberately fail.

Import checks exact root/collection/extractor/source inventory/build hashes/output digest and matching producer metadata. SCIP metadata must appear once, first, have a matching local file URI and explicitly declare UTF-8 source text. UTF-8/UTF-16/UTF-32 positions are checked against retained indexed text, including character boundaries and half-open ranges; typed occurrence ranges take precedence. Unspecified encodings, escaped/foreign/unindexed paths, duplicate documents, wrong languages, mismatched embedded text, invalid symbols or ranges reject the entire import. External symbol documentation is skipped.

Raw index bytes are capped at 64 MiB for digest identity and decoded one top-level field/document at a time, capped at 8 MiB per field/document, 100000 retained facts/edges and 30 seconds. This avoids constructing an unbounded whole SCIP Index but is not an unlimited large-repository importer. Enclosing-range metadata is not yet used to derive a resolved caller graph. Selected signatures/documentation are bounded.

Import transactionally publishes a new generation with source inventory, native/imported facts, edges and manifest. Reindexing conservatively drops the entire imported provider domain after any indexed source/inventory/extractor change or declared build-input change, including inputs omitted by native extraction. Unchanged indexing retains imports and avoids rewriting SQLite. Queries describe the published revision, not unsaved buffers or automatically checked live files. Declared inputs are a producer contract, not proof of complete compiler/environment capture. This initial slice retains one imported provider domain per collection.

## Storage and migration

Each collection owns `index.sqlite3` with the active revision/component pointer. New generations contain compact digest-bound inventories and immutable `catalog.sqlite3` exact facts/relationships; source text and extractions live in content-addressed objects. Queries pin the published generation and read individual artifacts. Legacy whole-snapshot SQLite and `snapshot.json` remain readable and migrate on successful publication. Corrupt/unsupported state fails closed.

WAL transactions publish all rows and revision metadata together, with full synchronous commits and a two-second busy timeout. Existing reader transactions keep their revision during publication; later requests see the new one. The collection writer lock serializes mutations. Read tools open SQLite read-only, perform no migrations and take no writer lock; SQLite may maintain derived WAL/shared-memory sidecars. Use local storage. Network-filesystem and power-loss/process-kill qualification remain open.

Legacy retained payloads retain a 512 MiB cap. New private catalogs use a reserved page cap; the small active pointer publishes after immutable components are ready. Application [logical-byte quotas](storage-quotas.md) cover participating writers, including temporary components, model copies, database growth and retained generations. They do not cap physical allocation overhead or independent external writes. Pruning protects generation leases and removes blobs only when no retained generation references them. Nothing is automatically evicted; see [collections](collections.md).

## Measurement

[The original-corpus benchmark](../scripts/benchmark-code.py) checks exact qualified-name retrieval, candidate calls, unchanged extraction reuse and change/deletion invalidation. It records wall time, response bytes, logical disk usage and CPU/peak child RSS where available. Windows reports unavailable CPU/RSS. Queries start fresh processes; repeat measures OS cache reuse, not a warm worker. There are no timing gates or model-token/billing claims.

~~~console
python scripts/benchmark-code.py --binary target/release/airk --files 256 --output artifacts/code-benchmark.json
~~~

The checked Windows build and each native CI target run a 64-file smoke and retain measurements. Rust integration tests cover migration, rollback, shared readers/writers, isolation, duplicate names, Unicode/ranges, stale/unbound imports and source/build invalidation. Live LSP/Ctags adapters, particular external producer qualification, broader framework/version packs, dependency-specific invalidation, held-out precision/recall and paired agent tasks remain follow-ups. No Atlas, StakGraph, Munarium or VCP implementation is included.
