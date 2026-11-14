// SPDX-License-Identifier: Apache-2.0
//! Guidance for implemented public behavior; command flags and tool schemas come from their live definitions.
use super::Topic;

pub(super) const TOPICS: &[Topic] = &[
    Topic {
        id: "workbench",
        title: "Workbench and bounded evidence plans",
        text: r#"Workbench presents local indexed questions, editable bounded plans, evidence recipes, native schema-generated forms, cited source/context actions, copy/export and local sessions. Use AIRK: Open Workbench in a trusted workspace. Default reads use the window's native scope; an explicit selection stays bound to it. Missing, stale or revoked scope is a refusal, never permission to switch indexes.
The parse tool uses airk.parse.request/1 and mode prepare or execute. Supply exactly one of text, recipe or plan. Explicit steps retain id, public tool, args and optional named bindings. Read capabilities.workbench for actual supported schemas/selectors. CLI uses airkctl parse TEXT, --execute, --project and --request FILE (or bounded stdin).
Review proposed plans before execution; learned suggestions remain inert until explicitly submitted. Plans retain exact publication, scope, digests and budgets: at most six steps, twelve reads, twelve fan-out targets and 512 KiB, with deadline checks between admitted synchronous reads. Partial results preserve failed/skipped steps, clipping and continuations. Stale plans never silently switch publications. No source writes, management commands or recursive plans. References are occurrences and relationships are candidates, not resolved-call proof.
Sessions can contain local excerpts; review copy/export destinations. Optional Arbiter answer gathering differs from parse and neighboring-line context; see topic=arbiter. A visible control or an available model does not prove independent model/platform qualification."#,
    },
    Topic {
        id: "about",
        title: "About AIRK, Ioka and the community",
        text: r#"AIRK is the independent Artifact Indexing and Retrieval Kernel: local indexed evidence for coding agents, with original locations, hashes and exact revision identities. Current service/extension versions and qualification are separate observations; inspect the actual installed products and release record rather than a historical version paragraph.
AIRK is an open-source project by Ioka LLC under the iokaio identity. Coding agents assist implementation under human design, verification and release authority. Original source/documentation is Apache-2.0; dependency notices retain their licenses.
Munarium's extraction/provenance work and VCP's coding-agent retrieval experiments informed the design. Ioka's history records VCP paused on October 6, 2026 as its scope became too extensive alongside Munarium. Their implementation/orchestration stacks are not incorporated. The inspiration record and NOTICE preserve attribution.
Website: https://ioka.io/airk ; downloads: https://airk.ioka.io/index.html ; source/issues: https://github.com/iokaio/airk ; contribution guidance: https://github.com/iokaio/airk/blob/main/CONTRIBUTING.md . Community: https://discord.gg/sJase5FbE . Review paths, excerpts, logs and share tokens before posting support evidence."#,
    },
    Topic {
        id: "explorer",
        title: "Explorer UI and agent assistance",
        text: r#"The shared main destinations are Overview, Search, Indexes, Activity, Agents, Service and Help. Explorer retains Code, Documents, Evidence, Review and Strategies under Explore evidence, including existing deep links. Advanced management entries open native browser Admin; Activity/Usage, index scope and supported agent controls remain native-backed inside the editor. Use AIRK: Open Explorer in VS Code; standalone service/CLI/browser/agent use does not require the editor.
Help searches bundled full native and Explorer guidance offline. Developer, Tester, Architect and Writer guides describe purposes rather than rewriting indexing policies. A contextual Help action navigates guidance; it never authorizes management.
The running MCP catalog supplies actual evidence, Help, readiness, diagnostics and reviewed draft schemas. A draft only prefills a matching trusted window for user review; validation is not admission. Suggestions wait for the user; applied never means executed. Evidence follows native selection authority rather than an arbitrary UI dropdown. Open agent defaults may explicitly follow a window scope; reviewed grant-bound connections preserve their fences.
An agent can read only a snapshot the user explicitly shared through its supported handoff. Retain expiry, revision, binding, publication, hashes and revocation details. Use tool selectors for complete actual schemas, command selectors for parser-generated flags, and indexing-choices before changing evidence detail or vector work."#,
    },
    Topic {
        id: "roles",
        title: "Developer, Tester, Architect and Writer",
        text: r#"Choose the user's purpose and source formats before recommending indexing changes. Ask when these are unknown. Read airk_status for the selected collection/publication and airk_capabilities for actual adapters. Overview's four role lenses are navigation aids, not automatic indexing presets. The Explorer Help screen has an offline guide for each role plus indexing-choices and document-types.
Developer: start with Agent Lean for supported declarations/imports/call candidates; use lexical search for exact errors and names, then symbols, code_facts, references and bounded context. Inspect tests/specifications too. Rich Structure helps when broader supported structure or declaration continuity is needed. A Runnable bug/API/component strategy can prepare evidence; verify live files before edits and run the project's own checks outside AIRK.
Tester: retain code, tests and requirements sources. Agent Lean supplies useful syntax/prose facts; consider Rich Structure for broader document/table inventories. document_facts can find requirement/acceptance candidates, and references can find written IDs. Review Changes/Impact/Tests produces leads, not test execution or certified coverage. A Planner task should state expected behavior, boundary/negative cases and acceptance checks. Citation validation does not verify behavior.
Architect: compare documented decisions with written imports and structural/candidate relationships. Agent Lean is a starting point; Rich Structure may add needed structure/continuity. Bound each investigation by component and collection; additional source roots need review. Inspect historical ADR status, unresolved edges, imported-producer claims and old rename/deletion paths. No absent edge proves absence of coupling or behavior.
Writer: Search Only suits text/quotation lookup; choose Agent Lean or richer detail for requirements, decisions, headings and table facts. Verify actual implementation before documenting shipped behavior. Check PDF page/Office locations, OCR provenance and spreadsheet formula-cache limitations. Conceptual vector/hybrid discovery still needs cited source support. Review the complete prompt or task before sharing and check returned citations before publishing.
Across roles: retain exact publication tokens, source hashes, original document locations and independent freshness/semantic observations. Follow needed continuation pages and disclose clipping. Never claim test success, complete language understanding, live freshness or full document coverage from a search result or a verified saved-byte snapshot alone."#,
    },
    Topic {
        id: "indexing-choices",
        title: "Indexing choices by purpose and source",
        text: r#"Choose evidence detail, vector work and extraction separately. Open native Admin Indexes, inspect the intended index and its settings; Explorer links advanced management to native Admin. The Index detail picker uses essential, agent_lean, balanced and detailed. Inspect airkctl settings and the guarded configure interface for actual extraction settings; preserve adapter overrides and use generated command help for exact inputs. Changing settings is not proof the index has published them; inspect configured/published state and the native job outcome.
Search Only (essential): text and citation lookup without rich structured facts. Suitable for known phrases and quotations. Poor fit for requirement/acceptance/table inventories or code relationships. Qualified parsers still run; no proportional PDF/Office conversion speedup is promised.
Agent Lean (agent_lean): new-collection default, retaining selected declarations, imports, local structure, unresolved calls and useful prose facts. A starting point for Developer/Tester and mixed sources. Broad document inventories and continuity fingerprints are omitted. Existing saved policies retain their semantics; opening a role lens does not rewrite them.
Rich Structure (balanced): broader supported structural/framework/document evidence and declaration continuity. Consider it for architecture/impact or document/table inventory gaps demonstrated in lean output. Detailed (detailed) retains the full existing adapter-supported evidence, including document-unit continuity where implemented. Neither level creates complete semantic resolution; richer detail costs more extraction and retained data.
Vectors are independent: lexical suits exact names, IDs and quotations; vector/hybrid can help conceptual questions if a local model and selected vectors are ready. Querying never downloads models or starts enrichment. Agent Lean's default vector policy skips reviewed lockfiles and minified JS/CSS while retaining lexical evidence; it is not a general generated/vendor/test classifier. Preserve useful tests/specifications and scope semantic exclusions deliberately. Explicit saved vector policies and opt-outs remain authoritative.
Choose extraction for actual formats. Code/config adapters supply supported syntax, not build execution. Markdown/plain text and text-layer PDF, DOCX, XLSX and PPTX expose different original locations and limitations; ask topic=documents and topic=formats. Scanned PDF/images require explicit local OCR and provisioned assets; raising detail or enabling vectors cannot recover absent text. Adapter overrides cannot expand supported dialects, run macros or recalculate spreadsheets.
Automatic watching suits evolving sources. Manual watching suits deliberate snapshots or costly trees; the user requests Index now. Watch off, project Pause, job cancellation and service Stop are distinct. Ordinary source changes usually need ordinary indexing, not repair/rebuild. Source ignores, scan limits, fact/byte budgets and resource admission can reduce coverage independently of detail. Check representative file_info records and native failures before increasing limits.
Ask the user about purpose, formats, dataset size, machine budget and freshness needs when choosing settings. Validate a proposed policy before saving it, preserve roots/overrides, observe publication completion and compare useful evidence/cost. Do not infer complete coverage or live-source equality from file counts."#,
    },
    Topic {
        id: "job-management",
        title: "CLI options for managing project work",
        text: r#"Use the registered project's actual global scope selectors with every project command; ask command for its generated flags before assembling the invocation. These descriptions execute nothing.
Use the exact project binding from native observations with airkctl --service-project when applicable. --service-runtime-dir names the verified service control directory, not the program directory. Historical serving_worker or broker-fallback fields do not imply separate resident processes. Never substitute a job ID for a project binding.
airkctl status reads published readiness plus runtime/work observations. airkctl index submits ordinary indexing; acceptance is not completion. airkctl inspect submits a bounded native inspection job. airkctl setup-model submits bundled model setup while preserving an existing/custom selection.
airkctl pause and airkctl resume change the selected project's native pause policy; they do not stop the shared service or delete data. Some configuration/maintenance operations may be eligible while paused. airkctl watch on and airkctl watch off change automatic source watching; disabling watching is not cancellation of already accepted work.
Canceling an active job and canceling a queued job are distinct typed service RPC operations, cancel and cancel_pending. Use the exact observed job ID from airk_diagnostics.work and the verified runtime control location. Ask command="service rpc" for transport flags. Do not invent request keys, IDs, preview digests or approval values.
Whole-service start/restart/stop and runtime installation are separate from project pause. Ask command="service start" and command="service" for exact help. Low-level service shutdown is a drain request, not the durable explicit-stop preference. AIRK Explorer > Service provides the corresponding guarded controls if a complete CLI lifecycle request has not been prepared.
For uncertain submission outcomes, recover the exact saved request through service requests rather than creating duplicate work. Ask command="service requests" and topic=request-recovery."#,
    },
    Topic {
        id: "index-health",
        title: "Explaining workspace index health",
        text: r#"Read airk_status for the actual selected collection. Publication identity and file/chunk counts describe committed queryable evidence. Discovery readiness and semantic readiness are independent; lexical evidence may be useful while selected vectors are pending or partial.
An index with no index set can report not readable: no index set. Review its memberships in Indexes > Search scope or Admin before requesting evidence. Creating a set groups indexes; deleting that grouping preserves its member indexes and independent memberships/data. A split result is complete only when both result indexes and their intended set memberships are confirmed.
Freshness watermarks/timestamps/failures describe observed source intent and indexing completion, not a complete live filesystem proof. Source watcher enabled and native/periodic backend coverage are separate. Check watch.failure, including root_limit or unavailable roots, instead of treating enabled=true as complete coverage.
The work bundle separately reports the sampled active/latest job, phase/counters, queued jobs/truncation, pause state, source watcher, progress sample age and exact-job waits. Check progress_current_worker before interpreting a cached phase as belonging to the serving worker. A successful health response and increasing heartbeat do not prove real work advancement.
Unknown totals remain unknown. A missing applicable wait is not evidence of no wait. A native failure code, stale worker sample, failed_observations entry or long interval without progress calls for refreshed observations and scoped diagnosis; one sample alone does not prove a deadlock or corruption.
Use runtime identities to check that frontend, worker and service versions/builds match when evidence is available. Use topic=job-management to explain CLI actions, topic=troubleshooting for symptoms, and topic=maintenance before proposing repairs or deletion."#,
    },
    Topic {
        id: "overview",
        title: "What AIRK does",
        text: r#"AIRK indexes local code and documents and returns bounded evidence with original locations, hashes and exact indexed revision identities. Sources remain read-only.
One per-user airk service hosts index owners, source watching and local inference in-process. airkctl owns public commands; VS Code uses one airkctl session per window. Agents connect directly to authenticated loopback HTTPS MCP. Stdio MCP and the stable launcher are retired; no resident process runs per index or agent.
Use status for published readiness, diagnostics for runtime/work observations, and help for documentation. Use topic=topics, topic=commands, command="service" or tool="search" for current guidance and actual schemas.
Install and verify the service separately from the extension. Extension activation does not silently install/update/remove the service or register/index folders. Offered automatic installation requires explicit user choice; manual installation and deferral remain available. Set up the index/set, intentionally prepare models/index work, then connect an agent with a default or reviewed scope.
Bundled Help is not live configuration, source-freshness proof or platform qualification."#,
    },
    Topic {
        id: "topics",
        title: "Help topic catalog",
        text: "Choose a topic ID from topics. Use command for exact CLI help generated from this executable's parser, or tool for a current MCP description and input schema. Follow next_line with the same selector and max_bytes to continue a long answer. Help never executes a command.",
    },
    Topic {
        id: "quickstart",
        title: "Getting started",
        text: r#"Download the matching native service installer or manual ZIP from https://airk.ioka.io/index.html and verify release identity. AIRK Service and the universal VS Code extension are independent products. Extension activation does not silently install or update the service; any offered automatic setup requires explicit choice, with manual installation and Not now alternatives.
After PATH changes open a fresh terminal. Run airkctl version --output json and airkctl service status --output json. Intentionally start with airkctl service start, then open browser Admin with airkctl admin open. Service, CLI, browser Admin and standalone agents do not require VS Code.
In Admin review the source folder, separate index storage and index/set setup, then intentionally index. Model setup and vector work are separate choices. Acceptance is not completed publication; inspect readiness/progress. In VS Code trust the workspace and select its search scope before using Workbench or connecting an agent.
Use airkctl agents --help for headless setup, or Explorer > Agents for an open connection/default scope. Agent choice is off by default. Reload changed configuration in the external client; a saved entry does not prove the client loaded it. See installation, portable and trust topics and command="library setup". Reading Help executes no example."#,
    },
    Topic {
        id: "cli",
        title: "CLI scope and invocation",
        text: r#"airkctl is the public service client; airk owns native service/setup roles and has no public user command tree. Use airkctl --help or append --help to an exact command path. Selectors include --service-runtime-dir, --service-project, --root, --data-dir and --collection; reviewed catalog/combined sessions differ from open index/set selection.
airkctl tools list --json prints actual schemas; airkctl tools help search describes a tool. airkctl tools call airk_help --args '{"topic":"overview"}' reads guidance through the service without resolving a project. Tool catalog/call reads are online; CLI --help and bundled UI/static Help remain available offline. --args-file avoids shell quoting errors.
Ordinary online commands may start an exited service using the verified matching pair and honor saved Stop. version, service status/health, lifecycle and offline diagnostics do not automatically start it. Root airkctl status is selected-index evidence and keeps online admission.
Preserve complete --output json results, error lines, truncation, continuation and retry details. Acceptance is not completion; never replay uncertain mutations with a new identity. Private roles and the tools console are not a general shell."#,
    },
    Topic {
        id: "commands",
        title: "Public CLI command catalog",
        text: r#"The paths below are generated by the airkctl parser. Ask command with a path such as service status, agents connect, library setup, tools call or usage report for current flags. An empty command or airkctl selects root help. Hidden native/setup roles are excluded. Descriptions never execute commands."#,
    },
    Topic {
        id: "mcp",
        title: "MCP connections and response modes",
        text: r#"Agents connect directly to the discovered authenticated loopback HTTPS /mcp endpoint. Stdio MCP, the stable launcher and per-agent proxy processes are retired. VS Code is an optional setup interface, not a transport proxy. Supported one-shot airkctl credential helpers exit after supplying a token; native credential custody and issuer trust remain separate requirements.
An open connection reads its service-owned default scope. Choice off allows unnamed reads of that assigned set and explicit narrowing inside it; list_indexes is unavailable and outside choices refuse agent_scope_choice_disabled. Choice on permits enumerating/reading every index of this OS user. An empty default refuses agent_default_scope_unset. Grant-bound connections retain reviewed selections/fences.
Inspect tools/list or airkctl tools list --json for current availability. Defaults and choice apply on later calls; a cached tool list may need refresh/reconnect. No live tools-list notification is promised. Terse is default; response_mode=full adds detailed provenance, coverage and readiness. Preserve full errors and continuations. Health/status do not prove live source freshness. Reconnect when a configuration or catalog update requires it."#,
    },
    Topic {
        id: "tools",
        title: "MCP tool discovery and arguments",
        text: r#"Use airk_help with tool set to an exact tool name to read its actual description and input schema. The public unified catalog uses unprefixed names and can vary with the connection's authority. airkctl tools list and airkctl tools help TOOL expose the actual native catalog; known airk_ compatibility aliases remain callable. The evidence names below describe those compatibility aliases; use their unprefixed counterparts on the unified MCP connection. Required fields, defaults and supported enum values come from the running tool catalog.
airk_list_files discovers indexed paths. airk_search finds relevant text; airk_context and airk_read_file expand bounded excerpts. airk_file_info explains extraction and coordinates. airk_outline, airk_symbols and airk_code_facts navigate code. airk_imports, airk_references and airk_relationships supply explicitly limited dependency/occurrence evidence. airk_document_facts finds structured and prose facts. airk_changes validates remembered evidence. airk_capabilities reports supported adapters. airk_status reports index/runtime/work observations. airk_help explains usage.
For CLI tool calls, pass a UTF-8 JSON object using --args-file. Inputs are bounded to 64 KiB. Unknown fields, invalid selectors and traversal attempts are rejected."#,
    },
    Topic {
        id: "tool-selection",
        title: "Choosing an evidence tool",
        text: r#"Start with a literal path prefix when a question concerns one component. Discover paths before reading them. Use focused lexical search for exact words or identifiers and vector/hybrid search for approximate conceptual matches when local vectors are ready.
Use outlines or exact symbols to locate definitions. Expand the selected hit with context or a bounded read. Use file_info before interpreting partial syntax or extracted document coordinates. Retain citation hashes, read validators and exact publication tokens.
Use changes for remembered evidence and independent freshness/coverage observations. Indexed evidence is not a live filesystem check. Verify live source before editing. An absent edge or search hit does not prove there is no relationship or behavior."#,
    },
    Topic {
        id: "search",
        title: "Lexical, vector and hybrid search",
        text: r#"Lexical search normalizes punctuation and quotes into AND terms over indexed text; quotes do not enable phrase matching. Lexical candidates use stable-id order, not relevance ranking. Vector search is approximate similarity using a provisioned local model. Hybrid combines lexical and vector candidates from one pinned generation. Omitting mode follows the saved collection search policy; explicit vector requests do not silently become lexical results.
Use airk_search mode=lexical for exact terminology. Use vector or hybrid for conceptual questions when vectors are available. path_prefix is a literal collection-relative prefix, not a glob. limit and max_bytes bound returned evidence; check truncation before drawing broad conclusions.
Pending, partial, ready or failed semantic readiness is reported separately from lexical/discovery availability. Vector similarity does not resolve references or prove an answer. Querying does not download models or start enrichment.
Ask tool=airk_search for the current argument schema and command=search for native CLI flags."#,
    },
    Topic {
        id: "indexing",
        title: "Indexing jobs and phases",
        text: r#"Native source work discovers and captures admitted files, extracts retained evidence, prepares changes and publishes bounded batches. Semantic work can continue independently in later embedding batches. A published lexical generation can be usable before all selected semantic artifacts finish.
Captured/extracted files, prepared/committed changes and published batches describe different stages. A missing total remains unknown; do not invent a completion percentage. Job accepted or pending means durable scheduling, not finished indexing.
Use airk_diagnostics.work for scoped job, progress, watcher and sampled wait observations, and top-level readiness/publication fields for what is actually queryable. Compare sample age and worker identity before treating cached progress as current. Service health and worker heartbeats are not real-work advancement.
Use airkctl index for ordinary indexing and native Admin for pause/resume, queued cancellation or guarded rebuild/repair. Ask command=index and topic=jobs."#,
    },
    Topic {
        id: "jobs",
        title: "Ongoing jobs, queues and waits",
        text: r#"Job records identify kind and state independently of progress. Source reconciliation/indexing, model setup, semantic embedding, inspection, configuration and maintenance can be separate jobs. The queue observation is capped at 16 pending jobs and includes a truncation flag; pending_job identifies its first item.
airk_diagnostics.work distinguishes sampled progress from the current worker generation, the latest/active job, bounded queued jobs, project pause policy and available wait metadata. No matching wait observation means there is no applicable sample; it does not prove that work is unstuck or CPU-active.
Compare job ID, service/worker identities and fencing generation. A wait reason belongs only to its matching job. last_advance_age_ms measures time since real progress; unpaused_idle_ms excludes explicit pause. No single elapsed interval proves a deadlock.
CLI management uses typed service RPC and durable request keys. Ask command=service and topic=request-recovery for exact recovery rules. AIRK Explorer > Service and native Admin expose reviewed pause/resume/cancel/restart and repair controls."#,
    },
    Topic {
        id: "freshness",
        title: "Published state and source freshness",
        text: r#"Publication tokens bind collection, incarnation and revision; a generation number alone is not an identity. Reads describe the last successful indexed snapshot. Keep the exact token and citation source hash when connecting evidence across calls.
Freshness contains requested/completed/running watermarks and observed timestamps/failures. These describe observed source intent and indexing work, not a filesystem proof that every current source byte matches. live_source_verified=false must not be presented as verified freshness.
airk_changes checks indexed continuity and remembered selectors. Follow every needed cursor page before advancing a baseline. Its index-current validation is distinct from live-source verification. Moves are content-based candidates, not proof of filesystem intent.
Do not confuse runtime version, extractor version, publication identity and semantic pipeline identity. They answer different questions."#,
    },
    Topic {
        id: "code",
        title: "Code facts, references and relationships",
        text: r#"AIRK extracts supported native syntax: definitions, written signatures/types, imports, call occurrences, local scopes and selected framework/document facts. It does not provide complete language understanding or generally resolve written targets.
symbols locates definitions; code_facts provides compact structured evidence; imports shows written import/use/include declarations. references finds exact-case whole-identifier textual occurrences, including strings/comments/definitions. It is not a semantic reference resolver.
relationships supplies bounded structural links and unresolved candidates. Imported hash-bound producer assertions retain their producer basis. Check language/profile coverage, partial syntax, truncation and original citation locations before using facts for impact analysis.
Ask topic=tools and tool=airk_code_facts, airk_references or airk_relationships for exact filters and limits."#,
    },
    Topic {
        id: "documents",
        title: "Document facts and original locations",
        text: r#"Document adapters retain original artifact paths and extraction provenance. PDF, DOCX and PPTX excerpts use extracted-text line coordinates inside an original page, block or slide; those line numbers are not original document page numbers.
document_facts exposes supported headings, structured keys/tables and bounded prose cues such as requirements, design or decisions. A cue is evidence, not an inferred obligation. Grammar or container support alone does not establish complete document understanding.
file_info reports the indexed language/profile, extraction method, hash, line basis and parse quality. Use it before reading document excerpts. Explicit local OCR evidence retains producer/recognition limitations and must not be described as original typed text.
Ask tool=airk_document_facts and topic=formats."#,
    },
    Topic {
        id: "formats",
        title: "Supported formats and adapters",
        text: r#"Use airk_capabilities for the running binary's actual native format and local provider inventory. Exact adapter profiles describe configured capabilities and limitations. Capability support is separate from whether an optional provider or this collection is ready.
Native ingestion covers supported source grammars, plain/structured text and document containers. Per-file file_info and indexing failures explain extraction method, partial syntax and unsupported/failed evidence. Provisioned local OCR/model/code adapters require explicit local inputs; AIRK does not add remote provider fallbacks.
Original sources remain read-only. Extraction budgets, ignored files, file-size limits and provider availability can reduce coverage. An indexed file count alone is not complete workspace coverage."#,
    },
    Topic {
        id: "models",
        title: "Offline models and inference",
        text: r#"The service runs offline MiniLM inference in-process through one shared, bounded model owner; since 1.8.14 there is no separate airk-embed helper. Compatible projects share verified model assets and loaded sessions. Opening a read-only connection does not itself load a model.
Model setup, source discovery and semantic embedding are separate work. Vector opt-out keeps lexical evidence available. An explicit vector query fails honestly when the required model/index is unavailable. Querying does not download or install models.
Custom models require supported local manifests and verified assets. Use the reviewed project configuration path; do not point a workspace setting at an arbitrary executable. AIRK Admin exposes explicit model setup and shared in-process model observations.
Use airk_status semantic_readiness/local_model_ready and runtime/work observations, then airk_capabilities for adapter limits. Ask command=model-catalog and command=configure for current CLI help."#,
    },
    Topic {
        id: "service",
        title: "Native service and process ownership",
        text: r#"One native airk service per user hosts independent index owners, watching, cleanup, admission and inference in-process. There is no resident process per project/index/agent. airkctl is the public client; airkctl session is the temporary per-window helper.
Health confirms responsiveness and exact observed identity, not freshness, semantics or work advancement. Work/progress, watching, inference and cached observations remain distinct. A queued/accepted job is not completion; retain failed and unavailable observations.
Use airkctl service status or service health to observe without automatic startup. service start intentionally clears saved Stop. Ordinary online reads may start an exited service but honor explicit Stop. Stop/restart/drain bind native ownership; never kill a guessed PID. Login startup is separate and macOS registration is unavailable in this build. Browser Admin needs the service running; its pages cannot restart a stopped server. Use the CLI or VS Code Start action then reopen Admin. See command="service", resources and troubleshooting."#,
    },
    Topic {
        id: "resources",
        title: "Memory, workers and resource policy",
        text: r#"Native policy bounds active in-process index owners, concurrent work, extraction/inference threads, admitted memory and background I/O. Queued projects can wait for memory/capacity while health stays responsive. Reserved memory is admission accounting, not measured RSS; host memory and logical CPU counts do not imply increased allowances or active threads.
Use native/Admin resource observations and sampled job waits. Missing applicable wait samples are unknown, not proof of no wait. Change policy through the owning reviewed controls, not retired editor scheduler settings. Keep per-index isolation and explicit pressure/failure signals. See command="service" and jobs. Help itself samples no resources and changes no policy."#,
    },
    Topic {
        id: "installation",
        title: "Installation and terminal command",
        text: r#"AIRK Service and the universal extension install independently. Download the platform installer/manual ZIP at https://airk.ioka.io/index.html and verify release digests and applicable signatures. Activation does not silently install/update/remove the service; explicit automatic setup, manual installation and deferral are different user choices.
Windows: run the downloaded AIRK Service installer interactively or with exact --silent. Unattended mode relays native Inno switches, never prompts for stdin and skips certificate trust by default. Headless /TRUST refuses rather than manufacturing consent. A predecessor reset needs /ACKNOWLEDGE-RESET.
Linux: sh ./install.sh --silent. macOS: bash "./Install AIRK.command" --silent from the verified installer. POSIX --yes remains an alias; no stdin prompts. A predecessor reset needs --acknowledge-reset. --no-start suppresses final start. Linux supports --no-login-startup; macOS login registration is unavailable. Read the POSIX wrapper's --help for actual location defaults/flags.
Native finalization, readiness and PATH failures are nonzero; Windows finalization failure is exit 11 and retains private recovery material. Inspect the original failure/receipt; file placement alone is not success. Verify in a new terminal with airkctl version and airkctl service status, then intentionally start with service start. On Windows Get-Command airkctl -All or where.exe airkctl explains resolution. See portable and trust. These are source instructions, not a claim that every platform/artifact is qualified."#,
    },
    Topic {
        id: "upgrades",
        title: "Replacement, rollback and legacy transition",
        text: r#"Update the native service using the matching installer or documented portable flow, and update the extension separately. One product's update is not permission to install/update/remove the other. Compare installed and running identities rather than a Help badge.
Native setup binds exact installation, roots, inventory and original operation. Interrupted/failed transactions retain recovery evidence for reconciliation; do not guess the current image or submit a blind new operation. Changed roots, unknown files, unproven ownership and held images are preserved refusals.
Uninstall retains index/state/cache/log data unless the separately confirmed purge applies. Reinstall with retained data preserves native identity or explicitly refuses/requires recovery. Old layouts/receipts do not authorize deleting sources or unrelated files. Refresh terminal resolution and external agent configuration separately. See command="service uninstall", installation, storage and request-recovery; obsolete runtime rollback/stable-launcher examples do not apply."#,
    },
    Topic {
        id: "configuration",
        title: "Project configuration and guard checks",
        text: r#"Project settings use native preparation, exact incarnation/settings guards and durable job receipts. Supported changes include model/vectors, extraction detail/policy, vector selection, source roots, scan limits, usage and shared quota policy. Use command=settings for available views and command=configure for the bounded draft input interface.
Native configuration can be asynchronous. A submitted/accepted job is not completed configuration. Keep the last configuration result separate from later indexing progress; reinspection is required when effect is uncertain or a reviewed guard changed.
Extension user/machine runtime and storage settings ignore repository overrides. Existing registered storage and watching preferences remain native selections. Choosing new default storage does not silently move or delete existing index files.
Do not invent JSON draft fields. Use exact CLI help, native Admin reviewed project controls and the version-matched configuration contract when constructing advanced requests."#,
    },
    Topic {
        id: "storage",
        title: "Index storage, isolation and quotas",
        text: r#"Each collection has a canonical source and independently owned derived storage. Sources cannot overlap registered data, control, runtime-state or bound logs. Original source files are read-only.
Native quotas account for current and retained objects, temporary/replacement admission and shared data-root usage. Limits can block publication even when host memory is available. A quota failure is different from a source-watcher root_limit warning.
Changing a default index-storage setting does not move an existing collection. Reviewed storage replacement preserves previous index files for separate unregister/cleanup. Removing a project stops new admission and drains owned workers, then automatically deletes its ownership-matched index, usage ledger and saved project settings. Sources and shared models are preserved. Pending cleanup resumes after interruption; unknown entries, aliases, ownership failures and held leases defer deletion and report failure. Only opaque identifiers and removal time remain in native history.
Historical 1.4 migration renamed its recognized default data/service-1.4 root to data/index-v1 while retaining revisions/settings. That dated procedure is not a general repair or permission to move current roots. Use the current native inventory and guarded relocation/recovery controls; occupied or redirected locations remain refusals. Do not recursively delete a shared AIRK directory to fix a single project. Ask topic=maintenance and command=settings."#,
    },
    Topic {
        id: "maintenance",
        title: "Repair, rebuild and deletion",
        text: r#"Repair planning inspects scoped state before choosing actions. Native plans retain exact identities, guarded previews and per-project outcomes; all required preview pages must be reviewed before execution. Partial failure and cancellation are distinct from successful project receipts.
Ordinary index requests are not full rebuilds. Full rebuilds and explicit repair cleanup require guarded native previews and separate approval. Confirmed project removal includes automatic deletion of that project's owned derived state. The public index --full shortcut refuses unreviewed rebuilding.
Project removal deletes owned index files, usage and saved settings after worker drain. Sources and shared models are preserved. Pending removal remains durable until cleanup completes. Payload repair preserves damaged slots according to its native inventory/ownership rules. Unknown files are not silently removed.
Use AIRK Explorer > Service and its native Admin recovery controls or exact typed CLI management interfaces. Ask command="service", command="library" and topic=request-recovery for actual served operations. Help does not authorize or execute repairs."#,
    },
    Topic {
        id: "agent-setup",
        title: "Connecting a coding agent",
        text: r#"Use the discovered native HTTPS endpoint, never a guessed fixed port or retired --root SOURCE mcp launcher. airkctl agents --help exposes current connect, verification, credential and trust commands. A saved URL alone grants no authority. Native configuration writers preserve unrelated servers, comments, disabled states and policies; concurrent changes or unsafe ownership refuse and retain private displaced-file recovery where applicable.
Open connections use agents connect --open and agents default for the assigned scope. Explorer > Agents applies the window scope and can follow later changes. Clearing the default refuses agent_default_scope_unset. Grant-bound connections retain their reviewed authority and revision fences.
Let the agent choose indexes is off by default: narrowing stays inside the assigned set, outside choices refuse agent_scope_choice_disabled and list_indexes is unavailable. Choice on permits enumerating/reading every index and set of this OS user. Later calls use changed defaults/choice, but cached tools may need refresh.
Load changed entries in the actual external client and complete its trust controls. Claude Code may need its MCP reconnect action, Codex a restart and VS Code a server restart. A native report does not prove a running agent reconnected. Preserve complete refusal/recovery details and original operation identity."#,
    },
    Topic {
        id: "request-recovery",
        title: "Unknown outcomes and exact retries",
        text: r#"AIRK management requests retain durable caller/key/scope identities before submission. A lost response can leave an unknown outcome even when native work was accepted or completed. The extension retains original keys internally, logs unresolved outcomes and checks read-only native receipts in the background. Recovery controls must retain the exact original operation/target and distinguish a retained receipt from new mutation permission. Unknown outcomes are not automatically replayed and do not globally block unrelated actions. For explicit CLI recovery, inspect the saved request and use only its exact identity. Canceling queued work, pausing a project and stopping the service are different operations. Native completed-request replay returns retained effects rather than repeating them.
Saved activation and executable retirement have separate native recovery journals. Changed selections, expired epochs, stale previews and unproven ownership are explicit refusals. Reinspect rather than editing journal files or weakening guards.
Use Activity and Service observations and diagnostic logs, and exact CLI help for deliberate request recovery. Ask topic=jobs, command=index and command="service"."#,
    },
    Topic {
        id: "troubleshooting",
        title: "Diagnosing common symptoms",
        text: r#"Start with airk_status for lightweight published readiness, counts, semantic coverage and source freshness. Use airk_diagnostics for service/runtime identity, work and editor observations. Successful status snapshots can be cached for less than five seconds; observed_unix_ms, cache_age_ms and cached disclose age. Status allows up to ten seconds for shared cold runtime setup, followed by a two-second observation budget. Expiry reports unknown readiness with null evidence. Pending installed-runtime verification remains owned within the session across timed-out polls; verification failures remain errors. Refresh observations before comparing counters. Service health alone is not indexing progress.
Not observed means a value is unavailable, not automatically failed. A queued model_setup job can coexist with running source work. Missing totals do not justify a percentage. No matching native wait observation is a missing applicable sample, not proof of no wait.
root_limit is a source-watcher traversal/resource limit, not proof of full disk. Watching enabled does not prove full native watch coverage. Pending source changes is a source-intent revision gap, not necessarily a count of modified files.
service_explicitly_stopped means saved Stop must be cleared by an intentional service start, not a reconnect. installation_not_found and pair_mismatch require the verified matching installation, not an arbitrary executable. scoped_read_unavailable or catalog_revision_mismatch require a fresh original-scope review; never silently widen or replace it.
For an old terminal version, close/recreate the terminal and inspect command resolution. For an old MCP connection, reconnect the agent separately. For locked old binaries, disconnect their clients before retrying installation cleanup.
Use logs, independent inspection and native repair previews when observations remain inconsistent. Preserve original sources and unknown files; do not guess process ownership or delete shared state."#,
    },
    Topic {
        id: "privacy",
        title: "Privacy and read-only boundaries",
        text: r#"AIRK serves local indexed evidence to the requesting client without modifying original sources. The agent's use of returned text is outside AIRK's local boundary. No cloud inference provider, external database or agent orchestration is added.
Index owners and inference share the native process. Agents use direct authenticated loopback HTTPS; local CLI/window control uses the same-user endpoint. Credential files, sessions, anti-forgery values and request/recovery custody remain native-owned. Help/tool schemas may document names such as caller or request_key; documentary names are not live secrets.
Model download/import is explicit and verified, distinct from offline inference. Reads never download models. Optional Arbiter requires global configuration plus immutable-index enrollment; excluded indexes contribute no optional-model content. Ordinary exact reads retain their results.
Review exports/diagnostics before sharing: evidence can contain intentionally returned excerpts; raw error logs can contain paths and error values. Help supplies public bundled text/schemas, never executes commands or opens private state. Preserve complete native failures instead of filtering them away."#,
    },
    Topic {
        id: "limits",
        title: "Budgets and interpretation limits",
        text: r#"Returned evidence is bounded by result counts, byte budgets and tool-specific traversal limits. Check truncation and continuation before claiming complete coverage. Literal path prefixes are not globs. Original source and extracted document coordinate systems differ.
Native syntax targets are generally unresolved candidates. Textual references include definitions/comments/strings; similarity is approximate; imported assertions depend on their producers. Absence of evidence is not proof of absence of behavior.
Runtime responsiveness, source watcher coverage, indexed source freshness, lexical readiness and semantic readiness are separate. Cached observations can belong to an earlier worker generation. Missing data remains unknown, not zero.
Help max_bytes bounds the complete JSON payload and next_line continues the selected article or schema. Ask an exact tool name for its actual current limits and input validation."#,
    },
    Topic {
        id: "usage",
        title: "Usage reports and savings estimates",
        text: r#"Usage accounting is local and scoped. Native reports track supported performed operations and retained evidence comparisons; accepted jobs alone are not proof that work completed. Query text and source content are not needed for usage counters.
Savings comparisons depend on the retained full-context specification, exact response revision and supported client/model pricing assumptions. Missing or mismatched comparison evidence stays an explicit reason, not fabricated savings. No universal provider-token or latency guarantee follows from a local fixture.
Native report windows are 1..30 days; a 31-day request refuses rather than silently widening the window. Wide native totals remain decimal strings; missing measurements stay unknown.
Use command=usage to discover report/policy/export options and command=configure for guarded policy changes. JSON, CSV and Markdown native reports are bounded and can require continuation. Shared quota limits are separate from usage/savings reporting."#,
    },
    Topic {
        id: "versions",
        title: "Versions, builds and runtime diagnostics",
        text: r#"The extension version identifies the VSIX; airkctl version --output json identifies the invoked CLI, I1/I2 contracts and discovered installation without starting it. airkctl service status observes installed/running state without starting a stopped service. A Help version identifies bundled content, not qualified artifacts/platforms.
One resident airk service owns index work and inference. Window airkctl session and CLI/installer/credential helpers are temporary. Compatibility fields may still say worker/frontend without implying separate resident processes. diagnostics retains exact observed identity, protocols, hashes, receipts, publication and work fields where available. Missing evidence is unknown, not a match.
Extractor/pipeline/publication identities describe indexed evidence, not the executable. Reconnect external clients where an updated configuration or cached tool catalog requires it; changing terminal PATH does not refresh those caches."#,
    },
    Topic {
        id: "architecture",
        title: "Components and data flow",
        text: r#"Core owns shared contracts; Host/process own platform and installation primitives; ingest/store/retrieval own extraction, derived state and vector primitives; Kernel owns publication, budgets and execution. MCP owns canonical evidence schemas/handlers. IPC and HTTPS/Admin adapt those owners. airkctl owns CLI/session, and the thin airk executable composes native service/setup roles.
Exactly one airk service is resident per installation. Index owners, cleanup, watching and inference run inside it. The per-window session, CLI/installer/credential helpers and bounded isolated PDF parser are temporary processes. Agents and browser Admin connect directly to native HTTPS; no proxy/sidecar is introduced.
Clients call owning native APIs rather than another authorization/retrieval engine. Preserve index/root/publication isolation, read-only sources, complete errors/coverage/continuations and explicit management consent. A local test qualifies only its recorded source/artifact/platform boundary."#,
    },
    Topic {
        id: "workflows",
        title: "Common user workflows",
        text: r#"Find a definition: discover paths, request an outline or exact symbol/code fact, then read a bounded cited excerpt. Explain behavior: search one component, expand hits, check call candidates and verify live source before edits.
Check indexing: read airk_status, separate published readiness from work progress, compare sample age/worker/job identity, then inspect failures/waits. Use native Admin or typed CLI management for an intentional pause, cancellation or scoped repair.
Manage installation: verify terminal resolution and runtime identities, reconnect agents separately, review ownership and retry deferred cleanup after old clients release files. Preserve project data and sources.
Ask airk_help for a topic, an exact command path or tool schema before constructing management commands. Help supplies explanations and examples; the agent must still use actual observed paths/IDs and the required native approval guards."#,
    },
    Topic {
        id: "glossary",
        title: "Terminology",
        text: r#"Index: independently owned derived evidence over admitted roots. Index set: grouping whose memberships select indexes; deleting it differs from deleting an index. Project binding: opaque native source/storage/collection identity. Publication token: exact indexed revision including incarnation.
Service: the one resident native owner. Index owner/worker: in-process index component, even when an older field says worker. Inference owner: bounded shared in-process model lifetime. Fencing generation prevents stale publication/delivery.
Discovery readiness describes committed extraction inventory; semantic readiness describes selected vector work. Freshness is observed source-intent/indexing watermarks, not live-file proof. Window scope is native editor authority; open default scope is an agent's assigned set; grant-bound scope is reviewed/fenced authority.
Matched pair: airk and airkctl staged/verified together. Stdio MCP and stable launcher are retired. Build receipt binds source/toolchain/profile. Retained receipt records original outcome/recovery. Unknown outcome may have effects despite a lost acknowledgment and never authorizes a blind new mutation."#,
    },
    Topic {
        id: "portable",
        title: "Portable setup, locations and removal",
        text: r#"The native service can run without VS Code. A manual ZIP is a verified native payload, not an automatically installed service. Extract the complete matching pair and inventory, verify release identity, then run the extracted airkctl service setup --portable --program <absolute extracted runtime folder>. Use command="service setup" for optional --data, --state, --cache and --logs paths. Native absolute/nonoverlap/ownership checks apply; never copy only one executable or select an arbitrary folder.
Portable setup uses the native transaction without adding PATH, OS login startup or certificate trust. Invoke airkctl directly when it is not on PATH. Verify with version and service status, then intentionally service start. No VS Code installation is required.
Update/reinstall with retained data uses native identity and recovery, not manual state edits. service setup-receipt observes the original receipt without starting the service. Stop/status/uninstall retain native ownership; ordinary uninstall preserves data unless the separate confirmed purge applies. Changed roots, unknown files and interrupted setup remain explicit recovery/refusal outcomes. Keep original receipts and private recovery material. Resume the same portable operation with airkctl service setup --resume <absolute original private request>; do not supply replacement paths, a new operation or PID controls. See installation, upgrades, storage and trust."#,
    },
    Topic {
        id: "trust",
        title: "Local TLS, browser sign-in and agent trust",
        text: r#"Browser Admin and MCP use discovered IPv4 loopback HTTPS. Copy the actual endpoint rather than guessing a fixed port. TLS issuer trust, an agent credential/grant and a browser session are distinct authorities.
Use airkctl admin open for native single-use browser sign-in. The launch secret is not a bookmark or public support link. Cookies/anti-forgery values stay in their browser/native session. A fresh browser load needs the service; bundled extension/static Help remains available offline. When VS Code browser certificate trust is unavailable, use the external browser.
Use reviewed agents connect/repair/verify and native credential handling. A saved URL grants nothing. Do not disable TLS verification or paste bearer tokens into diagnostics. Silent installers skip trust and never manufacture consent. Issuer replacement or revoked/stale authority requires its supported recovery; reconnect the actual external client after a successful update where required. Read command="agents", command="service trust" and agent-setup for actual controls."#,
    },
    Topic {
        id: "arbiter",
        title: "Optional Arbiter answers and model boundaries",
        text: r#"Bundled MiniLM supports ordinary semantic retrieval. Optional Arbiter contextual answers are separate: globally off by default, requiring explicit immutable-index enrollment plus a verified selected profile. Enabling globally does not enroll all indexes; deletion/recreation/import does not inherit consent.
Inspect actual native catalog/control output for the two smaller/two larger profile choices, requirements, supported tasks and qualification. A visible choice, downloaded asset or source implementation is not independent task/device/platform qualification. Unsupported/unavailable choices refuse honestly; no cloud, sidecar, unverified code or automatic read-triggered download is substituted.
The composed answer read differs from context, which reads neighboring indexed lines. A question such as which retry rule governs this queue can require a cited contract, implementation/test and document evidence. Bounded native gathering stays inside original scope/publications and budget, organizes supported cited facts, and exposes missing constraints/gaps instead of inventing an answer. This is an explanation of the task, not a fabricated successful model example.
Agents and Workbench use the same canonical contextual result when supported. Exact reads keep full results; excluded indexes contribute no optional-model content. Stale/revoked authority suppresses delivery. No recursive agent loop, source writes or commands. Fuller grounded context and fewer agent turns are goals; quality, latency and savings need independent measured evidence.
Use airkctl arbiter status to inspect the actual native profile catalog, requirements, readiness and configuration revision. airkctl arbiter configure --file <complete Configure request JSON file> applies the strict Core-native Configure request, including explicit immutable-index enrollments; use the generated schema, never invented fields. Provision and remove name --expected-revision and --profile; cancel and recover name --expected-revision. These are explicit native management operations; a status read never downloads or enrolls anything. Read command="arbiter" and each exact subcommand for current inputs. Provision, verified offline import, disable/cancel/remove and recovery preserve original configuration/request identities. Missing models never download during reads. Disablement suppresses delivery while actual native work/pins drain; do not remove live assets or replay uncertain provisioning. Use actual tools/list and CLI help for served controls. Ordinary retrieval remains available when optional advice is off or unsupported."#,
    },
];
