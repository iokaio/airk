// SPDX-License-Identifier: Apache-2.0
// Read-only index library (U97): the exact native catalog methods relayed
// through the window's airkctl session, and complete responses and errors.
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { LIBRARY_PAGE_BYTES, LIBRARY_PAGE_LIMIT, libraryMethod, readLibrary } from '../indexLibrary';

const index = 'idx_' + '12'.repeat(16), set = 'prj_' + '34'.repeat(16), operation = 'ab'.repeat(16);
const cursor = { kind: 'indexes', namespace: 'cd'.repeat(16), revision: 9, after: index, index_id: null, project_id: null };
const request = (fields: Record<string, unknown>) => ({ catalog_page: { request: { index_id: null, project_id: null, after: null, expected_catalog_revision: null, limit: 32, max_bytes: 49152, ...fields } } });

test('library reads are the native catalog methods with the reviewed page budget', () => {
    assert.equal(LIBRARY_PAGE_LIMIT, 32); assert.equal(LIBRARY_PAGE_BYTES, 49152);
    assert.deepEqual(libraryMethod({ method: 'page', kind: 'indexes' }), { expected: 'catalog_page', method: request({ kind: 'indexes' }) });
    assert.deepEqual(libraryMethod({ method: 'page', kind: 'projects', after: cursor, revision: 9 }).method, request({ kind: 'projects', after: cursor, expected_catalog_revision: 9 }));
    assert.deepEqual(libraryMethod({ method: 'page', kind: 'sources', id: index }).method, request({ kind: 'sources', index_id: index }));
    assert.deepEqual(libraryMethod({ method: 'page', kind: 'memberships', id: index }).method, request({ kind: 'memberships', index_id: index }));
    assert.deepEqual(libraryMethod({ method: 'page', kind: 'memberships', id: set }).method, request({ kind: 'memberships', project_id: set }));
    assert.deepEqual(libraryMethod({ method: 'page', kind: 'memberships' }).method, request({ kind: 'memberships' }));
    assert.deepEqual(libraryMethod({ method: 'detail', id: index }), { expected: 'catalog_detail', method: { catalog_detail: { entity: { kind: 'index', id: index }, expected_catalog_revision: null } } });
    assert.deepEqual(libraryMethod({ method: 'detail', id: set }).method, { catalog_detail: { entity: { kind: 'project', id: set }, expected_catalog_revision: null } });
    assert.deepEqual(libraryMethod({ method: 'policy', id: index }), { expected: 'catalog_policy_status', method: { catalog_policy_status: { index_id: index } } });
    assert.deepEqual(libraryMethod({ method: 'split', operation }), { expected: 'catalog_split_status', method: { catalog_split_status: { operation_id: operation } } });
});

test('library reads refuse what the native validator refuses', () => {
    for (const bad of [
        { method: 'page' }, { method: 'page', kind: 'sources' }, { method: 'page', kind: 'sources', id: set }, { method: 'page', kind: 'indexes', id: index },
        { method: 'page', kind: 'projects', revision: 3 }, { method: 'detail' }, { method: 'policy', id: set }, { method: 'split' }, { method: 'split', operation: 'xyz' },
    ]) { assert.throws(() => libraryMethod(bad as never), /Index library request is invalid/, JSON.stringify(bad)); }
});

test('a read returns the complete native response, and a failure keeps its native code instead of an empty page', async () => {
    const response = { status: 'catalog_page', page: { namespace: 'cd'.repeat(16), revision: 9, items: [], continuation: null }, capabilities: { future_native_field: 'kept' } };
    const sent: unknown[] = [];
    const ok = await readLibrary({ method: 'page', kind: 'indexes' }, async method => { sent.push(method); return response; }, () => 5);
    assert.deepEqual(ok, { method: request({ kind: 'indexes' }), expected: 'catalog_page', response, observedAt: 5 });
    assert.equal(sent.length, 1);
    const refused = await readLibrary({ method: 'detail', id: index }, async () => { throw Object.assign(new Error('session lost'), { code: 'session_unavailable', retryable: true }); }, () => 6);
    assert.deepEqual(refused.error, { code: 'session_unavailable', message: 'session lost', retryable: true }); assert.equal(refused.response, undefined);
    const unavailable = await readLibrary({ method: 'policy', id: index }, async () => { throw Object.assign(new Error('AIRK Service is unavailable: stopped'), { reason: 'stopped' }); });
    assert.equal(unavailable.error?.code, 'stopped');
    const other = { status: 'source_change_busy', code: 'catalog_busy', busy: { pending_jobs: 1 } };
    const odd = await readLibrary({ method: 'page', kind: 'indexes' }, async () => other);
    assert.deepEqual(odd.response, other, 'an unexpected response is kept complete'); assert.equal(odd.error?.code, 'catalog_busy');
    const unknown = await readLibrary({ method: 'page', kind: 'indexes' }, async () => ({ status: 'native_read', result: {} }));
    assert.equal(unknown.error?.code, 'unexpected_native_read');
});
