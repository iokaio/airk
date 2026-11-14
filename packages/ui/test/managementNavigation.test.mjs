// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { readFile } from 'node:fs/promises';
const require = createRequire(new URL('../../vscode/package.json', import.meta.url));
const { build } = require('esbuild');
const root = fileURLToPath(new URL('../../vscode/', import.meta.url));
const manifest = await readFile(new URL('../generated/help-manifest.json', import.meta.url), 'utf8');
const bundled = await build({ absWorkingDir: root, entryPoints: ['src/explorerWebview/workspaces.tsx'], bundle: true, write: false,
  format: 'esm', platform: 'node', jsx: 'automatic', jsxImportSource: 'preact', alias: { preact: `${root}/node_modules/preact` },
  plugins: [{ name: 'inert-navigation-observation', setup(plugin) {
    plugin.onResolve({ filter: /^\.\/api$/ }, args => args.importer.includes('explorerWebview') ? { path: 'api', namespace: 'inert-api' } : undefined);
    plugin.onLoad({ filter: /.*/, namespace: 'inert-api' }, () => ({ contents: `export class HostFailure extends Error{};export const onHost=()=>()=>{};export const savedState=()=>undefined;export const saveState=()=>{};export const notify=()=>{};export const ready=()=>{};export const request=()=>{throw Error('Navigation must not invoke native');};` }));
    plugin.onResolve({ filter: /^airk-explorer-help$/ }, () => ({ path: 'help', namespace: 'inert-help' }));
    plugin.onLoad({ filter: /.*/, namespace: 'inert-help' }, () => ({ contents: manifest, loader: 'json' }));
    plugin.onResolve({ filter: /^airk-shared-admin-css$/ }, () => ({ path: 'style', namespace: 'inert-style' }));
    plugin.onLoad({ filter: /.*/, namespace: 'inert-style' }, () => ({ contents: '', loader: 'js' }));
  } }] });
const { WorkspaceView } = await import(`data:text/javascript;base64,${Buffer.from(bundled.outputFiles[0].text).toString('base64')}`);
const nodes = value => !value || typeof value !== 'object' ? [] : Array.isArray(value) ? value.flatMap(nodes) : [value, ...nodes(value.props?.children)];
test('actual shared native page changes preserve existing inert editor routes and one owning shell', () => {
  const routes = [], tree = WorkspaceView({ route: { workspace: 'manage', tab: 'service' },
    state: { features: { 'manage/service': 'implemented' }, adminManagement: { epoch: 1, available: true } }, go: route => routes.push(route) });
  const shared = nodes(tree).find(node => node.type?.name === 'SharedManagement');
  assert.ok(shared); assert.equal(shared.props.page, 'service');
  for (const page of ['activity', 'usage', 'agents', 'storage', 'recovery', 'logs', 'index-library', 'search', 'help']) shared.props.onPageChange(page);
  assert.deepEqual(routes, [{ workspace: 'manage', tab: 'activity' }, { workspace: 'manage', tab: 'usage' }, { workspace: 'manage', tab: 'agents' },
    { workspace: 'manage', tab: 'storage' }, { workspace: 'manage', tab: 'repair' }, { workspace: 'manage', tab: 'logs' },
    { workspace: 'indexing', tab: 'library' }, { workspace: 'search', tab: 'query' }, { workspace: 'help' }]);
  assert.equal(nodes(tree).some(node => node.props?.role === 'tablist'), false, 'shared page does not duplicate the legacy Management tab strip');
  assert.ok(nodes(tree).some(node => node.type === 'details'), 'prior editor workflows remain available');
});
