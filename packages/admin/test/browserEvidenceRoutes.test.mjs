// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import {build} from 'esbuild';
import {fileURLToPath} from 'node:url';
const root=fileURLToPath(new URL('../',import.meta.url)),bundle=await build({absWorkingDir:root,stdin:{contents:"export * from './src/browserEvidenceRoutes';export {evidenceToolsPresentation} from '../ui/src/navigation';",resolveDir:root,sourcefile:'browser-routes-test.ts'},bundle:true,write:false,format:'esm',platform:'node',jsx:'automatic',jsxImportSource:'preact'});
const api=await import(`data:text/javascript;base64,${Buffer.from(bundle.outputFiles[0].contents).toString('base64')}`),expected=['code/symbols','code/references','code/outline','docs/facts','evidence/inspector','evidence/checker','evidence/changes','review/changes','review/impact','review/tests','strategies/library','strategies/studio','strategies/planner'];
const routes=node=>typeof node==='string'?[]:[...(node.route?[node.route]:[]),...node.children.flatMap(routes)];
test('all thirteen current browser guided routes have named hub controls and retained editor targets',()=>{assert.deepEqual(api.BROWSER_EVIDENCE_ROUTES,expected);assert.deepEqual(routes(api.evidenceToolsPresentation('browser',undefined,api.BROWSER_EVIDENCE_ROUTES)),expected);assert.deepEqual(routes(api.evidenceToolsPresentation('editor')),expected);for(const route of expected)assert.equal(api.isBrowserEvidenceRoute(route),true);});
test('arbitrary native tool names and malformed browser deep-links never become a guided route',()=>{for(const value of ['search','file_info','review/git','code/symbols/extra','../strategies/studio','source.open',null,{}])assert.equal(api.isBrowserEvidenceRoute(value),false);});
