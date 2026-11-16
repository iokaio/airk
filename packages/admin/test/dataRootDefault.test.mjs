// SPDX-License-Identifier: Apache-2.0
// Index setup data root: an empty field means the default inside the
// installation's data location, and Review never depends on the field having
// been pre-filled (1.8.205: the default was missing and Review stayed disabled).
import assert from 'node:assert/strict';
import test from 'node:test';
import { build } from 'esbuild';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../', import.meta.url));
const bundle = await build({
  absWorkingDir: root,
  entryPoints: ['src/dataRootDefault.ts'],
  bundle: true,
  write: false,
  format: 'esm',
  platform: 'node',
  jsx: 'automatic',
  jsxImportSource: 'preact',
});
const { dataRootChoice, dataLocationOf } = await import(
  `data:text/javascript;base64,${Buffer.from(bundle.outputFiles[0].contents).toString('base64')}`
);

const base = 'C:\\Users\\fixture\\AppData\\Local\\AIRK Service\\data\\indexes';

test('an empty field uses the collection folder inside the data location', () => {
  assert.deepEqual(
    dataRootChoice({ typed: '', dataBase: base, lookupError: null, collection: 'airk', collectionValid: true }),
    { root: `${base}\\airk`, isDefault: true },
  );
  assert.deepEqual(
    dataRootChoice({ typed: '   ', dataBase: base, lookupError: null, collection: 'airk', collectionValid: true }),
    { root: `${base}\\airk`, isDefault: true },
  );
});

test('a typed or picked data root is sent as-is, even before the default is known', () => {
  for (const dataBase of [base, null]) {
    assert.deepEqual(
      dataRootChoice({ typed: ' D:\\indexes\\airk ', dataBase, lookupError: null, collection: 'airk', collectionValid: true }),
      { root: 'D:\\indexes\\airk', isDefault: false },
    );
  }
});

test('an unknown data location is looked up at review time, and a failed lookup is reported', () => {
  assert.deepEqual(
    dataRootChoice({ typed: '', dataBase: null, lookupError: null, collection: 'airk', collectionValid: true }),
    { pending: true },
  );
  const failed = dataRootChoice({
    typed: '',
    dataBase: null,
    lookupError: 'Sign in again to browse folders.',
    collection: 'airk',
    collectionValid: true,
  });
  assert.ok('missing' in failed && failed.missing.includes('Sign in again to browse folders.'));
  const invalid = dataRootChoice({ typed: '', dataBase: base, lookupError: null, collection: '..', collectionValid: false });
  assert.ok('missing' in invalid);
});

test('the data location is the starting-list entry the service marks as data', () => {
  // The exact starting list the 1.8.205 service returned (paths shortened).
  const listing = {
    entries: [
      { name: 'C:', path: 'C:\\', kind: 'folder', start: 'drive' },
      { name: 'tdotn', path: 'C:\\Users\\tdotn', kind: 'folder', start: 'home' },
      { name: 'indexes', path: base, kind: 'folder', start: 'airk_location', airk_location: 'data' },
    ],
  };
  assert.equal(dataLocationOf(listing), base);
  assert.equal(dataLocationOf({ entries: listing.entries.slice(0, 2) }), null);
});
