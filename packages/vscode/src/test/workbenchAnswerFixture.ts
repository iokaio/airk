// SPDX-License-Identifier: Apache-2.0
// Answer /3 fixtures built on the Rust-emitted reply case `two_parts_valid`
// (fixtures/split/arbiter/answer_loop_cases.json). Only the record projection
// and the whole-reply verification receipt are filled in here, as the Core
// inventory test does; every other field is the emitted reply.
import { readFileSync } from 'node:fs';
import * as path from 'node:path';
import type { Value } from '../workbenchContracts';
const cases = JSON.parse(readFileSync(path.resolve(__dirname, '../../../../fixtures/split/arbiter/answer_loop_cases.json'), 'utf8'));
export const loopCases = cases;
export function emittedReply(id = 'two_parts_valid'): Value {
    const row = cases.response_cases.find((value: { id: string }) => value.id === id);
    if (!row?.accepted) throw new Error(`missing accepted Rust-emitted reply case ${id}`);
    return structuredClone(row.response);
}
const base = emittedReply();
export const answerRecord = (base.records as Value[])[1].id as string, answerIndex = (base.scope as Value).index_id as string;
export const answerProject = 'prj_' + '33'.repeat(16), answerRoot = 'src_' + '34'.repeat(16), answerHash = '35'.repeat(32);
export const answerPublication = (base.scope as Value).publication as Value;

/** The emitted reply with one located span on its second record (the cited `symbols` read) and a verified receipt for it. */
export function answerSourceFixture(): { answer: Value; info: Value } {
    const answer = emittedReply(), token = answerPublication.token as string;
    (answer.records as Value[])[1].canonical_result = { projection: 'airk.arbiter.loop-evidence/1', revision: token, full_bytes: 512, full_digest: '39'.repeat(32), hits_total: 1,
        spans: [{ pointer: '/hits/0', path: 'api/source.rs', start_line: 7, end_line: 9, content_hash: answerHash, text: 'Canonical source bytes.', truncated: false, focus: 0.5, future_span: { preserve: true } }],
        future_projection: { preserve: true } };
    const verification: Value = { schema: 'airk.saved-source-verification/1', method: 'sha256-full-saved-source/1',
        sources: [{ id: 'source-0', path: 'api/source.rs', expected_hash: answerHash, source_root_id: answerRoot, source_relative_path: 'api/source.rs', origin_id: 'origin-0', reference_ids: ['ref-0'] }],
        origins: [{ id: 'origin-0', index_id: answerIndex, project_ids: [answerProject], publication: token }],
        references: [{ id: 'ref-0', source_id: 'source-0', result_pointer: '/records/1/canonical_result/spans/0' }] };
    answer.source_verification = verification;
    return { answer, info: { collection: answerPublication.collection, publication: answerPublication, path: 'api/source.rs', content_hash: answerHash, line_basis: 'source',
        source: { root: '/native/verified-original', relative_path: 'api/source.rs' }, source_verification: structuredClone(verification) } };
}
