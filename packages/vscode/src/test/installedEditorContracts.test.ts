// SPDX-License-Identifier: Apache-2.0
// Synthetic Core-shaped consumer regressions; actual native/editor proof is separate.
import * as path from 'node:path';
import {reviewCombinedScopeSelection} from '../scopeSelection';
import {InstalledHttpFailure} from './installedEditorHttp';
import {test} from 'node:test';import {strict as assert} from 'node:assert';
import {installedDeliveryFence,installedSelector,installedToolBody,installedMember,installedRevocation,finishInstalledClient,InstalledNativeReplyError,installedLegacyLifecycle,installedSourceReview,installedPolicyReady,installedRestoredDefinition,installedDefinitionContinuity} from './installedEditorContracts';
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
test('both actual installed Workbench parse transport callers supply their original request budget',()=>{
    const source=require('node:fs').readFileSync(path.resolve(__dirname,'../../src/test/installedEditor.ts'),'utf8');
    assert.ok(/installedToolBody\(wire,\{tool:'airk_parse',request:turn\.request\}\)/.test(source),'Actual MCP parse caller supplies the original request');
    assert.ok(/installedToolBody\(\{result:cliEnvelope\},\{tool:'airk_parse',request:turn\.request\}\)/.test(source),'Actual CLI parse caller supplies the original request');
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
function fixture(denied=false,wholeSet=false):any{
    const index=id('idx_','1'),project=id('prj_','2'),grant=id('grt_','3'),source=id('src_','4');
    const session={schema:1,context_key:hash('a'),namespace:'5'.repeat(32),expected_catalog_revision:2,grant_id:grant,expected_grant_revision:1,project_ids:[project],...(wholeSet?{}:{index_ids:[index]})};
    const backing={binding:hash('e'),root:path.resolve('fixture-source'),data_root:path.resolve('fixture-data'),collection:'fixture'};
    const selection={index,project,grant,source_ids:[source],session,backing};
    const publication={collection:'fixture',incarnation:'6'.repeat(32),generation:1,revision:hash('7'),token:`airk-r1:${'6'.repeat(32)}:1:${hash('7')}`};
    const observed={qualifier:'selected_subset',provenance:'user_reviewed_project_grant',grant:{id:grant,revision:1},fingerprint:hash('8'),members:[{index_id:index,memberships:[{project_id:project}]}]};
    const body=denied?{error:{schema:1,code:'scoped_read_unavailable',retryable:false},scope_observation:observed}
        :{items:[{name:'Fixture',citation:{path:'fixture.ts',content_hash:hash('9'),start_line:1,end_line:1}}],publication,scope_observation:observed};
    const result={isError:denied,structuredContent:body,content:[{type:'text',text:JSON.stringify(body)}]};
    const raw={schema:'airk.combined-read/1',selection:{project_ids:[project],index_ids:[index],grant_id:grant,grant_revision:1,member_availability:'report',identity:hash('a')},
        publication_vector:{authority:hash('b'),observed_from_unix_ms:1,observed_to_unix_ms:2,members:[{index_id:index,publication:denied?null:publication}]},
        members:[{index_id:index,is_error:denied,allocation:{index_id:index,limit:20,max_bytes:65536,deadline_ms:30000,verification_max_sources:1,verification_max_bytes:65536,verification_deadline_ms:1},outcome:denied?'failed':'complete',result}],
        ranking:denied?[]:[{index_id:index,result_pointer:'/members/0/result/structuredContent/items/0',local_rank:1,rank_contribution:1/61,origin:{index_id:index,project_ids:[project],source_root_id:source,source_relative_path:'fixture.ts',publication}}],
        rank_policy:'fixture',partial:denied,truncated:false,delivery_observation:{receipt:{schema:1,namespace:session.namespace,catalog_revision:3,scope_identity:hash('b'),result_digest:hash('c')}}};
    const directory=path.resolve('fictional-native-private'),connection_id='f'.repeat(32);
    const nativeConnection={schema:1,connection_id,endpoint:{schema:1,installation:hash('1'),user:'fixture',namespace:session.namespace,generation:1,service_instance:'2'.repeat(32),executable_sha256:hash('3'),url:'https://127.0.0.1:57421/mcp',ca_sha256:hash('4'),certificate_sha256:hash('5'),state:'ready',predecessor_digest:null},selection:{kind:'combined',selection:session},ca_certificate_file:path.join(directory,'ca.pem'),credential_file:path.join(directory,'credential-'+connection_id+'.token')};
    const definition={uri:{toString:()=>nativeConnection.endpoint.url},headers:{Authorization:'Bearer airk1.'+connection_id+'.'+hash('a'),'X-AIRK-Response-Mode':'full'},nativeConnection};
    const host=hash('d'),descriptor={schema:1,namespace:session.namespace,host,key:session.context_key,catalog_revision:session.expected_catalog_revision,context:null};
    const detail=(kind:string,value:any)=>({status:'catalog_detail',detail:{namespace:session.namespace,revision:session.expected_catalog_revision,item:{kind,value}}});
    const grantDetail=detail('grant',{grant:{id:grant,host,revision:1,revoked:false,memberships:[{membership:{project_id:project,index_id:index,revision:1,scope:[{source_root_id:source,kind:'subtree',path:''}]},source_revision:1,policy_epoch:0,policy_digest:null}]},provenance:{kind:'user_reviewed_project_grant'}});
    const indexDetail=detail('index',{id:index,name:'Fixture index',backing,definition_revision:1,source_revision:1,policy:null,tombstone:false,source_import:'complete'});
    const reviewed=reviewCombinedScopeSelection({grant_id:grant,expected_grant_revision:1,project_ids:[project],...(wholeSet?{}:{index_ids:[index]})},descriptor,grantDetail.detail,[indexDetail.detail]);
    return {selection,raw,body,definition,ui:{outcome:{status:'search_scope_selected',selected:{index_ids:[],project_ids:[project]},activation:{selected_subset:true,selection:reviewed,host_context:{status:'catalog_host_context',descriptor},grant_detail:grantDetail,index_details:[indexDetail]}}}};
}
test('complete current combined definition binds native reviewed singleton and rejects obsolete flag',()=>{
    const f=fixture();assert.deepEqual(installedSelector(f.definition,f.ui,f.selection),f.selection.session);
    assert.throws(()=>installedSelector({args:['--catalog-session',JSON.stringify(f.selection.session),'mcp']},f.ui,f.selection));
});
test('restored definition observes pending then exact original authority and returns the untouched actual definition',async()=>{
    const f=fixture(false,true),original=structuredClone(f.selection.session),saved:any[]=[],pauses:number[]=[];let time=100,reads=0;
    f.definition.nativeConnection=structuredClone(f.definition.nativeConnection);
    f.definition.nativeConnection.selection.selection.expected_catalog_revision++;
    const actual=await installedRestoredDefinition(async()=>++reads===1?[]:[f.definition],original,async rows=>{saved.push(rows);},10000,{now:()=>time,pause:async ms=>{pauses.push(ms);time+=ms;}});
    assert.equal(actual,f.definition);assert.equal(reads,2);assert.deepEqual(pauses,[250]);assert.deepEqual(f.selection.session,original);
    const retained=saved.at(-1);assert.equal(retained.length,2);assert.equal(retained[0].count,0);assert.equal(retained[1].count,1);
    assert.equal(retained[1].selector.expected_catalog_revision,original.expected_catalog_revision+1);assert.equal(Object.hasOwn(retained[1].selector,'index_ids'),false);
    assert.ok(!/Authorization|Bearer|credential|headers/.test(JSON.stringify(retained)),'Retained observations contain no transport credential material');
});
test('first restored definition refuses crossed authority, backward clock, malformed input or multiple candidates without another read',async()=>{
    const mutations=[(s:any)=>s.namespace='0'.repeat(32),(s:any)=>s.context_key=hash('0'),(s:any)=>s.grant_id=id('grt_','0'),(s:any)=>s.expected_grant_revision++,
        (s:any)=>s.project_ids=[id('prj_','0')],(s:any)=>s.index_ids=[id('idx_','0')],(s:any)=>s.expected_catalog_revision--];
    const cases:any[]=[];
    for(const mutate of mutations){const f=fixture(false,true),connection=structuredClone(f.definition.nativeConnection);mutate(connection.selection.selection);cases.push({expected:f.selection.session,definitions:[{...f.definition,nativeConnection:connection}]});}
    const f=fixture(false,true);cases.push({expected:f.selection.session,definitions:[f.definition,f.definition]},{expected:f.selection.session,definitions:[{}]},{expected:f.selection.session,definitions:null});
    for(const candidate of cases){let reads=0,waits=0;await assert.rejects(installedRestoredDefinition(async()=>{reads++;return candidate.definitions;},candidate.expected,async()=>{},10000,{now:()=>0,pause:async()=>{waits++;}}));assert.equal(reads,1);assert.equal(waits,0);}
    const failure=Error('unknown definitions transport outcome');let reads=0;
    await assert.rejects(installedRestoredDefinition(async()=>{reads++;throw failure;},f.selection.session,async()=>{},10000,{now:()=>0}),error=>error===failure);assert.equal(reads,1);
});
test('restored definition keeps the original deadline and observation budget without dispatching selection work',async()=>{
    const f=fixture(false,true),saved:any[]=[];let time=1000,reads=0,waits=0;
    const read=async()=>{reads++;return [];},retain=async(rows:any[])=>{saved.push(rows);};
    await assert.rejects(installedRestoredDefinition(read,f.selection.session,retain,999,{now:()=>time}),/deadline/);assert.equal(reads,0);assert.deepEqual(saved.at(-1),[]);
    time=0;await assert.rejects(installedRestoredDefinition(async()=>{reads++;time=1001;return [];},f.selection.session,retain,1000,{now:()=>time}),/deadline/);assert.equal(reads,1);
    time=0;reads=0;
    await assert.rejects(installedRestoredDefinition(read,f.selection.session,retain,200000,{now:()=>time,pause:async ms=>{time+=ms;waits++;}}),/observation budget/);
    assert.equal(reads,128);assert.equal(waits,127);assert.equal(saved.at(-1).length,128);
    time=0;reads=0;waits=0;
    await assert.rejects(installedRestoredDefinition(read,f.selection.session,retain,200000,{now:()=>time,pause:async()=>{time=90000;waits++;}}),/deadline/);
    assert.equal(reads,1);assert.equal(waits,1,'Ninety-second ceiling cannot renew the caller deadline');
});
test('whole-set selector omits index restriction and proves the exact native frozen effective scope without rewriting it',()=>{
    const f=fixture(false,true),before=structuredClone(f.ui),nativeBefore=structuredClone(f.definition.nativeConnection);
    const selector=installedSelector(f.definition,f.ui,f.selection);
    assert.equal(Object.hasOwn(selector,'index_ids'),false);assert.deepEqual(selector,f.selection.session);
    assert.deepEqual(f.ui,before);assert.deepEqual(f.definition.nativeConnection,nativeBefore);
    assert.equal(installedMember(f.raw,f.selection).items.length,1,'Canonical delivery separately expands effective index IDs');
});
test('whole-set proof rejects extra effective projects or indexes, crossed grant metadata and changed source boundaries',()=>{
    const changes=[
        (a:any)=>a.selection.indexes.push({...a.selection.indexes[0],id:id('idx_','e')}),
        (a:any)=>a.selection.session.project_ids.push(id('prj_','e')),
        (a:any)=>a.index_details.push(structuredClone(a.index_details[0])),
        (a:any)=>a.index_details[0].detail.item.value.id=id('idx_','e'),
        (a:any)=>a.index_details[0].detail.item.value.backing.binding=hash('0'),
        (a:any)=>a.index_details[0].detail.item.value.source_revision=2,
        (a:any)=>a.index_details[0].detail.item.value.policy={epoch:1,digest:hash('0')},
        (a:any)=>a.grant_detail.detail.item.value.grant.id=id('grt_','0'),
        (a:any)=>a.grant_detail.detail.item.value.grant.memberships[0].membership.project_id=id('prj_','e'),
        (a:any)=>a.grant_detail.detail.item.value.grant.memberships[0].membership.index_id=id('idx_','e'),
        (a:any)=>a.grant_detail.detail.item.value.grant.memberships.push(structuredClone(a.grant_detail.detail.item.value.grant.memberships[0])),
        (a:any)=>{const extra=structuredClone(a.grant_detail.detail.item.value.grant.memberships[0]);extra.membership.index_id=id('idx_','e');a.grant_detail.detail.item.value.grant.memberships.push(extra);},
        (a:any)=>{const extra=structuredClone(a.grant_detail.detail.item.value.grant.memberships[0]);extra.membership.project_id=id('prj_','e');a.grant_detail.detail.item.value.grant.memberships.push(extra);},
        (a:any)=>a.grant_detail.detail.item.value.grant.memberships[0].membership.scope[0].path='narrower',
        (a:any)=>a.grant_detail.detail.item.value.grant.memberships[0].membership.scope.push({source_root_id:id('src_','e'),kind:'subtree',path:''})
    ];
    for(const change of changes){const f=fixture(false,true);change(f.ui.outcome.activation);assert.throws(()=>installedSelector(f.definition,f.ui,f.selection));}
});
test('crossed index, Project and native session refuse',()=>{
    for(const field of ['index_ids','project_ids','context_key']){
        const f=fixture(),bad=structuredClone(f.selection.session);bad[field]=field==='context_key'?hash('d'):[id(field==='index_ids'?'idx_':'prj_','e')];
        assert.throws(()=>installedSelector({...f.definition,nativeConnection:{...f.definition.nativeConnection,selection:{kind:'combined',selection:bad}}},f.ui,f.selection));
    }
});
test('current checkbox activation retains exact visible choice and reviewed native grant',()=>{
    const f=fixture();assert.deepEqual(installedSelector(f.definition,f.ui,f.selection),f.selection.session);
    for(const mutate of [(ui:any)=>{ui.outcome.selected.project_ids=[];},(ui:any)=>{ui.outcome.selected.index_ids=[f.selection.index];},(ui:any)=>{ui.outcome.activation.selected_subset=false;},(ui:any)=>{ui.outcome.activation.selection.session.grant_id=id('grt_','f');}]){
        const ui=structuredClone(f.ui);mutate(ui);assert.throws(()=>installedSelector(f.definition,ui,f.selection));
    }
});

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
    for(const mutation of [(x:any)=>{x.members[0].allocation.index_id=id('idx_','e');},(x:any)=>{x.members.push(x.members[0]);},(x:any)=>{x.ranking[0].origin.project_ids=[id('prj_','e')];},(x:any)=>{x.members[0].result.structuredContent.scope_observation.grant.revision=2;}]){
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

function safeDefinitionFixture(){
    const f=fixture(false,true);return {transport:'https',uri:f.definition.nativeConnection.endpoint.url,headers:{Authorization:'Bearer [redacted]','X-AIRK-Response-Mode':'full'},credential_id:f.definition.nativeConnection.connection_id,nativeConnection:structuredClone(f.definition.nativeConnection),version:'1.8.14:fixture:1:full',future_backend:{retained:true}};
}
test('storage definition continuity accepts only a forward catalog clock and leaves both complete safe snapshots untouched',()=>{
    const before=safeDefinitionFixture(),after=structuredClone(before);after.nativeConnection.selection.selection.expected_catalog_revision++;
    const originalBefore=structuredClone(before),originalAfter=structuredClone(after);
    installedDefinitionContinuity(before,after);assert.deepEqual(before,originalBefore);assert.deepEqual(after,originalAfter);
});
test('storage definition continuity refuses crossed authority, clock, endpoint, credential, version and private/backend fields',()=>{
    const mutations=[
        (d:any)=>d.nativeConnection.selection.selection.expected_catalog_revision--,
        (d:any)=>d.nativeConnection.selection.selection.grant_id=id('grt_','0'),
        (d:any)=>d.nativeConnection.selection.selection.project_ids=[id('prj_','0')],
        (d:any)=>d.nativeConnection.selection.selection.index_ids=[id('idx_','0')],
        (d:any)=>d.nativeConnection.endpoint.url='https://127.0.0.1:57422/mcp',
        (d:any)=>d.nativeConnection.endpoint.generation++,
        (d:any)=>d.nativeConnection.connection_id='0'.repeat(32),
        (d:any)=>d.credential_id='0'.repeat(32),
        (d:any)=>d.version+='-changed',
        (d:any)=>d.headers['X-AIRK-Response-Mode']='terse',
        (d:any)=>d.nativeConnection.ca_certificate_file=path.resolve('other-private','ca.pem'),
        (d:any)=>d.nativeConnection.credential_file=path.resolve('other-private','credential-'+'f'.repeat(32)+'.token'),
        (d:any)=>d.future_backend.retained=false,
        (d:any)=>d.backing={root:path.resolve('other-backing')}
    ];
    for(const mutate of mutations){const before=safeDefinitionFixture(),after=structuredClone(before);mutate(after);assert.throws(()=>installedDefinitionContinuity(before,after));}
});

test('plaintext native refusal retains complete raw envelope without a compatibility code',()=>{
    const reply={result:{isError:true,content:[{type:'text',text:'AIRK direct project connection failed.'}]}};
    let refusal:unknown;try{installedToolBody(reply);}catch(error){refusal=error;}
    assert.ok(refusal instanceof InstalledNativeReplyError);assert.equal(refusal.reply,reply);assert.equal(refusal.code,undefined);
    assert.throws(()=>installedToolBody({result:{...reply.result,isError:false}}),SyntaxError);
    assert.throws(()=>installedToolBody({result:{...reply.result,isError:'true'}}));
});
test('legacy lifecycle explicitly starts once and closes the manual read before one Stop',async()=>{
    const order:string[]=[],evidence:any={};
    await installedLegacyLifecycle(async()=>{order.push('start');},async()=>{order.push('read');order.push('transport closed');},async()=>{order.push('stop');},evidence,async()=>{});
    assert.deepEqual(order,['start','read','transport closed','stop']);assert.deepEqual(evidence,{});
});
test('failed legacy read remains primary when its single Stop also fails',async()=>{
    const primary=new Error('manual read failed'),stopFailure=new Error('Stop unconfirmed'),order:string[]=[],evidence:any={},saved:any[]=[];
    await assert.rejects(installedLegacyLifecycle(async()=>{order.push('start');},async()=>{order.push('read closed');throw primary;},async()=>{order.push('stop');throw stopFailure;},evidence,async(_name,value)=>{saved.push(structuredClone(value));}),error=>error===primary);
    assert.deepEqual(order,['start','read closed','stop']);assert.equal(saved[0].legacy_stop_failure.primary_preserved,true);
    assert.match(saved[0].legacy_stop_failure.error,/Stop unconfirmed/);
    await assert.rejects(installedLegacyLifecycle(async()=>{},async()=>{},async()=>{throw stopFailure;},{},async()=>{}),error=>error===stopFailure);
});

test('legacy failure identity survives failure to retain the Stop failure',async()=>{
    const primary=new Error('original manual failure'),stopFailure=new Error('exact Stop failure'),retention=new Error('retention unavailable '.repeat(500)),evidence:any={};
    const failMark=async()=>{throw retention;};
    await assert.rejects(installedLegacyLifecycle(async()=>{},async()=>{throw primary;},async()=>{throw stopFailure;},evidence,failMark),error=>error===primary);
    assert.equal(evidence.legacy_stop_failure.primary_preserved,true);assert.ok(evidence.legacy_stop_retention_failure.length<=4096);
    await assert.rejects(installedLegacyLifecycle(async()=>{},async()=>{},async()=>{throw stopFailure;},{},failMark),error=>error===stopFailure);
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
