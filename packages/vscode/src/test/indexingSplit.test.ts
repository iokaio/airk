// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { CatalogObject, catalogExport, catalogObject } from '../indexingCatalog';
import { CatalogControl, IndexingCatalogController } from '../indexingCatalogController';
import { SPLIT_JOURNAL_BYTES, SPLIT_SNAPSHOT_BYTES, SPLIT_RECEIPT_BYTES, hasSplitCapability, normalizeSplitSelectors, splitExport, splitTerminal, validateSplitContinuity, validateSplitDraft, validateSplitRecover, validateSplitRecoveryReceipt, validateSplitSelection, validateSplitSnapshot, validateSplitSourceState } from '../indexingSplit';
import { IntentJournal, IntentStorage } from '../serviceIntents';
import { ServiceClient, ServiceError } from '../serviceClient';
import { splitAtStage, splitFixture, splitFrozenFixture, splitSourceResponse } from './fixtures/split';

test('split choices preserve exact-file algebra, explicit Project/capacity controls and complete native fields',()=>{
    const f=splitFixture();validateSplitSelection(f.choice);validateSplitDraft(f.draft);validateSplitSnapshot(f.snapshot,f.draft);
    const selection=normalizeSplitSelectors([{source_root_id:f.source,kind:'exact',path:'src/a.rs'},{source_root_id:f.source,kind:'subtree',path:'docs'},{source_root_id:f.source,kind:'exact',path:'docs/spec.md'}]);
    assert.equal(selection.length,2);assert.ok(selection.some(atom=>atom.kind==='exact'&&atom.path==='src/a.rs'));
    assert.throws(()=>validateSplitSelection({...f.choice,capacity:{...catalogObject(f.choice.capacity),destination_growth:Number.MAX_SAFE_INTEGER}}),/integer/);
    assert.throws(()=>validateSplitSelection({...f.choice,projects:[{project_id:'prj_'+'ab'.repeat(16),retain_original:false,membership:{destination:'omit'}}]}),/membership/);
    assert.equal(hasSplitCapability({policy_contract:2}),true);assert.equal(hasSplitCapability({version:'1.9.0'}),false);assert.equal(hasSplitCapability({policy_contract:1}),false);
    const view={generation:1,busy:false,splitStatus:{status:'catalog_split_status',snapshot:f.snapshot},submittedSplitReview:{draft:f.draft,response:{draft:f.draft,future_envelope:true},selection:f.choice}};
    assert.ok(catalogExport(view).includes('future_native_snapshot'));assert.ok(catalogExport(view,true).includes('future_envelope'));assert.ok(splitExport(f.snapshot,true).includes('future_native_snapshot'));
});

test('split stage and physical-funding proof failures are explicit rather than successful completion',()=>{
    const f=splitFixture(),complete=splitAtStage(f.snapshot,'complete');validateSplitSnapshot(complete);
    const premature=structuredClone(complete);catalogObject(premature.receipt).funding_release_receipts={};assert.throws(()=>validateSplitSnapshot(premature),/funding/);
    const early=splitAtStage(f.snapshot,'building');catalogObject(early.receipt).original_fence='aa'.repeat(32);assert.throws(()=>validateSplitSnapshot(early),/stage/);
    const crossed=structuredClone(f.snapshot);catalogObject(crossed.request).original_index=f.destinationIndex;assert.throws(()=>validateSplitSnapshot(crossed),/destination|plan/);
});

test('no-effect split cancellation acknowledgment binds reviewed digest and keeps post-fence restriction explicit',()=>{
    const f=splitFixture(),reviewed=splitAtStage(f.snapshot,'original_fenced'),old=catalogObject(reviewed.receipt);
    const intent=validateSplitRecover({reviewed,catalog_split_recover:{request:{caller:'a1'.repeat(16),key:{namespace:f.namespace,epoch:7,nonce:'a2'.repeat(16)},operation_id:f.operation,request_digest:old.request_digest,expected_receipt_digest:reviewed.receipt_digest,action:'cancel'}}});
    const acknowledgment={request:intent.catalog_split_recover.request,request_digest:'a3'.repeat(32),outcome:{outcome:'restrictive_completion_required',receipt_digest:reviewed.receipt_digest},future_ack:'retained'};
    assert.equal(validateSplitRecoveryReceipt(acknowledgment,intent),acknowledgment);
    assert.throws(()=>validateSplitRecoveryReceipt({...acknowledgment,outcome:{...acknowledgment.outcome,receipt_digest:'ff'.repeat(32)}},intent),/no_effect/);
});

test('pre-fence split cancellation remains canceled and can resume only exact destination cleanup',()=>{
    const f=splitFixture(),reviewed=splitAtStage(f.snapshot,'building'),old=catalogObject(reviewed.receipt),key={namespace:f.namespace,epoch:7,nonce:'c1'.repeat(16)},caller='c2'.repeat(16);
    const cancel=validateSplitRecover({reviewed,catalog_split_recover:{request:{caller,key,operation_id:f.operation,request_digest:old.request_digest,expected_receipt_digest:reviewed.receipt_digest,action:'cancel'}}});
    const next=structuredClone(old);next.revision=Number(old.revision)+1;next.previous_receipt_digest=reviewed.receipt_digest;next.state={state:'canceled_before_fence',cleanup_pending:true};
    const ack={request:cancel.catalog_split_recover.request,request_digest:'c3'.repeat(32),outcome:{outcome:'cancel_before_fence_accepted',receipt_digest:'c4'.repeat(32),cleanup_pending:true,resulting_receipt:next}};validateSplitRecoveryReceipt(ack,cancel);
    const canceled={...reviewed,receipt:next,receipt_digest:'c4'.repeat(32)},resume=validateSplitRecover({reviewed:canceled,catalog_split_recover:{request:{caller,key:{...key,nonce:'c5'.repeat(16)},operation_id:f.operation,request_digest:old.request_digest,expected_receipt_digest:canceled.receipt_digest,action:'resume'}}});
    const cleaned=structuredClone(next);cleaned.revision=Number(next.revision)+1;cleaned.previous_receipt_digest=canceled.receipt_digest;cleaned.jobs=[{purpose:'destination_cleanup',caller,key:resume.catalog_split_recover.request.key,job:'c6'.repeat(32),backing:catalogObject(catalogObject(f.draft.request).destination_registration).binding,engine_identity:'c7'.repeat(32),configuration_digest:'c8'.repeat(32)}];
    const resumed={request:resume.catalog_split_recover.request,request_digest:'c9'.repeat(32),outcome:{outcome:'resume_accepted',accepted_job:'c6'.repeat(32),receipt_digest:'ca'.repeat(32),resulting_receipt:cleaned}};validateSplitRecoveryReceipt(resumed,resume);
    catalogObject((cleaned.jobs as unknown[])[0]).purpose='destination_build';assert.throws(()=>validateSplitRecoveryReceipt(resumed,resume),/canceled_split/);
});

test('split journal reserves worst terminal growth before dispatch and never replays after restart',async()=>{
    const values=new Map<string,unknown>(),storage:IntentStorage={keys:()=>[...values.keys()],get:<T>(key:string)=>values.get(key) as T|undefined,update:async(key,value)=>{if(value===undefined)values.delete(key);else values.set(key,structuredClone(value));}};
    const f=splitFixture(),intent={catalog_split_apply:{draft:f.draft}},journal=new IntentJournal(storage,'/control',f.namespace);
    const id=await journal.prepare('Reviewed split',intent);assert.equal(journal.list()[0].value.reservedObservationBytes,SPLIT_JOURNAL_BYTES);
    let sends=0;const client=new ServiceClient({dispose(){},async request(){sends++;throw new ServiceError('must_not_submit');}});
    const reopened=new IntentJournal(storage,'/control',f.namespace);await assert.rejects(reopened.submit(client,id),/split_requires_exact_native_receipt_reconciliation/);assert.equal(sends,0);
    const response={status:'catalog_split_receipt',caller:catalogObject(f.draft.request).caller,key:catalogObject(f.draft.request).key,snapshot:{...f.snapshot,future_blob:'a'.repeat(20000)},expired:false,future_envelope:'retained'};
    await reopened.observeCatalog(id,response);assert.equal(reopened.list()[0].value.nativeObservation?.future_envelope,'retained');
    for(let at=1;at<10;at++){const next=structuredClone(intent);catalogObject(catalogObject(next.catalog_split_apply.draft).request).key={namespace:f.namespace,epoch:7,nonce:at.toString(16).padStart(32,'0')};await journal.prepare('Another split',next);}
    const last=structuredClone(intent);catalogObject(catalogObject(last.catalog_split_apply.draft).request).key={namespace:f.namespace,epoch:7,nonce:'fe'.repeat(16)};await assert.rejects(journal.prepare('Over funded local tracking capacity',last),/byte_capacity/);
    const ordinary=(at:number)=>({catalog_apply:{request:{caller:'ba'.repeat(16),key:{namespace:f.namespace,epoch:7,nonce:at.toString(16).padStart(32,'0')},expected_catalog_revision:1,operation:{operation:'set_membership',expected_scope_revision:1,expected_membership_revision:null,membership:{project_id:'prj_'+'bc'.repeat(16),index_id:f.index,revision:1,scope:Array.from({length:10},(_,index)=>({source_root_id:f.source,kind:'subtree',path:`${index}-${'a'.repeat(3900)}`}))}}},preview_digest:'bd'.repeat(32)},request_digest:'be'.repeat(32)});
    for(const at of [100,101,102])await journal.prepare('Ordinary native membership',ordinary(at));
    await assert.rejects(journal.prepare('Mixed aggregate exceeds shared limit',ordinary(103)),/byte_capacity/);
    const ordinaryRow=journal.list().find(row=>row.id===(100).toString(16).padStart(32,'0'))!;assert.equal(ordinaryRow.value.reservedObservationBytes,undefined);
    await assert.rejects(journal.observeCatalog(ordinaryRow.id,{status:'catalog_receipt',future:'a'.repeat(65536)}),/record_oversize/);
    assert.equal(sends,0);
});

test('capability-qualified Recommendation link opens native detail without preparing or applying a split',async()=>{
    const f=splitFixture();let reads=0,drafts=0,applies=0,contract=1;
    const item={kind:'index',value:{id:f.index,name:'Original',definition_revision:1,source_revision:1,tombstone:false,backing:catalogObject(f.draft.request).original_registration}};
    const control:CatalogControl={catalogRead:async method=>{reads++;return 'catalog_page'in method?{status:'catalog_page',page:{namespace:f.namespace,revision:1,items:[item],continuation:null},capabilities:{policy_contract:contract,future_native_capability:true}}:{status:'catalog_detail',detail:{namespace:f.namespace,revision:1,item},capabilities:{policy_contract:contract,future_native_capability:true}};},catalogPrepare:async()=>{throw new Error('must not create a catalog mutation');},catalogApply:async()=>{throw new Error('must not apply a catalog mutation');},catalogReconcile:async()=>[],catalogPending:()=>[],catalogSplitDraft:async()=>{drafts++;return {response:{draft:f.draft},draft:f.draft,selection:f.choice};},catalogSplitSourceState:async()=>splitSourceResponse(f),catalogSplitApply:async()=>{applies++;return {};}};
    const controller=new IndexingCatalogController({control,identity:()=> 'stable',authorized:()=>true,changed(){}});
    await controller.page('indexes');const advice={schema:'airk.recommendations/1',selected:{index_ids:[f.index],project_ids:[]},cards:[{action:'review_split'}],future_advice:'retained'};
    await controller.recommendations(async()=>advice);assert.equal(controller.snapshot().canReviewSplit,false);const unsupportedReads=reads;await assert.rejects(controller.splitOpen(f.index,controller.snapshot().generation),/capability/);assert.equal(reads,unsupportedReads);assert.equal(drafts,0);assert.equal(applies,0);
    contract=2;await controller.page('indexes');await controller.recommendations(async()=>advice);const before=reads;await controller.splitOpen(f.index,controller.snapshot().generation);
    assert.equal(reads,before+1);assert.equal(drafts,0);assert.equal(applies,0);assert.equal(controller.snapshot().splitFormOpen,true);assert.equal(controller.snapshot().capabilities?.future_native_capability,true);assert.ok(catalogExport(controller.snapshot(),true).includes('future_advice'));
    assert.ok(controller.snapshot().canReviewSplit);const qualifiedReads=reads;await assert.rejects(controller.splitOpen(f.destinationIndex,controller.snapshot().generation),/advised_index/);assert.equal(reads,qualifiedReads);assert.equal(drafts,0);assert.equal(applies,0);
});

function listedSplitController(){
    const f=splitFixture(),item={kind:'index',value:{id:f.index,name:'Original',definition_revision:1,source_revision:1,tombstone:false,backing:catalogObject(f.draft.request).original_registration}};
    let detailItem:CatalogObject=item,detailRevision=1,reads=0,mutations=0;
    const control:CatalogControl={
        catalogRead:async method=>{reads++;return 'catalog_page' in method?{status:'catalog_page',page:{namespace:f.namespace,revision:1,items:[item],continuation:null},capabilities:{policy_contract:2}}:{status:'catalog_detail',detail:{namespace:f.namespace,revision:detailRevision,item:detailItem},capabilities:{policy_contract:2}};},
        catalogPrepare:async()=>{mutations++;throw new Error('unexpected catalog mutation');},catalogApply:async()=>{mutations++;throw new Error('unexpected catalog mutation');},
        catalogReconcile:async()=>[],catalogPending:()=>[],catalogSplitDraft:async()=>({response:{draft:f.draft},draft:f.draft,selection:f.choice}),
        catalogSplitSourceState:async()=>splitSourceResponse(f),catalogSplitApply:async()=>{mutations++;throw new Error('unexpected split mutation');},
    };
    const controller=new IndexingCatalogController({control,identity:()=> 'stable',authorized:()=>true,changed(){}});
    return {f,item,control,controller,reads:()=>reads,mutations:()=>mutations,setDetail:(next:CatalogObject,revision=1)=>{detailItem=next;detailRevision=revision;}};
}

test('a live listed index opens split review directly without Recommendations or any mutation',async()=>{
    const f=listedSplitController();await f.controller.page('indexes');
    assert.equal(f.controller.snapshot().pageKind,'indexes');assert.equal(f.controller.snapshot().recommendations,undefined);
    const before=f.reads();await f.controller.splitOpen(f.f.index,f.controller.snapshot().generation);
    assert.equal(f.reads(),before+1);assert.equal(f.controller.snapshot().splitFormOpen,true);assert.equal(f.mutations(),0);
    assert.equal(catalogObject(catalogObject(f.controller.snapshot().detail!.detail).item).kind,'index');
});

test('listed split shortcuts retain subset, live-detail, identity and revision checks',async()=>{
    const outside=listedSplitController();await outside.controller.page('indexes');const before=outside.reads();
    await assert.rejects(outside.controller.splitOpen(outside.f.destinationIndex,outside.controller.snapshot().generation),/listed_or_advised_index/);
    assert.equal(outside.reads(),before,'an index outside the displayed subset is not fetched');
    const retired=listedSplitController();retired.item.value.tombstone=true;await retired.controller.page('indexes');
    await assert.rejects(retired.controller.splitOpen(retired.f.index,retired.controller.snapshot().generation),/listed_or_advised_index/);
    assert.equal(retired.reads(),1);
    for(const failure of ['retired','crossed','revision'] as const){
        const f=listedSplitController();await f.controller.page('indexes');
        const next=structuredClone(f.item);
        if(failure==='retired')next.value.tombstone=true;
        if(failure==='crossed')next.value.id=f.f.destinationIndex;
        f.setDetail(next,failure==='revision'?2:1);
        await assert.rejects(f.controller.splitOpen(f.f.index,f.controller.snapshot().generation),failure==='retired'?/requires_live_index/:failure==='crossed'?/crossed_split_index/:/revision_changed/);
        assert.equal(f.controller.snapshot().splitFormOpen,undefined);assert.equal(f.mutations(),0);
    }
});

test('new catalog pages or inspected details discard the previous split selection and draft',async()=>{
    for(const action of ['page','detail'] as const){
        const f=listedSplitController();await f.controller.page('indexes');await f.controller.splitOpen(f.f.index,f.controller.snapshot().generation);
        await f.controller.splitSourceRead(f.controller.snapshot().generation);await f.controller.splitDraft(f.f.choice,f.controller.snapshot().generation);
        assert.ok(f.controller.snapshot().splitSourceState);assert.ok(f.controller.snapshot().splitReview);
        if(action==='page')await f.controller.page('indexes');else await f.controller.detail('index',f.f.index);
        assert.equal(f.controller.snapshot().splitFormOpen,undefined);assert.equal(f.controller.snapshot().splitSourceState,undefined);assert.equal(f.controller.snapshot().splitReview,undefined);
        assert.equal(f.mutations(),0);
    }
});

test('connected native context hydrates split capability automatically and context reads retain it',async()=>{
    const f=splitFixture(),descriptor={host:'22'.repeat(32),key:'33'.repeat(32),kind:'folder' as const,name:'AIRK'};
    const capabilities={policy_contract:3,future_native_capability:{retained:true}};
    let identity='installed-runtime',observation:CatalogObject|undefined={status:'catalog_host_context',capabilities,descriptor:{schema:1,namespace:f.namespace,host:descriptor.host,key:descriptor.key,catalog_revision:1,context:null}};
    let reads=0;
    const control:CatalogControl={catalogRead:async()=>{reads++;return {status:'catalog_detail',capabilities,detail:{namespace:f.namespace,revision:1,item:null}};},
        catalogPrepare:async()=>{throw new Error('no automatic mutation');},catalogApply:async()=>{throw new Error('no automatic mutation');},catalogReconcile:async()=>[],catalogPending:()=>[],
        catalogSplitDraft:async()=>{throw new Error('no automatic split');},catalogSplitApply:async()=>{throw new Error('no automatic split');},catalogSplitSourceState:async()=>splitSourceResponse(f)};
    const controller=new IndexingCatalogController({control,identity:()=>identity,authorized:()=>true,changed(){},windowContext:()=>descriptor,windowContextObservation:()=>observation});
    assert.equal(controller.snapshot().canReviewSplit,true,'connected observation needs no library refresh or migration');
    assert.equal(reads,0);assert.deepEqual(controller.snapshot().capabilities,capabilities);
    await controller.adoptHostContext();assert.equal(controller.snapshot().canReviewSplit,true);
    assert.deepEqual(JSON.parse(catalogExport(controller.snapshot())).hostContext,observation);
    await controller.readContext();assert.equal(reads,1);assert.equal(controller.snapshot().canReviewSplit,true);assert.deepEqual(controller.snapshot().capabilities,capabilities);
    observation=undefined;identity='replacement-runtime';assert.equal(controller.snapshot().canReviewSplit,false,'new connection must not inherit capability');
    observation={status:'catalog_host_context',descriptor:{schema:1,namespace:f.namespace,host:descriptor.host,key:descriptor.key,catalog_revision:1,context:null}};
    assert.equal(controller.snapshot().canReviewSplit,false,'a version or missing metadata cannot imply capability');
});

test('split receipt reads retry typed contention but split mutations dispatch only once',async()=>{
    const f=splitFixture();let reads=0,writes=0;
    const client=new ServiceClient({dispose(){},async request(input){const method=catalogObject(input);if('catalog_split_receipt'in method){reads++;if(reads<3)return {status:'failure',code:'registry_busy'};return {status:'catalog_split_receipt',caller:catalogObject(f.draft.request).caller,key:catalogObject(f.draft.request).key,snapshot:f.snapshot,expired:false};}writes++;return {status:'failure',code:'invalid_registry_request'};}});
    await client.catalogSplitReceipt({catalog_split_apply:{draft:f.draft}});assert.equal(reads,3);
    await assert.rejects(client.submitSplit({catalog_split_apply:{draft:f.draft}}),(error:unknown)=>error instanceof ServiceError&&error.outcomeUnknown);assert.equal(writes,1);
});


test('split source baseline uses the complete native Project census and rejects crossed or stale reads',async()=>{
    const f=splitFixture(),response=splitSourceResponse(f),state=catalogObject(response.state);
    state.affected_projects=[{project_id:'prj_'+'ab'.repeat(16),expected_scope_revision:2,expected_original_membership:3,retain_original:true,destination_scope:null,future_effect:'retained'}];
    assert.equal(validateSplitSourceState(response,f.index,1),response);
    assert.ok(splitExport(response,true).includes('future_effect'));
    assert.throws(()=>validateSplitSourceState(response,f.destinationIndex,1),/crossed/);
    assert.throws(()=>validateSplitSourceState(response,f.index,2),/crossed/);
    state.affected_projects=[...state.affected_projects as unknown[],...(state.affected_projects as unknown[])];
    assert.throws(()=>validateSplitSourceState(response,f.index,1),/duplicate/);
    let release!:(value:ReturnType<typeof catalogObject>)=>void,authorized=true;
    const client=new ServiceClient({dispose(){},request:async()=>new Promise(resolve=>{release=resolve;})});
    const pending=client.catalogSplitSourceState(f.index,1);release(splitSourceResponse(f));assert.deepEqual(await pending,splitSourceResponse(f));
    const item={kind:'index',value:{id:f.index,name:'Original',definition_revision:1,source_revision:1,tombstone:false,backing:catalogObject(f.draft.request).original_registration}};
    const control:CatalogControl={catalogRead:async()=>({status:'catalog_detail',detail:{namespace:f.namespace,revision:1,item},capabilities:{policy_contract:2}}),catalogPrepare:async()=>{throw new Error('no mutation');},catalogApply:async()=>{throw new Error('no mutation');},catalogReconcile:async()=>[],catalogPending:()=>[],catalogSplitSourceState:async()=>new Promise(resolve=>{release=resolve;})};
    const controller=new IndexingCatalogController({control,identity:()=> 'stable',authorized:()=>authorized,changed(){}});
    await controller.detail('index',f.index);const read=controller.splitSourceRead(controller.snapshot().generation);authorized=false;release(splitSourceResponse(f));await assert.rejects(read,/stale_catalog_view/);
    assert.equal(controller.snapshot().splitSourceState,undefined);
});


test('metadata-only split continuation preserves exact phase, jobs, full native proof and export data',()=>{
    const f=splitFixture(),reviewed=structuredClone(f.snapshot);catalogObject(reviewed.receipt).state={state:'needs_attention',code:'metadata_io'};
    const old=catalogObject(reviewed.receipt),intent=validateSplitRecover({reviewed,catalog_split_recover:{request:{caller:'d1'.repeat(16),key:{namespace:f.namespace,epoch:7,nonce:'d2'.repeat(16)},operation_id:f.operation,request_digest:old.request_digest,expected_receipt_digest:reviewed.receipt_digest,action:'resume'}}});
    const next=structuredClone(old);next.revision=Number(old.revision)+1;next.previous_receipt_digest=reviewed.receipt_digest;next.state={state:'pending'};
    const ack={request:intent.catalog_split_recover.request,request_digest:'d3'.repeat(32),outcome:{outcome:'continuation_accepted',receipt_digest:'d4'.repeat(32),resulting_receipt:next,future_metadata_continuation:'retained'},future_ack:true};
    assert.equal(validateSplitRecoveryReceipt(ack,intent),ack);assert.ok(splitExport(ack,true).includes('future_metadata_continuation'));
    const cancel=structuredClone(intent);cancel.catalog_split_recover.request.action='cancel';const wrongAction={...ack,request:cancel.catalog_split_recover.request};assert.throws(()=>validateSplitRecoveryReceipt(wrongAction,cancel),/continuation/);
    const advanced=structuredClone(ack);catalogObject(advanced.outcome.resulting_receipt).stage='destination_prepared';assert.throws(()=>validateSplitRecoveryReceipt(advanced,intent));
    const running=structuredClone(ack);catalogObject(running.outcome.resulting_receipt).state={state:'running'};assert.throws(()=>validateSplitRecoveryReceipt(running,intent),/continuation/);
    const crossed=structuredClone(ack);catalogObject(crossed.outcome.resulting_receipt).previous_receipt_digest='ff'.repeat(32);assert.throws(()=>validateSplitRecoveryReceipt(crossed,intent),/successor/);
});


test('frozen finalization acknowledgment binds the reviewed envelope and terminal bytes independently of later latches',()=>{
    const f=splitFrozenFixture(),intent=validateSplitRecover(f.intent);validateSplitSnapshot(f.reviewed);validateSplitRecoveryReceipt(f.acknowledgment,intent);
    assert.equal(splitTerminal(f.reviewed),false);assert.ok(splitExport(f.acknowledgment,true).includes('future_frozen_ack'));
    const later=structuredClone(f.reviewed);catalogObject(later.finalization).revision=3;catalogObject(later.finalization).attention='later_failure';validateSplitContinuity(f.reviewed,later);
    const stale=structuredClone(f.intent);stale.catalog_split_recover.request.expected_finalization_revision=2;assert.throws(()=>validateSplitRecover(stale),/finalization_review/);
    const crossed=structuredClone(f.acknowledgment);catalogObject(crossed.outcome).terminal_receipt_digest='ff'.repeat(32);assert.throws(()=>validateSplitRecoveryReceipt(crossed,intent),/finalization_ack/);
    const mutated=structuredClone(f.acknowledgment);catalogObject(catalogObject(mutated.outcome).terminal_receipt).original_fence='ff'.repeat(32);assert.throws(()=>validateSplitRecoveryReceipt(mutated,intent),/terminal|successor|proof/);
    const terminal=structuredClone(f.terminal);terminal.finalization={...catalogObject(f.reviewed.finalization),revision:4,progress:'acknowledging',attention:'funding_ack_failed'};validateSplitSnapshot(terminal);assert.equal(splitTerminal(terminal),false);
    catalogObject(terminal.finalization).progress='complete';assert.throws(()=>validateSplitSnapshot(terminal),/finalization/);
    catalogObject(terminal.finalization).funding_acknowledged=Object.values(catalogObject(catalogObject(terminal.receipt).funding_release_receipts));catalogObject(terminal.finalization).attention=null;validateSplitSnapshot(terminal);assert.equal(splitTerminal(terminal),true);
    const regressed=structuredClone(later);catalogObject(regressed.finalization).revision=1;assert.throws(()=>validateSplitContinuity(later,regressed),/finalization/);
    assert.throws(()=>validateSplitSnapshot({...f.reviewed,future_blob:'x'.repeat(SPLIT_SNAPSHOT_BYTES)}),/bound|oversize|budget/);
});

test('canceled cleanup continuation clears only its latch and exact completed cleanup supports terminal acknowledgment',()=>{
    const f=splitFixture(),reviewed=structuredClone(f.snapshot),receipt=catalogObject(reviewed.receipt);receipt.state={state:'canceled_before_fence',cleanup_pending:true};receipt.cleanup_attention='cleanup_io';receipt.future_immutable_proof={retained:true};
    const intent=validateSplitRecover({reviewed,catalog_split_recover:{request:{caller:'e1'.repeat(16),key:{namespace:f.namespace,epoch:7,nonce:'e2'.repeat(16)},operation_id:f.operation,request_digest:receipt.request_digest,expected_receipt_digest:reviewed.receipt_digest,action:'resume'}}});
    const next={...receipt,revision:2,previous_receipt_digest:reviewed.receipt_digest,cleanup_attention:null};
    const ack={request:intent.catalog_split_recover.request,request_digest:'e3'.repeat(32),outcome:{outcome:'continuation_accepted',receipt_digest:'e4'.repeat(32),resulting_receipt:next}};validateSplitRecoveryReceipt(ack,intent);
    const mutated=structuredClone(ack);catalogObject(mutated.outcome.resulting_receipt).future_immutable_proof={retained:false};assert.throws(()=>validateSplitRecoveryReceipt(mutated,intent),/continuation/);
    const completed=structuredClone(reviewed),done=catalogObject(completed.receipt);done.revision=2;done.previous_receipt_digest=reviewed.receipt_digest;done.state={state:'canceled_before_fence',cleanup_pending:false};done.cleanup_attention=null;done.physical_cleanup_authorization='e5'.repeat(32);done.cancellation_cleanup_receipt='e6'.repeat(32);completed.receipt_digest='e7'.repeat(32);
    const terminalIntent=validateSplitRecover({...intent,reviewed:completed,catalog_split_recover:{request:{...intent.catalog_split_recover.request,expected_receipt_digest:completed.receipt_digest}}});validateSplitRecoveryReceipt({request:terminalIntent.catalog_split_recover.request,request_digest:'e8'.repeat(32),outcome:{outcome:'already_complete',receipt_digest:completed.receipt_digest}},terminalIntent);
});

test('original witness jobs and immutable source-verification proof retain original backing and exact provenance',()=>{
    const f=splitFixture(),reviewed=splitAtStage(f.snapshot,'destination_validated'),old=catalogObject(reviewed.receipt),caller='f1'.repeat(16),key={namespace:f.namespace,epoch:7,nonce:'f2'.repeat(16)};
    const intent=validateSplitRecover({reviewed,catalog_split_recover:{request:{caller,key,operation_id:f.operation,request_digest:old.request_digest,expected_receipt_digest:reviewed.receipt_digest,action:'resume'}}});
    const next=structuredClone(old);next.revision=3;next.previous_receipt_digest=reviewed.receipt_digest;next.jobs=[{purpose:'original_witness',caller,key,job:'f3'.repeat(32),backing:catalogObject(catalogObject(reviewed.request).original_registration).binding,engine_identity:'f4'.repeat(32),configuration_digest:'f5'.repeat(32)}];
    const ack={request:intent.catalog_split_recover.request,request_digest:'f6'.repeat(32),outcome:{outcome:'resume_accepted',accepted_job:'f3'.repeat(32),receipt_digest:'f7'.repeat(32),resulting_receipt:next}};validateSplitRecoveryReceipt(ack,intent);
    const fenced=splitAtStage(reviewed,'original_fenced',3);validateSplitSnapshot(fenced);const changed=structuredClone(fenced);catalogObject(catalogObject(changed.receipt).remainder_witness).expected_hash='ff'.repeat(32);catalogObject(changed.receipt).revision=4;assert.throws(()=>validateSplitContinuity(fenced,changed),/proof/);
    const wrong=structuredClone(fenced);catalogObject(catalogObject(wrong.receipt).remainder_witness).before_policy_digest='ff'.repeat(32);assert.throws(()=>validateSplitSnapshot(wrong),/witness/);
});


test('individual native split receipt uses the exact 14 KiB budget while the recovery acknowledgment retains 16 KiB',()=>{
    const f=splitFixture(),snapshot=structuredClone(f.snapshot),receipt=catalogObject(snapshot.receipt);receipt.future_padding='';
    const bytes=(value:unknown)=>new TextEncoder().encode(JSON.stringify(value)).length;
    receipt.future_padding='x'.repeat(SPLIT_RECEIPT_BYTES-bytes(receipt));assert.equal(bytes(receipt),14336);validateSplitSnapshot(snapshot);
    receipt.future_padding=String(receipt.future_padding)+'x';assert.throws(()=>validateSplitSnapshot(snapshot),/bound|oversize|budget/);
    const reviewed=splitAtStage(f.snapshot,'complete'),old=catalogObject(reviewed.receipt),intent=validateSplitRecover({reviewed,catalog_split_recover:{request:{caller:'a1'.repeat(16),key:{namespace:f.namespace,epoch:7,nonce:'a2'.repeat(16)},operation_id:f.operation,request_digest:old.request_digest,expected_receipt_digest:reviewed.receipt_digest,action:'resume'}}});
    const ack={request:intent.catalog_split_recover.request,request_digest:'a3'.repeat(32),outcome:{outcome:'already_complete',receipt_digest:reviewed.receipt_digest},future_padding:''};ack.future_padding='y'.repeat(16384-bytes(ack));assert.equal(bytes(ack),16384);validateSplitRecoveryReceipt(ack,intent);
    ack.future_padding+='y';assert.throws(()=>validateSplitRecoveryReceipt(ack,intent),/bound|oversize|budget/);
});

test('pre-fence retained witness refresh accepts completed current job across skipped observations and rejects crossed lineage',()=>{
    const f=splitFixture(),before=splitAtStage(f.snapshot,'destination_validated',4),receipt=catalogObject(before.receipt);
    const proof=catalogObject(catalogObject(splitAtStage(f.snapshot,'original_fenced')).receipt).remainder_witness as Record<string,unknown>;
    const job=(id:string,purpose='original_witness')=>({purpose,caller:'a1'.repeat(16),key:{namespace:f.namespace,epoch:7,nonce:id.repeat(16)},job:id.repeat(32),backing:catalogObject(catalogObject(before.request).original_registration).binding,engine_identity:'a2'.repeat(32),configuration_digest:'a3'.repeat(32)});
    receipt.jobs=[job('b1'),job('b2')];receipt.remainder_witness={...proof,job:'b1'.repeat(32)};
    const refreshed=structuredClone(before),next=catalogObject(refreshed.receipt);next.revision=5;next.previous_receipt_digest=before.receipt_digest;refreshed.receipt_digest='b3'.repeat(32);next.remainder_witness={...proof,job:'b2'.repeat(32),expected_hash:'b4'.repeat(32),observed_from_unix_ms:3,observed_to_unix_ms:4};
    // The fresh job was already pending/running in the last observation; no new history entry is required here.
    validateSplitContinuity(before,refreshed);
    const enqueueSkipped=structuredClone(before);catalogObject(enqueueSkipped.receipt).jobs=[job('b1')];validateSplitContinuity(enqueueSkipped,refreshed);
    const fenceSkipped=splitAtStage(refreshed,'original_fenced',6);catalogObject(fenceSkipped.receipt).remainder_witness=structuredClone(next.remainder_witness);validateSplitContinuity(before,fenceSkipped);
    for(const id of ['b1','ff']){const crossed=structuredClone(refreshed);catalogObject(catalogObject(crossed.receipt).remainder_witness).job=id.repeat(32);assert.throws(()=>validateSplitContinuity(before,crossed),/proof/);}
    const stale=structuredClone(before);catalogObject(stale.receipt).remainder_witness={...proof,job:'b2'.repeat(32)};const backwards=structuredClone(refreshed);catalogObject(catalogObject(backwards.receipt).remainder_witness).job='b1'.repeat(32);assert.throws(()=>validateSplitContinuity(stale,backwards),/proof/);
    const nonWitness=structuredClone(refreshed);catalogObject(nonWitness.receipt).jobs=[job('b1'),job('b2'),job('b5','original_cleanup')];catalogObject(catalogObject(nonWitness.receipt).remainder_witness).job='b5'.repeat(32);assert.throws(()=>validateSplitContinuity(before,nonWitness),/proof/);
    const crossedSource=structuredClone(refreshed);catalogObject(catalogObject(crossedSource.receipt).remainder_witness).before_policy_digest='ff'.repeat(32);assert.throws(()=>validateSplitContinuity(before,crossedSource),/witness/);
    const afterFence=splitAtStage(before,'original_fenced',5);catalogObject(afterFence.receipt).remainder_witness=structuredClone(receipt.remainder_witness);const illegal=splitAtStage(refreshed,'original_fenced',6);catalogObject(illegal.receipt).remainder_witness=structuredClone(next.remainder_witness);assert.throws(()=>validateSplitContinuity(afterFence,illegal),/proof/);
});

test('split retains compact native policy inheritance and refuses contract/body-budget/selection mismatches',()=>{
    const f=splitFixture(),snapshot=structuredClone(f.snapshot),request=catalogObject(snapshot.request);
    request.required_policy_contract=3;request.extraction_policy={schema:1,policy_body_digest:'c1'.repeat(32),configured_policy_digest:'c2'.repeat(32),encoded_bytes:131072,retained_profile:'lean',legacy_fallback_detail:'agent_lean',required_extraction_contract:3};
    validateSplitSnapshot(snapshot);assert.ok(splitExport(snapshot,true).includes('configured_policy_digest'));
    const old=structuredClone(snapshot);catalogObject(old.request).required_policy_contract=2;assert.throws(()=>validateSplitSnapshot(old),/contract/);
    const oversized=structuredClone(snapshot);catalogObject(catalogObject(oversized.request).extraction_policy).encoded_bytes=131073;assert.throws(()=>validateSplitSnapshot(oversized),/reference/);
    const aliased=structuredClone(snapshot);catalogObject(catalogObject(aliased.request).extraction_policy).retained_profile='balanced';assert.throws(()=>validateSplitSnapshot(aliased),/reference/);
    const changed=structuredClone(snapshot);catalogObject(catalogObject(changed.request).extraction_policy).policy_body_digest='c3'.repeat(32);assert.throws(()=>validateSplitContinuity(snapshot,changed),/identity/);
});
