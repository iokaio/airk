// SPDX-License-Identifier: Apache-2.0
/** Actual shared console adapter uses native metadata reads, never a preparation job. */
import assert from 'node:assert/strict';
import test from 'node:test';
import {ServiceConsoleController} from '../serviceConsoleController';
import {ServiceClient,type RecordValue} from '../serviceClient';
import {IntentJournal,type IntentStorage} from '../serviceIntents';
const id=(x:string)=>x.repeat(32),h=(x:string)=>x.repeat(64),namespace=id('1'),index=`idx_${id('2')}`,project=`prj_${id('3')}`,binding=h('4'),token=`airk-r1:${id('5')}:1:${h('6')}`;
class Memory implements IntentStorage {readonly data=new Map<string,unknown>();keys(){return [...this.data.keys()];}get<T>(key:string){return this.data.get(key) as T|undefined;}async update(key:string,value:unknown){if(value===undefined)this.data.delete(key);else this.data.set(key,structuredClone(value));}}
function fixture(){let revoked=false,pending=false;const calls:string[]=[],storage=new Memory(),selection={schema:1,context_key:h('7'),namespace,expected_catalog_revision:1,grant_id:`grt_${id('8')}`,expected_grant_revision:1,project_ids:[project],index_ids:[index]};
 const client=new ServiceClient({dispose(){},async request(method){const name=typeof method==='string'?method:Object.keys(method as object)[0];calls.push(name);
  if(name==='catalog_host_context')return {status:name,descriptor:{schema:1,namespace,host:h('9'),key:h('7'),catalog_revision:1,context:null}};
  if(name==='catalog_detail'){const entity=(method as any).catalog_detail.entity;return {status:name,capabilities:{reference_refresh_contract:1},detail:{namespace,revision:1,item:entity.kind==='grant'?{kind:'grant',value:{grant:{id:selection.grant_id,revision:1,host:h('9'),revoked,memberships:[{membership:{project_id:project,index_id:index}}]}}}:{kind:'index',value:{id:index,tombstone:false,backing:{binding}}}}};}
  if(name==='inspection')return {status:name,observation:{snapshot:{project:binding,incarnation:id('5'),settings_digest:h('a'),publication:token,source_state:'available',recovery_required:pending}}};
  if(name==='request_epoch')return {status:name,namespace,epoch:0};
  throw new Error('unexpected native mutation/read '+name);
 }});
 const controller=new ServiceConsoleController({version:'1.8.8',authority:'fixture',storage,trusted:()=>true,changed(){},runtime:{prepareConfiguration:async()=>{throw new Error('generic CLI prepare must not run');}},confirm:async()=>true} as any);
 const internal=controller as any;internal.client=client;internal.journal=new IntentJournal(storage,'/control',namespace);internal.registryNamespace=namespace;controller.state.connected=true;
 const members=[{index_id:index,original_publication:token,sources:[{source_root_id:`src_${id('b')}`,source_relative_path:'one.rs',canonical_root:'/fixture/source',indexed_hash:h('c')}]}],query={tool:'airk_search',arguments:{query:'same query',verification:'require',future_query:{preserve:true}}},result={future_complete_result:{preserve:true}};
 const host={trusted:()=>true,confirm:async()=>true,persistEvidenceAllowed:()=>false,reconfirm:async(value:RecordValue)=>value,query:async()=>({})};
 return {controller,storage,calls,selection,members,query,result,host,revoke:()=>{revoked=true;},pending:()=>{pending=true;}};
}
test('shared native host prepares exact refresh from context grant index and inspection without enqueue or CLI preparation',async()=>{const f=fixture();try{const ready=await f.controller.referenceRefreshPrepare(f.selection,f.members,'selected_only',f.query,f.result,f.host,()=>true);assert.deepEqual(ready.intent.reference_refresh.original_query,f.query);assert.deepEqual(ready.intent.reference_refresh.original_result,f.result);assert.equal(ready.intent.reference_refresh.evidence_retention,'memory_only');assert.equal(ready.intent.reference_refresh.configure_project.key.epoch,0);const request=ready.intent.reference_refresh.configure_project.request;assert.equal(request.incarnation,id('5'));assert.equal(request.settings,h('a'));assert.equal(ready.intent.reference_refresh.configure_project.project,binding);assert.deepEqual(f.calls,['catalog_host_context','catalog_detail','catalog_detail','inspection','request_epoch']);assert.equal(f.storage.data.size,0,'review preparation reserves no mutation journal row');}finally{f.controller.dispose();}});
test('revoked native grant and pending recovery refuse before key journal or mutation dispatch',async()=>{for(const reason of ['grant','recovery']){const f=fixture();try{if(reason==='grant')f.revoke();else f.pending();await assert.rejects(f.controller.referenceRefreshPrepare(f.selection,f.members,'full_reconcile_reviewed',f.query,f.result,f.host,()=>true),/review_required/);assert.ok(!f.calls.includes('request_epoch'));assert.equal(f.storage.data.size,0);assert.ok(!f.calls.includes('enqueue')&&!f.calls.includes('configure_project'));}finally{f.controller.dispose();}}});
