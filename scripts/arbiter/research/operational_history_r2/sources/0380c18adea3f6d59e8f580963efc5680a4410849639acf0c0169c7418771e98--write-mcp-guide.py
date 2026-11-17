# SPDX-License-Identifier: Apache-2.0
import json
from pathlib import Path

root = Path.cwd()
native = '../crates/airk-mcp/src/lib.rs'
unified = '../crates/airk-mcp/src/unified.rs'
discovery = '../crates/airk-kernel/src/discovery.rs'
conditional = '../crates/airk-kernel/src/current/conditional.rs'
kernel = '../crates/airk-kernel/src/lib.rs'
assist = '../packages/vscode/src/explorerAssistance/tools.ts'
assist_tests = '../packages/vscode/src/test/explorerAssistanceTools.test.ts'
inspection = '../crates/airk-kernel/tests/integration/current_inspection.rs'
code_tests = '../crates/airk-kernel/tests/integration/code_graph.rs'
rows = []
def add(name, why, how, inputs, result, example, impl, tests):
    rows.append(dict(name=name, why=why, how=how, inputs=inputs, result=result, example=example, impl=impl, tests=tests))

add('projects', 'Choose an explicitly configured source scope before reading evidence in a multi-root connection. This prevents an editor selection from silently redirecting an agent.',
    'The unified adapter enumerates its configured project map and default selector. It neither scans all machine registrations nor changes the current project.',
    'No arguments. Reuse a returned project selector only on the connection that advertised it.',
    '`default_project` and configured `projects` entries. These connection selectors are distinct from native registry binding identities.', {}, unified, unified)
add('status', 'Answer the inexpensive question: what publication and readiness does this configured project currently expose?',
    'The frontend prepares the selected runtime, then obtains bounded scalar metadata. Successful observations can be reused for five seconds with their original observation time and cache age. The cold preparation allowance is ten seconds; established observation has a separate two-second allowance.',
    'Optional configured `project`, `response_mode` and `include_status`. It does not accept a request to run diagnostics.',
    'Version, index state, counts, publication, freshness and semantic readiness. A successful envelope can still report unavailable or unknown evidence. Require `index_state: available` and a publication; never turn a timeout into proof that the index is unavailable.',
    {'response_mode':'full'}, '../crates/airk-cli/src/status_observation.rs', unified)
add('diagnostics', 'Investigate an unavailable, stale, slow or incorrectly attached project after a lightweight status check is insufficient.',
    'The native adapter gathers explicitly requested runtime/process, worker/job, queue, watcher and sampled-wait observations. The unified adapter can add optional Explorer attachment information. Each observation retains its own availability and age.',
    'Optional configured `project` and response options. Request explicitly; do not poll it as an automatic substitute for status.',
    'Detailed coverage and runtime/work observations, including missing/failed observations and optional editor binding/revisions. It does not start indexing, repair storage or prove that an old progress sample is advancing.',
    {'response_mode':'full'}, '../crates/airk-cli/src/runtime_metadata.rs', '../crates/airk-mcp/src/local.rs')
add('capabilities', 'Determine what extraction formats and explicitly provisioned local adapters can support before choosing a retrieval approach.',
    'The native tool reports supported formats, provider identities and limits, with an optional exact adapter profile. Capability information is separate from a collection\'s current model, OCR or index readiness.',
    '`adapter` selects a returned adapter ID; `rich` expands profiles. Response options and configured project selection are available.',
    'Capability/profile metadata and limitations, including provider provenance where available. A listed format or adapter is not evidence that every file was extracted successfully.',
    {'rich':True,'response_mode':'full'}, native, '../crates/airk-mcp/src/local.rs')
add('search', 'Discover relevant code and documents together when the exact source path or declaration is not yet known.',
    'The kernel selects a current indexed head, applies a literal path scope and performs lexical, vector or hybrid retrieval. It materializes bounded, source-bound evidence at original locations. Local model/index readiness governs vector modes.',
    '`query`; optional `mode`, `path_prefix`, `limit`, `max_bytes`, `full_chunks` and response options. Lexical terms are ANDed. Omitted mode follows saved collection policy; an explicit mode does not silently mean another mode.',
    'Ranked hits, citations, exact publication and truncation/coverage information. Focused snippets are the default; `full_chunks` changes returned text, not the indexed source. Hybrid results while semantic work is partial do not prove complete vector coverage.',
    {'query':'retry budget','mode':'lexical','path_prefix':'src/','limit':5,'max_bytes':8192}, '../crates/airk-kernel/src/current/search.rs', '../crates/airk-kernel/tests/integration/persistent.rs')
add('list_files', 'Discover exact indexed paths before guessing filenames or requesting broad file reads.',
    'The inventory reader pages the published snapshot under count and encoded-byte budgets. It does not walk the live filesystem.',
    '`prefix` is a literal collection-relative prefix, not a glob. Keep it, response options and page budgets stable when consuming a returned `cursor`.',
    'Indexed paths, publication, bounded continuation and truncation information. Inventory is not a complete live-workspace listing or proof of extraction coverage.',
    {'prefix':'src/','limit':20,'max_bytes':8192}, discovery, inspection)
add('file_info', 'Establish the hash, extraction method and coordinate basis before interpreting a hit or issuing conditional reads.',
    'The current conditional metadata reader returns the retained file/profile identity. Optional validators bind the indexed representation; it never hashes the live editor buffer for you.',
    'Exact `path`; optional `artifact_id`, current `revision`, `known_hash`, `if_evidence` and conservative `follow_moves`, plus response options.',
    'Language/profile, parse/extraction state, source hash and indexed metadata, or an explicit conditional/no-content outcome. A source hash identifies saved indexed bytes, not live-file freshness.',
    {'path':'src/client.rs','response_mode':'full'}, conditional, inspection)
add('read_file', 'Read the smallest retained evidence range needed to verify or extend a search result, preserving its original citation.',
    'The reader materializes a bounded current indexed range and representation validator. Native-source line coordinates differ from normalized extracted-document text coordinates.',
    '`path`, 1-based inclusive `start_line` and `end_line`; optional current `revision`, `known_hash`, `if_evidence`, move handling and `max_bytes`.',
    'Cited text and a scoped read outcome/validator. Missing, moved, ambiguous, unchanged and expired evidence are explicit; a superseded token does not authorize replacement with today\'s live bytes.',
    {'path':'src/client.rs','start_line':1,'end_line':20,'max_bytes':8192}, conditional, '../crates/airk-kernel/tests/integration/file_continuity_v2.rs')
add('context', 'Expand around a known hit without loading an entire source file or losing its citation.',
    'The range reader selects neighboring indexed lines around one coordinate and applies the same source/representation identity rules as conditional file reads.',
    '`path`, `line`, `before`, `after`, `max_bytes`; optional exact current revision/hash/validator and move handling.',
    'Bounded cited neighboring text or a typed conditional outcome. For PDF/DOCX/PPTX, stay within the intended original page/block/slide and interpret extracted coordinates using file metadata.',
    {'path':'src/client.rs','line':40,'before':6,'after':10,'max_bytes':8192}, conditional, '../crates/airk-kernel/tests/integration/header_context_boundaries_v2.rs')
add('outline', 'Understand one file\'s structure before spending a large text budget on it.',
    'The discovery/conditional reader selects definitions, imports or structured document elements from the retained extraction, preserving locations, order and continuation.',
    'Exact `path`; optional `compact`, `limit`, `max_bytes`, `cursor`, current revision/hash/validator and response options.',
    'A bounded outline. `compact: false` requests richer facts and IDs; it is independent of response mode. Partial syntax or extraction remains partial, not resolved types or runtime behavior.',
    {'path':'src/client.rs','compact':False,'limit':20,'max_bytes':8192}, discovery, inspection)
add('symbols', 'Locate a written definition name directly rather than relying on broad text search.',
    'The discovery reader selects grammar-derived definitions from indexed code facts. A requested language/profile must match the index\'s vocabulary.',
    'Optional exact `name`, `language`, `path_prefix`, `compact`, budgets and continuation. Omit name to list definitions in a bounded scope.',
    'Definition signatures and locations; rich output adds IDs/details usable in later inspection. Equal written names can have multiple candidates. This is not a cross-language name resolver.',
    {'name':'Owner','path_prefix':'src/','compact':False,'limit':10}, discovery, code_tests)
add('imports', 'Inspect written static dependencies while keeping them distinct from transitive or runtime dependency resolution.',
    'The discovery reader selects retained import/use/include declarations with their source locations.',
    'Optional name substring, `language`, `path_prefix`, `compact`, budgets and cursor.',
    'Bounded import declarations and provenance. Absence does not prove that a dynamic dependency does not exist.',
    {'path_prefix':'src/','name':'serde','limit':20}, discovery, code_tests)
add('code_facts', 'Inspect richer declarations, written calls and other structural evidence, or reselect one remembered durable code identity.',
    'The kernel queries retained syntax facts and optional source-hash-bound producer assertions. Coverage and producer basis distinguish written candidates from imported assertions.',
    'Exact `name`/qualified name/symbol ID and optional kind/language/path filters, or one durable `symbol_key` without textual filters. `compact`, budgets and cursor control representation/paging.',
    'Fact records with locations, signatures, scope and provenance. Syntax does not resolve a call target; an imported SCIP assertion is evidence from its producer, not a runtime call observation.',
    {'name':'Owner','kind':'definition','path_prefix':'src/','compact':False,'limit':10}, discovery, code_tests)
add('document_facts', 'Find written requirements, decisions, dates and structured document elements without treating prose retrieval as an inferred obligation.',
    'The document-fact reader selects extracted statements/elements, preserving original page, block, row, cell or slide locations and extraction qualifiers.',
    'Optional `kind`, name/statement substring, language/profile and literal path prefix; alternatively a durable `document_key` alone. Supports compact output, budgets and cursor.',
    'Cited statements and structured facts. Modality, headings and original locations are evidence to inspect; OCR and document extraction retain their stated limitations.',
    {'kind':'requirement','name':'retry','path_prefix':'docs/','limit':10}, discovery, '../crates/airk-kernel/tests/integration/instruction_documents_v2.rs')
add('references', 'Collect bounded exact identifier mentions for impact investigation, including strings/comments that a structural query could omit.',
    'Indexed lexical candidates are verified against retained source for exact-case whole-identifier occurrences, including CamelCase handling.',
    'Exact `name`, optional literal `path_prefix`, `limit`, `max_bytes` and response options.',
    'Textual occurrences with locations. Check truncation before impact analysis; these are not semantically resolved references or executed test coverage.',
    {'name':'Owner','path_prefix':'src/','limit':20,'max_bytes':8192}, kernel, code_tests)
add('relationships', 'Investigate supported incoming/outgoing structure around a known symbol with explicit depth and evidence limits.',
    'The kernel traverses indexed structural edges or source-bound imported assertions under the current revision. Written target names remain candidates unless stronger producer evidence is present.',
    '`symbol` from returned evidence, direction incoming/outgoing, optional kind, literal path scope, `hops` from 1 to 3 and output budgets.',
    'Bounded related nodes/edges with their basis and coverage. Missing edges do not prove no impact; no complete call graph or runtime execution claim follows.',
    {'symbol':'<returned-symbol-id>','direction':'incoming','kind':'call','hops':1,'limit':20}, kernel, code_tests)
add('changes', 'Reuse remembered evidence safely and learn what the index observed changing since an exact baseline.',
    'The kernel validates remembered selectors or pages bounded retained changes. Independent coverage floors, publication identity and retained metadata govern what can be answered.',
    '`since` and/or typed path/artifact/symbol/document/citation selectors; optional literal `path_prefix`, cursor, inclusion flags and budgets. Consume every needed page before advancing the baseline.',
    'Added/modified/removed observations, selector validity and explicit unavailable/history outcomes. It is not arbitrary historical search, a Git diff or proof of a filesystem move. Do not replay an expired token as a live-source read.',
    {'since':'<returned-publication-token>','path_prefix':'src/','limit':10,'max_bytes':8192}, '../crates/airk-kernel/src/current/changes.rs', '../crates/airk-kernel/tests/integration/changes_v2.rs')
add('parse', 'Share bounded evidence recipes between Workbench, MCP and CLI, instead of implementing an agent or unbounded tool orchestrator in the kernel.',
    'The native Workbench runner resolves text, a named recipe or explicit steps into a finite plan, then optionally executes eligible read-only steps with publication, scope, call/fanout/byte/time budgets.',
    '`schema: airk.parse.request/1`; exactly one text/recipe/plan; mode prepare/execute. Preparation is the default. Explicit plan execution must retain required choices and exact revision identity.',
    'A prepared/needs-input/completed result with plan tier, per-step outcomes, citations and completeness. Finite supported parsing can execute when requested; learned proposals remain review-only until explicit plan submission. It never edits source or executes tests.',
    {'schema':'airk.parse.request/1','text':'Find the definition of Owner','mode':'prepare'}, '../crates/airk-kernel/src/workbench.rs', '../crates/airk-mcp/src/lib.rs')
add('help', 'Explain AIRK itself and discover the installed schemas without confusing product guidance with evidence from the user\'s repository.',
    'The unified adapter combines native topics/generated CLI help/public schemas with optional bundled UI guide search. Native help does not require an indexed publication; UI guidance requires the verified bundled adapter, not an open editor window.',
    'Omit selectors for overview; use `topic`, `command`, `tool`, `query` or `topic_id` according to the advertised schema. Native line/byte continuation and UI guide detail options differ.',
    'Documentation and schemas. It returns neither project search evidence nor current runtime health; use search/status/diagnostics for those questions.',
    {'tool':'search'}, '../crates/airk-mcp/src/help.rs', '../packages/vscode/src/test/explorerHelp.test.ts')
add('list_templates', 'Discover installed strategy IDs and availability before constructing a Studio proposal.',
    'The optional Assistance adapter pages the bundled core pack without executing operators or opening a project index.',
    '`offset`, `limit` and `max_bytes`. This catalog call does not take a project selector.',
    'Template IDs, versions, purposes, availability and `next_offset`. Preview availability is not execution qualification.',
    {'offset':0,'limit':5,'max_bytes':8192}, assist, assist_tests)
add('get_template', 'Read the exact parameters, defaults and operator contracts of one installed strategy before filling it in.',
    'The Assistance adapter looks up a known ID in the bundled core pack and returns its complete definition and referenced operator specifications.',
    'Exact `template_id` returned by list_templates; no project selector.',
    'One inert template definition, budgets/instructions and runnable/preview information. Unknown IDs fail; this call does not execute a strategy.',
    {'template_id':'airk.core/bug'}, assist, assist_tests)
add('draft_search', 'Hand a proposed query to the user\'s Search form without executing it or changing collection policy.',
    'The adapter schema-validates the envelope, then a matching trusted Explorer window revalidates binding/revision and admits the suggestion to its Inbox.',
    '`schema_version: 1.0`, idempotency key, title and nested `draft` query/mode/scope/budgets; optional workspace/target/evidence guards and project selector.',
    'Proposal receipt and review state, never search hits. The user chooses whether to apply and run it. Retrying an idempotency key requires the identical body.',
    {'schema_version':'1.0','idempotency_key':'guide-search-001','title':'Inspect retry handling','draft':{'query':'retry','mode':'lexical','path_prefix':'src/'}}, assist, '../packages/vscode/src/test/explorerAssistanceHost.test.ts')
add('draft_strategy', 'Offer a reproducible Studio investigation recipe while retaining human control over execution.',
    'The adapter validates a core template or supported finite operators/bindings and queues an inert Studio draft for a trusted window. Kernel/store do not depend on this editor adapter.',
    'Versioned/idempotent title envelope; template ID and parameters and/or bounded steps using supported operators. Capture binding/target revisions from diagnostics when guarding a concrete window.',
    'Inbox proposal receipt. Applying fills a Studio draft; it neither executes the strategy nor saves it to a source folder.',
    {'schema_version':'1.0','idempotency_key':'guide-strategy-001','title':'Collect retry evidence','steps':[{'id':'find','op':'airk.search','args':{'query':'retry','mode':'lexical'}}]}, assist, assist_tests)
add('draft_plan', 'Transfer a proposed task breakdown to Planner without writing a plan file or delegating work to agents.',
    'The adapter validates bounded task IDs, dependencies and text, then the attached trusted window offers them in the Suggestions Inbox.',
    'Versioned/idempotent envelope and tasks such as T1/T2; optional objective, acceptance, instructions and relative path hint. There is no save field.',
    'Proposal receipt. The user must apply and separately save any plan; no task or command is run.',
    {'schema_version':'1.0','idempotency_key':'guide-plan-001','title':'Review retry behavior','tasks':[{'task_id':'T1','title':'Inspect cited source','acceptance':'Verify the live implementation against indexed evidence.'}]}, assist, assist_tests)
add('suggest_view', 'Direct a user to the appropriate Navigation, Review or Management screen while leaving actions under user control.',
    'The unified wrapper chooses a nested schema by kind, then routes it to the same guarded proposal mechanism as other drafts.',
    '`kind` is navigation/review/management; `draft` contains the matching versioned/idempotent envelope and route, review scope or fixed management intent.',
    'Inbox proposal receipt. Management guidance does not start indexing, repair, tests or service actions. Applied means the user copied the suggestion into a screen/draft, not that an operation executed.',
    {'kind':'navigation','draft':{'schema_version':'1.0','idempotency_key':'guide-view-001','title':'Inspect definitions','route':'code/symbols','filters':{'name':'Owner'}}}, unified, '../packages/vscode/src/test/explorerAssistanceHost.test.ts')
add('validate_draft', 'Check a complete proposed tool argument object before offering it to the Inbox, without consuming proposal admission or idempotency state.',
    'The same strict validator runs offline. If a trusted target is attached, the adapter can additionally check binding, target revision and idempotency conflicts.',
    '`kind` and `draft`, where draft is the complete corresponding proposal arguments including its envelope, not merely its inner Search/Studio form.',
    '`valid`, errors and live-check availability, always `admitted: false` and `executed: false`. Offline validity does not establish that a particular live window will accept it.',
    {'kind':'search','draft':{'schema_version':'1.0','idempotency_key':'guide-validate-001','title':'Inspect retry handling','draft':{'query':'retry','mode':'lexical'}}}, assist, assist_tests)
add('proposal_status', 'Distinguish pending, rejected, expired or applied UI suggestions instead of assuming that a submitted form was acted on.',
    'The matching trusted window looks up the opaque proposal receipt and returns bounded review metadata without draft contents.',
    '`proposal_id` from a real proposal receipt and optional configured project.',
    'Review state, reason, admitted/current/applied target revisions and expiry. `executed` remains false; applying a proposal does not establish source changes, execution or a saved file.',
    {'proposal_id':'<returned-proposal-id>'}, assist, '../packages/vscode/src/test/explorerAssistanceHost.test.ts')
add('get_handoff', 'Retrieve only a complete Studio prompt or Planner task the user explicitly chose to share, instead of reading arbitrary editor drafts.',
    'The adapter resolves an expiring opaque share token against the matching trusted window. Draft, binding or evidence-publication changes revoke the share.',
    '`token` supplied by the user, plus configured project where needed. Tokens are capabilities; do not fabricate or publish them.',
    'The explicitly shared text, SHA-256, draft revision, workspace binding and evidence publication where present. It does not save, modify or execute the shared material.',
    {'token':'<user-issued-share-token>'}, '../packages/vscode/src/explorerHandoffs.ts', '../packages/vscode/src/test/explorerAssistanceHost.test.ts')

assert len(rows) == 28 and len({r['name'] for r in rows}) == 28
text = ['# AIRK MCP developer guide', '',
    'This guide explains **all 28 public tools in AIRK 1.8.2**: why each exists, when to choose it, how it works, its inputs/results, a request example, and the owning implementation/tests. It complements the [catalog and connection contract](unified-mcp.md), [native protocol/evidence contract](mcp.md), [response modes](mcp-response-modes.md) and [CLI developer guide](cli-developer-guide.md).', '',
    '## Start with the right layer', '',
    'The CLI composes the unified MCP frontend. Native schemas and rendering belong to `airk-mcp`; read behavior belongs to `airk-kernel` and retained storage to `airk-store`. Optional Explorer Assistance supplies bundled guidance, templates and guarded UI proposals. Kernel/store do not depend on the editor. Preserve the [architecture dependency direction](architecture.md) when changing a tool.', '',
    '| Need | Tools |', '|---|---|',
    '| Select a configured scope and inspect readiness | projects, status; diagnostics only when details are needed |',
    '| Discover evidence and exact paths | search, list_files, capabilities |',
    '| Read and interpret a specific retained source | file_info, read_file, context, outline |',
    '| Inspect written code/document structure | symbols, imports, code_facts, document_facts, references, relationships |',
    '| Revalidate remembered evidence | changes and conditional read validators |',
    '| Prepare or run a bounded native evidence recipe | parse |',
    '| Learn AIRK or construct an editor proposal | help, list_templates, get_template, draft_search, draft_strategy, draft_plan, suggest_view, validate_draft, proposal_status, get_handoff |', '',
    'Use `tools/list` on the actual MCP connection as the authoritative public catalog. Client namespaces such as `mcp__airk__` are client presentation, not part of these tool names. The terminal\'s `airk tools list --json` exposes the **18 native prefixed aliases**, not the complete 28-tool unified/Assistance catalog. [CLI Explorer commands](cli-developer-guide.md#explorer-proposals-and-shared-material) have separate offline-file behavior.', '',
    '## Shared request and evidence rules', '',
    '- In a multi-root connection, obtain selectors from projects and pass `project` only where the schema accepts it. Explorer UI selection never changes retrieval scope. A returned configured selector and the native service registry binding are different identity roles.',
    '- Native evidence is an indexed snapshot. Preserve exact publication tokens, content hashes, validators and original locations. Verify relevant live bytes before editing; neither a successful tool call nor a matching indexed hash proves live-source freshness.',
    '- Use literal relative prefixes where supported. `path` selects an exact indexed path. Scope before raising budgets. `limit`, `max_bytes`, compactness and pagination have per-tool contracts; a small response is not proof that internal work is constant.',
    '- Terse is the default where response options exist. `response_mode: full` adds explicit provenance/coverage; `include_status` repeats current observations. `compact: false` controls record richness separately. Do not add these keys to template/proposal calls whose schemas omit them.',
    '- Repeat continuation requests with the same filters and representation. Current-only revision tokens/cursors expire when superseded. Finish every needed changes page before advancing its baseline; independent coverage floors can make remembered history unavailable.',
    '- Typed busy/retry responses must be honored exactly as returned. Retry unchanged only where the receipt explicitly says the query was not submitted or permits that exact read retry. Transport failure after submission is uncertain; do not blindly replay a plan or operation.',
    '- Native evidence tools do not write project sources or expose management commands. Normal connection, cache, usage-accounting and owned service lifecycle metadata can still be maintained. UI proposals are separately marked as drafts and require user review.', '',
    'Examples below are **tool argument objects**, not executed calls. Source paths/names are fictional. Replace angle-bracket placeholders only with actual returned tokens or IDs; they are not valid synthetic capabilities. Draft examples illustrate schema shape and must not be posted to a live window merely to test documentation.', '',
    '## Tool index', '']
text += [f'- [{r["name"]}](#{r["name"]})' for r in rows]
for row in rows:
    text += ['', f'## {row["name"]}', '', f'**Why and when:** {row["why"]}', '', f'**How it works:** {row["how"]}', '', f'**Inputs:** {row["inputs"]}', '', f'**Result and limits:** {row["result"]}', '', '```json', json.dumps(row['example'], ensure_ascii=False, indent=2), '```', '', f'**Implementation:** [{Path(row["impl"]).name}]({row["impl"]}). **Contract/regression evidence:** [{Path(row["tests"]).name}]({row["tests"]}).']
text += ['', '## Workflows and maintenance', '',
    'For a code investigation: projects → status → scoped search → file_info/outline → bounded read_file/context → exact symbols/facts/references as needed. Preserve the response identities rather than combining unrelated revisions into one asserted result. Use document_facts for written requirements and relationships for qualified structural candidates.', '',
    'For a change check: retain the publication and selectors, call changes, consume its bounded continuation, then decide whether to recollect evidence. A history-unavailable result is an explicit gap, not permission to reconstruct history from live files.', '',
    'For a UI proposal: read the appropriate template/help, explicitly inspect diagnostics if a current window binding/revision is needed, validate the complete argument object, submit once with a stable idempotency key, then inspect proposal_status. The user still applies/runs/saves it. For handoff, wait for a user-created share token.', '',
    'When modifying a tool, read its schema, dispatcher, kernel/adapter contract and linked tests first. Preserve exact scopes, request/response identity, source read-only behavior, original locations, validators, budgets and failure distinctions. Update this guide and the catalog/native contracts together, and verify current names against the advertised catalog. Native catalog/argument tests live in [MCP lib.rs](../crates/airk-mcp/src/lib.rs), unified catalog/alias/deadline tests in [unified.rs](../crates/airk-mcp/src/unified.rs), and native transport/client qualification in [check-service-clients.py](../scripts/check-service-clients.py). A linked test is evidence for its asserted contract, not a claim of complete platform or language qualification.', '']
(root / 'docs/mcp-developer-guide.md').write_text('\n'.join(text), encoding='utf-8')
(root / 'artifacts/performance-1.8.2-20261101/mcp-doc-inventory.json').write_text(json.dumps(rows, indent=2), encoding='utf-8')
