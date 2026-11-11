// SPDX-License-Identifier: Apache-2.0
import {test} from 'node:test';import assert from 'node:assert/strict';
import {LegacyRetirementSession,LegacyRetirementHost,retirementJson,retirementMarkdown,validateRetirementReply} from '../runtimeLegacyRetirement';
import {RecordValue} from '../serviceClient';
const id='idx_'+'1'.repeat(32),hash='2'.repeat(64);
function review():RecordValue{return {status:'review',review_digest:hash,preview:{schema:1,request_digest:hash,source_configuration:'pending_metadata',
 original_backup_digest:hash,original_legacy_data_digest:hash,effect:'retire_and_cancel_with_cleanup_pending',affected_work:{jobs:2,job_ids_digest:hash},
 request:{schema:1,index_id:id,caller:'1'.repeat(32),upgrade_id:'3'.repeat(32),plan_digest:hash,installer_identity:hash,expected_authorizations_digest:hash,
 key:{namespace:'4'.repeat(32),epoch:0,nonce:'5'.repeat(32)},registration:{binding:hash,root:'/fictional/source',data_root:'/fictional/private',collection:'a'},future:'retained'},future:{proof:'complete'}}};}
function accepted(r:RecordValue,status='authorized'):RecordValue {
 const authorization={preview:r.preview,review_digest:r.review_digest};
 return status==='authorized'?{status,authorization}:{status:'applied',receipt:{schema:1,authorization,catalog_revision:2,
 canceled_work:(r.preview as RecordValue).affected_work,post_effect_legacy_data_digest:hash,cleanup:'pending',future:{native:'complete'}}};
}
function fixture(){let r=review(),auth=0,reads=0,resumes=0,trusted=true,lost=false;let observed:RecordValue|null=null;
 const host:LegacyRetirementHost={legacyRetirementCandidates:async()=>({schema:1,upgrade_id:'3'.repeat(32),plan_digest:hash,installer_identity:hash,namespace:'4'.repeat(32),after:null,items:[],continuation:null,subset:false,total_eligible:0}),legacyRetirementReview:async()=>r,
 legacyRetirementAuthorize:async(_review,admit)=>{admit();auth++;if(lost)throw new Error('lost response');return observed=accepted(r);},
 legacyRetirementReceipt:async()=>{reads++;return observed;},legacyRetirementResume:async(_request,admit)=>{admit();resumes++;return accepted(r,'applied');}};
 const session=new LegacyRetirementSession(host,()=>trusted);
 return {session,r,host,get auth(){return auth;},get reads(){return reads;},get resumes(){return resumes;},lose(){lost=true;},
 untrust(){trusted=false;},observe(value:RecordValue|null){observed=value;}};
}
test('lost authorization response uses exact readonly receipt and never authorizes twice',async()=>{
 const f=fixture();await f.session.prepare(id);f.lose();await assert.rejects(f.session.authorize(f.r));
 await assert.rejects(f.session.authorize(f.r));assert.equal(await f.session.reconcile(),null);assert.equal(f.session.last?.status,'review');
 f.observe(accepted(f.r));assert.equal((await f.session.reconcile())?.status,'authorized');assert.equal(f.auth,1);assert.equal(f.reads,2);
});
test('crossed source, preview and review digest refusals preserve original complete observation',async()=>{
 const f=fixture();await f.session.prepare(id);f.lose();await assert.rejects(f.session.authorize(f.r));
 for(const change of ['request','preview','digest']){
  const crossed=structuredClone(f.r),p=crossed.preview as RecordValue;
  if(change==='request')(p.request as RecordValue).future='crossed';else if(change==='preview')p.future={proof:'crossed'};else crossed.review_digest='3'.repeat(64);
  f.observe(accepted(crossed));await assert.rejects(f.session.reconcile());
 }
 assert.deepEqual(f.session.last,f.r);
});
test('Stop and revoked trust across modal awaits cannot dispatch mutation',async()=>{
 const f=fixture();await f.session.prepare(id);f.untrust();await assert.rejects(f.session.authorize(f.r));assert.equal(f.auth,0);
 const g=fixture();const changed=structuredClone(g.r);await g.session.prepare(id);changed.review_digest='4'.repeat(64);
 await assert.rejects(g.session.authorize(changed));assert.equal(g.auth,0);
});
test('Stop during native review drops completion without dispatching authorization',async()=>{
 const f=fixture();let release!:(value:RecordValue)=>void;
 f.host.legacyRetirementReview=()=>new Promise(resolve=>{release=resolve;});
 const pending=f.session.prepare(id);f.session.stop();release(f.r);
 await assert.rejects(pending);assert.equal(f.auth,0);assert.equal(f.session.last,undefined);
});
test('logical Applied keeps cleanup pending and permits explicit same-candidate continuation',async()=>{
 const f=fixture();await f.session.prepare(id);await f.session.authorize(f.r);f.observe(accepted(f.r,'applied'));await f.session.reconcile();
 await f.session.resume();assert.equal(f.resumes,1);assert.equal(f.auth,1);
 assert.match(retirementJson(f.session.last!),/"future"/);assert.match(retirementMarkdown(f.session.last!),/"native": "complete"/);
 assert.match(retirementMarkdown(f.session.last!),/Physical cleanup remains pending/);
});
test('reload restores exact native accepted key for readonly reconciliation without new review or authorization',async()=>{
 const f=fixture(),saved=accepted(f.r,'applied');f.observe(saved);f.session.restore(saved);
 assert.deepEqual(await f.session.reconcile(),saved);assert.equal(f.auth,0);assert.equal(f.reads,1);
 await assert.rejects(f.session.authorize(f.r));assert.equal(f.auth,0);
});
test('candidate pages disclose subsets and reject changed native plan or looping continuation',async()=>{
 const f=fixture(),next='idx_'+'6'.repeat(32);let changed=false;
 f.host.legacyRetirementCandidates=async after=>({schema:1,upgrade_id:'3'.repeat(32),plan_digest:changed?'7'.repeat(64):hash,installer_identity:hash,namespace:'4'.repeat(32),
 after:after??null,items:[{index_id:after?next:id,registration:{root:'/fictional/source'},source_import:'pending_metadata',observation:null}],
 continuation:after?null:id,subset:true,total_eligible:2});
 const first=await f.session.candidates();assert.equal(first.continuation,id);assert.equal(first.subset,true);
 const second=await f.session.candidates(id);assert.equal(second.continuation,null);assert.equal(second.subset,true);
 changed=true;await assert.rejects(f.session.candidates(id),/page_changed/);assert.equal(f.auth,0);
});
test('arbitrary reply, malformed status and false physical completion refuse',()=>{
 assert.throws(()=>validateRetirementReply({status:'ready'}));const value=accepted(review(),'applied');
 (value.receipt as RecordValue).cleanup='complete';assert.throws(()=>validateRetirementReply(value));
 for(const epoch of [-1,'0',0.5,Number.MAX_SAFE_INTEGER+1]){
  const bad=review();const p=bad.preview as RecordValue;((p.request as RecordValue).key as RecordValue).epoch=epoch;
  assert.throws(()=>validateRetirementReply(bad));
 }
 validateRetirementReply(review()); // Canonical request epoch zero is valid.
});
