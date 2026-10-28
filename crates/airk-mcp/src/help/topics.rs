// SPDX-License-Identifier: Apache-2.0
//! Guidance for implemented public behavior; command flags and tool schemas come from their live definitions.
use super::Topic;

pub(super) const TOPICS: &[Topic] = &[
    Topic {
        id: "about",
        title: "About AIRK, Ioka and the community",
        text: r#"AIRK means Artifact Indexing and Retrieval Kernel: local indexed evidence for coding agents, with bounded passages/facts, original locations, source hashes and exact indexed revision identities. AIRK is an open-source project by Ioka LLC under the iokaio identity. Ioka describes coding agents assisting implementation under human design, verification and release authority. Original source/documentation is Apache-2.0; dependency notices retain their own licenses.
AIRK's design was informed by Munarium's extraction/provenance/file-artifact work and VCP's coding-agent retrieval experiments. Ioka's public history records VCP being paused on October 6, 2026 as its scope became too extensive alongside Munarium. AIRK focuses independently on local indexing/retrieval and leaves reasoning/development to the user's agent. Munarium and VCP implementation/orchestration stacks are not incorporated; the inspiration record and NOTICE preserve this distinction.
Website: https://ioka.io/airk ; source: https://github.com/iokaio/airk ; inspiration: https://github.com/iokaio/airk/blob/main/docs/inspiration.md . AIRK Explorer's Help > About AIRK shows the installed extension version, credits/history, license notices and user-clicked public links. A public website may describe an earlier release; inspect the actual installed version/runtime separately.
Join the Ioka Discord community for community help, questions, workflows and feedback: https://discord.gg/sJase5FbE . Please report bugs or propose improvements at https://github.com/iokaio/airk/issues . Code, test and documentation contributions are welcome; read https://github.com/iokaio/airk/blob/main/CONTRIBUTING.md . Review shared diagnostics for private paths/source/share tokens before posting."#,
    },
    Topic {
        id: "explorer",
        title: "Explorer UI and agent assistance",
        text: r#"AIRK 1.5 provides AIRK Explorer: Overview, Search, Code, Documents, Evidence, Review, Strategies and Management. Open AIRK: Open Explorer. The Help screen searches bundled Explorer and engine guidance offline; role guides cover Developer, Tester, Architect and Writer. Help for this screen opens contextual guidance. Overview lenses reorder journeys without changing indexing settings.
AIRK 1.5.7 exposes one airk MCP connection with 26 unprefixed tools. search and the native fact/file tools return indexed evidence. help explains AIRK; draft_search, draft_strategy and draft_plan prepare reviewed forms. suggest_view accepts navigation, review or management guidance. Templates, validation, proposal status and explicitly shared handoffs use this same endpoint. Native evidence remains usable without an open editor.
Connect Coding Agent writes one endpoint. Trusted startup replaces recognized earlier Retrieval and Explorer definitions in supported project files with airk, preserving unrelated servers and disabled state. Custom conflicts, unsafe files and unsaved edits remain for review. Restart the coding agent and approve renamed tools through its own controls. A recognized manual VS Code entry owns its policies, so the provider does not add another copy.
Use help query and topic_id for UI guides, and status.explorer for attachment, draft revisions and blockers. UI drafts require a matching trusted window with the configured project selected. Evidence never follows the UI dropdown. For multiple configured projects, call projects and pass project explicitly. Validation does not admit a draft; live_checked=false does not prove applicability. Suggestions wait for the user; applied never means executed.
Studio and Planner expose Share snapshot with agent. After confirmation the user gives an expiring token to get_handoff, which reads only that complete snapshot with revision/binding/publication/hash metadata. Tokens expire in ten minutes and revoke after relevant edits, binding/trust or observed evidence changes. General draft enumeration is unavailable.
Use help tool selectors for public schemas and command selectors for exact native flags. Native guides and terminal commands retain the prefixed airk_* evidence aliases for compatibility; those aliases are callable but omitted from public MCP discovery. Ask topic=roles and topic=indexing-choices before choosing richer evidence or semantic work."#,
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
        text: r#"Choose evidence detail, vector work and extraction separately. In Explorer open Management > Indexing, inspect the project, then Project settings. The Index detail picker uses essential, agent_lean, balanced and detailed. The CLI collections extraction detail command preserves adapter overrides; ask its generated command help for exact flags. Changing settings is not proof the index has published them; inspect configured/published state and the native job outcome.
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
Use work.serving_worker.project from airk_status when present, or its broker-fallback project field, with the CLI --service-project selector. Supply a verified custom --service-install-dir only when needed; never substitute a job ID for the project binding.
airk status reads published readiness plus runtime/work observations. airk index submits ordinary indexing; acceptance is not completion. airk inspect submits a bounded native inspection job. airk setup-model submits bundled model setup while preserving an existing/custom selection.
airk pause and airk resume change the selected project's native pause policy; they do not stop the shared service or delete data. Some configuration/maintenance operations may be eligible while paused. airk watch on and airk watch off change automatic source watching; disabling watching is not cancellation of already accepted work.
Canceling an active job and canceling a queued job are distinct typed service RPC operations, cancel and cancel_pending. Use the exact observed job ID from airk_status.work and the verified runtime control location. Ask command="service rpc" for transport flags. Do not invent request keys, IDs, preview digests or approval values.
Whole-service start/restart/stop and runtime installation are separate from project pause. Ask command="runtime start" and command="runtime lifecycle" for exact help. Low-level service shutdown is a drain request, not the durable explicit-stop preference. AIRK Explorer > Management provides the corresponding guarded controls if a complete CLI lifecycle request has not been prepared.
For uncertain submission outcomes, recover the exact saved request through service requests rather than creating duplicate work. Ask command="service requests" and topic=request-recovery."#,
    },
    Topic {
        id: "index-health",
        title: "Explaining workspace index health",
        text: r#"Read airk_status for the actual selected collection. Publication identity and file/chunk counts describe committed queryable evidence. Discovery readiness and semantic readiness are independent; lexical evidence may be useful while selected vectors are pending or partial.
Freshness watermarks/timestamps/failures describe observed source intent and indexing completion, not a complete live filesystem proof. Source watcher enabled and native/periodic backend coverage are separate. Check watch.failure, including root_limit or unavailable roots, instead of treating enabled=true as complete coverage.
The work bundle separately reports the sampled active/latest job, phase/counters, queued jobs/truncation, pause state, source watcher, progress sample age and exact-job waits. Check progress_current_worker before interpreting a cached phase as belonging to the serving worker. A successful health response and increasing heartbeat do not prove real work advancement.
Unknown totals remain unknown. A missing applicable wait is not evidence of no wait. A native failure code, stale worker sample, failed_observations entry or long interval without progress calls for refreshed observations and scoped diagnosis; one sample alone does not prove a deadlock or corruption.
Use runtime identities to check that frontend, worker and service versions/builds match when evidence is available. Use topic=job-management to explain CLI actions, topic=troubleshooting for symptoms, and topic=maintenance before proposing repairs or deletion."#,
    },
    Topic {
        id: "overview",
        title: "What AIRK does",
        text: r#"AIRK indexes local code and documents and gives coding agents bounded evidence with original locations and indexed revision identities. It does not reason for the agent or modify original sources.
A per-user native service owns registration, jobs and resource policy. A project worker owns each collection and answers evidence requests directly. A separate local helper handles inference. MCP tools are read-only; CLI and extension controls manage indexing and service lifecycle.
Use airk_status for published index readiness, current runtime identities and sampled work observations. Use airk_help for explanations, not live state. Use topic=topics to browse guidance, topic=commands to list CLI command paths, command="runtime" for exact native command help, or tool="airk_search" for the current MCP schema.
General workflow: install the native runtime, register the workspace and separate index storage, prepare its local model if desired, index, then connect your coding agent. Trusted VSIX activation automates initial registration/model/index work unless automatic setup is disabled.
This help is bundled with the reported native version; it does not inspect local configuration or claim that the connected index is fresh."#,
    },
    Topic {
        id: "topics",
        title: "Help topic catalog",
        text: "Choose a topic ID from topics. Use command for exact CLI help generated from this executable's parser, or tool for a current MCP description and input schema. Follow next_line with the same selector and max_bytes to continue a long answer. Help never executes a command.",
    },
    Topic {
        id: "quickstart",
        title: "Getting started",
        text: r#"Install the matching platform VSIX and trust the workspace. Default activation installs the verified native runtime and terminal command, registers new workspace folders, prepares the bundled offline model and queues indexing with automatic watching. Existing 1.4 registrations, storage choices and watching settings are preserved.
Close old terminal sessions completely, open a new terminal, then run: airk --version
Developer: Reload Window can reconnect terminals with their old environment. External terminal applications may need restarting.
Open AIRK Explorer > Management to inspect service health, progress and index readiness. Connect Coding Agent is a separate explicit step. Use AIRK: Install or Repair CLI when automatic setup reports incomplete installation.
For standalone CLI setup, ask command=init, command=runtime, command=index and command=setup. Replace example paths and collection names with real values before running commands."#,
    },
    Topic {
        id: "cli",
        title: "CLI scope and invocation",
        text: r#"The CLI is a native service client. Public project operations use a registered canonical source root, separate index-data directory and collection. Global selectors include --service-install-dir, --root, --data-dir, --collection and --service-project. Use exact command help for required combinations.
Run airk --help for root command help; append --help to a public command path for its actual flags. airk tools list --json lists MCP tools and schemas without opening a workspace. airk tools help airk_search explains one tool. airk tools call airk_help --args-file help.json returns this guidance offline.
CLI index acceptance is not indexing completion. Read airk_status or AIRK Explorer > Management observations after submitting work. Mutation requests retain exact identities across uncertain outcomes; do not submit new requests merely because a response was lost.
Private worker roles and hidden development transport switches are not ordinary user workflows. The tools console is a collection-bound interface, not a general shell. JSON argument files avoid shell quoting errors."#,
    },
    Topic {
        id: "commands",
        title: "Public CLI command catalog",
        text: "The command paths below are generated from this executable's parser. Ask command with a path such as runtime retire-previous, tools call, usage report or settings to get its exact current help. An empty command or airk selects root help. Hidden worker roles are excluded. These are descriptions only; reading help executes none of them.",
    },
    Topic {
        id: "mcp",
        title: "MCP connections and response modes",
        text: r#"MCP uses stdio JSON-RPC and the official Rust SDK. One connection has an explicit allowlist of registered projects, with a single-project default when configured. Multiple projects require project from projects; UI selection never redirects evidence. The public catalog has 26 unprefixed tools; native CLI evidence aliases retain airk_ prefixes. status reports native index and optional Explorer availability separately. The broker supplies authenticated direct access to its worker; evidence and model inputs are not proxied through the broker. Tools cannot change roots, run CLI commands, index sources or remove collections.
Terse is the default evidence response mode. Full adds detailed provenance, coverage and readiness. Request response_mode=full on an individual call when diagnosing an unexpected result. airk_status and airk_help remain explicit diagnostics in both modes.
Extension version, terminal CLI version, MCP frontend version and selected native payload can differ. airk_status.runtime distinguishes the running frontend, worker and service, including available executable/payload hashes and build receipts. An old MCP process must reconnect to use new tools; changing PATH alone does not replace an existing connection."#,
    },
    Topic {
        id: "tools",
        title: "MCP tool discovery and arguments",
        text: r#"Use airk_help with tool set to an exact tool name to read its actual description and input schema. The public unified catalog has 26 unprefixed names. Native CLI equivalents are airk tools list and airk tools help TOOL, which retain sixteen prefixed evidence aliases. The evidence names below describe those compatibility aliases; use their unprefixed counterparts on the unified MCP connection. Required fields, defaults and supported enum values come from the running tool catalog.
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
        text: r#"Lexical search uses AND terms and indexed text. Vector search is approximate similarity using a provisioned local model. Hybrid combines lexical and vector candidates from one pinned generation. Omitting mode follows the saved collection search policy; explicit vector requests do not silently become lexical results.
Use airk_search mode=lexical for exact terminology. Use vector or hybrid for conceptual questions when vectors are available. path_prefix is a literal collection-relative prefix, not a glob. limit and max_bytes bound returned evidence; check truncation before drawing broad conclusions.
Pending, partial, ready or failed semantic readiness is reported separately from lexical/discovery availability. Vector similarity does not resolve references or prove an answer. Querying does not download models or start enrichment.
Ask tool=airk_search for the current argument schema and command=search for native CLI flags."#,
    },
    Topic {
        id: "indexing",
        title: "Indexing jobs and phases",
        text: r#"Native source work discovers and captures admitted files, extracts retained evidence, prepares changes and publishes bounded batches. Semantic work can continue independently in later embedding batches. A published lexical generation can be usable before all selected semantic artifacts finish.
Captured/extracted files, prepared/committed changes and published batches describe different stages. A missing total remains unknown; do not invent a completion percentage. Job accepted or pending means durable scheduling, not finished indexing.
Use airk_status.work for scoped job, progress, watcher and sampled wait observations, and top-level readiness/publication fields for what is actually queryable. Compare sample age and worker identity before treating cached progress as current. Service health and worker heartbeats are not real-work advancement.
Use airk index for ordinary indexing and AIRK Explorer > Management for pause/resume, queued cancellation or guarded rebuild/repair. Ask command=index and topic=jobs."#,
    },
    Topic {
        id: "jobs",
        title: "Ongoing jobs, queues and waits",
        text: r#"Job records identify kind and state independently of progress. Source reconciliation/indexing, model setup, semantic embedding, inspection, configuration and maintenance can be separate jobs. The queue observation is capped at 16 pending jobs and includes a truncation flag; pending_job identifies its first item.
airk_status.work distinguishes sampled progress from the current worker generation, the latest/active job, bounded queued jobs, project pause policy and available wait metadata. No matching wait observation means there is no applicable sample; it does not prove that work is unstuck or CPU-active.
Compare job ID, service/worker identities and fencing generation. A wait reason belongs only to its matching job. last_advance_age_ms measures time since real progress; unpaused_idle_ms excludes explicit pause. No single elapsed interval proves a deadlock.
CLI management uses typed service RPC and durable request keys. Ask command=service and topic=request-recovery for exact recovery rules. AIRK Explorer > Management exposes reviewed pause/resume/cancel/restart and repair controls."#,
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
        text: r#"The runtime pair separates lightweight airk from airk-embed, which contains the offline MiniLM inference payload. Compatible projects share verified model assets and loaded sessions. Opening a read-only connection does not itself load a model.
Model setup, source discovery and semantic embedding are separate work. Vector opt-out keeps lexical evidence available. An explicit vector query fails honestly when the required model/index is unavailable. Querying does not download or install models.
Custom models require supported local manifests and verified assets. Use the reviewed project configuration path; do not point a workspace setting at an arbitrary executable. AIRK Explorer > Management exposes bundled-model installation and owned helper controls.
Use airk_status semantic_readiness/local_model_ready and runtime/work observations, then airk_capabilities for adapter limits. Ask command=model-catalog and command=configure for current CLI help."#,
    },
    Topic {
        id: "service",
        title: "Native service and process ownership",
        text: r#"One per-user native service hosts independent registered collections. It owns metadata, admission and work scheduling; project workers own indexes and directly serve evidence. The shared inference helper receives model inputs directly from workers.
Health confirms responsiveness and the observed service identity. It does not prove source freshness, complete semantics or ongoing progress. Watcher availability, inference readiness, cached inspection and work observations are independent signals.
On-demand lifecycle differs from optional OS login registration or an organization-managed supervisor. Explicit Stop persists and reconnect cannot undo it. Start is the action that clears the stop preference. Restart/drain controls bind exact observed ownership; never guess a process to kill from its name or PID alone.
Ask command=runtime, command=service, topic=resources and topic=troubleshooting. Reading help performs none of these management actions."#,
    },
    Topic {
        id: "resources",
        title: "Memory, workers and resource policy",
        text: r#"Native resource presets bound resident workers, concurrent work, extraction/inference threads, admitted memory and background I/O. Queued projects can wait for memory or capacity even while service health is responsive.
Reserved memory is admission accounting, not measured RSS. Available host memory does not automatically increase a selected service allowance. Logical CPU count is not the number of extraction or inference threads currently admitted.
Use AIRK Explorer > Management resource observations and sampled job waits. Changing policy should use native reviewed controls or typed service RPC, not retired editor scheduler settings. Preserve worker/project isolation and explicit memory-pressure/failure signals.
Ask command=service for the current RPC interface and topic=jobs to interpret waits. Help does not sample resources or modify a preset."#,
    },
    Topic {
        id: "installation",
        title: "Installation and terminal command",
        text: r#"The VSIX verifies its platform runtime payload and installs/selects the native runtime through the native installer. The terminal command forwards to the stable approved launcher. Native source and extension versions are synchronized for the candidate.
Installation removes obsolete extension-managed CLI executables independent of their old version. It also retires the previous user-owned native runtime and clears rollback after successful replacement activation. Locked files report incomplete cleanup; disconnect clients holding them and use Install or Repair CLI or retry at the next trusted activation.
Close terminals completely and create new ones after PATH changes. Window reload can preserve their environment. On Windows, where.exe airk shows command paths; in PowerShell, Get-Command airk -All also shows aliases/functions. airk --version identifies that terminal invocation, not an already running MCP connection.
Managed provider payloads remain provider-owned. Original sources, indexes and saved settings are not executable-cleanup targets. Ask command=runtime and topic=upgrades."#,
    },
    Topic {
        id: "upgrades",
        title: "Replacement, rollback and legacy transition",
        text: r#"Compatible automatic 1.4 updates use fenced native staging/activation and preserve existing registrations, storage and watching choices. Selection changes only after exact candidate health/readiness succeeds; failed activation preserves or restores the predecessor.
Successful VSIX installation cleanup removes the previous user-owned native payload and its rollback reference. Standalone native activation retains rollback until explicit retirement. Deferred cleanup keeps its exact saved identity and must complete before another activation. Unknown or modified files are preserved and reported.
1.4 uses a fresh index namespace rather than converting live 1.3 stores, jobs or revision tokens. Old agent entries and OS startup registrations require separate review. Executable cleanup is not permission to delete old index data.
Managed runtimes remain under provider control. Ask command=runtime retire-previous, command=runtime rollback, topic=request-recovery and topic=storage for current details."#,
    },
    Topic {
        id: "configuration",
        title: "Project configuration and guard checks",
        text: r#"Project settings use native preparation, exact incarnation/settings guards and durable job receipts. Supported changes include model/vectors, extraction detail/policy, vector selection, source roots, scan limits, usage and shared quota policy. Use command=settings for available views and command=configure for the bounded draft input interface.
Native configuration can be asynchronous. A submitted/accepted job is not completed configuration. Keep the last configuration result separate from later indexing progress; reinspection is required when effect is uncertain or a reviewed guard changed.
Extension user/machine runtime and storage settings ignore repository overrides. Existing registered storage and watching preferences remain native selections. Choosing new default storage does not silently move or delete existing index files.
Do not invent JSON draft fields. Use exact CLI help, AIRK Explorer > Management reviewed project controls and the version-matched configuration contract when constructing advanced requests."#,
    },
    Topic {
        id: "storage",
        title: "Index storage, isolation and quotas",
        text: r#"Each collection has a canonical source and independently owned derived storage. Sources cannot overlap registered data, control, runtime-state or bound logs. Original source files are read-only.
Native quotas account for current and retained objects, temporary/replacement admission and shared data-root usage. Limits can block publication even when host memory is available. A quota failure is different from a source-watcher root_limit warning.
Changing a default index-storage setting does not move an existing collection. Reviewed storage replacement preserves previous index files for separate unregister/cleanup. Unregister stops new admission and drains the owner while preserving sources, settings and index data.
Index deletion requires its own exact native cleanup review. Do not recursively delete a shared AIRK directory to fix a single project. Ask topic=maintenance and command=settings."#,
    },
    Topic {
        id: "maintenance",
        title: "Repair, rebuild and deletion",
        text: r#"Repair planning inspects scoped state before choosing actions. Native plans retain exact identities, guarded previews and per-project outcomes; all required preview pages must be reviewed before execution. Partial failure and cancellation are distinct from successful project receipts.
Ordinary index requests are not full rebuilds. Full rebuilds and destructive derived-data cleanup require guarded native previews and separate approval. The public index --full shortcut refuses unreviewed rebuilding.
Unregister preserves sources, saved settings and index files. Cleanup is a separate operation on owner-verified derived state. Payload repair preserves damaged slots according to its native inventory/ownership rules. Unknown files are not silently removed.
Use AIRK Explorer > Management Diagnose & Repair or exact typed CLI management interfaces. Ask command=service, command=runtime repair-payload and topic=request-recovery. Help does not authorize or execute repairs."#,
    },
    Topic {
        id: "agent-setup",
        title: "Connecting a coding agent",
        text: r#"Connect Coding Agent creates a stable-launcher configuration for an existing registered project. CLI setup can print a configuration for a selected supported client; ask command=setup for the actual enum choices and required scope.
Registration/indexing and agent connection are separate. A successful extension verification checks its own bounded MCP connection, not the external agent's connection. Use the agent's MCP controls to enable/reconnect the single airk connection and verify status from that agent. Restart after startup migration of recognized earlier paired entries and approve renamed tools through the client's controls.
Recognized legacy JSON configurations have guarded reviewed replacement; unrelated client settings are preserved. TOML and other formats can require manual merging. Changing a terminal PATH or reloading an editor does not replace an agent's already running MCP process.
Use the runtime bundle to distinguish frontend/service/worker identities and payloads. Never paste connection grants or private control credentials into user-visible configuration or reports."#,
    },
    Topic {
        id: "request-recovery",
        title: "Unknown outcomes and exact retries",
        text: r#"AIRK management requests retain durable caller/key/scope identities before submission. A lost response can leave Outcome unknown even when native work was accepted or completed. Recheck/retry the exact saved request instead of creating new work with a different key.
Forgetting an extension-side receipt does not cancel native work. Canceling queued work, pausing a project and stopping the service are different operations. Native completed-request replay returns retained effects rather than repeating them.
Saved activation and executable retirement have separate native recovery journals. Changed selections, expired epochs, stale previews and unproven ownership are explicit refusals. Reinspect rather than editing journal files or weakening guards.
Use AIRK Explorer > Management request-recovery controls and exact CLI help for the affected command. Ask topic=jobs, command=index and command=runtime."#,
    },
    Topic {
        id: "troubleshooting",
        title: "Diagnosing common symptoms",
        text: r#"Start with airk_status: distinguish service/runtime identity, published index readiness, semantic coverage, source freshness and work observations. Refresh observations before comparing counters. Service health alone is not indexing progress.
Not observed means a value is unavailable, not automatically failed. A queued model_setup job can coexist with running source work. Missing totals do not justify a percentage. No matching native wait observation is a missing applicable sample, not proof of no wait.
root_limit is a source-watcher traversal/resource limit, not proof of full disk. Watching enabled does not prove full native watch coverage. Pending source changes is a source-intent revision gap, not necessarily a count of modified files.
For an old terminal version, close/recreate the terminal and inspect command resolution. For an old MCP connection, reconnect the agent separately. For locked old binaries, disconnect their clients before retrying installation cleanup.
Use logs, independent inspection and native repair previews when observations remain inconsistent. Preserve original sources and unknown files; do not guess process ownership or delete shared state."#,
    },
    Topic {
        id: "privacy",
        title: "Privacy and read-only boundaries",
        text: r#"AIRK indexes local sources and serves indexed excerpts to the calling coding agent. It does not add remote inference, cloud storage or external databases. The agent's own handling of returned evidence is outside AIRK's local boundary.
The broker carries metadata, not retrieved source excerpts, query text, embeddings or model weights. Evidence travels directly between clients and workers; model inputs travel directly to the local helper.
MCP tools are read-only. Native CLI/extension management can change derived state and lifecycle only through their own guarded controls. Sources remain separate and read-only. Diagnostic exports are metadata-only, previewed, redacted by default and not uploaded by AIRK.
Runtime status omits executable paths and credentials. Help returns bundled guidance and generated command/tool descriptions; it does not inspect sources, execute examples or reveal bearer grants."#,
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
Use command=usage to discover report/policy/export options and command=configure for guarded policy changes. JSON, CSV and Markdown native reports are bounded and can require continuation. Shared quota limits are separate from usage/savings reporting."#,
    },
    Topic {
        id: "versions",
        title: "Versions, builds and runtime diagnostics",
        text: r#"The extension version describes the VSIX. airk --version describes the command selected by that terminal. The MCP frontend, service and worker have their own running process identities. Changing one does not automatically reconnect every other process.
airk_status.runtime reports compiled versions, protocols, PIDs, exact service/worker identities, executable hashes, payload hashes and inventory-bound build receipts when available. Build information includes source fingerprint/file count, build profile and rustc. Frontend identity is explicitly client-reported; service health is bound to the worker's service instance.
versions_match, payloads_match and builds_match are nullable comparisons. Missing evidence is not a match. Bare development binaries may lack build receipts. Extractor version, semantic pipeline identity and publication revision describe indexed evidence, not the native executable version.
Use tool=airk_status for the complete tool description and command=runtime diagnose for bootstrap diagnostics."#,
    },
    Topic {
        id: "architecture",
        title: "Components and data flow",
        text: r#"Core defines contracts and identities. Ingestion discovers/extracts admitted sources. Store owns derived state, publication, quotas and durable receipts. Retrieval owns local vector primitives. Kernel coordinates local indexing/retrieval. MCP owns schemas and transport. CLI owns native process, IPC, installation and management adapters; the extension is a trusted service client.
Dependency direction is preserved: kernel/MCP do not depend on the CLI. Native host metadata is supplied to MCP before rendering/accounting; kernel index status remains collection evidence. Help is bundled read-only guidance and command metadata, not an agent reasoning engine.
Independent collections share one per-user service but preserve root/data/revision isolation. The broker does not forward evidence or model inputs. Native operation targets Windows, macOS and Linux; available local tests do not by themselves qualify every platform or policy environment."#,
    },
    Topic {
        id: "workflows",
        title: "Common user workflows",
        text: r#"Find a definition: discover paths, request an outline or exact symbol/code fact, then read a bounded cited excerpt. Explain behavior: search one component, expand hits, check call candidates and verify live source before edits.
Check indexing: read airk_status, separate published readiness from work progress, compare sample age/worker/job identity, then inspect failures/waits. Use AIRK Explorer > Management or typed CLI management for an intentional pause, cancellation or scoped repair.
Manage installation: verify terminal resolution and runtime identities, reconnect agents separately, review ownership and retry deferred cleanup after old clients release files. Preserve project data and sources.
Ask airk_help for a topic, an exact command path or tool schema before constructing management commands. Help supplies explanations and examples; the agent must still use actual observed paths/IDs and the required native approval guards."#,
    },
    Topic {
        id: "glossary",
        title: "Terminology",
        text: r#"Collection: independently owned indexed evidence selected by workspace configuration. Project binding: opaque service identity for canonical source/storage/collection settings. Publication token: exact indexed revision identity, including incarnation.
Service/broker: metadata, registration, resource and work owner. Worker: process owning a project's index and serving evidence. Inference helper: local model process shared by compatible projects. Fencing generation: ownership generation preventing stale workers from acting as current owners.
Discovery readiness: committed extraction/index inventory. Semantic readiness: completion of selected semantic work. Freshness: observed source-intent/indexing watermarks, not live-file proof. Watcher: native notifications and/or periodic source reconciliation.
Runtime payload: approved inventory of executable pair and required files. Stable launcher: installed command following verified selection. Build receipt: bounded source/toolchain/profile metadata. Retained receipt: durable request/activation/cleanup outcome. Unknown outcome: submission may have had effects despite a missing acknowledgment."#,
    },
];
