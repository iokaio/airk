// SPDX-License-Identifier: Apache-2.0
import type { EvidenceClass, FactItemView, FactListView, ProvenanceView } from './explorerContracts';
import { explorerError } from './explorerErrors';
import { EvidenceClient, EvidenceFailure, NativeTool } from './explorerEvidence';
import { HandleRegistry, RefRegistry, relativePrefix } from './explorerSearch';

type Value = Record<string, unknown>;
const obj = (value: unknown): Value => value && typeof value === 'object' && !Array.isArray(value) ? value as Value : {};
const list = (value: unknown): unknown[] => Array.isArray(value) ? value : [];
const text = (value: unknown, max = 4096): string | undefined => typeof value === 'string' ? value.slice(0, max) : undefined;
const line = (value: unknown): number | undefined => typeof value === 'number' && Number.isSafeInteger(value) && value >= 1 ? value : undefined;
const HASH = /^[0-9a-f]{64}$/;

export interface SymbolHandle { readonly project: string; readonly symbolId: string }
/** A continuation: the exact tool and arguments AIRK paged, bound to one publication. */
export interface CursorHandle { readonly project: string; readonly tool: NativeTool; readonly args: Value; readonly cursor: string; readonly publication?: string; readonly limit: number }

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
function citationRef(handles: FactHandles, project: string, publication: string | undefined, citation: Value): { ref?: string; path?: string; startLine?: number; endLine?: number } {
    const path = text(citation.path, 4096), start = line(citation.start_line), end = line(citation.end_line), hash = citation.content_hash;
    if (!path) { return {}; }
    if (!start || !end || typeof hash !== 'string' || !HASH.test(hash)) { return { path, ...(start ? { startLine: start } : {}), ...(end ? { endLine: end } : {}) }; }
    const ref = handles.refs.add({ project, path, startLine: start, endLine: end, contentHash: hash, ...(publication ? { publication } : {}) });
    return { ref, path, startLine: start, endLine: end };
}

function symbolRef(handles: FactHandles, project: string, id: unknown): string | undefined {
    return typeof id === 'string' && id.length <= 1024 && /^(syntax|scip|file):/.test(id) ? handles.symbols.add({ project, symbolId: id }) : undefined;
}

interface Request { readonly tool: NativeTool; readonly args: Value; readonly limit: number }

/** Map any of the fact-shaped replies (minimal tables, code facts, references, relationships, document facts). */
export function factList(request: Request, payload: Value, publication: string | undefined, project: string, handles: FactHandles): FactListView {
    const provenance = provenanceList(payload);
    const producerFor = (index: unknown) => typeof index === 'number' ? provenance[index] : undefined;
    const notes: string[] = [];
    let items: FactItemView[] = [];
    if (request.tool === 'airk_references') {
        const evidence = obj(payload.evidence);
        if (text(payload.resolution)) { notes.push(`AIRK: ${text(payload.resolution, 512)}`); }
        items = list(evidence.hits).flatMap(raw => {
            const hit = obj(raw), located = citationRef(handles, project, publication, obj(hit.citation));
            return located.path ? [{ ...located, path: located.path, kind: 'occurrence', name: String(request.args.name), evidenceClass: 'E1' as const,
                detail: text(hit.text, 2048) ?? '' }] : [];
        });
        return finish(request, payload, obj(payload.evidence), items, provenance, notes, publication, project, handles);
    }
    if (request.tool === 'airk_relationships') {
        if (text(payload.coverage)) { notes.push(text(payload.coverage, 512)!); }
        items = list(payload.items).flatMap(raw => {
            const edge = obj(raw), located = citationRef(handles, project, publication, obj(edge.citation));
            if (!located.path) { return []; }
            const target = text(edge.target, 512) ?? '';
            const producer = producerFor(edge.provenance), targetRef = symbolRef(handles, project, edge.target);
            return [{ ...located, path: located.path, kind: text(edge.kind, 64) ?? 'edge', name: target, target,
                ...(targetRef ? { symbolRef: targetRef } : {}),
                resolution: text(edge.resolution, 64) ?? 'unresolved', evidenceClass: evidenceClass(text(edge.basis, 64)),
                ...(text(edge.basis, 64) ? { basis: text(edge.basis, 64) } : {}), ...(producer ? { producer: producer.producer } : {}) }];
        });
        return finish(request, payload, payload, items, provenance, notes, publication, project, handles);
    }
    const files = list(payload.files).map(obj);
    items = list(payload.items).flatMap(raw => {
        const item = obj(raw);
        let located: { ref?: string; path?: string; startLine?: number; endLine?: number };
        if (item.citation) { located = citationRef(handles, project, publication, obj(item.citation)); }
        else {
            const file = typeof item.file === 'number' ? files[item.file] : request.tool === 'airk_outline' ? files[0] : undefined;
            const lines = list(item.lines);
            located = citationRef(handles, project, publication, { path: file?.path, content_hash: file?.content_hash, start_line: lines[0], end_line: lines[1] });
        }
        if (!located.path) { return []; }
        const code = obj(item.code);
        const basis = text(item.basis ?? code.basis, 64) ?? text(producerFor(item.provenance ?? code.provenance)?.basis, 64);
        const producer = producerFor(item.provenance ?? code.provenance)?.producer;
        const ownRef = symbolRef(handles, project, item.symbol_id ?? code.symbol_id);
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
    return finish(request, payload, payload, items, provenance, notes, publication, project, handles);
}

function finish(request: Request, payload: Value, container: Value, items: FactItemView[], provenance: ProvenanceView[], notes: string[],
    publication: string | undefined, project: string, handles: FactHandles): FactListView {
    const cursor = typeof payload.cursor === 'string' && payload.cursor.length <= 8192 ? payload.cursor : undefined;
    const more = cursor ? handles.cursors.add({ project, tool: request.tool, args: request.args, cursor, limit: request.limit, ...(publication ? { publication } : {}) }) : undefined;
    return { tool: request.tool, items, returned: items.length, limit: request.limit, truncated: container.truncated === true || payload.truncated === true,
        ...(publication ? { publication } : {}), ...(more ? { more } : {}), provenance, notes };
}

/** Run one fact request and map it. Cursor continuations are rejected once the publication changed. */
export async function runFacts(session: EvidenceClient, handles: FactHandles, project: string, request: Request): Promise<FactListView> {
    const result = await session.call(request.tool, request.args, { timeout: 30000, supersede: `facts:${request.tool}` });
    return factList(request, result.payload, result.publication, project, handles);
}

export async function continueFacts(session: EvidenceClient, handles: FactHandles, project: string, more: string): Promise<FactListView> {
    const handle = handles.cursors.get(more, project);
    const result = await session.call(handle.tool, { ...handle.args, cursor: handle.cursor }, { timeout: 30000 });
    if (handle.publication && result.publication && handle.publication !== result.publication) {
        throw new EvidenceFailure(explorerError('EVIDENCE_MISMATCH', 'The index was republished; run the query again instead of continuing.'));
    }
    return factList(handle, result.payload, result.publication, project, handles);
}

export function scoped(args: Value, pathPrefix: string | undefined): Value {
    const prefix = relativePrefix(pathPrefix);
    return prefix ? { ...args, path_prefix: prefix } : args;
}
