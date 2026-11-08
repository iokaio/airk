// SPDX-License-Identifier: Apache-2.0
import type { HitView, PassageView, SearchResultView, SemanticState, WebviewMessageOf } from './explorerContracts';
import { explorerError } from './explorerErrors';
import { EvidenceFailure } from './explorerEvidence';
import { evidenceRows } from './workbenchRender';
import { readResultBody } from './sourceVerification';

type Value = Record<string, unknown>;
const obj = (value: unknown): Value => value && typeof value === 'object' && !Array.isArray(value) ? value as Value : {};
const line = (value: unknown): number | undefined => typeof value === 'number' && Number.isSafeInteger(value) && value >= 1 ? value : undefined;

export interface QualifiedOrigin {
    readonly index_id: string; readonly source_root_id: string; readonly project_ids: readonly string[];
    readonly source_relative_path: string; readonly publication: Value & { token: string };
}
export function originArgs(origin?: QualifiedOrigin): Value {
    return origin ? { project_ids: [...origin.project_ids], index_ids: [origin.index_id], origin_publication: origin.publication.token } : {};
}
export function combinedRows(payload: Value): { row: Value; origin: QualifiedOrigin; body: Value }[] | undefined {
    if (payload.schema !== 'airk.combined-read/1') { return undefined; }
    if (!Array.isArray(payload.ranking) || payload.ranking.length > 100 || !Array.isArray(payload.members) || payload.members.length > 32) {
        throw new EvidenceFailure(explorerError('TOOL_ERROR', 'The combined response has incomplete member or ranking metadata.'));
    }
    const rows = evidenceRows(payload);
    const members = payload.members.map(obj);
    if (rows.length !== payload.ranking.length) { throw new EvidenceFailure(explorerError('EVIDENCE_MISMATCH', 'A combined citation lost its originating index.')); }
    return rows.map(row => {
        const origin = obj(row.origin);
        if (typeof origin.index_id !== 'string' || !/^idx_[0-9a-f]{32}$/.test(origin.index_id)
            || typeof origin.source_root_id !== 'string' || !/^src_[0-9a-f]{32}$/.test(origin.source_root_id)
            || !Array.isArray(origin.project_ids) || !origin.project_ids.length || origin.project_ids.length > 32
            || new Set(origin.project_ids).size !== origin.project_ids.length || origin.project_ids.some(id => typeof id !== 'string' || !/^prj_[0-9a-f]{32}$/.test(id))
            || typeof origin.source_relative_path !== 'string' || typeof obj(origin.publication).token !== 'string') {
            throw new EvidenceFailure(explorerError('EVIDENCE_MISMATCH', 'The combined citation has no complete originating member identity.'));
        }
        const member = members.find(member => member.index_id === origin.index_id);
        if (!member || member.is_error === true || member.outcome === 'failed' || member.outcome === 'unqueried') {
            throw new EvidenceFailure(explorerError('EVIDENCE_MISMATCH', 'The combined citation does not belong to a usable selected member.'));
        }
        const body = readResultBody(member.result), evidence = obj(body.evidence);
        const table = Object.keys(obj(body.minimal)).length ? obj(body.minimal) : Object.keys(evidence).length ? evidence : body;
        if (!row.citation && Array.isArray(table.files)) {
            const file = obj(table.files[typeof row.file === 'number' ? row.file : 0]), lines = Array.isArray(row.lines) ? row.lines : [];
            row = { ...row, citation: { path: file.path, content_hash: file.content_hash, start_line: lines[0], end_line: lines[1], location: file.location } };
        }
        return { row, origin: origin as unknown as QualifiedOrigin, body };
    });
}

/** Follow-ups may unwrap exactly one explicitly originating member. */
export function originatingResult(payload: Value, origin?: QualifiedOrigin): { payload: Value; publication?: string; nativeResult?: Value } {
    if (payload.schema !== 'airk.combined-read/1') { return { payload, ...(typeof obj(payload.publication).token === 'string' ? { publication: obj(payload.publication).token as string } : {}) }; }
    const members = Array.isArray(payload.members) ? payload.members.map(obj) : [];
    const body = readResultBody(members[0]?.result), requiredFailure = obj(body.error).code === 'verification_required_unmet';
    if (!origin || members.length !== 1 || members[0].index_id !== origin.index_id
        || (members[0].is_error === true || members[0].outcome !== 'complete') && !requiredFailure) {
        throw new EvidenceFailure(explorerError('CAPABILITY_DISABLED', 'Choose one originating index from qualified evidence for this read.'));
    }
    const token = obj(body.publication).token ?? obj(obj(body.read).publication).token ?? obj(obj(body.evidence).publication).token;
    if (token !== origin.publication.token) { throw new EvidenceFailure(explorerError('EVIDENCE_MISMATCH', 'The originating member publication changed.')); }
    const source = obj(body.source);
    if (source.root !== undefined || source.relative_path !== undefined) {
        const rows = combinedRows(payload), matching = rows?.filter(row => row.origin.index_id === origin.index_id
            && row.origin.source_root_id === origin.source_root_id && row.origin.source_relative_path === origin.source_relative_path);
        if (typeof source.root !== 'string' || source.relative_path !== origin.source_relative_path || matching?.length !== 1) {
            throw new EvidenceFailure(explorerError('EVIDENCE_MISMATCH', 'The native source projection did not confirm the originating source-root identity.'));
        }
    }
    return { payload: body, publication: String(token), nativeResult: payload };
}

/** One cited location the webview may refer to by an opaque handle. Paths never come back from the webview. */
export interface EvidenceRef {
    readonly project: string;
    readonly path: string;
    readonly startLine: number;
    readonly endLine: number;
    readonly contentHash: string;
    readonly publication?: string;
    readonly origin?: QualifiedOrigin;
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

/**
 * Human label for an AIRK DocumentLocation. Line locations return undefined (the line
 * numbers are shown); extracted documents cite their original page, slide, row or block.
 */
export function locationLabel(location: unknown): string | undefined {
    const value = obj(location);
    const [kind] = Object.keys(value);
    const fields = obj(value[kind]);
    const number = (key: string) => typeof fields[key] === 'number' ? String(fields[key]) : '?';
    switch (kind) {
        case 'Page': return `page ${number('number')}`;
        case 'Slide': return `slide ${number('number')}`;
        case 'Block': return `block ${number('number')}`;
        case 'TableRow': return `table ${number('table')} row ${number('row')}`;
        case 'WorksheetRow': return `sheet ${typeof fields.sheet === 'string' ? fields.sheet.slice(0, 128) : '?'} row ${number('row')}`;
        case 'Mapped': return 'mapped region';
        default: return undefined;
    }
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
    if (semantic === 'unavailable') { return { mode: requested === 'policy' ? 'collection policy (semantic unavailable)' : requested,
        ordering: requested === 'lexical' ? 'index_identity' : 'unknown', semantic }; }
    if (requested !== 'policy') { return { mode: requested, ordering: requested === 'lexical' ? 'index_identity' : 'fused_rank', ...(semantic ? { semantic } : {}) }; }
    const policy = typeof readiness.default_search_policy === 'string' ? readiness.default_search_policy.replaceAll('_', ' ') : 'collection policy';
    if (fallback.vectorsEnabled === false || semantic === 'disabled') { return { mode: `${policy} → lexical (vectors disabled)`, ordering: 'index_identity', semantic: 'disabled' }; }
    if (semantic === 'pending' || semantic === 'failed') { return { mode: `${policy} → lexical while vectors are ${semantic}`, ordering: 'index_identity', semantic }; }
    if (semantic === 'partial' || semantic === 'ready') { return { mode: `${policy} → hybrid (${semantic} vectors)`, ordering: 'fused_rank', semantic }; }
    return { mode: policy, ordering: 'unknown' };
}

function hitViews(project: string, payload: Value, publication: string | undefined, refs: RefRegistry): HitView[] {
    const qualified = combinedRows(payload);
    const hits = qualified ? qualified.map(({ row, origin }) => ({ ...row, origin })) : Array.isArray(payload.hits) ? payload.hits : [];
    return hits.flatMap(raw => {
        const hit = obj(raw), citation = obj(hit.citation);
        const path = typeof citation.path === 'string' ? citation.path : undefined;
        const start = line(citation.start_line), end = line(citation.end_line);
        const hash = typeof citation.content_hash === 'string' && /^[0-9a-f]{64}$/.test(citation.content_hash) ? citation.content_hash : undefined;
        if (!path || !start || !end || !hash) { return []; }
        const origin = qualified ? hit.origin as QualifiedOrigin : undefined;
        const token = origin?.publication.token ?? publication;
        const ref = refs.add({ project, path, startLine: start, endLine: end, contentHash: hash, ...(token ? { publication: token } : {}), ...(origin ? { origin } : {}) });
        const location = locationLabel(citation.location);
        return [{ ref, path, startLine: start, endLine: end, text: typeof hit.text === 'string' ? hit.text : '',
            truncated: hit.truncated === true, contentHash: hash, evidenceClass: 'E1', ...(location ? { location } : {}), ...(origin ? { indexId: origin.index_id, projectIds: origin.project_ids } : {}) } satisfies HitView];
    });
}

export function searchView(project: string, message: WebviewMessageOf<'search.run'>, payload: Value, publication: string | undefined,
    refs: RefRegistry, fallback: { semantic?: SemanticState; vectorsEnabled?: boolean }, rawPayload: Value = payload): SearchResultView {
    const combined = payload.schema === 'airk.combined-read/1';
    const applied: ReturnType<typeof appliedMode> = combined ? { mode: message.mode === 'policy' ? 'independent member policies' : message.mode, ordering: 'fused_rank' } : appliedMode(message.mode, payload, fallback);
    const hits = hitViews(project, payload, publication, refs);
    const warnings = applied.ordering === 'index_identity'
        ? ['Lexical results are ordered by index identity, not relevance. All query words must appear.']
        : applied.ordering === 'fused_rank' ? ['Semantic and hybrid results are similarity candidates, not proof of meaning or relevance.'] : [];
    if (applied.ordering === 'fused_rank' && applied.semantic && applied.semantic !== 'ready') {
        warnings.push(`Semantic coverage is ${applied.semantic}; results may be incomplete or unavailable.`);
    }
    if (hits.some(hit => hit.truncated)) { warnings.push('Some excerpts are truncated; expand a result to read more of the indexed lines.'); }
    if (combined) {
        warnings.push('Combined results use equal-weight reciprocal local ranks. Member publications are independent observations; complete execution policies, errors, retries and saved-source receipts remain in Complete native response.');
        if (payload.partial === true) { warnings.push('Some selected members failed or were not queried within the shared budget. This is a partial result.'); }
    }
    return { query: message.query, appliedMode: applied.mode, ordering: applied.ordering, returned: hits.length, limit: message.limit,
        truncated: payload.truncated === true || payload.partial === true, ...(publication ? { publication } : {}), ...(applied.semantic ? { semantic: applied.semantic } : {}), hits, warnings,
        ...(combined || applied.semantic === 'unavailable' || payload.legacy_adoption !== undefined ? { nativeResult: rawPayload } : {}) };
}

export function passageView(project: string, payload: Value, publication: string | undefined, refs: RefRegistry): PassageView {
    const [hit] = hitViews(project, payload, publication, refs);
    if (!hit) { throw new EvidenceFailure(explorerError('TOOL_ERROR', 'AIRK returned no indexed lines for this location.')); }
    return { ref: hit.ref, path: hit.path, startLine: hit.startLine, endLine: hit.endLine, text: hit.text, truncated: hit.truncated || payload.truncated === true,
        ...(refs.get(hit.ref, project).publication ? { publication: refs.get(hit.ref, project).publication } : {}) };
}
