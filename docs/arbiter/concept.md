# AIRK Arbiter: Technical Implementation Plan

Nov 10, 2026 · @Tyler Jensen

AIRK Arbiter is an opt-in tier of AIRK for workstations with a 16 GB or larger local GPU. It adds a resident local decision runtime that scores retrieval routes, evidence relevance and next steps without generating tokens, and it leaves existing AIRK binaries, indexes and MCP contracts unchanged.

## Name and scope

The proposed name is **AIRK Arbiter**, replacing the working title "AIRK Extended" and the internal term "Decision Kernel". An arbiter weighs supplied candidates and returns a bounded judgment or declines to rule, which is exactly the component's contract. It arbitrates among candidates; AIRK and the host keep authority over reads and actions.

| Name | Identifier prefix | Why it fits | Why not chosen |
| --- | --- | --- | --- |
| **AIRK Arbiter** (recommended) | `airk-arbiter`, `airk.arbiter.*` | Describes typed judgment over candidates with explicit abstention | "Arbiter" can imply final authority; the contract below says advice only |
| AIRK Counsel | `airk-counsel` | States the advice-only role directly | Reads as a chat or legal feature |
| AIRK Discern | `airk-discern` | Names evidence discrimination | Verb form is awkward as a product and binary name |
| AIRK Vantage | `airk-vantage` | Signals a higher tier | Says nothing about what the tier does |

Naming used in the rest of this plan:

- **AIRK Arbiter**: the tier and the user-facing setting.
- `airk-arbiter`: the proposed sidecar process that owns the GPU decision model.
- `airk.arbiter.advice/1`, `airk.arbiter.packet/1`: the proposed versioned envelopes.
- **Arbiter pack**: the provisioned model, tokenizer, runner and qualification assets.

The figures are carried over unchanged from the three source documents, so their labels still say "Decision Kernel". Read that label as Arbiter.

### What the tier adds

1. Typed, non-generative decisions (Choice, Binary, Ordinal) over retrieval requests, evidence and next steps.
2. A task-specific selected-evidence projection over native AIRK results.
3. An optional host controller that substitutes local decisions for routine agent control turns.
4. A local training and calibration loop that produces immutable task bundles.

### What stays in AIRK and the host

- AIRK resolves literal targets, validates plans and bindings, selects the exact publication and executes permitted reads.
- The host decides when to call a generator, which model to use, and authorizes edits, commands and tests.
- Arbiter never generates patches, prose, plans or JSON, and never invents a source hash, publication token, symbol id, file path or resolved relationship.

Source baseline: [iokaio/airk at b624b31](https://github.com/iokaio/airk/tree/b624b31c32c140d66c80f8daf3c62175e4719fcb), source versions 1.8.7. That snapshot was inspected, not built or benchmarked. Every interface, budget and target below is a proposal to validate.

## Compatibility contract

Arbiter is strictly additive: an AIRK install with Arbiter disabled, absent or failed behaves byte-for-byte like AIRK without it. These invariants are release gates.

| Surface | Invariant | Mechanism |
| --- | --- | --- |
| `airk` service, client and worker binaries | No transformer runtime, no second scheduler, no new mandatory dependency | Core gains only inert contract types and a capability probe; inference lives in `airk-arbiter` |
| `airk-embed` | Existing embedding protocol unchanged; `Request::Plan` keeps meaning token planning (`PlanningTokens`) | New scoring uses a separate connector or the sidecar, never an overloaded embed request |
| Indexes and publications | No schema migration, no re-index, no new stored representation required | Arbiter reads through canonical handlers; its own state lives in a separate store |
| `airk.parse.request/1`, `airk.parse.plan/1` | Unchanged; both deny unknown fields today | Decision provenance travels in the separate `airk.arbiter.advice/1` envelope |
| MCP catalog | The 28 unified tools and 18 native tools keep names, arguments and response modes | Arbiter surfaces are new, capability-gated tools; none replaces an existing tool |
| Byte budgets | `max_bytes` keeps its defined meaning | Generator-token budgets are a new, separately accounted field in the packet |
| Intent parser | Slash commands and deterministic grammar run first; `intent-broader/5` keeps `automatic_execution: false` and `proposal_confirmation_required: true` | Arbiter is consulted only after grammar and only on the learned-fallback population |
| Plan runner limits | 6 steps, 12 reads, 12 fan-out targets, 512 KiB materialized payload | Arbiter proposals compile into plans inside those limits or start an explicit new turn |
| Studio | 3 Runnable templates, closed operator catalog, lexical search operator | Arbiter may suggest among Runnable strategies; it cannot make Preview templates executable or change operator semantics |
| Usage ledger | Content-free; `provider_token_usage_observed` stays false, `estimated_bill_savings` stays null | Arbiter metrics go to a separate bounded aggregate; `paired_task` is not relabeled |

### Version and capability negotiation

- `airk` advertises an `arbiter` capability only when the sidecar is provisioned, warmed and passing its fixtures. Clients that never ask for it see no difference.
- The sidecar declares the AIRK contract versions it was qualified against. A mismatch disables Arbiter and returns `unavailable`; it never downgrades silently.
- Arbiter outputs carry their own schema ids. Older AIRK clients ignore tools they do not list; newer clients must tolerate the capability being absent.
- Explicit commands, direct tool arguments, user-selected operations and reviewed plans bypass Arbiter entirely.

### Failure behavior

- Sidecar missing, GPU saturated, model not loaded, deadline exceeded: return `unavailable` and use the existing AIRK path or review policy.
- Lexical reads and status never wait on the Arbiter queue.
- No model download during a read, and no automatic cloud fallback.

## Hardware qualification and packaging

Arbiter enables only on a machine that passes a measured GPU probe; a marketed VRAM figure is not sufficient. The reference target is a 16 GB discrete GPU with 32 GB host RAM and an SSD.

### Tiers

| Tier | Runs | Hardware | Ships as |
| --- | --- | --- | --- |
| T0 Baseline (today's AIRK) | Exact rules, deterministic grammar, the 852,116-byte sparse intent scorer | CPU | AIRK |
| T1 Registered heads | Trained heads over frozen encoder features (MiniLM INT8, about 23 MB of weights) | CPU, via `airk-embed` features where recipes match | AIRK, opt-in task bundles |
| T2 Arbiter compact | One resident 0.8B to 2B decision model, 2,048-token default profile | GPU with 16 GB VRAM | Arbiter pack |
| T3 Arbiter extended | Same model at the 4,096-token profile, or a larger quantized model on 24 GB and up | GPU with 16 GB (profile) or 24 GB+ (model) | Arbiter pack, separately qualified profile |

T1 is useful without a GPU and is the fallback when T2 is unavailable. T2 and T3 are what the Arbiter setting gates.

### GPU probe

The probe runs at enable time and after any driver, runtime or pack change. It records results in the pack's activation receipt.

1. Enumerate adapters; reject integrated and shared-memory devices for T2 unless separately qualified.
2. Identify backend: NVIDIA CUDA, AMD Vulkan or ROCm, each with its own qualification record.
3. Check free VRAM against the profile ceiling (12 GiB process allocation for the 16 GB profile) with the user's normal desktop load.
4. Load the pinned model, run the warmup fixtures, and compare typed outputs against reference scores within declared tolerances.
5. Measure warm p50 and p95 on the fixture set; fail the profile if targets are missed by the configured margin.
6. Write `qualified`, `qualified_reduced` (T2 only, no extended profile) or `not_qualified` with the failing step.

A failed probe leaves AIRK at T0 or T1 and reports the reason in status. It never retries in the background.

### Delivery options

| Aspect | A. Separate install | B. Bundled, opt-in |
| --- | --- | --- |
| What ships in AIRK | Inert contracts and the capability probe only | Contracts, probe, the `airk-arbiter` adapter binary and the settings UI |
| What the user installs | An Arbiter extension or installer carrying adapter, runner and assets | Nothing extra; enabling the setting triggers explicit provisioning of runner and model assets |
| Base AIRK download | Unchanged | Grows by the adapter binary only; model and accelerator runtime stay out of the VSIX |
| Release cadence | Independent; Arbiter must pin the AIRK contract versions it supports | Coupled; one version matrix, one qualification run |
| Failure isolation | Strongest; a broken Arbiter cannot affect an AIRK update | Requires the adapter to be inert until activated |
| Discoverability | Lower; power users find it | A disabled setting with a "check my GPU" action |
| Accelerator runtimes (CUDA and similar) | Installer owns them | On-demand download, reported separately from model bytes |

**Recommended: B with assets on demand.** Ship the small adapter and probe inside AIRK, off by default. The setting `airk.arbiter.enabled` stays unavailable until the probe returns `qualified`. Enabling it runs explicit provisioning of the Arbiter pack from the asset manifest. This keeps one compatibility matrix while keeping roughly 2 GB of model assets and any accelerator runtime out of the base install. If the adapter cannot be kept inert in practice, fall back to A; the contracts are identical in both.

Size targets, subject to release measurement: under 150 MB for the CPU install, under 2 GB of additional model assets for the compact GPU pack, accelerator runtime reported separately.

## Architecture

Three components keep three responsibilities: AIRK supplies and validates evidence, Arbiter returns bounded advice over that evidence, and the agent host owns generation and actions.

&#91;image: Integration: AIRK retrieves scoped evidence, the decision component returns bounded advice, the agent host retains action authority\]

Figure 1. Integration: AIRK retrieves scoped evidence, Arbiter returns bounded advice and a smaller evidence set, the host retains action authority.

| Component | Owns | Never does |
| --- | --- | --- |
| AIRK (`airk`, `airk-embed`) | Indexing, retrieval, target resolution, plan validation, publication identity, read execution | Reasoning, answer generation, model-call routing |
| Arbiter (`airk-arbiter`) | Typed decisions, calibration, acceptance policy, evidence selection scores | Read execution, identity minting, generation, external calls |
| Agent host | Generator calls, model choice, task budget, edits, commands, tests | Bypass AIRK plan validation |

### Request path

1. The adapter receives the information need, authorized scope, declared budget and current AIRK capabilities.
2. Exact rules and deterministic parsing run first. If they resolve the request, Arbiter is not invoked.
3. Otherwise the adapter invokes the smallest eligible Arbiter task and receives an advice envelope.
4. Accepted advice is validated and compiled into existing AIRK operations. AIRK resolves targets, checks bindings and executes permitted reads.
5. An optional selector scores the native result and assembles a packet of original records.
6. The host receives the packet with citations, identities, coverage limits and explicit omissions, then continues locally, requests review or calls a generator.

### Process model

One process owns the GPU weights. `airk-arbiter` is a persistent same-host sidecar bound to loopback, sitting beside `airk-embed` in the existing process model. Source content travels directly between scoped workers and the inference helper, not through the metadata broker, as it does for embeddings today.

&#91;image: Local runtime: applications, kernel adapter, CPU fast path, native runner, shared asset store and one resident GPU model\]

Figure 2. Local runtime: clients share one kernel adapter, a CPU fast path, one resident GPU runner and a shared asset store.

- **Adapter**: task registry, input budgets, compilation of questions and candidates, calibration and acceptance policy, status mapping.
- **CPU fast path**: sparse scorer and encoder heads; no GPU dependency.
- **Native runner**: pinned llama.cpp decision endpoint holding one resident model.
- **Shared asset store**: content-addressed, machine-wide, reused across projects and task bundles.
- **In-process option**: requires a qualified native shim or a complete ONNX pipeline; no Python inference dependency in either case.

### Route cascade

Policy selects an eligible route before execution. It does not try every installed model until one is confident.

&#91;image: Decision flow: CPU registered route and GPU flexible route share calibration and policy, ending in a typed result for the host workflow\]

Figure 3. Decision flow: registered (CPU) and flexible (GPU) routes share one calibration, applicability and acceptance contract; only qualified cases escalate.

- **Registered route**: a task-specific view of state is encoded once and reused across compatible heads. Different feature recipes do not share an embedding.
- **Flexible route**: a bounded input holds the question, explicit label descriptions or rubric, selected state and evidence references. The model scores candidates jointly with that context in one forward pass.
- **Escalation** applies only to task versions whose full cascade has been evaluated. Missing evidence triggers retrieval or abstention, not a larger model.
- **Exact decision cache**: hit only when task, policy, bundle, exact input, evidence revision and authorized scope match. No approximate semantic cache.
- **One resident GPU model.** Task heads are cheap; a LoRA adapter still needs its base, and a merged adapter is another full set of weights unless the runtime shares the base correctly.

## Local boundary

Every stage that touches request content, evidence, labels or learned artifacts runs on the local machine, and the build is tested with outbound networking blocked.

&#91;image: Boundary: sensitive context, local decisions, private learning and internal workflow inside the controlled environment, with no request path to an external model provider\]

Figure 4. Boundary: context, decisions, learning and workflow stay inside the controlled environment; there is no request path to an external model provider.

| Stage or artifact | Rule |
| --- | --- |
| Ingestion and preprocessing | Parse and extract locally; URLs inside evidence never trigger a fetch |
| Retrieval and features | Text, embeddings, indexes and feature caches stay local with source permissions and revision identities |
| Decision requests and results | In-process call or protected loopback; no external model fallback under load or failure |
| Annotation and evaluation | Examples, labels, local-teacher outputs, test sets and experiment tracking stay local |
| Model artifacts | Heads, adapters, checkpoints and rollback bundles are bound to the project and user that produced them |
| Diagnostics | Bounded operational metadata by default; content-bearing traces need explicit local capture, including crash dumps and temp files |
| Updates | Public assets arrive through explicit provisioning; nothing private is uploaded as part of an update |

### Limits of the boundary

- Arbiter being local does not make a hosted coding agent private. Evidence selected for a hosted generator still crosses the boundary; Arbiter only reduces how much.
- Data-destination policy is enforced in host code before any learned routing. A relevance score is not an authorization decision and not a secret scan.
- Reusing one runtime does not authorize sharing labels, adapters or cached features across projects or users.
- Typed outputs, reason codes and input hashes can themselves be sensitive; the host controls where they go.
- Retrieved text that contains instructions is evidence, never authority to change scope, run tools or alter the workflow.

### Library controls

Set Hugging Face Hub offline and telemetry controls after provisioning, disable runner telemetry and background model updates, then verify the whole running pipeline under the network policy. An environment variable alone is not proof.

## Decision tasks

Arbiter exposes eight independently versioned tasks, each with a typed answer and its own acceptance threshold. The ids are proposed; none exists in AIRK today.

| Task id | Answer type | Decides | First route | Hard boundary |
| --- | --- | --- | --- | --- |
| `airk.request_supported.v1` | Binary + abstention | Whether a request is a supported bounded read task | T0/T1 | Rejects mutation ("change every use of X"), unsupported reasoning ("prove the queue is thread-safe") and ambiguous targets |
| `airk.retrieval_route.v1` | Choice | Definition lookup, literal occurrence, document evidence, test mentions, or broader investigation; lexical vs semantic vs hybrid mode | T1, T2 on fallback | Cannot pick semantic search when that representation is absent; ANN similarity is not a correctness probability |
| `airk.recipe_choice.v1` | Choice | Which implemented recipe, and which bounded next branch from verified intermediate results | T1/T2 | Candidates come only from the implemented catalog; plan limits apply |
| `airk.evidence_relevance.v1` | Binary or Ordinal rubric | Query-to-record relevance for packet selection | T2 | Never filters an exhaustive exact lookup; contradictions are retained |
| `airk.evidence_role.v1` | Choice (multilabel only if separately qualified) | Declaration, constraint, example, test, other defined roles | T1/T2 | Does not upgrade the evidentiary basis of a fact |
| `airk.next_evidence_step.v1` | Choice | Expand a cited declaration, inspect a test candidate, clarify a target, or hand off to synthesis | T2 | "Ready for synthesis" means preparation is complete, not that behavior is proven |
| `airk.claim_support.v1` | Choice | Support, contradiction or insufficient information for an explicitly supplied claim | T2 | Does not extract claims from prose; a test asserting X is not X being true; syntax facts are not runtime behavior |
| `agent.generation_route.v1` | Choice (host-owned) | Which eligible generator handles the remaining work | Host | Code enforces provider and data-destination eligibility first |

### Answer primitives

- **Choice**: probabilities over registered label ids. The top option can lead while every option is inappropriate, so applicability is a separate check.
- **Binary**: an accepted `false` is distinct from insufficient evidence and from abstention.
- **Ordinal**: a distribution over rubric levels; the adapter recomputes expectation and modal level from that distribution.
- Ranking is built from candidate scores or repeated relevance judgments. Extraction is out of scope.
- When several labels can apply, use separate Binary tasks or an explicitly qualified multilabel head, never a forced single choice.

### Task rules

- A fixed head cannot be repurposed by a new prompt. A flexible task is qualified per question family, language, candidate count and context size.
- Retrieval supplies current source facts; training supplies stable decision behavior. Weights never stand in for repository state.
- Candidate count is a resource and quality parameter. Bound both original input and final model tokens.
- A flexible-question mode is an explicit versioned capability extension, not extra instructions slipped into a registered v1 task.

## Advice envelope

Arbiter returns one envelope type, `airk.arbiter.advice/1`, kept separate from AIRK's parse contracts because those deny unknown fields. Accepted advice remains advice until AIRK and the host validate the resulting operation.

Illustrative shape (field names are proposals):

```json
{
  "schema": "airk.arbiter.advice/1",
  "task": { "id": "airk.recipe_choice.v1", "version": "1.0.0" },
  "bundle": {
    "id": "sha256:...",
    "base_model": "sha256:...",
    "head_or_adapter": "sha256:...",
    "tokenizer": "sha256:...",
    "compiler": "question-compiler/3",
    "quantization": "Q4_K_M",
    "runtime": "llama.cpp@<pinned-commit>",
    "backend": "cuda"
  },
  "policy": { "calibration": "sha256:...", "acceptance": "sha256:..." },
  "scope": { "project": "...", "path_prefix": "src/", "publication": "..." },
  "input": { "recipe": "feature-recipe/2", "digest": "sha256:...", "tokens": 1184 },
  "evidence": [ { "source_hash": "...", "representation": "...", "location": "..." } ],
  "status": "accepted",
  "answer": {
    "type": "choice",
    "label": "definition",
    "distribution": { "definition": 0.91, "occurrences": 0.06, "document": 0.03 }
  },
  "recommendation": {
    "recipe": { "id": "definition", "parameters": { "name": "QueueClient" } }
  },
  "timing_ms": { "queue": 2, "compile": 1, "forward": 84, "policy": 0 }
}
```

The `recommendation.recipe` block is compiled by the adapter into an ordinary `airk.parse.request/1` in `prepare` mode. Preparation is not execution, and AIRK's digest and execution checks still apply. Literal values such as `QueueClient` come from the request or an authorized verified inventory, never from the model.

### Status values

| Status | Meaning | Caller behavior |
| --- | --- | --- |
| `accepted` | Applicable, evidence sufficient, calibrated score above the task threshold | Validate and compile; still subject to AIRK and host checks |
| `abstained` | Applicable, but uncertainty or evidence falls short | No actionable answer; retrieve more, clarify, or use the existing path |
| `unsupported` | Request is outside the task's qualified scope | Existing AIRK path or review |
| `invalid_input` | Malformed, over budget, or unresolvable inputs | Fix the request; do not retry unchanged |
| `unavailable` | Sidecar, model or capacity missing; deadline exceeded | Declared fallback; never an external call |

These five never collapse into an empty evidence result.

### Acceptance

Acceptance requires four independent conditions: applicability, adequate evidence, a calibrated task threshold, and a qualified execution profile.

- The existing intent scorer emits integer scores and a margin, not probabilities. Its one-versus-rest training includes unsupported examples with all-zero targets, so a softmax over its scores would force a winner. The adapter preserves those semantics or qualifies a replacement separately.
- A provider confidence field is input to policy, not an acceptance status.
- Thresholds are per task and per relevant slice. A wrong route wastes turns; a wrong sufficiency judgment can induce an unsupported patch, so tolerances differ.
- Any change to quantization, feature recipe, model, compiler or backend invalidates calibration until the held-out partition is rescored.
- Learned proposal acceptance does not enable automatic execution. That is a separately qualified, explicitly enabled mode.

### Bundle identity

A task bundle is immutable and pins: base weights, head or adapter, tokenizer, prompt compiler and template, candidate ordering and token mapping, attention and masking strategy, quantization, runtime build, backend, calibration, policy and retrieval recipe. Provenance hashes and reason codes are separate fields from any explanation text.

## Evidence packet

The packet, `airk.arbiter.packet/1`, is a selected view of original AIRK records, never a model-written substitute and never a replacement for the native result.

| Field group | Contents |
| --- | --- |
| Scope and identities | Project or collection, exact publication, source hashes, representation identities, source locations |
| Task and selection identity | Task version, selection recipe, model bundle, acceptance policy |
| Selected records | Complete original excerpts or facts, each with its evidence role and preserved syntax, imported or candidate basis |
| Coverage and omissions | Native extraction and read limits, truncation, omitted-record counts, whether more can be requested |
| Budget accounting | Serialized bytes, tokenizer identity when tokens are counted, selection work, remaining expansion allowance |
| Continuation | A valid route to more evidence that does not pretend expired handles are current |

### Selection algorithm

1. Retrieve a bounded candidate set through canonical handlers (initial tuning input: 20 candidates). Measure candidate recall here; the selector cannot recover what retrieval missed.
2. Deduplicate exact overlaps in code.
3. Reserve mandatory metadata and required facts before spending any context budget.
4. Score each remaining record with `airk.evidence_relevance.v1` and assign a role with `airk.evidence_role.v1`.
5. Greedy select by relevance per token under role-coverage and source-diversity constraints: at least one declaration, its controlling constraint, a representative use and a test where they exist.
6. Force-retain contradicting records and policy exceptions regardless of score.
7. Emit intact records only; never truncate inside a record to fit.
8. Record omissions and the expansion route.

Code enforces scope, budget, duplicates, required roles and intact records. The model supplies relevance estimates only.

### Rules

- The complete native result stays retrievable. The Workbench read adapter requests full responses and rich facts by default, so the packet is a separate projection, not a change to what the UI retains.
- Keep required metadata even when that makes a very small packet impossible.
- Token budgets are counted with the generator's tokenizer contract when available; otherwise the uncertainty is recorded.
- A deterministic baseline (native ranking plus grouping and diversity) is implemented first. If it performs as well on task outcomes, it ships and the learned stage is dropped. This revisits AIRK's standing no-reranker guidance, so the learned selector must beat that baseline to exist.

### Evidence manifest and reuse

For long tasks the host keeps a manifest of what it possesses versus what the current model context contains.

- AIRK's `changes`, representation validators and conditional reads stay authoritative. Arbiter assesses relevance of changed records only after exact validation.
- `not_modified` helps only if the host still holds the matching evidence. After truncation or session transfer, content is supplied again.
- A source hash alone does not establish live-file freshness.
- Exact reuse, prompt-cache reuse, index caching and decision caching are four mechanisms with four cache keys. None justifies replacing a needed passage with an identifier the generator cannot resolve.

## Source integration seams

Six seams in the inspected source carry the change; the first three are in the native Workbench implementation. Locations are verified at b624b31; the changes are proposals, not patches.

| Seam | Location | Change | Must preserve |
| --- | --- | --- | --- |
| Parse fallback | [`airk-kernel/src/workbench.rs`](https://github.com/iokaio/airk/blob/b624b31c32c140d66c80f8daf3c62175e4719fcb/crates/airk-kernel/src/workbench.rs) `parse_text` | Consult the Arbiter adapter after explicit commands and finite grammar | Deterministic behavior and learned-review defaults |
| Recipe construction | `proposed_steps` and native recipe construction | Translate an accepted recommendation into existing operation candidates | Targets resolved and arguments validated in code; no model-authored instructions executed |
| Plan provenance | `evidence_plan_with_reader` | Associate a separately versioned advice record with a prepared plan | Scope, publication, catalog, digest, binding, budget and execution checks |
| Packet projection | Canonical result handlers plus a new adapter beside [`airk-mcp/src/workbench.rs`](https://github.com/iokaio/airk/blob/b624b31c32c140d66c80f8daf3c62175e4719fcb/crates/airk-mcp/src/workbench.rs) | Produce the selected-evidence view from preserved native results | Citations, completeness, warnings, raw-result access |
| Intent scorer | [`airk-retrieval/src/intent.rs`](https://github.com/iokaio/airk/blob/b624b31c32c140d66c80f8daf3c62175e4719fcb/crates/airk-retrieval/src/intent.rs) | Expose scores and margin to the adapter unchanged; optional registered head as an alternative scorer | 13 fixed intents, integer score semantics, acceptance decision |
| Core contracts | [`airk-core/src/workbench.rs`](https://github.com/iokaio/airk/blob/b624b31c32c140d66c80f8daf3c62175e4719fcb/crates/airk-core/src/workbench.rs) | Add inert advice and packet types in a new module | Existing request and plan structs untouched |

### New components

- **`airk-arbiter` adapter crate and binary**: task registry, compiler, calibration, policy, admission queue, status mapping, loopback server.
- **Runner integration**: client for the pinned llama.cpp decision endpoint; optional ONNX Runtime path for in-process use.
- **Arbiter store**: bundles, calibration tables, activation receipts, decision cache and bounded aggregate metrics, kept outside AIRK's index store.
- **Feature connector**: an explicit connector to compatible frozen features from `airk-embed`.
- **Host adapter** (separate package): the controller that substitutes decisions for agent control turns.
- **Training tools** (Python, not shipped to end users): label tooling, head and adapter training, export, calibration, qualification harness.

### Feature-sharing constraints

The MiniLM pipeline is a starting point, not a universal feature contract.

- Each head binds to an exact model, tokenizer, input construction, pooling, normalization, precision and runtime.
- A cached document vector cannot stand in for a cross-encoder's joint query-document input.
- Sharing is valid only where training and serving recipes agree; the connector checks recipe identity and returns `unavailable` on mismatch.

### Scheduling

Decision work gets its own bounded admission, cancellation, deadlines and resource accounting. Core keeps inert contracts and gains no scheduler. The plan runner's deadline checks do not terminate an already admitted synchronous read, so Arbiter deadlines are enforced in the adapter, before a read is admitted.

## Host controller

Removing agent reasoning turns requires the host to substitute a local decision for an LLM call; an MCP server cannot stop its client from asking the model what to do next. Arbiter therefore ships at two integration depths.

| Depth | What changes | Works with | Effect |
| --- | --- | --- | --- |
| C1 Server-side | Better proposals and smaller, more useful tool results | Any standard MCP client, unchanged agent loop | Less input on later agent calls; call count unchanged |
| C2 Host-side | A bounded controller replaces named orchestration turns | Cooperating host, agent adapter, or direct Workbench flow | Fewer generative invocations |

Adding a `decide` tool and asking the same agent to call it adds a round trip instead of removing one. C2 exists to avoid that.

&#91;image: Workflow: establish context, use current facts, assess the case, act with authority, with an exception path through evidence and review\]

Figure 5. Controller loop: establish context, use current facts, assess the case with a qualified model, act with host authority; exceptions return through evidence and review.

### Controller loop

1. **Establish context.** Host code verifies scope and permissions and validates exact facts. No model.
2. **Gather.** `airk.request_supported.v1`, then `airk.retrieval_route.v1` or `airk.recipe_choice.v1`. AIRK prepares and executes the plan.
3. **Select.** Build the packet with relevance and role scoring.
4. **Assess.** `airk.next_evidence_step.v1` returns expand, inspect, clarify or hand off. Exact gates check required categories and identities first.
5. **Act.** On hand-off, the host calls its generator with the packet. On anything else it loops to step 2 within budget.
6. **Exceptions.** `abstained`, `unsupported` or missing evidence routes to more retrieval, clarification, or the unmodified agent loop.

### Controller rules

- Code owns budgets, loop limits, retries, deadlines and termination. The model selects among permitted transitions only.
- Candidate tools and workflows must already exist; the controller cannot invent capabilities.
- Fall back before paid generative work on that branch where possible, and count every call when fallback happens late.
- State can change between read and action. The host checks freshness or a version precondition at execution and re-prepares stale plans through the normal AIRK path.
- Per-decision acceptance does not compound cleanly: four decisions at 90% each give 65.61% joint coverage under independence. Measure whole-trajectory coverage and success directly.
- A test-workflow recommendation is not dependency analysis. Required tests and CI policy stay in force.

### Replaceable agent patterns

| Pattern | Replaced by Arbiter | Stays with host or generator |
| --- | --- | --- |
| Router or supervisor | Choice among existing tools, recipes or workflows | Inventing new capabilities |
| Recurring planner | Registered workflow plus bounded semantic branches | New task decomposition |
| Evaluator or critic | Fixed rubric; support of supplied claims | Broad critique; discovering omitted claims |
| Tool loop | Interpreting qualified results, choosing the next permitted transition | Budgets, retries, termination |
| Multi-step handoff | Structured facts and accepted outcomes in shared state | Novel synthesis |

### Studio and Planner

Arbiter can suggest one of the Runnable strategies, pick evidence roles, flag missing prerequisites and choose an approved prompt profile. Deterministic code compiles the handoff from the task, original excerpts, constraints and citations. A suggested draft stays inert, and evidence from another publication is never mixed into a saved plan.

## Model candidates

The first GPU experiment compares three purpose-built decision models against the existing sparse and MiniLM baseline; the compact deployment candidate is d1-2B Q4\_K\_M through a native runner, subject to its license. Model facts come from publisher cards inspected for the source documents; no model has been downloaded, trained or benchmarked.

### Decision models

| Candidate | Characteristics | License | Role | Main constraint |
| --- | --- | --- | --- | --- |
| [LiquidAI/d1-2B](https://huggingface.co/LiquidAI/d1-2B) | 1.92B parameters, LFM3 backbone, typed decisions with no generated tokens, 128K advertised context | Custom LFM license | Leading compact native candidate; text Choice, Binary, Ordinal only | Published latency is on H100 and MI325X, not consumer GPUs |
| [Strands Decider 2B](https://huggingface.co/StrandsAgents/strands-decider-2B-qwen3.5-v1-2610) | Qwen3.5-2B-Base plus LoRA and a learned option readout; Choice, Noul, Score | Apache-2.0 | Reference implementation for training and shared-state decisions | Adapter is not the full download; Python reference runner, Linux or WSL2 for GPU |
| [Intern-Decision 0.8B](https://huggingface.co/internlm/Intern-Decision-0.8B) | Qwen3.5-derived; one forward pass scores answer symbols at decision positions | Apache-2.0 | Smallest decoder challenger | Exact question compiler, template, position selection and calibration must be preserved |
| [tasksource-decider-nano](https://huggingface.co/tasksource/tasksource-decider-nano) | About 150M, ModernBERT-style cross-encoder, 8,192-token pair limit | Apache-2.0 | Sub-gigabyte encoder alternative | Tree attention and option pooling need a faithful runtime |
| [kev-4b](https://huggingface.co/jaredpalmer/kev-4b) | Qwen3.5-4B-Base plus LoRA and pointer head | Apache-2.0 | T3 quality comparison | Published config targets 24 GB; a 16 GB quantized profile is extra work |

Excluded for now: [sysone 0.4B](https://huggingface.co/forward-metric/sysone-typed-decisions-0.4B) is trained on the benchmark's own training split, and [Decima-small](https://huggingface.co/amyrmahdy/decima-small) flags non-commercial training data despite Apache weight metadata.

### General small LLMs (baselines and local teachers)

| Family | Option | Use |
| --- | --- | --- |
| Qwen3.5 | [2B](https://huggingface.co/Qwen/Qwen3.5-2B) as the general-LLM scoring baseline; 4B and 9B optional | Apache-2.0; full checkpoints are 1.75 GB (0.8B) and 4.55 GB (2B) safetensors |
| LFM3 | [LFM3-1.7B](https://huggingface.co/LiquidAI/LFM3-1.7B), 1.10 GB Q4\_K\_M | Card warns that disabling thinking is inconsistent |
| MiniCPM5 | [MiniCPM5-2B](https://huggingface.co/openbmb/MiniCPM5-2B), 1.56 GB Q4\_K\_M | Text-only challenger; test prefill separately |
| Gemma 4 | E2B (5.1B including embeddings), E4B (8B total) | Larger storage than the names suggest |
| Granite 4.2, SmolLM3, Phi-4-mini | 3B to 3.8B | Controls with mature export support |

### Selection rules

- Do not pick a winner from unrelated benchmark rankings. Model-card calibration describes the authors' distributions, not AIRK traffic.
- The LFM license sets additional commercial terms at $10 million or more in annual revenue. If that does not fit distribution, use the Apache candidates and budget their integration work.
- Mixture-of-experts models are poor defaults: a small active-parameter count still stores every expert.
- A 128K or 256K context claim says nothing about speed or fit on a 16 GB device.
- T3 on 24 GB and larger GPUs opens kev-4b and Qwen3.5-4B or 9B at four bits (2 GB and 4.5 GB of ideal weight payload before overhead). Each is a separately qualified profile.

## Runner and runtime

The compact profile runs a pinned llama.cpp decision server on loopback, with the Arbiter adapter translating between AIRK's typed envelopes and the runner's protocol. One native execution implementation serves every client; Python owns training and reference evaluation only.

### llama.cpp decision path

- d1 GGUF decision support is documented for llama.cpp **b11960 / v0.8.0 and newer**, exposed by the server at `/v1/systemone`. Pin a tested commit; a newer version string does not guarantee the same implementation.
- Type mapping: `noul` to Binary, choice probabilities to registered label ids, score-level probabilities to ordinal ids and scale.
- Each compiled question with its state must fit both batch and microbatch. Set `-b` and `-ub` to cover the qualified profile including template tokens, and validate length before dispatch.
- The server's decision implementation uses llama.cpp's staging extension header ([server-decision.cpp](https://github.com/ggml-org/llama.cpp/blob/master/tools/server/server-decision.cpp), [llama-ext.h](https://github.com/ggml-org/llama.cpp/blob/master/src/llama-ext.h)). Embedding it in-process means owning a narrow versioned shim, porting the compiler and postprocessor, and running parity fixtures.
- The inspected shared-prefix eligibility does not include LFM3 d1. Do not assume several questions in one call share state; record actual forward-pass count per request.

### Runtime options

| Runtime | Use in Arbiter | Boundary |
| --- | --- | --- |
| ONNX Runtime | Existing encoder and head path (T1); candidate in-process path for exported decision models | Export must preserve masks, pooling, positions, readout and quantization behavior |
| llama.cpp / GGUF | T2 and T3 native runner | Verify architecture, decision protocol, backend and quantization together |
| PyTorch, Transformers, PEFT | Reference evaluation, training, prototyping | Not in the end-user install when a qualified native artifact exists |
| vLLM | Optional Linux service for measured concurrency needs | Generic generation API is insufficient for custom decision heads |
| Ollama | Optional developer convenience | Explicit local model, cloud features disabled; never a required dependency |

### d1 ONNX route

Use this if strict in-process Rust integration matters more than the smallest download.

- The export provides logits and final hidden states; the documented row construction and typed postprocessing must be reproduced natively.
- At most 16 questions per packed row.
- On CUDA use the `fp16`, `q4f16` or `q8f16` graphs. Plain `q4` and `q8` use FP32 activations and push sliding-attention layers to CPU.
- The GPU `q4f16` package is about 1.95 GB including vision, a different artifact from the 1.10 GB GGUF text backbone.

### Score semantics

- Keep each model's trained scoring computation. A pointer head, a token-position classifier and a next-token decoder are different computations; swapping one for another invalidates calibration.
- General-LLM baseline: map choices to canonical single-token symbols in the exact template, read raw logits before grammar filtering or sampling, normalize over allowed candidates. Multi-token candidates need a specified sequence-likelihood computation.
- The resulting softmax is a relative distribution over candidates. It does not show that the top option is correct or that any option applies.
- Test candidate-order and label-wording sensitivity. Fit calibration on held-out examples scored by the shipped artifact.
- Answers are produced by code over validated scores. The model is never asked to print a confidence number.

### Platform matrix

NVIDIA CUDA, AMD Vulkan or ROCm, and integrated or shared-memory systems each need separate qualification. The Strands reference runner's GPU route is Linux or WSL2; a Rust client talking to that Python process is an experiment harness, not a native implementation.

## Memory and context budgets

The 16 GB profile caps the Arbiter process at 12 GiB of measured GPU allocation and 2,048 model-input tokens by default. Both are planning ceilings to tune, not measurements.

&#91;image: Budgets: 1.10 GB model download, installed disk with rollback, and a 12 GiB GPU process ceiling split 5, 4, 2 and 1 GiB\]

Figure 6. Budgets: a 1.10 GB download is one budget; serving memory, disk and training memory are separate.

### Four budgets

1. **Download**: the exact weight, tokenizer, runtime and accelerator files that travel.
2. **Disk**: active assets, cached originals, updates and one retained rollback version.
3. **Inference memory**: GPU weights, activations, cache or state, workspaces, concurrent requests. Host RAM is separate.
4. **Training memory**: weights plus gradients, optimizer state, saved activations and buffers.

### 16 GB serving allocation

| Reservation | Ceiling |
| --- | --- |
| Model and auxiliary weights | 5 GiB |
| Activations and workspaces | 4 GiB |
| Bounded cache or recurrent state | 2 GiB |
| Allocator slack | 1 GiB |
| **Process total** | **12 GiB** |
| Left for display, other applications, variability | Remainder of the device |

Weight arithmetic starts at `parameters x bits_per_weight / 8`, which excludes quantization scales, mixed-precision tensors, embeddings outside the nominal count and runtime memory. For a conventional attention decoder:

```
KV bytes ~= 2 x attention_layers x cached_tokens x KV_heads
            x head_dimension x bytes_per_element x concurrent_sequences
```

Sliding attention and hybrid recurrent architectures change this; add their state and use the runtime's actual allocation. A decision model may keep no generation cache but still needs prefill activations.

### Asset sizes

| Artifact | Size | Note |
| --- | --- | --- |
| MiniLM INT8 profile | About 23 MB of weights | Existing AIRK baseline; runtime and tokenizer are additional |
| 150M encoder | About 150 MB INT8, 300 MB BF16 | Arithmetic estimate before overhead |
| d1-2B GGUF Q4\_K\_M | 1.10 GB text backbone | Optional visual projector, 203 MB Q8 or 379 MB BF16, omitted for text-only |
| LFM3-1.7B / MiniCPM5-2B GGUF Q4\_K\_M | 1.10 GB / 1.56 GB | General-LLM comparison sizes |

Publisher sizes are decimal GB; GPU tools report MiB or GiB. Record units explicitly.

### Serving rules

- One active GPU request and a small finite queue to start. Raise concurrency only after measuring peak allocation and tail latency.
- No CPU offload as a default. Prefer a smaller fully resident model that meets quality targets.
- Token profiles: 2,048 default, 4,096 as an explicitly qualified extended profile.

### Context assembly

&#91;image: Context assembly: authorized sources, local retrieval, evidence selection, a prioritized context budget, the qualified model, and a missing-evidence branch\]

Figure 7. Context assembly: authorized, versioned evidence is combined with the question, rubric and required facts inside a qualified token budget; missing evidence leads to more retrieval or abstention.

Budget order, highest priority first:

1. Question, options and rubric.
2. Indispensable structured facts.
3. Required evidence and source ids.
4. Optional examples if space remains.

- Every token counts against the profile: templates, options, state and evidence.
- Never silently cut an exception that changes the answer. If required evidence cannot fit, use a separately evaluated decomposition or abstain.
- A model-written summary is another uncertain transformation and needs its own evaluation.
- Shared-state processing applies only where the backend supports it. Cache keys include the exact compiled prefix, model and adapter identity, tokenizer, precision, scope and evidence version.
- Question order and attention masks must not let one question change another's answer.

## Provisioning and activation

Assets are fetched once by an explicit action, verified, qualified on the target hardware and activated as a warmed snapshot; nothing downloads during a request.

&#91;image: Provisioning lifecycle: select exact assets, verify and store, qualify and warm, then offline inference with typed outcomes\]

Figure 8. Provisioning lifecycle: select exact assets, verify and store, qualify and warm, then serve offline with typed outcomes.

### Pack manifest

Each Arbiter pack manifest lists, per file: content hash, compressed transfer bytes, installed bytes, license, and role (weights, tokenizer, template, runner, accelerator runtime, fixtures). It also declares:

- Supported AIRK contract versions.
- Hardware profile: backend, minimum free VRAM, token profile, `-b` and `-ub` values.
- Qualification fixtures with reference scores and tolerances.
- Peak disk use during update with rollback retained.

### Lifecycle

1. **Select** exact files from the manifest. Never clone a repository holding every precision.
2. **Fetch** through the installer or an internal artifact mirror into the machine-wide content-addressed store.
3. **Verify** hashes and licenses.
4. **Qualify** by running fixtures on this GPU, driver and backend.
5. **Warm** the model and report readiness only after first inference completes.
6. **Activate** the snapshot and write an activation receipt (pack id, probe result, fixture results, timings).

### Update and rollback

- One base model serves many tasks. A new head or calibration table does not re-download the transformer.
- Keep one rollback version on disk and budget for it explicitly.
- On a single GPU, old and new snapshots usually cannot coexist in VRAM. Drain requests, swap weights, report `unavailable` for GPU tasks during the swap, and keep T0 and T1 tasks serving.
- Activation is atomic per snapshot: a failed warmup or fixture mismatch reverts to the previous snapshot.

### Request-time rules

- Missing model or failed warmup returns `unavailable`.
- No implicit download, no background model update, no runner telemetry, no cloud fallback.
- Cold start is reported separately from warm latency: page faults, weight load, kernel initialization, first inference.

## Latency targets and admission control

Warm, batch-one targets run from 2 ms p50 on the cached or sparse route to 750 ms p95 on the extended GPU profile. They include the adapter, exclude upstream ingestion, and are engineering goals rather than measurements.

&#91;image: Latency targets: p50 and p95 for cached or sparse, encoder plus heads, compact GPU and extended GPU routes on a log scale\]

Figure 9. Warm, batch-one p50 and p95 targets for the four routes, on a logarithmic scale.

| Route | Tier | p50 / p95 target | Test input |
| --- | --- | --- | --- |
| Exact cached result or sparse head | T0 | 2 / 10 ms | Bounded short state; exact cache identity or registered lexical recipe |
| Small encoder plus trained heads | T1 | 20 / 75 ms | Up to the encoder's qualified token limit; compatible heads share features |
| Compact GPU decision model | T2 | 100 / 300 ms | At most 1,024 total model tokens, one active request, up to four small questions |
| Extended GPU profile | T3 | 250 / 750 ms | At most 4,096 total model tokens, same question limits |

### Decomposition

```
T_local = admission + queue + validation + retrieval + tokenization
          + state/question forward pass + calibration/policy + transport

T_generation = T_local + generated_tokens / measured_decode_rate
```

Removing generation removes the sequential output term, not the cost of reading input. Measure prefill latency, not advertised tokens per second. Transport is local IPC or loopback.

### Tail behavior

A cascade's mean can look good while its tail is poor. If 90% of requests finish in 20 ms and 10% take an additional 200 ms, the mean is 40 ms and p95 sits in the slow route. Report route frequencies with p50, p95 and p99.

The same holds for the controller. With five 700 ms generative calls and 300 ms of other work, baseline is 3,800 ms. Replacing four calls with 100 ms decisions gives 1,400 ms. If 10% of cases run the four local decisions and then fall back to all four original calls, those cases take 4,200 ms and the mean is 1,680 ms, with the slow cases worse than baseline. These are illustrative figures.

### Admission

- Finite queue, per-request deadline, cancellation and explicit resource ownership in the adapter.
- Validate token length against the profile before dispatch.
- A saturated GPU returns `unavailable` or a qualified smaller route per policy.
- AIRK lexical reads, status and indexing are never queued behind Arbiter work.
- GPU serving pauses during local training unless isolation has been measured.
- Never run Arbiter where a fast deterministic path already succeeds.

## Training and calibration pipeline

Training runs on the same class of machine as serving and always ends in a new immutable bundle; live weights never change during a decision.

&#91;image: Training lifecycle: define the task, fit a baseline, improve if needed, compress and export, calibrate, test and activate\]

Figure 10. Training lifecycle: define the task, fit a baseline, improve only if needed, compress and export, calibrate, then test and activate.

| Stage | Method | Output |
| --- | --- | --- |
| 1. Define | Task card, rubric, missing-evidence rules, grouped splits | Versioned task definition and datasets |
| 2. Cheap baseline | Sparse linear model; frozen MiniLM features with logistic or linear heads | T0 and T1 heads; no GPU needed |
| 3. Flexible baseline | Existing decision model with versioned questions and context | Coverage delta against stage 2 |
| 4. Specialize | Fine-tune a small encoder, or LoRA / QLoRA on a 0.8B to 2B model | Adapter or head |
| 5. Compress | Distill adjudicated decisions into the smallest model that passes the task gate | Smaller specialist, ideally off the GPU |
| 6. Export and qualify | Quantize, calibrate deployment scores, select policy, run untouched tests | Immutable bundle |

### Four meanings of "learning"

| Mechanism | What changes | Use |
| --- | --- | --- |
| Retrieval | Evidence supplied to the next request | Current source facts and policy text |
| In-context examples | Bounded examples in model input | Rapid rubric trials; costs prefill and is not weight training |
| Head training | Small task parameters over a frozen representation | Frequent fixed-label decisions |
| Adapter training or distillation | Model behavior, offline | Hard boundaries; compressing a flexible model into a specialist |

### Data

- AIRK's parser corpora and replay records are usable with their provenance and splits respected. The manifest's 48,000 fitted rows come from a larger pool and are not 48,000 independent request families.
- Each task gets its own adjudicated data. Intent labels do not establish relevance labels; relevance labels do not establish sufficiency.
- Hard cases to include: same-name symbols in different modules, misleading tests, outdated documents, partial extraction, conflicting policies, negatives sharing the query's vocabulary, negation, instructions inside evidence.
- Group splits by repository, request family, symbol family and paraphrase origin. Keep train, development, calibration, policy-selection and final test partitions distinct. Hold out whole repositories and question families.
- Planning range: 50 to 200 adjudicated examples per label for feasibility, growing to hundreds or thousands where errors demand it.
- A local teacher may propose labels or soft targets; human adjudication is the authority, and teacher and student run sequentially on the GPU.

### Training on 16 GB

- LoRA cuts trainable parameters and QLoRA trains through a quantized frozen base; neither removes activation memory.
- A GGUF inference artifact is not a training checkpoint.
- Start at microbatch 1, 512 to 1,024 tokens, gradient accumulation and checkpointing. Measure peak memory before raising either.
- No full fine-tuning of a multi-billion-parameter model on this GPU. Fitting 2B inference does not show that an upstream multi-GPU recipe fits.

### Calibration

- After any adapter, quantization, compiler or backend change, rescore the held-out calibration partition on the shipped artifact.
- Report Brier score, log loss, reliability curves and accepted precision against coverage, per task and slice.
- A shipped temperature from a public model is a baseline only.
- Evaluate the cascade as a whole. Escalated cases are a harder, selected population; version the routing policy with the component models.
- The current `intent-broader/5` record is the reference point: 99.740% accepted precision at 52.125% abstention on its calibration set, `qualified: false`, supported ask-rate gate failing. The target is more accepted coverage on the fallback population at the same precision, with complete operation, target, scope and argument correctness.

## Instrumentation and experiment arms

Arbiter adds seven observations in a separate bounded aggregate and a separate experiment artifact; AIRK's content-free usage ledger and its `paired_task` schema keep their current meaning.

| Measurement | Owner | Purpose |
| --- | --- | --- |
| Task id, version and bounded route category | Arbiter aggregate | Where the integration actually ran |
| Counts of `accepted`, `abstained`, `unsupported`, `invalid_input`, `unavailable` | Arbiter aggregate | Separate model quality, scope rejection and operational failure |
| Queue and inference time | Arbiter aggregate | Local overhead and saturation |
| Selected and omitted record counts, token and byte totals | Packet accounting | Packet reduction without treating omissions as absence |
| Native reads and paid LLM calls | Host | Distinguish a smaller result from an eliminated turn |
| Fresh and cached input, output, retries, fallback | Host or provider counters | Final accounting under an explicit contract |
| Task outcome, review, rework | Independent harness or adjudication | Whether quality held |

The ledger does not acquire query hashes, source identities, raw prompts, explanation text or unbounded labels. Detailed provenance stays in scoped responses and qualification artifacts. Missing observations stay unknown.

### Experiment arms

| Arm | Configuration |
| --- | --- |
| A | Ordinary agent with competent conventional tools |
| B | Current AIRK configured well: terse and compact modes, explicit recipes |
| C1 | AIRK plus Arbiter evidence selection, agent loop and generator unchanged |
| C2 | AIRK plus Arbiter decisions and the host controller, named turns replaced |
| D (later) | Generator routing via `agent.generation_route.v1`, only after same-generator results are understood |

`paired_task` represents `with_airk` versus `without_airk` on one model and is not reused for B versus C. Arms A through C2 go in a new artifact; if product reporting later needs them, add a versioned schema.

### Method

- Frozen task set across navigation, documentation lookup, bug investigation, API change, patch review and implementation.
- Independent sessions, randomized or alternating order, repeated runs, cache conditions disclosed.
- Pin source, model, task bundle, runtime, tool instructions and acceptance checks.
- Identical final quality criteria; failures and incomplete tasks stay in the primary report.
- Ablations separate better prompts, native recipe use, deterministic grouping, learned selection and the controller. If B never uses recipes and C does, the gain is partly adoption.
- Selection can reorder a prompt prefix and reduce provider cache reuse. Track fresh and cached input separately.

Reference fixtures for arm B: 47,767 Full versus 30,508 Terse serialized MCP bytes across nine tools, and 8,477 rich versus 1,826 minimal logical-result bytes on a twelve-definition fixture. These are fixture comparisons, not provider-token measurements.

## Qualification matrix

A bundle or pack ships only when every row below has recorded evidence on the target hardware profile.

| Dimension | Required evidence |
| --- | --- |
| Compatibility | With Arbiter disabled, absent and failed: existing AIRK test suite passes unchanged; existing indexes open without migration; MCP catalog and response fixtures are byte-identical |
| Offline operation | Install and warm from local assets, then startup, retrieval, inference, evaluation and a representative training run with external network blocked |
| No hidden request path | Egress attempts inspected during normal operation, errors, saturation and missing-model cases; every dependency failure returns a typed status |
| Scope isolation | Cross-project and cross-user negative tests on evidence, caches, bundles and results |
| Decision quality | Per-task accepted precision, coverage, error costs, slice results and uncertainty intervals |
| Applicability | Unsupported-input acceptance, no-valid-option cases, language limits, missing and contradictory evidence |
| Flexibility | Held-out question families, new label descriptions, candidate order and count changes, paraphrases |
| Robustness | Negation, distractors, instruction-bearing evidence, long-context boundaries, source conflicts, harmful selections among valid labels |
| Calibration | Reliability, Brier and log loss, post-quantization threshold behavior, escalated cases |
| Evidence contract | Stale publication rejected or re-prepared; same-name candidates keep disambiguation; exhaustive lookups unfiltered; contradictions retained; omissions reported |
| Reuse | Unrelated edits, relevant edits, removed files, expired publications, representation changes, new sessions, lost agent context |
| Runtime | Warm and cold p50, p95, p99; throughput; queue delay; tokenizer time; cache hit and miss; sustained load; impact on AIRK reads and indexing |
| Resources | Exact GPU, driver and backend; peak allocated and reserved VRAM; host RAM; transfer bytes; installed disk |
| Lifecycle | Missing assets, interrupted provisioning, offline startup, failed warmup, deadline and OOM handling, activation, rollback |
| Portability | Reference-to-native agreement on tokens, masks, raw scores, typed probabilities and policy outcomes, with tolerances declared in advance |
| End-to-end | Arms A, B, C1, C2: task success, critical-evidence recall, follow-up reads, paid calls suppressed, retained, retried and fallen back |

### Evidence-contract cases

| Condition | Required behavior |
| --- | --- |
| Current publication differs from a prepared plan | Reject or prepare again under the native contract |
| Source changed after indexing | Preserve indexed identity; verify current saved files before edits |
| Several same-name candidates | Preserve disambiguation; a confident category does not select an entity |
| Required evidence omitted or unsupported | Report, retrieve through a permitted path, or abstain |
| Retrieved text contains instructions | Treat as evidence only |
| Decision model unavailable | Declared fallback or `unavailable`; no external call |
| Proposed test set is narrow | Required test and CI policy preserved |

### Sample size

Zero observed failures in 300 independent accepted cases gives roughly a 1% upper error bound at 95% confidence (rule of three). It does not establish 99.9% reliability, and correlated cases or thin per-class samples weaken it further. Size each accepted test set to the error claim being made.

## Delivery phases and gates

Eight phases, each gated on measured results from the one before; GPU work starts only after CPU decisions and deterministic rules are shown to leave work unresolved.

| Phase | Deliverable | Gate to proceed |
| --- | --- | --- |
| 0. Baseline and contracts | Frozen AIRK configuration, representative task traces, quality rubric; inert `airk.arbiter.advice/1` and `airk.arbiter.packet/1` types; capability probe; compatibility test suite | Specific decisions or evidence failures identified that existing rules and recipes do not solve; compatibility suite green |
| 1. CPU proposal assistance (T1) | Adapter for `airk.request_supported.v1` and `airk.retrieval_route.v1` in shadow or review-only mode, wired at `parse_text` after grammar | More useful coverage at the agreed precision; explicit commands and review policy unchanged |
| 2. Evidence packets (C1) | Packet projection with native-result preservation, budget accounting, expansion paths; deterministic selector first, learned selector second | Better end-task result than current AIRK plus deterministic selection, with critical evidence retained |
| 3. Reference model comparison | Local harness scoring the baseline against d1-2B, Strands Decider 2B and Intern-Decision 0.8B in reference precision, then quantized | At least one flexible model adds useful accepted coverage at acceptable error |
| 4. Arbiter runner (T2) | `airk-arbiter` sidecar with pinned native runner, GPU probe, pack manifest, provisioning, activation receipts, rollback | License fits distribution; full decision path works offline; no implicit download; no Python in the native profile; coverage gain exceeds latency and operational cost |
| 5. Host controller (C2) | Bounded controller replacing named orchestration calls, honoring AIRK plan validation | Paid calls actually reduced, fallback accounted for, task success equal or better |
| 6. Local specialization | First head or adapter training run, quantized artifact, calibration, untouched test report | Improvement survives export and does not regress important slices |
| 7. Packaging and T3 | Bundled opt-in setting or separate installer; versioned bundles; bounded reporting; platform-specific qualification; extended and 24 GB+ profiles; optional generation routing | Reproducible release evidence per platform; full qualification matrix recorded |

### First demonstrations

Each shows original AIRK behavior, the added decision, the resulting evidence and observed task cost.

1. A hard retrieval paraphrase that the current parser abstains on: "find the implementation behind the retry policy and the tests that document the limit".
2. Evidence preparation for a small API change: definition, relevant occurrences, controlling design note, test examples, chosen through `airk.recipe_choice.v1` inside plan limits.
3. A smaller packet that still holds the controlling contract and its tests.

### Standing constraints

- Reviewed-proposal mode ships before any automatic execution of learned plans.
- A better classifier score does not enable automatic execution.
- If the deterministic selector matches the learned one, ship the deterministic one and stop.

## Appendix: serving-volume reference and open questions

### Serving-volume reference

The source chart is retained for capacity planning: it shows the request volume at which a resident local runner undercuts a hosted decision API. It uses the source's illustrative inputs, not a hardware quote.

&#91;image: Monthly cost against requests per month for a hosted decision API and local runners with $100 and $300 fixed cost\]

Figure 11. Monthly cost against request volume at 2,000 billable input tokens per request; local fixed costs of $100 and $300 cross the hosted API line near 1.22 and 3.66 million requests.

```
C_local/month = fixed hardware allocation + idle energy + operations
                + training/labeling allocation + N x marginal serving cost

break_even_requests = fixed_monthly_cost / (API_cost_per_request - local_marginal_cost)
```

The denominator must be positive, and the runner must have measured capacity for that volume inside the latency targets. Four million decisions a month averages about 1.54 per second, which says nothing about peak load.

### Open questions

- [ ] Does the LFM license fit AIRK's distribution, or does the compact profile default to an Apache candidate?
- [ ] Loopback sidecar only, or also an in-process path through a native shim or the d1 ONNX export?
- [ ] Does the feature connector live in `airk-embed`, or does `airk-arbiter` load its own encoder session?
- [ ] Minimum qualifying GPU: is 12 GB with the compact model worth a `qualified_reduced` profile, or is 16 GB the floor?
- [ ] Which backends qualify first: CUDA only, or CUDA plus Vulkan?
- [ ] Windows native support for the runner and for training, versus WSL2 for training only.
- [ ] Which host gets the first C2 controller: the Workbench flow, a VS Code agent adapter, or a standalone MCP-side orchestrator?
- [ ] Does AIRK Server get a shared Arbiter, and if so what is the multi-tenant authorization model? Same-user ownership today does not cover it.
- [ ] Final call on packaging: bundled opt-in (recommended here) or separate install.

### Source documents

- *Extending AIRK with Decision Kernel* (integration architecture, tasks, packet, seams, phases)
- *Decision Kernel, The Genesis of an Idea* (models, runtime, memory, provisioning, training, latency)
- *Decision Kernel, as Alternative to AI* (answer primitives, controller patterns, workflow figure)
- AIRK source and docs at [iokaio/airk b624b31](https://github.com/iokaio/airk/tree/b624b31c32c140d66c80f8daf3c62175e4719fcb): [architecture](https://github.com/iokaio/airk/blob/b624b31c32c140d66c80f8daf3c62175e4719fcb/docs/architecture.md), [process model](https://github.com/iokaio/airk/blob/b624b31c32c140d66c80f8daf3c62175e4719fcb/docs/process-model.md), [shared embedding service](https://github.com/iokaio/airk/blob/b624b31c32c140d66c80f8daf3c62175e4719fcb/docs/service-embedding.md), [Workbench implementation](https://github.com/iokaio/airk/blob/b624b31c32c140d66c80f8daf3c62175e4719fcb/docs/workbench/implementation.md), [MCP developer guide](https://github.com/iokaio/airk/blob/b624b31c32c140d66c80f8daf3c62175e4719fcb/docs/mcp-developer-guide.md), [identity contract](https://github.com/iokaio/airk/blob/b624b31c32c140d66c80f8daf3c62175e4719fcb/docs/v2-identity-contract.md), [usage reporting](https://github.com/iokaio/airk/blob/b624b31c32c140d66c80f8daf3c62175e4719fcb/docs/usage-reporting.md), [ONNX models](https://github.com/iokaio/airk/blob/b624b31c32c140d66c80f8daf3c62175e4719fcb/docs/onnx-models.md)
- Runtime references: [d1-2B GGUF](https://huggingface.co/LiquidAI/d1-2B-GGUF), [d1-2B ONNX](https://huggingface.co/LiquidAI/d1-2B-ONNX), [llama.cpp server](https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md), [LFM license](https://huggingface.co/LiquidAI/LFM3-1.7B/blob/main/LICENSE)


## Training Data

There's no single open dataset that covers what the Arbiter encoder needs, but some of it is covered. Open data exists for **evidence relevance**: does this code or passage answer this request or issue? Nothing exists for the AIRK-specific tasks: request applicability, route and recipe choice, and the next evidence step. Those labels depend on AIRK's own operations, so you'd have to generate them.

**Usable for relevance and role training:**

| Dataset | What it gives you | License | Fit and caveats |
|---|---|---|---|
| [CoRNStack](https://arxiv.org/abs/2412.01007) (Nomic) | Docstring-to-function triplets with mined hard negatives, 6 languages; the [Java split](https://huggingface.co/datasets/nomic-ai/cornstack-java-v1) alone is about 22.9M rows | Apache-2.0 | Best large-scale pretraining for `evidence_relevance`. Its queries are docstrings, which are cleaner than real developer requests. The underlying repos carry their own licenses. |
| SweLoc ([SweRank](https://arxiv.org/html/2505.07849v1)) | GitHub issue text paired with the Python functions its fix changed. Unchanged functions serve as negatives. Built from 3,387 repos, with SWE-bench repos excluded. | Not confirmed; check the release page | The closest public analog to "which evidence matters for this task." Python only. |
| SWE-bench family | Issue, gold patch and tests; the touched files become positive labels | Check each variant | Keep it for evaluation only. Training on it contaminates the benchmark everyone compares against. |
| [CoIR](https://arxiv.org/abs/2407.02883) | A 10-dataset code retrieval benchmark, including CodeSearchNet, CoSQA and StackOverflow QA | Mixed by subset | Use it for evaluation, not training. |
| Stack Exchange dumps | Question, accepted answer and code | CC BY-SA | Natural developer phrasing. ShareAlike terms apply to redistributed derived data. |

**The gap:** nothing public labels "is this a supported read request," "definition vs. occurrences vs. hybrid search," or "expand, clarify or hand off." Those labels are defined by AIRK's operations, so no outside dataset can contain them.

**Suggested approach:**

1. **Pretrain relevance on CoRNStack**, then fine-tune on issue-to-code data, so the model sees both docstring-style and real-request phrasing.
2. **Build your own SweLoc-style set** from permissively licensed repos, using AIRK's own indexer:
   - Mine merged PRs linked to issues.
   - Take the functions, tests and docs each PR touched as positives.
   - Take the unchanged neighbors as hard negatives.

   The records come out with AIRK's representation identities and evidence roles. That also sidesteps the licensing uncertainty, since you choose the repos, and it extends past Python.
3. **Generate the routing and applicability data** from AIRK's existing template and identifier-harvesting pipeline, plus shadow-mode traces from real use. Have the labels adjudicated, not just teacher-generated.
4. **Evaluate on held-out repos**, plus CoIR and a SWE-bench slice you never train on.

One caution for commercial distribution: even an Apache-licensed dataset wraps code under many licenses. Keep the per-repo license metadata with every training row, so you can later filter to MIT/Apache/BSD sources only.

I can add this as a "Training data sources" subsection under the doc's training pipeline section if you want it there.