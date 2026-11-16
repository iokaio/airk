// SPDX-License-Identifier: Apache-2.0
// U110: the manual agent snippets carry only the endpoint URL and the
// X-AIRK-Response-Mode header: no Authorization, header helper, credential
// command or connection-id placeholder. Expected documents are written out here.
import assert from 'node:assert/strict';
import test from 'node:test';
import { readFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { build } from 'esbuild';

const src = resolve(dirname(fileURLToPath(import.meta.url)), '../src');
const bundle = await build({ stdin: { contents: `export { manualEntriesOf } from './agents';`, resolveDir: src, sourcefile: 'u110-snippets.ts' },
  bundle: true, write: false, platform: 'node', format: 'esm', jsx: 'automatic', jsxImportSource: 'preact' });
const { manualEntriesOf } = await import(`data:text/javascript;base64,${Buffer.from(bundle.outputFiles[0].text).toString('base64')}`);
const url = 'https://127.0.0.1:61021/mcp';

test('native manual entries preserve exact text, target and format, while malformed rows stay unavailable', () => {
  const entries=[{client:'claude',scope:'project',file:'/work/a & b/.mcp.json',format:'json',text:'{\n  "mcpServers": {}\n}\n'},
    {client:'codex',scope:'user',file:'/home/a/.codex/config.toml',format:'toml',text:'[mcp_servers.airk]\nurl = "https://127.0.0.1:61021/mcp"\n'},
    {client:'vscode',scope:'project',file:'.vscode/mcp.json',format:'json',text:'{"servers":{}}\n'}];
  const response={phase:'done',result:{kind:'ok',envelope:{result:{manual:entries}}}};
  assert.deepEqual(manualEntriesOf(response),entries);
  for(const entry of entries)assert.strictEqual(manualEntriesOf(response).find(row=>row.client===entry.client),entry);
  const invalid=[null,{}, {...entries[0],text:42},{...entries[0],format:'xml'},{...entries[0],client:'unknown'}];
  assert.deepEqual(manualEntriesOf({phase:'done',result:{kind:'ok',envelope:{result:{manual:invalid}}}}),[]);
  assert.deepEqual(manualEntriesOf({phase:'idle'}),[]);
});

test('U110 the Agents page source offers no credential, revoke or connection-id flow', async () => {
  const source = await readFile(resolve(src, 'agents.tsx'), 'utf8');
  for (const forbidden of [/Authorization:/, /headersHelper/, /http_headers_helper/, /<connection-id>/, /agents credential/, /agentsRevoke/, /--connection-id/, /airkToken_/]) {
    assert.doesNotMatch(source, forbidden, String(forbidden));
  }
  assert.match(source, /airkctl scope default --workspace/);
  assert.match(source, /list_scopes/);
});
