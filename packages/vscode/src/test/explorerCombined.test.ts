// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { continueFacts, factList, FactHandles } from '../explorerFacts';
import { inspect, checkCitations } from '../explorerInspect';
import { combinedRows, originArgs, originatingResult, RefRegistry, searchView } from '../explorerSearch';
import { EvidenceFailure } from '../explorerEvidence';
import { explorerError } from '../explorerErrors';
import { appendFactPage } from '../explorerContracts';

const project = 'ab'.repeat(32), hash = 'cd'.repeat(32);
const token = (member: number) => `airk-r1:${String(member).repeat(32)}:1:${'ef'.repeat(32)}`;
const origin = (member: number) => ({ index_id: `idx_${String(member).repeat(32)}`, source_root_id: `src_${String(member).repeat(32)}`,
    project_ids: [`prj_${String(member).repeat(32)}`], source_relative_path: 'same.rs', publication: { token: token(member) } });
const citation = { path: 'same.rs', content_hash: hash, start_line: 1, end_line: 1 };
function combined(facts = false): Record<string, any> {
    const receipt = { schema: 'airk.saved-source-verification/1', method: 'sha256-full-saved-source/1', sources: [], origins: [], references: [], status: 'unverified' };
    return { schema: 'airk.combined-read/1', members: [1, 2].map(member => ({ index_id: origin(member).index_id, outcome: 'complete', is_error: false,
        result: facts ? { publication: { token: token(member) }, files: [{ path: 'same.rs', content_hash: hash }], provenance: [{ producer: `parser-${member}` }],
            items: [{ file: 0, lines: [1, 1], name: `Type${member}`, kind: 'definition', symbol_id: 'syntax:same.rs:1:2:struct' }], source_verification: receipt }
            : { publication: { token: token(member) }, hits: [{ citation, text: `member ${member}`, truncated: false }], source_verification: receipt } })),
        ranking: [1, 2].map((member, index) => ({ index_id: origin(member).index_id, origin: origin(member), result_pointer: `/members/${index}/result/${facts ? 'items' : 'hits'}/0` })),
        partial: false, truncated: false, publication_vector: { authority: 'aa'.repeat(32), members: [] } };
}

test('Explorer Search resolves qualified combined ranks and preserves complete native member results', () => {
    const payload = combined(), refs = new RefRegistry();
    const view = searchView(project, { v: 1, requestId: 'combined', type: 'search.run', query: 'member', mode: 'lexical', limit: 2, maxBytes: 8192, fullChunks: false }, payload, undefined, refs, {});
    assert.equal(view.returned, 2);
    assert.equal(view.ordering, 'fused_rank');
    assert.equal(view.publication, undefined, 'no scalar publication is invented');
    assert.deepEqual(view.nativeResult, payload);
    const [a, b] = view.hits.map(hit => refs.get(hit.ref, project));
    assert.equal(a.path, b.path);
    assert.notEqual(a.origin?.index_id, b.origin?.index_id);
    assert.deepEqual(originArgs(a.origin), { project_ids: origin(1).project_ids, index_ids: [origin(1).index_id], origin_publication: token(1) });
    const malformed = structuredClone(payload); malformed.ranking[0].result_pointer = '/members/9/result/hits/0';
    assert.throws(() => combinedRows(malformed), /originating|identity|response|citation|index/);
});

test('Explorer fact tables keep member provenance, rank order and qualified follow-up handles', () => {
    const payload = combined(true), refs = new RefRegistry(), handles = new FactHandles(refs);
    const view = factList({ tool: 'airk_symbols', args: {}, limit: 2 }, payload, undefined, project, handles);
    assert.deepEqual(view.items.map(item => item.name), ['Type1', 'Type2']);
    assert.deepEqual(view.nativeResult, payload);
    assert.deepEqual(view.provenance.map(item => item.producer), ['parser-1', 'parser-2']);
    for (const [index, item] of view.items.entries()) {
        assert.equal(handles.symbols.get(item.symbolRef!, project).origin?.index_id, origin(index + 1).index_id);
        assert.equal(refs.get(item.ref!, project).origin?.source_root_id, origin(index + 1).source_root_id);
    }
});

test('Inspector uses one explicit originating index and retains its full combined receipt', async () => {
    const refs = new RefRegistry(), id = refs.add({ project, path: 'same.rs', contentHash: hash, startLine: 1, endLine: 1, publication: token(1), origin: origin(1) });
    const payload = { schema: 'airk.combined-read/1', members: [{ index_id: origin(1).index_id, outcome: 'complete', result: { publication: { token: token(1) }, content_hash: hash, indexed_lines: 1 } }], ranking: [] };
    const calls: unknown[] = [];
    const session = { call: async (tool: string, args: unknown) => { calls.push([tool, args]); return { payload }; } } as any;
    const view = await inspect(session, refs, project, id, () => false);
    assert.deepEqual(calls, [['airk_file_info', { path: 'same.rs', response_mode: 'full', ...originArgs(origin(1)) }]]);
    assert.equal(view.publication, token(1));
    assert.deepEqual(view.nativeResult, payload);
    await assert.rejects(checkCitations({ spec: { combinedSession: {} }, call: async () => { assert.fail('ambiguous pasted paths must not dispatch'); } } as any, refs, project, 'same.rs:1', () => false), /originating index/);
});

test('failed combined native facts and Inspector retain the complete attempted response', async () => {
    const payload = combined(); payload.error = { code: 'member_requirements_unmet', retryable: false };
    const wire = { content: [], structuredContent: payload, isError: true };
    const failure = new EvidenceFailure(explorerError('TOOL_ERROR'), wire);
    const refs = new RefRegistry(), handles = new FactHandles(refs);
    const { runFacts } = await import('../explorerFacts');
    const view = await runFacts({ call: async () => { throw failure; } } as any, handles, project, { tool: 'airk_references', args: { name: 'member' }, limit: 2 });
    assert.equal(view.nativeFailure, true);
    assert.deepEqual(view.nativeResult, wire);
    const selected = { schema: 'airk.combined-read/1', ranking: [], members: [{ index_id: origin(1).index_id, outcome: 'failed', is_error: true,
        result: { publication: { token: token(1) }, content_hash: hash, error: { code: 'verification_required_unmet', retryable: false } } }], error: { code: 'member_requirements_unmet' } };
    const attempted = { content: [], structuredContent: selected, isError: true };
    const ref = refs.add({ project, path: 'same.rs', contentHash: hash, startLine: 1, endLine: 1, publication: token(1), origin: origin(1) });
    const inspected = await inspect({ call: async () => { throw new EvidenceFailure(explorerError('TOOL_ERROR'), attempted); } } as any, refs, project, ref, () => false);
    assert.equal(inspected.nativeFailure, true);
    assert.deepEqual(inspected.nativeResult, attempted);
});

test('an external source Open projection must confirm the held source-root ID and root-relative path', () => {
    const held = origin(1);
    const body = { path: '@oldexternal/same.rs', content_hash: hash, publication: held.publication,
        source: { root: '/canonical/external-root', relative_path: 'same.rs' } };
    const payload = { schema: 'airk.combined-read/1', members: [{ index_id: held.index_id, outcome: 'complete', result: body }],
        ranking: [{ index_id: held.index_id, origin: held, result_pointer: '/members/0/result' }] };
    assert.equal((originatingResult(payload, held).payload.source as any).root, '/canonical/external-root');
    const wrongRoot = structuredClone(payload); wrongRoot.ranking[0].origin.source_root_id = 'src_' + '3'.repeat(32);
    assert.throws(() => originatingResult(wrongRoot, held), /source-root identity/);
    const wrongPath = structuredClone(payload); wrongPath.members[0].result.source.relative_path = '@oldexternal/same.rs';
    assert.throws(() => originatingResult(wrongPath, held), /source-root identity/);
});

function publicationVector(): Record<string, any> {
    return { authority: 'aa'.repeat(32), observed_from_unix_ms: 1, observed_to_unix_ms: 2, members: [1, 2].map(member => ({
        index_id: origin(member).index_id, publication: { collection: `member-${member}`, token: token(member) },
    })) };
}

test('combined fact/reference continuations retain the exact opaque32KiB cursor, qualified inputs and member publications', async () => {
    for (const tool of ['airk_symbols', 'airk_document_facts', 'airk_references', 'airk_list_files'] as const) {
        const refs = new RefRegistry(), handles = new FactHandles(refs), payload = combined(tool !== 'airk_references');
        payload.publication_vector = publicationVector();
        payload.cursor = 'airk-cq1:' + 'x'.repeat(16000); payload.truncated = true;
        for (const [index, member] of payload.members.entries()) { member.result.page = { cursor: `child-cursor-${index}`, more: true }; }
        if (tool === 'airk_list_files') {
            for (const member of payload.members) { member.result = { paths: ['@external/same.rs'] }; }
            payload.ranking.forEach((rank: any, index: number) => { rank.result_pointer = `/members/${index}/result/paths/0`; });
        }
        const args = { name: 'Type', project_ids: [origin(1).project_ids[0], origin(2).project_ids[0]], index_ids: [origin(1).index_id, origin(2).index_id],
            member_availability: 'require', limit: 2, max_bytes: 8192, reply_detail: 'rich', verification: 'off', path_prefix: '@external/' };
        const expectedArgs = structuredClone(args);
        const rawFirst = { ...structuredClone(payload), delivery_observation: { digest: 'first-encoded' } };
        const first = factList({ tool, args, limit: 2 }, payload, undefined, project, handles, rawFirst);
        assert.ok(first.more, `${tool} exposes the composite continuation`);
        assert.equal(first.more, 'c1', 'individual member cursors never enter the interactive combined handle cache');
        assert.equal(handles.cursors.get(first.more, project).cursor, payload.cursor);
        if (tool === 'airk_list_files') { assert.deepEqual(first.items.map(item => item.path), ['@external/same.rs', '@external/same.rs']); }
        args.project_ids.splice(0, 1); args.max_bytes = 1;
        const next = structuredClone(payload); next.cursor = 'airk-cq1:next-exact-opaque'; next.truncated = true;
        const calls: unknown[] = [];
        const rawNext = { ...structuredClone(next), delivery_observation: { digest: 'next-encoded' } };
        const session = { call: async (name: string, input: unknown) => { calls.push([name, structuredClone(input)]); return { payload: next, rawPayload: rawNext }; } } as any;
        const page = await continueFacts(session, handles, project, first.more);
        assert.deepEqual(structuredClone(calls), [[tool, { ...expectedArgs, cursor: payload.cursor }]]);
        assert.ok(page.more);
        assert.equal(handles.cursors.get(page.more, project).cursor, next.cursor, 'only the new native opaque cursor advances the next page');
        assert.deepEqual(page.nativeResult, rawNext);
        const appended = appendFactPage(first, page);
        assert.deepEqual(appended.nativePages, [rawFirst, rawNext], 'JSON/Markdown copies retain both complete native pages and their dated receipts');
        assert.equal(appended.returned, first.returned + page.returned);
        const last = structuredClone(next); last.cursor = null; last.truncated = false;
        const final = await continueFacts({ call: async (name: string, input: unknown) => { calls.push([name, structuredClone(input)]); return { payload: last }; } } as any, handles, project, page.more);
        assert.deepEqual(calls[1], [tool, { ...expectedArgs, cursor: next.cursor }]);
        assert.equal(final.more, undefined);
        await assert.rejects(continueFacts(session, handles, 'different-session', first.more), /reference/);
        assert.equal(calls.length, 2, 'a foreign-session cursor handle never dispatches a read');
        for (const changed of ['publication', 'scope', 'selected_indexes'] as const) {
            const invalid = structuredClone(next);
            if (changed === 'publication') { invalid.publication_vector.members[0].publication.token = token(1).replace(':1:', ':2:'); }
            if (changed === 'scope') { invalid.publication_vector.authority = 'bb'.repeat(32); }
            if (changed === 'selected_indexes') { invalid.publication_vector.members.pop(); }
            await assert.rejects(continueFacts({ call: async () => ({ payload: invalid }) } as any, handles, project, first.more), /publication or query scope changed/);
        }
        const oversized = structuredClone(payload); oversized.cursor = 'é'.repeat(16385);
        assert.throws(() => factList({ tool, args: expectedArgs, limit: 2 }, oversized, undefined, project, handles), /32768-byte/);
    }
});


test('Explorer adapters use decoded metadata while copies and continued pages retain original native bytes', async () => {
    const payload = combined(true), raw = { ...structuredClone(payload), shared_values: { schema: 'airk.shared-native-header-templates/1', values: {} },
        delivery_observation: { digest: 'original-encoded-only' } };
    const refs = new RefRegistry(), handles = new FactHandles(refs);
    const facts = factList({ tool: 'airk_symbols', args: {}, limit: 2 }, payload, undefined, project, handles, raw);
    assert.equal(facts.items.length, 2);
    assert.deepEqual(facts.nativeResult, raw);
    const searchPayload = combined();
    const search = searchView(project, { v: 1, requestId: 'raw', type: 'search.run', query: 'member', mode: 'lexical', limit: 2, maxBytes: 8192, fullChunks: false }, searchPayload, undefined, refs, {}, raw);
    assert.equal(search.returned, 2);
    assert.deepEqual(search.nativeResult, raw);
    const { statusSummary } = await import('../explorerStatus');
    const statusPayload = structuredClone(payload);
    for (const member of statusPayload.members) { member.result.index_state = 'available'; member.result.files = 3; }
    const status = statusSummary(statusPayload, 10, raw);
    assert.equal(status.files, 6);
    assert.deepEqual(status.combined?.nativeResult, raw);
    const info = { schema: 'airk.combined-read/1', members: [{ index_id: origin(1).index_id, outcome: 'complete', result: { publication: { token: token(1) }, content_hash: hash, indexed_lines: 1 } }], ranking: [] };
    const id = refs.add({ project, path: 'same.rs', contentHash: hash, startLine: 1, endLine: 1, publication: token(1), origin: origin(1) });
    const inspected = await inspect({ call: async () => ({ payload: info, rawPayload: raw }) } as any, refs, project, id, () => false);
    assert.equal(inspected.publication, token(1));
    assert.deepEqual(inspected.nativeResult, raw);
    assert.equal((raw.delivery_observation as any).digest, 'original-encoded-only');
});


test('undecodable native metadata remains an explicit failed observation with raw response available', async () => {
    const refs = new RefRegistry(), handles = new FactHandles(refs), raw = { content: [{ type: 'text', text: '{"schema":"airk.combined-read/1","members":[]}' }] };
    const error = new EvidenceFailure(explorerError('TOOL_ERROR'), raw, {});
    const session = { call: async () => { throw error; } } as any;
    const { runFacts } = await import('../explorerFacts');
    const facts = await runFacts(session, handles, project, { tool: 'airk_symbols', args: {}, limit: 2 });
    assert.equal(facts.nativeFailure, true); assert.equal(facts.truncated, true);
    assert.match(facts.notes.join(' '), /could not be decoded/); assert.deepEqual(facts.nativeResult, raw);
    const id = refs.add({ project, path: 'same.rs', contentHash: hash, startLine: 1, endLine: 1, publication: token(1), origin: origin(1) });
    const inspected = await inspect(session, refs, project, id, () => false);
    assert.equal(inspected.live, 'unchecked'); assert.equal(inspected.nativeFailure, true);
    assert.match(inspected.liveMessage, /status is unknown/); assert.deepEqual(inspected.nativeResult, raw);
    assert.equal(inspected.publication, undefined, 'failed decode does not invent a fresh publication');
});
