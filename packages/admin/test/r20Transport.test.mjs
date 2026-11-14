// SPDX-License-Identifier: Apache-2.0
// Actual adapter/session/bootstrap source with injected fetch and host observations.
import assert from 'node:assert/strict';
import test from 'node:test';
import { build } from 'esbuild';
import { readFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const src = resolve(dirname(fileURLToPath(import.meta.url)), '../src');
let instance = 0;
async function actual(bootstrap = false) {
  const result = await build({
    stdin: { contents: bootstrap ? `export * from './main';` : `export * from './api'; export * from './session'; export { useCall } from './render';`,
      resolveDir: src, sourcefile: `r20-${instance++}.ts` },
    bundle: true, write: false, format: 'esm', platform: 'node', target: 'node24', jsx: 'automatic', jsxImportSource: 'preact',
    plugins: [{ name: 'r20-host', setup(plugin) {
      plugin.onResolve({ filter: /^preact\/hooks$/ }, () => ({ path: 'hooks', namespace: 'r20' }));
      plugin.onLoad({ filter: /^hooks$/, namespace: 'r20' }, () => ({ contents: `
        export function useState(initial){const h=globalThis.__r20Hooks,i=h.cursor++;h.slots[i]??=initial;return[h.slots[i],v=>{h.slots[i]=v;}];}
        export function useRef(initial){const h=globalThis.__r20Hooks,i=h.cursor++;return h.slots[i]??={current:initial};}
      ` }));
      if (bootstrap) {
        plugin.onResolve({ filter: /^preact\/jsx-runtime$/ }, () => ({ path: 'jsx', namespace: 'r20' }));
        plugin.onLoad({ filter: /^jsx$/, namespace: 'r20' }, () => ({ contents: 'export function jsx(type,props){return {type,props};} export {jsx as jsxs};' }));
        plugin.onResolve({ filter: /^\.\/app$/ }, () => ({ path: 'app', namespace: 'r20' }));
        plugin.onLoad({ filter: /^app$/, namespace: 'r20' }, () => ({ contents: 'export function App(){}' }));
        plugin.onResolve({ filter: /^preact$/ }, () => ({ path: 'preact', namespace: 'r20' }));
        plugin.onLoad({ filter: /^preact$/, namespace: 'r20' }, () => ({ contents: 'export function render(tree,root){globalThis.__r20Render(tree,root);}' }));
        plugin.onLoad({ filter: /main\.tsx$/ }, async args => ({
          contents: (await readFile(args.path, 'utf8')).replace('void start()', 'export const boot = start()'), loader: 'tsx', resolveDir: src,
        }));
      }
    } }],
  });
  // Distinct module state per test, including session identity.
  return import(`data:text/javascript;base64,${Buffer.from(`${result.outputFiles[0].text}\n// ${instance}`).toString('base64')}`);
}
function environment() {
  const saved = { fetch: globalThis.fetch, window: globalThis.window, document: globalThis.document };
  const storage = new Map();
  const root = { className: 'loading', textContent: 'Loading AIRK Admin…', replaceChildren() {} };
  globalThis.window = { location: { hash: '#c=fixture-only', pathname: '/admin/', search: '' },
    history: { replaceState(_state, _unused, path) { window.location.hash = path.includes('#') ? path.slice(path.indexOf('#')) : ''; } },
    sessionStorage: { get length() { return storage.size; }, key(i) { return [...storage.keys()][i] ?? null; },
      getItem(key) { return storage.get(key) ?? null; }, setItem(key, value) { storage.set(key, value); }, removeItem(key) { storage.delete(key); } } };
  globalThis.document = { getElementById: () => root };
  globalThis.__r20Hooks = { cursor: 0, slots: [] };
  const rendered = [];
  globalThis.__r20Render = tree => rendered.push(tree);
  return { root, rendered, storage, close() {
    Object.assign(globalThis, saved); delete globalThis.__r20Hooks; delete globalThis.__r20Render;
  } };
}
const success = { schema: 'airk-admin/1', ok: true, operation: 'session.exchange', result: {
  csrf: 'fixture-csrf', service_instance: 'fixture-instance', idle_expires_in_ms: 60000, absolute_expires_in_ms: 60000,
} };
async function signedIn(module) {
  globalThis.fetch = async () => ({ status: 200, text: async () => JSON.stringify(success) });
  assert.equal((await module.exchange('fixture-only')).kind, 'ok');
}
function brokenBody(status) {
  return { status, text: async () => { throw new Error('private-body-diagnostic'); } };
}

for (const operation of ['service.health', 'catalog.policy_resume']) {
  test(`R20 ${operation} body rejection settles actual useCall without replay`, async () => {
    const host = environment();
    try {
      const module = await actual();
      await signedIn(module);
      let calls = 0;
      globalThis.fetch = async () => { calls++; return brokenBody(200); };
      const [, run] = module.useCall();
      const result = await run(operation, { fixture: 'original' });
      assert.equal(result.kind, 'transport');
      assert.equal(result.status, 200);
      assert.equal(globalThis.__r20Hooks.slots[0].phase, 'done');
      assert.equal(calls, 1);
      assert.equal(module.isUnknownOutcome(result), true, 'transport cannot prove that a sent mutation had no effects');
      assert.equal(module.recoveryOf(result), null, 'lost body cannot invent a recovery handle');
      assert.doesNotMatch(result.message, /private-body-diagnostic/);
    } finally { host.close(); }
  });
}

test('R20 observed 401 ends the original session before consuming its broken body', async () => {
  const host = environment();
  try {
    const module = await actual();
    await signedIn(module);
    globalThis.fetch = async () => brokenBody(401);
    const result = await module.call('service.health', {});
    assert.equal(result.kind, 'transport');
    assert.equal(result.status, 401);
    assert.equal(module.currentSession(), null);
    assert.equal(host.storage.size, 0);
  } finally { host.close(); }
});

test('R20 session exchange body rejection returns an observed-status failure', async () => {
  const host = environment();
  try {
    const module = await actual();
    globalThis.fetch = async () => brokenBody(502);
    const result = await module.exchange('fixture-only');
    assert.equal(result.kind, 'failed');
    assert.equal(result.response.kind, 'transport');
    assert.equal(result.response.status, 502);
    assert.equal(module.currentSession(), null);
  } finally { host.close(); }
});

test('R20 bootstrap renders its session failure gate after a broken exchange body', async () => {
  const host = environment();
  try {
    // Loading the real main starts bootstrap once. Observe its completion via
    // rendering rather than calling start a second time or replaying exchange.
    let calls = 0;
    globalThis.fetch = async () => { calls++; return brokenBody(503); };
    const module = await actual(true);
    await module.boot;
    assert.equal(calls, 1);
    assert.equal(host.rendered.length, 1);
    assert.equal(host.rendered[0].props.initialGate.kind, 'exchange_failed');
    assert.equal(host.rendered[0].props.initialGate.response.status, 503);
    assert.equal(window.location.hash.includes('fixture-only'), false);
  } finally { host.close(); }
});

test('R20 final bootstrap failure replaces loading with secret-free recovery guidance', async () => {
  const host = environment();
  try {
    globalThis.fetch = async () => ({ status: 200, text: async () => JSON.stringify(success) });
    globalThis.__r20Render = () => { throw new Error('private-render-diagnostic'); };
    const module = await actual(true);
    await module.boot;
    assert.match(host.root.textContent, /could not start|unable to start/i);
    assert.match(host.root.textContent, /airkctl admin open/);
    assert.doesNotMatch(host.root.textContent, /fixture-only|fixture-csrf|private-render-diagnostic/);
  } finally { host.close(); }
});

test('R20 observed 401 clears authorization before a pending body and cannot erase a newer session', async () => {
  const host = environment();
  let release;
  let request;
  try {
    const module = await actual();
    await signedIn(module);
    let bodyStarted;
    const started = new Promise(resolve => { bodyStarted = resolve; });
    const body = new Promise(resolve => { release = resolve; });
    globalThis.fetch = async () => ({ status: 401, text: async () => { bodyStarted(); return body; } });
    request = module.call('service.health', {});
    await started;
    assert.equal(module.currentSession(), null, 'headers revoke authorization without waiting on the body');
    assert.equal(host.storage.size, 0);
    await signedIn(module);
    const replacement = module.currentSession();
    release(JSON.stringify({ schema: 'airk-admin/1', ok: false,
      error: { code: 'expired', class: 'unauthenticated', retryable: false } }));
    const result = await request;
    assert.equal(result.status, 401);
    assert.equal(module.currentSession(), replacement, 'late old401 parsing preserves replacement authorization');
  } finally { release?.(''); await request?.catch(() => {}); host.close(); }
});

test('R20 lost fetch settles as uncertain and never supplies a recovery handle', async () => {
  const host = environment();
  try {
    const module = await actual();
    await signedIn(module);
    let calls = 0;
    globalThis.fetch = async () => { calls++; throw new Error('private-fetch-diagnostic'); };
    const result = await module.call('catalog.policy_resume', {});
    assert.equal(result.kind, 'transport');
    assert.equal(result.status, null);
    assert.equal(module.isUnknownOutcome(result), true);
    assert.equal(module.recoveryOf(result), null);
    assert.equal(calls, 1);
    assert.doesNotMatch(result.message, /private-fetch-diagnostic/);
  } finally { host.close(); }
});

test('R20 exchange401 broken body clears existing authorization and settles', async () => {
  const host = environment();
  try {
    const module = await actual();
    await signedIn(module);
    globalThis.fetch = async () => brokenBody(401);
    const result = await module.exchange('fixture-only');
    assert.equal(result.kind, 'failed');
    assert.equal(result.response.status, 401);
    assert.equal(module.currentSession(), null);
    assert.equal(host.storage.size, 0);
  } finally { host.close(); }
});

test('R20 launch-code cleanup failure takes the final bootstrap error path before any fetch', async () => {
  const host = environment();
  try {
    let calls = 0;
    globalThis.fetch = async () => { calls++; assert.fail('cleanup failure must precede network admission'); };
    window.history.replaceState = () => { throw new Error('private-history-diagnostic'); };
    const module = await actual(true);
    await module.boot;
    assert.equal(calls, 0);
    assert.match(host.root.textContent, /could not start/i);
    assert.doesNotMatch(host.root.textContent, /fixture-only|private-history-diagnostic/);
  } finally { host.close(); }
});
