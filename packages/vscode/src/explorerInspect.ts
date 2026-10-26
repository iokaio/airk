// SPDX-License-Identifier: Apache-2.0
import type { ChangesView, CitationCheckView, CitationResult, InspectView, LiveCheck } from './explorerContracts';
import { parseCitations } from './explorerContracts';
import { explorerError } from './explorerErrors';
import { EvidenceClient, EvidenceFailure } from './explorerEvidence';
import { RefRegistry } from './explorerSearch';
import { displayPath, liveCheck, sourceFile } from './explorerSource';

type Value = Record<string, unknown>;
const obj = (value: unknown): Value => value && typeof value === 'object' && !Array.isArray(value) ? value as Value : {};
const count = (value: unknown): number | undefined => typeof value === 'number' && Number.isSafeInteger(value) && value >= 0 ? value : undefined;
const text = (value: unknown, max = 256): string | undefined => typeof value === 'string' ? value.slice(0, max) : undefined;

export type DirtyCheck = (file: string) => boolean;

const LIVE_MESSAGE: Record<LiveCheck, string> = {
    matches_index: 'Saved bytes match the indexed content hash.',
    changed_since_index: 'Saved bytes differ from the indexed content hash; cited lines may have moved.',
    not_saved: 'The open editor has unsaved edits; the index describes the saved file.',
    missing: 'No file exists at the indexed location.',
    outside_workspace: 'The file is outside this window’s workspace folders.',
    unchecked: 'The file was too large to compare.',
};

async function liveOf(info: Value, contentHash: string, dirty: DirtyCheck): Promise<{ live: LiveCheck; root?: string }> {
    const source = obj(info.source);
    if (typeof source.root !== 'string' || typeof source.relative_path !== 'string') { return { live: 'unchecked' }; }
    try {
        const file = await sourceFile(source.root, source.relative_path);
        return { live: await liveCheck(file, contentHash, dirty(file)), root: displayPath(source.root) };
    } catch { return { live: 'unchecked' }; }
}

/** Evidence › Inspector: the full indexed record for one cited file, compared with its saved bytes. */
export async function inspect(session: EvidenceClient, refs: RefRegistry, project: string, id: string, dirty: DirtyCheck): Promise<InspectView> {
    const ref = refs.get(id, project);
    const result = await session.call('airk_file_info', { path: ref.path, response_mode: 'full' }, { timeout: 30000 });
    const info = result.payload;
    const indexedHash = typeof info.content_hash === 'string' ? info.content_hash : ref.contentHash;
    const { live, root } = await liveOf(info, indexedHash, dirty);
    const message = indexedHash !== ref.contentHash
        ? `${LIVE_MESSAGE[live]} The index was republished since this citation was captured.` : LIVE_MESSAGE[live];
    return {
        ref: id, path: ref.path, startLine: ref.startLine, endLine: ref.endLine, contentHash: indexedHash, live, liveMessage: message,
        ...(result.publication ? { publication: result.publication } : {}),
        ...(text(info.language) ? { language: text(info.language) } : {}), ...(text(info.parse_status) ? { parseStatus: text(info.parse_status) } : {}),
        ...(text(info.line_basis) ? { lineBasis: text(info.line_basis) } : {}), ...(text(info.extraction_method) ? { extractionMethod: text(info.extraction_method) } : {}),
        ...(count(info.indexed_lines) !== undefined ? { indexedLines: count(info.indexed_lines) } : {}),
        ...(count(info.source_bytes) !== undefined ? { sourceBytes: count(info.source_bytes) } : {}),
        ...(count(info.chunks) !== undefined ? { chunks: count(info.chunks) } : {}), ...(count(info.facts) !== undefined ? { facts: count(info.facts) } : {}),
        ...(root ? { sourceRoot: root } : {}),
    };
}

const STATUS_MESSAGE: Record<CitationResult['status'], string> = {
    valid: 'Indexed, in range, and the saved file matches the index.',
    changed_since_index: 'Indexed and in range, but the saved file changed since indexing.',
    not_saved: 'Indexed and in range; the open editor has unsaved edits.',
    missing: 'Indexed, but no file exists at its location now.',
    not_found: 'Not in this project’s published index.',
    line_out_of_range: 'Indexed, but the cited lines are beyond the indexed line count.',
    outside_collection: 'Absolute or parent-relative paths are outside the collection and are not checked.',
};

/**
 * Evidence › Checker: validate citations in pasted text (an agent report, a plan, a PR).
 * Only supported citation syntax is checked; text without any citation is reported as
 * unsupported, never as a pass. Each distinct path costs one airk_file_info call.
 */
export async function checkCitations(session: EvidenceClient, refs: RefRegistry, project: string, input: string, dirty: DirtyCheck): Promise<CitationCheckView> {
    const parsed = parseCitations(input);
    const paths = [...new Set(parsed.citations.filter(entry => !entry.absolute).map(entry => entry.path))].slice(0, 25);
    const records = new Map<string, { info?: Value; live?: LiveCheck; publication?: string }>();
    let publication: string | undefined;
    for (const path of paths) {
        try {
            const result = await session.call('airk_file_info', { path }, { timeout: 30000 });
            publication ??= result.publication;
            const hash = typeof result.payload.content_hash === 'string' ? result.payload.content_hash : '';
            records.set(path, { info: result.payload, live: hash ? (await liveOf(result.payload, hash, dirty)).live : 'unchecked', ...(result.publication ? { publication: result.publication } : {}) });
        } catch (error) {
            if (error instanceof EvidenceFailure && ['SERVICE_UNAVAILABLE', 'REQUEST_TIMEOUT', 'INDEX_UNAVAILABLE'].includes(String(error.error.code))) { throw error; }
            records.set(path, {});
        }
    }
    const citations = parsed.citations.map((entry): CitationResult => {
        const base = { text: entry.text, path: entry.path, startLine: entry.startLine, endLine: entry.endLine };
        if (entry.absolute) { return { ...base, status: 'outside_collection', message: STATUS_MESSAGE.outside_collection }; }
        const record = records.get(entry.path);
        if (!record) { return { ...base, status: 'not_found', message: 'Not checked: more than 25 distinct paths in one check.' }; }
        if (!record.info) { return { ...base, status: 'not_found', message: STATUS_MESSAGE.not_found }; }
        const lines = count(record.info.indexed_lines), hash = record.info.content_hash;
        if (lines !== undefined && entry.endLine > lines) { return { ...base, status: 'line_out_of_range', message: `${STATUS_MESSAGE.line_out_of_range} (${lines} indexed lines)` }; }
        const status: CitationResult['status'] = record.live === 'matches_index' ? 'valid' : record.live === 'changed_since_index' ? 'changed_since_index'
            : record.live === 'not_saved' ? 'not_saved' : record.live === 'missing' ? 'missing' : 'valid';
        const ref = typeof hash === 'string' && /^[0-9a-f]{64}$/.test(hash)
            ? refs.add({ project, path: entry.path, startLine: entry.startLine, endLine: entry.endLine, contentHash: hash, ...(record.publication ? { publication: record.publication } : {}) }) : undefined;
        const message = record.live === 'unchecked' ? 'Indexed and in range; the saved file could not be compared.' : STATUS_MESSAGE[status];
        return { ...base, status, message, ...(ref ? { ref } : {}) };
    });
    return { supported: citations.length > 0, citations, limited: parsed.limited, ...(publication ? { publication } : {}) };
}

/** Evidence › Changes: bounded observed changes since an earlier publication token. */
export async function changesSince(session: EvidenceClient, since: string, limit: number): Promise<ChangesView> {
    const result = await session.call('airk_changes', { since, limit, max_bytes: 32768 }, { timeout: 30000 });
    const payload = result.payload;
    const endpoint = (value: unknown) => { const point = obj(value); return text(point.citation_path, 4096) ?? text(point.path, 4096); };
    const items = (Array.isArray(payload.changes) ? payload.changes : []).map(raw => {
        const change = obj(raw), subject = obj(change.subject), [kind] = Object.keys(subject);
        const path = endpoint(change.after) ?? endpoint(change.before);
        const strings = (value: unknown) => (Array.isArray(value) ? value : []).filter((entry): entry is string => typeof entry === 'string').slice(0, 16);
        return { ...(path ? { path } : {}), subject: kind ?? 'unknown', causes: strings(change.causes), dimensions: strings(change.dimensions),
            ...(count(change.observations) !== undefined ? { observations: count(change.observations) } : {}) };
    });
    const coverage = Object.entries(obj(payload.coverage)).map(([area, value]) => {
        const entry = obj(value);
        return `${area}: complete after generation ${String(entry.complete_after ?? '?')}${entry.reason ? ` (${String(entry.reason)})` : ''}`;
    });
    if (payload.status === 'cursor_expired') { throw new EvidenceFailure(explorerError('EVIDENCE_MISMATCH', 'The change cursor expired; run the comparison again.')); }
    return { since, baseline: text(payload.baseline_status, 64) ?? 'unknown', status: text(payload.status, 64) ?? 'unknown', items,
        truncated: payload.truncated === true || !!payload.cursor, ...(result.publication ? { publication: result.publication } : {}), coverage };
}
