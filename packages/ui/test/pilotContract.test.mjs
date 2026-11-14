// SPDX-License-Identifier: Apache-2.0
// Independent local-host cases; native wire types are imported, never mirrored.
import assert from 'node:assert/strict';
import test from 'node:test';
import { readFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
const { build } = createRequire(new URL('../../admin/package.json', import.meta.url))('esbuild');
const bundle = await build({ entryPoints: [fileURLToPath(new URL('../src/pilotContract.ts', import.meta.url))],
  bundle: true, write: false, format: 'esm', platform: 'node', target: 'node24' });
const { admitPilotRead, parsePilotRequest } = await import(`data:text/javascript;base64,${Buffer.from(bundle.outputFiles[0].text).toString('base64')}`);

const cases = JSON.parse(await readFile(new URL('./fixtures/pilot_cases.json', import.meta.url), 'utf8'));
assert.equal(cases.version, 1);
assert.equal(cases.request_cases.length, 20);
assert.equal(cases.admission_cases.length, 9);
for (const row of cases.request_cases) {
  test(`shared pilot request ${row.name}`, () => {
    const parsed = parsePilotRequest(row.request);
    assert.equal(parsed !== null, row.accepted);
    if (row.accepted) {
      assert.deepEqual(parsed, row.request, 'preserve every admitted field');
      assert.notEqual(parsed, row.request, 'capture an independent request identity');
      assert.notEqual(parsed.read, row.request.read, 'capture immutable read inputs separately from renderer input');
      assert.equal(Object.isFrozen(parsed), true);
      assert.equal(Object.isFrozen(parsed.read), true);
    }
  });
}
for (const row of cases.admission_cases) {
  test(`shared pilot admission ${row.name}`, () => {
    const request = cases.request_cases.find(candidate => candidate.name === row.request)?.request;
    assert.ok(request, row.request);
    assert.equal(admitPilotRead(request, row.context), row.refusal);
  });
}
