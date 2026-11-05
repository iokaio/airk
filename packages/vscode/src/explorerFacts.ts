// SPDX-License-Identifier: Apache-2.0
import type { EvidenceClass, FactItemView, FactListView, ProvenanceView } from './explorerContracts';
import { explorerError } from './explorerErrors';
import { EvidenceClient, EvidenceFailure, NativeTool } from './explorerEvidence';
import { combinedRows, HandleRegistry, locationLabel, originArgs, QualifiedOrigin, RefRegistry, relativePrefix } from './explorerSearch';
import { readResultBody } from './sourceVerification';
import { comparePublicationVector } from './publications';

type Value = Record<string, unknown>;
const obj = (value: unknown): Value => value && typeof value === 'object' && !Array.isArray(value) ? value as Value : {};
const list = (value: unknown): unknown[] => Array.isArray(value) ? value : [];
const text = (value: unknown, max = 4096): string | undefined => typeof value === 'string' ? value.slice(0, max) : undefined;
const line = (value: unknown): number | undefined => typeof value === 'number' && Number.isSafeInteger(value) && value >= 1 ? value : undefined;
const HASH = /^[0-9a-f]{64}$/;

export interface SymbolHandle { readonly project: string; readonly symbolId: string; readonly origin?: QualifiedOrigin }
/** A continuation: the exact tool and arguments AIRK paged, bound to one publication. */
export interface CursorHandle { readonly project: string; readonly tool: NativeTool; readonly args: Value; readonly cursor: string; readonly publication?: string; readonly limit: number; readonly publicationVector?: Value }

export class FactHandles {
    readonly symbols = new HandleRegistry<SymbolHandle>('s');
    readonly cursors = new HandleRegistry<CursorHandle>('c', 200);
    constructor(readonly refs: RefRegistry) {}
    clear(): void { this.symbols.clear(); this.cursors.clear(); }
}

/** Syntax facts are E2; producer assertions (for example imported SCIP) are E4. */
export function evidenceClass(basis: string | undefined): EvidenceClass {
    return basis === undefined || basis === 'syntax' || basis === 'native' ? 'E2' : 'E4';
}

function provenanceList(payload: Value): ProvenanceView[] {
    return list(payload.provenance).map(entry => {
        const value = obj(entry);
        return { producer: text(value.producer, 256) ?? 'unknown producer', ...(text(value.basis, 64) ? { basis: text(value.basis, 64) } : {}),
            ...(text(value.coverage, 512) ? { coverage: text(value.coverage, 512) } : {}) };
    });
}

/** Register a cited location and return its handle, or undefined when the citation is incomplete. */
function citationRef(handles: FactHandles, project: string, publication: string | undefined, citation: Value, origin?: QualifiedOrigin): { ref?: string; path?: string; startLine?: number; endLine?: number; location?: string } {
    const path = text(citation.path, 4096), start = line(citation.start_line), end = line(citation.end_line), hash = citation.content_hash;
    if (!path) { return {}; }
    const location = locationLabel(citation.location);
    const extra = location ? { location } : {};
    if (!start || !end || typeof hash !== 'string' || !HASH.test(hash)) { return { path, ...(start ? { startLine: start } : {}), ...(end ? { endLine: end } : {}), ...extra }; }
    const ref = handles.refs.add({ project, path, startLine: start, endLine: end, contentHash: hash, ...(publication ? { publication } : {}), ...(origin ? { origin } : {}) });
    return { ref, path, startLine: start, endLine: end, ...extra };
}

function symbolRef(handles: FactHandles, project: string, id: unknown, origin?: QualifiedOrigin): string | undefined {
    return typeof id === 'string' && id.length <= 1024 && /^(syntax|scip|file):/.test(id) ? handles.symbols.add({ project, symbolId: id, ...(origin ? { origin } : {}) }) : undefined;
}

interface Request { readonly tool: NativeTool; readonly args: Value; readonly limit: number; readonly origin?: QualifiedOrigin; readonly continuation?: boolean }

/** Map any of the fact-shaped replies (minimal tables, code facts, references, relationships, document facts). */
export function factList(request: Request, payload: Value, publication: string | undefined, project: string, handles: FactHandles, rawPayload: Value = payload): FactListView {
    const combined = combinedRows(payload);
    if (combined) {
        const views = combined.map(({ row, origin, body }) => {
            const table = Object.keys(obj(body.minimal)).length ? obj(body.minimal) : body;
            const item = request.tool === 'airk_list_files' ? { ...row, citation: row.citation ?? { path: row.path }, kind: 'file' } : row;
            const isolated = request.tool === 'airk_references' ? { ...body, cursor: undefined, evidence: { ...obj(body.evidence), hits: [row] } } : { ...table, cursor: undefined, items: [item] };
            return factList({ ...request, args: { ...request.args, ...originArgs(origin) }, origin, continuation: false }, isolated, origin.publication.token, project, handles);
        });
        const items = views.flatMap((view, index) => view.items.map(item => ({ ...item, indexId: combined[index].origin.index_id, projectIds: combined[index].origin.project_ids })));
        const more = registerCursor(request, payload, publication, project, handles);
        return { tool: request.tool, items, returned: items.length, limit: request.limit, truncated: payload.truncated === true || payload.partial === true || !!more,
            ...(more ? { more } : {}),
            provenance: views.flatMap(view => view.provenance), notes: [...new Set(views.flatMap(view => view.notes)),
                'Combined member publications, policies, failure/retry details and source-verification receipts remain in Complete native response.',
                ...(typeof payload.continuation_unavailable === 'string' ? [`Combined continuation unavailable: ${payload.continuation_unavailable.slice(0, 256)}.`] : []),
                ...(payload.partial === true ? ['Selected members are unavailable or omitted by the shared budget; this is a partial result.'] : [])], nativeResult: rawPayload };
    }
    const provenance = provenanceList(payload);
    const producerFor = (index: unknown) => typeof index === 'number' ? provenance[index] : undefined;
    const notes: string[] = [];
    let items: FactItemView[] = [];
    if (request.tool === 'airk_references') {
        const evidence = obj(payload.evidence);
        if (text(payload.resolution)) { notes.push(`AIRK: ${text(payload.resolution, 512)}`); }
        items = list(evidence.hits).flatMap(raw => {
            const hit = obj(raw), located = citationRef(handles, project, publication, obj(hit.citation), request.origin);
            return located.path ? [{ ...located, path: located.path, kind: 'occurrence', name: String(request.args.name), evidenceClass: 'E1' as const,
                detail: text(hit.text, 2048) ?? '' }] : [];
        });
        return finish(request, payload, obj(payload.evidence), items, provenance, notes, publication, project, handles, rawPayload);
    }
    if (request.tool === 'airk_relationships') {
        if (text(payload.coverage)) { notes.push(text(payload.coverage, 512)!); }
        items = list(payload.items).flatMap(raw => {
            const edge = obj(raw), located = citationRef(handles, project, publication, obj(edge.citation), request.origin);
            if (!located.path) { return []; }
            const target = text(edge.target, 512) ?? '';
            const producer = producerFor(edge.provenance), targetRef = symbolRef(handles, project, edge.target, request.origin);
            return [{ ...located, path: located.path, kind: text(edge.kind, 64) ?? 'edge', name: target, target,
                ...(targetRef ? { symbolRef: targetRef } : {}),
                resolution: text(edge.resolution, 64) ?? 'unresolved', evidenceClass: evidenceClass(text(edge.basis, 64)),
                ...(text(edge.basis, 64) ? { basis: text(edge.basis, 64) } : {}), ...(producer ? { producer: producer.producer } : {}) }];
        });
        return finish(request, payload, payload, items, provenance, notes, publication, project, handles, rawPayload);
    }
    const files = list(payload.files).map(obj);
    items = list(payload.items).flatMap(raw => {
        const item = obj(raw);
        let located: { ref?: string; path?: string; startLine?: number; endLine?: number };
        if (item.citation) { located = citationRef(handles, project, publication, obj(item.citation), request.origin); }
        else {
            const file = typeof item.file === 'number' ? files[item.file] : request.tool === 'airk_outline' ? files[0] : undefined;
            const lines = list(item.lines);
            located = citationRef(handles, project, publication, { path: file?.path, content_hash: file?.content_hash, start_line: lines[0], end_line: lines[1] }, request.origin);
        }
        if (!located.path) { return []; }
        const code = obj(item.code);
        const basis = text(item.basis ?? code.basis, 64) ?? text(producerFor(item.provenance ?? code.provenance)?.basis, 64);
        const producer = producerFor(item.provenance ?? code.provenance)?.producer;
        const ownRef = symbolRef(handles, project, item.symbol_id ?? code.symbol_id, request.origin);
        const heading = list(item.heading_path).map(entry => text(entry, 256)).filter((entry): entry is string => !!entry);
        const view: FactItemView = {
            ...located, path: located.path, kind: text(item.kind, 64) ?? 'fact', name: text(item.name, 1024) ?? '',
            evidenceClass: request.tool === 'airk_document_facts' ? 'E1' : evidenceClass(basis),
            ...(text(item.signature ?? code.signature, 1024) ? { signature: text(item.signature ?? code.signature, 1024) } : {}),
            ...(text(item.qualified_name ?? code.qualified_name, 1024) ? { qualifiedName: text(item.qualified_name ?? code.qualified_name, 1024) } : {}),
            ...(ownRef ? { symbolRef: ownRef } : {}),
            ...(basis ? { basis } : {}), ...(producer ? { producer } : {}),
            ...(heading.length ? { heading } : {}),
            ...(text(item.document_basis, 64) ? { detail: `basis: ${text(item.document_basis, 64)}` } : {}),
        };
        return [view];
    });
    if (request.tool === 'airk_document_facts') { notes.push('Document facts are extraction candidates with their original locations; they are not verified requirements.'); }
    return finish(request, payload, payload, items, provenance, notes, publication, project, handles, rawPayload);
}

function finish(request: Request, payload: Value, container: Value, items: FactItemView[], provenance: ProvenanceView[], notes: string[],
    publication: string | undefined, project: string, handles: FactHandles, rawPayload: Value): FactListView {
    const more = registerCursor(request, payload, publication, project, handles);
    return { tool: request.tool, items, returned: items.length, limit: request.limit, truncated: container.truncated === true || payload.truncated === true || obj(payload.page).more === true || !!more,
        ...(publication ? { publication } : {}), ...(more ? { more } : {}), provenance, notes, nativeResult: rawPayload };
}

/** Keep the opaque canonical composite cursor, never an arbitrary child cursor.
 * The native owner binds its exact arguments and every selected publication. */
function registerCursor(request: Request, payload: Value, publication: string | undefined, project: string, handles: FactHandles): string | undefined {
    if (request.continuation === false) { return undefined; }
    const top = payload.cursor, page = obj(payload.page).cursor;
    if (top != null && page != null) { throw new EvidenceFailure(explorerError('TOOL_ERROR', 'The native response contains ambiguous continuation fields.')); }
    const cursor = top ?? page;
    if (cursor === undefined || cursor === null) { return undefined; }
    const combined = payload.schema === 'airk.combined-read/1';
    const maximum = 32768;
    if (typeof cursor !== 'string' || !cursor.length || new TextEncoder().encode(cursor).length > maximum) {
        throw new EvidenceFailure(explorerError('TOOL_ERROR', `The native continuation exceeded its ${maximum}-byte allowance.`));
    }
    const vector = combined ? obj(payload.publication_vector) : undefined;
    if (vector && comparePublicationVector({ authority: project, vector }, { authority: project, vector }) !== 'equal') {
        throw new EvidenceFailure(explorerError('EVIDENCE_MISMATCH', 'The combined continuation has no complete member publication vector.'));
    }
    return handles.cursors.add({ project, tool: request.tool, args: structuredClone(request.args), cursor, limit: request.limit,
        ...(publication ? { publication } : {}), ...(vector ? { publicationVector: structuredClone(vector) } : {}) });
}

/** Run one fact request and map it. Cursor continuations are rejected once the publication changed. */
export async function runFacts(session: EvidenceClient, handles: FactHandles, project: string, request: Request): Promise<FactListView> {
    try {
        const result = await session.call(request.tool, request.args, { timeout: 30000, supersede: `facts:${request.tool}` });
        return factList(request, result.payload, result.publication, project, handles, result.rawPayload);
    } catch (error) {
        if (error instanceof EvidenceFailure && error.nativeResponse) {
            const body = error.nativePayload ?? readResultBody(error.nativeResponse);
            if (error.nativePayload && !Object.keys(body).length) {
                return { tool: request.tool, items: [], returned: 0, limit: request.limit, truncated: true,
                    provenance: [], notes: ['Native metadata could not be decoded. No evidence was interpreted; the complete original response remains below.'],
                    nativeFailure: true, nativeResult: error.nativeResponse };
            }
            if (body.schema === 'airk.combined-read/1') {
                const view = factList(request, body, undefined, project, handles);
                return { ...view, nativeFailure: true, nativeResult: error.nativeResponse,
                    notes: ['The native read failed. Complete attempted member data and typed failure remain below; these are retained observations.', ...view.notes] };
            }
        }
        throw error;
    }
}

export async function continueFacts(session: EvidenceClient, handles: FactHandles, project: string, more: string): Promise<FactListView> {
    const handle = handles.cursors.get(more, project);
    const result = await session.call(handle.tool, { ...handle.args, cursor: handle.cursor }, { timeout: 30000 });
    if (handle.publicationVector && (result.payload.schema !== 'airk.combined-read/1'
        || comparePublicationVector({ authority: project, vector: handle.publicationVector }, { authority: project, vector: result.payload.publication_vector }) !== 'equal')) {
        throw new EvidenceFailure(explorerError('EVIDENCE_MISMATCH', 'A selected member publication or query scope changed; run the combined query again instead of continuing.'));
    }
    if (handle.publication && handle.publication !== result.publication) {
        throw new EvidenceFailure(explorerError('EVIDENCE_MISMATCH', 'The index was republished; run the query again instead of continuing.'));
    }
    return factList(handle, result.payload, result.publication, project, handles, result.rawPayload);
}

export function scoped(args: Value, pathPrefix: string | undefined): Value {
    const prefix = relativePrefix(pathPrefix);
    return prefix ? { ...args, path_prefix: prefix } : args;
}
