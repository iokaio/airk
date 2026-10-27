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
import { EvidenceClient } from '../explorerEvidence';
import { NativeServiceFixture } from './nativeServiceFixture';

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
            return { publication: 'airk-r1:p:1', payload: indexed[args.path] ? { content_hash: indexed[args.path], language: 'typescript' } : { read: { status: 'unknown', reason: 'path is absent from the current index' } } };
        }
        if (tool === 'airk_symbols') {
            return { publication: 'airk-r1:p:1', payload: args.path_prefix === 'src/session.ts'
                ? { items: [{ kind: 'function', name: 'renew', citation: { path: 'src/session.ts', start_line: 1, end_line: 1, content_hash: hash('x') } }] } : { items: [] } };
        }
        if (tool === 'airk_list_files') { return { publication: 'airk-r1:p:1', payload: { paths: ['test/session.test.ts'] } }; }
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

test('Git status numbers follow the vscode.git enum, including type changes and merge states', () => {
    const review = load([]);
    const kinds = [0, 1, 2, 3, 5, 6, 7, 9, 10, 11, 12, 16, 17, 18].map(review.kind);
    assert.deepEqual(kinds, ['modified', 'added', 'deleted', 'renamed', 'modified', 'deleted', 'untracked', 'added', 'renamed', 'modified', 'conflicted', 'conflicted', 'conflicted', 'conflicted']);
});

test('documentation cannot crowd out source changes; scoped test reads survive a crowded global hit limit', async () => {
    const root = path.resolve('review-many-files');
    const files = [...Array.from({ length: 15 }, (_, at) => `docs/${at}.md`), 'src/main.ts', 'test/main.test.ts'];
    const review = load([{ rootUri: uri(root), state: { indexChanges: [], workingTreeChanges: files.map(file => ({ uri: uri(path.join(root, file)), status: 5 })), mergeChanges: [] } }]);
    const calls: { tool: string; args: any }[] = [];
    const session = { call: async (tool: string, args: any) => {
        calls.push({ tool, args });
        if (tool === 'airk_symbols') {
            return { publication: 'p', payload: { files: [{ path: args.path_prefix, content_hash: hash('source') }], items: args.path_prefix === 'src/main.ts'
                ? [{ file: 0, kind: 'definition', name: 'changedApi', signature: 'export function changedApi()', lines: [5, 9] }] : [] } };
        }
        assert.equal(tool, 'airk_references');
        return { publication: 'p', payload: { evidence: { truncated: !args.path_prefix, hits: args.path_prefix ? [
            { citation: { path: 'test/main.test.ts', start_line: 7, end_line: 7, content_hash: hash('test') }, text: 'assert.equal(changedApi(), 1)' },
        ] : Array.from({ length: 20 }, (_, at) => ({ citation: { path: `src/caller-${at}.ts`, start_line: 1, end_line: 1, content_hash: hash('call') }, text: 'changedApi()' })) } } };
    } } as any;
    const context = { session, handles: new FactHandles(new RefRegistry()), project: 'ab'.repeat(32), root };
    const impact = await review.reviewImpact(context);
    assert.equal(calls[0].args.path_prefix, 'src/main.ts');
    assert.equal(impact.items[0].testReferences, 1);
    assert.equal(impact.items[0].references, 21); assert.equal(impact.truncated, true);
    const tests = await review.reviewTests(context);
    assert.ok(tests.candidates.some(item => item.path === 'test/main.test.ts' && item.line === undefined && item.reason === 'changed_test_file'));
    assert.ok(tests.candidates.some(item => item.path === 'test/main.test.ts' && item.line === 7 && item.mentions === 'changedApi'));
    assert.ok(calls.some(call => call.tool === 'airk_references' && call.args.path_prefix === 'test/'));
});

test('changed tests are visible even when the index has no declarations', async () => {
    const root = path.resolve('review-unindexed-test');
    const review = load([{ rootUri: uri(root), state: { indexChanges: [], workingTreeChanges: [{ uri: uri(path.join(root, 'tests', 'fresh.test.ts')), status: 7 }], mergeChanges: [] } }]);
    const result = await review.reviewTests({ root, handles: new FactHandles(new RefRegistry()), project: 'ab'.repeat(32), session: { call: async () => ({ publication: 'p', payload: { items: [] } }) } as any });
    assert.equal(result.declarations, 0);
    assert.deepEqual(result.candidates, [{ path: 'tests/fresh.test.ts', mentions: '', text: '', reason: 'changed_test_file' }]);
    assert.ok(result.notes.some(note => note.includes('No usable indexed declarations')));
});

test('Review finds source impact and tests through real native minimal symbols and nested reference publications', { skip: !process.env.AIRK_TEST_BINARY, timeout: 300000 }, async () => {
    const fixture = await NativeServiceFixture.create(process.env.AIRK_TEST_BINARY!);
    let session: EvidenceClient | undefined;
    try {
        const spec = await fixture.register('review-real');
        await fs.promises.mkdir(path.join(spec.root, 'src')); await fs.promises.mkdir(path.join(spec.root, 'tests')); await fs.promises.mkdir(path.join(spec.root, 'docs'));
        await fs.promises.writeFile(path.join(spec.root, 'src', 'contract.ts'), 'export function ChangedContract(value: number) { return value + 1; }\n');
        await fs.promises.writeFile(path.join(spec.root, 'tests', 'contract.test.ts'), 'import { ChangedContract } from "../src/contract";\nconst expected = ChangedContract(1) === 2;\n');
        const files = [...Array.from({ length: 15 }, (_, at) => `docs/${at}.md`), 'src/contract.ts', 'tests/contract.test.ts'];
        for (const file of files.filter(file => file.endsWith('.md'))) { await fs.promises.writeFile(path.join(spec.root, file), '# Fictional review fixture\n'); }
        await fixture.configure(spec, { operation: 'vectors', enabled: false }); await fixture.job(spec); await fixture.publishedStatus(spec);
        const review = load([{ rootUri: uri(spec.root), state: { indexChanges: [], workingTreeChanges: files.map(file => ({ uri: uri(path.join(spec.root, file)), status: 5 })), mergeChanges: [] } }]);
        session = new EvidenceClient(spec, 'review-native-test', { timeout: 60000 });
        const context = { root: spec.root, session, handles: new FactHandles(new RefRegistry()), project: spec.service!.project };
        const impact = await review.reviewImpact(context);
        assert.ok(impact.items.some(item => item.name === 'ChangedContract' && item.testReferences >= 1), JSON.stringify(impact));
        const tests = await review.reviewTests(context);
        assert.ok(tests.candidates.some(item => item.path === 'tests/contract.test.ts' && item.reason === 'changed_test_file'));
        assert.ok(tests.candidates.some(item => item.path === 'tests/contract.test.ts' && item.line && item.text.includes('ChangedContract')));
        assert.equal(tests.publication, impact.publication);
    } finally { session?.dispose(); await fixture.dispose(); }
});

test('Review preserves failed lookups and refuses mixed declarations/reference publications', async () => {
    const root = path.resolve('review-root');
    const review = load([{ rootUri: uri(root), state: { indexChanges: [], workingTreeChanges: [{ uri: uri(path.join(root, 'a.ts')), status: 5 }], mergeChanges: [] } }]);
    const base = { handles: new FactHandles(new RefRegistry()), project: 'ab'.repeat(32), root };
    await assert.rejects(review.reviewChanges({ ...base, session: { call: async () => { throw new Error('integrity failed'); } } as any }), /integrity/);
    const session = { call: async (tool: string) => tool === 'airk_list_files' ? { publication: 'p1', payload: { paths: ['test/a.test.ts'] } } : tool === 'airk_symbols' ? { publication: 'p1', payload: { items: [{ kind: 'function', name: 'changed', citation: { path: 'a.ts', start_line: 1, end_line: 1, content_hash: hash('a') } }] } }
        : { publication: 'p2', payload: { evidence: { hits: [] } } } } as any;
    await assert.rejects(review.reviewImpact({ ...base, session }), /index changed/);
    await assert.rejects(review.reviewTests({ ...base, session }), /index changed/);
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
