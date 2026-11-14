// SPDX-License-Identifier: Apache-2.0
// Actual Service callbacks and API body-rejection path; no native process.
import assert from 'node:assert/strict';
import test from 'node:test';
const preparationStorage = new Map();
globalThis.window = { sessionStorage: { getItem: key => preparationStorage.get(key) ?? null,
  setItem: (key, value) => preparationStorage.set(key, value), removeItem: key => preparationStorage.delete(key) } };
import { build } from 'esbuild';
import { fileURLToPath } from 'node:url';
const src = fileURLToPath(new URL('../src/', import.meta.url));
const bundle = await build({ stdin: { contents: `export {Service} from './pages';`, resolveDir: src }, bundle: true,
  write: false, platform: 'node', format: 'esm', jsx: 'automatic', jsxImportSource: 'preact',
  plugins: [{ name: 'lifecycle-host', setup(plugin) {
    plugin.onResolve({ filter: /^preact\/hooks$/ }, () => ({ path: 'hooks', namespace: 'lifecycle' }));
    plugin.onLoad({ filter: /^hooks$/, namespace: 'lifecycle' }, () => ({ contents: `
      export function useState(initial){const h=globalThis.__lifecycleHooks,i=h.cursor++;if(!(i in h.slots))h.slots[i]=typeof initial==='function'?initial():initial;
        return[h.slots[i],next=>{h.slots[i]=typeof next==='function'?next(h.slots[i]):next;}];}
      export function useRef(initial){const h=globalThis.__lifecycleHooks,i=h.cursor++;return h.slots[i]??={current:initial};}
      export function useLayoutEffect(effect,deps){const h=globalThis.__lifecycleHooks,i=h.cursor++,old=h.slots[i];
        if(!old||deps.some((d,n)=>d!==old.deps[n])){h.effects.push(()=>{old?.cleanup?.();h.slots[i]={deps,cleanup:effect()};});}}
      export {useLayoutEffect as useEffect};export function useMemo(make){globalThis.__lifecycleHooks.cursor++;return make();}
      export function useContext(){globalThis.__lifecycleHooks.cursor++;return true;}
    ` }));
    plugin.onResolve({ filter: /(?:^|\/)session$/ }, () => ({ path: 'session', namespace: 'lifecycle' }));
    plugin.onLoad({ filter: /^session$/, namespace: 'lifecycle' }, () => ({ contents: `export function currentSession(){return globalThis.__lifecycleSession;}export function dropSession(){globalThis.__lifecycleSession=null;}` }));
  } }],
});
const actual = await import(`data:text/javascript;base64,${Buffer.from(bundle.outputFiles[0].text).toString('base64')}`);
const nodes = value => value == null || typeof value !== 'object' ? [] : Array.isArray(value) ? value.flatMap(nodes) : [value, ...nodes(value.props?.children)];
function host() {
  const hooks = { cursor: 0, slots: [], effects: [] };
  return { hooks, render() { globalThis.__lifecycleHooks = hooks; hooks.cursor = 0; const tree = actual.Service({ onLifecycle() {} });
    for (const effect of hooks.effects.splice(0)) effect(); return tree; }, close() { for (const slot of hooks.slots) slot?.cleanup?.(); } };
}
const confirm = (tree, action = 'Stop') => nodes(tree).find(node => node.props?.phrase === `${action.toLowerCase()} service`).props.onConfirm;
async function settle(hooks) {
  const deadline = Date.now() + 1000;
  while (hooks.slots.some(slot => slot?.phase === 'pending') && Date.now() < deadline) await new Promise(resolve => setImmediate(resolve));
  assert.equal(hooks.slots.some(slot => slot?.phase === 'pending'), false);
}
test('R20 lifecycle body-loss unknown blocks fresh Stop and Restart keys', async () => {
  const fetch = globalThis.fetch; let calls = 0;
  globalThis.__lifecycleSession = { csrf: 'private-fixture', serviceInstance: 'fixture' };
  globalThis.fetch = async () => { calls++; return { status: 200, text: async () => { throw Error('body rejected'); } }; };
  const mounted = host();
  try { confirm(mounted.render())(); await settle(mounted.hooks); confirm(mounted.render())(); confirm(mounted.render(), 'Restart')(); await settle(mounted.hooks); assert.equal(calls, 1); }
  finally { mounted.close(); globalThis.fetch = fetch; }
});
test('R20 lifecycle same-session remount retains unknown original action and full transport status', async () => {
  const fetch = globalThis.fetch; let calls = 0;
  globalThis.__lifecycleSession = { csrf: 'private-fixture', serviceInstance: 'fixture' };
  globalThis.fetch = async () => { calls++; return { status: 202, text: async () => { throw Error('body rejected'); } }; };
  const first = host(), next = host();
  try {
    confirm(first.render())(); await settle(first.hooks); first.close();
    confirm(next.render(), 'Restart')(); await settle(next.hooks); assert.equal(calls, 1);
    const retained = nodes(next.render()).find(node => node.props?.state?.result?.kind === 'transport');
    assert.ok(retained); assert.equal(retained.props.state.result.status, 202);
  } finally { next.close(); globalThis.fetch = fetch; }
});
test('R20 lifecycle admits one in-flight original action before rendering', async () => {
  const fetch = globalThis.fetch; let calls = 0, release;
  const pending = new Promise((_resolve, reject) => { release = () => reject(Error('body rejected')); });
  globalThis.__lifecycleSession = { csrf: 'private-fixture', serviceInstance: 'fixture' };
  globalThis.fetch = async () => { calls++; return { status: 200, text: () => pending }; };
  const mounted = host();
  try { const tree = mounted.render(); confirm(tree)(); confirm(tree, 'Restart')(); const observed = calls; release(); await settle(mounted.hooks); assert.equal(observed, 1); }
  finally { mounted.close(); globalThis.fetch = fetch; }
});
