// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import {browserAvailable,browserHarness} from './browserHarness.mjs';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { readFile } from 'node:fs/promises';
const require = createRequire(new URL('../../vscode/package.json', import.meta.url));
const { build } = require('esbuild');
const root = fileURLToPath(new URL('../../vscode/', import.meta.url));
const manifest = await readFile(new URL('../generated/help-manifest.json', import.meta.url), 'utf8');
const bundled = await build({ absWorkingDir: root, stdin: {resolveDir:root,sourcefile:'workspace-navigation.tsx',contents:`import {h,render} from 'preact';import {WorkspaceView} from './src/explorerWebview/workspaces';window.routes=[];render(h(WorkspaceView,{route:{workspace:'manage',tab:'service'},state:{features:{'manage/service':'implemented'},adminManagement:{epoch:1,available:true}},go:route=>window.routes.push(route)}),document.body);`}, bundle: true, write: false,
  format: 'iife', platform: 'browser', jsx: 'automatic', jsxImportSource: 'preact', alias: { preact: `${root}/node_modules/preact` },
  plugins: [{ name: 'inert-navigation-observation', setup(plugin) {
    plugin.onResolve({filter:/^\.\/sharedManagement$/},()=>({path:'shared',namespace:'captured-props'}));
    plugin.onLoad({filter:/.*/,namespace:'captured-props'},()=>({contents:`import {h} from 'preact';export function SharedManagement(props){window.shared=props;return h('div',{'data-observed-shared':true});}`,resolveDir:root}));
    plugin.onResolve({ filter: /^\.\/api$/ }, args => args.importer.includes('explorerWebview') ? { path: 'api', namespace: 'inert-api' } : undefined);
    plugin.onLoad({ filter: /.*/, namespace: 'inert-api' }, () => ({ contents: `export class HostFailure extends Error{};export const onHost=()=>()=>{};export const savedState=()=>undefined;export const saveState=()=>{};export const notify=()=>{};export const ready=()=>{};export const request=()=>{throw Error('Navigation must not invoke native');};` }));
    plugin.onResolve({ filter: /^airk-explorer-help$/ }, () => ({ path: 'help', namespace: 'inert-help' }));
    plugin.onLoad({ filter: /.*/, namespace: 'inert-help' }, () => ({ contents: manifest, loader: 'json' }));
    plugin.onResolve({ filter: /^airk-shared-admin-css$/ }, () => ({ path: 'style', namespace: 'inert-style' }));
    plugin.onLoad({ filter: /.*/, namespace: 'inert-style' }, () => ({ contents: '', loader: 'js' }));
  } }] });
test('actual shared native page changes preserve existing inert editor routes and one owning shell',
  {skip:browserAvailable?false:'Chromium unavailable.'},async()=>{
  const browser=await browserHarness();try{
    await browser.evaluate(bundled.outputFiles[0].text);
    await browser.observed(`window.shared&&document.querySelector('[data-observed-shared]')`);
    assert.equal(await browser.evaluate('window.shared.page'),'service');
    await browser.evaluate(`for(const page of ['activity','usage','agents','storage','recovery','logs','index-library','search','help'])window.shared.onPageChange(page)`);
    assert.deepEqual(await browser.evaluate('window.routes'),[{workspace:'manage',tab:'activity'},{workspace:'manage',tab:'usage'},{workspace:'manage',tab:'agents'},
      {workspace:'manage',tab:'storage'},{workspace:'manage',tab:'repair'},{workspace:'manage',tab:'logs'},
      {workspace:'indexing',tab:'library'},{workspace:'search',tab:'query'},{workspace:'help'}]);
    assert.equal(await browser.evaluate(`document.querySelectorAll('[role="tablist"]').length`),0,'shared page does not duplicate the legacy Management tab strip');
    assert.equal(await browser.evaluate(`document.querySelector('details')!==null`),true,'prior editor workflows remain available');
  }finally{await browser.close();}
});
