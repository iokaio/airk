// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
const { build } = createRequire(new URL('../../vscode/package.json', import.meta.url))('esbuild');
const bundle = await build({ entryPoints: [fileURLToPath(new URL('../src/helpRoute.ts', import.meta.url))], bundle: true,
  write: false, platform: 'node', format: 'esm' });
const actual = await import(`data:text/javascript;base64,${Buffer.from(bundle.outputFiles[0].text).toString('base64')}`);

test('Help route round-trips inert query, guide and tool controls only', () => {
  const state = { query: 'missing results 日本語', guide: 'engine.request-recovery', tool: 'help' };
  assert.deepEqual(actual.parseHelpLocation(actual.formatHelpLocation(state)), state);
  assert.deepEqual([...actual.formatHelpLocation({ ...state, csrf: 'must-not-pass', caller: 'must-not-pass', key: 'must-not-pass' }).keys()], ['q', 'guide', 'tool']);
});

test('Help routes refuse active selectors and bound text without interpreting it as a command', () => {
  assert.deepEqual(actual.parseHelpLocation(new URLSearchParams('guide=command%3Aexecute&tool=https%3A%2F%2Fexample.com&caller=ignored&key=ignored')),
    { query: '', guide: null, tool: '' });
  assert.equal(actual.parseHelpLocation(new URLSearchParams({ q: 'x'.repeat(300) })).query.length, 256);
  assert.deepEqual(actual.parseHelpLocation(undefined), { query: '', guide: null, tool: '' });
});
