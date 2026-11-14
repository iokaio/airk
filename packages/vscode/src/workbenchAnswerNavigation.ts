// SPDX-License-Identifier: Apache-2.0
// Host projection of existing native receipts. It grants no live read authority.
import type { NativeRead } from './generated/arbiter';
import { object, type Value } from './workbenchContracts';
import { nativeVerification } from './sourceVerification';
import { comparePublication } from './publications';
const digest = (v: unknown): v is string => typeof v === 'string' && /^[0-9a-f]{64}$/.test(v);
const indexId = (v: unknown): v is string => typeof v === 'string' && /^idx_[0-9a-f]{32}$/.test(v);
const projectIds = (v: unknown): v is string[] => Array.isArray(v)
    && new Set(v).size === v.length && v.every(id => typeof id === 'string' && /^prj_[0-9a-f]{32}$/.test(id));
// Explicit per-surface applicability, checked against the generated native enum.
// Definitions/fact discovery, References' nested evidence and document sections
// need their own native locator owners; a coincidental `hits` field is not one.
const sourceHitOperation: Record<NativeRead, boolean> = {
    search: true, context: true, read_evidence: true,
    definitions: false, references: false, document_sections: false,
};
const equalPublication = (a: unknown, b: unknown, collection: unknown): boolean => typeof collection === 'string'
    && comparePublication({ authority: 'native-answer', collection, publication: a }, { authority: 'native-answer', collection, publication: b }) === 'equal';

/** Resolve an exact returned-reference association; roots never come from the renderer. */
export function answerSourceBinding(body: Value, path: string, hash: string, index: string, publication: unknown,
    pointer?: string): Value | undefined {
    const receipt = nativeVerification(body.source_verification);
    if (!receipt) return;
    const references = (receipt.references as unknown[]).map(object);
    const sources = (receipt.sources as unknown[]).map(object);
    const origins = (receipt.origins as unknown[]).map(object);
    if (new Set(sources.map(source => source.id)).size !== sources.length
        || new Set(origins.map(origin => origin.id)).size !== origins.length
        || new Set(references.map(reference => reference.id)).size !== references.length
        || !equalPublication(body.publication, publication, body.collection)) return;
    const matching = sources.filter(source => source.path === path && source.expected_hash === hash
        && typeof source.source_root_id === 'string' && /^src_[0-9a-f]{32}$/.test(source.source_root_id)
        && typeof source.source_relative_path === 'string' && source.source_relative_path.length > 0
        && !/[\\:\u0000]/.test(source.source_relative_path)
        && source.source_relative_path.split('/').every(part => !['', '.', '..'].includes(part))
        && origins.filter(origin => origin.id === source.origin_id && origin.index_id === index
            && equalPublication(origin.publication, publication, body.collection)
            && projectIds(origin.project_ids)).length === 1
        && (pointer === undefined || references.filter(reference => reference.result_pointer === pointer
            && reference.source_id === source.id && Array.isArray(source.reference_ids) && source.reference_ids.includes(reference.id)).length === 1));
    if (matching.length !== 1) return;
    const source = matching[0], origin = origins.find(origin => origin.id === source.origin_id)!;
    return { index_id: index, project_ids: origin.project_ids, source_root_id: source.source_root_id,
        source_relative_path: source.source_relative_path, publication };
}

/** Flatten only supported native hit arrays, preserving record ID and hit locator. */
export function answerEvidenceRows(envelope: Value): Value[] {
    if (envelope.schema !== 'airk.arbiter.context/1' || !Array.isArray(envelope.records)) return [];
    const records = envelope.records.map(object), ids = records.map(record => record.id);
    if (ids.some(id => !digest(id)) || new Set(ids).size !== ids.length) return [];
    return records.flatMap(record => {
        if (!Object.hasOwn(sourceHitOperation, String(record.operation)) || !sourceHitOperation[record.operation as NativeRead]) return [];
        const scope = object(record.scope), body = object(record.canonical_result), publication = object(scope.publication);
        const observation = object(body.scope_observation), members = Array.isArray(observation.members) ? observation.members.map(object) : [];
        const own = members.filter(member => member.index_id === scope.index_id);
        if (!indexId(scope.index_id) || !digest(scope.authorization) || observation.fingerprint !== scope.authorization || own.length !== 1
            || !equalPublication(body.publication, publication, body.collection) || !Array.isArray(body.hits)) return [];
        return body.hits.flatMap((raw, hit) => {
            const row = object(raw), citation = object(row.citation);
            if (typeof citation.path !== 'string' || !digest(citation.content_hash)) return [];
            const origin = answerSourceBinding(body, citation.path, citation.content_hash, String(scope.index_id), publication, `/hits/${hit}/citation`);
            const memberships = Array.isArray(own[0].memberships) ? own[0].memberships.map(object) : [];
            if (!origin || !Array.isArray(own[0].source_root_ids) || !own[0].source_root_ids.includes(origin.source_root_id)
                || !(origin.project_ids as string[]).every(id => memberships.some(member => member.project_id === id))) return [];
            return [{ ...row, origin, publication: publication.token, step: record.id, result_pointer: `/hits/${hit}` }];
        });
    });
}
