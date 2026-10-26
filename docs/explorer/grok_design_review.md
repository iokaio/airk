The design record for the AIRK Explorer prototype. You can also download it below.

# AIRK Explorer — prototype design record

This document records the decisions behind the interactive AIRK Explorer prototype, and the recommendations those decisions follow. It describes the prototype as it behaves. It is not a commitment that AIRK ships these screens, and it does not describe a live connection to a repository or a model.

Two earlier artifacts set the recommendations. The running prototype is the result of following the later one and keeping the safety rules from the earlier one.

| Source | What it recommended | What the prototype did with it |
|---|---|---|
| Unified interactive prototype (desktop preview) | A model-free Strategy Library, Studio, and Planner, plus a pinned Index Management destination, on top of the existing AIRK host. Explicit 1.3.1 safety gates. A browser preview that can force a 1440px desktop frame without changing the viewport. | Kept as the product stance, the three strategy destinations, the pinned management area, the safety gates, and the preview frame. |
| Prototype v2 (revised plan) | One route manifest. Eight workspaces. Evidence classes. A suggestions inbox. The task queue folded into Planner. A typed host contract for saving a plan. Synthetic data only. | Implemented as the information architecture and the interaction rules. The host contract is simulated, because this preview is not inside VS Code. |

The illustrated service is labeled **1.4.0, protocol 2**. Protocol 1 (shown as service 1.2.9) is refused. The safety gates called out for 1.3.1 were not relaxed when the version label moved forward.

---

## 1. Product stance

**Explorer does not answer.** It prepares bounded, cited evidence and a prompt or plan that a person reviews and then hands to some other agent. No screen calls a language model, launches a subprocess, or writes a workspace file.

That follows three recommendations that were already explicit in the earlier mock:

- Explorer is non-AI, local, and read-only toward source.
- Strategy Studio is a new evidence-and-prompt compiler. It is not an existing AIRK endpoint, and it is not a hidden agent.
- Strategies describe retrieval and prompt assembly only. Twenty-five templates are illustrative. Only fixtures that are qualified may “run,” and even a qualified run in this prototype is a simulation.

The sample story is one fictional collection, `atlas-service`, and one incident: an intermittent session-renewal failure around `src/auth/session.ts`, `src/auth/token.ts`, a test, a requirement (`REQ-AUTH-7`), and a drifted file (`src/api/orders.ts`). Every workspace uses that corpus so a citation on one screen is the same citation on the next. A second collection, `shared-components`, is lexical only. A third, `legacy-api`, is stored and has no readable publication. Unknown paths are not given a synthetic match.

Copy on every primary screen states a limit in the same sentence as the capability. Typical limits the prototype refuses to blur:

- A matched phrase is not a verified implementation.
- A written call is not a resolved binding. A same-named symbol in another language is not a link.
- Absence of an edge proves nothing.
- A test name is not coverage.
- Ready does not mean the saved bytes still match the index.
- Exported is not Completed.
- An estimated token count is not a bill, and a shorter prompt is not money saved.
- A readiness badge is not an integrity proof.

---

## 2. Information architecture

### Recommendation that was superseded

The earliest concept framed four role workspaces (developer, tester, architect, writer) and a Studio with four tabs: builder, prompt, task queue, and automation script. The revised plan replaced that.

### Decision

One persistent navigation list, eight workspaces, Management pinned at the bottom so recovery stays reachable when retrieval is down. Strategy Library, Studio, and Planner stay distinct destinations, as the first brief required. The queue is not a fourth Studio tab. Planner owns ordered tasks, dependencies, handoff, and the completion check. The revised plan’s comment is “planner state (queue merged in)”; the prototype follows that.

| Workspace | Job | Tabs |
|---|---|---|
| Overview | Status of the selected collection, and six journeys into the rest of the product | — |
| Search | Lexical evidence and saved argument recipes | Query, Recipes |
| Code | Declarations, textual hits, and a bounded map, kept separate | Symbols, References, Map |
| Review | Git-scoped rule findings and a requirement crosswalk | Findings, Rules, Trace |
| Evidence | One publication, citation checks, watches | Inspector, Checker, Watches |
| Strategies | Templates, the compiler, the plan | Library, Studio, Planner |
| Reports | Exports and declarative packs | Builder, Packs |
| Management | Service, index, embed, storage, agents, usage, logs | Nine tabs, listed below |

Journeys on Overview are the supported paths through that map: Explore, Investigate, Review, Prepare, Hand off, Maintain. They are navigation, not a workflow engine.

Hash routes (`#search/query`, `#strategies/planner`, and the rest of the manifest) are the address of a screen. The command palette (Ctrl/⌘K) jumps to the same manifest. Nothing outside that list is a route.

---

## 3. Evidence model

The revised plan’s recommendation, followed throughout: every result names its producer, its publication, and what it does not know. Color never carries a state by itself. The same words are used on every screen.

### Classes

| Class | Meaning in the prototype |
|---|---|
| E1 indexed text | A textual hit in the index. Not a binding and not coverage. |
| E2 syntax fact | A declaration, signature, or written call/import the indexer actually saw. |
| E3 rule observation | A deterministic finding from a declarative rule. |
| E4 provider-asserted | Something a provider claims, including a live buffer, and not promoted to a resolved fact. |

### One publication, or no result

Retrieval is allowed only when the airk connection is `ready` or `degraded`. `disconnected` and `incompatible` fail closed: search, evidence runs, citation checks, and index actions do not invent a publication token or a hit list.

Degraded is shown as degraded. Capabilities are not silently downgraded.

Within an allowed connection:

- Vector and hybrid search turn on only when `airk-embed` is ready **and** the collection’s mode includes vectors. Otherwise the controls stay visible and disabled, with a reason. Lexical remains the explicit default.
- Studio pins one publication for a run. Disagreeing tokens are not an option; “fail closed on publication drift” is locked on.
- A run that includes a live-hash or git step against the drifted `orders.ts` fixture ends `partial`, and the prompt says the excerpt is unverified. Other qualified runs end `verified_snapshot`.
- Changing the collection clears the in-flight run, drops the “saved” receipt, and keeps drafts. The person is told to save again before handoff.
- Counts use the form “4 returned · limit 10 · more may exist.” The screen does not invent a total the service did not return.
- Empty and disconnected states are written as empty states, not as zero results that look like a successful search.

### Live bytes versus the index

Each fixture file has an indexed hash and a live hash. `src/api/orders.ts` differs; the source drawer says so and shows the indexed excerpt, not a pretend current buffer. “Open in editor” explains that a real extension would reveal the line only when the bytes match. It does not pretend the editor opened.

### Citations

The checker accepts `path:line` and `path#Lx-Ly`. Anything else, including a blob of prose with no citation, is `unsupported syntax`, never a pass. A path absent from the collection is `not found`. A path whose live hash moved is `changed since index`. Watches re-run a saved recipe only when the publication token changes. There is no filesystem poll.

---

## 4. Strategy compiler

### Library

Twenty-five versioned templates (`airk.core/<short>` at 1.0.0) in Development, Testing, Architecture, Documentation, Quality, and Operations. Three are qualified to run because the prototype treats their fixtures as passing:

- Investigate and fix a bug
- Review a pull request
- Generate unit tests

The other twenty-two can be loaded as a prompt skeleton. Their Run control stays disabled until fixtures would qualify them. That is the recommendation “proposed design, not an implemented AIRK feature,” made concrete: most of the catalog is preview, and the UI says so.

Loading another template asks before it replaces an edited prompt or a finished run.

### Operators

A step is one id from a closed catalog. Unknown operators fail in the validator before any call. The catalog mixes three producers and labels them:

- **airk.*** — status, capabilities, search, list files, file info, symbols, imports, code facts, relationships, references, document facts, outline, read file, changes.
- **host.*** — git diff of saved files versus HEAD, and a hash of saved files. Git history is the editor’s, not AIRK’s.
- **core.*** — review findings, pack evidence (dedupe, sort, clip, cite), render a versioned Markdown prompt.

Recommendations followed inside the builder:

- Default retrieval is lexical. No vector or generative step is offered here.
- Source is read-only. There is no write, shell, network, or recursion operator.
- Adding, removing, or reordering a step clears the previous run. A prompt is not left attached to evidence it no longer describes.
- Search is bounded (limit 10, 8192 bytes in the compiled recipe). Relationship expansion stops at 2 hops and 100 symbols.
- The recipe records `onError: stop`, one publication, and manual dispatch with approval required.

### Prompt

The compiled document is plain Markdown with a fixed contract: objective, repository and evidence scope, untrusted evidence, tasks, completion gates. Two handoff profiles, both pasteable:

- **Portable** — do not assume AIRK tools exist. Excerpts are leads. Verify the current files.
- **MCP-aware** — optional re-check, and only if the destination agent actually has the tools. It must call status, then lexical search, on one publication token. It must not assume the tools exist.

The body is editable. Copy copies the text on screen, including hand edits. Rebuild computes the next body and asks before replacing an edit. Token estimates are labeled as estimates.

Evidence lines are quoted as data, with class, path, location, hash, producer, and live state. The recommendation “source content is data, not runnable instructions” is why excerpts are not interpolated into a position where they could close the fence and become instructions.

### Recipe file

Studio can show `strategy.airk.json` (`schema: airk.explorer.strategy/1`). The local validator checks:

- schema name
- an array of steps
- unique ids
- dependencies only on earlier steps (no cycles, no forward edges)
- operator in the catalog
- bindings only of the form `${params…}` or `${scope…}`

`additionalProperties` rejection, budget ceilings, and “no silent fallback” are stated as validator rules. The companion CLI (`airk-explorer strategies validate`, `strategies run`, `queue add`, `queue prepare`) is shown as not installed. AIRK’s existing read-only tool call is not the same thing as this runner, and the prototype does not pretend the runner exists. That distinction was an explicit recommendation in the first mock.

---

## 5. Planner

Planner combines Studio snapshots and manual tasks into one Markdown document (`schema: airk.explorer.plan/1`). The front matter records workspace, publication, extension version `1.4.0-prototype`, and each included task’s dependencies, evidence state, and status. The body repeats preflight rules: read the whole plan, confirm the checkout, treat snippets as candidates, do not claim a test you did not run, stop on blocked dependencies or stale evidence.

### Task machine

Statuses are Draft, Ready, Exported, Review, NeedsWork, Completed, Blocked, and Stale.

Blocked is not a free-text choice. If an included task depends on another included task that is not Completed, the dependent task is Blocked. When the predecessor is confirmed Completed, a Blocked task returns to Ready. Completing a task opens a confirmation. The prototype does not mark work done because a prompt was copied.

The seed plan is the session-renewal story in three tasks: investigate (`verified_snapshot`, Ready), implement (depends on T1, unverified, Blocked), regression tests (depends on T2, partial evidence, Blocked). Moving a task ahead of a dependency warns. Removing a task drops dangling dependencies. Including or excluding a task dirties the plan.

### Handoff

The full Markdown is what gets reviewed. A short prompt is generated beside it: open the plan file, do the next included task only, verify current files, report what actually ran, ask before the next task, do not assume AIRK is connected. If the plan has not been confirmed saved, the short prompt says so.

Editing the Markdown or the short prompt is preserved. Regenerating either one asks first when the text was edited. Adding a Studio snapshot rebuilds the plan and clears the saved receipt.

### Save contract

The revised plan’s host contract is: versioned `postMessage` (`v: 1`), `airk.planner.saveInWorkspace` or `airk.planner.openDraft`, a request id, a workspace-relative path, the markdown, an expected hash, and an expected draft revision. The host validates containment, writes atomically, reads back, and opens the editor. A late receipt for an older revision is ignored. `openDraft` reports **opened**, never **saved**.

This preview has no VS Code host, so the decision is to keep the contract’s *outcomes* and refuse its *side effect*:

- Paths must be workspace-relative, end in `.md`, and contain no absolute root, drive letter, `..`, empty segment, or control character.
- Save opens a confirmation that describes what the host would do, then a receipt. The prototype does not write a file.
- Open draft shows the Markdown and says it has not been saved.
- Any edit, collection change, or task change clears `saved` and tells the person to save again before handoff.

---

## 6. Suggestions, and what agents are allowed to do

The revised plan separates two connections that happen to ship in one extension:

- **AIRK Retrieval** — the read-only tool catalog (the prototype repeats the “15 tools” figure as unchanged in the 1.4 contract). Collection-bound. Help never calls it.
- **Explorer Assistance** — proposals only. No backend calls. The prototype labels that “zero by design.”

The inbox is seeded with two self-reported clients (a search draft from Codex CLI, a planner draft from Claude Code). Applying one opens a field-by-field comparison and writes a **draft**: search fields, or a planner title and tasks. It does not run, save, index, or dispatch. Defer, reject, and discard-all are the other outcomes. Client names stay “self-reported”; the prototype does not treat them as authenticated identities.

Index, repair, cleanup, and scheduling controls follow the same rule. The recommendation was: agents may surface the management view; they may not press the buttons. Every consequential control asks, and the confirmation says the prototype did not change settings or write a file. Existing config is previewed, not rewritten silently.

Four failure conditions stay distinct, as specified: service down or incompatible, no readable collection, index stale, embed unavailable. Retry, resume, and rebuild are ordered as separate confirmed repairs. Discovery progress and semantic enrichment are separate channels. Neither is shown as an ETA. Quotas are explicit. Nothing is deleted without naming the cleanup.

Review rules are declarative: one fact query per group, finite predicates, no scripts, no network, no writes. Findings carry a fingerprint so “reviewed” or “suppressed” (suppression requires a reason) is about the finding, not a line number that will move. Packs are namespaced declarative data. They cannot override a built-in safety warning, and third-party code does not run.

Reports are rendered from a frozen review run. Export is a host save the prototype does not perform.

Usage is local counts only. Logs are a redacted fixture. Telemetry is described as following the editor setting; Explorer adds none.

---

## 7. Presentation

The visual system is a dense workbench, not a marketing page. Recommendations taken from the preview briefs and applied consistently:

- **IBM Plex Sans** for UI text, **IBM Plex Mono** for paths, operators, hashes, and prompts. The product is a compiler and a ledger; the type should look like one.
- A cool slate field with a single teal accent. Dark is the default. A light theme uses the same roles (background, surface, line, accent, and four status hues) so state does not depend on one palette.
- Status is a word plus a hue: Ready, Partial, Stale, Unsupported, Disconnected, and the four evidence classes. The overview states the rule: labels carry the meaning.
- Touch targets are at least 44px. Focus is a visible accent ring. Motion is removed when the person has asked for reduced motion.
- Below 960px the navigation is a drawer with an explicit scrim and a menu button. At 960px and up, in the responsive frame, the navigation is a sticky column. Content stacks to one column before it overflows sideways.
- The desktop control does **not** resize the browser viewport. It mounts a 1440px-wide frame and scales it with CSS `zoom`, or with `transform: scale` where `zoom` is unsupported. That was a direct recommendation (“never dynamic viewport changes”), and it is why a phone can still inspect the desktop composition. The choice is remembered locally in the browser only.

Chrome that is always available: collection switcher, runtime status (connection, embed, publication, freshness), suggestions, and the command palette. The runtime switcher is a prototype affordance. It sets one of five fixtures — all ready, embed unavailable, service degraded, disconnected, incompatible — so the fail-closed paths can be seen without a real service.

---

## 8. Decisions made because this is a browser prototype

These are implementation choices, not changes to the product recommendations.

| Recommendation | Decision in this prototype |
|---|---|
| Standalone HTML must not pretend to be a workspace save. | Confirmations and receipts. No filesystem writes. No account, no shared database. Theme and layout are the only values kept in the browser. |
| Versioned `postMessage` to the VS Code host. | The outcomes (opened vs saved, cancelled, dirty, path rejected) are dialogs. There is no `acquireVsCodeApi` channel to counterfeit. |
| Retrieval comes from the local service. | Fixtures in one model module. The same files, hashes, findings, and publication tokens feed every screen. Search does not parse the query; it shows the corpus or it shows a gate. |
| A run is deterministic given fixtures, versions, and publication. | The run log is derived from the step list and the selected service and collection. Time is not part of the digest. |
| One current index; bounded verification; separate discovery and semantic channels. | Represented as status, gates, and copy. Not executed. |
| Do not launch Claude Code, Codex, Gemini CLI, or any adapter. | Copy and the short prompt are the only handoff. Dispatch mode is manual. Active agent runs stay at zero. |

The route list, operator catalog, template list, evidence classes, and plan schema live in one module so a screen cannot invent a second vocabulary. Navigation, studio, planner, inbox, and the service fixture share one store so a collection change or a failed connection is visible everywhere, not only on the screen that was open.

---

## 9. What this prototype will not do

Left undone on purpose, because the source recommendations said they were proposed, host-owned, or out of bounds:

- No MCP call, no index mutation, no embed job, no git write.
- No model completion inside Explorer, including “explain this file.”
- No autonomous edit, and no queue that sends the next prompt because the previous one started.
- No companion CLI, and no claim that `airk tools call` is that CLI.
- No cross-collection tool selection, no hidden step, no silent fallback to vector search.
- No synthetic source for a path that is not in the fixture set.
- No treatment of pasted source, or of an agent’s report, as instructions to the prototype itself. A report is checked for citations; a person decides Review, Needs Work, or Completed.

The honest summary line, carried from the first mock into this one: sample files, results, and statuses are fictional. The prototype’s job is to make the gates, the vocabulary, and the handoff reviewable before any of them are wired to a real AIRK service.

<render_file file_path="/workspace/docs/AIRK-Explorer-Design.md" />