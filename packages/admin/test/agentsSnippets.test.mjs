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
const bundle = await build({ stdin: { contents: `export { claudeJson, codexToml, vscodeJson } from './agents';`, resolveDir: src, sourcefile: 'u110-snippets.ts' },
  bundle: true, write: false, platform: 'node', format: 'esm', jsx: 'automatic', jsxImportSource: 'preact' });
const { claudeJson, codexToml, vscodeJson } = await import(`data:text/javascript;base64,${Buffer.from(bundle.outputFiles[0].text).toString('base64')}`);
const url = 'https://127.0.0.1:61021/mcp';

test('U110 Claude Code, VS Code and Codex snippets name only the URL and the response-mode header', () => {
  assert.deepEqual(JSON.parse(claudeJson(url)), { mcpServers: { airk: { type: 'http', url, headers: { 'X-AIRK-Response-Mode': 'terse' } } } });
  assert.deepEqual(JSON.parse(vscodeJson(url)), { servers: { airk: { type: 'http', url, headers: { 'X-AIRK-Response-Mode': 'terse' } } } });
  assert.equal(codexToml(url), `[mcp_servers.airk]\nurl = "${url}"\n\n[mcp_servers.airk.http_headers]\n"X-AIRK-Response-Mode" = "terse"\n`);
});

test('U110 the Agents page source offers no credential, revoke or connection-id flow', async () => {
  const source = await readFile(resolve(src, 'agents.tsx'), 'utf8');
  for (const forbidden of [/Authorization:/, /headersHelper/, /http_headers_helper/, /<connection-id>/, /agents credential/, /agentsRevoke/, /--connection-id/, /airkToken_/]) {
    assert.doesNotMatch(source, forbidden, String(forbidden));
  }
  assert.match(source, /airkctl scope default --workspace/);
  assert.match(source, /list_scopes/);
});
