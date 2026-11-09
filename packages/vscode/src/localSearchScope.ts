// SPDX-License-Identifier: Apache-2.0
/** Local-user selection orchestration. Native catalog owners still validate and
 * journal every membership/grant mutation; selected files are never modified. */
import { createHash, randomUUID } from 'node:crypto';
import { CatalogObject, CatalogReview, catalogBound, catalogHex, catalogId, catalogItems, catalogObject, catalogRevision, sameCatalogValue, validateNativeHostContext } from './indexingCatalog';
import type { CatalogControl } from './indexingCatalogController';

export interface LocalScopeChoice { index_ids:string[]; project_ids:string[] }
export interface ManagedScopeProject { schema:1; namespace:string;host:string;context_key:string;id:string }
export function localScopeChoice(input:unknown):LocalScopeChoice {
    const value=catalogObject(input);catalogBound(value,4096);
    if(Object.keys(value).sort().join(',')!=='index_ids,project_ids')throw new Error('invalid_search_scope_selection');
    const ids=(input:unknown,prefix:'idx_'|'prj_')=>{
        if(!Array.isArray(input)||input.length>32||new Set(input).size!==input.length)throw new Error('invalid_search_scope_selection');
        return input.map(id=>catalogId(id,prefix)).sort();
    };
    return {index_ids:ids(value.index_ids,'idx_'),project_ids:ids(value.project_ids,'prj_')};
}
export function managedScopeProject(input:unknown,context:unknown):ManagedScopeProject|undefined {
    if(input===undefined)return undefined;
    const value=catalogObject(input),native=validateNativeHostContext(context);
    if(value.schema!==1||value.namespace!==native.namespace||value.host!==native.host||value.context_key!==native.key)return undefined;
    return {schema:1,namespace:catalogHex(value.namespace),host:catalogHex(value.host,64),context_key:catalogHex(value.context_key,64),id:catalogId(value.id,'prj_')};
}

export interface LocalScopeAdapter {
    control:CatalogControl; context():CatalogObject; current():void;
    managed(fingerprint?:string):ManagedScopeProject|undefined; saveManaged(value:ManagedScopeProject,fingerprint?:string):Promise<void>;
    refreshContext():Promise<void>;
    use(choice:CatalogObject,namespace:string,revision:number):Promise<CatalogObject>;
    clear():Promise<CatalogObject>;
}

/** One explicit Apply. Failure stops the sequence; pending native receipts are
 * reconciled through the existing journal rather than replaying a failed step. */
export async function applyLocalSearchScope(adapter:LocalScopeAdapter,input:unknown):Promise<CatalogObject> {
    const selection=localScopeChoice(input);adapter.current();
    if(!selection.index_ids.length&&!selection.project_ids.length)return adapter.clear();
    if(selection.project_ids.length+(selection.index_ids.length?1:0)>32)throw new Error('search_scope_too_many_sets');
    const control=adapter.control;
    if(control.catalogPending().length)throw new Error('catalog_outcome_unconfirmed_reconcile_first');
    if(!control.catalogGrantDraft)throw new Error('native_search_scope_unavailable');
    const context=validateNativeHostContext(adapter.context()),namespace=context.namespace;
    let revision=context.catalog_revision;
    const observations:CatalogObject[]=[],steps:{review:CatalogReview;outcome:CatalogObject}[]=[];
    const check=()=>{adapter.current();if(control.catalogPending().length)throw new Error('catalog_outcome_unconfirmed_reconcile_first');};
    const read=async(method:CatalogObject,field:'page'|'detail')=>{
        check();const response=await control.catalogRead(method);check();catalogBound(response,65536);
        const body=catalogObject(response[field]);
        if(body.namespace!==namespace||body.revision!==revision)throw new Error('search_scope_catalog_changed_refresh');
        catalogItems(response,field);observations.push(response);return response;
    };
    const detail=async(kind:'project'|'index',id:string)=>{
        const response=await read({catalog_detail:{entity:{kind,id},expected_catalog_revision:revision}},'detail');
        const row=catalogItems(response,'detail')[0];
        if(!row||row.kind!==kind||row.value.id!==id)throw new Error('search_scope_item_unavailable');
        return row.value;
    };
    const page=async(kind:'sources'|'memberships',index_id:string|null,project_id:string|null)=>{
        let after:unknown=null;const values:CatalogObject[]=[];const seen=new Set<string>();
        for(let turn=0;turn<16;turn++){
            const response=await read({catalog_page:{request:{kind,index_id,project_id,after,expected_catalog_revision:revision,limit:32,max_bytes:48*1024}}},'page');
            values.push(...catalogItems(response).map(row=>row.value));
            if(values.length>128)throw new Error('search_scope_metadata_limit');
            after=catalogObject(response.page).continuation;
            if(after===null)return values;
            const token=JSON.stringify(after);if(seen.has(token))throw new Error('search_scope_continuation_repeated');seen.add(token);
        }
        throw new Error('search_scope_metadata_limit');
    };
    const mutate=async(operation:CatalogObject)=>{
        check();const review=await control.catalogPrepare(operation,revision);check();
        if(review.request.key.namespace!==namespace||review.request.expected_catalog_revision!==revision||!sameCatalogValue(review.request.operation,operation))throw new Error('crossed_search_scope_review');
        const outcome=await control.catalogApply(review,()=>{try{adapter.current();return true;}catch{return false;}});adapter.current();
        if(outcome.status!=='catalog_applied'||catalogObject(outcome.receipt).catalog_revision!==revision+1)throw Object.assign(new Error('search_scope_outcome_unconfirmed'),{nativeResponse:outcome});
        steps.push({review,outcome});revision++;
    };
    const chosenProjects=[...selection.project_ids];
    // Check visible selections before creating any supporting metadata.
    const union=new Set(selection.index_ids);
    for(const id of selection.project_ids){
        const project=await detail('project',id),members=await page('memberships',null,id);
        if(!members.some(member=>Array.isArray(member.scope)&&member.scope.length>0))throw new Error(`Index set "${String(project.name)}" has no indexes. Add an index before selecting it.`);
        if(members.some(member=>member.project_id!==id))throw new Error('crossed_search_scope_memberships');
        for(const member of members)union.add(catalogId(member.index_id,'idx_'));
        if(union.size>32)throw new Error('search_scope_too_many_indexes');
    }
    for(const id of selection.index_ids){const row=await detail('index',id);if(row.tombstone!==false||row.source_import!=='complete')throw new Error('search_scope_index_unavailable');}
    if(selection.index_ids.length){
        const scopes=new Map<string,CatalogObject[]>();
        for(const id of selection.index_ids){
            const roots=(await page('sources',id,null)).filter(root=>root.enabled===true);
            if(!roots.length||roots.some(root=>root.index_id!==id))throw new Error('search_scope_sources_unavailable');
            scopes.set(id,roots.map(root=>({source_root_id:catalogId(root.id,'src_'),kind:'subtree',path:''})).sort((a,b)=>String(a.source_root_id).localeCompare(String(b.source_root_id))));
        }
        const fingerprint=createHash('sha256').update(JSON.stringify([...scopes])).digest('hex');
        let managed=adapter.managed(fingerprint),reuse=false;
        if(managed&&selection.project_ids.includes(managed.id))throw new Error('managed_scope_set_is_not_selectable');
        if(managed){
            const response=await read({catalog_detail:{entity:{kind:'project',id:managed.id},expected_catalog_revision:revision}},'detail');
            const row=catalogItems(response,'detail')[0];
            if(row){
                if(row.kind!=='project'||row.value.id!==managed.id)throw new Error('crossed_managed_scope_set');
                const members=await page('memberships',null,managed.id);
                reuse=members.length===selection.index_ids.length&&members.every(member=>member.project_id===managed!.id&&scopes.has(String(member.index_id))&&sameCatalogValue(member.scope,scopes.get(String(member.index_id))));
            }
        }
        // A supporting set is immutable once used. A second window selecting a
        // different subset must never invalidate another window's frozen grant.
        if(!reuse){
            managed={schema:1,namespace,host:context.host,context_key:context.key,id:`prj_${randomUUID().replaceAll('-','')}`};
            await adapter.saveManaged(managed,fingerprint);adapter.current();
            await mutate({operation:'create_project',id:managed.id,name:'AIRK managed search scope'});
            let project=await detail('project',managed.id);
            for(const id of selection.index_ids){
                await mutate({operation:'set_membership',expected_scope_revision:project.scope_revision,expected_membership_revision:null,membership:{project_id:managed.id,index_id:id,revision:1,scope:scopes.get(id)}});
                project=await detail('project',managed.id);
            }
        }
        chosenProjects.push(managed!.id);
    }
    if(chosenProjects.length>32)throw new Error('search_scope_too_many_sets');
    await adapter.refreshContext();adapter.current();
    const fresh=validateNativeHostContext(adapter.context());
    if(fresh.namespace!==namespace||fresh.catalog_revision!==revision||fresh.host!==context.host||fresh.key!==context.key)throw new Error('search_scope_catalog_changed_refresh');
    const review=await control.catalogGrantDraft(context.key,{project_ids:chosenProjects.sort(),index_ids:null},revision);check();
    if(review.request.key.namespace!==namespace||review.request.expected_catalog_revision!==revision||review.request.operation.operation!=='create_grant')throw new Error('crossed_search_scope_grant');
    const outcome=await control.catalogApply(review,()=>{try{adapter.current();return true;}catch{return false;}});adapter.current();
    if(outcome.status!=='catalog_applied'||catalogObject(outcome.receipt).catalog_revision!==revision+1)throw Object.assign(new Error('search_scope_outcome_unconfirmed'),{nativeResponse:outcome});
    steps.push({review,outcome});revision++;
    await adapter.refreshContext();adapter.current();
    const grant=catalogObject(review.request.operation.grant);
    const activated=await adapter.use({grant_id:grant.id,expected_grant_revision:grant.revision,project_ids:chosenProjects.sort()},namespace,revision);
    return {status:'search_scope_selected',selected:selection,steps,observations,activation:activated};
}

/** Index-set editing retains an existing membership's folder boundary. Checking
 * a newly added index selects its complete enabled source roots. */
export async function saveLocalIndexSet(adapter:LocalScopeAdapter,input:unknown):Promise<CatalogObject>{
    const selection=catalogObject(input);catalogBound(selection,8192);
    if(Object.keys(selection).some(key=>!['id','name','index_ids'].includes(key))||typeof selection.name!=='string'||!selection.name.trim()||Buffer.byteLength(selection.name)>256)throw new Error('invalid_index_set');
    const chosen=localScopeChoice({index_ids:selection.index_ids,project_ids:[]});
    const existing=selection.id===undefined?undefined:catalogId(selection.id,'prj_');
    if(existing!==undefined&&existing===adapter.managed()?.id)throw new Error('managed_scope_set_is_not_editable');
    const id=existing??`prj_${randomUUID().replaceAll('-','')}`,native=validateNativeHostContext(adapter.context());
    let revision=native.catalog_revision;
    const steps:{review:CatalogReview;outcome:CatalogObject}[]=[],observations:CatalogObject[]=[];
    const read=async(method:CatalogObject,field:'page'|'detail')=>{
        adapter.current();const result=await adapter.control.catalogRead(method);adapter.current();catalogBound(result,65536);
        const body=catalogObject(result[field]);if(body.namespace!==native.namespace||body.revision!==revision)throw new Error('index_set_catalog_changed_refresh');
        catalogItems(result,field);observations.push(result);return result;
    };
    const project=async()=>{const result=await read({catalog_detail:{entity:{kind:'project',id},expected_catalog_revision:revision}},'detail');const item=catalogItems(result,'detail')[0];if(!item||item.kind!=='project'||item.value.id!==id)throw new Error('index_set_unavailable');return item.value;};
    const page=async(kind:'memberships'|'sources',index_id:string|null)=>{
        const values:CatalogObject[]=[];let after:unknown=null;const seen=new Set<string>();
        for(let attempt=0;attempt<16;attempt++){
            const result=await read({catalog_page:{request:{kind,index_id,project_id:kind==='memberships'?id:null,after,expected_catalog_revision:revision,limit:32,max_bytes:48*1024}}},'page');
            const expected=kind==='memberships'?'membership':'source',rows=catalogItems(result);if(rows.some(row=>row.kind!==expected))throw new Error('crossed_index_set_page');
            values.push(...rows.map(row=>row.value));if(values.length>128)throw new Error('index_set_metadata_limit');
            after=catalogObject(result.page).continuation;if(after===null)return values;
            const token=JSON.stringify(after);if(seen.has(token))throw new Error('index_set_continuation_repeated');seen.add(token);
        }
        throw new Error('index_set_metadata_limit');
    };
    const mutate=async(operation:CatalogObject)=>{
        adapter.current();if(adapter.control.catalogPending().length)throw new Error('catalog_outcome_unconfirmed_reconcile_first');
        const review=await adapter.control.catalogPrepare(operation,revision);adapter.current();
        if(review.request.key.namespace!==native.namespace||review.request.expected_catalog_revision!==revision||!sameCatalogValue(review.request.operation,operation))throw new Error('crossed_index_set_review');
        const outcome=await adapter.control.catalogApply(review,()=>{try{adapter.current();return true;}catch{return false;}});adapter.current();
        if(outcome.status!=='catalog_applied'||catalogObject(outcome.receipt).catalog_revision!==revision+1)throw Object.assign(new Error('index_set_outcome_unconfirmed'),{nativeResponse:outcome});
        steps.push({review,outcome});revision++;
    };
    // Validate selected indexes before the first durable metadata write.
    for(const index of chosen.index_ids){
        const result=await read({catalog_detail:{entity:{kind:'index',id:index},expected_catalog_revision:revision}},'detail');
        const row=catalogItems(result,'detail')[0];if(row?.kind!=='index'||row.value.id!==index||row.value.tombstone!==false||row.value.source_import!=='complete')throw new Error('index_set_index_unavailable');
    }
    if(!existing)await mutate({operation:'create_project',id,name:selection.name.trim()});
    let value=await project();const members=await page('memberships',null);
    if(value.name!==selection.name.trim()){await mutate({operation:'rename_project',id,expected_definition_revision:value.definition_revision,name:selection.name.trim()});value=await project();}
    for(const member of members){
        if(chosen.index_ids.includes(String(member.index_id)))continue;
        await mutate({operation:'remove_membership',project_id:id,index_id:member.index_id,expected_scope_revision:value.scope_revision,expected_membership_revision:member.revision});value=await project();
    }
    for(const index of chosen.index_ids){
        if(members.some(member=>member.index_id===index))continue;
        const sources=(await page('sources',index)).filter(root=>root.enabled===true);
        if(!sources.length||sources.some(root=>root.index_id!==index))throw new Error('index_set_sources_unavailable');
        await mutate({operation:'set_membership',expected_scope_revision:value.scope_revision,expected_membership_revision:null,membership:{project_id:id,index_id:index,revision:1,scope:sources.map(root=>({source_root_id:catalogId(root.id,'src_'),kind:'subtree',path:''}))}});value=await project();
    }
    const priorDefaults=value.default_indexes;
    // Existing defaults are an independent preference. Membership removal
    // already prunes invalid references natively; a rename must not widen them.
    if(!existing&&!sameCatalogValue(priorDefaults,chosen.index_ids))await mutate({operation:'set_project_defaults',project_id:id,expected_defaults_revision:value.defaults_revision,index_ids:chosen.index_ids});
    await adapter.refreshContext();adapter.current();
    return {status:'index_set_saved',id,selected_index_ids:chosen.index_ids,steps,observations};
}
