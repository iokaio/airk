// SPDX-License-Identifier: Apache-2.0
// Generation boundary cases; these synthetic inputs do not qualify native Help.
import assert from 'node:assert/strict';
import test from 'node:test';
import { createRequire } from 'node:module';
import { mkdtemp, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve, sep } from 'node:path';
const { engineGuides, nativeHelpContent, sharedHelpManifest } = createRequire(import.meta.url)('../../vscode/scripts/build-explorer.cjs');

test('shared native guide body preserves multiline commands and tables exactly after its heading', () => {
  const text = '| Action | Result |\n| --- | --- |\n| status | observed |\n\n```sh\nairkctl command \\\n  --option literal\n```';
  const [guide] = engineGuides({ topics: [{ id: 'fixture', title: 'Native fixture', text }] });
  assert.equal(guide.body, `# Native fixture\n\n${text}`);
  assert.equal(guide.source, 'crates/airk-mcp/src/help/content.json');
});

test('shared Help manifest keeps whole Rust-emitted content and complete schemas without filtering', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'airk-help-manifest-'));
  try {
    const native = { schema: 'airk.help.native-content/1', version: '1.2.3',
      topics: Array.from({ length: 20 }, (_, i) => ({ id: `topic-${i}`, title: `Topic ${i}`, text: `Full native text ${i}\nSecond line`, futureTopic: { retained: true } })),
      cliCommands: { future: { command: 'full help', description: 'Schemas may describe caller/request_key fields.' } },
      helpRequestSchema: { type: 'object', futureSchema: [1, true, null] },
      tools: [{ name: 'example', inputSchema: { type: 'object', futureInput: { kept: true } }, annotations: { future: true } }],
      futureNative: { all: ['unknown', true, null] } };
    const file = join(directory, 'native.json');
    await writeFile(file, JSON.stringify(native));
    const parsed = nativeHelpContent(file, '1.2.3');
    assert.deepEqual(parsed, native);
    const topics = [{ id: 'authored', title: 'Authored UI topic', body: 'Whole authored content', futureAuthored: true }];
    const manifest = sharedHelpManifest({ catalogVersion: 1, extensionVersion: '1.2.3', topics }, parsed);
    assert.deepEqual(manifest.native, native);
    assert.equal(manifest.tools, parsed.tools);
    assert.equal(manifest.helpRequestSchema, parsed.helpRequestSchema);
    assert.equal(manifest.cliCommands, parsed.cliCommands);
    assert.equal(manifest.topics, topics);
    assert.equal(manifest.channel, null, 'channel belongs to artifact identity');
    assert.equal(manifest.productVersion, manifest.extensionVersion);
    assert.throws(() => sharedHelpManifest({ extensionVersion: '9.0.0' }, parsed), /versions differ/);
    assert.throws(() => nativeHelpContent(file, '9.0.0'), /version-mismatched/);
    const duplicate = structuredClone(native); duplicate.topics[1].id = duplicate.topics[0].id;
    await writeFile(file, JSON.stringify(duplicate));
    assert.throws(() => nativeHelpContent(file, '1.2.3'), /duplicate topic/);
  } finally {
    assert.ok(resolve(directory).startsWith(`${resolve(tmpdir())}${sep}`));
    await rm(directory, { recursive: true, force: true });
  }
});
