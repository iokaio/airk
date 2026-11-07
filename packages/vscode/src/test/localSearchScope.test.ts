// SPDX-License-Identifier: Apache-2.0
import {strict as assert} from 'node:assert';
import {test} from 'node:test';
import {applyLocalSearchScope,saveLocalIndexSet,type ManagedScopeProject,type LocalScopeAdapter} from '../localSearchScope';
import {CatalogObject,CatalogReview,catalogObject} from '../indexingCatalog';
import {NativeServiceFixture} from './nativeServiceFixture';
import {record} from '../serviceClient';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';

const namespace='ab'.repeat(16),host='cd'.repeat(32),key='ef'.repeat(32),index='idx_'+'12'.repeat(16),source='src_'+'34'.repeat(16),set='prj_'+'56'.repeat(16);
function fixture(){
    let revision=1,managed:ManagedScopeProject|undefined,cleared=0;const operations:CatalogObject[]=[],projects=new Map<string,CatalogObject>(),members=new Map<string,CatalogObject>();
    const managedCache=new Map<string,ManagedScopeProject>();
    const descriptor=()=>({schema:1,namespace,host,key,catalog_revision:revision,context:null});
    const project=(id:string)=>({id,name:'Example',definition_revision:1,scope_revision:1,defaults_revision:1,default_indexes:[]});
    const adapter:LocalScopeAdapter={current(){},context:descriptor,managed:fingerprint=>fingerprint?managedCache.get(fingerprint):managed,saveManaged:async(value,fingerprint)=>{managed=value;if(fingerprint)managedCache.set(fingerprint,value);},refreshContext:async()=>{},clear:async()=>{cleared++;return {status:'search_scope_cleared'};},use:async choice=>({choice}),control:{
        catalogPending:()=>[],catalogReconcile:async()=>[],
        catalogRead:async input=>{
            if(input.catalog_detail){const detail=catalogObject(input.catalog_detail),entity=catalogObject(detail.entity);assert.equal(detail.expected_catalog_revision,revision);let item:unknown=null;
                if(entity.kind==='project'&&projects.has(String(entity.id)))item={kind:'project',value:projects.get(String(entity.id))};
                if(entity.kind==='index')item={kind:'index',value:{id:entity.id,name:'Local index',definition_revision:1,source_revision:1,source_import:'complete',tombstone:false,backing:{}}};
                return {status:'catalog_detail',detail:{namespace,revision,item}};
            }
            const request=catalogObject(catalogObject(input.catalog_page).request);assert.equal(request.expected_catalog_revision,revision);
            const items=request.kind==='memberships'?[...members.values()].filter(member=>member.project_id===request.project_id).map(value=>({kind:'membership',value})):[{kind:'source',value:{id:source,index_id:request.index_id,canonical_path:'/src',aliases:[],owner:true,enabled:true}}];
            return {status:'catalog_page',page:{namespace,revision,items,continuation:null}};
        },
        catalogPrepare:async operation=>({request:{caller:namespace,key:{namespace,epoch:1,nonce:namespace},expected_catalog_revision:revision,operation},response:{},previewDigest:host,requestDigest:key}),
        catalogApply:async review=>{
            const op=review.request.operation;assert.equal(review.request.expected_catalog_revision,revision);operations.push(op);revision++;
            if(op.operation==='create_project')projects.set(String(op.id),{...project(String(op.id)),name:op.name});
            if(op.operation==='rename_project')projects.set(String(op.id),{...projects.get(String(op.id)),name:op.name,definition_revision:2});
            if(op.operation==='set_membership'){const member=catalogObject(op.membership);members.set(`${member.project_id}:${member.index_id}`,member);const prior=projects.get(String(member.project_id))!;prior.scope_revision=Number(prior.scope_revision)+1;}
            if(op.operation==='remove_membership'){members.delete(`${op.project_id}:${op.index_id}`);const prior=projects.get(String(op.project_id))!;prior.scope_revision=Number(prior.scope_revision)+1;}
            if(op.operation==='set_project_defaults'){const prior=projects.get(String(op.project_id))!;prior.default_indexes=op.index_ids;prior.defaults_revision=Number(prior.defaults_revision)+1;}
            return {status:'catalog_applied',receipt:{catalog_revision:revision}};
        },
        catalogGrantDraft:async(_key,selection)=>({request:{caller:namespace,key:{namespace,epoch:1,nonce:namespace},expected_catalog_revision:revision,operation:{operation:'create_grant',grant:{id:'grt_'+'01'.repeat(16),host,revision:1,memberships:[],revoked:false}}},response:{selection},previewDigest:host,requestDigest:key}),
    }};
    return {adapter,operations,projects,members,project,managed:()=>managed,cleared:()=>cleared,revision:()=>revision};
}

test('direct index Apply creates native full-source membership and reuses an immutable matching set',async()=>{
    const f=fixture();const result=await applyLocalSearchScope(f.adapter,{index_ids:[index],project_ids:[]});
    assert.equal(result.status,'search_scope_selected');assert.deepEqual(f.operations.map(op=>op.operation),['create_project','set_membership','create_grant']);
    const id=f.managed()!.id,member=f.members.get(`${id}:${index}`)!;assert.deepEqual(member.scope,[{source_root_id:source,kind:'subtree',path:''}]);
    const before=f.operations.length;await applyLocalSearchScope(f.adapter,{index_ids:[index],project_ids:[]});assert.equal(f.managed()!.id,id);assert.deepEqual(f.operations.slice(before).map(op=>op.operation),['create_grant']);
    await applyLocalSearchScope(f.adapter,{index_ids:['idx_'+'78'.repeat(16)],project_ids:[]});assert.notEqual(f.managed()!.id,id);assert.deepEqual(f.members.get(`${id}:${index}`),member,'sibling window membership stays unchanged');
    await applyLocalSearchScope(f.adapter,{index_ids:[index],project_ids:[]});assert.equal(f.operations.filter(op=>op.operation==='create_project').length,2,'A to B to A reuses the exact owned immutable support set');
});

test('empty selection clears without catalog writes; empty sets and oversized union refuse before writes',async()=>{
    const f=fixture();await applyLocalSearchScope(f.adapter,{index_ids:[],project_ids:[]});assert.equal(f.cleared(),1);assert.equal(f.operations.length,0);
    f.projects.set(set,f.project(set));await assert.rejects(applyLocalSearchScope(f.adapter,{index_ids:[index],project_ids:[set]}),/has no indexes/);assert.equal(f.operations.length,0);
    await assert.rejects(applyLocalSearchScope(f.adapter,{index_ids:[index],project_ids:Array.from({length:32},(_,i)=>'prj_'+i.toString(16).padStart(32,'0'))}),/too_many_sets/);assert.equal(f.operations.length,0);
});

test('saving a renamed index set retains existing partial-folder membership',async()=>{
    const f=fixture();f.projects.set(set,f.project(set));const member={project_id:set,index_id:index,revision:1,scope:[{source_root_id:source,kind:'subtree',path:'src/backend'}]};f.members.set(`${set}:${index}`,member);
    await saveLocalIndexSet(f.adapter,{id:set,name:'Backend',index_ids:[index]});
    assert.deepEqual(f.members.get(`${set}:${index}`),member);assert.ok(!f.operations.some(op=>op.operation==='set_membership'));assert.equal(f.projects.get(set)!.name,'Backend');
});

test('uncertain native write stops orchestration without replay or grant activation',async()=>{
    const f=fixture();let writes=0;f.adapter.control.catalogApply=async()=>{writes++;throw new Error('transport_outcome_unknown');};
    await assert.rejects(applyLocalSearchScope(f.adapter,{index_ids:[index],project_ids:[]}),/outcome_unknown/);assert.equal(writes,1);assert.equal(f.operations.length,0);
});

test('local scope orchestration uses the actual native catalog and CLI transport', {skip:!process.env.AIRK_TEST_BINARY,timeout:300000},async()=>{
    const native=await NativeServiceFixture.create(process.env.AIRK_TEST_BINARY!);let managed:ManagedScopeProject|undefined;
    try{
        const spec=await native.register('local-scope'),client=native.client!,contextKey=spec.catalogSession!.context_key;
        await fs.writeFile(path.join(spec.root,'scope.ts'),'export const localScope = true;\n');
        await native.configure(spec,{operation:'vectors',enabled:false});await native.job(spec);await native.publishedStatus(spec);
        let context=record((await client.catalogHostContext(contextKey)).descriptor),activated:CatalogObject|undefined;
        const control:LocalScopeAdapter['control']={
            catalogRead:method=>client.rpc(method,Object.keys(method)[0]),catalogPending:()=>[],catalogReconcile:async()=>[],
            catalogPrepare:async(operation,revision)=>{const request=await client.prepareCatalog(operation,revision),response=await client.rpc({catalog_preview:{request}},'catalog_preview');return {request,response,previewDigest:String(response.preview_digest),requestDigest:String(record(response.preview).request_digest)};},
            catalogApply:review=>client.submitCatalog({catalog_apply:{request:review.request,preview_digest:review.previewDigest},request_digest:review.requestDigest}),
            catalogGrantDraft:async(key,selection,revision)=>client.catalogGrantDraft(key,{key:await client.key(),expected_catalog_revision:revision,project_ids:selection.project_ids as string[],index_ids:selection.index_ids as string[]|null}),
        };
        const adapter:LocalScopeAdapter={control,current(){},context:()=>context,managed:()=>managed,saveManaged:async value=>{managed=value;},refreshContext:async()=>{context=record((await client.catalogHostContext(contextKey)).descriptor);},clear:async()=>({status:'search_scope_cleared'}),use:async choice=>{activated=choice;return {choice};}};
        const outcome=await applyLocalSearchScope(adapter,{index_ids:[spec.catalogSession!.index_id],project_ids:[]});assert.equal(outcome.status,'search_scope_selected');assert.ok(managed&&activated);
        const detail=await client.rpc({catalog_detail:{entity:{kind:'project',id:managed.id},expected_catalog_revision:context.catalog_revision}},'catalog_detail');assert.equal(record(record(record(detail.detail).item).value).id,managed.id);
        const members=await client.rpc({catalog_page:{request:{kind:'memberships',index_id:null,project_id:managed.id,after:null,expected_catalog_revision:context.catalog_revision,limit:32,max_bytes:48*1024}}},'catalog_page');
        const row=record((record(members.page).items as unknown[])[0]);assert.equal(record(row.value).index_id,spec.catalogSession!.index_id);assert.deepEqual((record(row.value).scope as unknown[]).map(value=>record(value).path),['']);
        const selectedSpec=()=>({...spec,catalogSession:undefined,combinedSession:{schema:1 as const,context_key:contextKey,namespace:String(context.namespace),expected_catalog_revision:Number(context.catalog_revision),grant_id:String(activated!.grant_id),expected_grant_revision:Number(activated!.expected_grant_revision),project_ids:activated!.project_ids as string[]}});
        const query=await native.tool(selectedSpec(),'airk_query_scope',{view:'projects',max_bytes:65536,response_mode:undefined});assert.equal(query.schema,'airk.query-scope/1','actual CLI scoped read admits the internally selected grant');
        const saved=await saveLocalIndexSet(adapter,{name:'Named index set',index_ids:[spec.catalogSession!.index_id]});assert.equal(saved.status,'index_set_saved');
        const group=String(saved.id);await applyLocalSearchScope(adapter,{index_ids:[],project_ids:[group]});
        const setOnly=await native.tool(selectedSpec(),'airk_query_scope',{view:'projects',max_bytes:65536,response_mode:undefined});assert.equal((setOnly.items as unknown[]).length,1);
        await applyLocalSearchScope(adapter,{index_ids:[spec.catalogSession!.index_id],project_ids:[group]});
        const mixed=await native.tool(selectedSpec(),'airk_query_scope',{view:'projects',max_bytes:65536,response_mode:undefined});assert.equal((mixed.items as unknown[]).length,2);
        const renamed=await saveLocalIndexSet(adapter,{id:group,name:'Renamed index set',index_ids:[spec.catalogSession!.index_id]});assert.ok(!(renamed.steps as {review:CatalogReview}[]).some(step=>step.review.request.operation.operation==='set_membership'),'rename retains existing native membership');
        const stillValid=await native.tool(selectedSpec(),'airk_query_scope',{view:'projects',max_bytes:65536,response_mode:undefined});assert.equal(stillValid.schema,'airk.query-scope/1');
        const before=context.catalog_revision;await applyLocalSearchScope(adapter,{index_ids:[],project_ids:[]});await adapter.refreshContext();assert.equal(context.catalog_revision,before,'deselect all does not mutate native catalog');
    }catch(error){await native.retainFailure(error);throw error;}finally{await native.dispose();}
});
