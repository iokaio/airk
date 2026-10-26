// SPDX-License-Identifier: Apache-2.0
import type { HitView, PassageView, SearchResultView, SemanticState, WebviewMessageOf } from './explorerContracts';
import { explorerError } from './explorerErrors';
import { EvidenceFailure } from './explorerEvidence';

type Value = Record<string, unknown>;
const obj = (value: unknown): Value => value && typeof value === 'object' && !Array.isArray(value) ? value as Value : {};
const line = (value: unknown): number | undefined => typeof value === 'number' && Number.isSafeInteger(value) && value >= 1 ? value : undefined;

/** One cited location the webview may refer to by an opaque handle. Paths never come back from the webview. */
export interface EvidenceRef {
    readonly project: string;
    readonly path: string;
    readonly startLine: number;
    readonly endLine: number;
    readonly contentHash: string;
    readonly publication?: string;
}

/** Bounded, insertion-ordered handle registry; the oldest handles expire first. */
export class HandleRegistry<T extends { readonly project: string }> {
    private readonly entries = new Map<string, T>();
    private counter = 0;
    constructor(private readonly prefix: string, private readonly limit = 2000) {}
    add(value: T): string {
        const id = `${this.prefix}${(++this.counter).toString(36)}`;
        this.entries.set(id, value);
        while (this.entries.size > this.limit) { this.entries.delete(this.entries.keys().next().value!); }
        return id;
    }
    get(id: string, project: string | undefined): T {
        const value = this.entries.get(id);
        if (!value || value.project !== project) { throw new EvidenceFailure(explorerError('UNKNOWN_REFERENCE')); }
        return value;
    }
    clear(): void { this.entries.clear(); }
}
/** Handles to cited locations ("h…"). */
export class RefRegistry extends HandleRegistry<EvidenceRef> {
    constructor(limit = 2000) { super('h', limit); }
}

/** A collection-relative path prefix: no absolute roots, drive letters, parent segments or control characters. */
export function relativePrefix(value: string | undefined): string | undefined {
    if (value === undefined || value.trim() === '') { return undefined; }
    const prefix = value.trim().replaceAll('\\', '/');
    if (prefix.startsWith('/') || /^[A-Za-z]:/.test(prefix) || prefix.split('/').some(part => part === '..')
        || /[\u0000-\u001f\u007f]/.test(prefix) || prefix.length > 1024) {
        throw new EvidenceFailure(explorerError('INVALID_MESSAGE', 'Path scope must be relative to the project.'));
    }
    return prefix;
}

export function searchArgs(message: WebviewMessageOf<'search.run'>): Value {
    const prefix = relativePrefix(message.pathPrefix);
    return { query: message.query, limit: message.limit, max_bytes: message.maxBytes, full_chunks: message.fullChunks,
        ...(message.mode === 'policy' ? {} : { mode: message.mode }), ...(prefix ? { path_prefix: prefix } : {}) };
}

/**
 * Describe what the search actually did. An explicit mode is reported as sent; the
 * collection policy is resolved from the reply's semantic readiness when present,
 * otherwise from the last observed status, and stays "unknown" if neither is known.
 */
export function appliedMode(requested: string, payload: Value, fallback: { semantic?: SemanticState; vectorsEnabled?: boolean }): { mode: string; ordering: SearchResultView['ordering']; semantic?: SemanticState } {
    const readiness = obj(payload.semantic_readiness);
    const semantic = (typeof readiness.state === 'string' ? readiness.state : fallback.semantic) as SemanticState | undefined;
    if (requested !== 'policy') { return { mode: requested, ordering: requested === 'lexical' ? 'index_identity' : 'fused_rank', ...(semantic ? { semantic } : {}) }; }
    const policy = typeof readiness.default_search_policy === 'string' ? readiness.default_search_policy.replaceAll('_', ' ') : 'collection policy';
    if (fallback.vectorsEnabled === false || semantic === 'disabled') { return { mode: `${policy} → lexical (vectors disabled)`, ordering: 'index_identity', semantic: 'disabled' }; }
    if (semantic === 'pending' || semantic === 'failed') { return { mode: `${policy} → lexical while vectors are ${semantic}`, ordering: 'index_identity', semantic }; }
    if (semantic === 'partial' || semantic === 'ready') { return { mode: `${policy} → hybrid (${semantic} vectors)`, ordering: 'fused_rank', semantic }; }
    return { mode: policy, ordering: 'unknown' };
}

function hitViews(project: string, payload: Value, publication: string | undefined, refs: RefRegistry): HitView[] {
    const hits = Array.isArray(payload.hits) ? payload.hits : [];
    return hits.flatMap(raw => {
        const hit = obj(raw), citation = obj(hit.citation);
        const path = typeof citation.path === 'string' ? citation.path : undefined;
        const start = line(citation.start_line), end = line(citation.end_line);
        const hash = typeof citation.content_hash === 'string' && /^[0-9a-f]{64}$/.test(citation.content_hash) ? citation.content_hash : undefined;
        if (!path || !start || !end || !hash) { return []; }
        const ref = refs.add({ project, path, startLine: start, endLine: end, contentHash: hash, ...(publication ? { publication } : {}) });
        const location = obj(citation.location);
        const lines = obj(location.Lines);
        return [{ ref, path, startLine: start, endLine: end, text: typeof hit.text === 'string' ? hit.text : '',
            truncated: hit.truncated === true, contentHash: hash, evidenceClass: 'E1',
            ...(Object.keys(location).length && !lines.start ? { location: JSON.stringify(location).slice(0, 200) } : {}) } satisfies HitView];
    });
}

export function searchView(project: string, message: WebviewMessageOf<'search.run'>, payload: Value, publication: string | undefined,
    refs: RefRegistry, fallback: { semantic?: SemanticState; vectorsEnabled?: boolean }): SearchResultView {
    const applied = appliedMode(message.mode, payload, fallback);
    const hits = hitViews(project, payload, publication, refs);
    const warnings = applied.ordering === 'index_identity'
        ? ['Lexical results are ordered by index identity, not relevance. All query words must appear.']
        : applied.ordering === 'fused_rank' ? ['Vector and hybrid results are similarity candidates, not proof of meaning or relevance.'] : [];
    if (hits.some(hit => hit.truncated)) { warnings.push('Some excerpts are truncated; expand a result to read more of the indexed lines.'); }
    return { query: message.query, appliedMode: applied.mode, ordering: applied.ordering, returned: hits.length, limit: message.limit,
        truncated: payload.truncated === true, ...(publication ? { publication } : {}), ...(applied.semantic ? { semantic: applied.semantic } : {}), hits, warnings };
}

export function passageView(project: string, payload: Value, publication: string | undefined, refs: RefRegistry): PassageView {
    const [hit] = hitViews(project, payload, publication, refs);
    if (!hit) { throw new EvidenceFailure(explorerError('TOOL_ERROR', 'AIRK returned no indexed lines for this location.')); }
    return { ref: hit.ref, path: hit.path, startLine: hit.startLine, endLine: hit.endLine, text: hit.text, truncated: hit.truncated || payload.truncated === true,
        ...(publication ? { publication } : {}) };
}
