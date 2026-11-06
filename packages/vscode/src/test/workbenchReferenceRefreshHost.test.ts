// SPDX-License-Identifier: Apache-2.0
/** Actual message and host callbacks; mocks do not establish native qualification. */
import assert from 'node:assert/strict';
import Module from 'node:module';
import test from 'node:test';
import {parseWorkbenchMessage,object,type Value} from '../workbenchContracts';
import {newSession} from '../workbenchSessions';
import {captureWorkbenchRefresh} from '../workbenchReferenceRefresh';
const h=(x:string)=>x.repeat(64),id=(x:string)=>x.repeat(32),index=`idx_${id('2')}`,project=`prj_${id('3')}`,source=`src_${id('4')}`,publication=`airk-r1:${id('5')}:1:${h('6')}`;
function fixture(){
 let pendingConfirmation:{entered():void;wait:Promise<void>}|undefined;
 let errorReply=false,trusted=true,confirmation=true,submitted=0,checks=0,queries=0,sourceReads=0,prepared:Value|undefined,hostCallbacks:any,late:(()=>void)|undefined,pending=false;
 const selection={schema:1,context_key:h('a'),namespace:id('b'),expected_catalog_revision:1,grant_id:`grt_${id('c')}`,expected_grant_revision:1,project_ids:[project],index_ids:[index]};
 const spec={command:'/fixture/airk',root:'/fixture/source',dataDir:'/fixture/data',collection:'fixture',name:'Fixture',combinedSession:selection,service:{installDir:'/fixture/runtime',project:h('d')}};
 const query={query:'original full query',mode:'hybrid',verification:'require',verification_max_bytes:12345,verification_deadline_ms:2000,reply_detail:'rich',future_native_argument:{preserve:true},project_ids:[project],index_ids:[index]};
 const original={schema:'airk.combined-read/1',future_original:{preserve:true}},envelope={executed:true,status:'complete',steps:[{id:'step',tool:'search',arguments:query,status:'complete',result:original}],evidence:[{step:'step',publication,text:'original snippet',citation:{path:'api/one.rs',start_line:1,end_line:1,content_hash:h('e')},origin:{index_id:index,project_ids:[project],source_root_id:source,source_relative_path:'api/one.rs'}}]};
 const flow={reviewAndSubmit:async()=>{if(!await hostCallbacks.confirm('Exact native mutation review','Refresh reviewed files'))return undefined;submitted++;pending=true;throw new Error('refresh_outcome_unknown_check_exact_receipt');},check:async()=>{checks++;return {status:'reference_refresh_receipt',future_receipt:'complete'};},requery:async()=>{checks++;if(!await hostCallbacks.confirm('Run exact query once','Requery current index'))throw new Error('declined');const current=await hostCallbacks.query({tool:'airk_search',arguments:structuredClone(query)},selection);return {original:structuredClone(original),current,native_receipt:{future_receipt:true}};}};
 const control={state:{},referenceRefreshPending:()=>pending?[{id:id('f')}]:[],referenceRefreshReconfirm:async(value:Value)=>structuredClone(value),referenceRefreshFlow:()=>flow,referenceRefreshPrepare:async(selected:Value,members:Value[],mode:string,savedQuery:Value,result:Value,host:any)=>{prepared={selected,members,mode,query:savedQuery,result};hostCallbacks=host;return {flow,intent:{reference_refresh:{configure_project:{key:{nonce:id('f')}}}},capabilities:{capabilities:{reference_refresh_contract:1}}};}};
 class Emitter<T>{readonly event=(_fn:(e:T)=>void)=>({dispose(){}});dispose(){}fire(_e:T){}}
 class Client {constructor(..._args:any[]){}cancel(){}dispose(){}async call(tool:string,args:Value){if(tool==='airk_file_info'){sourceReads++;assert.equal(args.verification,'off');return {payload:{members:[{index_id:index,outcome:'complete',result:{source:{root:'/fixture/source',relative_path:'api/one.rs'},content_hash:h('e')}}],ranking:[{index_id:index,result_pointer:'/members/0/result',origin:{index_id:index,project_ids:[project],source_root_id:source,source_relative_path:'api/one.rs',publication:{token:publication}}}],schema:'airk.combined-read/1'}};}queries++;assert.equal(tool,'airk_search');assert.deepEqual(args,query);if(errorReply){const error=new Error('known_hash does not match current indexed source') as Error&{nativeResponse?:Value};error.nativeResponse={isError:true,future_error:{preserve:true}};throw error;}if(late)await new Promise<void>(resolve=>{late=resolve;});return {payload:{schema:'airk.combined-read/1',future_current:{preserve:true}}};}}
 const vscode={EventEmitter:Emitter,workspace:{getConfiguration:()=>({get:(key:string,fallback:unknown)=>key==='workbench.persistSessions'?false:fallback,inspect:()=>({globalValue:true})}),onDidChangeConfiguration:()=>({dispose(){}})},window:{showWarningMessage:async(_text:string,_options:any,label:string)=>{const held=pendingConfirmation;pendingConfirmation=undefined;if(held){held.entered();await held.wait;}return confirmation?label:undefined;}},commands:{executeCommand:async()=>undefined}};
 const loader=Module as unknown as {_load:(name:string,...args:any[])=>any},old=loader._load;loader._load=function(name:string,...args:any[]){if(name==='vscode')return vscode;if(name==='./explorerEvidence')return {...old.call(this,name,...args),EvidenceClient:Client};return old.call(this,name,...args);};
 let View:any,key:string;
 try{delete require.cache[require.resolve('../workbenchView')];const module=require('../workbenchView');View=module.WorkbenchView;key=module.workbenchProjectKey(spec);}finally{loader._load=old;}
 const view=new View({globalStorageUri:{fsPath:'/unused'},extensionUri:{fsPath:'/fixture/extension'}},{trusted:()=>trusted,specs:()=>[spec],activeProject:()=>key!,onDidChangeSpecs:()=>({dispose(){}}),version:'1.8.8',control:{controller:control}} as any);
 view.session=newSession(key!);const turn={id:'turn',at:1,input:'original query',format:'structured',envelope};view.session.turns.push(turn);view.turnAuthorities.set(turn,JSON.stringify(spec));
 const send=async(raw:Value)=>{const message=parseWorkbenchMessage(raw);assert.ok(message);return view.message(message);};
 const holdConfirmation=()=>{let enter!:()=>void,release!:()=>void;const entered=new Promise<void>(resolve=>{enter=resolve;}),wait=new Promise<void>(resolve=>{release=resolve;});pendingConfirmation={entered:enter,wait};return {entered,release};};
 return {holdConfirmation,view,turn,envelope,query,selection,send,control,refuseQuery:()=>{errorReply=true;},counts:()=>({submitted,checks,queries,sourceReads}),prepared:()=>prepared,setTrusted:(value:boolean)=>{trusted=value;},decline:()=>{confirmation=false;},markRestored:()=>{view.turnAuthorities=new WeakMap();},delay:()=>{late=()=>{};},release:()=>late?.()};
}
test('opaque refresh message rejects native intent authority and persistence injection',()=>{
 const base={type:'refreshReferences',turn:'turn',cards:['record-0'],mode:'selected_only'};assert.ok(parseWorkbenchMessage(base));
 for(const extra of [{selection:{}},{persist:true},{request:{}},{mode:'automatic_full'}])assert.equal(parseWorkbenchMessage({...base,...extra}),undefined);
 assert.equal(parseWorkbenchMessage({...base,cards:['record-0','record-0']}),undefined);
});
test('actual host captures full query and native location; lost submission checks exact receipt without replay',async()=>{
 const f=fixture();try{await assert.rejects(f.send({type:'refreshReferences',turn:'turn',cards:['record-0'],mode:'selected_only'}),/outcome_unknown/);assert.deepEqual(object(f.prepared()!.query).arguments,f.query);assert.equal(f.counts().submitted,1);assert.equal(f.counts().queries,0);assert.equal(f.counts().sourceReads,1);await f.send({type:'checkReferenceRefresh',id:id('f')});assert.deepEqual(f.counts(),{submitted:1,checks:1,queries:0,sourceReads:1});assert.deepEqual(f.turn.envelope,f.envelope);}finally{f.view.dispose();}
});
test('restored turn cannot mint authority and declined full review does not submit',async()=>{
 const f=fixture();try{f.markRestored();await assert.rejects(f.send({type:'refreshReferences',turn:'turn',cards:['record-0'],mode:'selected_only'}),/live_authorized/);assert.equal(f.counts().sourceReads,0);assert.equal(f.counts().submitted,0);}finally{f.view.dispose();}
 const g=fixture();try{g.decline();await g.send({type:'refreshReferences',turn:'turn',cards:['record-0'],mode:'full_reconcile_reviewed'});assert.equal(g.counts().submitted,0);assert.equal(g.control.referenceRefreshPending().length,0);assert.equal(g.prepared()!.mode,'full_reconcile_reviewed');}finally{g.view.dispose();}
});
test('actual host requery keeps exact settings and separate result; late response is only diagnostic',async()=>{
 const f=fixture();try{await assert.rejects(f.send({type:'refreshReferences',turn:'turn',cards:['record-0'],mode:'selected_only'}));await f.send({type:'requeryReferenceRefresh',id:id('f')});assert.equal(f.counts().queries,1);assert.equal(f.view.session.turns.length,2);assert.deepEqual(f.turn.envelope,f.envelope);assert.equal(f.view.session.turns[1].envelope.future_current.preserve,true);}finally{f.view.dispose();}
 const g=fixture();try{await assert.rejects(g.send({type:'refreshReferences',turn:'turn',cards:['record-0'],mode:'selected_only'}));g.delay();const waiting=g.send({type:'requeryReferenceRefresh',id:id('f')});for(let n=0;n<20&&g.counts().queries===0;n++)await Promise.resolve();assert.equal(g.counts().queries,1);g.setTrusted(false);g.release();await assert.rejects(waiting,/historical_diagnostic/);const last=g.view.session.turns.at(-1);assert.equal(last.envelope,undefined);assert.equal(last.nativeFailure.future_current.preserve,true);assert.deepEqual(g.turn.envelope,g.envelope);}finally{g.view.dispose();}
});
test('mixed native step handles refuse before query reconstruction',()=>{
 const f=fixture();try{const envelope=structuredClone(f.envelope);envelope.evidence.push({...envelope.evidence[0],step:'different'});assert.throws(()=>captureWorkbenchRefresh(f.turn as any,envelope,['record-0','record-1'],f.selection),/one_qualified_step/);}finally{f.view.dispose();}
});

test('actual native query refusal keeps full diagnostic without claiming a late successful response',async()=>{
 const f=fixture();try{await assert.rejects(f.send({type:'refreshReferences',turn:'turn',cards:['record-0'],mode:'selected_only'}));f.refuseQuery();await assert.rejects(f.send({type:'requeryReferenceRefresh',id:id('f')}),/known_hash/);const last=f.view.session.turns.at(-1);assert.equal(last.input,'Refused reference refresh requery');assert.match(last.error,/known_hash/);assert.equal(last.nativeFailure.future_error.preserve,true);assert.equal(last.envelope,undefined);assert.deepEqual(f.turn.envelope,f.envelope);}finally{f.view.dispose();}
});

test('Stop while separate requery confirmation is open revokes retained flow before query dispatch',{timeout:5000},async()=>{
 const f=fixture();try{
    await assert.rejects(f.send({type:'refreshReferences',turn:'turn',cards:['record-0'],mode:'selected_only'}),/outcome_unknown/);
    const held=f.holdConfirmation(),pending=f.send({type:'requeryReferenceRefresh',id:id('f')});await held.entered;
    f.view.stop();held.release();await assert.rejects(pending,/selection_changed/);
    assert.equal(f.counts().queries,0);assert.equal(f.counts().submitted,1);assert.equal(f.control.referenceRefreshPending().length,1);
    assert.deepEqual(f.turn.envelope,f.envelope);
 }finally{f.view.dispose();}
});
