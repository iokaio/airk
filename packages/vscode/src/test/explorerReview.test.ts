// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { createHash } from 'node:crypto';
import * as fs from 'node:fs';
import Module from 'node:module';
import * as os from 'node:os';
import * as path from 'node:path';
import { test } from 'node:test';
import { FactHandles } from '../explorerFacts';
import { RefRegistry } from '../explorerSearch';

const uri = (fsPath: string) => ({ fsPath });
function load(repositories: unknown[], setting?: string) {
    const stub = {
        extensions: { getExtension: (id: string) => id === 'vscode.git' ? { isActive: true, exports: { getAPI: () => ({ repositories }) } } : undefined },
        workspace: { textDocuments: [], getConfiguration: () => ({ get: (_key: string, fallback: unknown) => setting ?? fallback }) },
    };
    const loader = Module as unknown as { _load: (...args: any[]) => any }, original = loader._load;
    loader._load = function (request: string, ...args: any[]) { return request === 'vscode' ? stub : original.call(this, request, ...args); };
    delete require.cache[require.resolve('../explorerReview')];
    try { return require('../explorerReview') as typeof import('../explorerReview'); } finally { loader._load = original; }
}
const hash = (text: string) => createHash('sha256').update(text).digest('hex');

test('Review joins the editor\'s Git changes with indexed records and live hashes, inside the project root only', async t => {
    const root = fs.realpathSync.native(fs.mkdtempSync(path.join(os.tmpdir(), 'airk-review-')));
    t.after(() => fs.rmSync(root, { recursive: true, force: true }));
    fs.mkdirSync(path.join(root, 'src')); fs.mkdirSync(path.join(root, 'test'));
    fs.writeFileSync(path.join(root, 'src', 'session.ts'), 'export function renew() {}\n');
    fs.writeFileSync(path.join(root, 'src', 'edited.ts'), 'changed after indexing\n');
    const repository = { rootUri: uri(root), state: {
        indexChanges: [{ uri: uri(path.join(root, 'src', 'session.ts')), status: 0 }],
        workingTreeChanges: [{ uri: uri(path.join(root, 'src', 'session.ts')), status: 5 }, { uri: uri(path.join(root, 'src', 'edited.ts')), status: 5 },
            { uri: uri(path.join(root, 'src', 'gone.ts')), status: 6 }, { uri: uri(path.join(root, 'new.md')), status: 7 }, { uri: uri(path.join(path.dirname(root), 'outside.ts')), status: 5 }],
        mergeChanges: [] } };
    const review = load([repository]);
    const calls: string[] = [];
    const session = { call: async (tool: string, args: Record<string, any>) => {
        calls.push(`${tool} ${args.path ?? args.path_prefix ?? args.name}`);
        if (tool === 'airk_file_info') {
            const indexed: Record<string, string> = { 'src/session.ts': hash('export function renew() {}\n'), 'src/edited.ts': hash('original\n') };
            return { publication: 'airk-r1:p:1', payload: indexed[args.path] ? { content_hash: indexed[args.path], language: 'typescript' } : {} };
        }
        if (tool === 'airk_symbols') {
            return { publication: 'airk-r1:p:1', payload: args.path_prefix === 'src/session.ts'
                ? { items: [{ kind: 'function', name: 'renew', citation: { path: 'src/session.ts', start_line: 1, end_line: 1, content_hash: hash('x') } }] } : { items: [] } };
        }
        assert.equal(tool, 'airk_references');
        return { publication: 'airk-r1:p:1', payload: { evidence: { hits: [
            { citation: { path: 'src/auth.ts', start_line: 4, end_line: 4, content_hash: hash('a') }, text: 'renew()' },
            { citation: { path: 'test/session.test.ts', start_line: 9, end_line: 9, content_hash: hash('b') }, text: 'expect(renew()).toBe(1)' },
        ] } } };
    } } as any;
    const context = { session, handles: new FactHandles(new RefRegistry()), project: 'ab'.repeat(32), root };

    const changes = await review.reviewChanges(context);
    assert.equal(changes.git, 'available');
    assert.deepEqual(changes.files.map(file => [file.path, file.change, file.staged, file.indexed, file.live]), [
        ['new.md', 'untracked', false, false, 'unchecked'],
        ['src/edited.ts', 'modified', false, true, 'changed_since_index'],
        ['src/gone.ts', 'deleted', false, false, 'deleted'],
        ['src/session.ts', 'modified', true, true, 'matches_index'],
    ], 'one row per file, staged first wins, nothing outside the root');
    assert.ok(!calls.some(call => call.includes('outside')), 'no read for paths outside the project');
    assert.ok(changes.notes.some(note => /AIRK does not keep history/.test(note)));

    const impact = await review.reviewImpact(context);
    assert.deepEqual(impact.items.map(item => [item.name, item.references, item.testReferences, item.referenceFiles]), [['renew', 2, 1, ['src/auth.ts', 'test/session.test.ts']]]);
    assert.match(impact.notes[0], /checklist, not a verdict/);

    const tests = await review.reviewTests(context);
    assert.deepEqual(tests.candidates.map(candidate => `${candidate.path}:${candidate.line} ${candidate.mentions}`), ['test/session.test.ts:9 renew']);
    assert.match(tests.notes[0], /not executed, and this is not coverage/);
});

test('without Git or a repository Review says so, and the test-path setting falls back when invalid', async () => {
    const none = load([]);
    const context = { session: { call: async () => { throw new Error('no AIRK call expected'); } } as any, handles: new FactHandles(new RefRegistry()), project: 'ab'.repeat(32), root: os.tmpdir() };
    assert.equal((await none.reviewChanges(context)).git, 'no_repository');
    const manifest = JSON.parse(fs.readFileSync(path.resolve(__dirname, '..', '..', 'package.json'), 'utf8'));
    const properties = (Array.isArray(manifest.contributes.configuration) ? manifest.contributes.configuration[0] : manifest.contributes.configuration).properties;
    assert.equal(properties['airk.explorer.review.testPathPattern'].default, none.DEFAULT_TEST_PATTERN, 'the setting default is the code default');
    const pattern = new RegExp(none.DEFAULT_TEST_PATTERN);
    for (const file of ['test/a.ts', 'src/__tests__/a.ts', 'a.test.ts', 'pkg/a_test.go', 'spec/a.rb', 'a.spec.js']) { assert.ok(pattern.test(file), file); }
    for (const file of ['src/testing.ts', 'src/attest.ts', 'contest/a.ts']) { assert.ok(!pattern.test(file), file); }
    const broken = load([], '([');
    assert.equal((await broken.reviewTests(context)).pattern, none.DEFAULT_TEST_PATTERN.replaceAll('/', '\\/'));
});
