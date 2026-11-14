// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import { readFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
const { build } = createRequire(new URL('../../vscode/package.json', import.meta.url))('esbuild');
const bundle = await build({ stdin: { contents: `export * from './helpSchema'; export * from './helpToolPresentation'; export * from './helpMarkdown';`,
  resolveDir: fileURLToPath(new URL('../src/', import.meta.url)) }, bundle: true, write: false, platform: 'node', format: 'esm' });
const actual = await import(`data:text/javascript;base64,${Buffer.from(bundle.outputFiles[0].text).toString('base64')}`);
const manifest = JSON.parse(await readFile(new URL('../generated/help-manifest.json', import.meta.url), 'utf8'));
const nodes = node => [node, ...node.children.filter(child => typeof child !== 'string').flatMap(nodes)];

test('actual Rust HelpArgs optional nullable topic is presented separately from required fields', () => {
  const topic = actual.paramsOf(manifest.helpRequestSchema).find(parameter => parameter.name === 'topic');
  assert.equal(topic.required, false);
  assert.equal(topic.type, 'string or null');
});

test('shared public Help tool presentation retains the complete actual unified tool definition', () => {
  const tool = manifest.tools.find(tool => tool.name === 'help');
  assert.ok(tool, 'actual unified public Help tool exists');
  const tree = actual.helpToolPresentation(tool);
  const raw = nodes(tree).find(node => node.tag === 'pre');
  assert.deepEqual(JSON.parse(raw.children[0]), tool);
  assert.ok(nodes(tree).some(node => node.tag === 'table'));
});

test('shared Markdown preserves fenced commands, line breaks and HTML-looking text as text', () => {
  assert.deepEqual(actual.blocks('# Install\n\n```sh\nairkctl status\n$literal\n```\n\n<img src=x onerror=run()>'), [
    { kind: 'heading', level: 1, text: 'Install' },
    { kind: 'code', text: 'airkctl status\n$literal' },
    { kind: 'para', text: '<img src=x onerror=run()>' },
  ]);
});

test('shared authored Markdown keeps nested steps, tables and original section numbering', () => {
  const parsed = actual.blocks('3. Read the review.\n   - Keep source locations.\n   - Check nullable fields.\n\n4. Run `airkctl help`.\n\n| Field | Meaning |\n| --- | --- |\n| `mode` | Optional enum |');
  assert.deepEqual(parsed, [
    { kind: 'list', ordered: true, start: 3, items: [
      { text: 'Read the review.', children: [{ kind: 'list', ordered: false, items: [
        { text: 'Keep source locations.', children: [] }, { text: 'Check nullable fields.', children: [] },
      ] }] },
      { text: 'Run `airkctl help`.', children: [] },
    ] },
    { kind: 'table', headers: ['Field', 'Meaning'], rows: [['`mode`', 'Optional enum']] },
  ]);
});
