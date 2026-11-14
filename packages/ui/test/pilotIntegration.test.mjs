// SPDX-License-Identifier: Apache-2.0
// The actual C9 parser shares the same closed local read contract.
import assert from 'node:assert/strict';
import test from 'node:test';
import { readFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
const { build } = createRequire(new URL('../../admin/package.json', import.meta.url))('esbuild');
const bundle = await build({ entryPoints: [fileURLToPath(new URL('../../vscode/src/explorerContracts.ts', import.meta.url))],
  bundle: true, write: false, format: 'esm', platform: 'node', target: 'node24' });
const { parseWebviewMessage, explorerRequestPolicy } = await import(`data:text/javascript;base64,${Buffer.from(bundle.outputFiles[0].text).toString('base64')}`);
const cases = JSON.parse(await readFile(new URL('./fixtures/pilot_cases.json', import.meta.url), 'utf8'));
for (const row of cases.request_cases) {
  test(`C9 integrated pilot admission ${row.name}`, () => {
    const parsed = parseWebviewMessage(row.request);
    assert.equal(parsed !== undefined, row.accepted);
    if (row.accepted) assert.deepEqual(parsed, row.request);
  });
}
test('C9 pilot stays a bounded read without mutation or interactive replay semantics', () => {
  assert.deepEqual(explorerRequestPolicy('pilot.read'), { uncertain: false, interactive: false });
});
