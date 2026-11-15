// SPDX-License-Identifier: Apache-2.0
// Guided presentation over existing I3 catalog, scope and canonical read owners.
import type { JSX } from 'preact';
import { useEffect, useRef, useState } from 'preact/hooks';
import { call, type CallResult } from './api';
import { body, isCatalogId, isDigest, isIncarnation, OPS, type CatalogKind, type Operation } from './operations';
import { itemOf, type Item, type CatalogHeader } from './catalogRequests';
import { Field, JsonBlock, NumberField, Outcome, Panel, Text, type CallState } from './render';
import { currentSession } from './session';
import { ConcurrencyNote, useQueryConcurrency } from './concurrencyNote';
import { concurrencyNote } from './scopeConcurrency';
import manifest from '../../ui/generated/help-manifest.json';
import { helpToolPresentation, renderHelpNode } from '../../ui/src/helpToolPresentation';
import { h } from 'preact';
import { WORKBENCH_CONTEXT_BUDGET } from '../../vscode/src/workbenchContracts';
import type { ContextRequestV2 } from '../../contracts-gen/arbiter';
import { answerPresentation, type AnswerNode } from '../../ui/src/answerPresentation';
import { createBrowserNativeMetadataCodec } from '../../ui/src/nativeMetadataCodec';
import { createAnswerNavigation, isAnswerEnvelope } from '../../vscode/src/workbenchAnswerNavigation';

type Rec = Record<string, unknown>;
const rec = (value: unknown): Rec => value !== null && typeof value === 'object' && !Array.isArray(value) ? value as Rec : {};
const label = (value: unknown): string => typeof value === 'string' ? value : '';
const names = (items: Item[], ids: string[]) => ids.map(id => label(items.find(item => item.value.id === id)?.value.name) || id);
const ok = (result: CallResult): Rec | null => result.kind === 'ok' ? rec(result.envelope.result) : null;
export function searchBody(value: unknown): Rec {
  const outer = rec(value), result = outer.status === 'tool_result' ? rec(outer.result) : outer;
  if (Object.keys(rec(result.structuredContent)).length) return rec(result.structuredContent);
  const content = Array.isArray(result.content) ? result.content.map(rec) : [];
  const text = content.find(part => part.type === 'text' && typeof part.text === 'string');
  if (text) { try { return rec(JSON.parse(String(text.text))); } catch { /* Full original result is retained below. */ } }
  return result;
}
// Serializes only this existing session-scoped grant operation across page remounts.
// A retired pending selection must settle before a replacement can be sent.
const scopeQueues = new WeakMap<object, Promise<CallResult | null>>();
/** Exact owning canonical scope locations, never a scan of arbitrary evidence. */
export function returnedSearchScopeMatches(original: unknown, expected: string): boolean {
  if (!isDigest(expected)) return false;
  const value = searchBody(original), observation = rec(value.scope_observation), selection = rec(observation.selection);
  const observed: unknown[] = [];
  if (Object.hasOwn(selection, 'scope_identity')) observed.push(selection.scope_identity);
  if (value.schema === 'airk.combined-read/1' && Object.hasOwn(rec(value.publication_vector), 'authority')) observed.push(rec(value.publication_vector).authority);
  return observed.length > 0 && observed.every(value => value === expected);
}
/** Presentation state only; it never grants authority or retries a read. */
export function createSearchWorkspace(deps: { run(operation: Operation, request: unknown): Promise<CallResult>; session(): unknown; changed(): void; materialize(raw: Rec, checkCurrent: () => void): Promise<Rec> }) {
  let epoch = 0, disposed = false, loading = false, applying = false, sessionOwner = deps.session();
  let header: CatalogHeader | null = null, items: Item[] = [], projects: string[] = [], indexes: string[] = [];
  let applied: { epoch: number; session: unknown; title: string; request: unknown; scopeIdentity: string } | null = null;
  let verifiedScope = false, logicalBody: Rec | null = null, metadataError: string | null = null;
  let originalRequest: { tool: string; arguments: Rec; scope: string; scopeIdentity: string } | null = null;
  let problem: string | null = null, catalog: CallState = { phase: 'idle' }, scope: CallState = { phase: 'idle' }, result: CallState = { phase: 'idle' };
  const live = (generation: number, session: unknown) => !disposed && epoch === generation && session === deps.session();
  const notify = () => { if (!disposed) deps.changed(); };
  const run = async (operation: Operation, request: unknown): Promise<CallResult> => {
    try { return await deps.run(operation, request); } catch (error) { return { kind: 'transport', status: null, message: error instanceof Error ? error.message : String(error) }; }
  };
  const invalidate = () => { epoch++; applied = null; originalRequest = null; verifiedScope = false; logicalBody = null; metadataError = null; result = { phase: 'idle' }; };
  const synchronize = () => { if (sessionOwner !== deps.session()) { sessionOwner = deps.session(); invalidate(); header = null; items = []; loading = applying = false; scope = { phase: 'idle' }; problem = 'Session changed. Reload sources before applying a scope.'; } };
  const selection = () => {
    if (!header || loading || problem || !projects.length || projects.some(id => !items.some(item => item.kind === 'project' && item.value.id === id))) return null;
    const eligible = items.filter(item => item.kind === 'membership' && projects.includes(label(item.value.project_id))
      && Array.isArray(item.value.scope) && item.value.scope.length > 0).map(item => label(item.value.index_id));
    if (!eligible.length || indexes.some(id => !eligible.includes(id)) || projects.length > 32 || indexes.length > 32) return null;
    const chosen = indexes.length ? indexes : [...new Set(eligible)];
    if (chosen.length > 32 || chosen.some(id => !items.some(item => item.kind === 'index' && item.value.id === id && item.value.tombstone !== true))) return null;
    return { request: body.searchSelectScope(header.namespace, header.revision, [...indexes], [...projects]), chosen };
  };
  return {
    view: () => { synchronize(); return ({ loading, applying, header, items, projects, indexes, problem, catalog, scope, result, originalRequest, verifiedScope, logicalBody, metadataError,
      applied: applied && applied.epoch === epoch && applied.session === deps.session() ? applied : null, canApply: selection() !== null && !applying }); },
    choose(nextProjects: string[], nextIndexes: string[] = []) { synchronize(); invalidate(); projects = [...new Set(nextProjects)]; indexes = [...new Set(nextIndexes)]; notify(); },
    async reload() {
      synchronize(); if (applying) return; invalidate(); const generation = epoch, session = deps.session(); loading = true; header = null; items = []; problem = null; notify();
      const accepted: Item[] = []; let pinned: CatalogHeader | null = null;
      try {
        for (const kind of ['indexes', 'projects', 'memberships'] as CatalogKind[]) {
          let after: unknown = null; const cursors = new Set<string>();
          for (let pageNumber = 0; ; pageNumber++) {
            if (pageNumber >= 128) throw new Error('Catalog listing is incomplete. Narrow the library before selecting a scope.');
            const reply = await run(OPS.catalogPage, body.catalogPage(kind, {}, after, pinned?.revision ?? null));
            if (!live(generation, session)) return;
            catalog = { phase: 'done', operation: OPS.catalogPage, result: reply, at: Date.now() };
            const value = ok(reply), page = rec(value?.page);
            if (!value || !isIncarnation(label(page.namespace)) || !Number.isSafeInteger(page.revision) || !Array.isArray(page.items)) throw new Error('The catalog could not be read completely. Inspect the native result and reload.');
            if (pinned && (pinned.namespace !== page.namespace || pinned.revision !== page.revision)) throw new Error('Catalog changed while listing. Reload before selecting sources.');
            pinned = { namespace: String(page.namespace), revision: Number(page.revision) };
            for (const raw of page.items) { const item = itemOf(raw), expected = kind === 'indexes' ? 'index' : kind === 'projects' ? 'project' : 'membership'; if (!item || item.kind !== expected) throw new Error('The catalog returned an unexpected item. Reload.');
              if (item.kind !== 'membership' && (!isCatalogId(item.kind === 'index' ? 'idx_' : 'prj_', label(item.value.id)) || accepted.some(prior => prior.kind === item.kind && prior.value.id === item.value.id))) throw new Error('Duplicate or malformed catalog identity. Reload.');
              accepted.push(item); }
            if (page.continuation === null) break;
            if (page.continuation === undefined) throw new Error('Catalog continuation is missing. No partial scope was selected.');
            const cursor = JSON.stringify(page.continuation); if (cursors.has(cursor)) throw new Error('Catalog continuation repeated. No partial scope was selected.');
            cursors.add(cursor); after = page.continuation;
          }
        }
        if (live(generation, session)) { header = pinned; items = accepted; projects = projects.filter(id => accepted.some(item => item.kind === 'project' && item.value.id === id)); indexes = []; }
      } catch (error) { if (live(generation, session)) problem = error instanceof Error ? error.message : String(error); }
      finally { if (live(generation, session)) { loading = false; notify(); } }
    },
    async apply() {
      synchronize(); const chosen = selection(); if (!chosen || applying) return;
      invalidate(); const generation = epoch, session = deps.session(); applying = true; scope = { phase: 'pending', operation: OPS.searchSelectScope }; notify();
      const prior = session !== null && typeof session === 'object' ? scopeQueues.get(session) : undefined;
      const submitted = (prior ?? Promise.resolve()).then(() => live(generation, session) ? run(OPS.searchSelectScope, chosen.request) : null);
      if (session !== null && typeof session === 'object') scopeQueues.set(session, submitted);
      const reply = await submitted;
      if (session !== null && typeof session === 'object' && scopeQueues.get(session) === submitted) scopeQueues.delete(session);
      if (reply === null) { applying = false; notify(); return; }
      if (!live(generation, session)) { applying = false; notify(); return; }
      scope = { phase: 'done', operation: OPS.searchSelectScope, result: reply, at: Date.now() }; applying = false;
      const value = ok(reply);
      if (value?.status === 'admin_scope_granted' && isDigest(label(value.scope_identity)) && value.catalog_revision === header?.revision) {
        const setNames = names(items.filter(item => item.kind === 'project'), projects), indexNames = names(items.filter(item => item.kind === 'index'), chosen.chosen);
        applied = { epoch, session, title: `${setNames.join(', ')} · ${indexNames.join(', ')}`, request: chosen.request, scopeIdentity: String(value.scope_identity) };
      }
      notify();
    },
    async execute(tool: string, args: Rec, responseMode: 'terse' | 'full') {
      synchronize(); if (result.phase === 'pending' || !applied || applied.epoch !== epoch || applied.session !== deps.session() || applying || loading) return;
      const generation = epoch, session = deps.session(); originalRequest = { tool, arguments: structuredClone(args), scope: applied.title, scopeIdentity: applied.scopeIdentity }; verifiedScope = false; result = { phase: 'pending', operation: OPS.searchCall }; notify();
      const reply = await run(OPS.searchCall, body.searchCall(tool, structuredClone(args), responseMode));
      if (!live(generation, session)) return;
      const matched = reply.kind === 'ok' && returnedSearchScopeMatches(reply.envelope.result, originalRequest!.scopeIdentity);
      let restored: Rec | null = null, failure: string | null = null;
      if (matched && reply.kind === 'ok') {
        const checkCurrent = () => { if (!live(generation, session)) throw new Error('Search selection changed while restoring native metadata.'); };
        try { restored = await deps.materialize(searchBody(reply.envelope.result), checkCurrent); }
        catch { failure = 'Native metadata could not be restored. The complete original response remains available; no reference or citation was guessed.'; }
      }
      if (!live(generation, session)) return;
      result = { phase: 'done', operation: OPS.searchCall, result: reply, at: Date.now() };
      verifiedScope = matched; logicalBody = restored; metadataError = failure;
      if (!verifiedScope) applied = null;
      notify();
    },
    dispose() { disposed = true; epoch++; applied = null; },
  };
}
export function guidedArguments(kind: 'search' | 'answer', text: string, retrieval: string, path: string, limit: number, advanced: Rec): Rec {
  if (kind === 'answer') {
    const request: ContextRequestV2 = { schema: 'airk.arbiter.context.request/2', question: text, budget: structuredClone(WORKBENCH_CONTEXT_BUDGET) };
    return { ...request, ...advanced, schema: request.schema, question: text };
  }
  return { ...advanced, query: text, mode: retrieval, limit, ...(path ? { path_prefix: path } : {}) };
}
export function completeSearchMarkdown(original: unknown, request: unknown): string {
  const raw = JSON.stringify(original, null, 2), input = JSON.stringify(request, null, 2);
  const fence = '`'.repeat(Math.max(3, ...[...`${raw}${input}`.matchAll(/`+/g)].map(match => match[0].length + 1)));
  return `# AIRK indexed evidence\n\nComplete original request:\n\n${fence}json\n${input}\n${fence}\n\nComplete original native response (including limits and provenance):\n\n${fence}json\n${raw}\n${fence}\n`;
}
export function returnedSearchCursor(original: unknown, tool: string): string | null {
  const definition = manifest.tools.find(item => item.name === tool.replace(/^airk_/, ''));
  if (!definition || !Object.hasOwn(rec(rec(definition.inputSchema).properties), 'cursor')) return null;
  const body = searchBody(original);
  for (const cursor of [body.cursor, rec(body.page).cursor, rec(body.content).cursor, rec(rec(body.content).page).cursor]) if (typeof cursor === 'string' && cursor.length > 0) return cursor;
  return null;
}
function downloadText(name: string, text: string, type: string) {
  const url = URL.createObjectURL(new Blob([text], { type })), anchor = document.createElement('a');
  anchor.href = url; anchor.download = name; anchor.rel = 'noopener noreferrer'; anchor.click(); window.setTimeout(() => URL.revokeObjectURL(url), 0);
}
let browserMetadata: ReturnType<typeof createBrowserNativeMetadataCodec> | undefined;
function metadataOwner() { return browserMetadata ??= createBrowserNativeMetadataCodec(); }
let answerSequence = 0;
const answerIdentities = new WeakMap<object, string>();
function answerIdentity(state: object): string {
  let identity = answerIdentities.get(state);
  if (!identity) { identity = `management-answer-${++answerSequence}`; answerIdentities.set(state, identity); }
  return identity;
}
/** DOM adapter only: all Answer fields/roles/history/citations come from the shared owner. */
function SharedAnswer({ raw, identity }: { raw: Rec; identity: string }): JSX.Element {
  const navigation = createAnswerNavigation(metadataOwner().codec);
  let partitions: ReturnType<typeof navigation.answerPartitions> = [], sources: ReturnType<typeof navigation.answerEvidenceRows> = [];
  try { partitions = navigation.answerPartitions(raw); sources = navigation.answerEvidenceRows(raw); }
  catch { return <p role="alert">The native Answer could not be restored. Its complete original response remains available below.</p>; }
  const show = (value: AnswerNode | string): JSX.Element | string => typeof value === 'string' ? <Text value={value} /> : h(value.tag, {
    ...(value.id ? { id: value.id, tabIndex: -1 } : {}),
    ...(value.citationTarget ? { type: 'button', onClick: () => { const target = document.getElementById(value.citationTarget!); target?.closest('details')?.setAttribute('open', ''); target?.focus(); target?.scrollIntoView({ block: 'start' }); } } : {}),
  }, ...value.children.map(show));
  return <section aria-label="Native Answer partitions">
    {raw.schema === 'airk.combined-read/1' && <><h3>Combined Answer</h3><p>Every native member remains separate. Failed, exhausted and unqueried members do not establish coverage.</p></>}
    {partitions.map(partition => <section key={partition.member ?? 'local'} aria-label={`Answer partition ${partition.member ?? 'local'}`}>
      {partition.member !== undefined && <h4>Index <Text value={partition.indexId ?? 'unavailable'} /> · member {partition.member} · <Text value={partition.outcome} /></h4>}
      {partition.packet ? <>{show(answerPresentation(partition.packet, partition.member === undefined ? identity : `${identity}-member-${partition.member}`))}
        <p>{sources.some(source => source.answer_member === partition.member) ? 'Native source locators are bound to this original member and record. Evidence buttons focus the complete original record; this management view does not open an editor.' : 'No unambiguous native source locator was returned for this member. Evidence buttons focus the complete original record; no editor source authority is inferred.'}</p></>
        : <p>No valid locally bound current Answer packet was returned for this member. No local assessment or source authority is inferred.</p>}
      {partition.member !== undefined && <details><summary>Complete original member</summary><pre><Text value={JSON.stringify(Array.isArray(raw.members) ? raw.members[partition.member] : partition.raw, null, 2)} /></pre></details>}
    </section>)}
    {!partitions.length && <p>No supported current Answer packet could be projected. The complete original response remains below.</p>}
  </section>;
}
function Results({ state, request, verifiedScope, logicalBody, metadataError, more }: { state: CallState; request: { tool: string; arguments: Rec; scope: string } | null; verifiedScope: boolean; logicalBody: Rec | null; metadataError: string | null; more(cursor: string): void }): JSX.Element | null {
  const [notice, setNotice] = useState('');
  if (state.phase !== 'done') return <Outcome state={state} />;
  if (state.result.kind !== 'ok') return <><Outcome state={state} /><details><summary>Complete actual read outcome</summary><JsonBlock value={state.result} name="airk-search-outcome.json" /></details></>;
  const original = state.result.envelope.result, raw = searchBody(original), root = logicalBody ?? raw, answer = isAnswerEnvelope(root), cursor = request ? returnedSearchCursor(original, request.tool) : null;
  const members = root.schema === 'airk.combined-read/1' && Array.isArray(root.members) ? root.members.map(rec) : [{ outcome: 'local', result: root }];
  const markdown = completeSearchMarkdown(original, request);
  const copy = async (text: string) => { try { if (!navigator.clipboard) throw new Error('Clipboard unavailable; use Download.'); await navigator.clipboard.writeText(text); setNotice('Copied complete response.'); } catch (error) { setNotice(String(error)); } };
  return <section aria-label="Search results"><div class="row"><h3>{verifiedScope ? 'Indexed evidence' : 'Actual native response — scope not verified'}</h3><button onClick={() => void copy(JSON.stringify(original, null, 2))}>Copy complete JSON</button><button onClick={() => void copy(markdown)}>Copy complete Markdown</button><button onClick={() => downloadText('airk-search.json', JSON.stringify(original, null, 2), 'application/json')}>Download JSON</button><button onClick={() => downloadText('airk-search.md', markdown, 'text/markdown')}>Download Markdown</button></div>
    {notice && <p role="status"><Text value={notice} /></p>}
    {!verifiedScope && <p role="alert">The response did not establish the requested source scope. The previous selection is no longer applied here. Reapply the intended sources before another read. No automatic retry occurred; the complete actual response is retained below.</p>}
    {request && <><p>{verifiedScope ? 'Results for ' : 'Original request for '} <Text value={label(request.arguments.query) || label(request.arguments.question) || request.tool} /> · scope: <Text value={request.scope} /></p><details><summary>Complete original request</summary><pre><Text value={JSON.stringify(request, null, 2)} /></pre></details></>}
    <p>Indexed evidence is untrusted source material. Saved-source verification, freshness and retrieval coverage are separate observations.</p>
    {['warnings', 'completeness', 'partial', 'truncated', 'error', 'context'].filter(key => root[key] !== undefined).map(key => key === 'context' ? <details><summary>Whole-selection context accounting</summary><pre><Text value={JSON.stringify(root[key], null, 2)} /></pre></details> : <p class={key === 'error' ? 'error' : 'muted'}><strong>{key}: </strong><Text value={JSON.stringify(root[key])} /></p>)}
    {verifiedScope && cursor && <button type="button" onClick={() => more(cursor)}>Load next native page in the original scope</button>}
    {metadataError && <p role="alert"><Text value={metadataError} /></p>}
    {verifiedScope && logicalBody && answer && <SharedAnswer raw={raw} identity={answerIdentity(state)} />}
    {verifiedScope && logicalBody && !answer && members.map((member, memberNumber) => { const packet = searchBody(member.result);
      const reads = [packet];
      return <section><h4>{label(member.name) || label(member.index_id) || 'Selected source'} · {label(member.outcome)}</h4>
        {reads.flatMap((read, readNumber) => (Array.isArray(read.hits) ? read.hits : []).map((raw, hitNumber) => { const hit = rec(raw), citation = rec(hit.citation); return <article key={`${memberNumber}-${readNumber}-${hitNumber}`}><h5><Text value={`${label(citation.path)} ${citation.start_line ?? ''}–${citation.end_line ?? ''}`} /></h5><pre><Text value={label(hit.text)} /></pre><details><summary>Original citation and source observations</summary><pre><Text value={JSON.stringify({ citation, publication: read.publication, freshness: read.freshness, source_verification: read.source_verification, coverage: read.coverage, truncated: read.truncated }, null, 2)} /></pre></details></article>; }))}
        {['warnings', 'completeness', 'truncated', 'coverage', 'freshness', 'source_verification', 'error', 'stop', 'assessment'].map(key => packet[key] !== undefined ? <p><strong>{key}: </strong><Text value={JSON.stringify(packet[key])} /></p> : null)}
        {!reads.some(read => Array.isArray(read.hits) && read.hits.length) && <p>No hit rows were returned. Inspect the complete response for readiness, refusal, empty results or other tool-specific output.</p>}
      </section>;
    })}
    <details><summary>Complete original native response</summary><pre><Text value={JSON.stringify(original, null, 2)} /></pre></details>
  </section>;
}
export function Search(): JSX.Element {
  const [, refresh] = useState(0), workspace = useRef<ReturnType<typeof createSearchWorkspace> | null>(null);
  if (!workspace.current) workspace.current = createSearchWorkspace({ run: call, session: currentSession, changed: () => refresh(value => value + 1), materialize: (raw, check) => metadataOwner().materializeNativeMetadata(raw, check) });
  const owner = workspace.current, view = owner.view(), concurrency = useQueryConcurrency();
  useEffect(() => { void owner.reload(); return () => owner.dispose(); }, []);
  const [kind, setKind] = useState<'search' | 'answer'>('search'), [query, setQuery] = useState(''), [retrieval, setRetrieval] = useState('lexical'), [path, setPath] = useState(''), [limit, setLimit] = useState(10);
  const [extra, setExtra] = useState(''), [tool, setTool] = useState('search'), [custom, setCustom] = useState(false), [response, setResponse] = useState<'terse' | 'full'>('full'), [filter, setFilter] = useState('');
  let advanced: Rec | null = {}; try { const value = extra.trim() ? JSON.parse(extra) : {}; advanced = value !== null && typeof value === 'object' && !Array.isArray(value) ? value : null; } catch { advanced = null; }
  const projects = view.items.filter(item => item.kind === 'project'), indexes = view.items.filter(item => item.kind === 'index');
  const allowed = view.items.filter(item => item.kind === 'membership' && view.projects.includes(label(item.value.project_id)) && Array.isArray(item.value.scope) && item.value.scope.length > 0).map(item => label(item.value.index_id));
  const scopeNote = concurrencyNote({ indexes: view.indexes.length || new Set(allowed).size, concurrency, subject: 'scope' });
  const selectedTool = custom ? tool : kind, definition = manifest.tools.find(item => item.name === selectedTool.replace(/^airk_/, ''));
  return <>
    <Panel title="Search your indexed sources"><p><strong>Management search scope.</strong> Sources selected here apply to this management session; editor-window search and agent grants are separate.</p>
      <p role="status">{view.applied ? <>Applied scope: <Text value={view.applied.title} /></> : 'Choose named sources and apply the scope before reading.'}</p>
      <details open={!view.applied}><summary>Choose sources</summary><div class="row"><Field label="Filter named index sets" value={filter} onInput={setFilter} /><button type="button" disabled={view.loading || view.applying} onClick={() => void owner.reload()}>Reload sources</button></div>
        {view.loading && <p>Loading the native catalog…</p>}{view.problem && <p role="alert"><Text value={view.problem} /></p>}
        <fieldset style={{ maxHeight: '16rem', overflowY: 'auto' }}><legend>Named index sets</legend>
        {projects.filter(item => label(item.value.name).toLocaleLowerCase().includes(filter.toLocaleLowerCase())).map(item => { const id = label(item.value.id); return <label><input type="checkbox" disabled={view.loading || view.applying} checked={view.projects.includes(id)} onChange={event => owner.choose(event.currentTarget.checked ? [...view.projects, id] : view.projects.filter(value => value !== id))} /><Text value={label(item.value.name)} />{item.value.description && <span> · <Text value={label(item.value.description)} /></span>}</label>; })}</fieldset>
        {!view.loading && !view.problem && !projects.length && <p>No index sets are available. Create an index in Indexes, then reload.</p>}
        {!!view.projects.length && <fieldset><legend>Indexes in these sets</legend><p>Leave every box clear to read all enabled membership sources of the chosen sets. Checking indexes narrows that exact scope.</p>{indexes.filter(item => allowed.includes(label(item.value.id))).map(item => { const id = label(item.value.id); return <label><input type="checkbox" checked={view.indexes.includes(id)} disabled={view.loading || view.applying || item.value.tombstone === true} onChange={event => owner.choose(view.projects, event.currentTarget.checked ? [...view.indexes, id] : view.indexes.filter(value => value !== id))} /><Text value={label(item.value.name)} /> · {item.value.tombstone === true ? 'retired' : item.value.source_import === 'complete' ? 'Sources registered; search readiness not yet observed' : 'Source setup is incomplete or not observed'}</label>; })}</fieldset>}
        <ConcurrencyNote text={scopeNote} /><button type="button" disabled={!view.canApply} onClick={() => void owner.apply()}>Use selected sources</button>
        <details><summary>Native catalog clock and selection details</summary><pre><Text value={JSON.stringify({ clock: view.header, project_ids: view.projects, index_ids: view.indexes }, null, 2)} /></pre><Outcome state={view.catalog} /><Outcome state={view.scope} /></details>
      </details>
      <form onSubmit={event => { event.preventDefault(); if (!advanced || !view.applied) return; void owner.execute(selectedTool, custom ? advanced : guidedArguments(kind, query, retrieval, path, limit, advanced), response); }}>
        <div class="row"><label class="field">Read<select value={kind} onChange={event => { setKind(event.currentTarget.value === 'answer' ? 'answer' : 'search'); setCustom(false); }}><option value="search">Search indexed evidence</option><option value="answer">Gather cited context for a question</option></select></label><Field label={kind === 'answer' ? 'Question' : 'Query'} wide value={query} placeholder="Where is request cancellation handled?" onInput={setQuery} /></div>
        {!custom && kind === 'search' && <div class="row"><label class="field">Retrieval mode<select value={retrieval} onChange={event => setRetrieval(event.currentTarget.value)}><option value="lexical">Text</option><option value="hybrid">Hybrid</option><option value="vector">Semantic</option></select></label><Field label="Path prefix (optional)" value={path} onInput={setPath} /><NumberField label="Result limit" value={limit} min={1} max={100} onInput={setLimit} /></div>}
        <button type="submit" disabled={!view.applied || view.loading || view.applying || view.result.phase === 'pending' || advanced === null || (!custom && !query.trim())}>{custom ? 'Run selected read' : kind === 'answer' ? 'Gather cited context' : 'Search'}</button>
        <details><summary>Advanced tools, arguments and full native schema</summary><label><input type="checkbox" checked={custom} onChange={event => setCustom(event.currentTarget.checked)} />Use an advanced native tool with exactly the JSON arguments below</label><label class="field">Native tool<select value={tool} onChange={event => setTool(event.currentTarget.value)}>{manifest.tools.map(item => <option value={item.name}>{item.name}</option>)}</select></label><Field label="Other native tool name (optional)" value={tool} onInput={setTool} /><label class="field">Native arguments JSON<textarea rows={5} value={extra} class={advanced === null ? 'invalid' : ''} onInput={event => setExtra(event.currentTarget.value)} /></label><p>Advanced tool mode sends this object unchanged. Guided search keeps every extra field; the visible query, retrieval mode and limit take precedence. Question mode retains a supplied budget; otherwise it uses the existing Workbench context budget.</p><label class="field">Response detail<select value={response} onChange={event => setResponse(event.currentTarget.value === 'terse' ? 'terse' : 'full')}><option value="full">Full</option><option value="terse">Terse</option></select></label>{definition && renderHelpNode((tag, children) => h(tag, {}, ...children), helpToolPresentation(definition))}</details>
      </form>
    </Panel>
    <Panel title="Results"><Results state={view.result} request={view.originalRequest} verifiedScope={view.verifiedScope} logicalBody={view.logicalBody} metadataError={view.metadataError} more={cursor => { if (view.originalRequest && view.applied) void owner.execute(view.originalRequest.tool, { ...view.originalRequest.arguments, cursor }, response); }} /></Panel>
  </>;
}
