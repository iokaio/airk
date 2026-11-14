// SPDX-License-Identifier: Apache-2.0
// R19 failures exercise the production request/state procedures used by the UI.
import assert from 'node:assert/strict';
import test from 'node:test';
import { readFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { build } from 'esbuild';
import ts from 'typescript';

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
const { createCatalogRequests, indexDetailValue, setDetailValue } = await owner('catalogRequests');
const deferred = () => {
  let resolve;
  const work = new Promise(yes => { resolve = yes; });
  return { work, resolve };
};
const ok = result => ({ kind: 'ok', status: 200, envelope: { schema: 'airk-admin/1', ok: true, operation: 'fixture', result } });
const job = (id, state = 'failed') => ok({ status: 'job', job: { id, state } });
const page = (id, revision, namespace = 'ns-A') => ok({ status: 'catalog_page',
  page: { namespace, revision, items: [{ kind: 'index', value: { id } }], continuation: null } });

test('race reverse-order job reads never pair A record with B selection', async () => {
  const a = deferred(), b = deferred();
  let selected = '', record = null;
  const requests = createJobRequests({ runJob: async (_operation, body) => body.job.id === 'job-A' ? a.work : b.work,
    runJobAction: async () => { throw new Error('unexpected action'); },
    setJobId: value => { selected = value; }, setJobRecord: value => { record = value; } });
  const first = requests.openJob('job-A'), latest = requests.openJob('job-B');
  b.resolve(job('job-B')); await latest;
  a.resolve(job('job-A')); await first;
  assert.equal(selected, 'job-B');
  assert.equal(record?.id, 'job-B');
});

for (const kind of ['retry', 'cancel']) {
  test(`race pending ${kind} A cannot replace the newly selected B record`, async () => {
    const action = deferred();
    let selected = '', record = null;
    const requests = createJobRequests({ runJob: async (_operation, body) => job(body.job.id),
      runJobAction: async () => action.work, setJobId: value => { selected = value; }, setJobRecord: value => { record = value; } });
    await requests.openJob('job-A');
    const pending = requests.jobChange('job-A', kind);
    await requests.openJob('job-B');
    action.resolve(job('job-A', kind === 'retry' ? 'pending' : 'canceled')); await pending;
    assert.equal(selected, 'job-B');
    assert.equal(record?.id, 'job-B');
  });
}

function catalog(run) {
  let rows = [], cursor = null, header = null;
  const revisions = [], published = [];
  const requests = createCatalogRequests({ run, setItems: value => { rows = value; },
    setCursor: value => { cursor = value; }, setHeader: value => { header = value; } });
  const props = { kind: 'indexes', onRevision: value => revisions.push(value), onItems: value => published.push(value) };
  const context = (filter = { indexId: null, projectId: null }, reload = 0) =>
    requests.bind({ kind: 'indexes', ...filter, reload, session: undefined });
  return { requests, props, revisions, published, context, rows: () => rows, cursor: () => cursor, header: () => header };
}

test('race a late catalog filter response cannot replace the newer filter rows or metadata', async () => {
  const a = deferred(), b = deferred();
  const f = catalog(async (_operation, body) => body.catalog_page.request.index_id === 'index-A' ? a.work : b.work);
  const older = f.requests.load(null, f.props, f.context({ indexId: 'index-A', projectId: null }));
  const newer = f.requests.load(null, f.props, f.context({ indexId: 'index-B', projectId: null }));
  b.resolve(page('index-B', 9)); await newer;
  a.resolve(page('index-A', 7)); await older;
  assert.deepEqual(f.rows(), [{ kind: 'index', value: { id: 'index-B' } }]);
  assert.deepEqual(f.header(), { namespace: 'ns-A', revision: 9 });
  assert.deepEqual(f.revisions, [9]);
  assert.equal(f.published.length, 1);
});

test('race a late continuation cannot overwrite a completed reload with its captured old rows', async () => {
  const continuation = deferred(), reload = deferred();
  const f = catalog(async (_operation, body) => body.catalog_page.request.after === null ? reload.work : continuation.work);
  const filter = { indexId: null, projectId: null };
  const context = f.context(filter);
  const old = f.requests.load({ namespace: 'ns-A', revision: 7, after: 'old-first-page' }, f.props, context);
  const latest = f.requests.load(null, f.props, context);
  reload.resolve(page('fresh-first-page', 9)); await latest;
  continuation.resolve(page('old-second-page', 7)); await old;
  assert.deepEqual(f.rows(), [{ kind: 'index', value: { id: 'fresh-first-page' } }]);
  assert.deepEqual(f.revisions, [9]);
});

test('race loaded A detail cannot be relabeled and cached as current B', () => {
  const loadedA = { namespace: 'ns-A', revision: 7, item: { kind: 'index', value: { id: 'index-A', name: 'A' } } };
  const cache = { current: null };
  indexDetailValue({ id: 'index-A' }, { phase: 'done' }, loadedA, cache);
  assert.equal(indexDetailValue({ id: 'index-B' }, { phase: 'done' }, loadedA, cache), null);
  assert.notEqual(cache.current?.id, 'index-B');
});

for (const changed of ['namespace', 'revision']) {
  test(`race a continuation from another ${changed} publishes no rows or metadata`, async () => {
    const f = catalog(async () => page('foreign-page', changed === 'revision' ? 8 : 7, changed === 'namespace' ? 'ns-B' : 'ns-A'));
    await f.requests.load({ namespace: 'ns-A', revision: 7, after: 'index-A' }, f.props, f.context());
    assert.deepEqual(f.rows(), []);
    assert.deepEqual(f.revisions, []);
    assert.deepEqual(f.published, []);
    assert.equal(f.header(), null);
  });
}

test('race retry A returning a distinct successor selects that actual job before later actions', async () => {
  let selected = '', record = null;
  const sent = [];
  const requests = createJobRequests({
    runJob: async (_operation, body) => job(body.job.id),
    runJobAction: async (operation, body) => { sent.push([operation, body]); return job('job-R', 'pending'); },
    setJobId: value => { selected = value; }, setJobRecord: value => { record = value; },
  });
  await requests.openJob('job-A');
  await requests.jobChange('job-A', 'retry');
  assert.deepEqual(sent, [['jobs.retry', { retry: { id: 'job-A' } }]]);
  assert.equal(selected, 'job-R', 'native retry preserves terminal A and returns a successor');
  assert.equal(record?.id, 'job-R');
});

test('race a retry successor received after selecting B cannot replace B or redirect selection', async () => {
  const action = deferred();
  let selected = '', record = null;
  const requests = createJobRequests({ runJob: async (_operation, body) => job(body.job.id),
    runJobAction: async () => action.work,
    setJobId: value => { selected = value; }, setJobRecord: value => { record = value; } });
  await requests.openJob('job-A');
  const pending = requests.jobChange('job-A', 'retry');
  await requests.openJob('job-B');
  action.resolve(job('job-R', 'pending')); await pending;
  assert.equal(selected, 'job-B');
  assert.equal(record?.id, 'job-B');
});

const namedPage = (id, namespace = 'ns-A', continuation = null) => ok({ status: 'catalog_page',
  page: { namespace, revision: 7, continuation,
    items: [{ kind: 'index', value: { id, name: id, tombstone: false, backing: { binding: `binding-${id}` } } }] } });
const noRegistered = () => ok({ status: 'projects', page: { revision: 3, projects: [], after: null } });

test('race old index-name reload cannot overwrite a completed newer reload', async () => {
  const older = deferred(), newer = deferred();
  let calls = 0, indexes = [];
  const requests = createIndexRequests({ runCatalog: async () => ++calls === 1 ? older.work : newer.work,
    runProjects: async () => noRegistered(), setRegistryKnown: () => {}, setPartial: () => {},
    setIndexes: value => { indexes = value; } });
  const a = requests.reload(), b = requests.reload();
  newer.resolve(namedPage('index-B')); await b;
  older.resolve(namedPage('index-A')); await a;
  assert.deepEqual(indexes.map(index => index.id), ['index-B']);
});

test('race index-name pages crossing namespace never publish their combined names', async () => {
  const updates = [];
  let calls = 0;
  const requests = createIndexRequests({
    runCatalog: async () => ++calls === 1
      ? namedPage('index-A', 'ns-A', { namespace: 'ns-A', revision: 7, after: 'index-A' })
      : namedPage('index-B', 'ns-B'),
    runProjects: async () => noRegistered(), setRegistryKnown: () => {}, setPartial: () => {},
    setIndexes: value => updates.push(value),
  });
  await requests.reload();
  assert.deepEqual(updates, [], 'an incomplete cross-namespace read cannot replace the accepted index names');
});

test('race failed catalog evidence cannot become a complete index-name observation when registry succeeds', async () => {
  let partial = null, registryKnown = null, indexes = [];
  const requests = createIndexRequests({
    runCatalog: async () => ({ kind: 'transport', status: null, message: 'catalog evidence unavailable' }),
    runProjects: async () => ok({ status: 'projects', page: { revision: 3,
      projects: [{ registration: { binding: 'binding-A' }, paused: false }], after: null } }),
    setRegistryKnown: value => { registryKnown = value; }, setPartial: value => { partial = value; },
    setIndexes: value => { indexes = value; },
  });
  await requests.reload();
  assert.equal(registryKnown, true, 'the successful registry observation remains independently known');
  assert.equal(partial, true, 'missing catalog evidence is not an empty, complete names result');
  assert.deepEqual(indexes, [{ binding: 'binding-A', name: null, id: null, paused: false, retired: false, registered: true }]);
});

test('ownership retry transition retains the complete accepted receipt and origin in the successor owner', async () => {
  const context = {}, transitions = [], sent = [];
  const successor = { id: 'job-R', project: 'binding-A', kind: 'index', state: 'pending', attempts: 1, epoch: 3, fencing_generation: 9, outcome: null };
  const response = { kind: 'ok', status: 200, envelope: { schema: 'airk-admin/1', ok: true, operation: 'jobs.retry',
    request: { nonce: 'ab'.repeat(16), state: 'accepted' }, review: 'cd'.repeat(16), result: { status: 'job', job: successor } } };
  const requests = createJobRequests({ runJob: async () => job('job-A'), runJobAction: async () => response,
    currentContext: () => context, observedAt: () => 31, transition: value => transitions.push(value),
    setJobId: value => sent.push(value), setJobRecord: () => {} });
  await requests.openJob('job-A'); await requests.jobChange('job-A', 'retry');
  assert.deepEqual(sent, ['job-A', 'job-R']);
  assert.equal(transitions.length, 1);
  const seed = transitions[0];
  assert.equal(seed.origin, 'job-A'); assert.equal(seed.job, successor);
  assert.equal(seed.result, response, 'the envelope, request receipt and native body are handed off intact');
  assert.equal(seed.context, context); assert.equal(seed.at, 31);
  const next = createJobRequests({ seed, currentContext: () => context,
    runJob: async () => { throw new Error('a transition seed needs no replacement read'); },
    runJobAction: async (operation, body) => { sent.push([operation, body]); return job('job-R', 'canceled'); },
    setJobId: () => {}, setJobRecord: () => {} });
  assert.equal(next.snapshot().record, successor);
  assert.equal(next.snapshot().transition.result, response);
  await next.jobChange('job-R', 'cancel');
  assert.deepEqual(sent.at(-1), ['jobs.cancel', { cancel: { id: 'job-R' } }]);
  const unrelated = createJobRequests({ seed, currentContext: () => ({}), runJob: async () => job('job-B'),
    runJobAction: async () => { throw new Error('no accepted job'); }, setJobId: () => {}, setJobRecord: () => {} });
  assert.equal(unrelated.snapshot().record, null);
  assert.equal(unrelated.snapshot().transition, undefined);
});

test('ownership job context changes before render discard old replies, callbacks and call metadata', async () => {
  const a = {}, b = {}, pending = deferred(), sent = [], records = [];
  let session = a;
  const requests = createJobRequests({ currentContext: () => session,
    runJob: async (_operation, body) => { sent.push(body.job.id); return body.job.id === 'job-A' ? pending.work : job('job-B'); },
    runJobAction: async () => { throw new Error('no accepted stale action'); }, setJobId: () => {}, setJobRecord: value => records.push(value) });
  const oldContext = requests.bind(a), old = requests.openJob('job-A', oldContext);
  session = b;
  pending.resolve(job('job-A')); await old;
  assert.deepEqual(records, [null], 'the observer fences completion before any new render binds B');
  assert.equal(requests.snapshot().record, null); assert.equal(requests.snapshot().detailCurrent, false);
  const current = requests.bind(b);
  await requests.openJob('job-A', oldContext);
  await requests.openJob('job-B', current);
  assert.deepEqual(sent, ['job-A', 'job-B']);
  assert.equal(requests.snapshot().record.id, 'job-B');
  requests.dispose();
  await requests.openJob('job-B', current);
  assert.deepEqual(sent, ['job-A', 'job-B']);
  assert.equal(requests.snapshot().record, null);
});

test('ownership catalog render binding invalidates before effects and old callbacks cannot rebind it', async () => {
  const a = {}, b = {}, pending = deferred(), sent = [], published = [];
  let session = a;
  const requests = createCatalogRequests({ currentContext: () => session,
    run: async (_operation, body) => { sent.push(body.catalog_page.request.index_id); return body.catalog_page.request.index_id === 'index-A' ? pending.work : page('index-B', 9); },
    setItems: value => published.push(value), setCursor: () => {}, setHeader: () => {} });
  const props = { kind: 'indexes', onRevision: () => {} };
  const oldContext = requests.bind({ kind: 'indexes', indexId: 'index-A', projectId: null, reload: 0, session: a });
  const old = requests.load(null, props, oldContext);
  session = b;
  assert.equal(requests.view(oldContext).callCurrent, false);
  pending.resolve(page('index-A', 7)); await old;
  assert.deepEqual(published, []);
  const current = requests.bind({ kind: 'indexes', indexId: 'index-B', projectId: null, reload: 0, session: b });
  await requests.load(null, props, oldContext);
  await requests.load(null, props, current);
  assert.deepEqual(sent, ['index-A', 'index-B']);
  assert.deepEqual(requests.view(current).items, [{ kind: 'index', value: { id: 'index-B' } }]);
  requests.dispose(); await requests.load(null, props, current);
  assert.deepEqual(sent, ['index-A', 'index-B']);
  assert.equal(requests.view(current).callCurrent, false);
});

for (const changed of ['namespace', 'revision']) {
  test(`ownership an admitted continuation preserves accepted rows when the reply changes ${changed}`, async () => {
    const first = { kind: 'index', value: { id: 'index-A' } }, cursor = { namespace: 'ns-A', revision: 7, after: 'index-A' };
    let calls = 0;
    const f = catalog(async () => ++calls === 1 ? ok({ status: 'catalog_page', page: { namespace: 'ns-A', revision: 7, items: [first], continuation: cursor } })
      : page('foreign', changed === 'revision' ? 8 : 7, changed === 'namespace' ? 'ns-B' : 'ns-A'));
    const context = f.context();
    await f.requests.load(null, f.props, context);
    await f.requests.load(cursor, f.props, context);
    assert.equal(calls, 2, 'this exercises response validation after an actually admitted continuation');
    assert.deepEqual(f.rows(), [first]); assert.deepEqual(f.revisions, [7]);
    assert.match(f.requests.view(context).problem, /changed/);
  });
}

test('ownership index-name completion observes session changes before render and unmount', async () => {
  const a = {}, b = {}, pending = deferred(), updates = [];
  let session = a, calls = 0;
  const requests = createIndexRequests({ currentContext: () => session,
    runCatalog: async () => { calls++; return calls === 1 ? pending.work : namedPage('index-B'); },
    runProjects: async () => noRegistered(), setRegistryKnown: () => {}, setPartial: () => {}, setIndexes: value => updates.push(value) });
  const oldContext = requests.bind(a), old = requests.reload(oldContext);
  session = b;
  pending.resolve(namedPage('index-A')); await old;
  assert.deepEqual(updates, []); assert.equal(requests.snapshot().catalogCurrent, false);
  const current = requests.bind(b);
  await requests.reload(oldContext); await requests.reload(current);
  assert.equal(calls, 2); assert.deepEqual(updates[0].map(index => index.id), ['index-B']);
  requests.dispose(); await requests.reload(current);
  assert.equal(calls, 2); assert.deepEqual(requests.snapshot().indexes, []);
});

test('ownership bounded valid name pages remain a disclosed subset and failed registry stays unknown', async () => {
  let calls = 0, partial = null, known = null, indexes = [];
  const requests = createIndexRequests({
    runCatalog: async () => {
      calls++;
      return namedPage(`index-${calls}`, 'ns-A', { namespace: 'ns-A', revision: 7, after: `index-${calls}` });
    },
    runProjects: async () => ({ kind: 'transport', status: null, message: 'registry unavailable' }),
    setRegistryKnown: value => { known = value; }, setPartial: value => { partial = value; }, setIndexes: value => { indexes = value; },
  });
  await requests.reload();
  assert.equal(calls, 16); assert.equal(indexes.length, 16);
  assert.equal(partial, true); assert.equal(known, false);
  assert.ok(indexes.every(index => index.registered === false && index.paused === null));
});

test('ownership index-set details retain actual identity, namespace and revision and accept authoritative absence', () => {
  const detail = { namespace: 'ns-A', revision: 7, item: { kind: 'project', value: { id: 'set-A', name: 'A', scope_revision: 3 } } };
  const cache = { current: null };
  assert.equal(setDetailValue({ id: 'set-A', namespace: 'ns-A' }, { phase: 'done' }, detail, cache), detail);
  assert.equal(setDetailValue({ id: 'set-B', namespace: 'ns-A' }, { phase: 'done' }, detail, cache), null);
  assert.equal(setDetailValue({ id: 'set-A', namespace: 'ns-B' }, { phase: 'pending' }, null, cache), null);
  assert.equal(setDetailValue({ id: 'set-A' }, { phase: 'done' }, { ...detail, revision: 6 }, cache), null);
  const absent = { namespace: 'ns-A', revision: 8, item: null };
  assert.equal(setDetailValue({ id: 'set-A' }, { phase: 'done' }, absent, cache), absent);
  assert.equal(cache.current, null);
});

test('boundary reload binding cannot adopt another namespace within the same session', async () => {
  const updates = [];
  let calls = 0;
  const requests = createCatalogRequests({ run: async () => page(++calls === 1 ? 'index-A' : 'index-B', 7, calls === 1 ? 'ns-A' : 'ns-B'),
    setItems: value => updates.push(value), setCursor: () => {}, setHeader: () => {} });
  const props = { kind: 'indexes', onRevision: () => {} };
  const scope = { kind: 'indexes', indexId: null, projectId: null, session: undefined };
  await requests.load(null, props, requests.bind({ ...scope, reload: 0 }));
  const reloaded = requests.bind({ ...scope, reload: 1 });
  await requests.load(null, props, reloaded);
  assert.equal(calls, 2);
  assert.equal(updates.length, 1, 'a props reload must retain the established session namespace pin');
  assert.deepEqual(requests.view(reloaded).items, []);
});

test('boundary first namespace observation preserves the actual index and set detail component keys', async () => {
  const source = capture?.files?.['indexLibrary.tsx'] ?? await readFile(resolve(sourceDir, 'indexLibrary.tsx'), 'utf8');
  const file = ts.createSourceFile('indexLibrary.tsx', source, ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
  const keyExpression = name => {
    let expression;
    const visit = node => {
      if ((ts.isJsxSelfClosingElement(node) || ts.isJsxOpeningElement(node)) && node.tagName.getText() === name) {
        const key = node.attributes.properties.find(attribute => ts.isJsxAttribute(attribute) && attribute.name.getText() === 'key');
        expression = key?.initializer && ts.isJsxExpression(key.initializer) ? key.initializer.expression?.getText() : undefined;
      }
      ts.forEachChild(node, visit);
    };
    visit(file);
    assert.ok(expression, `${name} must retain an explicit selection/session key`);
    return new Function('sessionScope', 'namespace', 'openIndex', 'openSet', `return (${expression});`);
  };
  for (const name of ['IndexDetail', 'SetDetail']) {
    const key = keyExpression(name), sessionScope = { current: { generation: 3 } };
    assert.equal(key(sessionScope, null, 'index-A', 'set-A'), key(sessionScope, 'ns-A', 'index-A', 'set-A'),
      `${name} must not discard an accepted review/receipt while the initial namespace becomes known`);
  }
});

test('boundary actual parent selection wins before child unmount while a legitimate successor retains its receipt', async () => {
  const bundled = await build({
    stdin: { contents: "export { Activity, JobDetail } from './activity';", resolveDir: sourceDir },
    bundle: true, write: false, format: 'esm', platform: 'node', target: 'node24', jsx: 'automatic', jsxImportSource: 'preact',
    plugins: [{ name: 'r19-parent-observation', setup(plugin) {
      plugin.onResolve({ filter: /^preact\/hooks$/ }, () => ({ path: 'hooks', namespace: 'r19-observation' }));
      plugin.onLoad({ filter: /.*/, namespace: 'r19-observation' }, () => ({ contents: `
        const host=()=>globalThis.__airkR19Hooks;
        export function useState(initial){const h=host(),i=h.cursor++; if(!(i in h.slots))h.slots[i]=typeof initial==='function'?initial():initial;
          return[h.slots[i],next=>{h.slots[i]=typeof next==='function'?next(h.slots[i]):next;}];}
        export function useRef(initial){const h=host(),i=h.cursor++;return h.slots[i]??=({current:initial});}
        export function useEffect(effect){host().cursor++;host().effects.push(effect);}
        export function useLayoutEffect(effect){host().cursor++;host().layouts.push(effect);}
        export function useMemo(make){host().cursor++;return make();}
        export function useCallback(callback){host().cursor++;return callback;}
        export function useContext(){host().cursor++;return true;}
      ` }));
      plugin.onResolve({ filter: /^\.\/session$/ }, () => ({ path: 'session', namespace: 'r19-session' }));
      plugin.onLoad({ filter: /.*/, namespace: 'r19-session' }, () => ({ contents: `
        export function currentSession(){return globalThis.__airkR19Session;}
        export function dropSession(){globalThis.__airkR19Session=null;}
      ` }));
      plugin.onLoad({ filter: /activity\.tsx$/ }, async args => ({ loader: 'tsx', resolveDir: dirname(args.path),
        contents: `${capture?.files?.['activity.tsx'] ?? await readFile(args.path, 'utf8')}\nexport { JobDetail };` }));
      plugin.onLoad({ filter: /activityRequests\.ts$/ }, async args => ({ loader: 'ts', resolveDir: dirname(args.path),
        contents: capture?.modules.activityRequests ?? await readFile(args.path, 'utf8') }));
    } }], logLevel: 'silent',
  });
  const actual = await import(`data:text/javascript;base64,${Buffer.from(bundled.outputFiles[0].contents).toString('base64')}`);
  const hooks = () => ({ cursor: 0, slots: [], effects: [], layouts: [] });
  const render = (component, props, host) => {
    host.cursor = 0; host.effects = []; host.layouts = []; globalThis.__airkR19Hooks = host;
    return component(props);
  };
  const nodes = value => value == null || typeof value !== 'object' ? [] : Array.isArray(value)
    ? value.flatMap(nodes) : [value, ...nodes(value.props?.children)];
  const formForJob = tree => nodes(tree).find(node => node.type === 'form'
    && nodes(node.props.children).some(child => child.type?.name === 'Field' && child.props.label === 'Job id (64 hex characters)'));
  const originalFetch = globalThis.fetch;
  const originalWindow = globalThis.window;
  // This actual browser API callback now requires prepare-first journal
  // admission. Supply its browser dependency rather than bypassing persistence.
  const preparationStorage = new Map();
  globalThis.window = { sessionStorage: { getItem: key => preparationStorage.get(key) ?? null,
    setItem: (key, value) => preparationStorage.set(key, value), removeItem: key => preparationStorage.delete(key) } };
  const a = '01'.repeat(32), b = '02'.repeat(32), successor = '03'.repeat(32);
  try {
    for (const selectB of [false, true]) {
      const parent = hooks(), child = hooks(), entered = deferred(), response = deferred(), calls = [];
      globalThis.__airkR19Session = { csrf: 'fixture-only', serviceInstance: '04'.repeat(16) };
      const receipt = { schema: 'airk-admin/1', ok: true, operation: 'jobs.retry', review: '05'.repeat(16),
        request: { nonce: '06'.repeat(16), state: 'accepted' }, result: { status: 'job', job: { id: successor, project: '07'.repeat(32),
          kind: 'index', state: 'pending', attempts: 1, epoch: 1, fencing_generation: 1, outcome: null } } };
      globalThis.fetch = async (url, init) => {
        const operation = String(url).split('/').at(-1), body = JSON.parse(init.body);
        calls.push([operation, body]);
        if (operation === 'jobs.get') {
          assert.equal(body.job.id, a);
          return { status: 200, text: async () => JSON.stringify({ schema: 'airk-admin/1', ok: true, operation,
            result: { status: 'job', job: { id: a, project: '07'.repeat(32), kind: 'index', state: 'failed' } } }) };
        }
        assert.equal(operation, 'jobs.retry'); assert.equal(body.retry.id, a);
        entered.resolve(); return response.work;
      };
      let tree = render(actual.Activity, {}, parent);
      const typeJob = id => {
        const field = nodes(tree).find(node => node.type?.name === 'Field' && node.props.label === 'Job id (64 hex characters)');
        assert.ok(field); field.props.onInput(id);
        tree = render(actual.Activity, {}, parent);
      };
      typeJob(a); formForJob(tree).props.onSubmit({ preventDefault() {} });
      tree = render(actual.Activity, {}, parent);
      const selectedA = nodes(tree).find(node => node.type === actual.JobDetail);
      assert.equal(selectedA.props.id, a);
      let card = render(actual.JobDetail, selectedA.props, child);
      for (const effect of child.effects) effect();
      const settle = async () => {
        const deadline = Date.now() + 5000;
        while (child.slots.some(slot => slot?.phase === 'pending')) {
          assert.ok(Date.now() < deadline, 'actual child callback must finish under the observation deadline');
          await new Promise(resolve => setImmediate(resolve));
        }
      };
      await settle(); card = render(actual.JobDetail, selectedA.props, child);
      assert.equal(card.props.record.id, a);
      card.props.onChange('retry');
      await new Promise((resolve, reject) => {
        const deadline = setTimeout(() => reject(Error('actual retry must enter its native call under the observation deadline')), 5000);
        entered.work.then(() => { clearTimeout(deadline); resolve(); });
      });
      if (selectB) {
        typeJob(b);
        formForJob(tree).props.onSubmit({ preventDefault() {} });
        // Deliberately no parent render, child render or cleanup between B's
        // real selection callback and the old child's native reply.
      }
      response.resolve({ status: 200, text: async () => JSON.stringify(receipt) });
      await settle(); tree = render(actual.Activity, {}, parent);
      const chosen = nodes(tree).find(node => node.type === actual.JobDetail);
      assert.equal(chosen.props.id, selectB ? b : successor);
      const next = hooks(), shown = render(actual.JobDetail, chosen.props, next);
      if (selectB) {
        assert.equal(chosen.props.seed, undefined);
        assert.equal(shown.props.record, null); assert.equal(shown.props.action.phase, 'idle');
      } else {
        assert.notEqual(chosen.key, selectedA.key);
        assert.equal(shown.props.id, successor); assert.equal(shown.props.record.id, successor);
        assert.equal(shown.props.actionOrigin, a); assert.equal(shown.props.action.phase, 'done');
        assert.deepEqual(shown.props.action.result.envelope, receipt, 'the new keyed owner retains every receipt/native field');
        for (const effect of next.effects) effect();
      }
      assert.deepEqual(calls.map(call => call[0]), ['jobs.get', 'jobs.retry']);
    }
  } finally {
    globalThis.fetch = originalFetch; globalThis.window = originalWindow; delete globalThis.__airkR19Hooks; delete globalThis.__airkR19Session;
  }
});
