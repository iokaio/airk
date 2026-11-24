// SPDX-License-Identifier: Apache-2.0
// Synthetic Core-shaped consumer regressions; actual native/editor proof is separate.
import * as path from 'node:path';
import {InstalledHttpFailure} from './installedEditorHttp';
import {test} from 'node:test';import {strict as assert} from 'node:assert';
import {installedDeliveryFence,installedToolBody,installedMember,installedRevocation,finishInstalledClient,InstalledNativeReplyError,installedSourceReview,installedPolicyReady,installedPublicationBusy} from './installedEditorContracts';
const id=(prefix:string,byte:string)=>prefix+byte.repeat(32),hash=(byte:string)=>byte.repeat(64);

const planWire=(body:any)=>({result:{isError:false,content:[{type:'text',text:JSON.stringify(body)}],structuredContent:structuredClone(body)}});
const planRequest=(output?:any)=>({schema:'airk.parse.request/1',mode:'execute',plan:{steps:[],...(output===undefined?{}:{output})}});
const declaredPlanBody=(wire:any,request:any)=>installedToolBody(wire,{tool:'airk_parse',request});
test('installed parse parity uses the original declared plan budget while ordinary reads stay bounded',()=>{
    const body={plan:{output:{max_bytes:262144}},steps:[{result:{payload:'x'.repeat(70000)}}]},wire=planWire(body);
    assert.throws(()=>installedToolBody(wire),/body bound/);
    assert.deepEqual(declaredPlanBody(wire,planRequest()),body);
    assert.deepEqual(declaredPlanBody(wire,{...planRequest({max_bytes:1024}),output:{}}),body,'Top-level output has canonical precedence, including its defaults');
    assert.throws(()=>declaredPlanBody(wire,planRequest({max_bytes:1024})));
});
test('installed parse parity enforces exact byte boundaries, declared range and returned budget identity',()=>{
    const body={plan:{output:{max_bytes:1024}},payload:''};body.payload='x'.repeat(1024-Buffer.byteLength(JSON.stringify(body)));
    assert.deepEqual(declaredPlanBody(planWire(body),planRequest({max_bytes:1024})),body);
    assert.throws(()=>declaredPlanBody(planWire({...body,payload:body.payload+'x'}),planRequest({max_bytes:1024})),/body bound/);
    for(const max_bytes of [0,1023,524289,1024.5,'262144',null])assert.throws(()=>declaredPlanBody(planWire(body),planRequest({max_bytes})));
    assert.throws(()=>declaredPlanBody(planWire(body),{...planRequest(),schema:'untrusted.schema'}));
    assert.throws(()=>declaredPlanBody(planWire(body),{...planRequest({max_bytes:1024}),output:{max_bytes:2048}}),'Response cannot substitute its own budget');
});
test('installed parse parity retains complete copies and the existing public wire bound',()=>{
    const body={plan:{output:{max_bytes:524288}},payload:'"'.repeat(240000)},wire=planWire(body);
    assert.ok(Buffer.byteLength(JSON.stringify(body))<524288);
    assert.throws(()=>declaredPlanBody(wire,planRequest({max_bytes:524288})),/wire bound/);
    const small=planWire({plan:{output:{max_bytes:262144}},payload:'exact'});
    small.result.structuredContent.payload='crossed';assert.throws(()=>declaredPlanBody(small,planRequest()),/copies agree/);
    assert.throws(()=>installedToolBody({...planWire({payload:'exact'}),extra:'x'.repeat(1048576)}),/wire bound/);
});
function policyFixture(){
    const index=id('idx_','a'),policy={index_id:index,owner_binding:hash('b'),epoch:1,roots:[{id:id('src_','c'),canonical_path:'/fixture',owner:true}],denies:[{path:'hidden'}]};
    const operation={operation:'change_index_policy',id:index,expected_source_revision:2,configuration:{configuration:{operation:'apply_source_policy',operation_id:'d'.repeat(32),policy}}};
    const observation={policyStatus:{status:'catalog_policy_status',index_id:index,snapshot:{policy:structuredClone(policy),operation_id:'d'.repeat(32),job:hash('e'),application_job:hash('e'),application:{operation_id:'d'.repeat(32),policy_digest:hash('f'),physical_cleanup_pending:true,future_native:{retained:true}}},job:{id:hash('e'),project:hash('b'),kind:'configure',state:'succeeded'}},
        indexDetail:{status:'catalog_detail',detail:{item:{kind:'index',value:{id:index,backing:{binding:hash('b')},source_revision:2,tombstone:false,source_import:'complete',policy:{epoch:1,digest:hash('f')}}}},future_native:{complete:true}}};
    return {index,operation,observation};
}

test('policy readiness observes original accepted job until matching application and retains complete raw replies',async()=>{
    const {index,operation,observation}=policyFixture(),pending=structuredClone(observation),finished=structuredClone(observation),saved:any[]=[];
    pending.policyStatus.job.state='running';
    for(const value of [pending,finished]){(value.policyStatus.snapshot as any).application=null;(value.policyStatus.snapshot as any).application_job=null;}
    const replies=[pending,finished,observation],before=structuredClone(replies);let reads=0;
    await installedPolicyReady(index,operation,async()=>replies[reads++],async values=>{saved.push(values);},Date.now()+2000);
    assert.equal(reads,3);assert.deepEqual(saved.at(-1),before);assert.deepEqual(replies,before);
    assert.equal(saved.at(-1)[2].policyStatus.snapshot.application.physical_cleanup_pending,true);
});

test('policy readiness refuses crossed identities, policy, receipt digest and terminal jobs without another read',async()=>{
    const mutations=[
        (o:any)=>o.policyStatus.index_id=id('idx_','0'),(o:any)=>o.indexDetail.detail.item.value.id=id('idx_','0'),
        (o:any)=>o.indexDetail.detail.item.value.backing.binding=hash('0'),(o:any)=>o.indexDetail.detail.item.value.source_revision++,
        (o:any)=>o.indexDetail.detail.item.value.tombstone=true,(o:any)=>o.indexDetail.detail.item.value.source_import='pending_metadata',
        (o:any)=>o.indexDetail.detail.item.value.policy.epoch++,(o:any)=>o.policyStatus.snapshot.operation_id='0'.repeat(32),
        (o:any)=>o.policyStatus.snapshot.policy.denies=[],(o:any)=>o.policyStatus.job.id=hash('0'),
        (o:any)=>o.policyStatus.job.project=hash('0'),(o:any)=>o.policyStatus.job.kind='index',
        (o:any)=>o.policyStatus.snapshot.application_job=hash('0'),(o:any)=>o.policyStatus.snapshot.application.operation_id='0'.repeat(32),
        (o:any)=>o.policyStatus.snapshot.application.policy_digest=hash('0'),
        (o:any)=>o.policyStatus.job.state='failed',(o:any)=>o.policyStatus.job.state='canceled'
    ];
    for(const mutate of mutations){
        const {index,operation,observation}=policyFixture();mutate(observation);let reads=0;const saved:any[]=[];
        await assert.rejects(installedPolicyReady(index,operation,async()=>{reads++;return observation;},async values=>{saved.push(values);},Date.now()+1000));
        assert.equal(reads,1);assert.deepEqual(saved.at(-1),[observation]);
    }
});

test('policy readiness pins the accepted job across observations and never retries a failed read',async()=>{
    const {index,operation,observation}=policyFixture(),pending=structuredClone(observation),crossed=structuredClone(observation);
    pending.policyStatus.job.state='pending';(pending.policyStatus.snapshot as any).application=null;(pending.policyStatus.snapshot as any).application_job=null;
    crossed.policyStatus.snapshot.job=hash('0');crossed.policyStatus.job.id=hash('0');crossed.policyStatus.snapshot.application_job=hash('0');let reads=0;
    await assert.rejects(installedPolicyReady(index,operation,async()=>++reads===1?pending:crossed,async()=>{},Date.now()+1000),/job changed/);assert.equal(reads,2);
    const changedDigest=structuredClone(observation);changedDigest.indexDetail.detail.item.value.policy.digest=hash('0');changedDigest.policyStatus.snapshot.application.policy_digest=hash('0');reads=0;
    await assert.rejects(installedPolicyReady(index,operation,async()=>++reads===1?pending:changedDigest,async()=>{},Date.now()+1000),/policy digest changed/);assert.equal(reads,2);
    const failure=new Error('unknown native read outcome');reads=0;
    await assert.rejects(installedPolicyReady(index,operation,async()=>{reads++;throw failure;},async()=>{},Date.now()+1000),error=>error===failure);assert.equal(reads,1);
});

test('policy readiness expires before the first read and retains empty observations',async()=>{
    const {index,operation}=policyFixture(),saved:any[]=[];let reads=0;
    await assert.rejects(installedPolicyReady(index,operation,async()=>{reads++;throw new Error('must not read');},async values=>{saved.push(values);},Date.now()-1),/deadline/);
    assert.equal(reads,0);assert.deepEqual(saved.at(-1),[]);
});

test('pure source review retains native busy, observes idle and prepares a new exact review',async()=>{
    const index=id('idx_','a'),order:string[]=[],saved:any[]=[];
    const busy={error:'Finish the current indexing work',outcome:{status:'source_change_busy',code:'source_change_jobs_pending',busy:{index_id:index,pending_jobs:1,running_jobs:0,future_detail:{retained:true}},future_native:{complete:true}}};
    const complete={review:{request:{key:{nonce:'new-pure-review'},operation:{operation:'change_index_policy',id:index}}}};
    let reads=0;
    const result=await installedSourceReview(index,async()=>{order.push('review');return ++reads===1?busy:complete;},async()=>{order.push('actual idle');},async attempts=>{saved.push(structuredClone(attempts));},Date.now()+1000);
    assert.equal(result,complete);assert.deepEqual(order,['review','actual idle','review']);
    assert.deepEqual(saved.at(-1),[{attempt:1,view:busy},{attempt:2,view:complete}]);
});

test('crossed, unknown and malformed source refusals never authorize another review',async()=>{
    const index=id('idx_','a'),base={error:'source refusal',outcome:{status:'source_change_busy',code:'source_change_jobs_pending',busy:{index_id:index,pending_jobs:1,running_jobs:0}}};
    const invalid:any[]=[{...base,outcome:{status:'failure',code:'registry_busy'}},{...base,outcome:{status:'failure',code:'catalog_apply_outcome_unknown'}},
        {...base,outcome:{...base.outcome,busy:{...base.outcome.busy,index_id:id('idx_','b')}}},{...base,review:{request:{operation:{operation:'change_index_policy',id:index}}}},
        {review:{request:{operation:{operation:'change_index_policy',id:id('idx_','b')}}}},
        {review:{request:{operation:{operation:'retire_index',id:index}}}}];
    for(const patch of [{pending_jobs:0},{pending_jobs:-1},{pending_jobs:4294967296},{running_jobs:0.5},{pending_jobs:'1'}])invalid.push({...base,outcome:{...base.outcome,busy:{...base.outcome.busy,...patch}}});
    for(const value of invalid){
        let reads=0,waits=0;const retained:any[]=[];
        await assert.rejects(installedSourceReview(index,async()=>{reads++;return value;},async()=>{waits++;},async attempts=>{retained.push(structuredClone(attempts));},Date.now()+1000));
        assert.equal(reads,1);assert.equal(waits,0);assert.deepEqual(retained.at(-1),[{attempt:1,view:value}]);
    }
    let reads=0,waits=0;const unknown=new Error('uncertain source review transport');
    await assert.rejects(installedSourceReview(index,async()=>{reads++;throw unknown;},async()=>{waits++;},async()=>{},Date.now()+1000),error=>error===unknown);
    assert.equal(reads,1);assert.equal(waits,0);
});

test('source review deadline and request budget do not permit extra reviews or Apply',async()=>{
    const index=id('idx_','a');let reads=0,waits=0;const saved:any[]=[];
    const review=async()=>{reads++;return {error:'busy',outcome:{status:'source_change_busy',code:'source_change_jobs_pending',busy:{index_id:index,pending_jobs:1,running_jobs:0}}};};
    await assert.rejects(installedSourceReview(index,review,async()=>{waits++;},async attempts=>{saved.push(attempts);},Date.now()-1),/deadline/);
    assert.equal(reads,0);assert.equal(waits,0);assert.deepEqual(saved.at(-1),[]);
    await assert.rejects(installedSourceReview(index,review,async()=>{waits++;},async attempts=>{saved.push(attempts);},Date.now()+1000),/request budget/);
    assert.equal(reads,3);assert.equal(waits,2);assert.equal(saved.at(-1).length,3);
});
function fixture(denied=false):any{
    const index=id('idx_','1'),project=id('prj_','2'),source=id('src_','4');
    const session={namespace:'5'.repeat(32),expected_catalog_revision:2,project_ids:[project],index_ids:[index]};
    const origin={owner:'window',scope_identity:hash('5'),catalog_revision:session.expected_catalog_revision};
    const backing={binding:hash('e'),root:path.resolve('fixture-source'),data_root:path.resolve('fixture-data'),collection:'fixture'};
    const selection={index,project,source:origin,source_ids:[source],session,backing};
    const publication={collection:'fixture',incarnation:'6'.repeat(32),generation:1,revision:hash('7'),token:`airk-r1:${'6'.repeat(32)}:1:${hash('7')}`};
    const observed={qualifier:'selected_subset',selection:structuredClone(origin),fingerprint:hash('8'),members:[{index_id:index,memberships:[{project_id:project}]}]};
    const body=denied?{error:{schema:1,code:'scoped_read_unavailable',retryable:false},scope_observation:observed}
        :{items:[{name:'Fixture',citation:{path:'fixture.ts',content_hash:hash('9'),start_line:1,end_line:1}}],publication,scope_observation:observed};
    const result={isError:denied,structuredContent:body,content:[{type:'text',text:JSON.stringify(body)}]};
    const raw={schema:'airk.combined-read/1',selection:{project_ids:[project],index_ids:[index],selection:structuredClone(origin),member_availability:'report',identity:hash('a')},
        publication_vector:{authority:hash('b'),observed_from_unix_ms:1,observed_to_unix_ms:2,members:[{index_id:index,publication:denied?null:publication}]},
        members:[{index_id:index,is_error:denied,allocation:{index_id:index,limit:20,max_bytes:65536,deadline_ms:30000,verification_max_sources:1,verification_max_bytes:65536,verification_deadline_ms:1},outcome:denied?'failed':'complete',result}],
        ranking:denied?[]:[{index_id:index,result_pointer:'/members/0/result/structuredContent/items/0',local_rank:1,rank_contribution:1/61,origin:{index_id:index,project_ids:[project],source_root_id:source,source_relative_path:'fixture.ts',publication}}],
        rank_policy:'fixture',partial:denied,truncated:false,delivery_observation:{receipt:{schema:1,namespace:session.namespace,catalog_revision:3,scope_identity:hash('b'),result_digest:hash('c')}}};
    return {selection,raw,body};
}
test('lossless member projection preserves raw body, floats and distinct identity domains',()=>{
    const f=fixture(),before=structuredClone(f.raw);assert.equal(installedMember(f.raw,f.selection).items.length,1);
    assert.deepEqual(f.raw,before);assert.notEqual(f.raw.selection.identity,f.raw.publication_vector.authority);
});
test('canonical status summary retains its exact token-only publication beside the complete member vector',()=>{
    const f=fixture(),publication=f.raw.publication_vector.members[0].publication;
    const body={collection:publication.collection,generation:publication.generation,index_state:'available',publication:{token:publication.token},scope_observation:f.body.scope_observation};
    f.raw.members[0].result={isError:false,structuredContent:body,content:[{type:'text',text:JSON.stringify(body)}]};f.raw.ranking=[];
    const untouched=structuredClone(f.raw);
    assert.deepEqual(installedMember(f.raw,f.selection),body);assert.deepEqual(f.raw,untouched);
    assert.deepEqual(Object.keys(f.raw.members[0].result.structuredContent.publication),['token'],'The native summary is never filled with invented fields');
    assert.equal(f.raw.publication_vector.members[0].publication.collection,f.selection.backing.collection);
});
test('publication comparisons reject crossed tokens, incomplete envelopes and internally inconsistent full identities',()=>{
    const f=fixture(),publication=f.raw.publication_vector.members[0].publication;
    const invalid=[null,{}, {token:'airk-r1:invalid'}, {token:publication.token,collection:publication.collection},
        {...publication,collection:'other'}, {...publication,incarnation:'0'.repeat(32)}, {...publication,generation:2}, {...publication,revision:hash('0')},
        {...publication,token:`airk-r1:${publication.incarnation}:2:${publication.revision}`}, {...publication,unexpected:true}];
    for(const actual of invalid){
        const value=fixture(),body={...value.body,publication:actual};
        value.raw.members[0].result={isError:false,structuredContent:body,content:[{type:'text',text:JSON.stringify(body)}]};
        assert.throws(()=>installedMember(value.raw,value.selection));
    }
    for(const mutate of [(p:any)=>p.token='airk-r1:invalid',(p:any)=>p.generation=2,(p:any)=>p.incarnation='0'.repeat(32),(p:any)=>p.revision=hash('0'),(p:any)=>p.collection='other']){
        const value=fixture();mutate(value.raw.publication_vector.members[0].publication);
        const body={...value.body,publication:{token:value.raw.publication_vector.members[0].publication.token}};
        value.raw.members[0].result={isError:false,structuredContent:body,content:[{type:'text',text:JSON.stringify(body)}]};value.raw.ranking=[];
        assert.throws(()=>installedMember(value.raw,value.selection),'A token-only member cannot hide a corrupt complete vector');
    }
    for(const patch of [{collection:'other'},{generation:2}]){
        const value=fixture(),body={...value.body,...patch,publication:{token:value.raw.publication_vector.members[0].publication.token}};
        value.raw.members[0].result={isError:false,structuredContent:body,content:[{type:'text',text:JSON.stringify(body)}]};
        assert.throws(()=>installedMember(value.raw,value.selection),'Summary scalars retain exact native publication identity');
    }
});
test('Report denied source is failed singleton with null publication, not outer-error success',()=>{
    const f=fixture(true);assert.equal(installedMember(f.raw,f.selection,true).error.code,'scoped_read_unavailable');
    assert.throws(()=>installedMember(f.raw,f.selection));const bad=structuredClone(f.raw);bad.members[0].is_error=false;assert.throws(()=>installedMember(bad,f.selection,true));
    const published=structuredClone(f.raw);published.publication_vector.members[0].publication={token:'not-a-denial-publication'};assert.throws(()=>installedMember(published,f.selection,true));
});
test('crossed allocation, unexpected member and wrong citation origin refuse',()=>{
    for(const mutation of [(x:any)=>{x.members[0].allocation.index_id=id('idx_','e');},(x:any)=>{x.members.push(x.members[0]);},(x:any)=>{x.ranking[0].origin.project_ids=[id('prj_','e')];},(x:any)=>{x.members[0].result.structuredContent.scope_observation.selection.catalog_revision=3;}]){
        const f=fixture();mutation(f.raw);assert.throws(()=>installedMember(f.raw,f.selection));
    }
});
test('outer flags and complete text/structured copies remain strict',()=>{
    const f=fixture(),wire={result:{isError:false,structuredContent:f.raw,content:[{type:'text',text:JSON.stringify(f.raw)}]}};
    assert.deepEqual(installedToolBody(wire),f.raw);
    assert.throws(()=>installedToolBody({result:{...wire.result,isError:'false'}}));
    assert.throws(()=>installedToolBody({result:{...wire.result,structuredContent:{different:true}}}));
    assert.throws(()=>installedToolBody({error:{code:1},result:wire.result}));
});
test('current revocation refuses before submission and never invents fresh brokerage authority',()=>{
    const f=fixture(),body={error:{code:'combined_scope_unavailable',query_submitted:false,retryable:false,query_replay_safe:false},requested_scope:{confirmed:false,selection:f.selection.session}};
    installedRevocation(body,f.selection.session);assert.throws(()=>installedRevocation({...body,error:{...body.error,retryable:true}},f.selection.session));
    assert.throws(()=>installedRevocation({...body,items:[]},f.selection.session));
});
test('manual legacy flat reply stays intact under transport decoding',()=>{
    const legacy={items:[],scope_observation:{provenance:'imported_legacy_snapshot'}};
    assert.deepEqual(installedToolBody({result:{content:[{type:'text',text:JSON.stringify(legacy)}]}}),legacy);
});
test('primary native failure survives cleanup failure with complete raw evidence retained',async()=>{
    const cleanup=new Error('owned close failed'),evidence:any={native:[],failure:{error:'primary query failed'}},saved:any[]=[];
    await finishInstalledClient({observations:[{raw:'complete'}],close:async()=>{throw cleanup;}},evidence,async(_name,value)=>{saved.push(structuredClone(value));});
    assert.equal(saved[0].failure.error,'primary query failed');assert.match(saved[0].transport_cleanup_failure,/owned close failed/);
    assert.deepEqual(saved[0].native,[{transport:[{raw:'complete'}],cleanup_failure:String(cleanup)}]);
    await assert.rejects(finishInstalledClient({observations:[],close:async()=>{throw cleanup;}},{native:[]},async()=>{}),error=>error===cleanup);
});
test('each closed native client retains its complete raw cohort and actual close receipt',async()=>{
    const evidence:any={native:[]},saved:any[]=[],first=[{raw:{complete:'first',future:[1,2]}}],second=[{raw:{complete:'second',future:{retained:true}}}];
    const firstReceipt={transport:'https',closed:true,requests:7,future_cleanup:{retained:true}},secondReceipt={transport:'https',closed:true,requests:3};
    let closes=0;
    const retain=async(_name:string,value:unknown)=>{saved.push(structuredClone(value));};
    await finishInstalledClient({observations:first,close:async()=>{closes++;return firstReceipt;}},evidence,retain);
    await finishInstalledClient({observations:second,close:async()=>{closes++;return secondReceipt;}},evidence,retain);
    assert.equal(closes,2);assert.deepEqual(saved.at(-1).native,[{transport:first,cleanup:firstReceipt},{transport:second,cleanup:secondReceipt}]);
    assert.equal(evidence.native[0].cleanup,firstReceipt);assert.equal(evidence.native[1].cleanup,secondReceipt);
    assert.equal(saved[0].native.length,1,'Each closure is durably retained before the next cohort starts');
});

test('plaintext native refusal retains complete raw envelope without a compatibility code',()=>{
    const reply={result:{isError:true,content:[{type:'text',text:'AIRK direct project connection failed.'}]}};
    let refusal:unknown;try{installedToolBody(reply);}catch(error){refusal=error;}
    assert.ok(refusal instanceof InstalledNativeReplyError);assert.equal(refusal.reply,reply);assert.equal(refusal.code,undefined);
    assert.throws(()=>installedToolBody({result:{...reply.result,isError:false}}),SyntaxError);
    assert.throws(()=>installedToolBody({result:{...reply.result,isError:'true'}}));
});

function completedDeletion(){const f=fixture(),review={request:{operation:{operation:'delete_project',id:f.selection.project},caller:'vscode',key:{nonce:'f'.repeat(32)},expected_catalog_revision:2},requestDigest:hash('6')};
    const applied={verifiedReceipt:{authority_fence_required:true,caller:review.request.caller,key:review.request.key,request_digest:review.requestDigest,catalog_revision:3},verifiedFencedReceipt:{proof:{native:'actual receipt kept by driver'}}};return {f,review,applied};}
test('completed exact native deletion plus empty TLS reset proves delivery fence without inventing admission',()=>{
    const {f,review,applied}=completedDeletion();installedDeliveryFence(new InstalledHttpFailure('network',{code:'ECONNRESET',response_bytes:0}),review,applied,f.selection.session);
    for(const error of [new Error('socket failure'),new InstalledHttpFailure('timeout',{response_bytes:0}),new InstalledHttpFailure('network',{code:'ECONNREFUSED',response_bytes:0}),new InstalledHttpFailure('network',{code:'ECONNRESET',response_bytes:12})])assert.throws(()=>installedDeliveryFence(error,review,applied,f.selection.session));
    for(const mutate of [(a:any)=>a.verifiedReceipt.authority_fence_required=false,(a:any)=>a.verifiedReceipt.request_digest=hash('0'),(a:any)=>a.verifiedReceipt.catalog_revision=2,(a:any)=>a.verifiedFencedReceipt.proof=null]){const bad=structuredClone(applied);mutate(bad);assert.throws(()=>installedDeliveryFence(new InstalledHttpFailure('network',{code:'ECONNRESET',response_bytes:0}),review,bad,f.selection.session));}
    assert.throws(()=>installedDeliveryFence(new InstalledHttpFailure('network',{code:'EPIPE',response_bytes:0}),{...review,request:{...review.request,operation:{operation:'delete_project',id:id('prj_','0')}}},applied,f.selection.session));
});
test('versioned native HTTP refusal keeps its complete body and requires matching deletion proof',()=>{
    const {f,review,applied}=completedDeletion();for(const [status,code]of [[403,'native_grant_refused'],[503,'native_delivery_fenced']] as const){const body={schema:'airk.local-api-error/1',error:{code},future:{retained:true}},failure=new InstalledHttpFailure('http',{status,response_bytes:100,body});installedDeliveryFence(failure,review,applied,f.selection.session);assert.equal(failure.detail.body,body);}
    assert.throws(()=>installedDeliveryFence(new InstalledHttpFailure('http',{status:500,response_bytes:10,body:{schema:'airk.local-api-error/1',error:{code:'native_delivery_fenced'}}}),review,applied,f.selection.session));
});
test('publication wait polls through only the exact typed report-mode publication_busy member',()=>{
    // Shape retained from the installed multi-scenario reply observed while an index was publishing.
    const busy=()=>({schema:'airk.combined-read/1',partial:true,truncated:false,ranking:[],continuation_unavailable:'member_failure_or_budget_cutoff',
        members:[{index_id:'idx_'+'15'.repeat(16),is_error:true,outcome:'failed',result:{action:'repeat_same_request',code:'publication_busy',retry_after_ms:25,retryable:true,status:'retry'}}]});
    assert.equal(installedPublicationBusy(busy()),true);
    for(const change of [(v:any)=>{v.members[0].result.code='source_denied';},(v:any)=>{v.members[0].result.retryable=false;},(v:any)=>{v.members[0].result.status='failure';},
        (v:any)=>{v.members[0].result.action='review_scope';},(v:any)=>{v.members[0].result.retry_after_ms=25.5;},(v:any)=>{v.members[0].outcome='complete';},(v:any)=>{v.members[0].is_error=false;},
        (v:any)=>{v.partial=false;},(v:any)=>{v.members.push(structuredClone(v.members[0]));},(v:any)=>{v.ranking=[{}];},(v:any)=>{v.schema='other';}]){
        const value=busy();change(value);assert.equal(installedPublicationBusy(value),false);
    }
    for(const value of [undefined,null,{},[],'publication_busy'])assert.equal(installedPublicationBusy(value),false);
    // Shape retained from the installed usage-scenario reply observed while the first publication replaced its projection.
    const expired=()=>{const value=busy() as any;value.members[0].result={collection:'fixture-a',error:{code:'source_projection_expired',query_replay_safe:false,query_submitted:true,retryable:false}};return value;};
    assert.equal(installedPublicationBusy(expired()),true);
    for(const change of [(v:any)=>{v.members[0].result.error.code='source_denied';},(v:any)=>{v.members[0].result.error.retryable=true;},(v:any)=>{v.members[0].result.error.query_submitted=false;},
        (v:any)=>{delete v.members[0].result.error;},(v:any)=>{v.members[0].outcome='complete';}]){const value=expired();change(value);assert.equal(installedPublicationBusy(value),false);}
});
