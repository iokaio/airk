// SPDX-License-Identifier: Apache-2.0
// Independent serial callback traces for R19a. Set AIRK_R19_CAPTURE to run
// the captured original procedure bodies instead of the relocated owners.
import assert from 'node:assert/strict';
import test from 'node:test';
import { readFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { build } from 'esbuild';

const sourceDir = resolve(dirname(fileURLToPath(import.meta.url)), '../src');
const capture = process.env.AIRK_R19_CAPTURE
  ? JSON.parse(await readFile(resolve(process.env.AIRK_R19_CAPTURE), 'utf8')) : null;
async function owner(name) {
  const source = capture?.modules[name] ?? await readFile(resolve(sourceDir, `${name}.ts`), 'utf8');
  const bundled = await build({ stdin: { contents: source, resolveDir: sourceDir, loader: 'ts' },
    bundle: true, write: false, format: 'esm', platform: 'node', logLevel: 'silent' });
  return import(`data:text/javascript;base64,${Buffer.from(bundled.outputFiles[0].contents).toString('base64')}`);
}
const { createJobRequests, createIndexRequests } = await owner('activityRequests');
const { createCatalogRequests, indexDetailValue } = await owner('catalogRequests');
const ok = result => ({ kind: 'ok', status: 200, envelope: { schema: 'airk-admin/1', ok: true, operation: 'fixture', result } });
const failed = { kind: 'failed', status: 409, envelope: { schema: 'airk-admin/1', ok: false,
  error: { code: 'refused', class: 'refused', retryable: false } } };

test('serial Activity open/retry/cancel preserves exact calls and UI callback order', async () => {
  const trace = [];
  const records = [{ id: 'job-A', state: 'failed' }, { id: 'job-A', state: 'pending' }, { id: 'job-A', state: 'canceled' }];
  let at = 0;
  const run = async (operation, body) => { trace.push(['call', operation, body]); return ok({ status: 'job', job: records[at++] }); };
  const requests = createJobRequests({ runJob: run, runJobAction: run,
    setJobId: id => trace.push(['selected', id]), setJobRecord: record => trace.push(['record', record]) });
  await requests.openJob('job-A');
  await requests.jobChange('job-A', 'retry');
  await requests.jobChange('job-A', 'cancel');
  assert.deepEqual(trace, [
    ['selected', 'job-A'], ['record', null], ['call', 'jobs.get', { job: { id: 'job-A' } }],
    ['record', { id: 'job-A', state: 'failed' }],
    ['call', 'jobs.retry', { retry: { id: 'job-A' } }], ['record', { id: 'job-A', state: 'pending' }],
    ['call', 'jobs.cancel', { cancel: { id: 'job-A' } }], ['record', { id: 'job-A', state: 'canceled' }],
  ]);
});

test('serial failed Activity detail clears record; failed action leaves it alone', async () => {
  const trace = [];
  const run = async (operation, body) => { trace.push(['call', operation, body]); return failed; };
  const requests = createJobRequests({ runJob: run, runJobAction: run,
    setJobId: id => trace.push(['selected', id]), setJobRecord: record => trace.push(['record', record]) });
  await requests.openJob('job-A');
  await requests.jobChange('job-A', 'cancel');
  assert.deepEqual(trace, [
    ['selected', 'job-A'], ['record', null], ['call', 'jobs.get', { job: { id: 'job-A' } }], ['record', null],
    ['call', 'jobs.cancel', { cancel: { id: 'job-A' } }],
  ]);
});

test('serial catalog first/continuation pages preserve calls, rows, cursor, header and callbacks', async () => {
  const trace = [];
  const a = { kind: 'index', value: { id: 'index-A', name: 'A' } };
  const b = { kind: 'index', value: { id: 'index-B', name: 'B' } };
  const cursor = { namespace: 'ns-A', revision: 7, after: 'index-A' };
  const pages = [
    { namespace: 'ns-A', revision: 7, items: [a], continuation: cursor },
    { namespace: 'ns-A', revision: 7, items: [b], continuation: null },
  ];
  let at = 0;
  const requests = createCatalogRequests({
    run: async (operation, body) => { trace.push(['call', operation, body]); return ok({ status: 'catalog_page', page: pages[at++] }); },
    setItems: value => trace.push(['rows', value]), setCursor: value => trace.push(['cursor', value]),
    setHeader: value => trace.push(['header', value]),
  });
  const props = { kind: 'indexes', onRevision: value => trace.push(['revision', value]),
    onItems: (value, complete) => trace.push(['items', value, complete]) };
  const filter = { indexId: null, projectId: null };
  await requests.load(null, props, filter, []);
  await requests.load(cursor, props, filter, [a]);
  assert.deepEqual(trace, [
    ['call', 'catalog.page', { catalog_page: { request: { kind: 'indexes', index_id: null, project_id: null,
      after: null, expected_catalog_revision: null, limit: 32, max_bytes: 49152 } } }],
    ['rows', [a]], ['items', [a], false], ['cursor', cursor], ['header', { namespace: 'ns-A', revision: 7 }], ['revision', 7],
    ['call', 'catalog.page', { catalog_page: { request: { kind: 'indexes', index_id: null, project_id: null,
      after: cursor, expected_catalog_revision: 7, limit: 32, max_bytes: 49152 } } }],
    ['rows', [a, b]], ['items', [a, b], true], ['cursor', null], ['header', { namespace: 'ns-A', revision: 7 }], ['revision', 7],
  ]);
});

test('serial failed catalog page publishes no derived state', async () => {
  const trace = [];
  const requests = createCatalogRequests({ run: async () => failed,
    setItems: value => trace.push(value), setCursor: value => trace.push(value), setHeader: value => trace.push(value) });
  await requests.load(null, { kind: 'indexes', onRevision: value => trace.push(value), onItems: value => trace.push(value) },
    { indexId: null, projectId: null }, []);
  assert.deepEqual(trace, []);
});

test('serial IndexDetail retains same-index details only during a pending reload', () => {
  const detail = { namespace: 'ns-A', revision: 7, item: { kind: 'index', value: { id: 'index-A', name: 'A' } } };
  const cache = { current: null };
  assert.equal(indexDetailValue({ id: 'index-A' }, { phase: 'done' }, detail, cache), detail);
  assert.deepEqual(cache.current, { id: 'index-A', detail });
  assert.equal(indexDetailValue({ id: 'index-A' }, { phase: 'pending' }, null, cache), detail);
  assert.equal(indexDetailValue({ id: 'index-B' }, { phase: 'pending' }, null, cache), null);
  assert.equal(indexDetailValue({ id: 'index-A' }, { phase: 'done' }, null, cache), null);
});

test('serial index-name loader preserves catalog/registry calls and callback order', async () => {
  const trace = [];
  const requests = createIndexRequests({
    runCatalog: async (operation, body) => {
      trace.push(['call', operation, body]);
      return ok({ status: 'catalog_page', page: { namespace: 'ns-A', revision: 7, continuation: null,
        items: [{ kind: 'index', value: { id: 'index-A', name: 'Index A', tombstone: false, backing: { binding: 'binding-A' } } }] } });
    },
    runProjects: async (operation, body) => {
      trace.push(['call', operation, body]);
      return ok({ status: 'projects', page: { revision: 3, projects: [{ registration: { binding: 'binding-A' }, paused: true }], after: null } });
    },
    setRegistryKnown: value => trace.push(['known', value]), setPartial: value => trace.push(['partial', value]),
    setIndexes: value => trace.push(['indexes', value]),
  });
  await requests.reload();
  assert.deepEqual(trace, [
    ['call', 'catalog.page', { catalog_page: { request: { kind: 'indexes', index_id: null, project_id: null,
      after: null, expected_catalog_revision: null, limit: 32, max_bytes: 49152 } } }],
    ['call', 'indexes.list', { projects: { after: null, revision: null, limit: 32 } }],
    ['known', true], ['partial', false],
    ['indexes', [{ binding: 'binding-A', name: 'Index A', id: 'index-A', paused: true, retired: false, registered: true }]],
  ]);
});
