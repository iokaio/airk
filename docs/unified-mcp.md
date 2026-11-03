# One AIRK MCP connection

**1.8.8 P4 development:** [Saved-source verification](source-verification.md) defaults to Report for evidence reads, with bounded native checks and explicit Off/Require choices. CLI, MCP, Workbench and Inspector/Checker retain dated outcomes and uncertainty. Phase acceptance and performance qualification remain in the [ledger](research/implementation-v190.md).

The 1.8.8 working build supplies optional [evidence guidance](evidence-guidance.md)
from returned metadata. Advice is bounded and inert; CLI/MCP/Workbench and
complete/selected exports retain applicable reasons without additional reads.



For detailed per-tool developer guidance, see the [MCP developer guide](mcp-developer-guide.md) and its companion [CLI developer guide](cli-developer-guide.md). Native terminal tools cover 18 operations; projects and nine Assistance operations have separate CLI commands and different registry/proposal scope where documented.



The AIRK 1.8.8 manual-test working build retains one server named `airk`, with 28 unique, unprefixed tools (the preceding 1.6.4 catalog has 27). Evidence tools answer from the configured index. `parse` prepares or executes a bounded read-only evidence plan without an editor. Help explains AIRK. Draft tools prepare suggestions that wait for the user in Explorer.

P1 repairs compact discovery bindings inside `parse` and advertises its nested
plan/recipe/budget/output inputs as JSON objects. See the [shared binding and
parity contract](read-interface-parity.md#regression-evidence). Existing direct
read defaults and compact producer replies are unchanged; source verification
and Project/index management remain later planned phases.

Native read tools accept optional `reply_detail: lean | rich` independently of
retention and response mode. The public help route validates the input while
keeping documentation's existing payload. The [reply guide](reply-detail.md)
records defaults, legacy conflicts and per-step plan selection.



Tool names are unique within this server; the client may add its own server namespace. Repeating `airk_` on every tool adds no distinction between reading evidence and proposing a form. Verbs and descriptions carry that distinction. Previously saved `airk_*` and `airk_explorer_*` calls remain callable compatibility aliases; they are omitted from discovery so agents see one catalog. Native `airk tools` exposes eighteen read-only aliases including `airk_parse`; Explorer retains its seventeen direct evidence operations.



| Tool | Description |

|---|---|

| `projects` | List only the bindings configured on this connection and its default. Does not enumerate or register every machine project. |

| `parse` | Prepare (default) or execute text, a named recipe or explicit bounded steps/bindings. Returns exact publication, per-step outcomes, citations and completeness; never writes sources or runs an agent. See [Workbench contracts](workbench/implementation.md). |

| `status` | Quick published counts, exact publication, freshness and semantic readiness. No editor or runtime/job probes. |

| `diagnostics` | Detailed coverage, runtime/process identities, job progress, queues, watcher/waits and optional editor attachment. May take longer; does not index or repair. |

| `capabilities` | Describe extraction adapters and supported evidence capabilities. |

| `search` | Search indexed code/documents and return bounded, cited lexical, vector or hybrid evidence. |

| `read_file` | Read an indexed line range with its revision and source identity. |

| `list_files` | List indexed paths, with bounded continuation. |

| `file_info` | Inspect indexed file metadata, extraction basis and hash. |

| `outline` | Read one indexed file's declaration or document outline. |

| `symbols` | Find native definition names and their indexed locations. |

| `imports` | Find selected import/use/include declarations. |

| `code_facts` | Retrieve supported syntax or producer-backed code facts and coverage. |

| `document_facts` | Retrieve structured document facts with original locations and extraction qualifiers. |

| `references` | Find exact-case textual identifier occurrences; does not claim resolved references. |

| `relationships` | Retrieve supported structural candidates or producer assertions with their basis. |

| `context` | Read bounded neighboring indexed lines. |

| `changes` | Validate remembered evidence or page through observed indexed changes. |

| `help` | Read native topics/CLI help/tool schemas, search bundled guidance with `query`, or read a guide with `topic_id`. Returns documentation. |

| `draft_search` | Propose an Explorer Search form; returns a receipt and never runs the query. |

| `draft_strategy` | Propose a Studio strategy using supported templates/operators; never executes it. |

| `draft_plan` | Propose Planner tasks and instructions; never saves the plan. |

| `suggest_view` | Propose Navigation, Review or Management guidance using `kind` and a typed `draft`; never runs a management action or test. |

| `list_templates` | Discover the installed strategy pack, with bounded paging and availability. |

| `get_template` | Read one installed template's parameters, defaults and contracts. |

| `validate_draft` | Validate a proposal without admitting it to the Inbox. |

| `proposal_status` | Read a proposal's review state; applied does not mean executed. |

| `get_handoff` | Read only the complete snapshot explicitly shared by the user through an expiring token. |



## Identifier routing and bounded admission



For a named identifier, prefer compact `symbols`, scoped `references`, then a bounded `read_file`/`context` expansion. Use `search` for concepts and supplementary code/document passages. `outline` returns indexed file structure in source order; `suggest_view` returns only a reviewed UI proposal, even when its route is named `code/outline`. Exact tokens, citations and original locations remain usable outside the connection. Terse status receipts suppress repeated observations without replacing evidence identity.



The bridge admits the configured concurrent evidence requests (default 16, choices 4, 8, 16, 32 and 64) and one additional reserved `status`/`airk_status` lane. Evidence, native-channel and worker waiting stages each have a finite 128-waiter bound and two-second admission deadline. Frontends lease independent authenticated evidence channels instead of serializing one stream; the worker receives the same configured allowance plus status/control headroom. Additional status observations can borrow idle evidence capacity, but never queue behind evidence before their existing cold/observation budgets. Capacity is a maximum, not a promise that ownership, publication or resource contention cannot refuse a request. Cancellation, authorization expiry and Stop/drain retain their fences; submitted or uncertain work is never blindly replayed.



The [MCP developer guide](mcp-developer-guide.md) documents optional indexed-language/evidence-kind filters, explicit name matching, qualified count precision, reference paging and bounded associated-method targets. These are shared native contracts; the [CLI developer guide](cli-developer-guide.md) maps their terminal flags. Workbench's Tool forms expose the corresponding controls without requiring raw JSON, and structured results show summaries/pointers with explicit follow-up actions.



## Scope and availability



From 1.5.9, the stdio lifecycle accepts a legacy `initialize` fallback after a discovery probe without retaining a contradictory modern-only metadata requirement. Each connection negotiates independently: Codex, Claude Code, Copilot and other clients can address the same saved project with different supported handshake styles. Client names are diagnostic labels, not routing or compatibility switches. Modern-only requests retain their metadata checks; catalog discovery does not wait for the service, index or editor adapter. See [the 1.5.9 evidence record](releases/release-1.5.9.md). Native protocol fixtures are distinct from qualification inside each assistant application.



New project-local connections retain a canonical source folder as their scope: `airk-launcher --root <source> mcp`. They do not persist a data path or internal project ID. Each request resolves that source's active registration, so rebuilding or restoring the index does not require regenerating the connection. Multiple active indexes for one source require an explicit `--index-collection`. Missing, removed and ambiguous registrations remain distinct; tools/help still discover normally. Additional source folders use repeated `--additional-root` with `--no-default-project`. Select a configured scope from `projects`; it is a stable connection selector, not the internal native binding. Explorer selection never redirects retrieval. Legacy explicitly pinned launches remain supported, and Explorer's internal evidence connection retains its registration and revision fences.



The native frontend owns stdio and direct evidence reads. The CLI composes an optional verified extension adapter for offline UI help, templates and window IPC. Kernel/store never depend on the editor. A missing, closed or invalid editor adapter cannot remove native tools or prevent evidence retrieval. Native help, catalog discovery and configured-project listing do not elect the service; the first native observation/query checks the selected runtime and service. Explicit Stop is respected. UI help requires the configured verified extension bundle, but no open window; UI drafts require a matching trusted window.



`status` keeps compact publication/readiness fields and adds the configured `project` scope. In the 1.8.0 F06a candidate, it has a ten-second cold-setup allowance followed by a two-second observation allowance. Setup keeps the original shared attempt start time. Expiration reports readiness as unknown, null evidence, and `index_error.stage` identifying setup or observation. Hidden `airk_status` follows the same dispatch, schema and deadline contract. Genuine preparation/verification failures remain errors for both names: the internal/native chain is preserved, while the MCP bridge returns its existing generic `isError` response. Only deadline expiry is reported as an unknown setup-timeout observation. `diagnostics` adds detailed native observations and `explorer`. Neither response mode nor include_status makes status run diagnostics. Each component retains its own availability and observation age. A successful status envelope can report unavailable evidence; callers must require `index_state: available`, the expected collection and a publication token before treating results as indexed evidence. Source publication and semantic coverage remain separate observations. Native usage accounting stays at the evidence boundary; optional editor calls and the added UI portion of composite status are not measured as native evidence bytes.



## Startup migration



After verified runtime selection and native registration, trusted startup and **Install or Repair CLI** consolidate recognized existing project configurations in one guarded save per file:



Version 1.5.11 checks saved editor bindings against the current native registry before generating these updates. An unambiguous replacement registration for the same source and collection repairs the editor reference; unavailable or ambiguous metadata cannot authorize a configuration rewrite. Explicit setup still revalidates immediately before writing. Already matching files are retained without repeated update messages.



- `.codex/config.toml`

- `.mcp.json`

- `.vscode/mcp.json`

- `.gemini/settings.json`

- `.agents/mcp_config.json`

- `.cursor/mcp.json`

- `.roo/mcp.json`

- `.junie/mcp/mcp.json`



For the current project, the old `airk-<collection>` and extension-owned `airk_explorer` definitions are removed and replaced with `airk`. Launchers, canonical roots, collection/binding and the extension descriptor must match this installation; old commands are never executed. New setup snippets also contain one entry. The retired VS Code Assistance provider withdraws its definitions instead of advertising a second server.



Migration preserves unrelated servers/settings, JSON comments, supported TOML comments, environment values and disabled state. If either old server was disabled, the unified connection remains disabled. Supported tool allow/deny/preapproval lists are translated; partially approved combined view tools require fresh approval rather than granting additional actions. Conflicting environments/policies, unknown launchers, unsupported TOML, unsafe/linked files or unsaved/concurrent edits preserve the file and produce a message for review. Missing configurations do not enable an agent. User-wide and client-private approval stores are not rewritten. Restart the agent and approve renamed tools through that client's controls.



In 1.5.11, opening an agent guide checks the selected source/collection against a complete, revision-bound native registration listing. An obsolete editor registration can be repaired only when there is one matching active native index; ambiguity, missing registrations and unavailable metadata remain explicit failures. This changes the editor reference, not native registrations or index contents. Setup rechecks before writing configuration.



After a fresh installation or a reviewed storage change, use the explicit **Update existing AIRK connection** action for Codex, or **Install** in the supported JSON-client guide. These actions can replace the storage path and project identity together for the same source, collection and recognized launcher. Recognized legacy installed launches migrate using their matching launcher, canonical source and collection even if local binding receipts have been erased. Obsolete data/binding selectors are removed together. Unknown commands, other source roots/collections and custom launch arguments remain for review. Root-based entries remain unchanged across binding/storage replacement. Merely replacing `service-1.4` with `index-v1` in an existing configuration is insufficient because storage participates in the native project identity. Codex saves retain the document's newline convention and read back the saved file. Both creation and update automatically probe AIRK; the guide distinguishes saved, already matching, and connection-check failure. This does not prove the external agent loaded its configuration or override its tool/disabled policies.



Index jobs, sources, publications and model settings are unchanged by connection migration. The service remains the sole owner of indexing and recovery. See [index management](index-management.md), [the evidence contract](mcp.md) and the [1.5.7 development record](releases/release-1.5.7.md).



Successful lightweight status snapshots are cached in memory for less than five seconds per native client/project. The result includes the responding AIRK `version`, basic `health`, collection/scope, index counts and an exact publication token. Source and semantic states are summarized without pipeline/selection digests or detailed coverage metadata. `observed_unix_ms`, `cache_age_ms` and `cached` disclose the observation and cache age. In the 1.8.0 F05 candidate, source-scoped calls recheck the saved registry's incarnation and revision before reusing a separately bounded root/collection observation; a changed stamp or expired observation requires fresh registration decoding. The same observation fences the native status cache, so status does not open the registry again. Explicitly pinned connections retain their active-registration check; F06a also captures matching registry stamps around that lookup to bind the status receipt to the observed namespace/revision and worker generation. Removed/recreated bindings and newer observed worker epochs cannot reuse old status state. Unavailable/unknown/error native evidence results are not cached. Diagnostics bypasses and invalidates the native status cache.



A completed status query that overlaps ordinary scalar-cache invalidation returns its original snapshot successfully with `status_observation.superseded: true`, `reason: scalar_invalidation` and an explanatory message. Original publication and observation age are retained. The superseded snapshot does not repopulate the current cache or acknowledge an obsolete status receipt; the query is not replayed. Project/namespace changes, authenticated worker replacement, expired source authorization and shutdown retain their existing rejection behavior. Cached reuse has its five-second limit; configured evidence capacity has one additional reserved status slot.



F06a gives each frontend a dedicated owned status observer, separate from the evidence connection lock. Its immutable rendered snapshot is keyed by exact response options, so terse and full requests retain their distinct fields; changing options obtains a new observation without forcing a new healthy connection. Its private direct channel can be reused within the existing idle/grant lifetime when namespace, project and authenticated worker generation still agree; scalar invalidation alone does not force fresh brokerage. The channel is separate from cached data, and an uncertain exchange discards it without replay. Identical in-flight calls share that observer; conflicting calls receive a known-unsubmitted busy result. Completed evidence/diagnostic outcomes invalidate local scalar reuse even when the publication is unchanged. Unsolicited native changes remain bounded by the original five-second snapshot lifetime. Shared work is confined to one frontend; independent agents retain independent logical receipts and no worker-wide scalar cache is implied. Within that frontend, status and evidence channels share the latest authenticated worker identity, separately from scalar-cache invalidation. A known namespace/project replacement or newer actual worker prevents reuse of either older channel; equal generations require the exact same identity. Health, Connect and Query admission retain the original scope/expiry and EOF fence, including setup retries. An already submitted old read can return its own result but cannot overwrite the newer worker's receipt or repopulate cached status.



The two-second observation allowance bounds the public waiter. An already accepted status exchange remains owned and drains under the existing transport limits (five seconds for write and 120 seconds for read); it does not imply a two-second shutdown or OS-I/O bound. No background index work or periodic status polling is added to agent connections. Cached responses are snapshots, not live-source verification.



The source-scope observation can retain missing, removed or ambiguous registration metadata for less than five seconds under those same revision checks. Such status replies still have null counts and publication, and disclose the original scope observation age. Rechecking the stamp does not extend that age; an absent registry is checked afresh. Unannounced project-row damage that does not change the native revision is detected on the next full lookup, rather than deeply audited on each hit. This metadata policy does not weaken worker checks of returned evidence. A cancelled lookup remains owned until it finishes; EOF closes admission and drains accepted lookup work. Queueing and registry contention share one two-second admission allowance, and an expired observation is refused before query submission.



Cold installed-runtime verification is shared within each MCP session for both source-scoped and explicitly pinned connections. A quick-status timeout reports unknown readiness with null evidence and retains the actual verification owner. F06a's original-start setup deadline closes admission to further setup effects while already accepted work drains. A later caller can observe its real completion; expiration does not authorize a replacement operation or fabricate readiness. A failed verification never becomes ready. In the 1.8.0 F04 candidate, full verification runs on an owned blocking worker; actual transport EOF/read failure closes new setup admission before SDK response draining, and session shutdown joins its driver and worker. Already accepted exact lifecycle operations may settle, while later elections are refused. This does not replay submitted evidence queries or weaken payload, control-root or health identity checks. Static initialization authenticates bounded metadata without authorizing payload execution; launcher verification remains independent. The [execution ledger](optimizations.md) records the phase's qualification evidence and limits.



## Concurrent evidence requests



Management → Service → Concurrent requests… sets the service-host policy: default 16, choices 4, 8, 16, 32 and 64. MCP admission, independent native channel pools and project-worker processing use the same allowance, with one extra status/control slot. Native owners acknowledge live changes before the policy is saved; subsequent frontend requests observe it. Reductions drain accepted work, and finite queues and typed unsubmitted retry receipts remain authoritative. See [service resources](service-resources.md#concurrent-evidence-requests-in-185) and [candidate validation](releases/release-1.8.7.md#candidate-1-8-5).


## Request capacity in status, diagnostics and plans

Successful MCP `status` and `diagnostics` responses (including the `airk_status` and `airk_diagnostics` aliases) include top-level `query_concurrency`, for example `"query_concurrency": 16`, in both JSON text and structured content. It is the evidence request limit read by the frontend for this request's admission: default 16, configured choices 4, 8, 16, 32 and 64. The separate reserved status/control slot is additional. It does not report free permits, queue depth or guaranteed admission. For a multi-project bridge, it is the minimum configured backend limit used by that bridge.

CLI status/diagnostics include top-level `query_concurrency`. Workbench `/status` and `/diagnostics`, CLI-executed plans and MCP `parse` retain it inside each completed step result. The shared frontend/CLI response adapter reuses the outer request's admission observation; payload accounting and output limits still apply. Previously saved replies retain their original fields; rerun them with 1.8.7 or later to obtain the new data.

The value reuses the existing admission policy read; reporting adds no configuration read, collection access or service RPC. It is refreshed for each request even when the index status snapshot is cached. Original publication, observation time and cache age continue to describe the index snapshot. Error receipts remain unchanged. Both terse and full responses include the field; status retains its lightweight scope. The [bridge implementation and regression](../crates/airk-mcp/src/remote.rs) cover live capacity changes, aliases, cached metadata, text/structured agreement, error preservation and one policy read per request.

Status and diagnostics preserve recognized temporary publication/selection contention as the existing typed read retry receipt (`publication_busy` or `selection_busy`, with `action: repeat_same_request`). This is not evidence that the index is unavailable. Honor the exact retry delay and repeat only that read. Diagnostic runtime/work context does not suppress the receipt. Missing or damaged evidence retains its separate existing reporting path.

See [CLI, MCP and Workbench parity](read-interface-parity.md) for the complete command audit, input/output contracts and regression coverage.
