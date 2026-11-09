// SPDX-License-Identifier: Apache-2.0
import {strict as assert} from 'node:assert';
import {test} from 'node:test';
import type {ConsoleIntent,SavedIntent} from '../serviceIntents';
import {validatePolicyResumeIntent,validateSourceResolveIntent} from '../indexingCatalog';
import {validateSplitApply,validateSplitRecover} from '../indexingSplit';
import {validateRefreshIntent,refreshDigest,refreshJobId,refreshSelectionIdentity} from '../referenceRefresh';
import {splitFixture,splitAtStage,splitFrozenFixture} from './fixtures/split';
import {acceptedIntentSlots,selectAcceptedIntentEvidence,mergeAcceptedIntentEvidence,withoutAcceptedIntentEvidence,
    acceptedIntentEvidenceDominates,MAX_ACCEPTED_INTENT_SLOTS,MAX_UNRESERVED_ACCEPTED_INTENT_SLOTS} from '../intentAcceptedEvidence';

type Row=Record<string,unknown>;
const ns='11'.repeat(16),caller='22'.repeat(16),digest='33'.repeat(32),nonce='44'.repeat(16),operation='55'.repeat(16);
const key={namespace:ns,epoch:1,nonce},index='idx_'+'66'.repeat(16),source='src_'+'77'.repeat(16);
const saved=(intent:ConsoleIntent,extra:Partial<SavedIntent>={}):SavedIntent=>({schema:1,scope:digest,title:'Reviewed request',intent,...extra});
function fence(){
    const request={caller,key,expected_catalog_revision:1,operation:{operation:'rename_project',id:'prj_'+'88'.repeat(16),expected_definition_revision:1,name:'Reviewed'}};
    const intent={catalog_apply:{request,preview_digest:digest},request_digest:digest} as ConsoleIntent;
    const metadata={schema:1,operation_id:operation,caller,key,request_digest:digest,catalog_revision:2,authority_fence_required:true};
    const common={schema:1,operation_id:operation,request,preview_digest:digest,prepared_digest:digest};
    const pending={status:'catalog_receipt',receipt:null,fence:{state:'pending',value:{...common,stage:'prepared',plan:{service:ns,gate_generation:1,targets:[]},confirmations:[]}}};
    const applied={status:'catalog_applied',receipt:metadata,fenced_receipt:{...common,metadata,proof:{plan_digest:digest,service:ns,gate_generation:1,confirmations:[]}},future_native:{all:['retained']}};
    return {intent,pending,applied,metadata};
}
function setup(){
    const request={caller,key,expected_catalog_revision:1,name:'Explicit',registration:{binding:digest,root:'/source',data_root:'/data',collection:'fixture'},sources:[{canonical_path:'/source',aliases:[],owner:true}],setup_watch:false};
    const draft={schema:1,request,storage_action:'create_missing_leaf',review_digest:digest};
    const intent={catalog_index_setup_apply:{draft},request_digest:digest} as ConsoleIntent;
    const reserved={schema:1,operation_id:operation,request,request_digest:digest,index_id:index,source_ids:[source],stage:'reserved',catalog_revision:2,source_revision:1,preparation:null,completion:null,future_native:'retained'};
    const prepared={...reserved,stage:'prepared',preparation:{digest,storage_identity:digest,source_configuration_digest:digest,review:{expected_catalog_revision:2,expected_source_revision:1}}};
    const ready={...prepared,stage:'ready',catalog_revision:3,source_revision:2,completion:{prepared_digest:digest,expected_catalog_revision:2,expected_source_revision:1}};
    return {intent,reply:(receipt:unknown)=>({status:'catalog_index_setup_receipt',receipt}),reserved,prepared,ready};
}
function sources(){
    const change={operation:'begin_index_source_change',id:index,expected_source_revision:1,plan:{registration:{binding:digest,root:'/source',data_root:'/data',collection:'fixture'},engine_identity:digest,job:digest,
        configuration:{incarnation:ns,settings:digest,configuration:{operation:'add_root',alias:'extra',path:'/extra'}},before:{primary_root:'/source',aliases:{}},after:{primary_root:'/source',aliases:{extra:'/extra'}},
        sources:[{id:source,canonical_path:'/source',aliases:[],owner:true},{id:'src_'+'99'.repeat(16),canonical_path:'/extra',aliases:['extra'],owner:false}]}};
    const request={caller,key,expected_catalog_revision:1,operation:change};
    const metadata={schema:1,operation_id:operation,caller,key,request_digest:digest,catalog_revision:2,authority_fence_required:true};
    const begin={request,metadata,preview_digest:digest,prepared_digest:digest,proof:{plan_digest:digest,service:ns,gate_generation:1,confirmations:[]}};
    const intent=validateSourceResolveIntent({catalog_source_change_resolve:{begin_operation:operation},namespace:ns});
    const observation={begin,job:{id:digest,project:digest,kind:'configure',state:'running',fencing_generation:1,attempts:1},completion:null as Row|null,future_native:'retained'};
    const pending={status:'catalog_source_change_observation',observation};
    const complete={status:'catalog_source_change_resolved',observation:{...observation,completion:{schema:1,operation_id:'aa'.repeat(16),caller,key:{...key,nonce:'bb'.repeat(16)},request_digest:digest,catalog_revision:3,authority_fence_required:false}}};
    return {intent,pending,complete};
}
function refresh(){
    const f=splitFixture(),before=`airk-r1:${ns}:1:${digest}`,after=`airk-r1:${ns}:2:${digest}`;
    const query={tool:'airk_search',arguments:{query:'reviewed',max_bytes:8192,reply_detail:'full',verification:'required',semantic:'off'}};
    const result={publication_vector:[{index_id:f.index,publication:before}],hits:[]};
    const members=[{index_id:f.index,original_publication:before,sources:[{source_root_id:f.source,source_relative_path:'one.txt',canonical_root:'/source',indexed_hash:digest}]}];
    const selection={schema:1,caller,key,original_result_digest:refreshDigest(result),original_query_digest:refreshDigest(query),original_evidence_selection_digest:refreshDigest(members),mode:'selected_only',selection:{schema:1,context_key:digest,namespace:ns,expected_catalog_revision:1,grant_id:'grt_'+'cc'.repeat(16),expected_grant_revision:1,project_ids:['prj_'+'dd'.repeat(16)],index_ids:[f.index]},members};
    const intent=validateRefreshIntent({reference_refresh:{configure_project:{project:digest,caller,key,request:{incarnation:ns,settings:digest,configuration:{operation:'refresh_references',selection}}},original_query:query,original_result:result,evidence_retention:'memory_only'}});
    const job={id:refreshJobId(intent),project:digest,kind:'configure',state:'running',fencing_generation:1,attempts:1};
    const configured={status:'reference_refresh_receipt',expired:false,job,configuration:{engine:digest,request:intent.reference_refresh.configure_project.request},result:null as Row|null,future_native:'retained'};
    const completed={...configured,job:{...job,state:'succeeded',outcome:{state:'succeeded',revision:after}},result:{schema:'airk.reference-refresh-result/1',selection_digest:refreshSelectionIdentity(selection),original_query_digest:selection.original_query_digest,effective_mode:'selected_only',effective_scope:'selected_sources',before_publication:before,after_publication:after,report:{generation:2},status:{publication:{token:after}},requery_executed:false}};
    return {intent,configured,completed,terminal:{status:'job',job:completed.job}};
}

test('finite slot catalogs cover each native family without a dynamic history',()=>{
    const f=splitFrozenFixture();
    assert.equal(acceptedIntentSlots(validateSplitRecover(f.intent)).length,MAX_ACCEPTED_INTENT_SLOTS);
    assert.equal(acceptedIntentSlots(validateSplitApply({catalog_split_apply:{draft:f.draft}})).length,15);
    assert.equal(acceptedIntentSlots(fence().intent).length,MAX_UNRESERVED_ACCEPTED_INTENT_SLOTS);
    assert.deepEqual(acceptedIntentSlots({enqueue:{project:digest,caller,key,kind:'index'}}),[]);
});
test('elision preserves unselected legacy diagnostics and refuses corrupt same-family history',()=>{
    const original=saved({enqueue:{project:digest,caller,key,kind:'index'}},{nativeSourceObservation:{diagnostic:'unrelated legacy bytes'},nativeRefreshReceipt:{diagnostic:'not refresh authority'}});
    assert.deepEqual(withoutAcceptedIntentEvidence(original),original);
    assert.deepEqual(mergeAcceptedIntentEvidence(original,[]),original);
    const f=setup();assert.throws(()=>withoutAcceptedIntentEvidence(saved(f.intent,{nativeSetupObservation:{diagnostic:'invalid claimed setup proof'}})));
});
test('stale unknown and pending catalog replies retain the entire applied fence anchor',()=>{
    const f=fence(),original=saved(f.intent,{nativeFenceObservation:f.applied}),anchors=selectAcceptedIntentEvidence(original);
    for(const current of [saved(f.intent,{nativeObservation:{status:'catalog_receipt',receipt:null}}),saved(f.intent,{nativeFenceObservation:f.pending})]){
        const merged=mergeAcceptedIntentEvidence(current,anchors);assert.deepEqual(merged.nativeFenceObservation,f.applied);
        assert.deepEqual(merged.nativeObservation,current.nativeObservation);
    }
    const pending=selectAcceptedIntentEvidence(saved(f.intent,{nativeFenceObservation:f.pending}))[0];
    assert.equal(acceptedIntentEvidenceDominates(original,anchors[0],pending),true);
    assert.equal(acceptedIntentEvidenceDominates(original,pending,anchors[0]),false);
});
test('changed fenced authority cannot replace an accepted proof',()=>{
    const f=fence(),s=saved(f.intent,{nativeFenceObservation:f.applied}),anchors=selectAcceptedIntentEvidence(s),changed=structuredClone(f.applied);
    changed.fenced_receipt.metadata.operation_id='ee'.repeat(16);
    changed.receipt={...changed.receipt,operation_id:'ee'.repeat(16)};
    assert.throws(()=>mergeAcceptedIntentEvidence(saved(f.intent,{nativeFenceObservation:changed}),anchors));
});
test('unknown catalog envelopes use their canonical validator and preserve full pending/applied observations',()=>{
    const f=fence();
    for(const [state,value,slot] of [['pending',f.pending.fence.value,'fence_pending'],['applied',f.applied.fenced_receipt,'fence_applied']] as const){
        const response={status:'catalog_outcome_unknown',code:'catalog_outcome_unknown',caller,key,request_digest:digest,observation:{kind:'authority_fence',fence:{state,value}},future_native:{all:'kept'}};
        const s=saved(f.intent,{nativeFenceObservation:response}),anchors=selectAcceptedIntentEvidence(s);
        assert.equal(anchors[0].slot,slot);assert.deepEqual(anchors[0].value,response);
        assert.deepEqual(mergeAcceptedIntentEvidence(saved(f.intent,{nativeObservation:{status:'catalog_receipt',receipt:null}}),anchors).nativeFenceObservation,response);
    }
    assert.throws(()=>selectAcceptedIntentEvidence(saved(f.intent,{nativeFenceObservation:{...f.applied,status:'failure'}})));
    assert.equal(selectAcceptedIntentEvidence(saved(f.intent,{nativeObservation:{...f.applied,status:'failure'}})).length,0);
});
test('setup phase advancement dominates earlier stages and keeps current full envelopes',()=>{
    const f=setup(),old=saved(f.intent,{nativeSetupObservation:f.reply(f.reserved)}),newer=saved(f.intent,{nativeSetupObservation:f.reply(f.ready)});
    const early=selectAcceptedIntentEvidence(old),late=selectAcceptedIntentEvidence(newer);
    for(const evidence of [[...early,...late],[...late,...early]])assert.deepEqual(mergeAcceptedIntentEvidence(old,evidence).nativeSetupObservation,f.reply(f.ready));
    assert.equal(acceptedIntentEvidenceDominates(newer,late[0],early[0]),true);
    const changed=structuredClone(f.ready);changed.preparation.storage_identity='ff'.repeat(32);
    assert.throws(()=>mergeAcceptedIntentEvidence(saved(f.intent,{nativeSetupObservation:f.reply(changed)}),selectAcceptedIntentEvidence(saved(f.intent,{nativeSetupObservation:f.reply(f.prepared)}))));
});
test('source proof is found in either field and completion survives stale begin observations',()=>{
    const f=sources(),complete=saved(f.intent,{nativeObservation:f.complete}),anchors=selectAcceptedIntentEvidence(complete);
    const merged=mergeAcceptedIntentEvidence(saved(f.intent,{nativeObservation:f.pending}),anchors);
    assert.equal(merged.nativeSourceObservation,undefined);assert.deepEqual(merged.nativeObservation,f.complete);
    const diagnostic={status:'failure',code:'source_observation_unavailable',future_native:{all:'kept'}};
    const unavailable=mergeAcceptedIntentEvidence(saved(f.intent,{nativeObservation:diagnostic}),anchors);
    assert.deepEqual(unavailable.nativeSourceObservation,f.complete);assert.deepEqual(unavailable.nativeObservation,diagnostic);
    assert.equal(mergeAcceptedIntentEvidence(complete,anchors).nativeSourceObservation,undefined,'do not double a native-size accepted body');
    const large=structuredClone(f.complete);Object.assign(large,{future_padding:'x'.repeat(50000)});
    const largeSaved=saved(f.intent,{nativeObservation:large});
    assert.equal(JSON.stringify(mergeAcceptedIntentEvidence(largeSaved,selectAcceptedIntentEvidence(largeSaved))).length,JSON.stringify(largeSaved).length);
    const changed=structuredClone(f.complete);changed.observation.completion.operation_id='ec'.repeat(16);
    assert.throws(()=>mergeAcceptedIntentEvidence(saved(f.intent,{nativeObservation:changed}),anchors));
});
test('native-size source begin progresses to an original completed DTO within unchanged capacity',()=>{
    const f=sources(),pending={...f.pending,future_padding:'x'.repeat(61000)},complete={...f.complete,future_padding:'x'.repeat(61000)};
    const old=saved(f.intent,{nativeObservation:pending}),done=saved(f.intent,{nativeObservation:complete});
    const before=selectAcceptedIntentEvidence(old),after=selectAcceptedIntentEvidence(done);
    assert.ok(Buffer.byteLength(JSON.stringify(old))>60000&&Buffer.byteLength(JSON.stringify(done))<=65536);
    for(const evidence of [after,[...before,...after],[...after,...before]]){
        const merged=mergeAcceptedIntentEvidence(old,evidence);
        assert.deepEqual(merged.nativeObservation,complete);assert.equal(merged.nativeSourceObservation,undefined);
        assert.equal(Buffer.byteLength(JSON.stringify(merged)),Buffer.byteLength(JSON.stringify(done)));
    }
    const unknown={status:'source_change_resolution_outcome_unknown',code:'source_change_resolution_outcome_unknown',begin_operation:operation,observation:null,future_padding:'y'.repeat(61000)};
    const retained=mergeAcceptedIntentEvidence(saved(f.intent,{nativeObservation:unknown}),after);
    assert.deepEqual(retained.nativeObservation,unknown);assert.deepEqual(retained.nativeSourceObservation,complete);
    assert.ok(Buffer.byteLength(JSON.stringify(retained))>65536,'caller must reject this projected record before publication without dropping the full current reply');
});
test('historical refresh pre-admission refusal is weaker than subsequent accepted authority',()=>{
    const f=refresh(),request=f.intent.reference_refresh.configure_project;
    const refusal={status:'reference_refresh_review_required',project:request.project,caller:request.caller,key:request.key,request:request.request,review:{reasons:['no_current_index']},future_native:'kept'};
    const weak=selectAcceptedIntentEvidence(saved(f.intent,{nativeRefreshReceipt:refusal})),strong=selectAcceptedIntentEvidence(saved(f.intent,{nativeRefreshReceipt:f.configured}));
    for(const evidence of [[...weak,...strong],[...strong,...weak]])assert.deepEqual(mergeAcceptedIntentEvidence(saved(f.intent),evidence).nativeRefreshReceipt,f.configured);
});
test('refresh result/configuration preference is deterministic with terminal job-only proof',()=>{
    const f=refresh(),configured=selectAcceptedIntentEvidence(saved(f.intent,{nativeRefreshReceipt:f.configured})),terminal=selectAcceptedIntentEvidence(saved(f.intent,{nativeRefreshReceipt:f.terminal}));
    for(const anchors of [[...terminal,...configured],[...configured,...terminal]]){
        const current=saved(f.intent,{nativeRefreshLookup:{status:'reference_refresh_receipt',job:null,configuration:null,result:null,expired:true}});
        const merged=mergeAcceptedIntentEvidence(current,anchors);assert.deepEqual(merged.nativeRefreshReceipt,f.configured);assert.deepEqual(merged.nativeRefreshLookup,current.nativeRefreshLookup);
        assert.deepEqual(merged.nativeRefreshTerminalReceipt,f.terminal,'terminal admission proof stays separately visible without inventing a fused native DTO');
    }
    const full=selectAcceptedIntentEvidence(saved(f.intent,{nativeRefreshReceipt:f.completed}));
    assert.deepEqual(mergeAcceptedIntentEvidence(saved(f.intent),[...terminal,...full]).nativeRefreshReceipt,f.completed);
    assert.equal(mergeAcceptedIntentEvidence(saved(f.intent),[...terminal,...full]).nativeRefreshTerminalReceipt,undefined);
    const latestTerminal=mergeAcceptedIntentEvidence(saved(f.intent,{nativeRefreshLookup:f.terminal}),[...configured,...terminal]);
    assert.equal(latestTerminal.nativeRefreshTerminalReceipt,undefined,'current validated terminal reply already supplies the original full proof');
    const changed=structuredClone(f.terminal);changed.job={...changed.job,state:'failed',outcome:{state:'succeeded',revision:`airk-r1:${ns}:2:${digest}`}};
    assert.throws(()=>mergeAcceptedIntentEvidence(saved(f.intent,{nativeRefreshReceipt:changed}),terminal));
});
test('split anchors keep the stronger complete snapshot independent of order or stale same-stage progress',()=>{
    const f=splitFixture(),intent=validateSplitApply({catalog_split_apply:{draft:f.draft}}),old=saved(intent,{nativeSplitSnapshot:f.snapshot});
    const next=splitAtStage(f.snapshot,'destination_prepared',2),early=selectAcceptedIntentEvidence(old),late=selectAcceptedIntentEvidence(saved(intent,{nativeSplitSnapshot:next}));
    for(const anchors of [[...early,...late],[...late,...early]])assert.deepEqual(mergeAcceptedIntentEvidence(old,anchors).nativeSplitSnapshot,next);
    const later=structuredClone(next);(later.receipt as Row).revision=3;(later.receipt as Row).catalog_revision=4;later.receipt_digest='ef'.repeat(32);
    const laterEvidence=selectAcceptedIntentEvidence(saved(intent,{nativeSplitSnapshot:later}));
    assert.deepEqual(mergeAcceptedIntentEvidence(saved(intent,{nativeSplitSnapshot:next}),laterEvidence).nativeSplitSnapshot,later);
    for(const anchors of [[...late,...laterEvidence],[...laterEvidence,...late]])assert.deepEqual(mergeAcceptedIntentEvidence(old,anchors).nativeSplitSnapshot,later);
    const crossed=structuredClone(next);(crossed.receipt as Row).destination_storage='ed'.repeat(32);
    assert.throws(()=>mergeAcceptedIntentEvidence(saved(intent,{nativeSplitSnapshot:crossed}),late));
});
test('split recovery cannot anchor status older than its immutable reviewed snapshot',()=>{
    const f=splitFrozenFixture(),intent=validateSplitRecover(f.intent);
    assert.throws(()=>selectAcceptedIntentEvidence(saved(intent,{nativeSplitSnapshot:f.snapshot})));
    const s=saved(intent,{nativeSplitSnapshot:f.reviewed,nativeSplitReceipt:f.acknowledgment});
    assert.doesNotThrow(()=>mergeAcceptedIntentEvidence(s,selectAcceptedIntentEvidence(s)));
});
test('policy accepted job has a separate historical anchor while unknown current reply stays intact',()=>{
    const f=splitFixture(),project=String((f.draft.request as Row).original_registration&&((f.draft.request as Row).original_registration as Row).binding);
    const intent=validatePolicyResumeIntent({catalog_policy_resume:{index_id:f.index,operation_id:operation,caller,key},project});
    const reply={status:'catalog_policy_resumed',index_id:f.index,accepted_job_id:digest,status_basis:'current_policy',snapshot:{schema:1,operation_id:operation,job:digest,policy:f.draft.before_policy,last_error:null,application:null,application_job:null},job:{id:digest,project,kind:'configure',state:'running',fencing_generation:1,attempts:1},future_native:{all:'kept'}};
    const original=saved(intent,{nativeObservation:reply}),anchors=selectAcceptedIntentEvidence(original);
    assert.equal(anchors[0].slot,'policy_resume');assert.equal(mergeAcceptedIntentEvidence(original,anchors).nativePolicyObservation,undefined);
    const current=saved(intent,{nativeObservation:{status:'catalog_policy_resume_receipt',receipt:null}}),merged=mergeAcceptedIntentEvidence(current,anchors);
    assert.deepEqual(merged.nativePolicyObservation,reply);assert.deepEqual(merged.nativeObservation,current.nativeObservation);
    const changed={...reply,accepted_job_id:'fa'.repeat(32)};assert.throws(()=>mergeAcceptedIntentEvidence(saved(intent,{nativeObservation:changed}),anchors));
});
test('non-fenced catalog metadata retains accepted identity without duplicating current full reply',()=>{
    const f=fence(),metadata={...f.metadata,authority_fence_required:false},reply={status:'catalog_applied',receipt:metadata,future_native:'kept'},original=saved(f.intent,{nativeObservation:reply});
    const anchors=selectAcceptedIntentEvidence(original);assert.equal(anchors[0].slot,'catalog_metadata_applied');
    assert.equal(mergeAcceptedIntentEvidence(original,anchors).nativeCatalogObservation,undefined);
    const current=saved(f.intent,{nativeObservation:{status:'catalog_receipt',receipt:null}});
    assert.deepEqual(mergeAcceptedIntentEvidence(current,anchors).nativeCatalogObservation,reply);
    assert.deepEqual(withoutAcceptedIntentEvidence({...original,nativeCatalogObservation:reply}).nativeObservation,reply);
    assert.throws(()=>mergeAcceptedIntentEvidence(saved(f.intent,{nativeObservation:{...reply,receipt:{...metadata,operation_id:'fb'.repeat(16)}}}),anchors));
});
