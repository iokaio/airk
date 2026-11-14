// SPDX-License-Identifier: Apache-2.0
// In-memory bundle boundaries only; this writes no compiled/shipped assets.
import assert from 'node:assert/strict';
import test from 'node:test';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
const require = createRequire(new URL('../../vscode/package.json', import.meta.url));
const { build } = require('esbuild');
const root = fileURLToPath(new URL('../../../', import.meta.url));

test('shared pilot native bundle contains exactly the closed adapter and pure request owner', async () => {
  const bundle = await build({ absWorkingDir: root, entryPoints: ['packages/ui/src/nativePilot.ts'], bundle: true,
    write: false, metafile: true, platform: 'node', format: 'cjs', target: 'node20' });
  assert.deepEqual(Object.keys(bundle.metafile.inputs).map(path => path.replaceAll('\\', '/')).sort(), [
    'packages/ui/src/nativePilot.ts', 'packages/ui/src/pilotContract.ts', 'packages/vscode/src/uiPilotContract.ts',
  ]);
  assert.doesNotMatch(bundle.outputFiles[0].text, /\brequire\s*\(/);
});
test('shared editor pilot uses one existing Preact runtime and no browser/session implementation', async () => {
  const bundle = await build({ absWorkingDir: root, entryPoints: ['packages/vscode/src/explorerWebview/pilotPane.tsx'],
    bundle: true, write: false, metafile: true, platform: 'browser', format: 'iife', target: 'es2022', jsx: 'automatic', jsxImportSource: 'preact' });
  const inputs = Object.keys(bundle.metafile.inputs).map(path => path.replaceAll('\\', '/'));
  const framework = inputs.filter(path => path.includes('/node_modules/preact/'));
  assert.ok(framework.length > 0);
  assert.ok(framework.every(path => path.startsWith('packages/vscode/node_modules/preact/')));
  assert.ok(inputs.includes('packages/ui/src/pilotPresentation.ts'));
  assert.ok(inputs.every(path => !path.startsWith('packages/admin/') && !path.includes('nativePilot') && !path.includes('browserPilot')));
  assert.doesNotMatch(bundle.outputFiles[0].text, /\brequire\s*\(|\bfetch\s*\(/);
});
