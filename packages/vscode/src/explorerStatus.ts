// SPDX-License-Identifier: Apache-2.0
import type { ConnectionState, FreshnessState, IndexState, SemanticState, StatusSummary } from './explorerContracts';

type Value = Record<string, unknown>;
const obj = (value: unknown): Value => value && typeof value === 'object' && !Array.isArray(value) ? value as Value : {};
const count = (value: unknown): number | undefined => typeof value === 'number' && Number.isSafeInteger(value) && value >= 0 ? value : undefined;
const pick = <T extends string>(value: unknown, allowed: readonly T[], fallback: T): T => allowed.find(entry => entry === value) ?? fallback;

const SEMANTIC: readonly SemanticState[] = ['pending', 'partial', 'ready', 'failed', 'disabled'];
const FRESHNESS: readonly FreshnessState[] = ['unregistered', 'unavailable', 'idle', 'pending', 'running', 'paused', 'blocked', 'failed'];

/** Published readiness and the saved vector policy are independent observations. */
export function semanticExplanation(status: Pick<StatusSummary, 'semantic' | 'vectorsEnabled' | 'localModelReady'> | undefined): { title: string; detail: string } {
    if (!status) { return { title: 'Semantic status not checked', detail: 'Refresh published status to check coverage and model configuration.' }; }
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
export function statusSummary(payload: Value, observedAt = Date.now()): StatusSummary {
    const indexState: IndexState = payload.index_state === 'available' ? 'available' : payload.index_state === 'unavailable' ? 'unavailable' : 'unknown';
    const publication = obj(payload.publication), semantic = obj(payload.semantic_readiness), freshness = obj(payload.freshness);
    const runtime = obj(payload.runtime), work = obj(payload.work), progress = obj(work.progress), indexError = obj(payload.index_error);
    const runtimeVersions = ([['MCP frontend (airk)', 'mcp_frontend'], ['Index worker (airk)', 'index_worker']] as const).map(([label, key]): readonly [string, string] => {
        const observed = obj(runtime[key]), process = obj(observed.process);
        const version = typeof process.version === 'string' ? process.version.slice(0, 64) : 'not observed';
        const basis = typeof observed.basis === 'string' ? observed.basis.slice(0, 80) : 'no process observation';
        return [label, `${version} · ${basis}${count(process.process_id) !== undefined ? ` · PID ${process.process_id}` : ''}`];
    });
    const summary: StatusSummary = {
        indexState,
        semantic: pick(semantic.state, SEMANTIC, 'unknown'),
        freshness: pick(freshness.state, FRESHNESS, 'unknown'),
        observedAt,
        ...(typeof indexError.code === 'string' ? { indexError: indexError.code.slice(0, 128) } : {}),
        ...(typeof publication.token === 'string' ? { publication: publication.token } : {}),
        ...(count(payload.files) !== undefined ? { files: count(payload.files) } : {}),
        ...(count(payload.chunks) !== undefined ? { chunks: count(payload.chunks) } : {}),
        ...(count(semantic.completed_artifacts) !== undefined ? { semanticCompleted: count(semantic.completed_artifacts) } : {}),
        ...(count(semantic.selected_artifacts) !== undefined ? { semanticSelected: count(semantic.selected_artifacts) } : {}),
        ...(typeof freshness.live_source_verified === 'boolean' ? { liveSourceVerified: freshness.live_source_verified } : {}),
        ...(typeof payload.vectors_enabled === 'boolean' ? { vectorsEnabled: payload.vectors_enabled } : {}),
        ...(typeof payload.local_model_ready === 'boolean' ? { localModelReady: payload.local_model_ready } : {}),
        ...('versions_match' in runtime ? { versionsMatch: typeof runtime.versions_match === 'boolean' ? runtime.versions_match : null } : {}),
        ...(runtime.mcp_frontend || runtime.index_worker ? { runtimeVersions } : {}),
        ...(typeof progress.phase === 'string' ? { workPhase: progress.phase.slice(0, 64) } : {}),
        ...(typeof work.project_paused === 'boolean' ? { workPaused: work.project_paused } : {}),
    };
    return summary;
}

/** One word for the status bar and pill: the worst current condition, never a merged "OK". */
export function headline(connection: ConnectionState, status: StatusSummary | undefined, projects: number): { word: string; severity: 'ok' | 'warn' | 'error' | 'info' } {
    if (connection === 'untrusted') { return { word: 'untrusted', severity: 'warn' }; }
    if (connection === 'incompatible') { return { word: 'incompatible', severity: 'error' }; }
    if (connection === 'disconnected') { return { word: 'disconnected', severity: 'error' }; }
    if (connection === 'connecting') { return { word: 'connecting', severity: 'info' }; }
    if (!projects) { return { word: 'no projects', severity: 'info' }; }
    if (!status) { return { word: connection === 'degraded' ? 'degraded' : 'ready', severity: connection === 'degraded' ? 'warn' : 'info' }; }
    if (status.indexState !== 'available') { return { word: 'index unavailable', severity: 'warn' }; }
    if (status.freshness === 'failed' || status.freshness === 'blocked' || status.semantic === 'failed') { return { word: 'attention', severity: 'error' }; }
    if (connection === 'degraded') { return { word: 'degraded', severity: 'warn' }; }
    if (status.freshness === 'running' || status.freshness === 'pending') { return { word: 'indexing', severity: 'info' }; }
    return { word: 'ready', severity: 'ok' };
}
