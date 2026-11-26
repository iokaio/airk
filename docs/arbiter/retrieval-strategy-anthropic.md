# Retrieval strategy: how Claude gathers task context, and what AIRK needs to do the same

**26 November 2026. Analysis by Claude (Fable 5.2) running in Claude Code.**
This document answers four questions the user asked, in order:

1. How would I, as an AI agent with my own harness, build the context each of
   the 20 development tasks needs, with no help from AIRK?
2. Which AIRK index retrieval function, or which simple file read or parse
   beyond the index, can do each of those steps, and where is there nothing?
3. How should the gaps be filled?
4. How could AIRK's context gathering (the tool called Answer today) work
   with the available tools and those gap fixes?

It is input to the [completion plan](completion-plan.md). It changes no
contract, gate, budget or decision recorded there.

## Method and limits

- **Tasks.** The 20 frozen development instructions: ten for Munarium at
  `9948345d` and ten for Bookbuilder at `238b22a7`. Both checkouts were at
  those commits when this ran. The instruction text is copied verbatim from
  the frozen scenario files. [Scenario inventory](answer-evaluation/scenarios.md)
- **The simulation was executed.** Every step in section 1 is a tool call I
  made on 26 November 2026 against the pinned checkouts with Claude Code's
  own tools: Grep (ripgrep), Glob, Read and read-only Git. "Expect" is what I
  predicted before the call. "Got" is what came back. Failed calls and
  wasted calls are kept in the record.
- **No AIRK tool was used in section 1.** I called AIRK `status` and
  `list_scopes` once to confirm the session binding, as the standing rules
  require. No AIRK search, read or Answer call informed a step.
- **The answer key stayed closed.** I read only the `instruction` field of
  each scenario file. I did not open the reference notes before or during
  the simulation, and I did not open the sealed scenarios at all. The
  traces show what an agent finds from the prompt. They are not graded
  against the notes here.
- **Each task was worked from its own prompt.** A step is justified only by
  the instruction, what the harness loads, and earlier results of the same
  task. Tool calls for different tasks sometimes went out in one message to
  save time. Where that happened, each call is listed under the task whose
  prompt motivated it.
- **No source is quoted.** Paths, line ranges, identifiers and counts from
  the two repositories are named. Their source text is described in my own
  words.
- **Limits.** One agent and one pass per task. Another model, or the same
  model on another day, would choose somewhat different steps. Turn and call
  counts describe this run. They are observations and not a benchmark.
  Section 2 reads AIRK's capabilities from its contracts, tool schemas and
  source, with a small number of live probes on the installed service
  (1.8.206, which has a Bookbuilder index and no Munarium index). It is not
  a measurement of the evaluation candidate.

## 1. How I build context without AIRK

### 1.1 What the harness gives me before my first call

Claude Code starts a session in the repository's folder and puts four things
in my context before I act:

| Given | Use |
| --- | --- |
| The repository's `CLAUDE.md` | Orientation I did not have to search for. Munarium's names the four top-level components, the coupled CI files, the validation commands per component and the rules for contracts and migrations. Bookbuilder's names the stack, the product invariants, the hard rules for bridge handlers and registries, a "where things live" reference table and the test commands. |
| Working folder, platform, branch, status and recent commits | Tells me which tree I am reading and whether it is clean. |
| My tool list | Grep, Glob, Read, a shell with Git, and subagents for broad sweeps. |
| The instruction | The only task-specific input. |

Both evaluation repositories carry such a file. In the judged evaluation the
agent "receives the instruction and nothing else about the repository", so
this free orientation is a real difference between my unaided sessions and
the evaluated ones. Section 2 records it as a mapped capability with a gap.

### 1.2 The loop I follow

The 20 traces below are instances of one loop.

1. **Take anchors from the prompt.** Literal tokens first: identifiers in
   code font, environment variables, routes, numbers with units, quoted
   error text and interface labels. Then domain nouns. Then the kind of
   deliverable, because it decides what "enough" means.
2. **Take a census before reading.** One count-mode or files-mode Grep per
   literal anchor over the whole tree. One call returns the file map, the
   blast radius and how ambiguous a word is in this repository.
3. **Read the owner.** Whole when it is small, by line range when it is
   large. Module comments usually state the invariant the change must keep.
4. **Follow the edges the owner names.** A constant, function, table,
   route, generator or test name found in the source becomes the next
   exact search. I do not guess vocabulary when the code has given me its
   own.
5. **Enumerate every site.** For a change, every place that enforces,
   declares, tests or documents the thing. This needs an exhaustive literal
   search with a known scope, because a missed site is a defect.
6. **Find the tests and their seam.** Which file, which fixture, how the
   transport or store is stubbed, how tests are selected and run.
7. **Find the governing documents and checks.** Reference docs, CI
   workflows, generators and drift checks that will fail if a coupled file
   is not updated.
8. **Stop when each clause of the instruction maps to a located site.** I
   stop when I can name the edit sites or facts, the invariant, the
   comparable implementation and the test home. What is left is then a
   choice to make, and no longer a fact to find.

Independent calls go out together in one turn. A turn below is one message
of mine with one to five parallel calls.

### 1.3 Munarium tasks

Munarium is a Rust server (`server/`) with client libraries in Rust, Python,
.NET and Java (`clients/`). Paths below are relative to the Munarium
checkout.

#### Munarium C1 (coding)

> Vocabulary generation currently borrows the general completion default for
> its output-token ceiling. Give it its own per-call budget,
> `vocabulary_generation`, alongside the existing eight: a built-in value, a
> `MUNARIUM_MAX_TOKENS_*` variable, the `/v1/max-tokens` read and replace,
> and the client libraries, with tests. Tenants that have already replaced
> their budgets must keep their settings after the upgrade.

**Anchors.** `MUNARIUM_MAX_TOKENS_`, `/v1/max-tokens`, `vocabulary_generation`,
"existing eight", "client libraries", "keep their settings after the upgrade".

1. **Turn 1, three calls.**
   - Grep `MUNARIUM_MAX_TOKENS_`, whole tree, count mode. *Expect:* one
     config module, an environment table in the docs, a few clients.
     *Got:* 57 hits in 18 files. One file holds 16 of them:
     `server/src/munarium-server/src/max_tokens_api.rs`. Also a migration
     `0031_max_tokens_budgets.sql`, `server/docs/tokenbudgets.md` (9) and
     one hit in each of the four client model files.
   - Grep for the route and type names (`max-tokens`, `MaxTokens`), count
     mode. *Expect:* handler, store, proto, OpenAPI, client transports and
     tests. *Got:* 423 hits in 84 files, which lists every transport and
     test file per client, the proto, `openapi.json`, `clients/server-api.json`
     and a store module `munarium-store-pg/src/max_tokens.rs`.
   - Grep `vocabulary generat`, case-insensitive, content mode. *Expect:*
     the call site. *Got:* 30 lines, mostly docs. The code is
     `vocabulary_api.rs`, and `tokenbudgets.md:24` and `:74-75` state that
     vocabulary generation uses `complete_default`.
2. **Turn 2, three calls.** Read all of `max_tokens_api.rs` (579 lines),
   `munarium-store-pg/src/max_tokens.rs` (72) and the migration (14).
   *Expect:* the list of eight and how a replacement is stored. *Got:* one
   `FIELDS` table of eight rows (JSON field, variable, accessor) at `:50-91`,
   range checks at `:123-140`, and the registry at `:156-206`. A tenant's
   replacement is one JSON object per tenant. On load, a stored row that
   does not decode as the whole object falls back to the process defaults
   with a warning. That is the upgrade hazard the last sentence of the task
   is about: a ninth required field would make every existing row
   undecodable. The tests at `:312-578` pin the count, the built-in values
   and the "missing field is refused" rule.
3. **Turn 3, three calls.**
   - Grep `MaxTokensBudgets` in `munarium-api-types/src/lib.rs` with
     context. *Got:* the wire type at `:3224-3261`, response at `:3266-3278`.
   - Grep `complete_default` and the registry's `effective` call in
     `server/src`. *Expect:* the vocabulary call site and the other users.
     *Got:* `vocabulary_api.rs:552` is the one to change.
     `answers_api.rs:393` and `providers_api.rs:418-427` are the users that
     keep it.
   - Grep `MaxTokens` in `server/proto/mmp/v1/server_api.proto`. *Expect:*
     a message with eight fields. *Got:* only two generic RPCs at
     `:181-184`. The gRPC twin passes JSON through, so the proto needs no
     field.
4. **Turn 4, four calls.** Read the wire type (`lib.rs:3212-3283`), the call
   site (`vocabulary_api.rs:536-559`), the store test
   (`pg_integration.rs:1575-1634`) and Grep the budget terms in
   `tokenbudgets.md`. *Got:* every field is required on the wire and the
   type derives a plain deserializer, the store test proves whole-object
   replace, and the doc has a per-budget table, an environment table
   (`:88-95`), wire examples and a client table (`:184-196`).
5. **Turn 5, two calls.** Grep the last existing field name,
   `authoring_assist`, in `clients/` (content) and in `server/` (count).
   *Expect:* every place that lists the eight. *Got:* 34 client lines
   across the .NET, Java and Python models and tests and the Rust client
   tests, and 25 server hits in 8 files including `openapi.json`,
   `dev-guide.md`, `platform-features.md` and `authoring_api.rs`.
6. **Turn 6, four calls.** Read the .NET model
   (`Models.Platform.cs:340-411`), the Python model (`models.py:1100-1161`)
   and the Java model (`Providers.java:56-175`), and Grep the Rust client
   for the type. *Got:* .NET has required init properties and a response
   record that copies eight fields back. Java has a record with eight
   `with…` methods. Python subclasses the budgets model for the response.
   The Rust client reuses the server's wire crate, and its gRPC transport
   reports max-tokens as unsupported (`grpc.rs:1582-1586`).
7. **Turn 7, three calls.**
   - Grep `max.tokens` in seven server files (config, state, router,
     OpenAPI registration, generated gRPC, two test configs). *Got:* the
     parse site `config.rs:355`, the route `rest.rs:1758-1760`, the
     generated twin `grpc_api_generated.rs:237-242`, and two test configs
     that build the default object.
   - Glob the migrations folder. *Got:* the newest is `0088`. A backfill
     migration, if chosen, is `0089`. `CLAUDE.md` already told me applied
     migrations are never edited.
   - Grep `max.tokens` in `clients/server-api.json`, the compatibility
     replay cases, the retention inventory and `docs/route-index.py`.
     *Got:* a v1.4 replay case reads the route (`replay-v14.json:134`) and
     the retention inventory lists the table.
8. **Turn 8, one call.** Grep the compatibility tool for how it treats an
   added response field. *Got:* no matches. That stays an open question to
   settle by running the replay.

**Context in hand.** The registration table and its tests, the wire type,
the storage shape and its fail-closed load path, the single call site, the
other two users of the shared default, four client models with their tests,
the generated and hand-written documents that list the eight, the migration
numbering rule and the compatibility replay.

**Stop.** Every clause maps to a site. The design choice left is how an
existing eight-field row keeps decoding. That is a decision, and the facts
for it are in hand. **8 turns, 23 calls.**

#### Munarium C2 (coding)

> Operators set MUNARIUM_SESSION_IDLE_TTL_SECS=30, yet a session left idle
> for more than a minute sometimes still accepts a turn instead of answering
> 409 session-not-open. Find the cause and fix it so an idle session is
> refused as soon as its TTL has passed.

**Anchors.** `MUNARIUM_SESSION_IDLE_TTL_SECS`, `session-not-open`, "sometimes",
"more than a minute".

1. **Turn 1, two calls.**
   - Grep the setting name in both spellings, content mode. *Expect:* a
     parse site, a sweeper and docs. *Got:* 38 lines. `config.rs:75` and
     `:352-354` parse it. `state.rs:580` uses it. About twenty test configs
     set it to zero. `server/README.md:262` says a janitor stamps idle
     sessions expired.
   - Grep the error slug in `server/src`. *Got:* the constructor
     `error.rs:338` and exactly one raise site, `sessions_api.rs:1332`.
2. **Turn 2, four calls.** Read the janitor (`state.rs:560-623`), the turn
   entry (`sessions_api.rs:1296-1345`), the setting's comment
   (`config.rs:64-77`) and the design note (`dev-guide.md:8128-8149`).
   *Expect:* a periodic sweep whose period explains "sometimes". *Got:* the
   cause. The sweep interval is the larger of one twenty-fourth of the TTL
   and 60 seconds, after a random start delay of up to 59 seconds. The turn
   entry refuses only when the stored state is not open. It never compares
   the session's age with the TTL. With a 30-second TTL a session can stay
   open for up to about 90 seconds. The sweep exists only with the
   PostgreSQL store.
3. **Turn 3, three calls.** Grep `last_turn_at`, Grep the state comparisons
   and the janitor's words, and Grep the session loader and turn function
   names. *Got:* the timestamp is written at `sessions_api.rs:1868` after a
   turn. The turn function is called from REST (`:2135`), the streaming
   route (`:2212`) and gRPC (`grpc_platform.rs:183`), so one check covers
   every transport. A second private loader lives in `mcp_tools.rs:1520`.
4. **Turn 4, four calls, one failed.** Read the loader
   (`sessions_api.rs:486-545`), the timestamp write (`:1852-1875`) and the
   close path (`:2258-2337`). A fourth Grep with a nested brace glob was
   rejected by ripgrep. *Got:* the loader selects five columns and no
   timestamp, so the fix must extend the query or do the comparison in SQL.
   The close path shows the house pattern for a guarded state change: one
   conditional update that only moves an open session.
5. **Turn 5, three calls.** Re-run the failed search as a files-mode Grep of
   the slug over the tree, Grep `mcp_tools.rs` for state checks, and Grep
   the session model tests for janitor coverage. *Got:* 25 files name the
   slug, including `docs/api/errors.md`, `docs/ops/troubleshooting.md` and
   each client's error decoder. The MCP file has no state check of its own.
   No test covers idle expiry.
6. **Turn 6, three calls.** Read the MCP loader (`mcp_tools.rs:1490-1551`),
   Grep the close route and the closed state across the server, and Grep
   the three reference documents for the slug. *Got:* MCP session tools
   dispatch through the native route, so they inherit the fix. The nearest
   lifecycle test is the close test in `mcp_tests.rs:1609-1618`. The
   documents to update are `errors.md:59`, `troubleshooting.md:26`,
   `README.md:262` and the design note.

**Context in hand.** The cause with its two line ranges, the single
enforcement site shared by all transports, the loader that lacks the
timestamp, the write site of the timestamp, the house pattern for a guarded
state change, the absence of a regression test and its nearest neighbor,
and four documents that describe the old behavior.

**Stop.** Cause located and confirmed in source. Fix site, pattern and test
home known. I have not reproduced the failure by running it. **6 turns, 19
calls, 1 failed.**

#### Munarium C3 (coding)

> Raise the per-request file cap for batch ingest and bulk-upload chunks
> from 500 to 1,000 files. Update every place that enforces, tests or
> documents the limit, on both transports and in the client libraries, and
> leave the 256 MiB body ceiling as it is.

**Anchors.** `500`, "batch ingest", "bulk-upload chunks", "256 MiB", "every
place".

1. **Turn 1, three calls.**
   - Grep a prose pattern (500 near files, documents, per request), content
     mode. *Expect:* documentation and comments. *Got:* 35 lines: four
     client guides, client doc comments, `openapi.json`, three server
     guides, a proto comment and two error strings in `ingest_api.rs:198`
     and `:605`.
   - Grep constant-like names (a cap word next to batch, bulk or files).
     *Expect:* one server constant. *Got:* four client constants (Rust
     `planes.rs:229`, Python `_errors.py:329`, .NET `Planes.cs:116`, Java
     `Wire.java:19`) and no server constant.
   - Grep the body ceiling in three spellings, count mode. *Got:* 60 hits
     in 39 files. I now know what to leave alone.
2. **Turn 2, two calls.** The server enforces with a bare number, which a
   prose pattern cannot find. Grep `\b50[01]\b` in the server's ingest, gRPC
   and router files with context, and in client code files. *Got:* the two
   comparisons, `ingest_api.rs:196` and `:604`. In clients: the four
   constants, about twenty doc comments, and tests that build 501 files in
   every language (.NET `PlatformTests.cs:47-55` and `PlatformSmokes.cs:591-596`,
   Java `PlatformSmokesTest.java:444-450`, Python `test_c8_surface.py:77`
   and `conformance/test_platform.py:520-521`, Rust
   `platform_smoke.rs:975-987`). Several assert that the error text
   contains "500".
3. **Turn 3, four calls.**
   - Grep the two operation function names. *Got:* gRPC calls the same
     batch function (`grpc_data.rs:161`), so the batch check is shared by
     both transports. The chunk route is REST only.
   - Grep `501` and the error string over all server code. *Expect:* a
     server test of the cap. *Got:* none. I also got a hit I had not
     looked for: `munarium-cli/src/main.rs:598` clamps a value to 500.
   - Grep with only-matching output around `500` in the three API
     reference documents. *Got:* `rest.md:188`, `grpc.md:22` and
     `grpc-reference.md:716`.
   - Read the Python guard (`_errors.py:318-338`). *Got:* one shared check
     whose message is built from the constant.
4. **Turn 4, four calls.** Read the CLI (`main.rs:584-605`), count the four
   client constant names over the tree, Grep `500` in the root docs and
   scripts, and Grep the changelogs and compatibility text. *Got:* the CLI
   defaults to 400 files per chunk and clamps to 500. The constants have 19
   uses in 8 files. `clients/README.md:383` states the cap.
5. **Turn 5, one call.** Grep `MAX_SOURCE_BYTES`. *Got:* the body ceiling
   is one constant at `rest.rs:24`, used by the route limits and both gRPC
   message limits. Nothing about the file cap touches it.

**Context in hand.** Two server comparisons with their error strings, one
shared by both transports. One CLI clamp. Four client constants with their
guards. Client and conformance tests in four languages that assert 501 is
refused and that the message names 500. Generated and hand-written documents
on both sides. No server test exists.

**Stop.** The set of sites is closed under three independent searches
(prose, bare literal, constant names). **5 turns, 14 calls.**

#### Munarium W1 (writing)

> Write the user-guide page on retiring a runbook version: the two-step
> request-and-confirm flow and its time limit, what callers and existing
> sessions see afterwards, what is and is not deleted, whether it can be
> undone, and how to call it from each client library.

**Anchors.** "retiring a runbook version", "two-step", "time limit", "each
client library".

1. **Turn 1, two calls.**
   - Grep `retir`, case-insensitive, count mode. *Expect:* a handler and a
     guide section. *Got:* 441 hits in 138 files. The word is used for
     index versions, artifacts, collections and a license identifier. It is
     too ambiguous to follow.
   - Glob `**/docs/guides/*.md` to see where a guide page belongs. *Got:*
     219 paths, most of them stale copies under an ignored
     `server/scratch/` folder. Glob does not honor ignore rules. The
     tracked guides are in `server/docs/guides/` and `clients/docs/guides/`.
     This call was mostly noise.
2. **Turn 2, three calls.** Grep `retir` in the runbook handler file, in
   client code and docs, and next to "runbook" or "version" in server docs.
   *Expect:* the feature's handler. *Got:* the handler file uses the word
   only for a different step that reclaims old index rows. The client hits
   are about collections. One line gave the real vocabulary: a figure
   caption in `dev-guide.md:13351` describes "double-pass soft removal".
3. **Turn 3, one call.** Grep the new vocabulary (double pass, soft
   removal, confirm removal). *Got:* 70 lines. The feature is called
   **removal** everywhere in code: `remove_request` and `remove_confirm` in
   every client, `ConfirmRemoval` on gRPC, and a conformance scenario named
   for the double pass.
4. **Turn 4, two calls.** Grep the server for the removal functions,
   identifier and constants, and Grep `remov` in the developer guide.
   *Got:* the implementation is `runbooks_api.rs:1286-1469` with a
   15-minute constant at `:1292`. The guide has a full narrative at
   `dev-guide.md:12285-12332`.
5. **Turn 5, three calls.** Read the implementation
   (`runbooks_api.rs:1286-1471`), the guide narrative
   (`dev-guide.md:12284-12349`) and the session section
   (`dev-guide.md:10440-10509`). *Got:* the complete rules. A request must
   name an exact version. It returns an identifier and an expiry. Confirm
   refuses when nothing is pending, when the identifier differs, or after
   15 minutes, and an expired request resets the runbook to active. Both
   calls need the read-write role. After confirmation the exact version
   answers 410 on session routes and is hidden from the default listing.
   Nothing is deleted.
6. **Turn 6, three calls.** Grep the client method signatures in four
   languages, Grep `remov` in client guides and READMEs, and Grep the two
   error slugs in the API reference. *Got:* 38 signature lines covering
   REST and gRPC transports in Rust, Python (sync and async), .NET and
   Java. The client guides say nothing about removal, so the page fills a
   real hole. `errors.md:87` and `:89` hold the two slugs.
7. **Turn 7, four calls.** Read the Python conformance scenario
   (`conformance/test_platform.py:312-355`), the Rust one
   (`platform_smoke.rs:516-575`), the apply function
   (`runbooks_api.rs:770-819`) and the two error rows. *Got:* the calling
   idiom per client and the typed errors a caller catches. Re-applying a
   removed version is refused, and re-applying while a request is pending
   cancels the request.
8. **Turn 8, two calls.** Grep the call sites of the removed-runbook error
   and read the session loader (`sessions_api.rs:424-453`). *Got:* a bare
   name resolves to the newest version that is not removed. An exact
   removed version answers 410 for new and existing sessions.

**Context in hand.** The two routes and their rules, the time limit and its
reset, the role, both error slugs with status and gRPC code, what a session
and a listing see, what is retained, the two "cannot undo" rules, and the
method names on every client and transport with a working example for two
of them. Also where the page goes and that the index must link it.

**Stop.** Every clause of the instruction has a cited source. **8 turns, 20
calls.** The first three turns were spent finding the repository's word for
the feature.

#### Munarium W2 (writing)

> Write an explainer for a new contributor on how a proposed claim is
> governed and written: what the command path does between the REST or gRPC
> handler and the database, what the gates decide, how a blocked claim, a
> head conflict and a concurrent writer are handled, and what a later read
> then returns.

**Anchors.** "proposed claim", "gates", "head conflict", "concurrent writer",
"later read".

1. **Turn 1, three calls.** Grep the propose names in Rust and proto files
   (count), Grep the head-conflict names (count) and list the headings of
   `server/docs/architecture.md`. *Got:* 56 and 105 hits. `service.rs`,
   `rest.rs`, `grpc.rs`, the core `storage.rs`, `governance.rs` and both
   stores carry them. The architecture document has no section on the
   command path.
2. **Turn 2, two calls.** Grep `propose` in the three server files with
   line numbers, and the conflict names in three core files with context.
   *Got:* the REST handler (`rest.rs:310-360`), the gRPC handler
   (`grpc.rs:215-266`) and a module comment at `service.rs:6-8` that says
   the command path is the governance path. The storage trait documents the
   optimistic head check.
3. **Turn 3, two calls.** Read `service.rs:1-300` and `rest.rs:296-363`.
   *Got:* the spine of the explainer. Both transports call one function.
   It reads the head, checks a caller's expected head, loads a snapshot at
   that head, runs the gates, marks blocked claims as disputed and the rest
   as accepted, and appends the batch together with its findings pinned to
   the observed head. When the head moved and the caller sent no expected
   head, it gates again, up to three attempts.
4. **Turn 4, three calls.** Outline `munarium-core/src/gates.rs` by Grep,
   read the core error type (`error.rs:1-60`), and Grep the store function
   names and lock statements. *Got:* five always-on gates with their rule
   identifiers at `gates.rs:55-59`, a sixth chronology gate that is armed
   per version, and the PostgreSQL append functions.
5. **Turn 5, five calls.** Read the gate runner (`gates.rs:355-429`), the
   uncommitted append (`munarium-store-pg/src/lib.rs:176-245`), the lineage
   lock (`:448-487`), the two append entry points (`:720-799`) and the
   snapshot loader (`munarium-core/src/storage.rs:244-303`). *Got:* how a
   concurrent writer is serialized: one transaction holds a row lock on the
   lineage head, checks the expected head under that lock, and writes the
   claims and the findings together.
6. **Turn 6, four calls.** Grep the slice resolver with context
   (`ledger.rs:86-142`), Grep the error mapping in the server's `error.rs`,
   read the gRPC handler (`grpc.rs:212-267`) and Grep the idempotency
   wrapper (`rest.rs:193-257`). *Got:* a later read returns accepted claims
   by default, so a disputed claim is stored and hidden unless the reader
   asks for that status. A head conflict is HTTP 409 and gRPC aborted, with
   the expected and actual sequence numbers. Commands require an
   idempotency key, and a replay returns the stored response.
7. **Turn 7, one call.** Count "disputed" in the server docs. *Got:* 104
   hits in 17 files, 65 of them in the developer guide, which is where the
   explainer should link.

**Context in hand.** The handler-to-store path on both transports, the six
gates and what a block does, the three outcomes the task names with the
exact code that produces each, the read rule, and the error mapping.

**Stop.** Each clause is cited to code I read. **7 turns, 20 calls.**

#### Munarium T1 (test)

> Make sure the Python client's REST retry rules are proven offline, end to
> end through the real transport: reads retry transient failures, a command
> is re-sent with the same idempotency key only after a connect-phase
> failure or a typed `overloaded`, never after a gateway 502 or 504, and
> plain writes are sent exactly once. Cover both the sync and the async
> transport, and add whatever tests are missing.

**Anchors.** "Python client", "retry", "idempotency key", `overloaded`, "502
or 504", "sync and the async", "whatever tests are missing".

1. **Turn 1, two calls.** Grep `retr` in the Python client (count) and Glob
   `clients/python/tests/*.py`. *Got:* the rules live in `rest.py` (39
   hits) with per-operation classes in `_specs.py` (43). Thirteen test
   files exist. `test_replay_safety.py` looks like the retry tests.
2. **Turn 2, three calls.** Read `_retry.py` (25 lines), Grep the retry
   terms in `rest.py` and list the test functions in
   `test_replay_safety.py`. *Got:* backoff only in `_retry.py`. The
   decision is in `rest.py:172-186` and the two run loops. The existing
   tests in that file call the decision function directly. They do not go
   through a transport.
3. **Turn 3, three calls.** Read `rest.py:1-60` and `:152-297`, and Grep
   the tests for a mock transport. *Got:* the module comment states the
   three rules. The transport constructor takes an injected transport as a
   documented test seam (`rest.py:217-228`). A second test file,
   `test_review_batch.py`, has a section that exercises the rule on the
   wire.
4. **Turn 4, four calls.** Read that section
   (`test_review_batch.py:134-245`), the async run loop
   (`rest.py:356-411`), Grep the retry classes in `_specs.py` and Grep the
   test configuration in `pyproject.toml`. *Got:* five wire tests exist,
   all on the sync transport: a command after typed overloaded keeps its
   key, a command after a connect error is re-sent, a command after 502 or
   504 is not, a read after 502 or 504 is, and the overloaded retry is
   bounded. The backoff patch only covers the sync sleep. Async tests run
   in auto mode.
5. **Turn 5, five calls, one failed.** Read the upload path
   (`rest.py:306-351`), the spec type (`_specs.py:16-51`), Grep the
   transient flags in `_errors.py` and Grep the Python job in
   `clients-ci.yml`. A multi-line Grep failed because the flag was missing.
   *Got:* content-addressed uploads retry on purpose and are outside the
   three rules. 502, 503 and 504 are transient for reads. CI runs
   `python -m pytest tests -q`, so new offline tests run automatically.
6. **Turn 6, one call.** Read `_specs.py:194-205` in place of the failed
   search. *Got:* a plain write spec to use in the new tests.

**Context in hand.** The rules as written, the two run loops, the seam, the
five existing wire tests and their helper, and the coverage gaps: every
async case, reads after transport failures and typed overloaded, plain
writes after each failure kind, the same generated key across a connect
retry, the other connect-phase error types, and a command after a failure
that may have been delivered.

**Stop.** I can write the missing tests against a known seam and helper. **6
turns, 18 calls, 1 failed.**

#### Munarium T2 (test)

> I've added a new REST route to the server, backed by a new SQL migration.
> Before I open the pull request, list the checks I must run, with the exact
> commands, and say what each one does and does not prove.

**Anchors.** None are literal. The harness file supplies them:
`.github/workflows/server-ci.yml`, `server/docs/guides/validation.md`, the
gate catalog files, `gates.ps1`, `test.ps1` and the per-component table of
checks.

1. **Turn 1, three calls.** List the headings of the validation guide, read
   `CONTRIBUTING.md` and Grep the job and step names of the server
   workflow. *Got:* the gates table per component, the rule that a served
   route missing from the API reference fails a test, the additive
   migration rule, and the ordered CI steps including an OpenAPI drift
   check, a gRPC reference drift check and a migration check.
2. **Turn 2, two calls.** Read the validation guide (`:1-200`) and the
   workflow steps (`:140-267`). *Got:* the local commands, the exit-code
   meanings, the statement that database tests which return early without a
   database URL are not evidence, and the exact commands of each CI step.
3. **Turn 3, four calls.** Glob `server/tools/*`, Grep the step identifiers
   in the gate catalog, read the documentation coverage test's header
   (`docs_coverage.rs:1-70`) and read the pull request template. *Got:* the
   catalog's step names, what the route-coverage test matches and what it
   cannot, and the checks the template asks the author to tick.
4. **Turn 4, four calls.** Grep the retention and hygiene tools in the gate
   scripts, read the header of the generated gRPC file, read the retention
   checker's header and read `gates.ps1`. *Got:* the generated file names
   its generator, `scripts/generate_server_api.py`. The retention checker
   states that it checks declarations and not runtime erasure. The local
   gate profile lists every step in order.
5. **Turn 5, three calls.** Grep the workflows for the generator and the
   hygiene scripts, read the generator's header and Grep for a closed route
   table in the MCP tests. *Got:* the client workflow runs the generator in
   check mode, so a new route needs regenerated gRPC and client stubs. The
   root hygiene workflow runs the private-material scan, the link check and
   the license gate. MCP exposes a closed list of routes.
6. **Turn 6, one call.** Grep the retention checker's name over the tree.
   *Got:* its two commands in `docs/ops/retention-inventory.md:130-131` and
   a Rust test that runs it. A new table must be declared there.

**Context in hand.** For a route plus a migration: formatting through the
catalog entry point, both Clippy steps, the workspace tests with and without
a database, the route and link coverage test, OpenAPI regeneration and its
drift check, the generator check for gRPC and clients, the gRPC reference
drift check, the additive migration check and what it cannot prove, the
retention inventory check for the new table, the conformance tiers, the
root hygiene checks, and the template's requirements.

**Stop.** The list is complete against CI, the local gate profile and the
template. **6 turns, 17 calls.**

#### Munarium A1 (architecture)

> Trace a `POST /v1/search` request that filters on one collection, from the
> moment it reaches the REST listener to the ranked hits and provenance
> envelope it returns, naming each component it passes through and what each
> one decides. Assume the default PostgreSQL retrieval mode and a collection
> with no inference stages.

**Anchors.** `POST /v1/search`, "REST listener", "provenance envelope",
"PostgreSQL retrieval mode", "no inference stages".

1. **Turn 1, two calls.** Grep the route string and search function names
   in server code, and Grep the middleware layers. *Got:* the route is
   registered at `rest.rs:1528` to a handler at `:1258-1342`. Two
   router-wide layers are applied at `rest.rs:1889-1896`.
2. **Turn 2, three calls.** Read the handler (`rest.rs:1236-1347`), the
   layers (`:1876-1905`) and the server start-up (`main.rs:396-445`).
   *Got:* the handler's decisions in order: the filter shape, exactly one
   collection, the caller's identity and query scope, the collection lookup
   by identifier and then by name, the clearance check that answers "not
   found" for a collection the caller may not see, the query preparation
   and the governed search.
3. **Turn 3, three calls.** Grep the function definitions the handler
   calls, Grep the REST listener in `main.rs`, and Grep the module comments
   of the two middleware files. *Got:* the listener at `main.rs:325-352`,
   the capture middleware that enforces the user identity header and
   records the interaction, the mutual-TLS admission layer, and the
   coordinator crate `munarium-retrieval`.
4. **Turn 4, three calls.** Read the coordinator
   (`munarium-retrieval/src/lib.rs:384-565`), the access helper
   (`rest.rs:104-149`) and the disclosure helper (`:1196-1237`). *Got:*
   the query is prepared once. The dispatch checks the scope is readable,
   then serves from PostgreSQL unless the mode routes the scope to the
   datastore.
5. **Turn 5, three calls.** Grep the governed search with context
   (`inference.rs:371-433`), read the PostgreSQL search
   (`munarium-retrieval-pg/src/collections.rs:1196-1405`) and the result
   types (`munarium-core/src/retrieval.rs:20-99`). *Got:* with no plan for
   the collection the governed search goes straight to the plain dispatch.
   The index version is the active one unless the caller pins one.
6. **Turn 6, two calls.** Read the two legs (`collections.rs:1404-1573`)
   and Grep the mode type and its default. *Got:* a lexical leg that ranks
   by matched terms with an OR query and optional runbook demotions, and a
   vector leg in its own transaction with its search settings. PostgreSQL
   is the default mode and the fallback for an unrecognized value.
7. **Turn 7, one call.** Read the fusion and envelope
   (`collections.rs:1574-1683`). *Got:* reciprocal-rank fusion, truncation
   to the requested count, an envelope that names the chunks, sources,
   content hashes, index version and watermark, and a final check that
   every cited source is still readable.

**Context in hand.** The whole path with a line range for each component
and decision: listener, admission, capture, handler, access, collection
lookup and clearance, vocabulary expansion, query preparation, governed
dispatch, version resolution, two legs, fusion, envelope, retention check
and disclosure.

**Stop.** The chain is unbroken from socket to response. **7 turns, 17
calls.**

#### Munarium A2 (architecture)

> We're thinking of making isolated PDF extraction the default instead of
> inline. What would that change for existing deployments (index identities,
> which documents end up indexed, builds, paid escalation, hosts that are
> not Linux), and what would have to change alongside it?

**Anchors.** "isolated PDF extraction", "default", "inline", and the five
named impact areas.

1. **Turn 1, two calls.**
   - Grep `isolat`, case-insensitive, count mode. *Expect:* one extraction
     module. *Got:* 635 hits in 134 files. The word also covers transaction
     isolation, tenant isolation and test isolation. Too broad to follow.
   - Grep the setting family by shape (`MUNARIUM_` with EXTRACT, PDF or
     ISOLAT), only-matching. *Got:* `MUNARIUM_PDF_EXTRACTION` and eight
     companions. The parse site is `config.rs:589-690`. The operator guide
     is `docs/guides/document-intelligence.md:167-249`. There is a
     measurements document and an upgrade guide.
2. **Turn 2, three calls.** Read the parser (`config.rs:576-699`), the guide
   section (`document-intelligence.md:150-261`) and the upgrade guide
   (`server-1.5.md:60-99`). *Got:* inline is the default and equals the
   previous release. The guide answers four of the five areas in prose:
   the extractor identity changes, the extracted text is the same when
   extraction succeeds, a status table lists the new outcomes, and
   escalation and deferral each have a policy setting.
3. **Turn 3, three calls.** Read the module
   (`munarium-extract/src/isolated/mod.rs:1-200`), Grep the identity strings
   over code and fixtures, and Grep the measurements document for its
   headings and the words provisional, frozen and default. *Got:* the
   identity includes the operating-system mechanism and four limits
   (`mod.rs:123-138`). The mechanism is `rlimit` on 64-bit Linux and `none`
   everywhere else (`:187-193`). Tests pin the default identity string in
   three places.
4. **Turn 4, four calls.** Read the registry version function
   (`munarium-extract/src/lib.rs:386-431`), the deferred-document handling
   (`munarium-retrieval-pg/src/lib.rs:222-313`), the measurement findings
   (`pdf-isolation-measurements.md:95-146`) and Grep the deployment files,
   changelog and decision ledger. *Got:* the sorted extractor identities
   join the index identity, so every index rebuilt after the flip is a new
   index. A skipped deferred document is listed in the manifest so a
   rebuild does not reuse that version. The findings say CPU is the
   tightest real limit and that a document refused for it is indexed with
   no text under the default policy. Kubernetes limits and request latency
   under extraction load are recorded as not yet qualified. No deployment
   file sets the variable, so a default flip reaches every deployment that
   relies on the default.
5. **Turn 5, two calls.** Grep the configuration enum's uses and read the
   registry builder (`state.rs:318-357`). *Got:* three configuration tests
   assert the inline default (`config.rs:1109-1123`). The builder resolves
   the server's own executable at start-up and fails start-up if it cannot.

**Context in hand.** Index identity: how it is formed, that it differs
between Linux and other hosts and with each limit, and that existing indexes
keep serving until a rebuild is verified and activated. Indexed documents:
the outcome classes and the measured documents at risk. Builds: the skip or
fail policy, the manifest member and the error. Paid escalation: which
classes escalate under which setting. Other hosts: no memory or CPU limit
and a different identity. Alongside: the default and its tests, start-up
behavior, five documents that state the default, the frozen limits and their
open qualification items, and the two recorded maintainer decisions.

**Stop.** Each impact area has a code source and a document source. The
recommendation itself is the reader's decision. **5 turns, 14 calls.**

#### Munarium A3 (architecture)

> We want MCP agents to be able to list their own sessions, so that an agent
> which has lost its context can find the session it had open. Where does
> that tool belong, how does it relate to the existing tools and native
> routes, and what rules does a new MCP tool have to satisfy?

**Anchors.** "MCP", "list their own sessions", "existing tools", "native
routes", "rules".

1. **Turn 1, three calls.** Outline `mcp_tools.rs` by Grep (tool names,
   route templates, constants, module comment), list the headings of the
   three MCP documents, and Grep the session routes and operations in five
   server files. *Got:* a closed catalog of eleven tools at
   `mcp_tools.rs:78-224` and a separate route allowlist at `:228-240`. The
   design document has sections for design rules, the tool contract and
   dispatch boundaries. There is no native route that lists a caller's
   sessions. The only listings are a management report and a console page
   function.
2. **Turn 2, three calls.** Read the catalog (`mcp_tools.rs:24-245`), the
   design rules (`mcp-analysis.md:1-30`) and the tool contract and dispatch
   boundaries (`:68-157`). *Got:* three handler kinds exist beside routed
   dispatch, all dedicated bounded reads. The seven design rules. Reports
   are excluded from MCP because they include other users' activity. Opening
   a session is a keyed tool whose recovery form already returns the
   session for the caller's operation key.
3. **Turn 3, four calls.** Read the implementation section
   (`mcp-analysis.md:156-225`) and the deferred and decision sections
   (`:336-381`), Grep the handler dispatch, the catalog count and the
   session helpers, and Glob `server/contract/`. *Got:* the durable
   admission rule applies to every tool before it reads data. Filtered
   listings must authorize before ordering, counting and paging, and bind
   the cursor to the caller. A test pins the catalog at eleven. The
   contract folder holds only a README in this checkout.
4. **Turn 4, four calls.** Read the session authority check
   (`mcp_tools.rs:1540-1609`), the console listing
   (`sessions_api.rs:1972-2033`), the closed-table test
   (`mcp_tests.rs:912-963`) and the operator's tool table
   (`docs/ops/mcp.md:130-173`). *Got:* the authority rule a listing must
   apply per row: same user, runbook still allowed, and the live token
   still covers the clearance the session was opened with. The console
   query is tenant-wide and cannot be reused unfiltered.

**Context in hand.** Where it belongs: a fourth dedicated filtered read in
the compiled catalog, with the query scope, not keyed and read-only. How it
relates: no native equivalent exists, the management report is deliberately
out of reach, the per-session authority check already exists, and
operation-key recovery on the open tool covers part of the lost-context
case. The rules: the seven design rules, durable admission, filtering before
paging, fixed metadata, the result size bound, the closed-table and parity
tests that pin the count, the operator and design tables that list the
tools, and the contract process for a new wire shape.

**Stop.** Placement, relation and rules are each cited. **4 turns, 14
calls.**

### 1.4 Bookbuilder tasks

Bookbuilder is WritingForge Pro: a .NET desktop application with a Vue
front end. Its code is under `pro/src/`, and paths below are relative to
that folder unless they start with `pro/`. The working tree holds large
untracked logs, build output and `node_modules`. Grep skips what the ignore
file lists. I scoped every search to `pro/` so that tracked fixtures and
bundled payloads elsewhere could not crowd the results.

#### Bookbuilder C1 (coding)

> Add Mistral as another AI provider next to OpenAI, Anthropic, Gemini,
> DeepSeek, OpenRouter and Ollama. Users should be able to enter and test a
> Mistral API key in Settings, have it stored the same way as the other
> keys, pick Mistral models, and run generation through it.

**Anchors.** The six sibling names. A new provider has no anchor of its own,
so I search for the rarest sibling: every place that lists the providers
must name DeepSeek.

1. **Turn 1, three calls.**
   - Grep `deepseek`, case-insensitive, in `pro/src`, count mode. *Expect:*
     an enum, a factory, a provider class, settings screens and tests.
     *Got:* 329 hits in 61 files across the Core, Services, Bridge and Host
     projects, the user interface and the tests.
   - Grep `provider` in the harness-named reference
     `pro/docs/ai/where-things-live.md`. *Got:* the provider interface, the
     library service interface and the registration file.
   - Grep `deepseek` in `pro/` outside `src`. *Got:* 130 hits in 19 files,
     among them a model catalog generator script with 19.
2. **Turn 2, four calls.** Read the provider enum
   (`WritingForgePro.Core/Enums/AIProviderType.cs`), the factory
   (`WritingForgePro.Services/AI/AIProviderFactory.cs`), the registration
   (`WritingForgePro.Host/ServiceRegistration/AiProviderExtensions.cs`) and
   the first 120 lines of the closest existing implementation
   (`WritingForgePro.Services/AI/DeepSeekProvider.cs`). *Got:* six numbered
   enum values, a factory that maps the enum to a string identifier and
   throws for an unknown one, one typed HTTP client per provider with its
   base address, and a comparable provider to copy the shape of.
3. **Turn 3, three calls.** Grep the same word with content output in three
   scopes: the Bridge, Core and Data projects, the Services project without
   the provider's own files, and the user interface's settings, utility,
   view, type and store files. *Got:* the full list of enumeration sites.
   Six bridge handlers map between the enum and the identifier. In Services:
   the embedding provider map and class, the model capability registry, the
   model families file, the error parser, the token ratio table, and three
   more identifier maps. In the interface: five hard-coded provider arrays
   in four views and two settings sections, the key-link table, and the
   built-in model module.
4. **Turn 4, four calls.** Read the save handler and the connection-test
   handler, Grep the key and request path inside the comparable provider,
   and read the head of the catalog generator. *Got:* the save handler
   parses the enum by name and its error text lists the six names. The test
   handler holds a per-provider default model. The provider resolves its
   key from the library row and decrypts it. The generator states that one
   specification produces the C# registry block, the interface data module,
   the help tables and a help-order test, so models are added there and not
   by hand.
5. **Turn 5, four calls.** Grep the library save method and the key
   encryption service, read the model-list handler
   (`GetModelsHandler.cs:28-139`), read the provider settings section
   (`SettingsProvidersSection.vue:1-70`), and Grep the interface types for
   a provider-name union. *Got:* the key is encrypted in
   `LibraryService.cs:938` through one service with three platform
   implementations. The provider type is a plain string in the interface
   types, so no union needs widening.
6. **Turn 6, one call.** Read `LibraryService.cs:895-946`. *Got:* storage is
   one row per provider type and has no provider-specific branch, so "stored
   the same way" needs no new code there.

**Context in hand.** The enum, factory and registration. A provider to
model the new one on. Every identifier map and hard-coded list on both
sides. The save, test and model-list handlers. The catalog generator and
the files it writes. The key path. The test files that enumerate providers.
The harness rules: direct HTTP client and no vendor SDK, keys decrypted only
for a call.

**Stop.** Each of the four user-visible capabilities maps to sites. **6
turns, 19 calls.**

#### Bookbuilder C2 (coding)

> During first-run onboarding, choosing Google Gemini, pasting a valid key
> and pressing Test connection fails with a message like "Specified argument
> was out of the range of valid values". The other providers test fine. Find
> the cause and fix it.

**Anchors.** "onboarding", "Test connection", the quoted message, "Gemini".

1. **Turn 1, three calls.**
   - Grep the button text and its camel-case form in interface code.
     *Got:* the onboarding view sends one bridge action with the provider
     type and the pasted key (`views/OnboardingView.vue:102-127`).
   - The quoted message is the framework's default text for an
     out-of-range argument exception, so Grep that exception type in the
     Bridge, Services and Core projects. *Got:* 23 sites. One is in the
     handler for that action:
     `WritingForgePro.Bridge/Handlers/TestConnectionHandler.cs:128`.
   - Grep the error catalog, bug log and gotchas, which the harness file
     says to check first. *Got:* no catalog entry.
2. **Turn 2, five calls.** Read the handler (131 lines) and the onboarding
   view (`:60-139`), Grep the handler and its helper over the tree, Grep
   the Gemini model identifiers in the capability registry, and Grep the
   bug log with only-matching output. *Got:* the cause. The handler's
   default-model lookup has a case for five providers and none for Gemini.
   When a key arrives in the payload, which only onboarding does, the
   handler calls the lookup before its error-catching block, so the
   exception surfaces as the message the author saw. The Settings screen
   takes the saved default model first and reaches the lookup only when
   none is saved. No backend test covers the handler. Valid Gemini model
   identifiers are at `ModelCapability.cs:708-845`.
3. **Turn 3, one call.** Glob regression tests named for a handler. *Got:*
   none.
4. **Turn 4, one call.** Glob the bridge test folder. *Got:* 28 handler
   test files, including one for the AI handlers, which is where a
   symptom-named regression test's neighbors live.

**Context in hand.** The failing line, why only onboarding and only Gemini
reach it, why the message is raw, a list of valid model identifiers, the
missing test and the harness's rule that a behavioral fix needs a
symptom-named regression test and a bug-log entry.

**Stop.** Cause confirmed in source and fix site known. Not reproduced by
running the application. **4 turns, 10 calls.**

#### Bookbuilder C3 (coding)

> Every AI call re-reads the provider row from the library and decrypts the
> API key again, including each retry. Change this so a key is decrypted
> once per generation run and reused for that run's calls, and a key the
> user saves mid-run is picked up by the next run. Cover every place that
> reads a provider key, and keep the key-handling rules intact.

**Anchors.** "decrypts", "provider row", "each retry", "generation run",
"every place", "key-handling rules".

1. **Turn 1, two calls.** Grep the decrypt call and the encrypted-key
   column outside tests, and Grep the architecture and guide documents for
   the rules. *Got:* 41 lines. Five generation providers each decrypt in a
   private method. So do the embedding service, the research key reader and
   the settings masking helper. The copy handlers move encrypted bytes
   without decrypting. The rule is written four times, and
   `pro/docs/architecture.md:1918-1923` says a key is decrypted only at the
   moment of a call.
2. **Turn 2, five calls.** Read the send method and the key method of one
   provider (`DeepSeekProvider.cs:136-163` and `:432-457`), the library
   helpers (`LibraryService.cs:1028-1063`), Grep the ambient scopes in
   Services and read the rule text. *Got:* the key is resolved inside the
   method that the retry loop calls, so each attempt opens a scope, reads
   the row and decrypts. A key passed in the request bypasses the lookup.
   Twenty ambient scopes exist, several named for pipeline execution.
3. **Turn 3, four calls.** Read the pipeline execution scope, Grep the key
   resolver call sites, Grep where that scope begins, and count key-service
   uses in tests. *Got:* that scope is per stage and carries the run's
   identifier. One provider has a second call site for accounting with an
   extra "must be enabled" condition. Tests already use a key-service
   double in 36 files.
4. **Turn 4, three calls.** Glob the orchestrator's partial files, read the
   dispatch (`PipelineOrchestrator.Dispatch.cs:1196-1239`) and read the
   run ownership type. *Got:* ownership of a project is acquired once per
   run and is ambient for the run.
5. **Turn 5, three calls.** Grep the ownership acquisition sites, read the
   embedding key path (`EmbeddingProviders.cs:112-141`) and the second
   provider's resolver (`OpenRouterProvider.cs:564-585`). *Got:* the run
   entries are in `PipelineOrchestrator.cs:120` and `:1004` and the
   generation plan file. Fifteen other callers acquire ownership for
   shorter work.
6. **Turn 6, two calls.** Read the factory and the registration lines.
   *Got:* the factory is a singleton that builds its provider instances
   once, so a cache held on a provider instance would outlive every run. A
   run-scoped holder is required.

**Context in hand.** All eight reading sites with the three that must not
change. Where the retry loop sits relative to the key read. The candidate
run boundary and its entry points. The lifetime trap. The ephemeral-key
bypass. The written rule that the change touches and its four locations.
The test double.

**Stop.** Every reader is listed and the run boundary has a concrete
candidate. **6 turns, 19 calls.**

#### Bookbuilder W1 (writing)

> Our in-app help only mentions sequel preparation for imported Word
> manuscripts in passing. Write a dedicated help page for authors: what "Let
> the AI learn this book for sequels" does, where they can run it (during
> import or later), what it costs and produces, what they see when it is
> done, and how it leads to starting a sequel or prequel.

**Anchors.** The quoted option label, "in-app help", "during import or
later".

1. **Turn 1, three calls.** Grep the label, Grep sequel and prequel in the
   help folder, and Glob the help folder. *Got:* the option is in
   `views/ImportView.vue:528`, a second entry point is in
   `views/GenerationView.vue:2405`, and two regression tests pin the label
   text. Help mentions the feature in three places in passing. Help
   articles are exported strings in 33 files with one index.
2. **Turn 2, four calls.** Read the import dialog (`ImportView.vue:505-566`),
   the later entry point (`GenerationView.vue:2376-2425`), the bridge
   handler and the existing help section (`help/articles/getting-started.ts:142-177`).
   *Got:* what the dialog tells the author about cost and that it can be
   skipped and run later. The later entry point is the generate phase of an
   imported book, which shows a status, three counts and an excerpt once
   preparation is done. The handler streams progress and uses the model
   family's background assignment.
3. **Turn 3, four calls.** Outline the service by Grep, read the rest of the
   panel (`GenerationView.vue:2425-2474`), read the help index
   (`help/index.ts:56-105`) and Grep the interface for the preparation and
   sequel actions. *Got:* two buttons, of which the second opens the sequel
   wizard and is enabled only after preparation. How an article is
   registered. The store action and the mock.
4. **Turn 4, three calls.** Read the service
   (`WritingForgePro.Services/ImportedNovelSequelPrepService.cs:14-219`),
   the summary line (`GenerationView.vue:174-209`) and the troubleshooting
   entry. *Got:* what is produced, in order: chapter digests without a
   model call, one whole-book analysis, then book digests, a style charter,
   story entities, promises and anchored details, and a book audit. It
   runs only for Word imports and returns early when already prepared.
5. **Turn 5, one call.** Grep the help test. *Got:* slugs must be unique
   and each article's category must exist.

**Context in hand.** What the option does and produces, both places to run
it, what the author sees during and after, what the dialog says about cost,
how the second button leads into the sequel wizard, the three existing
passing mentions to link from, and how to register a page.

**Stop.** All five things the page must say have a source. The exact cost
is not stated anywhere I read, and the page should say what the dialog
says. **5 turns, 15 calls.**

#### Bookbuilder W2 (writing)

> Write a short explainer for a new contributor on how prompt templates work
> in WritingForgePro: where the built-in templates are defined, how the
> template used for a stage is chosen when there are project, genre and
> global versions, and how placeholders and prompt blocks are filled in
> before the request goes to the model. Point out anything in the existing
> architecture document that no longer matches the code.

**Anchors.** "built-in templates", "project, genre and global", "placeholders
and prompt blocks", "architecture document". The harness file names the
registry file and states a three-step resolution order.

1. **Turn 1, three calls.** Glob `WritingForgePro.Services/Prompts/*.cs`,
   Grep the architecture document's headings for prompt and template, and
   count `genre` in the prompts folder. *Got:* 36 files: a resolver, a
   project-aware resolver, a hydrator, a block file and a built-in store
   with 24 partial files. The document has a section 5 with eight
   subsections (`pro/docs/architecture.md:392-524`) and a short summary at
   `:1957`.
2. **Turn 2, two calls.** Read the document section and the resolver
   (`PromptResolver.cs`, 194 lines). *Got:* the documented order and the
   coded order, side by side.
3. **Turn 3, four calls.** Outline the built-in store by Grep, read the
   hydrator (`TemplateHydrator.cs:1-200`), the project-aware resolver (100
   lines) and the document's summary. *Got:* the built-in registry function
   and its genre keys, the four hydration passes in order, and a second
   project-level override table.
4. **Turn 4, four calls.** Read the rest of the hydrator (`:199-348`), the
   store's lookup (`BuiltInTemplateStore.cs:17-52`) and registration
   (`:95-169`), and count the resolve and hydrate call sites. *Got:* block
   lookup order, what happens to an unresolved token, parent-genre
   fallback in the built-in store, and 36 calling files with one pipeline
   preparation file.

**Context in hand.** Where built-ins are registered and keyed. The coded
resolution order. The hydration order. Six places where the document and
the code differ: the code adds a parent-genre fallback, a selected pipeline
default with three possible positions, and an error when an explicit
override is missing. A project override can be a text-only row that
inherits schema and defaults. Blocks are expanded before data tokens, where
the document says after. Conditional sections and retrieval tokens are not
in the document. Recursion stops at a fixed depth where the document says
cycle detection. Blocks fall back to a compiled set.

**Stop.** Each clause is cited and each mismatch has both line ranges. I did
not check the document's JSON example against the entity. **4 turns, 13
calls.**

#### Bookbuilder T1 (test)

> The Word (DOCX) export has tests for margins, running headers, title page
> and scene breaks, but nothing checks what it writes as chapter headings.
> Add tests covering the chapter heading for every heading style our
> built-in export profiles use, including how chapter numbers above ten and
> chapter titles with small words are written.

**Anchors.** "DOCX export", "chapter headings", "heading style", "built-in
export profiles", "above ten", "small words".

1. **Turn 1, two calls.** Glob the test project for export tests and count
   the heading-style property in `pro/src`. *Got:* 25 export test files,
   one of them `Services/DocxExportFormatterTests.cs`. The property appears
   in the profile file, the formatter, the plain-text formatter and the
   model.
2. **Turn 2, two calls.** Grep the property with context in the Core and
   Services projects, and list the test methods of the formatter's test
   file. *Got:* the built-in profiles use six distinct style values across
   seven profiles (`Services/Export/BuiltInExportProfiles.cs:38-242`). The
   formatter has a heading function at `DocxExportFormatter.cs:1558` and a
   special path at `:725`. The test file has five tests and none reads a
   heading.
3. **Turn 3, four calls.** Read the special path (`:722-791`), the heading
   function with its two helpers (`:1552-1622`), the test file's first 120
   lines, and Grep the profile file for names. *Got:* two of the six values
   used by profiles have no case of their own and take the default form.
   One style spells numbers as words up to ten and writes digits above
   that. One style bypasses the heading function and writes two centered
   paragraphs, passing the title through a title-casing helper with a list
   of small words that keeps first and last words capitalized. The profile
   Grep found nothing because the profiles are positional records.
4. **Turn 4, four calls, one empty.** Read the head of the profile file,
   Grep for the test-visibility attribute, and read the test file's helper
   and assertions (`:140-216`). *Got:* the Services assembly exposes
   internals to the test project, so the heading function and the number
   helper can be tested directly. The existing tests export to a temporary
   path and read the document body with the Open XML library, which is the
   pattern for asserting what a reader sees.

**Context in hand.** The six style values to cover and where they are
declared. The function, the special path and the two helpers with their
rules. Which behaviors are reachable directly and which only through an
exported document. The test file, its helper and its assertion pattern.

**Stop.** I can write the tests and I know the expected text for each style
from the code. Whether two profile styles falling to the default form is
intended is a question for the author. **4 turns, 12 calls.**

#### Bookbuilder T2 (test)

> I'm about to change how the generation pipeline orchestrates stages. Which
> test suites cover the pipeline, how do I run them on my machine (quick
> loop and the full gate), which of them need real provider keys, and does
> anything run them automatically?

**Anchors.** "pipeline", "quick loop", "full gate", "real provider keys",
"automatically". The harness file names the test script, its two modes and
the sample list.

1. **Turn 1, three calls.**
   - Grep the test script (`pro/scripts/test-all.ps1`) for its parameters
     and the commands it runs. *Got:* four switches and the exact commands:
     the full mode runs the backend suite with the integration category
     filtered out and the whole front-end suite. The default mode runs a
     sample plus tests changed in the working tree.
   - Glob the test project for files named for the pipeline. *Got:* 90
     files across the Services, Regression, Bridge and Integration folders.
   - `git ls-files` for workflow, hook, YAML and script paths. *Expect:* a
     CI workflow. *Got:* none. No workflow file, hook or YAML is tracked,
     only PowerShell scripts.
2. **Turn 2, four calls.** Read the script's header (`:1-46`), read the
   sample list (`pro/scripts/smoke-tests.txt`, 321 lines), Grep the release
   and launcher scripts and both package manifests for test invocations,
   and count the category trait in the test project. *Got:* the sample
   names the orchestrator and builder suites and about twenty pipeline
   regression classes. The release scripts do not run tests. A menu script
   offers the full run. 24 classes carry the integration category.
3. **Turn 3, three calls, one empty.** Read the integration fixture's head,
   Grep three pipeline integration classes for their traits, environment
   variables and class comments, and Grep the guides for integration
   instructions. *Got:* provider integration tests read keys from a secret
   settings file in the test output folder and skip without it. One
   pipeline smoke test is opt-in through environment variables and asserts
   a live key is present. Two others are offline replays of locally
   retained files and use no key and no network. The guides say nothing
   more.

**Context in hand.** The suites by folder. The two commands and what each
includes and excludes, with the harness's rule for orchestration changes.
Which classes need keys, how keys are supplied and how a test skips. That
nothing tracked runs tests automatically.

**Stop.** All four questions are answered from files. The last answer is an
absence, and it rests on the tracked-file listing. **3 turns, 10 calls.**

#### Bookbuilder A1 (architecture)

> Trace what happens when an author starts a generation run from the
> Generate screen: from the Vue store call, across the Photino bridge,
> through the pipeline orchestrator and a stage, to the AI provider HTTP
> call, and back until the output is saved in the .bbpro project and
> progress reaches the screen. Name each component on the way.

**Anchors.** "Generate screen", "Vue store", "Photino bridge", "pipeline
orchestrator", "stage", "provider HTTP call", ".bbpro", "progress".

1. **Turn 1, three calls.** Grep the pipeline store for bridge calls, Grep
   the bridge project for pipeline action names, and Grep the "where things
   live" reference for the bridge and the orchestrator. *Got:* the store
   sends one execute action (`stores/pipelineStore.ts:146`). The handler is
   `WritingForgePro.Bridge/Handlers/ExecutePipelineHandler.cs`. The
   reference names the orchestrator's partial files.
2. **Turn 2, three calls.** Read the store's run function
   (`pipelineStore.ts:120-215`), Grep the bridge composable for the
   envelope and correlation logic, and read the handler. *Got:* the store
   checks saved decisions and an accepted plan, then sends the action with
   a progress callback and no timeout. The composable assigns a correlation
   identifier, posts through the window's message function and routes
   progress and final replies by that identifier. The handler registers a
   cancellable run, wraps progress into envelopes with the same identifier
   and calls the orchestrator in a new scope.
3. **Turn 3, two calls.** Grep the host and bridge for the window's message
   registration and the router, and Grep the orchestrator files for method
   signatures. *Got:* `WritingForgePro.Host/Program.cs:262-288` and
   `WritingForgePro.Bridge/MessageRouter.cs:45`. The signature search
   matched only the dispatch file because the main file wraps its
   signatures across lines.
4. **Turn 4, four calls.** Read the registration (`Program.cs:256-295`),
   the router (`MessageRouter.cs:40-159`), the orchestrator's entry
   (`PipelineOrchestrator.cs:96-205`) and the stage dispatch
   (`PipelineOrchestrator.Dispatch.cs:1019-1070`). *Got:* the router finds
   the handler by action, tracks cancellation by correlation identifier and
   sends the final or error envelope. The orchestrator takes ownership of
   the project for the run, opens the project database, refuses imported
   books, resolves the pipeline and its stages and checks the dependency
   graph. Dispatch picks a built-in function, an extension or a prompt
   stage.
5. **Turn 5, two calls.** Read the prompt-stage path (`Dispatch.cs:1183-1196`)
   and count provider calls and artifact writes in the pipeline folder.
   *Got:* 69 hits in 49 files. The shared stage base class holds four.
6. **Turn 6, two calls.** Grep the stage base class
   (`Services/Pipeline/PipelineStageBase.cs`) for its resolve, hydrate,
   provider, save and progress lines, and count the progress consumers in
   the interface. *Got:* the order inside a stage: open the project
   database through the factory (`:141`), resolve the prompt (`:446`),
   prepare and hydrate it (`:471`), build the request (`:534`), get the
   provider (`:1270`), call it under a timed policy (`:2218`), add the
   artifact (`:337`) and save (`:232`), reporting progress along the way.
   The store, the view and one panel consume progress.

**Context in hand.** Every hop with a file and line range, outward and
back: view, store, composable, native window, router, handler, orchestrator,
dispatch, stage base, prompt resolver and hydrator, provider factory with
its three decorators, provider and its HTTP request, the artifact write to
the project file, and the progress path in reverse.

**Stop.** The chain is closed in both directions. I read the provider's
request code in another task and would cite `DeepSeekProvider.cs:44-163`
after one more read here. **6 turns, 16 calls.**

#### Bookbuilder A2 (architecture)

> We are considering shipping a macOS build again. Assess what that would
> take: what macOS runtime code still exists and is wired up, what in the
> build, release and installer path is Windows-only, which project rules and
> tests would have to change, and where the platform documentation is out of
> date.

**Anchors.** "macOS", "wired up", "build, release and installer",
"Windows-only", "project rules and tests", "platform documentation". The
harness file states the rule: a Windows installer only, macOS runtime code
retained.

1. **Turn 1, three calls.**
   - Grep the platform words (the macOS check, `osx-`, Keychain) in
     non-test source, count mode. *Got:* 193 hits in 79 files. Most are
     two-line platform switches. The dense ones are the host start-up, the
     path service, the dialog service and the ownership lock.
   - Grep the build side: the release, deploy and launcher scripts and
     every project file, for runtime identifiers, installer words and
     platform conditions. *Got:* the release script publishes one Windows
     runtime and builds a WiX package. Two installer projects exist. The
     host project copies native webview files for six runtime identifiers,
     macOS included.
   - Count the platform words in the root and `pro/docs` documents. *Got:*
     109 hits in 24 files, including a `platform-notes.md`.
2. **Turn 2, three calls.** Grep the host, native UI, path, file-system,
   security and one handler file with content output, read
   `pro/docs/platform-notes.md`, and count platform and installer words in
   the test project. *Got:* what is wired: the registration picks macOS
   file-system, shell and Keychain implementations
   (`ServiceRegistration/InfrastructureExtensions.cs:25-28`,
   `LibraryServiceExtensions.cs:56-57`), the host has a non-Windows
   single-instance lock, the dialog service has a macOS branch and the
   path service has macOS locations. Tests exist for the key service, the
   file system and the path service.
3. **Turn 3, three calls.** Grep the release regression test for its
   assertions, read the host project file, and Grep six documents for the
   platform words with content output. *Got:* a test asserts that deleted
   release infrastructure stays deleted and names a Mac installer project,
   a Mac build folder and a Mac release script. The host project is a
   Windows executable with a Windows icon. The architecture document lists
   macOS runtime targets and says a macOS port will follow. The developer
   guide names a cross-platform verify script.
4. **Turn 4, one call.** `git ls-files` for Mac, Linux and shell scripts.
   *Got:* two legacy shell scripts and no packaging script. The Linux
   installer script and the verify script that the documents name are not
   tracked.

**Context in hand.** Runtime: the seven wired macOS surfaces. Build:
everything from publish to package is Windows-specific, in three scripts and
two installer projects. Rules and tests: the harness rule and its two README
statements, and the regression test that forbids the Mac release files.
Documents: the platform notes name an interface the code does not have,
attribute paths to the wrong class, and describe a Linux installer and an
update channel that are not in the tree. The architecture document and the
developer guide carry the stale statements above.

**Stop.** Each of the four parts has evidence. Whether the retained macOS
code still works was not run and cannot be judged from source alone. **4
turns, 10 calls.**

#### Bookbuilder A3 (architecture)

> We want to ship a new bundled pipeline product for grant proposals. Where
> does a new product's source go, what files and contract must it have, how
> does it get compiled into the installer payload, which validation rules
> must it pass, and what tests or counts will break when we add it?

**Anchors.** "bundled pipeline product", "source", "contract", "compiled
into the installer payload", "validation rules", "tests or counts". The
harness file names the seed build script.

1. **Turn 1, three calls.** `git ls-files` for the two product roots
   grouped to two levels, read the seed build script
   (`pro/scripts/subscripts/build-seed-pipelines.ps1:1-120`), and Grep the
   number 46 in tests, interface code, scripts and project files. *Expect:*
   a count somewhere. *Got:* the folder shape of a product. The script
   builds a builder project, runs it over both roots, stages one bundle per
   product plus a catalog into the host project, refuses duplicate slugs
   and identifiers, and requires the staged count to equal the number of
   product folders. 39 lines mention 46, of which about twenty are real
   count assertions in eight backend test files and two front-end tests, a
   generated help constant, four help sentences and one capture script.
2. **Turn 2, three calls, one wasted.** `git ls-files` for one sibling
   product, the root documents and the builder project. My pathspec
   matched recursively and returned 160 KB. Also read the sibling's
   `product.json` and the validation script
   (`pro/scripts/validate-pipelines.ps1:1-70`). *Got:* the closest sibling
   is an academic book proposal product with 51 files. Its metadata shows
   the setup defaults and recommendations a product declares.
3. **Turn 3, two calls.** Re-run the listing with exact globs and Grep the
   bundling regression test. *Got:* 40 products in one root and 6 in the
   other. The root has an authoring guide, an AI guide and an inventory.
   The builder is two source files. The test asserts the total, the
   nonfiction count and uniqueness of identifiers, slugs and file names.
4. **Turn 4, three calls.** Grep the headings of the three root documents,
   Grep the builder for its validator functions, and read the rest of the
   validation script (`:70-129`). *Got:* about fifty validators by name:
   product, project setup, pipeline, compatibility, stages, dependency
   graph, fan-out, projections, entities, prompts and their tokens, the
   generation contract, the final prose stage, the interview schema and
   the structured-output schema subset. The script runs the builder in
   validate mode and writes a report.
5. **Turn 5, one call.** Read the authoring guide
   (`pro/pipeline-products/README.md:1-125`). *Got:* the written contract:
   required and optional files, required metadata fields, stage and prompt
   rules, the build command and the validate command. It also states the
   product counts per category, which will change.

**Context in hand.** Where the source goes and what a sibling looks like.
The written contract and the validators that enforce it. The build path
into the host project and from there into the installer. The commands. The
complete list of places that hard-code the product count or the nonfiction
count.

**Stop.** All five questions have sources. **5 turns, 12 calls, 1 wasted.**

### 1.5 What the 20 traces have in common

| Task | Kind | Turns | Calls | The step that unlocked it |
| --- | --- | --- | --- | --- |
| Munarium C1 | coding | 8 | 23 | Count search for the variable prefix named one owning file |
| Munarium C2 | coding | 6 | 19 | Reading the sweep next to the single refusal site |
| Munarium C3 | coding | 5 | 14 | Bare-number search, after a prose search missed the server |
| Munarium W1 | writing | 8 | 20 | A figure caption supplied the code's own word for the feature |
| Munarium W2 | writing | 7 | 20 | One module comment that names the path as the governance path |
| Munarium T1 | test | 6 | 18 | Searching the tests for the transport seam found a second test file |
| Munarium T2 | test | 6 | 17 | The harness file's pointers to the workflow and the gate scripts |
| Munarium A1 | architecture | 7 | 17 | The route string, then following each called function |
| Munarium A2 | architecture | 5 | 14 | Searching by the shape of the setting's name |
| Munarium A3 | architecture | 4 | 14 | Outlining the tool catalog file by pattern |
| Bookbuilder C1 | coding | 6 | 19 | Searching for the rarest sibling to list every registration site |
| Bookbuilder C2 | coding | 4 | 10 | Translating the quoted message to its exception type |
| Bookbuilder C3 | coding | 6 | 19 | Searching for the decrypt call listed all eight readers |
| Bookbuilder W1 | writing | 5 | 15 | The quoted interface label |
| Bookbuilder W2 | writing | 4 | 13 | Reading the document and the resolver side by side |
| Bookbuilder T1 | test | 4 | 12 | The style property name led to profiles, formatter and tests |
| Bookbuilder T2 | test | 3 | 10 | The test script, and a tracked-file listing for the absence |
| Bookbuilder A1 | architecture | 6 | 16 | The action name that both sides of the bridge share |
| Bookbuilder A2 | architecture | 4 | 10 | Three parallel censuses: runtime, build and documents |
| Bookbuilder A3 | architecture | 5 | 12 | The build script and a search for the catalog count |
| **All 20** | | **109** | **312** | Mean 5.5 turns and 15.6 calls per task |

By tool: 154 Grep calls, 141 Read calls, 12 Glob calls and 5 read-only Git
listings. Four calls failed or were wasted through my own pattern or scope
errors, and several more returned nothing, which was itself the answer.

Eleven observations hold across the set. Section 2 turns each into a
capability to look for in AIRK.

1. **The first turn is a census, and it is exact.** In 16 of 20 tasks the
   first turn counted or listed every occurrence of a token or pattern
   over the whole tree or a whole component. Ranking played no part. I
   wanted the complete set and its distribution across files.
2. **Half the evidence is found by pattern, and the other half is read by
   address.** Search calls and read calls are nearly equal in number.
   Almost every read used a path and line number that a search had just
   returned.
3. **I read ranges of 20 to 200 lines, and whole files when they are under
   about 600 lines.** A function, a test or a document section is the unit.
   A passage of a few hundred bytes would not have been enough for most
   read steps.
4. **The decisive facts were often not near the prompt's words.** The cause
   in Munarium C2 is an interval expression. In Munarium C3 it is a bare
   number. In Bookbuilder C2 it is a missing case in a switch. In
   Bookbuilder T1 it is two style values with no case. None of these lines
   shares a content word with the instruction.
5. **Completeness claims need exhaustive, scoped search.** "Every place"
   appears in five instructions. I closed each set with two or three
   independent searches and reported the scope searched.
6. **An absence is a finding.** No server test of the cap, no test of idle
   expiry, no backend test of the handler, no CI workflow, no native list
   route, no removal page in the client guides. Each needed a search whose
   empty result I could trust, over a scope I could state.
7. **The repository's vocabulary differed from the user's in four tasks.**
   "Retire" is "removal" in code. "Isolated" is one setting among 635 uses
   of the word. A quoted message is a framework exception type. A limit of
   500 is a bare number. I pivoted through a document, a comment, the
   shape of a name or my own knowledge of the framework.
8. **Sibling search finds registration sites.** To add a ninth budget or a
   seventh provider, I searched for the eighth and the sixth. The result
   was the list of places a new member must be added.
9. **Tests, generators and checks are found through non-code files.** CI
   workflows, PowerShell scripts, a JSON gate catalog, project files, a
   sample list, SQL migrations and a pull request template carried the
   answers to the test and architecture tasks.
10. **Structure was recovered by pattern, not by a parser.** I outlined
    files by searching for headings, function signatures and test
    attributes. It failed twice on signatures that wrap across lines.
11. **The harness file gave the starting point in six tasks** by naming the
    workflow, the scripts, the registries and the rule a change must keep.

The 141 reads covered about 11,400 lines: a mean of roughly 80 lines per
read and 570 lines per task. These are counts of requested line ranges, not
measured bytes.

## 2. Each step mapped to what AIRK can do today

This section maps the steps of section 1 to AIRK's index retrieval functions
and to simple file reads or parsing beyond the index. Its purpose is to
find what is missing. It does not describe how Answer should work. That is
section 4.

### 2.1 What is available today

**Sources.** The public tool definitions served by the installed service
(1.8.206), the capability report from the same service, the loop types on
this branch, and the prior tool inventory in the Search plan. The installed
build is older than the evaluation candidate, so a newer build may differ in
detail.
[Existing readers](search-improvement-plan.md#how-airks-existing-tools-can-improve-search) · [Loop types](../../crates/airk-core/src/arbiter/answer_loop.rs) · [Loop recipes](../../crates/airk-kernel/src/arbiter/answer/retrieve.rs)

| Operation | What it matches or returns | Limits that matter for section 1 |
| --- | --- | --- |
| Search | Ranked passages for a query in lexical, vector or hybrid mode, filtered by literal path prefix, indexed language or evidence kind | Lexical mode turns punctuation and quotes into terms that must all match. It has no phrase match and returns candidates in stable-identifier order. No regular expressions. No count or files-only output. At most 100 hits and 65,536 bytes |
| Symbols | Written code definitions by exact name, or by suffix or substring of a definition name | Definitions only. Names are case-sensitive |
| References | Lines of retained text that contain one exact-case whole identifier, across code, documents and configuration, with path, language and kind filters and paging | One identifier per call. A name with a dot, hyphen, slash or space is refused. Counts are a lower bound for the page scanned. Coverage flags report an incomplete candidate scan |
| Code Facts | Retained facts by kind: definition, import, call, inheritance, route registration, dependency injection, test declaration and others | Syntax candidates for supported languages. Coverage depends on the indexing level and the language |
| Document Facts | Headings, requirements, decisions, table rows and configuration keys, filtered by kind, path and a case-insensitive substring of the statement or heading | One substring per call |
| Outline | One file's imports, definitions, call candidates and document facts in file order, paged | One exact path per call |
| Relationships | Incoming or outgoing structural candidates from a symbol, one to three hops | Candidates only. Absent edges prove nothing |
| Imports | Written import declarations by name, path or language | Syntax only |
| List Files | The indexed inventory under a literal path prefix, in path order, paged | Prefix only. No pattern, base-name or suffix filter. No per-folder totals |
| File Info | A file's indexed language, extraction method, hash and line basis | One path per call |
| Read File | An inclusive line range of one indexed file | Both ends are required. A range past the last line is refused. 8,192 bytes by default and 65,536 at most. A reply cut by the byte limit ends mid-line and carries no continuation |
| Context | Up to 100 lines before and after one line | One anchor per call |
| Changes | Indexed changes since a token | Describes the index, not Git history |

**Beyond the index.** The only read of live files today is saved-source
verification, which hashes a returned file to report whether it still
matches. It delivers no content. There is no scan of retained text, no
pattern match over file paths, no parse of a file at query time and no
access to Git. The plan defers live-source reading until after the Windows
review. [Current-files rule](completion-plan.md#retrieval-rounds-inside-one-answer)

**Inside Answer.** The loop on this branch may dispatch nine read kinds:
Search, Symbols, Definitions, References, Imports, Outline, Read File,
Document Sections and Context. A request has at most four parts, four
sub-needs per part and three rounds, with six reads per sub-need in the
first round and one follow-up per sub-need in each later round. Search hits
are 768 bytes. Retention defaults to 256 KiB and a reply is at most 128 KiB.
Decision 6 of the plan approved testing Code Facts, Document Facts,
Relationships and a scoped literal scan of retained text in change set 1.
A name search of the crates found no such scan yet.
[Decision 6](completion-plan.md#decisions-for-the-user-at-this-review)

### 2.2 Probes on the installed Bookbuilder index

Twelve read-only calls checked the behaviors the map depends on. The
installed index covers the live folder at an unknown revision, so these
show how the operations behave. They are not measurements of the evaluation
publications. No Munarium index is installed, so Munarium fits are judged
from the definitions and these probes.

| Probe | Result |
| --- | --- |
| References for the provider name, exact case, under `pro/src`, with file summaries and count | Five cited lines from three files per page of five. The count was a lower bound of six with "complete: false", against 329 case-insensitive matches in 61 files from Grep. A cursor continues the scan |
| References for the bare number 46 under the regression tests | Worked. A number is a token, and the count assertions were returned as cited lines |
| References for a dotted action name | Refused: the name must be a single identifier |
| Symbols for the handler's helper method | One definition with its signature and line range |
| Code Facts for test declarations in the DOCX formatter's test file | The test methods with names and line ranges |
| Document Facts for sections named "prompt" in the architecture document | Eight headings with their heading paths and lines |
| List Files under the prompts folder | All 36 paths in one page |
| List Files under `.github` | An empty list with guidance that an empty reply proves no absence |
| Read File for lines 1 to 131 of a 130-line file | Refused: the range exceeds the indexed source |
| Read File for 16 lines with a 300-byte limit | Text cut mid-line, marked truncated, no cursor. The reply was about 2.9 KB for 300 bytes of source |
| Search and Relationships on the named index | Both refused on this connection with a scope error, so neither was observed |

### 2.3 Gaps

| Gap | What my process needed | What exists | Seen in |
| --- | --- | --- | --- |
| G1 Exhaustive census | Every occurrence of a token in a scope, counted per file, in one call, with the scope stated | References pages cited lines and reports a lower bound | 25 turns |
| G2 Literals that are not one identifier | Phrases, quoted interface text, routes, dotted action names, hyphenated slugs and error text | References refuses them. Lexical Search matches their words in any order and position | 20 turns |
| G3 Loose token matching | Case-insensitive, prefix, substring and "family of names" matching over text | Suffix and substring matching exist for definition names only | 11 turns |
| G4 Patterns | Alternation, word-bounded numbers, a word near another word, a name shape | None | 13 turns |
| G5 Paths by pattern | Files by name pattern anywhere under a root, and totals per folder | Literal prefix listing | 7 turns |
| G6 Trustworthy absence | "No match in these N files" as a result I can cite | Replies state that an empty result proves nothing | 7 turns |
| G7 Reading units | A whole small file, "from here to the end", or an enclosing function or section, in one read | Explicit line ranges under a byte limit, with a hard refusal past the end | About one read in ten was larger than the 8,192-byte default. My mean read of about 80 lines is larger than the 3,072 bytes the loop gives some targeted reads |
| G8 Matches with context | Each match with a few lines around it, across files | One line per match from References, then one Context call per match | 4 turns, and more calls inside turns that otherwise fit |
| G9 Repository facts | Which files are tracked, and what Git records | None. The evaluation indexes are built from Git archives, so their inventory equals the tracked set, but no reply says so | 2 turns |
| G10 Vocabulary pivot | From the user's word to the repository's word, from a label to its action, from a message to its exception type | Ranked Search only | 3 tasks |
| G11 Orientation | The repository's instruction file before the first call | The adapter recognizes instruction files. Nothing reads them first | 6 tasks |
| G12 Depth | Four to eight dependent turns per task | Three rounds, with one follow-up per sub-need after the first | All tasks |

### 2.4 The map

"Fit" is **Yes** when one or two existing reads return the same evidence,
**Part** when it is reachable with more calls, narrower matching or without
completeness, and **No** when no existing read returns it.

**Munarium C1**

| Turn | Step | Closest read today | Fit | Gap |
| --- | --- | --- | --- | --- |
| 1 | Count a variable prefix, a hyphenated route name and a phrase over the tree | References on one full variable name, paged | No | G1, G2, G3 |
| 2 | Read three whole files of 579, 72 and 14 lines | File Info or Outline for the last line, then Read File with a raised limit | Part | G7 |
| 3 | Find the wire type, every use of one field, and the proto entries | Symbols and References | Yes | |
| 4 | Three range reads and terms inside one document | Read File and Document Facts | Yes | |
| 5 | Sibling census of the last field name in three spellings | References once per spelling | Part | G1, G3 |
| 6 | Read four client models | Symbols, then Read File | Yes | |
| 7 | One term in seven named files, the migration list, five mixed files | References per prefix and List Files | Part | G2, G5 |
| 8 | Confirm a tool folder says nothing about added fields | None | No | G6 |

**Munarium C2**

| Turn | Step | Closest read today | Fit | Gap |
| --- | --- | --- | --- | --- |
| 1 | Find the setting in two spellings and the error slug | References for the names. The slug is hyphenated | Part | G2 |
| 2 | Four range reads | Read File | Yes | |
| 3 | A column name, the state comparisons, the loader and turn functions | References and Symbols. The comparison is a pattern | Part | G4 |
| 4 | Three range reads | Read File | Yes | |
| 5 | Files naming the slug, state checks in one file, absence of a test | None for the slug or the absence | Part | G2, G6 |
| 6 | One read, the close route, the slug in three documents | Read File and Symbols | Part | G2 |

**Munarium C3**

| Turn | Step | Closest read today | Fit | Gap |
| --- | --- | --- | --- | --- |
| 1 | A number near certain words, constant names by shape, a count of the body ceiling | None | No | G1, G4 |
| 2 | The bare numbers 500 and 501 in chosen files | References for each number, paged | Part | G1 |
| 3 | Two function names, 501 and an error string over the server, three documents | Symbols and References. The string is a phrase | Part | G2, G8 |
| 4 | One read, a count of four constant names, the number in documents | Read File and References | Part | G1, G2 |
| 5 | Every use of the body-ceiling constant | References | Yes | |

**Munarium W1**

| Turn | Step | Closest read today | Fit | Gap |
| --- | --- | --- | --- | --- |
| 1 | Count a word stem, case-insensitive, and find guide files by pattern | None | No | G1, G3, G5 |
| 2 | The stem in one file, in the clients and near two other words | None | No | G3, G4 |
| 3 | Pivot to the repository's vocabulary with a phrase alternation | Hybrid Search may rank the guide section | Part | G2, G4, G10 |
| 4 | Functions and constants by name pattern, and the stem in the guide | Symbols by substring and Document Facts | Part | G3 |
| 5 | Three range reads | Read File | Yes | |
| 6 | Method signatures in four languages, absence in client guides, two slugs | Symbols per name | Part | G2, G6 |
| 7 | Four range reads | Read File | Yes | |
| 8 | Call sites of one constructor and one read | References and Read File | Yes | |

**Munarium W2**

| Turn | Step | Closest read today | Fit | Gap |
| --- | --- | --- | --- | --- |
| 1 | Counts for two name families and a document's headings | References per name and Document Facts | Part | G1 |
| 2 | Names in three files, and matches with context | References, then Context per match | Part | G8 |
| 3 | Reads of 300 and 68 lines | Read File with a raised limit | Yes | |
| 4 | Outline a file, read the error type, find store functions and a lock statement | Outline and Symbols. The statement is a phrase | Part | G2 |
| 5 | Five range reads | Read File | Yes | |
| 6 | Four definitions with their bodies | Symbols and Read File | Yes | |
| 7 | Count a word in the documents | References, paged | Part | G1 |

**Munarium T1**

| Turn | Step | Closest read today | Fit | Gap |
| --- | --- | --- | --- | --- |
| 1 | Count a stem in the Python client and list the test files | List Files for the tests. None for the stem | Part | G1, G3 |
| 2 | One small read, several terms in one file, the test functions of a file | Read File, Code Facts for test declarations | Part | G4 |
| 3 | Two reads and every test that uses a mock transport | Read File and References | Yes | |
| 4 | Two reads, the retry class per operation, test configuration keys | Read File and Document Facts. The class assignment is a pattern | Part | G2, G4 |
| 5 | Two reads, transient flags, the CI job's commands | Read File and References | Part | G4 |
| 6 | One range read | Read File | Yes | |

**Munarium T2**

| Turn | Step | Closest read today | Fit | Gap |
| --- | --- | --- | --- | --- |
| 1 | Guide headings, one whole document, the workflow's jobs and steps | Document Facts and Read File. Step names depend on the workflow adapter | Part | G7 |
| 2 | Reads of 200 and 128 lines | Read File with a raised limit | Yes | |
| 3 | List a tools folder, step identifiers in a JSON catalog, two reads | List Files, Document Facts for keys, Read File | Part | G4 |
| 4 | Several tool names across five named scripts, three reads | References per name | Part | G4 |
| 5 | Workflows that run a generator, one read, a closed-table phrase in tests | References and Read File | Part | G2 |
| 6 | Every mention of one checker | References | Yes | |

**Munarium A1**

| Turn | Step | Closest read today | Fit | Gap |
| --- | --- | --- | --- | --- |
| 1 | Find a route string, search functions and middleware layers | Symbols for the functions. The route is not an identifier | Part | G2 |
| 2 | Three range reads | Read File | Yes | |
| 3 | Nine function definitions and two module comments | Symbols and Outline | Yes | |
| 4 | Three range reads | Read File | Yes | |
| 5 | One definition with context and two reads | Symbols, Context and Read File | Yes | |
| 6 | One read and an enumeration with its default | Read File and Symbols | Yes | |
| 7 | One range read | Read File | Yes | |

**Munarium A2**

| Turn | Step | Closest read today | Fit | Gap |
| --- | --- | --- | --- | --- |
| 1 | Count a stem, and find a family of settings by the shape of the name | Hybrid Search may rank the guide | No | G1, G3, G4, G10 |
| 2 | Three range reads | Read File | Yes | |
| 3 | One read, identity strings with punctuation, a document's headings | Read File and Document Facts | Part | G2 |
| 4 | Three reads, the setting across deployment files, and its absence there | Read File and References | Part | G6 |
| 5 | Uses of one type and one read | References and Read File | Yes | |

**Munarium A3**

| Turn | Step | Closest read today | Fit | Gap |
| --- | --- | --- | --- | --- |
| 1 | Outline a catalog by its string fields, three documents' headings, session routes | Outline and Document Facts. Tool names and routes are string literals | Part | G2 |
| 2 | Three range reads | Read File | Yes | |
| 3 | Two reads, dispatch and count sites, one folder listing | Read File, References and List Files | Part | G1 |
| 4 | Four range reads | Read File | Yes | |

**Bookbuilder C1**

| Turn | Step | Closest read today | Fit | Gap |
| --- | --- | --- | --- | --- |
| 1 | Case-insensitive sibling census with counts, in and outside the source tree | References in each spelling, paged, with lower-bound counts | No | G1, G3 |
| 2 | Four whole-file reads | Symbols, then Read File | Yes | |
| 3 | List every enumeration site in three scopes | References per spelling and prefix | Part | G1, G3 |
| 4 | Two reads, key-path lines in one provider, a generator's head | Read File and References | Yes | |
| 5 | Two names, two reads, a union type pattern | Symbols and Read File | Part | G4 |
| 6 | One range read | Read File | Yes | |

**Bookbuilder C2**

| Turn | Step | Closest read today | Fit | Gap |
| --- | --- | --- | --- | --- |
| 1 | A button label, an exception type, the bug documents | References for the type and the camel-case name. The label is a phrase | Part | G2, G10 |
| 2 | Two reads, the handler names, the dotted action name, model identifiers by prefix | Read File and References. The action name is refused | Part | G2, G3 |
| 3 | Regression tests named for a handler | None | No | G5 |
| 4 | List the bridge test folder | List Files | Yes | |

**Bookbuilder C3**

| Turn | Step | Closest read today | Fit | Gap |
| --- | --- | --- | --- | --- |
| 1 | Every decrypt call and key column outside tests, and the written rules | References with an exclusion prefix, and Document Facts | Part | G1 |
| 2 | Four reads and the ambient scopes | Read File and References | Yes | |
| 3 | One read, call sites of two names, a count in tests | Read File and References | Part | G1 |
| 4 | Files sharing a name prefix, two reads | List Files and Read File | Yes | |
| 5 | Acquisition sites and two reads | References and Read File | Yes | |
| 6 | Two reads | Read File | Yes | |

**Bookbuilder W1**

| Turn | Step | Closest read today | Fit | Gap |
| --- | --- | --- | --- | --- |
| 1 | A quoted interface label, two words in the help folder, the folder's files | Lexical Search on the label's words, References per word, List Files | Part | G2 |
| 2 | Four range reads | Read File | Yes | |
| 3 | A service outline, two reads, four action names | Outline, Read File and References | Yes | |
| 4 | Three range reads | Read File | Yes | |
| 5 | Constraints asserted by one test file | Read File | Part | G4 |

**Bookbuilder W2**

| Turn | Step | Closest read today | Fit | Gap |
| --- | --- | --- | --- | --- |
| 1 | List a folder, find a document's prompt headings, count a word | List Files and Document Facts | Part | G1 |
| 2 | Reads of 136 and 194 lines | Read File with a raised limit | Yes | |
| 3 | Outline a store and three reads | Outline and Read File | Yes | |
| 4 | Three reads and a count of call sites | Read File and References | Part | G1 |

**Bookbuilder T1**

| Turn | Step | Closest read today | Fit | Gap |
| --- | --- | --- | --- | --- |
| 1 | Test files named for export anywhere under the tests, and a count of one property | References for the property | Part | G1, G5 |
| 2 | The property with context in two projects, and a file's test methods | References, then Context per match, and Code Facts | Part | G8 |
| 3 | Three range reads | Read File | Yes | |
| 4 | Two reads and one attribute in the project files | Read File and References | Yes | |

**Bookbuilder T2**

| Turn | Step | Closest read today | Fit | Gap |
| --- | --- | --- | --- | --- |
| 1 | Test files named for the pipeline, and whether any workflow or hook is tracked | List Files returned an empty page with a disclaimer | No | G5, G6, G9 |
| 2 | Two reads, test commands in six named files, a count of one trait | Read File and References | Part | G1, G2 |
| 3 | One read, traits and variables in three files, silence in the guides | Read File and References | Part | G6 |

**Bookbuilder A1**

| Turn | Step | Closest read today | Fit | Gap |
| --- | --- | --- | --- | --- |
| 1 | Dotted action names on both sides of the bridge, and the reference document | Symbols by substring for the handler. The action name is refused | Part | G2 |
| 2 | Two reads and several terms in one file | Read File and References | Yes | |
| 3 | Two registration names and a file's method signatures | References and Code Facts. Parsed definitions also find the wrapped signatures my pattern missed | Yes | |
| 4 | Four range reads | Read File | Yes | |
| 5 | One read and a count of two calls across a folder | Read File and References | Part | G1 |
| 6 | Five anchors in one large file and a count of consumers | References per anchor | Part | G1, G8 |

**Bookbuilder A2**

| Turn | Step | Closest read today | Fit | Gap |
| --- | --- | --- | --- | --- |
| 1 | Three censuses with counts: runtime, build files and documents | None | No | G1, G3, G4 |
| 2 | Matches in a chosen set of files, one document, a count in tests | References per prefix and Read File | Part | G1 |
| 3 | A test's method names, one project file, words in six documents | Code Facts, Read File and References | Part | G1 |
| 4 | Which packaging scripts are tracked, and which are not | None | No | G6, G9 |

**Bookbuilder A3**

| Turn | Step | Closest read today | Fit | Gap |
| --- | --- | --- | --- | --- |
| 1 | Product folders grouped two levels deep, one script, the number 46 everywhere | List Files in pages of 100, Read File, References for the number | Part | G1, G5 |
| 2 | One product's files and two reads | List Files and Read File | Yes | |
| 3 | Count the product manifests one level down, and one test's assertions | Read File. No pattern over paths | Part | G5 |
| 4 | Three documents' headings and a builder's validators by name | Document Facts and Symbols by substring | Yes | |
| 5 | One range read | Read File | Yes | |

### 2.5 Tally

| | Yes | Part | No | Turns |
| --- | --- | --- | --- | --- |
| Munarium | 26 | 30 | 6 | 62 |
| Bookbuilder | 21 | 21 | 5 | 47 |
| Both | 47 | 51 | 11 | 109 |
| First turn of each task | 0 | 13 | 7 | 20 |

Three readings of the tally:

1. **The reading half is available.** Nearly every "Yes" is a read at a
   known address or the lookup of a named definition. AIRK can already do
   what I did once I knew where to look.
2. **The finding half is not.** No task's first turn maps fully. The turn
   that produced the addresses used exhaustive counting, loose matching,
   non-identifier literals or path patterns, which are gaps G1 to G5.
3. **Part fits cost calls.** A "Part" step usually means three to ten AIRK
   calls for one of mine, without the count or the completeness I relied
   on. Inside Answer those calls are cheap. For an agent calling the tools
   itself, they are the turns the evaluation counts.

One more observation belongs here. I used no semantic or ranked retrieval
in any of the 20 tasks. Three tasks had a vocabulary problem that ranked
Search over documents could have shortened. The other 17 were solved by
exact finding and reading. The scenario instructions are rich in literal
anchors, so this says more about these 20 prompts than about tasks in
general.

## 3. Strategy to fill the gaps

The items are ordered by how many turns they would have served. Each states
what to add, which gaps it closes, the smallest useful form and where it
stands against the plan's recorded decisions. Items marked "needs a
decision" go beyond what the plan has approved. The plan lists all of them
as open item 14.
[Recorded decisions](completion-plan.md#decisions-for-the-user-at-this-review)

### 3.1 Give the retained-text scan three outputs and a receipt

**Closes G1, G2, G3, G6 and G8, and most of G4.** Decision 6 already
approves a scoped literal scan of retained indexed text. The traces say
what its contract needs so that it can stand in for my census.

| Part | Smallest useful form | Why |
| --- | --- | --- |
| Match | One or more literal strings of any characters, with three options: ignore case, whole word, word prefix | Phrases, routes, dotted and hyphenated names and name families appear in 29 turns (G2 and G3). Several literals in one call covers most of what I used alternation for |
| Scope | The existing filters: path prefix, language, evidence kind, exclusion prefix. Add the path pattern of 3.2 | A scan is only as useful as its scope is precise |
| Output A | Counts per file | The census. One call replaced a ranked list in 16 first turns |
| Output B | Matching lines with a chosen number of lines before and after, cited | Replaces References followed by one Context call per match |
| Output C | Paths only | For choosing what to read |
| Receipt | Files examined, bytes examined, matches, whether the scan completed, and a continuation when it did not | A completed scan with no match is a citable absence inside its stated scope |
| Cost | Charged to the request's call, byte and deadline budgets | Keeps the approved limits meaningful |

Use the lexical postings to choose candidate files when a literal contains
an indexed term, verify every match on retained bytes, and fall back to a
sequential scan of the scope when the literal has no indexed term or the
caller needs completeness. Report which path ran. Measure one
representative scan on each publication before fixing limits. I have no
timing for a scan of either publication.

**Regular expressions need a decision.** Thirteen turns used a pattern. In
most of them the pattern was an alternation of literals or a number with
word boundaries, which the options above cover. A bounded pattern subset on
a linear-time matcher would cover the rest. I would add it only if replays
show misses that the literal options leave.

### 3.2 Let the inventory answer path questions

**Closes G5 and part of G6 and G9. Needs a decision.** Add two things to the
inventory reader that Answer uses: a pattern over the relative path, such as
every `product.json` one level below a folder or every test file whose name
contains a word, and a roll-up that returns counts per folder to a chosen
depth. Both read the inventory the index already holds and open no source.
For a publication built from a Git archive, record that basis in status and
in each receipt, so that "nothing is tracked under this path" is a complete
answer.

### 3.3 Read in units

**Closes G7.** Three changes, in order of value:

1. Accept a range that runs past the end of the file, clamp it and say so.
   An agent that does not know a file's length should not need a second
   call to learn it.
2. Read by unit: the definition that encloses a line, the heading section
   that encloses a document line, or the whole file when it is under a
   line limit. Symbols, Code Facts and Document Facts already return the
   spans and heading lines this needs.
3. Cut at a line boundary and return the next line number when a byte limit
   stops a read.

The plan already compares fixed excerpt sizes with need-based sizes in
change set 1. The traces give that comparison a target: about 80 lines for
a typical read and 150 to 300 lines for the decisive one in ten.
[Read and delivery budgets](completion-plan.md#read-and-delivery-budgets)

### 3.4 Count exactly before ranking

**Closes the first-turn gap. Needs a decision on how it counts against the
three rounds.** No first turn mapped fully because the loop starts each need
with Search. Start each need that has a literal anchor with a census
instead: a count scan for the anchor and its spelling variants, and an
inventory pattern for any artifact kind the task names. The plan holds
three rounds fixed for the Windows comparison. Either the census counts as
part of the first round's reads, or it runs as a pass before the rounds.
The second keeps three rounds for work that depends on reading.

### 3.5 Pivot vocabulary on purpose

**Closes G10.**

- **Variants.** Generate the snake, camel, Pascal, kebab and upper-snake
  spellings of an anchor and scan for all of them in one call. Munarium C1
  and Bookbuilder C1 needed three spellings each.
- **From documents.** When an anchor's census is empty or spread thinly
  across many files, run hybrid Search over documents and comments, take
  the identifiers and code-formatted terms in the top passages as new
  anchors, and census those. This is how "retire" became "removal".
- **Across a boundary.** A label leads to the handler named on the same
  line, an action string on one side of a bridge leads to the same string
  on the other, and a file that says it is generated leads to its
  generator. Each is a literal followed across files, which 3.1 makes
  possible.
- **Outside the repository.** A quoted runtime message that the census
  cannot find is probably the framework's. Report that as a finding. The
  calling agent or the Arbiter model can supply the type name.

### 3.6 Read the repository's own instructions first

**Closes G11. Needs a decision.** The indexes already classify instruction
files. At the start of a request, read the headings of the root and nearest
component instruction files and the sections whose headings share a noun
with the task, within a small byte allowance. Treat them as untrusted data
that points at paths, commands and rules. They gave the starting point in
six of the 20 tasks.

### 3.7 Use text scans for structured files before adding parsers

Workflows, scripts, project files, migrations and catalogs carried the test
and architecture answers. A scan with context lines returns a workflow
step's command, a script's parameters and a project property as text.
Document Facts already holds keys and table rows for several formats. Add a
parser only for a miss that a replay shows.

### 3.8 Depth

**Bears on G12.** My tasks took four to eight dependent turns. The loop has
three rounds. Before asking for more rounds I would test whether a census
pass plus unit reads fits a typical task into three: owners in round one,
the names found in owners in round two, and closure, tests and documents in
round three. The two eight-turn tasks spent their extra turns on vocabulary
and on discovery in sequence that a wide first pass removes. This is a
hypothesis for the replays.

### 3.9 What the traces do not justify

- **Live-source reads.** No step needed bytes outside the pinned revision.
- **Git history.** No task used log or blame.
- **Resolved references from a provider.** Textual occurrences followed by
  a read were enough in every impact step.
- **A different embedding model.** No step depended on ranking quality.

## 4. How Answer could work

This section outlines a gathering process built from the existing reads and
the fixes in section 3. It follows the loop's stages of Split, Plan,
Retrieve, Coverage, Follow-up, Compose and Union. It changes what each
stage does first.
[Loop stages](final-plan-fp16.md#121-the-stages) · [Retrieval rounds](completion-plan.md#retrieval-rounds-inside-one-answer)

### 4.1 Four rules

1. **Find exactly, then read by address.** Ranked Search is the fallback
   for when exact finding fails.
2. **State scope and completeness with every result.**
3. **Cover the task clause by clause.** Each clause ends with cited
   evidence or a named gap.
4. **Run independent reads together.** Only dependent reads need a later
   round.

### 4.2 Stages

| Stage | What it does | Reads | Needs |
| --- | --- | --- | --- |
| 0 Orient | Instruction-file headings and matching sections. Folder roll-up to two levels | Outline, Document Sections, inventory roll-up | 3.2, 3.6 |
| 1 Read the task | Split into clauses. For each clause take the anchors: code-formatted tokens, quoted text, numbers with units, named siblings, named surfaces. Classify the need: locate an owner, list every site, explain behavior, find tests, find rules, trace a path, assess impact or confirm an absence | None | Existing Split and Plan |
| 2 Census | A count scan for each anchor and its variants. An inventory pattern for each artifact kind. If an anchor's map is empty or thin, pivot through documents and census again. If it is broad, keep the files that also hold a second anchor | Scan, inventory, Search for pivots | 3.1, 3.2, 3.4, 3.5 |
| 3 Owners | Choose owners from the file maps: a file that defines the anchor, then the densest file, with source ahead of tests, documents and generated files. Read the enclosing unit of each | Symbols or Definitions, Read File by unit | 3.3 |
| 4 Edges | From each owner take the new names it uses: functions it calls, constants, tables, configuration keys, error names, strings that cross a boundary, a generated-by header. Resolve each and read its unit. Prefer edges that reach an uncovered clause | Symbols, References, scan, Read File | 3.1, 3.3 |
| 5 Closure | For a clause that says "every" or "all", run a second scan in a different form, such as the bare number or the constant's name, and reconcile the two lists | Scan | 3.1 |
| 6 Tests and checks | Tests that name the subject. How those tests stub the subject's collaborators. How tests are run, from scripts, workflows and contributor documents | Inventory pattern, scan in test paths, Code Facts for test declarations | 3.1, 3.2 |
| 7 Rules and documents | Sections and requirements that name the subject. For a "does the document match" clause, pair each documented statement with the code range it describes | Document Sections, Read File by unit | 3.3 |
| 8 Coverage | Mark each clause covered, partial or missing with the reason. Follow up a missing clause that has a concrete anchor. Otherwise name the gap and the next read | The loop's Coverage and Follow-up | Existing |
| 9 Compose | Per clause: the decisive ranges, a site table of path, count and role, absences with their scope, conflicts between document and code, and the choices left to the agent | The loop's Compose and Union | Existing |

Stages 0 to 2 are the census pass of 3.4. Stage 3 is round one. Stage 4 is
round two. Stages 5 to 7 are round three and can run together.

### 4.3 Recipes by kind of task

Each recipe is the order of work the traces followed for that shape of
task. A fixed recipe per task kind is what the plan asks change set 1 to
replay with both models off.

| Shape | Order of work | Traces |
| --- | --- | --- |
| Add a member to a set | Census the rarest sibling. List every registration site. Read one comparable member. Read the storage and compatibility path. Find the tests, generators and documents that enumerate the set | Munarium C1, Bookbuilder C1, Bookbuilder A3 |
| Fix a symptom | Find the symptom's literal and its single enforcement site. Find what writes the state that site trusts. Read the timing or branch that explains "sometimes" or "only for X". Find the nearest test | Munarium C2, Bookbuilder C2 |
| Change a limit | Census the bare number with word boundaries across code, tests and documents. Census the constant names. Decide which enforcement sites are shared and which are copies | Munarium C3 |
| Change how a value is obtained everywhere | Census the operation that obtains it. Read one site and its caller loop. Find the scope that matches the requested lifetime. Find the written rule the change touches | Bookbuilder C3 |
| Write a user page | Find the repository's word for the feature. Read the implementation's rules, limits and errors. Read the existing mentions. List each client surface and one usage example. Find where pages are registered | Munarium W1, Bookbuilder W1 |
| Explain a mechanism and check a document | Read the document section and the owning code side by side. List each statement that differs with both ranges | Munarium W2, Bookbuilder W2 |
| Add tests | Find the property under test and the values it takes. Read the code that turns each value into output, with its helpers. Read the existing test file's pattern, helper and what it can reach | Munarium T1, Bookbuilder T1 |
| List the checks to run | Read the instruction file's pointers. Read the CI workflow's steps and the local gate script. Find generators and drift checks tied to the changed artifact. Read the pull request template | Munarium T2, Bookbuilder T2 |
| Trace a request | Find the anchor both sides share, such as a route or an action name. Follow each called function one hop at a time to the state write. Follow the return path | Munarium A1, Bookbuilder A1 |
| Assess a change of default or platform | Find the setting or platform switch by the shape of its name. Read the parse site, the default and the identity it feeds. Read the policy documents. Find the tests that pin the current behavior and the files that never set it | Munarium A2, Bookbuilder A2 |
| Place a new capability | Outline the catalog it would join. Read the design rules. Find the nearest existing analog. Find the tests that close the set | Munarium A3 |

### 4.4 Where the models help

| Decision | Deterministic rule | Model role |
| --- | --- | --- |
| Which files of a file map to read | A definition site, then density, then path class | Focus ranks candidate units against the clause when the map is large, as with 61 files for one sibling |
| Which pivot terms to try | Identifiers and code-formatted terms in the top document passages | Arbiter scores the candidate terms |
| Which edge to follow next | Edges that reach an uncovered clause, nearest first | Arbiter scores a short roster of grounded choices, as the plan describes |
| Whether a clause is covered | A cited range exists for each anchor of the clause | Arbiter judges whether the range settles the clause, including whether a missing case or an absence is the finding |

### 4.5 Budgets

My runs used about 16 calls and about 570 lines per task. The loop's
default of 132 calls, 256 KiB of retention and a 128 KiB reply is several
times that. Totals are not the constraint. The constraints are the
768-byte search hit, the 3,072-byte targeted read, one follow-up per need
per later round, and three rounds. A site table costs little: 61 paths with
counts is about 5 KB and replaces dozens of excerpts.

### 4.6 Two worked examples

**Munarium C3, change a limit.** Census the numbers 500 and 501 with word
boundaries, with counts per file. Census names that contain a cap word next
to "files" or "chunk". The maps show two
server lines in one file, one CLI line, four client constants and six test
files. Read the units around the two server lines and the CLI line. Resolve
the two server function names and find that both transports call one of
them. Close with a second scan for the four constant names. Scan for the
body-ceiling constant and report that nothing about the file cap touches
it. Report the absence of a server test of the cap, with the scan's scope.
That is three rounds after the census, about a dozen internal reads, and a
reply of one site table and three ranges. My unaided run took five turns.

**Bookbuilder C2, fix a symptom.** Scan for the quoted message: no match in
scope, which is itself the first finding. Scan for the button label and
take the action string on the same line. Scan for that action string and
find the handler. Read the handler as one unit. A coverage check for "only
this provider" finds that the helper's cases name five of the six provider
values. Scan test paths for the handler's name and report that no backend
test names it. That is two rounds after the census. The step a small model
is least likely to make unaided is the first: knowing that the message
belongs to the framework.

### 4.7 What to measure

Use the plan's steering measures, computed by script from native replays
with both models off:

- scenarios whose first delivery covers every essential, and those one
  essential short, with the census pass off and on;
- the same with unit reads off and on, at equal total bytes;
- for each essential, the first stage that delivered it;
- internal scans, files and bytes examined, and time, per request;
- scans that returned a complete absence, and whether an essential
  depended on one.

[Steering measures](completion-plan.md#what-the-gate-needs-and-how-to-steer-toward-it)

## 5. Limits and what to check next

- **These traces are one agent's process on 20 literal-rich prompts.** A
  task stated only in a user's own words would lean on the vocabulary pivot
  much more than these did.
- **The traces are not scored.** I did not open the reference notes, so I
  do not know how many of the 113 essentials my reads covered. Every trace
  lists the ranges it read. The existing coverage script could score them
  against the notes without a model. That would show whether exact finding
  and reading reach the essentials at all, which is the premise of
  sections 3 and 4.
- **Munarium fits are unprobed.** No Munarium index is installed here.
- **The probes ran on an older installed build** and on a live-folder index,
  and Search and Relationships refused the named scope on this connection.
- **Nothing in sections 3 and 4 was built or replayed.** They are proposals.
  The plan's approved experiments, limits and gates are unchanged.
- **This is not evaluation evidence.** The plan assigns all model-assisted
  evaluation and judging to OpenAI models. This analysis was written by an
  Anthropic model at the user's request, as design input.
