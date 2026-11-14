// SPDX-License-Identifier: Apache-2.0
// Source-only bundle closure; no native invocation or artifact qualification.
import assert from 'node:assert/strict';
import test from 'node:test';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
const require = createRequire(new URL('../../vscode/package.json', import.meta.url));
const { build } = require('esbuild');
const { sharedAdminStylePlugin } = require('./scripts/build-explorer.cjs');
const root = fileURLToPath(new URL('../../vscode/', import.meta.url));
export const bundle = await build({ absWorkingDir: root, entryPoints: ['src/explorerWebview/sharedManagement.tsx'], bundle: true, write: false,
  metafile: true, outdir: 'out/unused-source-check', platform: 'browser', format: 'iife', jsx: 'automatic', jsxImportSource: 'preact',
  alias: { preact: `${root}/node_modules/preact` }, plugins: [sharedAdminStylePlugin()] });
test('actual shared management bundles the same presentation with exactly one consuming Preact owner', () => {
  const inputs = Object.keys(bundle.metafile.inputs).map(input => input.replaceAll('\\', '/'));
  assert.ok(inputs.includes('../admin/src/app.tsx'));
  assert.ok(inputs.includes('../admin/src/indexLibrary.tsx'));
  assert.ok(inputs.includes('../admin/src/arbiter.tsx'));
  assert.ok(inputs.includes('../ui/src/management.ts'));
  assert.equal(inputs.filter(input => input.endsWith('/preact/dist/preact.module.js')).length, 1);
  assert.equal(inputs.some(input => input.includes('../admin/node_modules/preact')), false);
  assert.equal(inputs.some(input => input.endsWith('/admin/src/main.tsx')), false);
  assert.equal(inputs.some(input => input.includes('node:') || input.endsWith('/adminUiHost.ts')), false);
});
test('shared management source CSS remains scoped and includes actual folder-dialog styles', () => {
  const css = bundle.outputFiles.find(file => file.path.endsWith('.css')).text;
  assert.match(css, /@scope\s*\(\.airk-shared-management\)/);
  assert.equal(/\.overlay\s*\{/.test(css) && /\.picker\s*\{/.test(css), true, 'actual folder-picker overlay and panel rules are retained');
  assert.doesNotMatch(css, /(^|\n)body\s*\{/);
});
if (process.env.AIRK_PRINT_MANAGEMENT_INPUTS) console.log(Object.keys(bundle.metafile.inputs).filter(input => input.startsWith('../')).sort().join('\n'));
