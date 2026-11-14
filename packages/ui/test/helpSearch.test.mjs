// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
const { build } = createRequire(new URL('../../vscode/package.json', import.meta.url))('esbuild');
const bundle = await build({ entryPoints: [fileURLToPath(new URL('../src/helpSearch.ts', import.meta.url))], bundle: true,
  write: false, platform: 'node', format: 'esm' });
const { helpScore } = await import(`data:text/javascript;base64,${Buffer.from(bundle.outputFiles[0].text).toString('base64')}`);
const guide = { id: 'engine.installation', title: 'Install AIRK', aliases: ['silent setup'],
  body: 'Install the service without VS Code. Chemins français et 日本語 are supported.' };

test('shared Help searches complete article text, titles, aliases and Unicode terms', () => {
  assert.ok(helpScore(guide, 'without VS Code') > 0);
  assert.ok(helpScore(guide, 'silent setup') > 0);
  assert.ok(helpScore(guide, 'français') > 0);
  assert.ok(helpScore(guide, '日本語') > 0);
  assert.equal(helpScore(guide, 'unrelatedword'), 0);
  assert.equal(helpScore(guide, '  '), 0);
  assert.ok(helpScore(guide, 'Install AIRK') > helpScore(guide, 'without VS Code'));
});
