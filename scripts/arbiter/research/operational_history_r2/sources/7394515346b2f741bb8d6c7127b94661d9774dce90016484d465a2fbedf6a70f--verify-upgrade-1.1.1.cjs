// SPDX-License-Identifier: Apache-2.0
// Local release qualification; intentionally excluded from source control.
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const crypto = require('node:crypto');

async function main() {
  const [oldExtension, newExtension, destination] = process.argv.slice(2).map(p => path.resolve(p));
  const { prepareHelper, runJson, runText, probeMcp } = require(path.join(newExtension, 'out/native.js'));
  await fs.mkdir(destination); // Refuse to overwrite any prior qualification state.
  const root = path.join(destination, 'workspace');
  await fs.mkdir(root);
  const source = 'fn release_upgrade_fixture() {}\n';
  const file = path.join(root, 'fixture.rs');
  await fs.writeFile(file, source, { flag: 'wx' });
  const hash = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
  const storage = path.join(destination, 'extension-storage');
  const oldCached = await prepareHelper(oldExtension, storage, '1.1.0');
  const oldDigest = hash(await fs.readFile(oldCached));
  const spec = { command: oldCached, root, dataDir: path.join(destination, 'state'), collection: 'release-upgrade', name: 'Release upgrade fixture' };
  const oldIndex = await runJson(spec, 'index');
  assert.equal(oldIndex.collection, spec.collection);
  assert.equal((await probeMcp(spec, 16)).tools.length, 16);
  const newCached = await prepareHelper(newExtension, storage, '1.1.1');
  assert.notEqual(newCached, oldCached);
  const next = { ...spec, command: newCached };
  const readArgs = ['fixture.rs', '--start-line', '1', '--end-line', '1'];
  const before = await runJson(next, 'read', readArgs);
  assert.ok(before.hits.some(h => h.text.includes('release_upgrade_fixture')));
  assert.equal((await probeMcp(next, 16)).tools.length, 16);
  const limits = await runJson(next, 'collections', ['limits', 'describe']);
  assert.ok(limits.automatic && limits.effective && limits.maximum);
  const rebuilt = await runJson(next, 'index', ['--full']);
  assert.equal(rebuilt.collection, spec.collection);
  const after = await runJson(next, 'read', readArgs);
  assert.ok(after.hits.some(h => h.text.includes('release_upgrade_fixture')));
  assert.equal(await fs.readFile(file, 'utf8'), source);
  assert.equal(hash(await fs.readFile(oldCached)), oldDigest);
  assert.equal((await runText(oldCached, ['--version'])).trim(), 'airk 1.1.0');
  assert.equal((await runText(newCached, ['--version'])).trim(), 'airk 1.1.1');
  const receipt = { passed: true, oldVersion: '1.1.0', newVersion: '1.1.1', oldIndex, rebuilt, limits, oldHelperPreserved: true, sourceUnchanged: true, mcpTools: 16 };
  await fs.writeFile(path.join(destination, 'receipt.json'), JSON.stringify(receipt, null, 2));
  console.log('PASS: packaged 1.1.0 -> 1.1.1 helper upgrade, preserved cache/source, prior-generation read, full rebuild, limits and MCP.');
}
main().catch(error => { console.error(error); process.exitCode = 1; });


