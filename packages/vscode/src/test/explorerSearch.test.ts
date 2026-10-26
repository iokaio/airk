// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { createHash } from 'node:crypto';
import * as fs from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';
import { test } from 'node:test';
import type { WebviewMessageOf } from '../explorerContracts';
import { EvidenceFailure } from '../explorerEvidence';
import { appliedMode, passageView, RefRegistry, relativePrefix, searchArgs, searchView } from '../explorerSearch';
import { displayPath, inside, liveCheck, sourceFile } from '../explorerSource';

const project = 'ab'.repeat(32), hash = 'ba'.repeat(32);
const message = (fields: Partial<WebviewMessageOf<'search.run'>> = {}): WebviewMessageOf<'search.run'> =>
    ({ v: 1, type: 'search.run', requestId: 'r', query: 'refreshSession', mode: 'policy', limit: 2, maxBytes: 16384, fullChunks: false, ...fields });
const failure = (code: string) => (error: unknown) => error instanceof EvidenceFailure && error.error.code === code;
// Shape captured from a real 1.4.4 airk_search reply (vectors disabled, two hits).
const payload = {
    collection: 'probe', truncated: true,
    semantic_readiness: { state: 'disabled', default_search_policy: 'partial_hybrid' },
    publication: { token: 'airk-r1:x:3:y' },
    hits: [
        { citation: { path: 'notes.md', start_line: 1, end_line: 3, content_hash: hash, location: { Lines: { start: 1, end: 3 } } }, text: '# Notes', truncated: false },
        { citation: { path: 'session.ts', start_line: 1, end_line: 3, content_hash: hash, location: { Lines: { start: 1, end: 3 } } }, text: 'export function', truncated: true },
        { citation: { path: 'broken.ts', start_line: 0, end_line: 1, content_hash: hash }, text: 'dropped: invalid line', truncated: false },
    ],
};

test('search arguments keep scope relative and send a mode only when one is chosen', () => {
    assert.deepEqual(searchArgs(message()), { query: 'refreshSession', limit: 2, max_bytes: 16384, full_chunks: false });
    assert.deepEqual(searchArgs(message({ mode: 'lexical', pathPrefix: 'src\\auth\\' })), { query: 'refreshSession', limit: 2, max_bytes: 16384, full_chunks: false, mode: 'lexical', path_prefix: 'src/auth/' });
    assert.equal(relativePrefix('  '), undefined);
    for (const bad of ['/etc', 'C:/Windows', 'src/../../secret', '..', 'a\u0001b']) { assert.throws(() => relativePrefix(bad), failure('INVALID_MESSAGE'), bad); }
});

test('the applied mode and ordering are reported from the reply, never assumed', () => {
    assert.deepEqual(appliedMode('lexical', {}, {}), { mode: 'lexical', ordering: 'index_identity' });
    assert.equal(appliedMode('hybrid', {}, {}).ordering, 'fused_rank');
    assert.deepEqual(appliedMode('policy', payload, {}), { mode: 'partial hybrid → lexical (vectors disabled)', ordering: 'index_identity', semantic: 'disabled' });
    assert.equal(appliedMode('policy', { semantic_readiness: { state: 'partial', default_search_policy: 'partial_hybrid' } }, {}).ordering, 'fused_rank');
    assert.equal(appliedMode('policy', {}, { semantic: 'pending' }).ordering, 'index_identity');
    assert.deepEqual(appliedMode('policy', {}, {}), { mode: 'collection policy', ordering: 'unknown' });
    assert.equal(appliedMode('policy', {}, { vectorsEnabled: false, semantic: 'ready' }).ordering, 'index_identity');
});

test('search results become cited hit views with opaque references and honest counts', () => {
    const refs = new RefRegistry();
    const view = searchView(project, message(), payload, 'airk-r1:x:3:y', refs, {});
    assert.equal(view.returned, 2, 'malformed citations are dropped, not repaired');
    assert.equal(view.truncated, true);
    assert.equal(view.ordering, 'index_identity');
    assert.match(view.warnings.join(' '), /not relevance/);
    assert.match(view.warnings.join(' '), /truncated/);
    assert.deepEqual(view.hits.map(hit => [hit.path, hit.startLine, hit.evidenceClass]), [['notes.md', 1, 'E1'], ['session.ts', 1, 'E1']]);
    for (const hit of view.hits) { assert.match(hit.ref, /^h[0-9a-z]+$/); }
    assert.equal(refs.get(view.hits[1].ref, project).path, 'session.ts');
    assert.throws(() => refs.get(view.hits[1].ref, 'cd'.repeat(32)), failure('UNKNOWN_REFERENCE'));
    assert.throws(() => refs.get('h999', project), failure('UNKNOWN_REFERENCE'));
    const passage = passageView(project, { hits: [payload.hits[1]], truncated: false }, 'airk-r1:x:3:y', refs);
    assert.equal(passage.path, 'session.ts'); assert.equal(passage.truncated, true);
    assert.throws(() => passageView(project, { hits: [] }, undefined, refs), failure('TOOL_ERROR'));
});

test('the reference registry is bounded and expires the oldest handles first', () => {
    const refs = new RefRegistry(2);
    const first = refs.add({ project, path: 'a', startLine: 1, endLine: 1, contentHash: hash });
    refs.add({ project, path: 'b', startLine: 1, endLine: 1, contentHash: hash });
    refs.add({ project, path: 'c', startLine: 1, endLine: 1, contentHash: hash });
    assert.throws(() => refs.get(first, project), failure('UNKNOWN_REFERENCE'));
});

test('live checks hash raw saved bytes and source paths stay inside their root', async () => {
    assert.equal(displayPath('\\\\?\\C:\\work\\repo'), 'C:\\work\\repo');
    assert.equal(displayPath('\\\\?\\UNC\\server\\share'), '\\\\server\\share');
    assert.equal(displayPath('/home/user/repo'), '/home/user/repo');
    const root = await fs.mkdtemp(path.join(os.tmpdir(), 'airk-explorer-source-'));
    try {
        const bytes = Buffer.from('\ufeffexport function a() {}\r\n', 'utf8');
        await fs.writeFile(path.join(root, 'a.ts'), bytes);
        const digest = createHash('sha256').update(bytes).digest('hex');
        const file = await sourceFile(root, 'a.ts');
        assert.ok(inside(root, file) || inside(await fs.realpath(root), file));
        assert.equal(await liveCheck(file, digest, false), 'matches_index', 'BOM and CRLF are part of the hashed bytes');
        assert.equal(await liveCheck(file, digest, true), 'not_saved');
        await fs.writeFile(file, 'export function a() {}\n');
        assert.equal(await liveCheck(file, digest, false), 'changed_since_index');
        assert.equal(await liveCheck(path.join(root, 'missing.ts'), digest, false), 'missing');
        for (const bad of ['../outside.ts', '/etc/passwd', 'sub/../../x']) { await assert.rejects(sourceFile(root, bad), failure('TOOL_ERROR'), bad); }
        await assert.rejects(sourceFile('relative/root', 'a.ts'), failure('TOOL_ERROR'));
    } finally { await fs.rm(root, { recursive: true, force: true }); }
});
