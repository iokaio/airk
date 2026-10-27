// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { createHash } from 'node:crypto';
import * as fs from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';
import { test } from 'node:test';
import { parseCitations, parseWebviewMessage } from '../explorerContracts';
import { explorerError } from '../explorerErrors';
import { EvidenceFailure } from '../explorerEvidence';
import { changesSince, checkCitations, inspect } from '../explorerInspect';
import { RefRegistry } from '../explorerSearch';
import { FactHandles } from '../explorerFacts';

const project = 'ab'.repeat(32);
const sha = (text: string) => createHash('sha256').update(Buffer.from(text)).digest('hex');

test('selected-file inspection creates a project-bound reference from one indexed record', async () => {
    const refs = new RefRegistry();
    const session = { call: async (tool: string, args: unknown) => {
        assert.equal(tool, 'airk_file_info');
        assert.deepEqual(args, { path: 'a.ts', response_mode: 'full' });
        return { publication: 'p', payload: { content_hash: sha('a'), indexed_lines: 1 } };
    } } as any;
    const view = await inspect(session, refs, project, undefined, () => false, 'a.ts');
    assert.equal(view.path, 'a.ts');
    assert.equal(view.publication, 'p');
    assert.equal(view.live, 'unchecked');
    assert.equal(refs.get(view.ref, project).contentHash, sha('a'));
    assert.throws(() => refs.get(view.ref, 'other-project'), /reference/);
    await assert.rejects(inspect(session, refs, project, undefined, () => false), /source file/);
    for (const payload of [{ read: { status: 'unknown', reason: 'path is absent from the current index' } }, { content_hash: 'invalid' }]) {
        await assert.rejects(inspect({ call: async () => ({ publication: 'p', payload }) } as any, refs, project, undefined, () => false, 'a.ts'));
    }
    await assert.rejects(inspect({ call: async () => ({ payload: { content_hash: sha('a') } }) } as any, refs, project, undefined, () => false, 'a.ts'), /hash and publication/);
    assert.ok(parseWebviewMessage({ v: 1, type: 'evidence.inspect', requestId: 'selected' }));
    assert.equal(parseWebviewMessage({ v: 1, type: 'evidence.inspect', requestId: 'selected', path: '../foreign.ts' }), undefined);
});

test('citation parsing accepts the supported forms, skips URLs and flags paths outside the collection', () => {
    const text = 'See src/a.ts:12, `src/b.ts:3-9` and docs/c.md#L4-L6 plus C:\\work\\d.ts:2, /etc/e.txt:1 and ../f.ts:2.'
        + ' Link https://example.com/repo/blob/main/g.ts:7 is not a citation. Repeat src/a.ts:12.';
    const { citations, limited } = parseCitations(text);
    assert.equal(limited, false);
    assert.deepEqual(citations.map(entry => [entry.path, entry.startLine, entry.endLine, entry.absolute]), [
        ['src/a.ts', 12, 12, false], ['src/b.ts', 3, 9, false], ['docs/c.md', 4, 6, false],
        ['C:/work/d.ts', 2, 2, true], ['/etc/e.txt', 1, 1, true], ['../f.ts', 2, 2, true],
    ]);
    assert.deepEqual(parseCitations('No citations here, only prose about session.ts.').citations, []);
    const many = Array.from({ length: 60 }, (_, index) => `f${index}.ts:1`).join(' ');
    assert.equal(parseCitations(many).citations.length, 50);
    assert.equal(parseCitations(many).limited, true);
});

test('checking citations reports real statuses from the index and the saved files', async () => {
    const root = await fs.mkdtemp(path.join(os.tmpdir(), 'airk-explorer-check-'));
    try {
        await fs.mkdir(path.join(root, 'src'));
        await fs.writeFile(path.join(root, 'src', 'same.ts'), 'a\nb\nc\n');
        await fs.writeFile(path.join(root, 'src', 'edited.ts'), 'changed\n');
        const info: Record<string, any> = {
            'src/same.ts': { content_hash: sha('a\nb\nc\n'), indexed_lines: 3, source: { root, relative_path: 'src/same.ts' } },
            'src/edited.ts': { content_hash: sha('original\n'), indexed_lines: 1, source: { root, relative_path: 'src/edited.ts' } },
            'src/gone.ts': { content_hash: sha('x\n'), indexed_lines: 5, source: { root, relative_path: 'src/gone.ts' } },
        };
        const calls: string[] = [];
        const session = { call: async (tool: string, args: any) => {
            assert.equal(tool, 'airk_file_info'); calls.push(args.path);
            if (!info[args.path]) { return { payload: { read: { status: 'unknown', reason: 'path is absent from the current index' } }, publication: 'airk-r1:p' }; }
            return { payload: info[args.path], publication: 'airk-r1:p' };
        } } as any;
        const refs = new RefRegistry();
        const view = await checkCitations(session, refs, project,
            'src/same.ts:2 src/same.ts:9 src/edited.ts:1 src/gone.ts:1 src/unknown.ts:1 /abs/x.ts:1 src/same.ts:1-3', () => false);
        assert.equal(view.supported, true);
        assert.deepEqual(view.citations.map(entry => entry.status), ['valid', 'line_out_of_range', 'changed_since_index', 'missing', 'not_found', 'outside_collection', 'valid']);
        assert.deepEqual(calls.sort(), ['src/edited.ts', 'src/gone.ts', 'src/same.ts', 'src/unknown.ts'], 'one lookup per distinct path; absolute paths are never sent');
        assert.equal(refs.get(view.citations[0].ref!, project).path, 'src/same.ts');
        assert.equal(view.publication, 'airk-r1:p');
        const dirty = await checkCitations(session, new RefRegistry(), project, 'src/same.ts:1', () => true);
        assert.equal(dirty.citations[0].status, 'not_saved');
        const prose = await checkCitations(session, new RefRegistry(), project, 'Everything was verified and all tests pass.', () => false);
        assert.equal(prose.supported, false, 'prose without citations is never a pass');
        const down = { call: async () => { throw new EvidenceFailure(explorerError('SERVICE_UNAVAILABLE')); } } as any;
        await assert.rejects(checkCitations(down, new RefRegistry(), project, 'src/same.ts:1', () => false),
            (error: unknown) => error instanceof EvidenceFailure && error.error.code === 'SERVICE_UNAVAILABLE', 'an unavailable service is not reported as "not found"');

        const inspected = await inspect({ call: async () => ({ payload: { ...info['src/same.ts'], language: 'typescript', line_basis: 'source', chunks: 1 }, publication: 'airk-r1:p' }) } as any,
            refs, project, view.citations[0].ref!, () => false);
        assert.equal(inspected.live, 'matches_index');
        assert.equal(inspected.language, 'typescript');
        assert.equal(inspected.sourceRoot, root);
    } finally { await fs.rm(root, { recursive: true, force: true }); }
});

test('changes map subjects, causes and coverage, and report expired cursors explicitly', async () => {
    const payload = { status: 'ready', baseline_status: 'verified', truncated: false, cursor: null,
        changes: [{ subject: { Artifact: { artifact_id: 'x' } }, causes: ['modified'], dimensions: ['content', 'code_body'], observations: 2,
            after: { citation_path: 'src/a.ts', path: 'src/a.ts' } }],
        coverage: { code: { complete_after: 3, reason: 'unsupported' } } };
    const view = await changesSince({ call: async (tool: string, args: any) => {
        assert.equal(tool, 'airk_changes'); assert.equal(args.since, 'airk-r1:old');
        return { payload, publication: 'airk-r1:new' };
    } } as any, 'airk-r1:old', 20);
    assert.deepEqual(view.items, [{ path: 'src/a.ts', subject: 'Artifact', causes: ['modified'], dimensions: ['content', 'code_body'], observations: 2 }]);
    assert.equal(view.baseline, 'verified');
    assert.match(view.coverage[0], /code: complete after generation 3 \(unsupported\)/);
    await assert.rejects(changesSince({ call: async () => ({ payload: { ...payload, status: 'cursor_expired' } }) } as any, 'airk-r1:old', 20),
        (error: unknown) => error instanceof EvidenceFailure && error.error.code === 'EVIDENCE_MISMATCH');
    assert.equal(parseWebviewMessage({ v: 1, type: 'evidence.changes', requestId: 'c', since: 'not-a-token', limit: 5 }), undefined);
    assert.ok(parseWebviewMessage({ v: 1, type: 'evidence.changes', requestId: 'c', since: 'airk-r1:abc:3:def', limit: 5 }));
});

test('Checker never converts tool failures to absence and rejects mixed publications', async () => {
    const failure = { call: async () => { throw new EvidenceFailure(explorerError('TOOL_ERROR', 'integrity failure')); } } as any;
    await assert.rejects(checkCitations(failure, new RefRegistry(), project, 'a.ts:1', () => false), /integrity failure/);
    let count = 0;
    const mixed = { call: async () => ({ publication: `p${++count}`, payload: { content_hash: sha('a'), indexed_lines: 1 } }) } as any;
    await assert.rejects(checkCitations(mixed, new RefRegistry(), project, 'a.ts:1 b.ts:1', () => false), /index changed/);
});

test('Changes pages retain their exact baseline and reject a republished continuation', async () => {
    const handles = new FactHandles(new RefRegistry());
    const calls: any[] = [];
    let publication = 'p';
    const session = { call: async (_tool: string, args: any) => {
        calls.push(args);
        return { publication, payload: { status: 'ready', changes: [], cursor: args.cursor ? null : 'native-cursor' } };
    } } as any;
    const first = await changesSince(session, 'old', 10, handles, project);
    assert.ok(first.more);
    const next = await changesSince(session, '', 1, handles, project, first.more);
    assert.equal(next.more, undefined); assert.equal(next.since, 'old');
    assert.deepEqual(calls[1], { since: 'old', limit: 10, max_bytes: 32768, cursor: 'native-cursor' });
    publication = 'new';
    await assert.rejects(changesSince(session, '', 1, handles, project, first.more), /index changed/);
});
