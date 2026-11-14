// SPDX-License-Identifier: Apache-2.0
// Actual schema renderer/topic callbacks. These are frontend semantics, not transport qualification.
import assert from 'node:assert/strict';
import test from 'node:test';
import { readFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { build } from 'esbuild';
const src = resolve(dirname(fileURLToPath(import.meta.url)), '../src');
const bundle = await build({ stdin: { contents: `export {typeText,paramsOf} from '../../ui/src/helpSchema'; export {Help,TopicPane} from './help';`, resolveDir: src, sourcefile: 'help-semantics.ts' },
  bundle: true, write: false, platform: 'node', format: 'esm', jsx: 'automatic', jsxImportSource: 'preact',
  plugins: [{ name: 'help-observation', setup(plugin) {
    plugin.onLoad({ filter: /help\.tsx$/ }, async args => ({ contents: `${await readFile(args.path, 'utf8')}\nexport {TopicPane};`, loader: 'tsx', resolveDir: src }));
    plugin.onResolve({ filter: /^preact\/hooks$/ }, () => ({ path: 'hooks', namespace: 'help-test' }));
    plugin.onLoad({ filter: /^hooks$/, namespace: 'help-test' }, () => ({ contents: `
      const host=()=>globalThis.__helpHooks;
      export function useState(initial){const h=host(),i=h.cursor++;if(!(i in h.slots))h.slots[i]=typeof initial==='function'?initial():initial;
        return[h.slots[i],next=>{h.slots[i]=typeof next==='function'?next(h.slots[i]):next;}];}
      export function useRef(initial){const h=host(),i=h.cursor++;return h.slots[i]??={current:initial};}
      export function useEffect(){host().cursor++;} export function useLayoutEffect(){host().cursor++;}
      export function useContext(){host().cursor++;return true;}
      export function useMemo(make){host().cursor++;return make();}
    ` }));
    plugin.onResolve({ filter: /(?:^|\/)session$/ }, () => ({ path: 'session', namespace: 'help-test' }));
    plugin.onLoad({ filter: /^session$/, namespace: 'help-test' }, () => ({ contents: `export function currentSession(){return globalThis.__helpSession;}export function dropSession(){globalThis.__helpSession=null;}` }));
  } }],
});
const actual = await import(`data:text/javascript;base64,${Buffer.from(bundle.outputFiles[0].text).toString('base64')}`);
function nodes(value) {
  if (value == null || typeof value !== 'object') return [];
  if (Array.isArray(value)) return value.flatMap(nodes);
  return [value, ...nodes(value.props?.children)];
}
const view = () => ({ reply: { topic: 'usage', title: 'Usage', content: 'Previously loaded text.', start_line: 1, end_line: 10,
  total_lines: 30, next_line: 11, truncated: true }, content: 'Previously loaded text.' });

test('Help schema null-only is null and does not accept arbitrary any', () => {
  assert.equal(actual.typeText({ type: 'null' }, null), 'null');
});
test('Help schema nullable enum preserves the actual enum and null alternative', () => {
  assert.equal(actual.typeText({ anyOf: [{ enum: ['lexical', 'hybrid'] }, { type: 'null' }] }, null), 'one of "lexical", "hybrid" or null');
});
test('Help schema optionality and nullable type are independent', () => {
  const parameters = actual.paramsOf({ type: 'object', properties: {
    optional: { type: ['string', 'null'] }, required: { type: 'string' },
  }, required: ['required'] });
  assert.deepEqual(parameters.map(({ name, type, required }) => ({ name, type, required })), [
    { name: 'required', type: 'string', required: true }, { name: 'optional', type: 'string or null', required: false },
  ]);
});
test('Help continuation labels its bounded next section accurately', () => {
  const tree = actual.TopicPane({ state: { phase: 'idle' }, view: view(), onMore() {} });
  const buttons = nodes(tree).filter(node => node.type === 'button');
  assert.ok(buttons.some(node => node.props.children === 'Load next section'));
});
test('Help failed continuation preserves previously loaded article and the exact failure', async () => {
  const fetch = globalThis.fetch;
  const hooks = { cursor: 0, slots: [] };
  globalThis.__helpHooks = hooks;
  globalThis.__helpSession = { csrf: 'fixture-only' };
  try {
    // Actual useCall state/ref pairs for catalog, tool help, topic call, then
    // topics/view/topicView/filter. This observes the pre-existing loaded state.
    hooks.slots[6] = [{ id: 'usage', title: 'Usage' }];
    hooks.slots[7] = { kind: 'topic', id: 'usage' };
    const prior = { kind: 'ok', status: 200, envelope: { schema: 'airk-admin/1', ok: true, operation: 'search.call',
      result: { synthetic: 'previous complete section', future: { retained: [1, true, null] } } } };
    hooks.slots[8] = { ...view(), observations: [prior] };
    const calls = [];
    globalThis.fetch = async (url, init) => {
      calls.push({ url, body: JSON.parse(init.body) });
      return { status: 503, text: async () => JSON.stringify({ schema: 'airk-admin/1', ok: false,
        error: { code: 'service_draining', class: 'unavailable', retryable: false } }) };
    };
    const tree = actual.Help();
    const pane = nodes(tree).find(node => node.type === actual.TopicPane);
    assert.ok(pane);
    pane.props.onMore(pane.props.view.reply, pane.props.view.content);
    const deadline = Date.now() + 1000;
    do { await new Promise(resolve => setImmediate(resolve)); } while (hooks.slots.some(slot => slot?.phase === 'pending') && Date.now() < deadline);
    assert.equal(calls.length, 1);
    assert.equal(calls[0].body.tools_call.arguments.start_line, 11);
    assert.equal(hooks.slots[8].content, 'Previously loaded text.');
    assert.equal(hooks.slots[8].problem.code, 'service_draining');
    assert.equal(hooks.slots[8].observations.length, 2);
    assert.deepEqual(hooks.slots[8].observations[0], prior);
    assert.equal(hooks.slots[8].observations[1].envelope.error.code, 'service_draining');
  } finally { globalThis.fetch = fetch; delete globalThis.__helpHooks; delete globalThis.__helpSession; }
});
