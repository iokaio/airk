// SPDX-License-Identifier: Apache-2.0
import type { ConnectionState, FreshnessState, IndexState, ProjectSummary, ProjectWorkView, ReadRouteView, SemanticState, StatusSummary } from './explorerContracts';
import { comparePublicationVector } from './publications';
import { observedConcurrency } from './indexingCatalog';

type Value = Record<string, unknown>;
const obj = (value: unknown): Value => value && typeof value === 'object' && !Array.isArray(value) ? value as Value : {};
const count = (value: unknown): number | undefined => typeof value === 'number' && Number.isSafeInteger(value) && value >= 0 ? value : undefined;
const pick = <T extends string>(value: unknown, allowed: readonly T[], fallback: T): T => allowed.find(entry => entry === value) ?? fallback;

const SEMANTIC: readonly SemanticState[] = ['pending', 'partial', 'ready', 'failed', 'disabled', 'unavailable'];
const FRESHNESS: readonly FreshnessState[] = ['unregistered', 'unavailable', 'idle', 'pending', 'running', 'paused', 'blocked', 'failed'];

/** Present the observed job's own work; semantic work reads retained inputs, not source files. */
export function workProgressDisplay(work: ProjectWorkView, kind?: string, state?: string) {
    const semantic = kind === 'vectors';
    const phases: Record<string,string> = { starting:'Starting worker', discovering:'Discovering source files', extracting:'Extracting source files',
        preparing:'Preparing index changes', publishing:'Publishing index changes', embedding:'Building semantic index', verifying:'Verifying retained data',
        recovering:'Recovering captured work', retiring:'Cleaning up owned index state', finished:'Work finished' };
    if (semantic) {
        phases.preparing = 'Preparing semantic index batch';
        phases.publishing = 'Publishing semantic index batch';
    }
    const cleanup=work.phase==='retiring';
    if(cleanup)phases.retiring=work.publications!==undefined&&work.publications>0?'Cleaning up after publication':'Cleaning up retained index data';
    if(work.phase==='finished')phases.finished=state==='failed'?'Attempt failed':state==='canceled'?'Attempt canceled':state==='succeeded'?'Latest attempt completed':'Finishing this attempt';
    const stages = semantic ? [['embedding','Prepare & build'],['publishing','Publish'],['retiring','Clean up']]
        : [['discovering','Discover'],['extracting','Extract'],['preparing','Prepare'],['publishing','Publish'],['embedding','Semantic'],['retiring','Clean up']];
    const activePhase = semantic && work.phase === 'preparing' ? 'embedding' : work.phase;
    const metrics: readonly (readonly [string,number | undefined])[] = semantic
        ? [['Semantic units completed (reported)',work.semanticUnits],['Files committed this attempt',work.committed],['Batches published this attempt',work.publications]]
        : cleanup ? [['Work items committed this attempt',work.committed],['Batches published this attempt',work.publications]]
        : ['inspect','plan_inspect','recover','verify'].includes(kind??'') ? [['Retained objects verified',work.verified],['Batches published this attempt',work.publications]]
        : kind==='model_setup' ? []
        : [['Captured files',work.captured],['Newly extracted',work.extracted],['Index transactions published',work.publications],['Total eligible files',work.total]];
    return { semantic, metrics, stages:stages.map(([phase,label])=>({phase,label,active:work.current&&activePhase===phase})),
        title:kind==='model_setup'&&state==='running'?'Preparing offline model':work.phase?phases[work.phase]??work.phase:'Progress not observed',
        explanation:cleanup?'AIRK is removing obsolete index data. Published results remain separate from this cleanup phase. Capture and extraction counters do not measure cleanup.':semantic?'This job builds semantic search from already indexed content. Preparation includes vectors and search structures; progress can advance before a batch is published. Completed units are reported at batch boundaries and can include retained work from earlier attempts.':undefined };
}

/** Prefer the complete MCP snapshot; use console metadata until it arrives. Never combine revisions. */
export function indexContents(project: ProjectSummary, status?: StatusSummary) {
    if(status)return {...status};
    return {indexState:project.indexState,files:project.indexedFiles,chunks:undefined,vectors:undefined,
        semantic:project.semantic??'unknown',semanticCompleted:project.semanticCompleted,semanticSelected:project.semanticSelected,
        discoveryState:project.discoveryState,publication:project.publication,observedAt:project.observedAt,freshness:undefined};
}

export function sourceWorkExplanation(freshness?:string):string {
    return ({pending:'More saved changes are waiting to be indexed.',running:'Updating the text index from saved source changes.',
        paused:'Source updates are paused.',blocked:'Source updates are blocked; see the current issue below.',failed:'The latest source update failed; the published snapshot is shown above.',
        idle:'No source update is currently reported as pending. This does not verify unsaved or live file contents.'} as Record<string,string>)[freshness??'']??'Current source-update status has not been reported in this observation.';
}

/** Published readiness and the saved vector policy are independent observations. */
export function semanticExplanation(status: Pick<StatusSummary, 'semantic' | 'vectorsEnabled' | 'localModelReady'> | undefined): { title: string; detail: string } {
    if (!status) { return { title: 'Semantic status not checked', detail: 'Refresh published status to check coverage and model configuration.' }; }
    if (status.semantic === 'unavailable') { return { title: 'Semantic pipeline unavailable', detail: 'The native semantic pipeline is unavailable for this saved evidence. This is not a saved vector opt-out or semantic readiness. Source and lexical evidence have separate availability; review the complete native readiness and compatibility details.' }; }
    if (status.vectorsEnabled === false) { return { title: 'Vector search is turned off', detail: 'The saved project policy disables vector search. Review project settings to enable it. Published source evidence has separate availability.' }; }
    if (status.semantic === 'disabled' && status.vectorsEnabled === true && status.localModelReady !== false) {
        return { title: 'Semantic setup not yet published', detail: 'Semantic mode is enabled in the saved settings, but this publication has no semantic coverage. Model preparation or indexing may still be queued; Recent activity shows their outcome.' };
    }
    if (status.semantic === 'disabled') { return { title: 'Not available in this publication', detail: status.localModelReady === false
        ? 'This publication has no model configuration or semantic work record. This does not by itself mean vector search was turned off. Model setup or source indexing may still be pending; review semantic setup.'
        : 'The publication reports semantic coverage as disabled. This alone does not establish a saved vector opt-out or its cause. Review model configuration and project settings.' }; }
    if (status.semantic === 'failed') { return { title: 'Semantic indexing failed', detail: 'Review the vector job failure and local model status. Source indexing and lexical evidence have separate status.' }; }
    if (status.semantic === 'partial') { return { title: 'Semantic coverage is partial', detail: 'Some selected artifacts have vectors. Additional work may be queued, paused or blocked; check activity.' }; }
    if (status.semantic === 'pending') { return { title: 'Semantic indexing pending', detail: 'The publication has pending semantic work. Check activity for admission, pause or failure details.' }; }
    if (status.semantic === 'ready') { return { title: 'Semantic coverage ready', detail: 'Selected semantic work is complete for this publication. Coverage applies to the configured selection, not every source file.' }; }
    return { title: 'Semantic status unknown', detail: 'No recognized semantic readiness observation is available. Refresh published status or review diagnostics.' };
}

/** airk_status payload → published readiness summary. Unknown or absent values stay unknown. */
export function statusSummary(payload: Value, observedAt = Date.now(), rawPayload: Value = payload): StatusSummary {
    if (payload.schema === 'airk.combined-read/1') { return combinedStatusSummary(payload, observedAt, rawPayload); }
    const indexState: IndexState = payload.index_state === 'available' ? 'available' : payload.index_state === 'unavailable' ? 'unavailable' : 'unknown';
    const publication = obj(payload.publication), semantic = obj(payload.semantic_readiness), freshness = obj(payload.freshness), discovery=obj(payload.discovery_readiness);
    const runtime = obj(payload.runtime), work = obj(payload.work), indexError = obj(payload.index_error);
    // `work.progress` is the native ProgressObservation: counters in `.progress`, the sampled job in `.job`.
    const sample = obj(work.progress), progress = Object.keys(obj(sample.progress)).length ? obj(sample.progress) : sample, sampledJob = obj(sample.job);
    const runtimeVersions = ([['MCP frontend (airk)', 'mcp_frontend'], ['Index worker (airk)', 'index_worker']] as const).map(([label, key]): readonly [string, string] => {
        const observed = obj(runtime[key]), process = obj(observed.process);
        const version = typeof process.version === 'string' ? process.version.slice(0, 64) : 'not observed';
        const basis = typeof observed.basis === 'string' ? observed.basis.slice(0, 80) : 'no process observation';
        return [label, `${version} · ${basis}${count(process.process_id) !== undefined ? ` · PID ${process.process_id}` : ''}`];
    });
    const summary: StatusSummary = {
        indexState,
        semantic: pick(semantic.state, SEMANTIC, 'unknown'),
        ...(Object.keys(semantic).length ? { semanticReadiness: structuredClone(semantic) } : {}),
        ...(Object.keys(obj(payload.legacy_adoption)).length ? { legacyAdoption: structuredClone(obj(payload.legacy_adoption)) } : {}),
        freshness: pick(freshness.state, FRESHNESS, 'unknown'),
        observedAt,
        ...(typeof indexError.code === 'string' ? { indexError: indexError.code.slice(0, 128) } : {}),
        ...(typeof publication.token === 'string' ? { publication: publication.token } : {}),
        ...(count(payload.files) !== undefined ? { files: count(payload.files) } : {}),
        ...(count(payload.chunks) !== undefined ? { chunks: count(payload.chunks) } : {}),
        ...(count(payload.indexed_vectors) !== undefined ? { vectors: count(payload.indexed_vectors) } : {}),
        ...(typeof discovery.state==='string' ? {discoveryState:discovery.state.slice(0,64)} : {}),
        ...(count(semantic.completed_artifacts) !== undefined ? { semanticCompleted: count(semantic.completed_artifacts) } : {}),
        ...(count(semantic.selected_artifacts) !== undefined ? { semanticSelected: count(semantic.selected_artifacts) } : {}),
        ...(typeof freshness.live_source_verified === 'boolean' ? { liveSourceVerified: freshness.live_source_verified } : {}),
        ...(typeof payload.vectors_enabled === 'boolean' ? { vectorsEnabled: payload.vectors_enabled } : {}),
        ...(typeof payload.local_model_ready === 'boolean' ? { localModelReady: payload.local_model_ready } : {}),
        ...('versions_match' in runtime ? { versionsMatch: typeof runtime.versions_match === 'boolean' ? runtime.versions_match : null } : {}),
        ...(runtime.mcp_frontend || runtime.index_worker ? { runtimeVersions } : {}),
        ...(typeof progress.phase === 'string' ? { workPhase: progress.phase.slice(0, 64) } : {}),
        ...(typeof work.project_paused === 'boolean' ? { workPaused: work.project_paused } : {}),
        ...(typeof sampledJob.state === 'string' ? { workJobState: sampledJob.state.slice(0, 64) } : {}),
        ...(count(progress.committed_changes) !== undefined ? { workCommitted: count(progress.committed_changes) } : {}),
        ...(count(progress.captured_files) !== undefined ? { workCaptured: count(progress.captured_files) } : {}),
        ...(Object.keys(work).length ? { work: structuredClone(work) } : {}),
        ...(observedConcurrency(payload.query_concurrency) !== undefined ? { queryConcurrency: observedConcurrency(payload.query_concurrency) } : {}),
    };
    return summary;
}

function combinedStatusSummary(payload: Value, observedAt: number, rawPayload: Value): StatusSummary {
    const rows = Array.isArray(payload.members) && payload.members.length <= 32 ? payload.members.map(obj) : [];
    const seen = new Set<string>();
    const members = rows.map(member => {
        const indexId = typeof member.index_id === 'string' && /^idx_[0-9a-f]{32}$/.test(member.index_id) ? member.index_id : '';
        const envelope = obj(member.result);
        const body = Array.isArray(envelope.content) && Object.keys(obj(envelope.structuredContent)).length ? obj(envelope.structuredContent) : envelope;
        const valid = !!indexId && !seen.has(indexId) && body.schema !== 'airk.combined-read/1';
        seen.add(indexId);
        const stamp = count(body.observed_unix_ms), received = stamp === undefined ? observedAt : Math.min(stamp, observedAt);
        const outcome = valid && member.outcome === 'complete' && member.is_error !== true ? 'complete' : typeof member.outcome === 'string' ? member.outcome : 'unknown';
        return { indexId, outcome, status: statusSummary(valid && outcome === 'complete' ? body : {}, received) };
    });
    const usable = members.filter(member => member.outcome === 'complete' && member.status.indexState === 'available');
    const complete = rows.length > 0 && members.every(member => member.indexId && member.outcome === 'complete' && member.status.indexState === 'available');
    const sum = (key: 'files' | 'chunks' | 'vectors' | 'semanticCompleted' | 'semanticSelected'): number | undefined => {
        if (!usable.length || usable.some(member => member.status[key] === undefined)) { return undefined; }
        const value = usable.reduce((total, member) => total + member.status[key]!, 0);
        return Number.isSafeInteger(value) ? value : undefined;
    };
    const all = <T>(key: keyof StatusSummary, values: readonly T[]): T | undefined => {
        const valuesSeen = members.map(member => member.status[key]);
        return complete && valuesSeen.length && valuesSeen.every(value => value === valuesSeen[0]) && values.includes(valuesSeen[0] as T) ? valuesSeen[0] as T : undefined;
    };
    const states = members.map(member => member.status.semantic);
    const semantic: SemanticState = states.length && states.every(state => state === 'ready') ? 'ready'
        : states.length && states.every(state => state === 'unavailable') ? 'unavailable'
        : states.length && states.every(state => state === 'disabled') ? 'disabled'
        : states.some(state => state === 'ready' || state === 'partial') ? 'partial'
        : states.some(state => state === 'failed') ? 'failed'
        : states.some(state => state === 'unavailable') ? 'unavailable'
        : states.length && states.every(state => state === 'pending') ? 'pending' : 'unknown';
    const freshness = (['failed', 'blocked', 'running', 'pending', 'paused', 'unavailable', 'unregistered', 'unknown', 'idle'] as const)
        .find(state => members.some(member => member.status.freshness === state)) ?? 'unknown';
    const vectorsEnabled = all('vectorsEnabled', [true, false]), localModelReady = all('localModelReady', [true, false]);
    const partial = !complete || payload.partial === true;
    return {
        indexState: usable.length ? 'available' : members.length && members.every(member => member.status.indexState === 'unavailable') ? 'unavailable' : 'unknown',
        semantic, freshness, observedAt, liveSourceVerified: false,
        ...(sum('files') !== undefined ? { files: sum('files') } : {}), ...(sum('chunks') !== undefined ? { chunks: sum('chunks') } : {}),
        ...(sum('vectors') !== undefined ? { vectors: sum('vectors') } : {}),
        ...(sum('semanticCompleted') !== undefined ? { semanticCompleted: sum('semanticCompleted') } : {}),
        ...(sum('semanticSelected') !== undefined ? { semanticSelected: sum('semanticSelected') } : {}),
        ...(vectorsEnabled !== undefined ? { vectorsEnabled } : {}), ...(localModelReady !== undefined ? { localModelReady } : {}),
        ...(observedConcurrency(payload.query_concurrency) !== undefined ? { queryConcurrency: observedConcurrency(payload.query_concurrency) } : {}),
        combined: { available: usable.length, total: rows.length, partial,
            countsQualifier: `Counts sum ${usable.length} available member index(es), not unique sources.${partial ? ' Unavailable or unqueried members are omitted; their outcomes remain below.' : ''}`,
            members, ...(Object.keys(obj(payload.publication_vector)).length ? { publicationVector: obj(payload.publication_vector) } : {}), nativeResult: rawPayload },
    };
}

/** The selected index's live work (native Progress read) and read route, for the header (U103). */
export interface HeadlineWork {
    readonly jobState?: string;
    readonly pendingJobState?: string;
    readonly paused?: boolean;
    readonly committed?: number;
    readonly captured?: number;
    readonly route?: ReadRouteView;
}
/** The selected index's live work (its project summary) and read route, for `headline`. */
export function headlineWork(project: Pick<ProjectSummary, 'jobState' | 'pendingJobState' | 'paused' | 'committedChanges' | 'capturedFiles'> | undefined, route?: ReadRouteView): HeadlineWork {
    return { ...(project?.jobState ? { jobState: project.jobState } : {}), ...(project?.pendingJobState ? { pendingJobState: project.pendingJobState } : {}),
        ...(project?.paused !== undefined ? { paused: project.paused } : {}), ...(project?.committedChanges !== undefined ? { committed: project.committedChanges } : {}),
        ...(project?.capturedFiles !== undefined ? { captured: project.capturedFiles } : {}), ...(route ? { route } : {}) };
}
/** "indexing <committed>/<captured> files" when both counts are known, else "indexing…". */
export function indexingWord(committed: number | undefined, captured: number | undefined): string {
    return committed !== undefined && captured !== undefined ? `indexing ${committed.toLocaleString('en-US')}/${captured.toLocaleString('en-US')} files` : 'indexing…';
}
export const NOT_READABLE_WORD = 'not readable: no index set';
/** One word for the status bar and pill: the worst current condition, never a merged "OK". A known
 * cause (running work, pause, no index set) is named; "index unavailable" is only the native
 * `index_state: unavailable` or another unexplained unreadable state. */
export function headline(connection: ConnectionState, status: StatusSummary | undefined, projects: number, work: HeadlineWork = {}): { word: string; severity: 'ok' | 'warn' | 'error' | 'info' } {
    if (connection === 'untrusted') { return { word: 'untrusted', severity: 'warn' }; }
    if (connection === 'incompatible') { return { word: 'incompatible', severity: 'error' }; }
    if (connection === 'disconnected') { return { word: 'disconnected', severity: 'error' }; }
    if (connection === 'connecting') { return { word: 'connecting', severity: 'info' }; }
    if (!projects) { return { word: 'no projects', severity: 'info' }; }
    // A paused index keeps its job record; pause is the cause to name.
    if ((work.paused ?? status?.workPaused) === true) { return { word: 'paused', severity: 'info' }; }
    const jobState = work.jobState ?? status?.workJobState;
    if (jobState === 'running') {
        return { word: indexingWord(work.committed ?? status?.workCommitted, work.captured ?? status?.workCaptured), severity: 'info' };
    }
    if (work.route?.state === 'not_readable' || status?.indexError === 'index_read_unavailable') { return { word: NOT_READABLE_WORD, severity: 'warn' }; }
    if (!status) { return { word: connection === 'degraded' ? 'degraded' : 'ready', severity: connection === 'degraded' ? 'warn' : 'info' }; }
    if (status.combined?.partial) { return { word: status.combined.available ? 'partial indexes' : 'member status unknown', severity: 'warn' }; }
    if (status.indexState !== 'available') {
        if (work.pendingJobState === 'pending' || status.freshness === 'pending' || status.freshness === 'running') { return { word: 'indexing…', severity: 'info' }; }
        if (status.indexState === 'unknown' && status.indexError) { return { word: `not readable (${status.indexError})`.slice(0, 96), severity: 'warn' }; }
        return { word: 'index unavailable', severity: 'warn' };
    }
    if (status.freshness === 'failed' || status.freshness === 'blocked' || status.semantic === 'failed') { return { word: 'attention', severity: 'error' }; }
    if (status.semantic === 'unavailable') { return { word: 'semantic unavailable', severity: 'warn' }; }
    if (connection === 'degraded') { return { word: 'degraded', severity: 'warn' }; }
    if (status.freshness === 'running' || status.freshness === 'pending') { return { word: 'indexing', severity: 'info' }; }
    return { word: 'ready', severity: 'ok' };
}


/** Preserve separately sampled details without presenting them as freshly sampled.
 * Native counts/readiness always come from the scalar observation. A merged
 * scalar and detailed timestamps stay independent across subsequent cache hits.
 */
export function scalarWithRetainedDetails(scalar: StatusSummary, details?: StatusSummary, publicationDetailsValid = true): StatusSummary {
    if (!details) { return scalar; }
    const runtime = {
        ...(details.versionsMatch !== undefined ? { versionsMatch: details.versionsMatch } : {}),
        ...(details.runtimeVersions !== undefined ? { runtimeVersions: details.runtimeVersions } : {}),
    };
    const samePublication = publicationDetailsValid && (scalar.combined && details.combined
        ? comparePublicationVector({ authority: 'same-bound-status', vector: scalar.combined.publicationVector }, { authority: 'same-bound-status', vector: details.combined.publicationVector }) === 'equal'
        : scalar.publication !== undefined && scalar.publication === details.publication);
    const publicationDetails = samePublication ? {
        ...(details.vectorsEnabled !== undefined ? { vectorsEnabled: details.vectorsEnabled } : {}),
        ...(details.localModelReady !== undefined ? { localModelReady: details.localModelReady } : {}),
        ...(details.workPhase !== undefined ? { workPhase: details.workPhase } : {}),
        ...(details.workPaused !== undefined ? { workPaused: details.workPaused } : {}),
    } : {};
    const retained = Object.keys(runtime).length + Object.keys(publicationDetails).length > 0;
    return { ...runtime, ...publicationDetails, ...scalar,
        ...(retained ? { detailsObservedAt: details.detailsObservedAt ?? details.observedAt } : {}) };
}

/** The Management version line, shown even when no service is found. The
 * extension version is this bundle's; the service version is only what a live
 * health observation of a connected service reported. */
export function installedVersions(extensionVersion: string, service?: { readonly connected: boolean; readonly serviceVersion?: string }): string {
    const observed = !service ? 'not observed' : !service.connected ? 'not connected' : service.serviceVersion ?? 'version not reported';
    return `VS Code extension ${extensionVersion} · AIRK Service ${observed}`;
}
