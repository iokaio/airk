// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import { readFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
const { build } = createRequire(new URL('../../vscode/package.json', import.meta.url))('esbuild');
const bundle = await build({ entryPoints: [fileURLToPath(new URL('../src/usagePresentation.ts', import.meta.url))], bundle: true,
  write: false, platform: 'node', format: 'esm' });
const { usagePresentation } = await import(`data:text/javascript;base64,${Buffer.from(bundle.outputFiles[0].text).toString('base64')}`);
const fixture = JSON.parse(await readFile(new URL('../../../fixtures/split/ui_pilot/native_reads.json', import.meta.url), 'utf8'));
const nodes = tree => [tree, ...tree.children.filter(child => typeof child !== 'string').flatMap(nodes)];
const text = tree => tree.children.map(child => typeof child === 'string' ? child : text(child)).join(' ');
for (const name of ['usage_empty', 'usage_populated', 'usage_unknown_fields']) {
  test(`shared Usage ${name} keeps full Rust-emitted document in every view`, () => {
    const document = fixture.cases.find(entry => entry.name === name).native_response.document;
    for (const tab of ['summary', 'breakdown', 'recording']) {
      const tree = usagePresentation(document, { index: 'fixture-index', days: 7 }, tab);
      const complete = nodes(tree).find(node => node.tag === 'details' && node.children[0].children[0] === 'Complete native Usage document (JSON)');
      assert.deepEqual(JSON.parse(complete.children[1].children[0]), document);
      assert.match(text(tree), /requested 7 days/);
      assert.match(text(tree), /UTC report interval/);
      assert.match(text(tree), /not proof of every client call or source freshness/);
    }
  });
}
test('shared Usage does not turn unavailable measurements or partial recording into measured savings', () => {
  const document = fixture.cases.find(entry => entry.name === 'usage_populated').native_response.document;
  const tree = usagePresentation(document, { index: 'fixture-index', days: 7 }, 'summary');
  const content = text(tree);
  assert.match(content, /Native status: partial/); assert.match(content, /Complete history: false/);
  assert.match(content, /Native estimated bill savings not available/);
  assert.match(content, /no savings percentage is inferred/);
  assert.doesNotMatch(content, /\b\d+(?:\.\d+)?%/);
});
