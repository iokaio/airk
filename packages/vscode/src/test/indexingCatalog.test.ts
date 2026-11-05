// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { CatalogApplyIntent, CatalogObject, CatalogRequest, CatalogReview, WindowContextDescriptor, CATALOG_UI_OPERATIONS, catalogExport, catalogItems, catalogMembershipDraft, validateCatalogScope, validateCatalogApply, validateCatalogOperation, validateGrantSelection, validateNativeGrantDraft, validateNativeHostContext } from '../indexingCatalog';
import { CatalogControl, IndexingCatalogController } from '../indexingCatalogController';
import { IntentJournal, IntentStorage } from '../serviceIntents';
import { ServiceClient, ServiceError } from '../serviceClient';
import { parseWebviewMessage } from '../explorerContracts';
import { parseRoute } from '../explorerRoutes';
import { validateIndexSetupSelection, validateIndexSetupApply, validateNativeIndexSetupDraft, validateIndexSetupReceipt, validateNativeLegacyBinding } from '../indexingCatalog';
import { validateNativeSourceDraft, validateSourceMutationSelection } from '../indexingCatalog';
import { catalogObject } from '../indexingCatalog';

const namespace='ab'.repeat(16), caller='cd'.repeat(16), nonce='ef'.repeat(16), digest='12'.repeat(32), project='prj_'+'34'.repeat(16), index='idx_'+'56'.repeat(16), source='src_'+'78'.repeat(16);
const requestDigest='13'.repeat(32);
const projectValue={id:project,name:'Project <safe>',definition_revision:1,scope_revision:1,defaults_revision:1,default_indexes:[]};
const operation={operation:'rename_project',id:project,expected_definition_revision:1,name:'Renamed'};
const request:CatalogRequest={caller,key:{namespace,epoch:1,nonce},expected_catalog_revision:1,operation};
const intent:CatalogApplyIntent={catalog_apply:{request,preview_digest:digest},request_digest:requestDigest};
const review:CatalogReview={request,response:{status:'catalog_preview',preview:{request_digest:requestDigest,catalog_revision:1,next_revision:2,operation,before:[],after:[],authority_fence_required:false,file_data_changes:false},preview_digest:digest},previewDigest:digest,requestDigest};
const page=(extra:CatalogObject={})=>({status:'catalog_page',page:{namespace,revision:1,items:[{kind:'project',value:projectValue}],continuation:null,...extra}});
test('recommendations retain complete native results and fence late owner changes without mutations',async()=>{
    let owner='one',resolve!:(value:CatalogObject)=>void,mutations=0;
    const control:CatalogControl={catalogRead:async()=>page(),catalogPrepare:async()=>{mutations++;return review;},catalogApply:async()=>{mutations++;return {};},catalogReconcile:async()=>[],catalogPending:()=>[]};
    const controller=new IndexingCatalogController({control,identity:()=>owner,authorized:()=>true,changed(){}});
    const result={schema:'airk.recommendations/1',future_field:{retained:true},selected:{project_ids:[project],index_ids:[index]},instructions:{text:'scope example'}};
    const first=await controller.recommendations(async()=>result);
    assert.deepEqual(first.recommendations,result);assert.match(catalogExport(first),/future_field/);
    const pending=controller.recommendations(()=>new Promise(done=>{resolve=done;}));
    owner='two';resolve(result);await assert.rejects(pending,/stale_catalog_view/);
    assert.equal(controller.snapshot().recommendations,undefined);assert.equal(mutations,0);
    await controller.recommendations(async()=>result);controller.dismissRecommendations();assert.equal(controller.snapshot().recommendations,undefined);
    const dismissed=controller.recommendations(()=>new Promise(done=>{resolve=done;}));controller.dismissRecommendations();resolve(result);
    await assert.rejects(dismissed,/stale_catalog_view/);assert.equal(controller.snapshot().busy,false);assert.equal(controller.snapshot().recommendations,undefined);
    assert.ok(parseWebviewMessage({v:1,type:'indexing.recommendations',requestId:'advice',arguments:{selection:{project_ids:[project],index_ids:[index]}}}));
});
function setupFixture(){
    const selection={key:request.key,expected_catalog_revision:1,name:'Explicit setup',primary_root:'/chosen/source',data_root:'/chosen/storage',collection:'fixture',additional_roots:[],setup_watch:false};
    const setupRequest={caller:'9a'.repeat(16),key:request.key,expected_catalog_revision:1,name:selection.name,registration:{binding:'11'.repeat(32),root:'/native/source',data_root:'/native/storage',collection:'fixture'},sources:[{canonical_path:'/native/source',aliases:[],owner:true}],setup_watch:false};
    const draft={schema:1,request:setupRequest,storage_action:'create_missing_leaf',review_digest:digest,future_native_review:'retained'};
    const response={status:'catalog_index_setup_draft',draft,request_digest:requestDigest,future_native_response:'retained'};
    const reserved={schema:1,operation_id:'22'.repeat(16),request:setupRequest,request_digest:requestDigest,index_id:index,source_ids:[source],stage:'reserved',catalog_revision:2,source_revision:1,preparation:null,completion:null,future_native_receipt:'retained'};
    const prepared={...reserved,stage:'prepared',preparation:{digest:'33'.repeat(32),storage_identity:'44'.repeat(32),source_configuration_digest:'55'.repeat(32),review:{expected_catalog_revision:2,expected_source_revision:1}}};
    const ready={...prepared,stage:'ready',catalog_revision:3,source_revision:2,completion:{prepared_digest:prepared.preparation.digest,expected_catalog_revision:2,expected_source_revision:1}};
    return {selection,draft,response,reserved,prepared,ready};
}

test('setup selectors carry choices only; native canonical locations and complete review are retained',()=>{
    const f=setupFixture(),{key,expected_catalog_revision,...choices}=f.selection;
    assert.deepEqual(validateIndexSetupSelection(choices),choices);
    assert.throws(()=>validateIndexSetupSelection({...choices,binding:'11'.repeat(32)}),/fields/);
    assert.throws(()=>validateIndexSetupSelection({...choices,primary_root:'é'.repeat(2049)}),/path/);
    assert.throws(()=>validateIndexSetupSelection({...choices,additional_roots:[{alias:'a',path:'/a'},{alias:'a',path:'/b'}]}),/alias/);
    const reviewed=validateNativeIndexSetupDraft(f.response,f.selection);assert.equal(reviewed.response,f.response);assert.equal(reviewed.draft,f.draft);
    assert.throws(()=>validateNativeIndexSetupDraft(f.response,{...f.selection,key:{...key,nonce:caller}}),/crossed/);
    assert.throws(()=>validateNativeIndexSetupDraft(f.response,{...f.selection,expected_catalog_revision:expected_catalog_revision+1}),/crossed/);
    assert.deepEqual(validateIndexSetupApply({catalog_index_setup_apply:{draft:f.draft},request_digest:requestDigest}),{catalog_index_setup_apply:{draft:f.draft},request_digest:requestDigest});
    const exported=catalogExport({generation:1,busy:false,setupReview:reviewed});assert.match(exported,/future_native_review/);assert.match(exported,/native\/storage/);
});

test('native legacy eligibility preserves complete positive snapshots and refuses crossed identities',async()=>{
    const binding='66'.repeat(32),registration={binding,root:'/native/source',data_root:'/native/storage',collection:'legacy'};
    const value={status:'catalog_legacy_binding',observation:{schema:1,namespace,catalog_revision:null,project:binding,eligibility:{kind:'authority_zero',registration},future_native_field:{retained:true}},future_response:'retained'};
    assert.equal(validateNativeLegacyBinding(value,binding),value);
    assert.throws(()=>validateNativeLegacyBinding(value,digest),/crossed/);assert.throws(()=>validateNativeLegacyBinding(value,binding,2),/crossed/);
    const grant={...nativeGrantOperation().grant,id:'grt_'+'91'.repeat(16)},wrapped={grant,provenance:{kind:'imported_legacy_snapshot',upgrade_id:nonce,plan_digest:digest,legacy_binding:binding,configuration_digest:requestDigest},future_provenance:'retained'};
    const imported={...value,observation:{...value.observation,catalog_revision:2,eligibility:{kind:'imported_snapshot',registration,grant:wrapped}}};
    assert.equal(validateNativeLegacyBinding(imported,binding,2),imported);
    assert.throws(()=>validateNativeLegacyBinding({...imported,observation:{...imported.observation,eligibility:{...imported.observation.eligibility,grant:{...wrapped,grant:{...grant,revoked:true}}}}},binding,2),/crossed/);
    const client=new ServiceClient({dispose(){},async request(method){assert.deepEqual(method,{catalog_legacy_binding:{project:binding,expected_catalog_revision:2}});return imported;}});
    assert.equal(await client.catalogLegacyBinding(binding,2),imported);
});

test('setup full receipt validation fences identity, stage transitions and immutable native preparation',()=>{
    const f=setupFixture();assert.equal(validateIndexSetupReceipt(f.reserved,f.draft),f.reserved);validateIndexSetupReceipt(f.prepared,f.draft,f.reserved);validateIndexSetupReceipt(f.ready,f.draft,f.prepared);
    for(const bad of [{...f.ready,request:{...f.ready.request,caller}},{...f.ready,source_ids:[]},{...f.ready,completion:null},{...f.ready,catalog_revision:2},{...f.ready,source_revision:1}])assert.throws(()=>validateIndexSetupReceipt(bad,f.draft));
    assert.throws(()=>validateIndexSetupReceipt(f.reserved,f.draft,f.prepared),/continuity/);
    assert.throws(()=>validateIndexSetupReceipt({...f.ready,preparation:{...f.prepared.preparation,storage_identity:digest}},f.draft,f.prepared),/continuity/);
});

test('setup journal persists exact native draft, never replays, and retains original receipt through a missing lookup',async()=>{
    const f=setupFixture(),store=storage(),journal=new IntentJournal(store.adapter,'control',namespace),value={catalog_index_setup_apply:{draft:f.draft},request_digest:requestDigest};
    const id=await journal.prepare('Explicit setup',value);let writes=0;
    const client=new ServiceClient({dispose(){},async request(){writes++;return {status:'failure',code:'unexpected'};}});
    await assert.rejects(journal.submit(client,id),/receipt_reconciliation/);assert.equal(writes,0);
    await journal.observeCatalog(id,{status:'catalog_index_setup_receipt',receipt:f.prepared});await journal.observeCatalog(id,{status:'catalog_index_setup_receipt',receipt:null});
    const reopened=new IntentJournal(store.adapter,'control',namespace),saved=reopened.list()[0];assert.deepEqual(saved.value.intent,value);assert.deepEqual(saved.value.nativeSetupObservation,{status:'catalog_index_setup_receipt',receipt:f.prepared});
    assert.equal(client.validateIndexSetupObservation({status:'catalog_index_setup_receipt',receipt:null},f.draft,requestDigest,saved.value.nativeSetupObservation),false);
    assert.equal(client.validateIndexSetupObservation({status:'catalog_index_setup_receipt',receipt:f.ready},f.draft,requestDigest,saved.value.nativeSetupObservation),true);
    assert.throws(()=>client.validateIndexSetupObservation({status:'catalog_index_setup_receipt',receipt:{...f.ready,request_digest:digest}},f.draft,requestDigest),/digest/);
    assert.throws(()=>client.validateIndexSetupObservation({status:'catalog_outcome_unknown',code:'catalog_index_setup_outcome_unknown',caller:f.draft.request.caller,key:f.draft.request.key,request_digest:digest},f.draft,requestDigest),/crossed/);
});

test('setup controller requires native review then explicit apply and drops late replaced-client drafts',async()=>{
    const f=controller(),setup=setupFixture();let applies=0;
    f.control.catalogIndexSetupDraft=async()=>validateNativeIndexSetupDraft(setup.response,setup.selection);
    f.control.catalogIndexSetupApply=async()=>{applies++;return {status:'catalog_index_setup_applied',receipt:setup.ready};};
    await f.value.page('indexes');const {key,expected_catalog_revision,...selection}=setup.selection;
    await f.value.setupDraft(selection,f.value.snapshot().generation);assert.equal(applies,0);assert.equal(f.value.snapshot().setupReview?.response,setup.response);
    await f.value.setupApply(f.value.snapshot().generation);assert.equal(applies,1);assert.equal(f.value.snapshot().setupReview,undefined);assert.ok(f.value.snapshot().submittedSetupReview);
    await f.value.page('indexes');f.control.catalogIndexSetupDraft=async()=>{f.replace();return validateNativeIndexSetupDraft(setup.response,setup.selection);};
    await assert.rejects(f.value.setupDraft(selection,f.value.snapshot().generation),/stale/);assert.equal(f.value.snapshot().setupReview,undefined);assert.equal(applies,1);
});
function nativeGrantOperation(host='22'.repeat(32)){
    return {operation:'create_grant',grant:{id:'grt_'+'11'.repeat(16),host,revision:1,revoked:false,memberships:[{
        membership:{project_id:project,index_id:index,revision:1,scope:[{source_root_id:source,kind:'subtree',path:''}]},source_revision:1,policy_epoch:0,policy_digest:null,
    }]}};
}
function nativeGrantResponse(key=request.key,host='22'.repeat(32)){
    const operation=nativeGrantOperation(host),nativeRequest={...request,caller:'9a'.repeat(16),key,operation};
    return {status:'catalog_grant_draft',draft:{request:nativeRequest,preview:{request_digest:requestDigest,catalog_revision:1,next_revision:2,operation,before:[],after:[{kind:'grant',value:{grant:operation.grant,provenance:{kind:'user_reviewed_project_grant'}}}],authority_fence_required:false,file_data_changes:false,future_preview_field:{retained:true}},preview_digest:digest,future_draft_field:{retained:true}},future_response_field:{retained:true}};
}
function nativeSourceOperation(){
    return {operation:'begin_index_source_change',id:index,expected_source_revision:1,plan:{registration:{binding:digest,root:'/source',data_root:'/data',collection:'fixture'},engine_identity:'21'.repeat(32),job:'22'.repeat(32),
        configuration:{incarnation:'da'.repeat(16),settings:'db'.repeat(32),configuration:{operation:'add_root',alias:'extra',path:'/extra'}},before:{primary_root:'/source',aliases:{}},after:{primary_root:'/source',aliases:{extra:'/extra'}},
        sources:[{id:source,canonical_path:'/source',aliases:[],owner:true},{id:'src_'+'91'.repeat(16),canonical_path:'/extra',aliases:['extra'],owner:false}]}};
}
function nativeSourceFixture(retire=false){
    const operation:CatalogObject=retire?{operation:'retire_index',id:index,expected_definition_revision:1,expected_source_revision:1,reason:{reason:'explicit'}}:nativeSourceOperation();
    const nativeRequest:CatalogRequest={...request,caller:'9a'.repeat(16),operation},metadata={schema:1,operation_id:'90'.repeat(16),caller:nativeRequest.caller,key:nativeRequest.key,request_digest:requestDigest,catalog_revision:2,authority_fence_required:true};
    const proof:CatalogObject={plan_digest:'23'.repeat(32),service:'24'.repeat(16),gate_generation:1,confirmations:[],...(retire?{worker_obligation:{index_id:index,backing:digest,workers:[],providers:[]},worker_confirmations:[]}:{})};
    const begin={request:nativeRequest,metadata,preview_digest:digest,prepared_digest:'25'.repeat(32),proof};
    const response={status:'catalog_source_mutation_draft',draft:{request:nativeRequest,preview:{request_digest:requestDigest,catalog_revision:1,next_revision:2,operation,before:[],after:[],authority_fence_required:true,file_data_changes:false,future_preview:'retained'},preview_digest:digest,future_draft:'retained'},future_response:'retained'};
    const observation={status:'catalog_source_change_observation',observation:{begin,job:{id:'22'.repeat(32),project:digest,kind:'configure',state:'failed',fencing_generation:1,attempts:2,outcome:{state:'failed',code:'disk_full'},future_job:'retained'},configuration_result:{prior_native_result:'retained'},completion:null as CatalogObject|null,future_observation:'retained'}};
    return {nativeRequest,metadata,begin,response,observation};
}

test('native retirement evidence validates complete lineage and native-event reasons without public mutation admission',async()=>{
    const f=nativeSourceFixture(true),full=structuredClone(f.begin);full.request.expected_catalog_revision=2;full.metadata.catalog_revision=3;full.request.caller='aa'.repeat(16);full.metadata.caller=full.request.caller;full.request.operation.reason={reason:'confirmed_missing_source',registration_epoch:0,configuration_epoch:2};
    const response:CatalogObject={status:'catalog_index_retirement_evidence',index_id:index,evidence:{catalog_clock:{namespace,revision:4},index:{id:index,name:'Retired',definition_revision:2,source_revision:2,backing:{binding:digest},tombstone:true,source_import:'pending_metadata'},retirement:full,future_evidence:'retained'},future_output:'retained'};
    let output=response;const calls:unknown[]=[],client=new ServiceClient({dispose(){},async request(method){calls.push(method);return output;}});
    assert.equal(await client.catalogRetirementEvidence(index,namespace,digest,2),response);assert.deepEqual(calls,[{catalog_index_retirement_evidence:{index_id:index}}]);
    assert.throws(()=>validateCatalogOperation(full.request.operation),/fields|explicit/,'native-event output does not broaden public input');
    for(const mutate of [(x:CatalogObject)=>{catalogObject(x.evidence).index={...catalogObject(catalogObject(x.evidence).index),tombstone:false};},(x:CatalogObject)=>{catalogObject(catalogObject(x.evidence).retirement).proof={...catalogObject(catalogObject(catalogObject(x.evidence).retirement).proof),worker_obligation:undefined};},(x:CatalogObject)=>{catalogObject(catalogObject(x.evidence).index).source_revision=3;},(x:CatalogObject)=>{catalogObject(catalogObject(x.evidence).catalog_clock).namespace='aa'.repeat(16);},(x:CatalogObject)=>{catalogObject(x.evidence).future='x'.repeat(32768);}]){
        output=structuredClone(response);mutate(output);await assert.rejects(client.catalogRetirementEvidence(index,namespace,digest,2));
    }
    output=structuredClone(response);const body=catalogObject(output.evidence);body.padding='';body.padding='x'.repeat(32768-Buffer.byteLength(JSON.stringify(body)));assert.ok(Buffer.byteLength(JSON.stringify(output))>32768);assert.equal(await client.catalogRetirementEvidence(index,namespace,digest,2),output,'body at native limit plus frame headers is valid');body.padding=String(body.padding)+'x';await assert.rejects(client.catalogRetirementEvidence(index,namespace,digest,2),/oversize/);
    output={...response,frame_padding:'x'.repeat(65536)};await assert.rejects(client.catalogRetirementEvidence(index,namespace,digest,2),/oversize/);
    output={status:'catalog_index_retirement_evidence',index_id:index,evidence:null};assert.equal(await client.catalogRetirementEvidence(index,namespace,digest,2),output);
});

test('source discovery keeps its 48 KiB native body budget separate from the 64 KiB response frame',async()=>{
    const native=nativeSourceFixture(),body:CatalogObject=structuredClone(native.observation.observation);body.padding='';body.padding='x'.repeat(49152-Buffer.byteLength(JSON.stringify(body)));
    let response:CatalogObject={status:'catalog_pending_source_change',index_id:index,observation:body};const client=new ServiceClient({dispose(){},async request(){return response;}});
    assert.ok(Buffer.byteLength(JSON.stringify(response))>49152);assert.equal(await client.catalogPendingSource(index,namespace),response);body.padding=String(body.padding)+'x';await assert.rejects(client.catalogPendingSource(index,namespace),/oversize/);
    response={status:'catalog_pending_source_change',index_id:index,observation:native.observation.observation,frame_padding:'x'.repeat(65536)};await assert.rejects(client.catalogPendingSource(index,namespace),/oversize/);
});

test('native source draft preserves exact frozen inputs and rejects changed selector/revision/plan identity',()=>{
    const f=nativeSourceFixture(),selection={key:request.key,expected_catalog_revision:1,index_id:index,change:{operation:'add_root',alias:'extra',path:'/user-spelling'}};
    const reviewed=validateNativeSourceDraft(f.response,selection);assert.equal(reviewed.response,f.response);assert.equal(reviewed.request,f.response.draft.request);
    assert.equal((reviewed.request.operation.plan as CatalogObject).configuration!==undefined,true,'canonical omitted prepare:false is accepted unchanged');
    assert.throws(()=>validateNativeSourceDraft(f.response,{...selection,index_id:'idx_'+'92'.repeat(16)}),/crossed/);
    assert.throws(()=>validateNativeSourceDraft(f.response,{...selection,expected_catalog_revision:2}),/crossed/);
    assert.throws(()=>validateNativeSourceDraft(f.response,{...selection,change:{operation:'remove_root',alias:'extra'}}),/crossed/);
    assert.throws(()=>validateSourceMutationSelection({operation:'retire_index',reason:{reason:'confirmed_missing_source'}}),/fields/);
    const changed=structuredClone(f.response);catalogObjectForTest(changed.draft.request.operation.plan).after={primary_root:'/source',aliases:{other:'/extra'}};assert.throws(()=>validateNativeSourceDraft(changed,selection),/plan|alias/);
    assert.match(catalogExport({generation:1,busy:false,review:reviewed}),/future_draft/);
    const core=readFileSync(resolve(__dirname,'../../../..','crates/airk-core/src/catalog_source_draft.rs'),'utf8');
    for(const [native,choice] of [['AddRoot',{operation:'add_root',alias:'extra',path:'/extra'}],['RemoveRoot',{operation:'remove_root',alias:'extra'}]] as const){
        const fields=new RegExp(`    ${native} \\{([\\s\\S]*?)^    \\}`, 'm').exec(core);assert.ok(fields);assert.deepEqual(Object.keys(validateSourceMutationSelection(choice)).filter(key=>key!=='operation').sort(),[...fields![1].matchAll(/^        ([a-z_]+):/gm)].map(value=>value[1]).sort());
    }
    assert.ok(parseWebviewMessage({v:1,type:'indexing.source.draft',requestId:'source',generation:1,change:{operation:'retire_index'}}));
    assert.equal(parseWebviewMessage({v:1,type:'indexing.source.resolve',requestId:'resolve',generation:1,proof:{frontend:true}}),undefined);
    assert.equal(parseWebviewMessage({v:1,type:'indexing.setup.resume',requestId:'resume',generation:1,id:nonce,draft:f.response.draft}),undefined);
});

test('concrete native source configuration keeps collection incarnation and settings independent from registry and backing selectors',()=>{
    const native=nativeSourceFixture(),selection={key:request.key,expected_catalog_revision:1,index_id:index,change:{operation:'add_root',alias:'extra',path:'/extra'}},reviewed=validateNativeSourceDraft(native.response,selection);
    const configuration=catalogObject(catalogObject(reviewed.request.operation.plan).configuration);
    assert.notEqual(configuration.incarnation,request.key.namespace);assert.notEqual(configuration.settings,catalogObject(catalogObject(reviewed.request.operation.plan).registration).binding);
    assert.equal(configuration.incarnation,'da'.repeat(16));assert.equal(configuration.settings,'db'.repeat(32));assert.equal(reviewed.request,native.response.draft.request);
    const exported=JSON.parse(catalogExport({generation:1,busy:false,review:reviewed}));assert.deepEqual(exported.review.request.operation.plan.configuration,configuration);
    for(const [field,value] of [['incarnation','da'.repeat(32)],['settings','db'.repeat(16)],['prepare',true]] as const){
        const invalid=structuredClone(native.nativeRequest);catalogObject(catalogObject(invalid.operation.plan).configuration)[field]=value;assert.throws(()=>validateCatalogApply({catalog_apply:{request:invalid,preview_digest:digest},request_digest:requestDigest}));
    }
    for(const field of ['incarnation','settings'] as const){
        const changed=structuredClone(native.response);changed.draft.request={...changed.draft.request,operation:structuredClone(changed.draft.request.operation)};
        catalogObject(catalogObject(changed.draft.request.operation.plan).configuration)[field]=field==='incarnation'?'dc'.repeat(16):'dd'.repeat(32);
        assert.throws(()=>validateNativeSourceDraft(changed,selection),/crossed_native_source_draft/,'a valid-shaped concrete identity still must match the full native preview');
    }
});

test('retirement requires exact native worker/provider obligation and complete reap proof even for empty backing',async()=>{
    const f=nativeSourceFixture(true),value:CatalogApplyIntent={catalog_apply:{request:f.nativeRequest,preview_digest:digest},request_digest:requestDigest};let full=structuredClone(f.begin);
    const client=new ServiceClient({dispose(){},async request(){return {status:'catalog_applied',receipt:f.metadata,fenced_receipt:full};}});
    await client.submitCatalog(value);
    delete full.proof.worker_obligation;await assert.rejects(client.submitCatalog(value),/worker_obligation_required/);
    const worker={service:'24'.repeat(16),worker:'26'.repeat(16),project:digest,fencing_generation:3},obligation={index_id:index,backing:digest,workers:[worker],providers:[{worker,history:{kind:'helpers',generations:['27'.repeat(16)]}}]};
    full=structuredClone(f.begin);full.proof.worker_obligation=obligation;full.proof.worker_confirmations=[{worker,method:'native_worker_reap'}];await client.submitCatalog(value);
    for(const proof of [
        {...full.proof,worker_confirmations:[]},
        {...full.proof,worker_confirmations:[{worker,method:'native_scope_ack'}]},
        {...full.proof,worker_obligation:{...obligation,index_id:'idx_'+'93'.repeat(16)}},
        {...full.proof,worker_obligation:{...obligation,providers:[]}},
        {...full.proof,worker_obligation:{...obligation,providers:[{worker,history:{kind:'helpers',generations:[]}}]}},
        {...full.proof,worker_obligation:{...obligation,providers:[{worker,history:{kind:'helpers',generations:['27'.repeat(16),'27'.repeat(16)]}}]}},
    ]){const original=full;full={...full,proof};await assert.rejects(client.submitCatalog(value));full=original;}
    const pending={status:'catalog_receipt',receipt:null,fence:{state:'pending',value:{schema:1,operation_id:f.metadata.operation_id,request:f.nativeRequest,preview_digest:digest,prepared_digest:f.begin.prepared_digest,plan:{service:'24'.repeat(16),gate_generation:1,targets:[],worker_obligation:obligation},stage:'prepared',confirmations:[],worker_confirmations:[]}}};
    assert.equal(client.validateCatalogObservation(pending,f.nativeRequest,requestDigest,digest),false);
    client.validateCatalogFenceContinuity(pending,{status:'catalog_receipt',receipt:f.metadata,fence:{state:'applied',value:full}});
    const crossed={...full,proof:{...full.proof,worker_obligation:{...obligation,providers:[{worker,history:{kind:'native_never_authorized'}}]}}};assert.throws(()=>client.validateCatalogFenceContinuity(pending,{status:'catalog_receipt',receipt:f.metadata,fence:{state:'applied',value:crossed}}),/crossed/);
});

test('source observation preserves failed job history while native completion resolves metadata using its distinct key',()=>{
    const f=nativeSourceFixture(),client=new ServiceClient({dispose(){},async request(){throw new Error('No effects on read validation');}}),id=f.metadata.operation_id;
    assert.equal(client.validateSourceObservation(f.observation,id,namespace),false);
    const completion={schema:1,operation_id:'31'.repeat(16),caller:f.nativeRequest.caller,key:{...request.key,nonce:'32'.repeat(16)},request_digest:'33'.repeat(32),catalog_revision:3,authority_fence_required:false,future_completion:'retained'};
    const resolved={status:'catalog_source_change_resolved',observation:{...f.observation.observation,completion,configuration_result:{...f.observation.observation.configuration_result,catalog_native_desired_resolution:{retained:true}}}};
    assert.equal(client.validateSourceObservation(resolved,id,namespace,f.observation),true);assert.equal(resolved.observation.job.state,'failed');assert.deepEqual(resolved.observation.job.outcome,{state:'failed',code:'disk_full'});
    assert.throws(()=>client.validateSourceObservation({...resolved,observation:{...resolved.observation,job:{...resolved.observation.job,id:digest}}},id,namespace,f.observation),/crossed_source_job/);
    assert.throws(()=>client.validateSourceObservation(f.observation,id,namespace,resolved),/continuity/);
    assert.match(catalogExport({generation:1,busy:false,sourceObservation:resolved}),/catalog_native_desired_resolution/);
});

test('pending source lookup uses only the selected native Index and preserves full results; None never settles an intent',async()=>{
    const f=nativeSourceFixture(),s=storage(),journal=new IntentJournal(s.adapter,'control',namespace),id=f.metadata.operation_id;
    await journal.prepare('Pending resolution',{catalog_source_change_resolve:{begin_operation:id},namespace},f.observation);
    let response:CatalogObject={status:'catalog_pending_source_change',index_id:index,observation:f.observation.observation,future_native_lookup:'retained'};const calls:unknown[]=[];
    const client=new ServiceClient({dispose(){},async request(method){calls.push(method);return response;}});
    assert.equal(await client.catalogPendingSource(index,namespace),response);assert.deepEqual(calls,[{catalog_pending_source_change:{index_id:index}}]);
    response={status:'catalog_pending_source_change',index_id:index,observation:null,future_native_lookup:'retained'};assert.equal(await client.catalogPendingSource(index,namespace),response);assert.equal(journal.list().length,1,'neutral no-current-observation cannot clear an unknown resolution');
    response={status:'catalog_pending_source_change',index_id:'idx_'+'94'.repeat(16),observation:null};await assert.rejects(client.catalogPendingSource(index,namespace),/crossed_pending_source_index/);
    response={status:'catalog_pending_source_change',index_id:index,observation:{...f.observation.observation,begin:{...f.begin,request:{...f.nativeRequest,operation:{...f.nativeRequest.operation,id:'idx_'+'95'.repeat(16)}}}}};await assert.rejects(client.catalogPendingSource(index,namespace),/crossed/);
});

test('source resolution journal pins original full begin proof once and never replays after an observation-less unknown',async()=>{
    const f=nativeSourceFixture(),s=storage(),journal=new IntentJournal(s.adapter,'control',namespace),id=f.metadata.operation_id;
    const large={...f.observation,observation:{...f.observation.observation,future_payload:'x'.repeat(43000)}};
    const intent={catalog_source_change_resolve:{begin_operation:id},namespace};await journal.prepare('Resolve failed source metadata',intent,large);
    const unknown={status:'source_change_resolution_outcome_unknown',code:'source_change_resolution_outcome_unknown',begin_operation:id,observation:null};await journal.observeCatalog(id,unknown);
    const saved=journal.list()[0];assert.deepEqual(saved.value.nativeSourceObservation,large);assert.deepEqual(saved.value.nativeObservation,unknown);assert.equal((JSON.stringify(saved).match(/future_payload/g)??[]).length,1);assert.ok(Buffer.byteLength(JSON.stringify(saved.value))<65536);
    const reopened=new IntentJournal(s.adapter,'control',namespace);let calls=0;const client=new ServiceClient({dispose(){},async request(){calls++;return unknown;}});await assert.rejects(reopened.submit(client,id),/source_resolution_requires_observation/);await reopened.reconcile(client,async()=>{});assert.equal(calls,0);
    await reopened.observeCatalog(id,large);assert.equal(reopened.list()[0].value.nativeSourceObservation,undefined);assert.deepEqual(reopened.list()[0].value.nativeObservation,large);
});

test('source controller uses native drafts, reads inertly and preserves failed history after explicit resolution',async()=>{
    const f=controller(),native=nativeSourceFixture();let applies=0,resolves=0,reads=0;
    f.control.catalogRead=async method=>'catalog_detail' in method?{status:'catalog_detail',detail:{namespace,revision:1,item:{kind:'index',value:{id:index,name:'Index',definition_revision:1,source_revision:1,backing:{binding:digest},tombstone:false,source_import:'complete'}}}}:page();
    f.control.catalogSourceDraft=async()=>validateNativeSourceDraft(native.response,{key:request.key,expected_catalog_revision:1,index_id:index,change:{operation:'add_root',alias:'extra',path:'/chosen/extra'}});
    f.control.catalogApply=async()=>{applies++;return {status:'catalog_applied',receipt:native.metadata,fenced_receipt:native.begin};};
    f.control.catalogSourceObservation=async()=>{reads++;return native.observation;};
    f.control.catalogPendingSource=async selected=>{assert.equal(selected,index);reads++;return {status:'catalog_pending_source_change',index_id:index,observation:native.observation.observation,future_lookup:'retained'};};
    const completion={schema:1,operation_id:'31'.repeat(16),caller:native.nativeRequest.caller,key:{...request.key,nonce:'32'.repeat(16)},request_digest:'33'.repeat(32),catalog_revision:3,authority_fence_required:false,future_completion:'retained'};
    f.control.catalogSourceResolve=async response=>{resolves++;return {status:'catalog_source_change_resolved',observation:{...response.observation as CatalogObject,completion}};};
    await f.value.page('indexes');await f.value.detail('index',index);await f.value.sourcePending(f.value.snapshot().generation);assert.equal(reads,1);assert.equal(f.value.snapshot().sourceObservation?.future_lookup,'retained');await f.value.sourceDraft({operation:'add_root',alias:'extra',path:'/chosen/extra'},f.value.snapshot().generation);assert.equal(applies,0);assert.equal(reads,1);assert.equal(resolves,0);
    await assert.rejects(f.value.preview(native.nativeRequest.operation,f.value.snapshot().generation),/native_source_draft_required/);
    await f.value.sourceDraft({operation:'add_root',alias:'extra',path:'/chosen/extra'},f.value.snapshot().generation);await f.value.apply(f.value.snapshot().generation);assert.equal(applies,1);assert.equal(resolves,0);
    await f.value.sourceRead(native.metadata.operation_id,f.value.snapshot().generation);assert.equal(reads,2);assert.equal(resolves,0);await f.value.sourceResolve(f.value.snapshot().generation);assert.equal(resolves,1);
    const exported=JSON.parse(catalogExport(f.value.snapshot()));assert.equal(exported.sourceObservation.observation.job.state,'failed');assert.equal(exported.sourceObservation.observation.job.future_job,'retained');assert.deepEqual(exported.sourceObservation.observation.completion,completion);
});
function storage(){
    const values=new Map<string,unknown>();
    const adapter:IntentStorage={keys:()=>[...values.keys()],get:<T>(key:string)=>values.get(key) as T|undefined,update:async(key,value)=>{if(value===undefined)values.delete(key);else values.set(key,structuredClone(value));}};
    return {values,adapter};
}
test('explicit retirement can supersede only native-qualified same-index tracking and local settlement preserves full exported proof',async()=>{
    const f=controller(),native=nativeSourceFixture(true),id='90'.repeat(16);let allowed=true,drafts=0,stopped=0;
    f.control.catalogRead=async()=>({status:'catalog_detail',detail:{namespace,revision:1,item:{kind:'index',value:{id:index,name:'Index',definition_revision:1,source_revision:1,backing:{binding:digest},tombstone:false}}}});
    f.control.catalogPending=()=>[id];f.control.catalogCanRetire=(chosen,backing,ns)=>allowed&&chosen===index&&backing===digest&&ns===namespace;
    f.control.catalogSourceDraft=async()=>{drafts++;return validateNativeSourceDraft(native.response,{key:request.key,expected_catalog_revision:1,index_id:index,change:{operation:'retire_index'}});};
    await f.value.detail('index',index);await f.value.sourceDraft({operation:'retire_index'},f.value.snapshot().generation);assert.equal(drafts,1);assert.deepEqual(f.control.catalogPending(),[id]);
    await assert.rejects(f.value.sourceDraft({operation:'add_root',alias:'extra',path:'/source'},f.value.snapshot().generation),/unconfirmed/);allowed=false;await assert.rejects(f.value.sourceDraft({operation:'retire_index'},f.value.snapshot().generation),/unconfirmed/);assert.equal(drafts,1);
    f.control.catalogRetirementReview=async selected=>({id:selected,response:{status:'catalog_index_retirement_evidence',index_id:index,evidence:{future_native_proof:'retained'}}});
    f.control.catalogStopTracking=async(review,valid)=>{assert.ok(valid());stopped++;return {local_tracking:'stopped_after_native_retirement',native_retirement_evidence:review.response};};
    await f.value.retirementRead(id,f.value.snapshot().generation);assert.equal(stopped,0);assert.equal(JSON.parse(catalogExport(f.value.snapshot())).retirementReview.response.evidence.future_native_proof,'retained');await f.value.stopTracking(f.value.snapshot().generation);assert.equal(stopped,1);assert.equal(f.value.snapshot().outcome?.local_tracking,'stopped_after_native_retirement');
    assert.ok(parseWebviewMessage({v:1,type:'indexing.retirement.read',requestId:'retire-read',id,generation:1}));assert.equal(parseWebviewMessage({v:1,type:'indexing.retirement.stopTracking',requestId:'retire-stop',generation:1,evidence:{}}),undefined);
});
function controller(initialDescriptor?:WindowContextDescriptor){
    let trusted=true,identity='owner';const calls:CatalogObject[]=[];
    let descriptor=initialDescriptor;
    const control:CatalogControl={catalogRead:async method=>{calls.push(method);return 'catalog_detail' in method?{status:'catalog_detail',detail:{namespace,revision:1,item:{kind:'project',value:projectValue}}}:page();},
        catalogPrepare:async()=>structuredClone(review),catalogApply:async()=>({status:'catalog_applied',receipt:{full:'receipt'}}),catalogReconcile:async()=>[],catalogPending:()=>[]};
    const value=new IndexingCatalogController({control,identity:()=>identity,authorized:()=>trusted,windowContext:()=>descriptor,changed(){}});
    return {value,control,calls,trust:(next:boolean)=>{trusted=next;},replace:()=>{identity+='next';},descriptor:(next:WindowContextDescriptor|undefined)=>{descriptor=next;}};
}
test('catalog UI preserves complete native pages, details, preview and exports',async()=>{
    const f=controller();const rich=page({future_receipt:{coverage:['all native fields'],readiness:'retained'},continuation:{kind:'projects',namespace,revision:1,after:project,index_id:null,project_id:null}});
    f.control.catalogRead=async method=>{f.calls.push(method);return 'catalog_detail' in method?{status:'catalog_detail',detail:{namespace,revision:1,item:{kind:'project',value:{...projectValue,native_provenance:{kept:true}}},additional_native_field:'retained'}}:rich;};
    await f.value.page('projects');
    assert.deepEqual(f.value.snapshot().page,rich);
    await f.value.page('projects',true);
    assert.deepEqual((f.calls[1].catalog_page as CatalogObject).request,{kind:'projects',index_id:null,project_id:null,after:(rich.page as CatalogObject).continuation,expected_catalog_revision:1,limit:16,max_bytes:49152});
    await f.value.detail('project',project);
    const generation=f.value.snapshot().generation;
    await f.value.preview(operation,generation);
    const exported=JSON.parse(catalogExport(f.value.snapshot()));
    assert.deepEqual(exported.page,rich);assert.deepEqual(exported.review.response,review.response);
    assert.deepEqual(exported.detail.detail.item.value.native_provenance,{kept:true});
    await assert.rejects(f.value.apply(generation-1),/stale_catalog_review/);
    assert.equal(f.value.snapshot().review,undefined);
});
test('stale catalog trust, client, namespace, revisions and continuation filters refuse late work',async()=>{
    for(const fence of ['trust','client'] as const){
        const f=controller();let release!:(value:CatalogObject)=>void;
        f.control.catalogRead=()=>new Promise(resolve=>{release=resolve;});
        const pending=f.value.page('projects');
        if(fence==='trust')f.trust(false);else f.replace();
        release(page());await assert.rejects(pending,/stale_catalog_view/);
        assert.equal(f.value.snapshot().page,undefined);
    }
    const f=controller();await f.value.page('projects');
    f.control.catalogRead=async()=>page({namespace:'99'.repeat(16)});
    await assert.rejects(f.value.page('projects'),/namespace_changed/);assert.equal(f.value.snapshot().page,undefined);
    const g=controller();await g.value.page('projects');
    g.control.catalogRead=async()=>({status:'catalog_detail',detail:{namespace,revision:2,item:null}});
    await assert.rejects(g.value.detail('project',project),/revision_changed/);
    const h=controller();h.control.catalogRead=async()=>page({continuation:{kind:'projects',namespace,revision:1,after:project,index_id:index,project_id:null}});
    await assert.rejects(h.value.page('projects'),/crossed_catalog_continuation/);
});
test('uncertain apply consumes the review and permits only receipt reconciliation',async()=>{
    const f=controller();let applies=0,receipts=0;
    f.control.catalogApply=async()=>{applies++;throw new ServiceError('console_transport_closed',true);};
    f.control.catalogReconcile=async()=>{receipts++;return [{status:'catalog_receipt',receipt:null}];};
    await f.value.page('projects');const generation=f.value.snapshot().generation;await f.value.preview(operation,generation);
    await assert.rejects(f.value.apply(generation),/console_transport_closed/);
    await assert.rejects(f.value.apply(generation),/stale_catalog_review/);
    await f.value.reconcile();assert.equal(applies,1);assert.equal(receipts,1);
    assert.deepEqual(f.value.snapshot().outcome,{receipts:[{status:'catalog_receipt',receipt:null}]});
});
test('shared journal persists the exact request and native digest and never replays catalog applies',async()=>{
    const s=storage(),journal=new IntentJournal(s.adapter,'/control',namespace);await journal.prepare('Rename Project',intent);
    assert.deepEqual(journal.list()[0].value.intent,intent);
    let dispatches=0;const client=new ServiceClient({dispose(){},async request(){dispatches++;return {status:'catalog_receipt',receipt:null};}});
    await assert.rejects(journal.submit(client,nonce),/catalog_apply_requires_receipt_reconciliation/);
    await journal.reconcile(client,async()=>{});
    assert.equal(dispatches,0,'routine job reconciliation cannot discard catalog outcomes');
    assert.equal(journal.list().length,1);
    assert.equal(JSON.stringify(journal.list()).includes('before'),false,'do not duplicate the native preview into the journal');
    assert.throws(()=>validateCatalogApply({catalog_apply:{...intent.catalog_apply,preview:{}}}),/fields/);
});
test('native applied receipt is complete, scoped and bounded through the actual ServiceClient adapter',async()=>{
    let submitted:unknown;const receipt={schema:1,operation_id:'90'.repeat(16),caller,key:request.key,request_digest:requestDigest,catalog_revision:2,authority_fence_required:false,extra_native_field:{kept:true}};
    const client=new ServiceClient({dispose(){},async request(method){submitted=method;return {status:'catalog_applied',receipt};}});
    assert.deepEqual(await client.submitCatalog(intent),{status:'catalog_applied',receipt});assert.deepEqual(submitted,{catalog_apply:intent.catalog_apply});
    const crossed=new ServiceClient({dispose(){},async request(){return {status:'catalog_applied',receipt:{...receipt,key:{...request.key,nonce:caller}}};}});
    await assert.rejects(crossed.submitCatalog(intent),/crossed_catalog_receipt/);
    const digestMismatch=new ServiceClient({dispose(){},async request(){return {status:'catalog_applied',receipt:{...receipt,request_digest:'99'.repeat(32)}};}});
    await assert.rejects(digestMismatch.submitCatalog(intent),/crossed_catalog_receipt/);
});
test('scope editor preserves all native atoms, rejects duplicates and matches Rust UTF-8/control boundaries',()=>{
    const atoms=[{source_root_id:source,kind:'subtree' as const,path:''},{source_root_id:'src_'+'91'.repeat(16),kind:'exact' as const,path:'a\n\u0085b'}];
    const draft=catalogMembershipDraft({index_id:index,revision:7,scope:atoms});
    assert.deepEqual(draft,{index,revision:'7',scope:atoms});assert.notEqual(draft.scope,atoms);assert.notEqual(draft.scope[0],atoms[0]);
    assert.deepEqual(validateCatalogScope(atoms),atoms);
    assert.throws(()=>validateCatalogScope([...atoms,{...atoms[0]}]),/duplicate_membership_scope/);
    assert.doesNotThrow(()=>validateCatalogScope([{source_root_id:source,kind:'exact',path:'é'.repeat(2048)}]));
    assert.throws(()=>validateCatalogScope([{source_root_id:source,kind:'exact',path:'é'.repeat(2049)}]),/invalid_membership_scope/);
    assert.doesNotThrow(()=>validateCatalogScope(Array.from({length:128},(_,i)=>({source_root_id:source,kind:'exact',path:String(i)}))));
    assert.throws(()=>validateCatalogScope(Array.from({length:129},(_,i)=>({source_root_id:source,kind:'exact',path:String(i)}))),/invalid_membership_scope/);
    for(const path of ['a\\b','a:b','a\0b','a//b','/a','a/','a/../b','./a'])assert.throws(()=>validateCatalogScope([{source_root_id:source,kind:'exact',path}]),/invalid_membership_scope/);
    for(const name of ['a\u0080b','a\u009fb','a\u007fb','a\nb'])assert.throws(()=>validateCatalogOperation({...operation,name}),/invalid_catalog_name/);
    assert.doesNotThrow(()=>validateCatalogOperation({...operation,name:'a\u2028b'}));
    assert.doesNotThrow(()=>validateCatalogOperation({...operation,name:'\ufeff'}),'Rust White_Space excludes the ECMAScript BOM');
});
test('catalog journal admits native-size requests within the shared 2 MiB and 64-record bounds',async()=>{
    const s=storage(),journal=new IntentJournal(s.adapter,'/control',namespace);
    const largeOperation={operation:'set_membership',expected_scope_revision:1,expected_membership_revision:null,membership:{project_id:project,index_id:index,revision:1,
        scope:Array.from({length:128},(_,i)=>({source_root_id:source,kind:'exact',path:`${'x'.repeat(240)}${i}`}))}};
    const large:CatalogApplyIntent={catalog_apply:{request:{...request,operation:largeOperation},preview_digest:digest},request_digest:digest};
    assert.ok(Buffer.byteLength(JSON.stringify(large))>32768);
    validateCatalogApply(large);
    let admitted=0;
    for(let i=1;i<=64;i++){
        const next=structuredClone(large);next.catalog_apply.request.key.nonce=i.toString(16).padStart(32,'0');
        try{await journal.prepare('Reviewed membership',next);admitted++;}catch(error){assert.match(String(error),/local_intent_byte_capacity_exceeded/);break;}
    }
    assert.ok(admitted>0&&admitted<64);
    assert.ok([...s.values.values()].reduce<number>((total,value)=>total+Buffer.byteLength(JSON.stringify(value)),0)<=2*1024*1024);
    const smallStorage=storage(),smallJournal=new IntentJournal(smallStorage.adapter,'/control',namespace);
    for(let i=1;i<=64;i++){const next=structuredClone(intent);next.catalog_apply.request.key.nonce=i.toString(16).padStart(32,'0');await smallJournal.prepare('Rename',next);}
    const over=structuredClone(intent);over.catalog_apply.request.key.nonce='ff'.repeat(16);
    await assert.rejects(smallJournal.prepare('Rename',over),/local_intent_capacity_exceeded/);
});
test('UI operation subset and serialized field catalogs agree with native Core contracts',()=>{
    const core=readFileSync(resolve(__dirname,'../../../..','crates/airk-core/src/catalog_management.rs'),'utf8');
    const snake=(name:string)=>name.replace(/[A-Z]/g,(letter,at)=>`${at?'_':''}${letter.toLowerCase()}`);
    const variants=[...core.slice(core.indexOf('pub enum CatalogOperation'),core.indexOf('pub struct CatalogRequest')).matchAll(/^    ([A-Z][A-Za-z]+) \{/gm)].map(match=>snake(match[1]));
    assert.ok(CATALOG_UI_OPERATIONS.every(value=>variants.includes(value)));
    const supported=[
        {operation:'create_project',id:project,name:'New'},operation,
        {operation:'delete_project',id:project,expected_definition_revision:1,expected_scope_revision:1,expected_defaults_revision:1},
        {operation:'rename_index',id:index,expected_definition_revision:1,name:'New'},
        {operation:'set_membership',expected_scope_revision:1,expected_membership_revision:null,membership:{project_id:project,index_id:index,revision:1,scope:[{source_root_id:source,kind:'subtree',path:''}]}},
        {operation:'remove_membership',project_id:project,index_id:index,expected_scope_revision:1,expected_membership_revision:1},
        {operation:'set_project_defaults',project_id:project,expected_defaults_revision:1,index_ids:[index]},
        {operation:'set_context',expected_revision:null,context:{id:'ctx_'+'11'.repeat(16),host:'22'.repeat(32),key:'33'.repeat(32),kind:'empty',name:'Empty window',revision:1,project_ids:[],index_ids:[]}},
        {operation:'delete_context',id:'ctx_'+'11'.repeat(16),expected_revision:1},
        nativeGrantOperation(),
        {operation:'revoke_grant',id:'grt_'+'11'.repeat(16),expected_revision:1},
        nativeSourceOperation(),
        {operation:'retire_index',id:index,expected_definition_revision:1,expected_source_revision:1,reason:{reason:'explicit'}},
    ];
    assert.deepEqual(supported.map(value=>value.operation).sort(),[...CATALOG_UI_OPERATIONS].sort());
    for(const value of supported){
        assert.deepEqual(validateCatalogOperation(value),value);assert.throws(()=>validateCatalogOperation({...value,unexpected:true}),/fields/);
        const nativeName=value.operation.split('_').map(part=>part[0].toUpperCase()+part.slice(1)).join('');
        const block=new RegExp(`    ${nativeName} \\{([\\s\\S]*?)^    \\}`, 'm').exec(core);
        assert.ok(block,nativeName);
        const nativeFields=[...block![1].matchAll(/^        ([a-z_]+):/gm)].map(match=>match[1]).sort();
        assert.deepEqual(Object.keys(value).filter(key=>key!=='operation').sort(),nativeFields,`complete ${nativeName} field catalog`);
    }
    assert.throws(()=>validateCatalogOperation({...operation,id:index}),/invalid_catalog_id/);
    assert.throws(()=>validateCatalogOperation({...operation,name:'é'.repeat(129)}),/invalid_catalog_name/);
    assert.throws(()=>validateCatalogOperation({...operation,expected_definition_revision:0}),/revision/);
    const service=readFileSync(resolve(__dirname,'../../../..','crates/airk-core/src/service.rs'),'utf8');
    for(const variant of ['CatalogPage','CatalogDetail','CatalogContext','CatalogPreview','CatalogApply','CatalogReceipt'])assert.match(service,new RegExp(`\\b${variant} \\{`));
    assert.deepEqual(parseRoute('#indexing/library'),{workspace:'indexing',tab:'library'});
    assert.deepEqual(parseRoute('#manage/projects'),{workspace:'manage',tab:'overview'});
    assert.ok(parseWebviewMessage({v:1,type:'indexing.page',requestId:'x',kind:'sources',next:false,index}));
    assert.equal(parseWebviewMessage({v:1,type:'indexing.page',requestId:'x',kind:'sources',next:false,index:project}),undefined);
    const contextKinds=['folder','workspace','empty','untitled','legacy_pending'];
    const nativeCatalog=readFileSync(resolve(__dirname,'../../../..','crates/airk-core/src/index_catalog.rs'),'utf8');
    const kindBlock=/pub enum ContextKind \{([\s\S]*?)^\}/m.exec(nativeCatalog);assert.ok(kindBlock);
    assert.deepEqual([...kindBlock![1].matchAll(/^    ([A-Z][A-Za-z]+),/gm)].map(match=>snake(match[1])),contextKinds);
    for(const kind of contextKinds){
        assert.doesNotThrow(()=>validateCatalogOperation({operation:'set_context',expected_revision:null,context:{id:'ctx_'+'11'.repeat(16),host:'22'.repeat(32),key:'33'.repeat(32),kind,name:'Context',revision:1,project_ids:[],index_ids:[]}}));
    }
    assert.throws(()=>validateCatalogOperation({operation:'set_context',expected_revision:null,context:{id:'ctx_'+'11'.repeat(16),host:'22'.repeat(32),key:'33'.repeat(32),kind:'empty',name:'Context',revision:2,project_ids:[],index_ids:[]}}),/next_context_revision/);
});

test('bounded grant browse/detail retains provenance and derives revocation only from current native detail',async()=>{
    const f=controller(),operation=nativeGrantOperation(),grant=operation.grant,wrapper={grant,provenance:{kind:'user_reviewed_project_grant'},future_native_wrapper:{retained:true}};let previews=0;
    f.control.catalogRead=async method=>'catalog_detail' in method?{status:'catalog_detail',detail:{namespace,revision:1,item:{kind:'grant',value:wrapper},future_detail:'retained'}}:{status:'catalog_page',page:{namespace,revision:1,items:[{kind:'grant',value:wrapper}],continuation:null,future_page:'retained'}};
    f.control.catalogPrepare=async op=>{previews++;assert.deepEqual(op,{operation:'revoke_grant',id:grant.id,expected_revision:1});return {...review,request:{...request,operation:op}};};
    await f.value.page('grants');assert.deepEqual(catalogItems(f.value.snapshot().grants)[0].value,wrapper);assert.equal(previews,0);
    await assert.rejects(f.value.revokeGrant(f.value.snapshot().generation),/inspect_current/);
    await f.value.detail('grant',grant.id);assert.match(catalogExport(f.value.snapshot()),/future_native_wrapper/);assert.equal(previews,0);
    await assert.rejects(f.value.preview({operation:'revoke_grant',id:grant.id,expected_revision:1},f.value.snapshot().generation),/inspect_native_grant/);
    await f.value.revokeGrant(f.value.snapshot().generation);assert.equal(previews,1);assert.equal(f.value.snapshot().review?.request.operation.operation,'revoke_grant');
});

test('window associations use only the injected host identity, explicit reads and reviewed zero-Project CRUD',async()=>{
    const descriptor:WindowContextDescriptor={host:'22'.repeat(32),key:'33'.repeat(32),kind:'empty',name:'Empty window'};
    const f=controller(descriptor);let context:CatalogObject|null=null,revision=1;const previews:CatalogObject[]=[];
    f.control.catalogRead=async method=>{f.calls.push(method);return {status:'catalog_detail',detail:{namespace,revision,item:context?{kind:'context',value:context}:null,native_detail_field:{preserved:true}}};};
    f.control.catalogPrepare=async(operation,expected)=>{previews.push(operation);return {...review,request:{...request,expected_catalog_revision:expected,operation},response:{...review.response,preview:{...review.response.preview as CatalogObject,catalog_revision:expected,operation}}};};
    f.control.catalogApply=async received=>{context={...catalogObjectForTest(received.request.operation.context),future_native_context_field:{retained:true}};revision++;return {status:'catalog_applied',receipt:{request_digest:received.requestDigest}};};
    assert.deepEqual(f.value.snapshot().windowDescriptor,descriptor);assert.equal(f.calls.length,0,'opening the screen cannot infer or register an association');
    await f.value.readContext();assert.deepEqual(f.calls,[{catalog_context:{host:descriptor.host,key:descriptor.key,expected_catalog_revision:null}}]);
    let generation=f.value.snapshot().generation;await f.value.previewContext({project_ids:[],index_ids:[]},generation);
    const created=previews[0].context as CatalogObject;
    assert.match(String(created.id),/^ctx_[0-9a-f]{32}$/);assert.equal(created.host,descriptor.host);assert.equal(created.key,descriptor.key);
    assert.deepEqual(created.project_ids,[]);assert.deepEqual(created.index_ids,[]);assert.equal(created.revision,1);
    await f.value.apply(generation);await f.value.readContext();
    assert.equal(((f.value.snapshot().context!.detail as CatalogObject).native_detail_field as CatalogObject).preserved,true);
    assert.match(catalogExport(f.value.snapshot()),/future_native_context_field/);
    generation=f.value.snapshot().generation;await f.value.previewContext({project_ids:[project],index_ids:[index]},generation);
    const updated=previews.at(-1)!.context as CatalogObject;assert.equal(updated.id,created.id);assert.equal(updated.revision,2);
    await f.value.previewContext({project_ids:[],index_ids:[]},generation,true);
    assert.deepEqual(previews.at(-1),{operation:'delete_context',id:created.id,expected_revision:1});
    const before=JSON.stringify(f.value.snapshot().context);
    await assert.rejects(f.value.previewContext({project_ids:[],index_ids:[index]},generation),/invalid_context_selection/);
    assert.equal(JSON.stringify(f.value.snapshot().context),before);
    await assert.rejects(f.value.preview({operation:'set_context',expected_revision:null,context:created},generation),/requires_host_review/);
    assert.equal(parseWebviewMessage({v:1,type:'indexing.context.read',requestId:'read',host:descriptor.host}),undefined);
    assert.equal(parseWebviewMessage({v:1,type:'indexing.context.preview',requestId:'review',generation,selection:{project_ids:[],index_ids:[]},key:descriptor.key}),undefined);
});
function catalogObjectForTest(input:unknown):CatalogObject {assert.ok(input&&typeof input==='object'&&!Array.isArray(input));return input as CatalogObject;}

test('unavailable or replaced window descriptors refuse association work and delayed reviews',async()=>{
    const unavailable=controller();await assert.rejects(unavailable.value.readContext(),/descriptor_unavailable/);assert.deepEqual(unavailable.calls,[]);
    const descriptor:WindowContextDescriptor={host:'22'.repeat(32),key:'33'.repeat(32),kind:'workspace',name:'Reviewed workspace'};
    const f=controller(descriptor);f.control.catalogRead=async()=>({status:'catalog_detail',detail:{namespace,revision:1,item:null}});
    await f.value.readContext();let release!:(value:CatalogReview)=>void;
    f.control.catalogPrepare=()=>new Promise(resolve=>{release=resolve;});
    const pending=f.value.previewContext({project_ids:[],index_ids:[]},f.value.snapshot().generation);
    f.descriptor({...descriptor,key:'44'.repeat(32)});f.value.snapshot();release(review);
    await assert.rejects(pending,/stale_catalog_view/);assert.equal(f.value.snapshot().context,undefined);assert.equal(f.value.snapshot().review,undefined);
});

test('context operations share exact durable intents and native receipt-only recovery',async()=>{
    const s=storage(),journal=new IntentJournal(s.adapter,'/control',namespace);
    const operation={operation:'set_context',expected_revision:null,context:{id:'ctx_'+'11'.repeat(16),host:'22'.repeat(32),key:'33'.repeat(32),kind:'empty',name:'Empty window',revision:1,project_ids:[],index_ids:[]}};
    const prepared:CatalogApplyIntent={...intent,catalog_apply:{...intent.catalog_apply,request:{...request,operation}}};
    await journal.prepare('Save association',prepared);assert.deepEqual(journal.list()[0].value.intent,prepared);
    const restored=new IntentJournal(s.adapter,'/control',namespace);assert.deepEqual(restored.list()[0].value.intent,prepared);
    let sent=0;const client=new ServiceClient({dispose(){},async request(){sent++;throw new Error('apply cannot replay');}});
    await assert.rejects(restored.submit(client,nonce),/requires_receipt_reconciliation/);assert.equal(sent,0);
});

test('native host context descriptors preserve complete results and enforce identity, schema and 16 KiB bounds',async()=>{
    const host='22'.repeat(32),key='33'.repeat(32),context={id:'ctx_'+'11'.repeat(16),host,key,kind:'empty',name:'Empty window',revision:2,project_ids:[],index_ids:[],future_context_field:{preserved:true}};
    const descriptor={schema:1,namespace,host,key,catalog_revision:3,context,future_descriptor_field:{full:['native','object']}};
    assert.equal(validateNativeHostContext(descriptor),descriptor);
    assert.doesNotThrow(()=>validateNativeHostContext({...descriptor,context:null}));
    for(const value of [{...descriptor,schema:2},{...descriptor,namespace:'bad'},{...descriptor,host:'bad'},{...descriptor,key:'bad'},
        {...descriptor,context:{...context,host:'44'.repeat(32)}},{...descriptor,context:{...context,key:'44'.repeat(32)}},
        {...descriptor,context:{...context,revision:4}},{...descriptor,catalog_revision:0},{...descriptor,future_descriptor_field:'x'.repeat(16384)}]){
        assert.throws(()=>validateNativeHostContext(value));
    }
    const response={status:'catalog_host_context',descriptor,extra_response_field:{preserved:true}},calls:unknown[]=[];
    const client=new ServiceClient({dispose(){},async request(method){calls.push(method);return calls.length===1?{status:'failure',code:'registry_busy'}:response;}});
    const complete=await client.catalogHostContext(key,3);assert.deepEqual(complete,response);
    assert.deepEqual(calls,[{catalog_host_context:{key,expected_catalog_revision:3}},{catalog_host_context:{key,expected_catalog_revision:3}}]);
    const copied=JSON.parse(catalogExport({generation:1,busy:false,outcome:complete}));assert.deepEqual(copied.outcome,response);
    for(const altered of [{...descriptor,key:'44'.repeat(32)},{...descriptor,catalog_revision:4}]){
        const invalid=new ServiceClient({dispose(){},async request(){return {status:'catalog_host_context',descriptor:{...altered,context:null}};}});
        await assert.rejects(invalid.catalogHostContext(key,3),/crossed_native_host_context/);
    }
});

test('Indexing adopts the exact host observation without a second RPC and reviews its original context revision',async()=>{
    const f=controller(),descriptor:WindowContextDescriptor={host:'22'.repeat(32),key:'33'.repeat(32),kind:'workspace',name:'Saved workspace'};
    const context={id:'ctx_'+'11'.repeat(16),host:descriptor.host,key:descriptor.key,kind:'workspace',name:'Earlier name',revision:2,project_ids:[],index_ids:[],future_context_field:{retained:true}};
    const observation={status:'catalog_host_context',descriptor:{schema:1,namespace,host:descriptor.host,key:descriptor.key,catalog_revision:3,context,future_native_field:{retained:true}},future_response_field:{retained:true}};
    const value=new IndexingCatalogController({control:f.control,identity:()=> 'current',authorized:()=>true,changed(){},windowContext:()=>descriptor,windowContextObservation:()=>observation});
    await value.adoptHostContext();assert.equal(f.calls.length,0);assert.deepEqual(value.snapshot().hostContext,observation);
    assert.deepEqual(JSON.parse(catalogExport(value.snapshot())).hostContext,observation);
    let operation:CatalogObject|undefined;f.control.catalogPrepare=async op=>{operation=op;return {...review,request:{...request,expected_catalog_revision:3,operation:op}};};
    await value.previewContext({project_ids:[],index_ids:[]},value.snapshot().generation);
    assert.deepEqual(operation,{operation:'set_context',expected_revision:2,context:{id:context.id,host:descriptor.host,key:descriptor.key,kind:descriptor.kind,name:descriptor.name,revision:3,project_ids:[],index_ids:[]}});
});

test('grant page/detail copies preserve the native persisted wrapper and provenance without creating grants',()=>{
    for(const imported of [false,true]){
        const grant={id:'grt_'+(imported?'f1':'01').repeat(16),host:'22'.repeat(32),revision:1,revoked:false,memberships:[{
            membership:{project_id:project,index_id:index,revision:1,scope:[{source_root_id:source,kind:'subtree',path:''}]},source_revision:1,policy_epoch:0,policy_digest:null,
        }],future_grant_field:{retained:true}};
        const provenance=imported?{kind:'imported_legacy_snapshot',upgrade_id:'11'.repeat(16),plan_digest:'22'.repeat(32),legacy_binding:'33'.repeat(32),configuration_digest:'44'.repeat(32),future_provenance_field:{retained:true}}
            :{kind:'user_reviewed_project_grant',future_provenance_field:{retained:true}};
        const value={grant,provenance,future_wrapper_field:{retained:true}},item={kind:'grant',value};
        const nativePage={status:'catalog_page',page:{namespace,revision:1,items:[item],continuation:null}};
        const detail={status:'catalog_detail',detail:{namespace,revision:1,item}};
        assert.equal(catalogItems(nativePage)[0].value,value);assert.equal(catalogItems(detail,'detail')[0].value,value);
        const copied=JSON.parse(catalogExport({generation:1,busy:false,page:nativePage,detail}));
        assert.deepEqual(copied.page,nativePage);assert.deepEqual(copied.detail,detail);
        assert.throws(()=>validateCatalogOperation({operation:'create_grant',grant}),'admin wrappers and future read-only fields cannot become frontend grant inputs');
    }
});

test('native grant drafts preserve exact native callers, previews and unknown fields without issuing access',async()=>{
    const selection={key:request.key,expected_catalog_revision:1,project_ids:[project],index_ids:[index]},response=nativeGrantResponse(),methods:unknown[]=[];
    const client=new ServiceClient({dispose(){},async request(method){methods.push(method);return response;}});
    const reviewed=await client.catalogGrantDraft('33'.repeat(32),selection);
    assert.equal(reviewed.request.caller,'9a'.repeat(16));assert.deepEqual(reviewed.response,response);assert.equal(reviewed.requestDigest,requestDigest);assert.equal(reviewed.previewDigest,digest);
    assert.deepEqual(methods,[{catalog_grant_draft:{context_key:'33'.repeat(32),selection}}]);
    for(const change of ['key','revision','operation','digest','subset'] as const){
        const changed=structuredClone(response);
        if(change==='key')changed.draft.request.key.nonce='99'.repeat(16);
        else if(change==='revision')changed.draft.request.expected_catalog_revision=2;
        else if(change==='operation')changed.draft.preview.operation={...nativeGrantOperation(),grant:{...nativeGrantOperation().grant,id:'grt_'+'22'.repeat(16)}};
        else if(change==='digest')changed.draft.preview.request_digest='bad';
        else changed.draft.request.operation.grant.memberships[0].membership.project_id='prj_'+'99'.repeat(16);
        assert.throws(()=>validateNativeGrantDraft(changed,selection),change);
    }
    for(const invalid of [{project_ids:[],index_ids:null},{project_ids:[project,project],index_ids:null},{project_ids:[project],index_ids:[]},{project_ids:[project],index_ids:[index,index]},
        {project_ids:Array.from({length:33},(_,i)=>`prj_${i.toString(16).padStart(32,'0')}`),index_ids:null},{project_ids:[project],index_ids:null,host:'invented'}])assert.throws(()=>validateGrantSelection(invalid));
    assert.deepEqual(validateGrantSelection({project_ids:[project],index_ids:null}),{project_ids:[project],index_ids:null});
});

test('Indexing grant review consumes only native drafts, preserves the submitted draft and never applies on read',async()=>{
    const descriptor:WindowContextDescriptor={host:'22'.repeat(32),key:'33'.repeat(32),kind:'empty',name:'Empty window'},f=controller();let drafts=0,applies=0;
    const response=nativeGrantResponse();
    f.control.catalogGrantDraft=async(key,selection,revision)=>{drafts++;assert.equal(key,descriptor.key);assert.equal(revision,1);return {...validateNativeGrantDraft(response,{key:request.key,expected_catalog_revision:1,...selection}),contextKey:key};};
    const uncertain={status:'catalog_outcome_unknown',code:'catalog_apply_outcome_unknown',caller:response.draft.request.caller,key:request.key,request_digest:requestDigest,future_unknown_field:{retained:true}};
    f.control.catalogApply=async()=>{applies++;throw new ServiceError('catalog_apply_outcome_unknown',true,uncertain);};
    const value=new IndexingCatalogController({control:f.control,identity:()=> 'current',authorized:()=>true,changed(){},windowContext:()=>descriptor,windowContextObservation:()=>({status:'catalog_host_context',descriptor:{schema:1,namespace,host:descriptor.host,key:descriptor.key,catalog_revision:1,context:null}})});
    await value.adoptHostContext();assert.equal(drafts,0);assert.equal(applies,0);
    const generation=value.snapshot().generation;await value.grantDraft({project_ids:[project],index_ids:[index]},generation);
    assert.deepEqual(value.snapshot().review?.response,response);assert.equal(drafts,1);assert.equal(applies,0);
    await assert.rejects(value.apply(generation),/catalog_apply_outcome_unknown/);assert.equal(applies,1);
    assert.deepEqual(JSON.parse(catalogExport(value.snapshot())).submittedReview.response,response);
    assert.deepEqual(value.snapshot().outcome,uncertain);
    await assert.rejects(value.apply(generation),/stale_catalog_review/);assert.equal(applies,1);
    await assert.rejects(value.preview(nativeGrantOperation(),generation),/native_grant_draft_required/);
});

test('late grant drafts are rejected after trust, caller ownership or context identity replacement',async()=>{
    for(const fence of ['trust','caller','context'] as const){
        let trusted=true,owner='one';let descriptor:WindowContextDescriptor={host:'22'.repeat(32),key:'33'.repeat(32),kind:'empty',name:'Empty window'};
        const f=controller();let finish!:(review:CatalogReview)=>void;
        f.control.catalogGrantDraft=()=>new Promise(resolve=>{finish=resolve;});
        const value=new IndexingCatalogController({control:f.control,identity:()=>owner,authorized:()=>trusted,changed(){},windowContext:()=>descriptor,windowContextObservation:()=>({status:'catalog_host_context',descriptor:{schema:1,namespace,host:descriptor.host,key:descriptor.key,catalog_revision:1,context:null}})});
        await value.adoptHostContext();const pending=value.grantDraft({project_ids:[project],index_ids:null},value.snapshot().generation);
        if(fence==='trust')trusted=false;else if(fence==='caller')owner='replaced';else descriptor={...descriptor,key:'44'.repeat(32)};
        const native=nativeGrantResponse();finish({...validateNativeGrantDraft(native,{key:request.key,expected_catalog_revision:1,project_ids:[project],index_ids:null}),contextKey:'33'.repeat(32)});
        await assert.rejects(pending,/stale_catalog_view/);assert.equal(value.snapshot().review,undefined);
    }
});

test('catalog fence wrappers bind exact requests and preserve complete native proof through journal restore',async()=>{
    const fencedRequest:CatalogRequest={...request,operation:{operation:'remove_membership',project_id:project,index_id:index,expected_scope_revision:1,expected_membership_revision:1}};
    const metadata={schema:1,operation_id:'21'.repeat(16),caller,key:request.key,request_digest:requestDigest,catalog_revision:2,authority_fence_required:true};
    const target={worker:{service:'22'.repeat(16),worker:'23'.repeat(16),project:'24'.repeat(32),fencing_generation:2},scope:{schema:1,lease_generation:3,index_id:index,grant_id:'grt_'+'11'.repeat(16),grant_revision:1,fingerprint:'25'.repeat(32)},future_target_field:{retained:true}};
    const full={request:fencedRequest,metadata,preview_digest:digest,prepared_digest:'27'.repeat(32),proof:{plan_digest:'26'.repeat(32),service:target.worker.service,gate_generation:7,confirmations:[{target,method:'native_scope_ack'}],future_proof_field:{retained:true}},future_fenced_field:{retained:true}};
    const applied={status:'catalog_applied',receipt:metadata,fenced_receipt:full,future_response_field:{retained:true}};
    const client=new ServiceClient({dispose(){},async request(){return applied;}});
    const fencedIntent:CatalogApplyIntent={catalog_apply:{request:fencedRequest,preview_digest:digest},request_digest:requestDigest};
    assert.deepEqual(await client.submitCatalog(fencedIntent),applied);
    const pending={status:'catalog_receipt',receipt:null,fence:{state:'pending',value:{schema:1,operation_id:metadata.operation_id,request:fencedRequest,preview_digest:digest,prepared_digest:full.prepared_digest,plan:{service:target.worker.service,gate_generation:7,targets:[target]},stage:'prepared',confirmations:[],future_intent_field:{retained:true}}}};
    const observed={status:'catalog_receipt',receipt:metadata,fence:{state:'applied',value:full}};
    assert.equal(client.validateCatalogObservation(pending,fencedRequest,requestDigest,digest),false);
    assert.equal(client.validateCatalogObservation(observed,fencedRequest,requestDigest,digest),true);client.validateCatalogFenceContinuity(pending,observed);
    const uncertain={status:'catalog_outcome_unknown',code:'catalog_apply_outcome_unknown',caller,key:request.key,request_digest:requestDigest,observation:{kind:'authority_fence',fence:pending.fence},future_unknown_field:{retained:true}};
    const unknownClient=new ServiceClient({dispose(){},async request(){return uncertain;}});
    await assert.rejects(unknownClient.submitCatalog(fencedIntent),(error:unknown)=>error instanceof ServiceError&&error.outcomeUnknown&&error.nativeResponse===uncertain);
    for(const changed of [{...uncertain,caller:'99'.repeat(16)},{...uncertain,request_digest:'99'.repeat(32)},{...uncertain,key:{...request.key,nonce:'99'.repeat(16)}},
        {...uncertain,observation:{kind:'index_setup',receipt:{}}}]){
        const foreign=new ServiceClient({dispose(){},async request(){return changed;}});
        await assert.rejects(foreign.submitCatalog(fencedIntent),(error:unknown)=>error instanceof ServiceError&&error.outcomeUnknown&&error.nativeResponse===undefined);
    }
    for(const mutation of ['request','metadata','digest','method','lease','plan','missing'] as const){
        const altered=structuredClone(applied);
        if(mutation==='request')altered.fenced_receipt.request={...fencedRequest,caller:'99'.repeat(16)};
        else if(mutation==='metadata')altered.fenced_receipt.metadata={...metadata,request_digest:'99'.repeat(32)};
        else if(mutation==='digest')altered.fenced_receipt.preview_digest='99'.repeat(32);
        else if(mutation==='method')altered.fenced_receipt.proof.confirmations[0].method='frontend_claim';
        else if(mutation==='lease')altered.fenced_receipt.proof.confirmations[0].target.scope.lease_generation=0;
        else if(mutation==='plan')altered.fenced_receipt.proof.plan_digest='bad';
        else (altered as any).fenced_receipt=undefined;
        assert.throws(()=>client.validateCatalogReceipt(altered,fencedRequest,requestDigest,digest),mutation);
    }
    const crossed=structuredClone(observed);crossed.fence.value.prepared_digest='99'.repeat(32);
    assert.throws(()=>client.validateCatalogFenceContinuity(pending,crossed),/crossed_catalog_fence_identity/);
    const incomplete=structuredClone(observed);incomplete.fence.value.proof.confirmations=[];
    assert.throws(()=>client.validateCatalogFenceContinuity(pending,incomplete),/crossed_catalog_fence_plan/);
    const s=storage(),journal=new IntentJournal(s.adapter,'/control',namespace);await journal.prepare('Reviewed removal',fencedIntent);
    await journal.observeCatalog(nonce,uncertain);assert.deepEqual(journal.list()[0].value.nativeObservation,uncertain);
    await journal.observeCatalog(nonce,pending);await journal.observeCatalog(nonce,observed);await journal.observeCatalog(nonce,{status:'catalog_receipt',receipt:null});
    const restored=new IntentJournal(s.adapter,'/control',namespace).list()[0].value;
    assert.deepEqual(restored.intent,fencedIntent);assert.deepEqual(restored.nativeFenceObservation,observed);assert.deepEqual(restored.nativeObservation,{status:'catalog_receipt',receipt:null});
    assert.match(catalogExport({generation:1,busy:false,pendingDetails:[{intent:restored.intent,native_fence_observation:restored.nativeFenceObservation}]}),/future_proof_field/);
});
