// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import * as path from 'node:path';
import { validateCombinedCatalogSession } from '../catalogSession';
import { combinedScopeSelectionIdentity, reviewCombinedScopeSelection, scopeSelectionPreference, WindowScopeSelection } from '../scopeSelection';
import { launchArgs, unifiedEntry } from '../model';
import { comparePublicationVector } from '../publications';
import { combinedReadDetails, discoveryDetails, discoveryFollowupPlan, evidenceCards, evidenceRows, renderExport, workbenchStepReportOmissions } from '../workbenchRender';
import { object, type Value, type WorkbenchTurn } from '../workbenchContracts';
import { readResultBody, selectedVerification } from '../sourceVerification';
import { assertOriginalPublicationForSymbolInputs, strategyFromPlan } from '../workbenchStrategies';

const project=(n:number)=>'prj_'+n.toString(16).padStart(2,'0').repeat(16);
test('advisory details and complete exports preserve the entire canonical recommendation',()=>{
    const native={schema:'airk.recommendations/1',available:[],selected:{project_ids:[],index_ids:[]},instructions:{text:'validated example'},future_field:{retained:true}};
    const envelope={executed:true,status:'complete',steps:[{id:'advice',tool:'query_scope',status:'complete',result:native}]};
    assert.deepEqual(discoveryDetails(envelope)[0].recommendations,native);
    const turn={id:'advice',input:'Scope advice',envelope} as unknown as WorkbenchTurn;
    for(const format of ['json','markdown'] as const)assert.match(renderExport({schema:1,id:'session',project:hash,title:'Advice',createdAt:1,updatedAt:1,revision:1,omitted:0,turns:[turn]},format),/future_field/);
});
const index=(n:number)=>'idx_'+n.toString(16).padStart(2,'0').repeat(16);
const source=(n:number)=>'src_'+n.toString(16).padStart(2,'0').repeat(16);
const hash='11'.repeat(32),grantId='grt_'+'04'.repeat(16),namespace='01'.repeat(16),host='02'.repeat(32),contextKey='03'.repeat(32);
const publication=(n:number)=>({collection:'fixture',incarnation:'12'.repeat(16),generation:n,revision:hash,token:`airk-r1:${'12'.repeat(16)}:${n}:${hash}`});
const context={schema:1,namespace,host,key:contextKey,catalog_revision:7,context:null};
const session=()=>({schema:1 as const,context_key:contextKey,namespace,expected_catalog_revision:7,grant_id:grantId,expected_grant_revision:2,project_ids:[project(1),project(2)]});
function fixture(){
    const memberships=[[1,1,'api'],[2,1,'impl'],[2,2,''],[3,1,'private']].map(([p,i,prefix])=>({membership:{
        project_id:project(Number(p)),index_id:index(Number(i)),revision:1,scope:[{source_root_id:source(Number(i)),kind:'subtree',path:prefix}]},source_revision:3,policy_epoch:0,policy_digest:null}));
    const grant={namespace,revision:7,item:{kind:'grant',value:{provenance:{kind:'user_reviewed_project_grant'},grant:{id:grantId,host,revision:2,revoked:false,memberships}}}};
    const indexes=[1,2].map(n=>({namespace,revision:7,item:{kind:'index',value:{id:index(n),name:'Native '+n,definition_revision:1,source_revision:3,policy:null,tombstone:false,source_import:'complete',
        backing:{binding:n.toString(16).padStart(2,'0').repeat(32),root:path.resolve('source'+n),data_root:path.resolve('data'+n),collection:'fixture'}}}}));
    const choice={grant_id:grantId,expected_grant_revision:2,project_ids:[project(2),project(1)]};
    return {grant,indexes,choice};
}
function combined():Value {
    const hit={citation:{path:'same.md',content_hash:hash,start_line:1,end_line:1,location:{Lines:{start:1,end:1}}},text:'original excerpt',truncated:false};
    const native={publication:publication(1),hits:[hit],execution:{version:1,requested_mode:'default',served_mode:'lexical',incomplete:true,incomplete_reason:'helper_unavailable'},future_observation:{retained:true}};
    const members=[1,2].map(n=>({index_id:index(n),allocation:{limit:1,max_bytes:1024},outcome:'complete',result:{content:[{type:'text',text:JSON.stringify(native)}],structuredContent:structuredClone(native),isError:false}}));
    return {schema:'airk.combined-read/1',selection:{project_ids:[project(1),project(2)],index_ids:[index(1),index(2)],grant_id:grantId,grant_revision:2,member_availability:'report',identity:'23'.repeat(32)},rank_policy:'airk.rank-fusion.rrf-equal/1',members,
        publication_vector:{authority:hash,observed_from_unix_ms:1,observed_to_unix_ms:2,members:[1,2].map(n=>({index_id:index(n),publication:publication(1)}))},
        ranking:[1,2].map((n,i)=>({result_pointer:`/members/${i}/result/structuredContent/hits/0`,index_id:index(n),local_rank:0,rank_contribution:1/61,
            origin:{index_id:index(n),source_root_id:source(n),source_relative_path:'same.md',project_ids:[project(n)],publication:publication(1)}})),partial:false,truncated:false};
}

test('combined wire choice is explicit, canonical, bounded and separate from the legacy selector',()=>{
    assert.deepEqual(validateCombinedCatalogSession({...session(),project_ids:[project(2),project(1)],index_ids:[index(2),index(1)]}).index_ids,[index(1),index(2)]);
    assert.ok(!Object.hasOwn(validateCombinedCatalogSession(session()),'index_ids'));
    for(const change of [{index_id:index(1)},{project:'10'.repeat(32)},{project_ids:[]},{project_ids:[project(1),project(1)]},{index_ids:[]},{index_ids:null},{index_ids:Array(33).fill(index(1))},{schema:2},{grant_id:'grt_bad'}])assert.throws(()=>validateCombinedCatalogSession({...session(),...change}));
});

test('combined review deduplicates shared indexes and never adds an unselected Project or index',()=>{
    const f=fixture(),selection=reviewCombinedScopeSelection(f.choice,context,f.grant,f.indexes);
    assert.deepEqual(selection.indexes.map(row=>row.id),[index(1),index(2)]);
    assert.deepEqual(selection.session.project_ids,[project(1),project(2)]);
    assert.ok(!selection.session.project_ids.includes(project(3)));
    assert.equal(selection.label,'2 selected index sets · 2 indexes');
    assert.throws(()=>reviewCombinedScopeSelection({...f.choice,index_ids:[index(3)]},context,f.grant,[]),/outside_reviewed_grant/);
    assert.throws(()=>reviewCombinedScopeSelection({...f.choice,project_ids:[project(4)]},context,f.grant,f.indexes),/outside_reviewed_grant/);
    assert.throws(()=>reviewCombinedScopeSelection({...f.choice,context_key:'09'.repeat(32)},context,f.grant,f.indexes),/choice_fields/);
    assert.throws(()=>reviewCombinedScopeSelection(f.choice,context,f.grant,[f.indexes[0],f.indexes[0]]),/crossed_scope_selection_metadata/);
    const stale=structuredClone(f);stale.indexes[1].revision=8;assert.throws(()=>reviewCombinedScopeSelection(stale.choice,context,stale.grant,stale.indexes));
    const revoked=structuredClone(f);revoked.grant.item.value.grant.revoked=true;assert.throws(()=>reviewCombinedScopeSelection(revoked.choice,context,revoked.grant,revoked.indexes));
});

test('combined identity changes on scope changes but preserves a mere catalog clock refresh',()=>{
    const selected=session(),identity=combinedScopeSelectionIdentity(selected);
    assert.equal(identity,combinedScopeSelectionIdentity({...selected,expected_catalog_revision:8}));
    for(const change of [{index_ids:[index(1)]},{project_ids:[project(1)]},{expected_grant_revision:3},{context_key:'09'.repeat(32)}])assert.notEqual(identity,combinedScopeSelectionIdentity({...selected,...change}));
    assert.notEqual(identity,combinedScopeSelectionIdentity({...selected,index_ids:[index(1),index(2)]}),'omission and an explicit subset remain distinct frozen choices');
});

test('publication vectors compare only the same authority and member set and disclose mixed order',()=>{
    const observation=(a:number,b:number)=>({authority:'trusted-host',vector:{authority:hash,members:[{index_id:index(1),publication:publication(a)},{index_id:index(2),publication:publication(b)}]}});
    assert.equal(comparePublicationVector(observation(1,1),observation(2,2)),'older');assert.equal(comparePublicationVector(observation(2,2),observation(1,1)),'newer');
    assert.equal(comparePublicationVector(observation(1,2),observation(2,1)),'mixed');assert.equal(comparePublicationVector(observation(1,1),observation(1,1)),'equal');
    const other=observation(1,1);other.vector.members[1].index_id=index(3);assert.equal(comparePublicationVector(observation(1,1),other),'different');
    const missing=observation(1,1);(missing.vector.members[1] as unknown as Value).publication=null;assert.equal(comparePublicationVector(observation(1,1),missing),'unknown');
    const changed=observation(1,1);changed.vector.authority='13'.repeat(32);assert.equal(comparePublicationVector(observation(1,1),changed),'different');
});

test('combined cards retain distinct index origins for identical paths and complete native results in exports',()=>{
    const result=combined(),envelope={status:'complete',executed:true,steps:[{id:'search',tool:'search',status:'complete',result}]} as Value;
    const cards=evidenceCards(envelope);assert.equal(cards.length,2);assert.notEqual(cards[0].indexId,cards[1].indexId);assert.equal(cards[0].path,cards[1].path);
    assert.deepEqual(cards[0].projectIds,[project(1)]);assert.equal(cards[1].sourceRootId,source(2));assert.equal(cards[0].publication,publication(1).token);
    assert.equal(combinedReadDetails(envelope)[0].step,'search');
    const turn:WorkbenchTurn={id:'turn',at:1,input:'search',format:'structured',envelope};
    const exported=renderExport({schema:1,id:'session',project:hash,title:'Combined',createdAt:1,updatedAt:1,revision:1,omitted:0,turns:[turn]},'json');
    assert.deepEqual(JSON.parse(exported).turns[0].envelope,envelope);
    assert.ok(renderExport({schema:1,id:'session',project:hash,title:'Combined',createdAt:1,updatedAt:1,revision:1,omitted:0,turns:[turn]},'markdown').includes('future_observation'));
    const selected=JSON.parse(renderExport({schema:1,id:'session',project:hash,title:'Combined',createdAt:1,updatedAt:1,revision:1,omitted:0,turns:[turn]},'json','turn',['record-1']));
    assert.equal(selected.turns[0].envelope.evidence.length,1);assert.equal(selected.turns[0].envelope.evidence[0].origin.index_id,index(2));
    const hostile=combined();object((hostile.ranking as Value[])[0]).result_pointer='/__proto__/polluted';assert.equal(evidenceRows(hostile).filter(row=>Object.keys(object(row.citation)).length).length,1);
});

test('member verification never crosses identical paths from separate originating indexes',()=>{
    const envelope=combined(),members=envelope.members as Value[];
    const receipt={schema:'airk.saved-source-verification/1',method:'sha256-full-saved-source/1',sources:[{id:'source-0',origin_id:'origin-0',path:'same.md',expected_hash:hash,reference_ids:['ref-0']}],
        references:[{id:'ref-0',source_id:'source-0',result_pointer:'/hits/0/citation'}],origins:[{id:'origin-0',publication:publication(1).token}],counts:{},limits:{},qualifications:[],status:'partial'};
    object(members[0].result).structuredContent={...readResultBody(members[0].result),source_verification:receipt};
    const cards=evidenceCards(envelope),checks=selectedVerification(envelope,cards);
    assert.equal(checks.requested,true);assert.equal(checks.observations.length,1);assert.equal(checks.observations[0].index_id,index(1));assert.equal(checks.accounting.with_retained_observations,1);
    assert.deepEqual(checks.accounting.without_retained_observations,['record-1']);
    assert.deepEqual(readResultBody(members[0].result).source_verification,receipt);
    const finalCheck=structuredClone(envelope);readResultBody((finalCheck.members as Value[])[0].result).future_private_payload='UNSELECTED_PRIVATE_RECEIPT_PAYLOAD';
    envelope.member_verification=finalCheck;
    assert.ok(combinedReadDetails(envelope).some(detail=>detail.step==='Returned-source verification'),'the response-wide dated member checks remain visible in structured details');
    const sessionValue={schema:1 as const,id:'session',project:hash,title:'Combined',createdAt:1,updatedAt:1,revision:1,omitted:0,
        turns:[{id:'turn',at:1,input:'search',format:'structured' as const,envelope}]};
    assert.ok(renderExport(sessionValue,'json').includes('UNSELECTED_PRIVATE_RECEIPT_PAYLOAD'));
    const selected=renderExport(sessionValue,'json','turn',['record-1']);assert.ok(!selected.includes('UNSELECTED_PRIVATE_RECEIPT_PAYLOAD'));
    assert.equal(JSON.parse(selected).turns[0].envelope.member_verification,undefined,'selected copies replace complete member receipts with explicit selected observations');
});

test('combined definition followups keep one explicit originating index and exact member publication',()=>{
    const result=combined();
    for(const [i,member] of (result.members as Value[]).entries()){const body=readResultBody(member.result);body.definitions=[{name:'Widget',matches:[{symbol_id:'syntax:same.md:0:10:definition',citation:{path:'same.md'}}]}];
        body.scope_observation={schema:1,qualifier:'selected_subset',grant:{id:grantId,revision:2},members:[{index_id:member.index_id,memberships:[{project_id:project(i+1),membership_revision:1}]}]};}
    const envelope={status:'complete',executed:true,plan:{path_prefix:''},steps:[{id:'search',tool:'search',status:'complete',arguments:{query:'Widget'},result}]};
    const details=discoveryDetails(envelope);assert.equal(details.length,2);assert.equal(details[0].indexId,index(1));assert.equal(details[1].indexId,index(2));assert.ok(details.every(detail=>detail.canFollow));
    const plan=discoveryFollowupPlan(envelope,details[1].id,'Widget','same.md');
    assert.deepEqual(plan.query_scope,{project_ids:[project(2)],index_ids:[index(2)]});assert.equal(plan.publication,undefined);
    const args=object((plan.steps as Value[])[0].args);assert.equal(args.name,'syntax:same.md:0:10:definition');assert.equal(args.origin_publication,publication(1).token);
    assert.doesNotThrow(()=>assertOriginalPublicationForSymbolInputs(plan));
    assert.throws(()=>assertOriginalPublicationForSymbolInputs({...plan,query_scope:{project_ids:[project(1),project(2)],index_ids:[index(1),index(2)]}}),/original publication/);
    assert.throws(()=>strategyFromPlan(plan,'Widget','Widget'),/explicit combined scope/,'Strategy conversion cannot discard combined selector arrays');
    delete readResultBody((result.members as Value[])[1].result).scope_observation;
    assert.equal(discoveryDetails(envelope)[1].canFollow,false,'a missing member association cannot infer all selected Projects');
    assert.equal(discoveryDetails(envelope)[1].projectIds,undefined,'unobserved attribution is disclosed as unknown');
});

test('selected combined copies preserve canonical scope observation and exact grant identity',()=>{
    const envelope=combined(),sessionValue={schema:1 as const,id:'session',project:hash,title:'Combined',createdAt:1,updatedAt:1,revision:1,omitted:0,
        turns:[{id:'turn',at:1,input:'search',format:'structured' as const,envelope}]};
    const selected=JSON.parse(renderExport(sessionValue,'json','turn',['record-0'])).turns[0].envelope;
    assert.deepEqual(selected.selection,envelope.selection);assert.equal(selected.selected_subset,true);
    assert.equal(selected.selection.grant_id,grantId);assert.equal(selected.selection.grant_revision,2);assert.equal(selected.selection.identity,'23'.repeat(32));
    const markdown=renderExport(sessionValue,'markdown','turn',['record-0']);assert.ok(markdown.includes(grantId));assert.ok(markdown.includes('23'.repeat(32)));
});

test('selected source coverage keeps each member index name and elapsed time (U108)',()=>{
    const envelope=combined();(envelope.members as Value[]).forEach((member,n)=>{member.index_name=`Native ${n+1}`;member.elapsed_ms=7*(n+1);});
    const sessionValue={schema:1 as const,id:'session',project:hash,title:'Combined',createdAt:1,updatedAt:1,revision:1,omitted:0,turns:[{id:'turn',at:1,input:'search',format:'structured' as const,envelope}]};
    const coverage=renderExport(sessionValue,'markdown','turn',['record-0']).split('### Original source request coverage for selected evidence')[1]?.split('\n### ')[0]??'';
    for(const fragment of ['"index_name": "Native 1"','"index_name": "Native 2"','"elapsed_ms": 7','"elapsed_ms": 14'])assert.ok(coverage.includes(fragment),fragment);
});

test('combined file rows preserve the native virtual path separately from the source-relative origin',()=>{
    const result=combined(),member=(result.members as Value[])[0];
    readResultBody(member.result).paths=['@external/same.rs'];
    result.ranking=[{result_pointer:'/members/0/result/structuredContent/paths/0',index_id:index(1),local_rank:0,rank_contribution:1/61,
        origin:{index_id:index(1),source_root_id:source(1),project_ids:[project(1)],source_relative_path:'same.rs',publication:publication(1)}}];
    const rows=evidenceRows(result);assert.equal(rows.length,1);assert.equal(rows[0].path,'@external/same.rs');
    assert.equal(object(rows[0].origin).source_relative_path,'same.rs');assert.equal(evidenceCards(result).length,0,'a file-path string does not invent a cited line or content hash');
});

test('all seventy-two canonical step reports retain later combined evidence and final member checks',()=>{
    const result=combined(),steps=Array.from({length:72},(_,i)=>({id:'step-'+i,tool:'search',status:i===71?'complete':'failed',result:i===71?result:{error:{code:'skipped'}}}));
    const checks={schema:'airk.combined-verification/1',scope_identity:'34'.repeat(32),members:[{index_id:index(1),result:{source_verification:{status:'partial',unknown_future_check:true}}}],partial:true};
    const envelope={status:'partial',executed:true,steps,member_verification:checks};
    assert.equal(evidenceCards(envelope).length,2);assert.equal(evidenceCards(envelope)[0].step,'step-71');
    assert.ok(combinedReadDetails(envelope).some(detail=>detail.step==='step-71'));assert.deepEqual(combinedReadDetails(envelope).at(-1)?.result,checks);
    assert.equal(workbenchStepReportOmissions(envelope),0);assert.equal(workbenchStepReportOmissions({...envelope,steps:[...steps,steps[0]]}),1);
    const sessionValue={schema:1 as const,id:'session',project:hash,title:'Expanded',createdAt:1,updatedAt:1,revision:1,omitted:0,turns:[{id:'turn',at:1,input:'expanded',format:'structured' as const,envelope}]};
    const selected=JSON.parse(renderExport(sessionValue,'json','turn',['record-0'])).turns[0].envelope;
    assert.equal(selected.evidence.length,1);assert.equal(selected.evidence[0].step,'step-71');assert.ok(selected.source_coverage.some((row:Value)=>row.step==='step-71'));
    assert.deepEqual(JSON.parse(renderExport(sessionValue,'json')).turns[0].envelope,envelope);
});
