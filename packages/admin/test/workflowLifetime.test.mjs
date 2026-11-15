// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import {build} from 'esbuild';
import {fileURLToPath} from 'node:url';
const root=fileURLToPath(new URL('../',import.meta.url)),compiled=await build({absWorkingDir:root,entryPoints:['src/workflowLifetime.ts'],bundle:true,write:false,format:'esm',platform:'browser'}),{guardWorkflowCapture}=await import(`data:text/javascript;base64,${Buffer.from(compiled.outputFiles[0].contents).toString('base64')}`);
test('hidden original view refuses before send and after a held native completion without replay',async()=>{
 let visible=true,epoch=1,sends=0,release,entered;const barrier=new Promise(resolve=>entered=resolve),held=new Promise(resolve=>release=resolve),original=epoch;
 const actual={indexIds:Object.freeze(['actual-index']),backingBinding:'actual-backing',checkCurrent(){},async read(){sends++;entered();await held;return {raw:{full:true},logical:{full:true},scopeIdentity:'actual-scope',request:{kept:true}};}};
 const capture=guardWorkflowCapture(actual,()=>visible&&epoch===original),pending=capture.read('file_info',{path:'first.rs'});await barrier;visible=false;epoch++;visible=true;release();await assert.rejects(pending,/hidden or replaced/);await assert.rejects(capture.read('file_info',{path:'second.rs'}),/hidden or replaced/);assert.equal(sends,1);assert.equal(capture.indexIds,actual.indexIds);assert.equal(capture.backingBinding,actual.backingBinding);
});
test('presentation wrappers retain native revocation fences and complete exact successful observations',async()=>{
 let permitted=true,sends=0;const response={raw:{unknown:[1,2,3]},logical:{unknown:[1,2,3]},scopeIdentity:'scope',request:{args:{future:true}}};
 const capture=guardWorkflowCapture({indexIds:[],checkCurrent(){if(!permitted)throw Error('native_scope_revoked');},async read(){sends++;return response;}},()=>true);
 assert.equal(await capture.read('symbols',{}),response);permitted=false;await assert.rejects(capture.read('symbols',{}),/native_scope_revoked/);assert.equal(sends,1);
});
