// SPDX-License-Identifier: Apache-2.0
// Synthetic Core-shaped consumer regressions; actual native/editor proof is separate.
import * as path from 'node:path';
import {InstalledHttpFailure} from './installedEditorHttp';
import {test} from 'node:test';import {strict as assert} from 'node:assert';
import {installedDeliveryFence,installedSelector,installedToolBody,installedMember,installedRevocation,finishInstalledClient,InstalledNativeReplyError,installedLegacyLifecycle} from './installedEditorContracts';
const id=(prefix:string,byte:string)=>prefix+byte.repeat(32),hash=(byte:string)=>byte.repeat(64);
function fixture(denied=false):any{
    const index=id('idx_','1'),project=id('prj_','2'),grant=id('grt_','3'),source=id('src_','4');
    const session={schema:1,context_key:hash('a'),namespace:'5'.repeat(32),expected_catalog_revision:2,grant_id:grant,expected_grant_revision:1,project_ids:[project],index_ids:[index]};
    const selection={index,project,grant,source_ids:[source],session};
    const publication={collection:'fixture',incarnation:'6'.repeat(32),generation:1,revision:hash('7'),token:'airk-r1:fixture-test-token'};
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
    return {selection,raw,body,definition,ui:{outcome:{status:'search_scope_selected',selected:{index_ids:[],project_ids:[project]},activation:{selected_subset:true,selection:{session}}}}};
}
test('complete current combined definition binds native reviewed singleton and rejects obsolete flag',()=>{
    const f=fixture();assert.deepEqual(installedSelector(f.definition,f.ui,f.selection),f.selection.session);
    assert.throws(()=>installedSelector({args:['--catalog-session',JSON.stringify(f.selection.session),'mcp']},f.ui,f.selection));
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
    assert.deepEqual(saved[0].native,[{transport:[{raw:'complete'}]}]);
    await assert.rejects(finishInstalledClient({observations:[],close:async()=>{throw cleanup;}},{native:[]},async()=>{}),error=>error===cleanup);
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
