// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { EvidenceFailure } from '../explorerEvidence';
import { continueFacts, evidenceClass, FactHandles, factList, scoped } from '../explorerFacts';
import { RefRegistry } from '../explorerSearch';

const project = 'ab'.repeat(32), other = 'cd'.repeat(32), hash = 'b9'.repeat(32);
const handles = () => new FactHandles(new RefRegistry());
const failure = (code: string) => (error: unknown) => error instanceof EvidenceFailure && error.error.code === code;
const citation = (path: string, start: number, end: number) => ({ path, start_line: start, end_line: end, content_hash: hash, location: { Lines: { start, end } } });

test('minimal fact tables resolve file indexes and keep producer provenance', () => {
    const h = handles();
    const view = factList({ tool: 'airk_symbols', args: { limit: 5 }, limit: 5 }, {
        files: [{ path: 'src/session.ts', content_hash: hash, line_basis: 'source' }],
        items: [{ file: 0, kind: 'definition', lines: [9, 11], name: 'refreshSession', signature: 'function refreshSession(token: string)' },
            { file: 7, kind: 'definition', lines: [1, 1], name: 'orphan' }],
        provenance: [{ basis: 'syntax', producer: 'airk-tree-sitter/typescript', coverage: 'written syntax only' }],
        truncated: false, cursor: 'airk-d1:opaque',
    }, 'airk-r1:p', project, h);
    assert.equal(view.returned, 1, 'items pointing at an unknown file are dropped');
    const [item] = view.items;
    assert.deepEqual([item.path, item.startLine, item.endLine, item.kind, item.evidenceClass], ['src/session.ts', 9, 11, 'definition', 'E2']);
    assert.equal(h.refs.get(item.ref!, project).contentHash, hash);
    assert.deepEqual(view.provenance, [{ producer: 'airk-tree-sitter/typescript', basis: 'syntax', coverage: 'written syntax only' }]);
    assert.match(view.more!, /^c/);
    assert.equal(h.cursors.get(view.more!, project).cursor, 'airk-d1:opaque');
    assert.throws(() => h.cursors.get(view.more!, other), failure('UNKNOWN_REFERENCE'));
});

test('code facts register symbol handles; references stay textual E1 with the resolution note', () => {
    const h = handles();
    const facts = factList({ tool: 'airk_code_facts', args: { name: 'refreshSession' }, limit: 10 }, {
        items: [{ basis: 'syntax', citation: citation('src/session.ts', 9, 11), kind: 'definition', name: 'refreshSession', qualified_name: 'refreshSession',
            signature: 'function refreshSession(token: string)', symbol_id: 'syntax:src/session.ts:140:226:function_declaration', provenance: 0 },
            { basis: 'scip', citation: citation('src/x.ts', 1, 1), kind: 'definition', name: 'x', symbol_id: 'not-a-symbol' }],
        provenance: [{ producer: 'airk-tree-sitter/typescript' }],
    }, undefined, project, h);
    assert.equal(h.symbols.get(facts.items[0].symbolRef!, project).symbolId, 'syntax:src/session.ts:140:226:function_declaration');
    assert.equal(facts.items[1].symbolRef, undefined, 'unrecognized ids are not turned into handles');
    assert.equal(facts.items[1].evidenceClass, 'E4');
    const refs = factList({ tool: 'airk_references', args: { name: 'verifyToken' }, limit: 10 }, {
        evidence: { hits: [{ citation: citation('src/session.ts', 5, 5), text: '    return verifyToken(token);\n', truncated: false }], truncated: false },
        resolution: 'textual-occurrences; includes comments, strings and definitions; no name resolution',
    }, 'airk-r1:p', project, h);
    assert.equal(refs.items[0].evidenceClass, 'E1');
    assert.equal(refs.items[0].kind, 'occurrence');
    assert.match(refs.notes[0], /no name resolution/);
    assert.equal(refs.more, undefined, 'references have no cursor');
});

test('relationship edges are labeled candidates and expose only recognized symbol targets', () => {
    const h = handles();
    const edges = factList({ tool: 'airk_relationships', args: {}, limit: 10 }, {
        coverage: 'Native syntax candidates and source-bound producer assertions. Impact is incomplete.',
        items: [{ basis: 'syntax', citation: citation('src/session.ts', 10, 10), kind: 'call', provenance: 0, resolution: 'syntax_candidate',
            source: 'syntax:a', target: 'new SessionStore().refresh' },
            { basis: 'syntax', citation: citation('src/session.ts', 10, 10), kind: 'contains', provenance: 1, resolution: 'syntax_candidate',
                source: 'syntax:a', target: 'syntax:src/session.ts:190:223:call_expression' }],
        provenance: [{ producer: 'airk-tree-sitter/typescript' }, { producer: 'airk-syntax-containment' }],
    }, undefined, project, h);
    assert.deepEqual(edges.items.map(item => [item.kind, item.resolution, !!item.symbolRef, item.producer]),
        [['call', 'syntax_candidate', false, 'airk-tree-sitter/typescript'], ['contains', 'syntax_candidate', true, 'airk-syntax-containment']]);
    assert.match(edges.notes[0], /Impact is incomplete/);
});

test('document facts keep heading context and are labeled as extraction candidates', () => {
    const view = factList({ tool: 'airk_document_facts', args: { kind: 'requirement' }, limit: 10 }, {
        items: [{ citation: citation('requirements.md', 5, 5), document_basis: 'heuristic-candidate', heading_path: ['Requirements', 'REQ-AUTH-7 Session renewal'],
            kind: 'requirement', name: 'The system shall renew sessions before expiry.' }],
    }, undefined, project, handles());
    assert.deepEqual(view.items[0].heading, ['Requirements', 'REQ-AUTH-7 Session renewal']);
    assert.equal(view.items[0].detail, 'basis: heuristic-candidate');
    assert.match(view.notes.join(' '), /not verified requirements/);
    assert.equal(evidenceClass(undefined), 'E2');
});

test('continuations replay the exact arguments and refuse a republished index', async () => {
    const h = handles();
    const first = factList({ tool: 'airk_list_files', args: { limit: 2 }, limit: 2 }, { paths: [], items: [], cursor: 'k1', truncated: true }, 'airk-r1:old', project, h);
    const calls: unknown[] = [];
    const session = (publication: string) => ({ call: async (tool: string, args: unknown) => { calls.push([tool, args]); return { payload: { items: [], truncated: false }, publication }; } }) as any;
    await continueFacts(session('airk-r1:old'), h, project, first.more!);
    assert.deepEqual(calls[0], ['airk_list_files', { limit: 2, cursor: 'k1' }]);
    await assert.rejects(continueFacts(session('airk-r1:new'), h, project, first.more!), failure('EVIDENCE_MISMATCH'));
    assert.deepEqual(scoped({ a: 1 }, 'src/'), { a: 1, path_prefix: 'src/' });
    assert.throws(() => scoped({}, '../up'), failure('INVALID_MESSAGE'));
});
