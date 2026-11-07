// SPDX-License-Identifier: Apache-2.0
import { useEffect, useRef, useState } from 'preact/hooks';
import { CatalogObject, CatalogView, catalogExport, catalogItems, catalogObject, catalogReviewPreview, sourceChangeObservation } from '../indexingCatalog';
import type { ScreenProps } from './workspaces';
import { request } from './api';
import { IndexSplitPanel } from './indexingSplit';
import { upgradePending } from './runtimeUpgrade';
import { canReviewPolicyUnblock, policyStatusLabels, validatePolicyImpact } from '../indexingCatalog';

/** Native metadata is rendered as escaped Preact text; complete replies stay available in details/export. */
export function IndexingScreen(props: ScreenProps) {
    const view=props.state.indexing??{generation:0,busy:false};
    const [indexTask,setIndexTask]=useState<'sources'|'split'|'memberships'|'settings'>('sources');
    const selectedPanel=useRef<HTMLElement>(null);
    const automaticRead=useRef('');
    const [error,setError]=useState(''),[name,setName]=useState(''),[filter,setFilter]=useState('');
    const [index,setIndex]=useState('');
    const [setIndexes,setSetIndexes]=useState<string[]>([]);
    const [newSetName,setNewSetName]=useState(''),[newSetIndexes,setNewSetIndexes]=useState<string[]>([]);
    const [scopeIndexes,setScopeIndexes]=useState<string[]>([]),[scopeProjects,setScopeProjects]=useState<string[]>([]);
    const [setupName,setSetupName]=useState(''),[setupPrimary,setSetupPrimary]=useState(''),[setupData,setSetupData]=useState(''),[setupCollection,setSetupCollection]=useState(''),[setupWatch,setSetupWatch]=useState(false);
    const [setupRoots,setSetupRoots]=useState<{alias:string;path:string}[]>([]);
    const actionLabels:Record<string,string>={keep_layout:'Keep this layout',review_reuse:'Review reuse',use_query_scope:'Choose a query scope',review_split:'Review an independent split',split_pending:'Split unavailable on this connection',removal_pending:'Review permanent exclusions in Sources',manual_selection:'Choose manually while observations are unknown'};
    const [situation,setSituation]=useState('keep');
    const [sourceAlias,setSourceAlias]=useState(''),[sourcePath,setSourcePath]=useState(''),[removeAlias,setRemoveAlias]=useState(''),[beginOperation,setBeginOperation]=useState('');
    const [policyRoot,setPolicyRoot]=useState(''),[policyKind,setPolicyKind]=useState<'exact'|'subtree'>('exact'),[policyPath,setPolicyPath]=useState(''),[unblockRule,setUnblockRule]=useState('');
    const scope=view.searchScope;
    const scopeSelection=JSON.stringify([scope?.selectedIndexIds,scope?.selectedProjectIds]);
    useEffect(()=>{setScopeIndexes(scope?.selectedIndexIds??[]);setScopeProjects(scope?.selectedProjectIds??[]);},[scopeSelection]);
    const scopeDirty=JSON.stringify([scopeIndexes.slice().sort(),scopeProjects.slice().sort()])!==JSON.stringify([(scope?.selectedIndexIds??[]).slice().sort(),(scope?.selectedProjectIds??[]).slice().sort()]);
    const toggle=(items:string[],id:string,checked:boolean)=>checked?[...items.filter(item=>item!==id),id]:items.filter(item=>item!==id);
    const run=async(work:()=>Promise<unknown>)=>{setError('');try{await work();}catch(failure){setError(failure instanceof Error?failure.message:String(failure));}};
    const preview=(operation:CatalogObject)=>run(()=>request('indexing.preview',{operation,generation:view.generation}));
    const connectionPending=['connecting','disconnected','incompatible','untrusted'].includes(props.state.connection);
    const disabled=view.busy||!props.state.trusted||props.state.nativeExecutionEnabled===false||upgradePending(props.state.runtimeUpgrade)||connectionPending;
    const tab=props.route.tab;
    const rows=catalogItems(view.page), retainedDetail=catalogItems(view.detail,'detail')[0];
    const detail=(tab==='library'&&retainedDetail?.kind==='index'||tab==='projects'&&retainedDetail?.kind==='project')?retainedDetail:undefined;
    const project=detail?.kind==='project'?detail.value:undefined, selectedIndex=detail?.kind==='index'?detail.value:undefined;
    useEffect(()=>{setPolicyRoot('');setPolicyPath('');setUnblockRule('');},[selectedIndex?.id]);
    const policySnapshot=view.policyStatus?.snapshot?catalogObject(view.policyStatus.snapshot):undefined;
    const policy=policySnapshot?catalogObject(policySnapshot.policy):undefined;
    const policyApplication=policySnapshot?.application?catalogObject(policySnapshot.application):undefined;
    const policyDenies=Array.isArray(policy?.denies)?policy.denies.map(catalogObject):[];
    const policyImpact=view.review?.request.operation.operation==='change_index_policy'?validatePolicyImpact(catalogReviewPreview(view.review).source_policy_impact):undefined;
    const adviceSelection=view.recommendations?.schema==='airk.recommendations/1'?catalogObject(view.recommendations.selected):undefined;
    const setSignature=JSON.stringify([view.setEditor?.id,view.setEditor?.name,view.setEditor?.indexIds]);
    useEffect(()=>{if(view.setEditor?.id===project?.id){setName(view.setEditor?.name??'');setSetIndexes(view.setEditor?.indexIds??[]);}},[setSignature,project?.id]);
    const sourceStatus=view.sourceObservation?sourceChangeObservation(view.sourceObservation):undefined,sourceJob=sourceStatus?.job?catalogObject(sourceStatus.job):undefined;
    const retirement=view.submittedReview?.request.operation.operation==='retire_index'?view.submittedReview:undefined;
    const retiredBefore=retirement?catalogReviewPreview(retirement).before:undefined;
    const retiredItem=Array.isArray(retiredBefore)?retiredBefore.map(item=>catalogObject(item)).find(item=>item.kind==='index'&&catalogObject(item.value).id===retirement?.request.operation.id):undefined;
    const retiredName=retiredItem?String(catalogObject(retiredItem.value).name):'Index';
    const sourceBegin=sourceStatus?String(catalogObject(catalogObject(sourceStatus.begin).metadata).operation_id):undefined;
    useEffect(()=>{if(sourceBegin)setBeginOperation(sourceBegin);},[sourceBegin]);
    useEffect(()=>{if(view.submittedReview?.request.operation.operation==='begin_index_source_change'&&view.outcome?.status==='catalog_applied'){const receipt=catalogObject(view.outcome.receipt);if(typeof receipt.operation_id==='string')setBeginOperation(receipt.operation_id);}},[view.outcome]);
    const pageKind=view.pageKind??(rows[0]?.kind==='project'?'projects':rows[0]?.kind==='index'?'indexes':undefined);
    const expectedPage=tab==='projects'?'projects':'indexes';
    const pageLoaded=!!view.page&&pageKind===expectedPage;
    const visibleRows=pageLoaded?rows:[];
    const next=(response?:CatalogObject)=>!!response&&!!catalogObject(response.page).continuation;
    const page=(kind:'projects'|'indexes'|'sources'|'memberships'|'grants',more=false,idx?:string,prj?:string)=>run(()=>request('indexing.page',{kind,next:more,...(idx?{index:idx}:{}),...(prj?{project:prj}:{})}));
    const readOwner=JSON.stringify([tab,props.state.selectedProject,props.state.selectionEpoch,props.state.trusted,props.state.nativeExecutionEnabled,props.state.runtimeUpgrade?.toVersion,props.state.runtimeUpgrade?.stage]);
    useEffect(()=>{
        if(connectionPending){automaticRead.current='';return;}
        if(disabled||automaticRead.current===readOwner)return;
        if(tab==='window'){
            if(!scope){automaticRead.current=readOwner;void run(()=>request('indexing.scope.read',{}));}
        }else{
            automaticRead.current=readOwner;
            if(tab==='projects'&&!scope)void run(async()=>{await request('indexing.scope.read',{});if(!pageLoaded&&!detail)await request('indexing.page',{kind:'projects',next:false});});
            else if(!pageLoaded&&!detail)void page(expectedPage);
        }
    },[readOwner,disabled,connectionPending,pageLoaded,detail,scope]);
    useEffect(()=>{setFilter('');},[tab]);
    useEffect(()=>{setIndexTask(view.splitFormOpen?'split':'sources');},[selectedIndex?.id,view.splitFormOpen]);
    useEffect(()=>{selectedPanel.current?.scrollIntoView?.({block:'start'});},[detail?.value.id,indexTask]);
    useEffect(()=>{if(project)void run(()=>request('indexing.set.read',{id:String(project.id)}));},[project?.id]);
    return <section class="indexing-screen">
        <p class="indexing-intro">{tab==='window'?'Choose the indexes and index sets to search in this VS Code window.':tab==='projects'?'Save reusable sets of indexes without copying index data.':'Create and manage local indexes. Change sources, split an index, or delete an index you no longer need.'}</p>
        {(error||view.error)&&<p class="notice" role="alert">{error||view.error}</p>}
        {view.busy&&<p role="status">Loading native catalog details…</p>}
        {!props.state.trusted&&<p class="notice">Trust this workspace to load and manage indexes.</p>}
        {props.state.nativeExecutionEnabled===false&&<p class="notice">Enable native execution in AIRK settings to load and manage indexes.</p>}
        {connectionPending&&props.state.trusted&&<p role="status">{props.state.connection==='connecting'?'Connecting to the native service before loading the catalog…':'The native service is unavailable. Reconnect in Management → Service; the catalog will load when the connection is ready.'}</p>}
        {props.route.tab==='window'?<><article class="card"><h2>Search scope</h2>
            <p>Check the indexes and index sets you want to search, then apply your selection. Shared indexes are searched once.</p>
            <button disabled={disabled} onClick={()=>void run(()=>request('indexing.scope.read',{}))}>Refresh choices</button>
            {!scope&&!view.busy&&<p>Search choices have not loaded. Use Refresh choices to retry.</p>}
            {scope&&<form onSubmit={event=>{event.preventDefault();void run(async()=>{await request('indexing.scope.apply',{generation:view.generation,selection:{index_ids:scopeIndexes,project_ids:scopeProjects}});await request('indexing.scope.read',{});});}}>
                <fieldset disabled={disabled} class="indexing-scope-list"><legend>Indexes</legend>
                    {scope.indexes.filter(item=>item.tombstone!==true).map(item=><label key={String(item.id)}><input type="checkbox" checked={scopeIndexes.includes(String(item.id))} onChange={event=>setScopeIndexes(items=>toggle(items,String(item.id),event.currentTarget.checked))}/><span><strong>{String(item.name??item.id)}</strong>{typeof catalogObject(item.backing??{}).root==='string'&&<small>{String(catalogObject(item.backing).root)}</small>}</span></label>)}
                    {!scope.indexes.some(item=>item.tombstone!==true)&&<p>No indexes on this page. Create one in Index library.</p>}
                    {scope.indexContinuation&&<button type="button" disabled={disabled} onClick={()=>void run(()=>request('indexing.scope.read',{kind:'indexes',next:true}))}>More indexes</button>}
                </fieldset>
                <fieldset disabled={disabled} class="indexing-scope-list"><legend>Index sets</legend>
                    {scope.projects.filter(item=>!scope.managedProjectIds?.includes(String(item.id))).map(item=><label key={String(item.id)}><input type="checkbox" checked={scopeProjects.includes(String(item.id))} onChange={event=>setScopeProjects(items=>toggle(items,String(item.id),event.currentTarget.checked))}/>{String(item.name??item.id)}</label>)}
                    {!scope.projects.some(item=>!scope.managedProjectIds?.includes(String(item.id)))&&<p>No index sets on this page. Create one in Index sets.</p>}
                    {scope.projectContinuation&&<button type="button" disabled={disabled} onClick={()=>void run(()=>request('indexing.scope.read',{kind:'projects',next:true}))}>More index sets</button>}
                </fieldset>
                <p>{scopeIndexes.length} {scopeIndexes.length===1?'index':'indexes'} and {scopeProjects.length} {scopeProjects.length===1?'index set':'index sets'} selected. Uncheck everything to stop searching indexes in this window.</p>
                <p role="status">{scopeDirty?'Changes not applied.':scope.active?'Selection active in this window.':'Apply your selection to use it in this window.'}</p>
                <button disabled={disabled||!scopeDirty&&scope.active}>Apply selection</button>
                <p class="muted">Existing manual coding-agent configurations keep their own selection.</p>
            </form>}
        </article>
        <details class="card" id="recommendations"><summary>Help me choose indexes</summary><h2>Recommendations</h2>
            <p>Choose what to reuse and query using authorized saved metadata. Readiness and costs without dated observations remain unknown. This read starts no source scan, evidence query or model.</p>
            <form onSubmit={event=>{event.preventDefault();void run(()=>request('indexing.recommendations',{arguments:{situation}}));}}>
                <label>Situation<select value={situation} onChange={event=>setSituation(event.currentTarget.value)}>
                    <option value="keep">Keep this layout</option><option value="reuse">Reuse existing evidence</option><option value="focused_task">One application or package</option><option value="shared_dependencies">Shared code or specifications</option><option value="different_lifecycles">Different lifecycle or policy needs</option><option value="overlap">Possible overlapping indexes</option><option value="irrelevant_path">Path irrelevant to this task</option><option value="never_index_path">Path should never be indexed</option>
                </select></label>


                <p>Recommendations use the current search selection.</p>
                <button disabled={disabled}>Read recommendations</button>
            </form>
            {view.recommendations&&(view.recommendations.schema==='airk.recommendations/1'?<>
                <p>Available to this connection: {Array.isArray(view.recommendations.available)?view.recommendations.available.length:0} unique indexes. Selected for this query: {Array.isArray(catalogObject(view.recommendations.selected).index_ids)?(catalogObject(view.recommendations.selected).index_ids as unknown[]).length:0}.</p>
                <p>Omitted indexes: {JSON.stringify(view.recommendations.omitted_index_ids)}. Observation: {String(catalogObject(view.recommendations.observation).source)} at {new Date(Number(view.recommendations.observed_unix_ms)).toLocaleString()}. Connection scope only; machine library coverage is incomplete.</p>
                <details><summary>Available indexes, Index set names, selected source and prefix boundaries</summary><pre>{JSON.stringify({available:view.recommendations.available,projects:view.recommendations.projects,source_boundaries:view.recommendations.source_boundaries,selected_scope:view.recommendations.selected_scope},null,2)}</pre></details>
                {Array.isArray(view.recommendations.cards)&&view.recommendations.cards.map((item,at)=>{const card=catalogObject(item);return <article key={at}><h3>{actionLabels[String(card.action)]??String(card.action)}</h3><p>{String(card.reason)}</p><p>{String(card.expected_benefit)}</p><p>{String(card.tradeoff)}</p>{card.action==='review_split'&&view.canReviewSplit&&Array.isArray(adviceSelection?.index_ids)&&adviceSelection.index_ids.map(id=><button key={String(id)} disabled={disabled} onClick={()=>void run(async()=>{await request('indexing.split.open',{index:String(id),generation:view.generation});props.go({workspace:'indexing',tab:'library'});})}>Open split review for {String(id)}</button>)}</article>;})}
                <button disabled={disabled} onClick={()=>void run(()=>request('indexing.recommendations.use',{}))}>Use this query scope</button>
                <button disabled={disabled} onClick={()=>void run(()=>request('indexing.recommendations.copy',{}))}>Copy agent scope instructions</button>
                <p>Use this query scope prefills a temporary Workbench form. Review it before running. It does not change an agent connection or replace saved session choices automatically.</p>
                <p>Save reusable choices in Index sets. Split recommendations open a separate review in Index library.</p>
                <button disabled={disabled} onClick={()=>props.go({workspace:'indexing',tab:'projects'})}>Manage index sets</button>
                <details><summary>Complete Recommendations response</summary><pre>{JSON.stringify(view.recommendations,null,2)}</pre></details>
            </>:<p>Recommendations unavailable on this connection. Review the complete native response: <code>{JSON.stringify(view.recommendations)}</code></p>)}
            <button disabled={disabled} onClick={()=>void run(()=>request('indexing.recommendations.dismiss',{}))}>Keep this layout / Dismiss</button>
        </details>
        </>:<>
        <article class="card"><h2>{props.route.tab==='projects'?'Index sets':'Index library'}</h2>
            <p>{tab==='projects'?'Create reusable selections of indexes. Choose a set in Search scope to search its indexes together.':'Open an index to manage it, or delete one you no longer need.'} </p>
            <button disabled={disabled} onClick={()=>void page(props.route.tab==='projects'?'projects':'indexes')}>Refresh {props.route.tab==='projects'?'index sets':'indexes'}</button>
            <label>Filter displayed page<input value={filter} onInput={event=>setFilter(event.currentTarget.value)} /></label>
            <p class="muted">Showing one page. Use Next page to see more. The filter applies only to this displayed page.</p>
            {visibleRows.filter(row=>row.kind!=='project'||!scope?.managedProjectIds?.includes(String(row.value.id))).filter(row=>JSON.stringify(row.value).toLocaleLowerCase().includes(filter.toLocaleLowerCase())).map(row=><article class="card" key={String(row.value.id)}><strong>{String(row.value.name??row.value.id)}</strong><details><summary>Identity</summary><code>{String(row.value.id)}</code></details>
                {row.kind==='index'&&<p>{String(catalogObject(row.value.backing).root??'Source location is in complete details')}</p>}
                <div class="actions"><button disabled={disabled} onClick={()=>void run(()=>request('indexing.detail',{kind:row.kind==='project'?'project':'index',id:String(row.value.id)}))}>{row.kind==='project'?'Open index set':'Open index'}</button>
                {row.kind==='index'&&row.value.tombstone===false&&<button disabled={disabled||!view.canReviewSplit} onClick={()=>void run(()=>request('indexing.split.open',{index:String(row.value.id),generation:view.generation}))}>Split index</button>}{row.kind==='index'&&row.value.tombstone===false&&<button class="danger" disabled={disabled||!view.canReviewSource} onClick={()=>void run(async()=>{const current=await request<CatalogView>('indexing.detail',{kind:'index',id:String(row.value.id)});await request('indexing.source.draft',{generation:current.generation,change:{operation:'retire_index'}});})}>Delete index</button>}</div>
                {row.kind==='index'&&row.value.tombstone===true&&<p>Retired index</p>}</article>)}
            {pageLoaded&&next(view.page)&&<button disabled={disabled} onClick={()=>void page(props.route.tab==='projects'?'projects':'indexes',true)}>Next page</button>}
            {pageLoaded&&rows.length===0&&<p>No {tab==='projects'?'Index sets':'indexes'} on this catalog page.</p>}
            {pageLoaded&&rows.length>0&&visibleRows.every(row=>!JSON.stringify(row.value).toLocaleLowerCase().includes(filter.toLocaleLowerCase()))&&<p>No matches on this page. Clear the filter or read the next page.</p>}
            {!pageLoaded&&!view.busy&&<p>The {tab==='projects'?'Index set list':'index library'} has not loaded. Use Refresh above to retry. A failed read does not mean the library is empty.</p>}
            {props.route.tab==='projects'&&<details><summary>Create an index set</summary><form onSubmit={event=>{event.preventDefault();void run(async()=>{const saved=await request<CatalogObject>('indexing.set.save',{selection:{name:newSetName,index_ids:newSetIndexes},generation:view.generation});await request('indexing.scope.read',{});await request('indexing.page',{kind:'projects',next:false});if(typeof saved.id==='string')await request('indexing.set.read',{id:saved.id});setNewSetName('');setNewSetIndexes([]);});}}>
                <label>Index set name<input required maxLength={256} value={newSetName} onInput={event=>setNewSetName(event.currentTarget.value)} /></label>
                <fieldset disabled={disabled} class="indexing-scope-list"><legend>Include indexes</legend>{scope?.indexes.filter(item=>item.tombstone!==true).map(item=><label key={String(item.id)}><input type="checkbox" checked={newSetIndexes.includes(String(item.id))} onChange={event=>setNewSetIndexes(items=>toggle(items,String(item.id),event.currentTarget.checked))}/>{String(item.name??item.id)}</label>)}</fieldset>
                {scope?.indexContinuation&&<button type="button" disabled={disabled} onClick={()=>void run(()=>request('indexing.scope.read',{kind:'indexes',next:true}))}>More indexes</button>}
                <button disabled={disabled||!view.revision||!scope}>Create index set</button>
            </form></details>}
            {props.route.tab==='library'&&<details><summary>Set up a new index</summary>{view.canReviewSetup?<form onSubmit={event=>{event.preventDefault();void run(()=>request('indexing.setup.draft',{generation:view.generation,selection:{name:setupName,primary_root:setupPrimary,data_root:setupData,collection:setupCollection,additional_roots:setupRoots,setup_watch:setupWatch}}));}}>
                <h3>Set up a new index</h3><p>Choose source and index-storage locations explicitly. Native review resolves their actual locations and storage ownership. Use a new index storage folder and collection name, including when replacing a deleted index. Source folders may overlap another index.</p>
                <label>Index name<input required value={setupName} onInput={event=>setSetupName(event.currentTarget.value)} /></label>
                <label>Primary source location<input required value={setupPrimary} onInput={event=>setSetupPrimary(event.currentTarget.value)} /></label>
                <label>New index storage folder<input required value={setupData} onInput={event=>setSetupData(event.currentTarget.value)} /></label>
                <label>New collection name<input required value={setupCollection} onInput={event=>setSetupCollection(event.currentTarget.value)} /></label>
                {setupRoots.map((root,at)=><fieldset key={at} disabled={disabled}><legend>Additional source {at+1}</legend>
                    <label>Alias<input required value={root.alias} onInput={event=>{const alias=event.currentTarget.value;setSetupRoots(rows=>rows.map((row,i)=>i===at?{...row,alias}:row));}} /></label>
                    <label>Source location<input required value={root.path} onInput={event=>{const path=event.currentTarget.value;setSetupRoots(rows=>rows.map((row,i)=>i===at?{...row,path}:row));}} /></label>
                    <button type="button" onClick={()=>setSetupRoots(rows=>rows.filter((_,i)=>i!==at))}>Remove additional source {at+1}</button>
                </fieldset>)}
                <button type="button" disabled={disabled||setupRoots.length>=16} onClick={()=>setSetupRoots(rows=>[...rows,{alias:'',path:''}])}>Add source location</button>
                <label><input type="checkbox" checked={setupWatch} onChange={event=>setSetupWatch(event.currentTarget.checked)} />Enable native watching after setup</label>
                {!view.revision&&<p>Refresh the index library before preparing setup.</p>}
                <button disabled={disabled||!view.revision}>Review new index</button>
            </form>:<p>Index creation is unavailable on this connection. Reconnect in Management → Service.</p>}</details>}
        </article>
        {detail&&<article class="card indexing-detail" ref={selectedPanel}><h2>{detail.kind==='project'?'Index set':'Index'}: {String(detail.value.name)}</h2><details><summary>Identity</summary><code>{String(detail.value.id)}</code></details>
            {selectedIndex&&selectedIndex.tombstone!==true&&<div class="indexing-delete"><p>Deleting an index stops maintenance and removes its searchable data. Original files stay in place. You can create a different index over the same folders.</p><button class="danger" disabled={disabled||selectedIndex.tombstone===true||!view.canReviewSource} onClick={()=>void run(()=>request('indexing.source.draft',{generation:view.generation,change:{operation:'retire_index'}}))}>Delete index</button></div>}
            {selectedIndex&&<nav class="actions indexing-tasks" aria-label="Selected index actions">
                {([['sources','Sources and exclusions'],['split','Split index'],['memberships','Index sets using this index'],['settings','Index settings']] as const).map(([task,label])=><button type="button" aria-pressed={indexTask===task} onClick={()=>setIndexTask(task)}>{label}</button>)}
            </nav>}
            {selectedIndex&&<div hidden={indexTask!=='split'}><IndexSplitPanel key={String(selectedIndex.id)} view={view} index={selectedIndex} disabled={disabled} run={run} upgrade={props.state.runtimeUpgrade}/></div>}
            {selectedIndex&&indexTask==='settings'&&<details><summary>Rename {selectedIndex?'index':'Index set'}</summary>
            <form onSubmit={event=>{event.preventDefault();void preview({operation:detail.kind==='project'?'rename_project':'rename_index',id:detail.value.id,expected_definition_revision:detail.value.definition_revision,name});}}><label>New name<input required maxLength={256} value={name} onInput={event=>setName(event.currentTarget.value)} /></label><button disabled={disabled}>Preview rename</button></form></details>}
            {selectedIndex&&indexTask==='sources'&&<button disabled={disabled} onClick={()=>void page('sources',false,String(selectedIndex.id))}>Read Sources</button>}
            {selectedIndex&&indexTask==='sources'&&(view.canReviewSource?<article><h3>Manage index sources</h3><p>Native review freezes actual source identities, configuration and policy. Preparing a draft makes no changes. Applying a root change begins a native configuration job; a begin receipt does not mean the root update has completed.</p>
                {view.canReadPendingSource?<button disabled={disabled} onClick={()=>void run(()=>request('indexing.source.pending',{generation:view.generation}))}>Read pending source change for this index</button>:<p class="muted">Current-change lookup by index is unavailable on this connection. A known native begin operation ID can be read below; no operation history is inferred from source status.</p>}
                <form onSubmit={event=>{event.preventDefault();void run(()=>request('indexing.source.draft',{generation:view.generation,change:{operation:'add_root',alias:sourceAlias,path:sourcePath}}));}}>
                    <label>Additional source alias<input required value={sourceAlias} onInput={event=>setSourceAlias(event.currentTarget.value)} /></label><label>Source location<input required value={sourcePath} onInput={event=>setSourcePath(event.currentTarget.value)} /></label>
                    <button disabled={disabled||selectedIndex.tombstone===true}>Prepare native add-source review</button>
                </form>
                <form onSubmit={event=>{event.preventDefault();void run(()=>request('indexing.source.draft',{generation:view.generation,change:{operation:'remove_root',alias:removeAlias}}));}}>
                    <label>Alias to remove<input required list="source-removal-aliases" value={removeAlias} onInput={event=>setRemoveAlias(event.currentTarget.value)} /></label>
                    <datalist id="source-removal-aliases">{catalogItems(view.sources).filter(row=>row.value.index_id===selectedIndex.id).flatMap(row=>Array.isArray(row.value.aliases)?row.value.aliases.map(alias=><option value={String(alias)} />):[])}</datalist>
                    <button disabled={disabled||selectedIndex.tombstone===true}>Prepare native remove-source review</button>
                </form>

                {view.canReadPolicy&&<article><h3>Permanent path exclusions</h3>
                    <p>Block an exact file or a directory and its future descendants in this index. Every Index set using this index is affected. Other indexes covering the same source remain separate. Source files are preserved.</p>
                    <button disabled={disabled} onClick={()=>void run(()=>request('indexing.policy.read',{generation:view.generation}))}>Read permanent exclusions and cleanup status</button>
                    <form onSubmit={event=>{event.preventDefault();void run(()=>request('indexing.source.draft',{generation:view.generation,change:{operation:'trim_path',source_root_id:policyRoot,kind:policyKind,path:policyPath}}));}}>
                        <label>Source root<select required value={policyRoot} onChange={event=>setPolicyRoot(event.currentTarget.value)}><option value="">Read Sources, then choose a root</option>{catalogItems(view.sources).filter(row=>row.value.index_id===selectedIndex.id&&row.value.enabled===true).map(row=><option key={String(row.value.id)} value={String(row.value.id)}>{String(row.value.canonical_path)}</option>)}</select></label>
                        <label>Block<select value={policyKind} onChange={event=>setPolicyKind(event.currentTarget.value as 'exact'|'subtree')}><option value="exact">Exact file</option><option value="subtree">Directory and descendants</option></select></label>
                        <label>Path relative to the selected root<input required={policyKind==='exact'} value={policyPath} onInput={event=>setPolicyPath(event.currentTarget.value)} placeholder="private/notes.md" /></label>
                        {policyKind==='subtree'&&<p>A blank directory path blocks the entire selected source root.</p>}
                        <button disabled={disabled||!policyRoot||selectedIndex.tombstone===true}>Prepare permanent-block review</button>
                    </form>
                    {view.policyStatus&&<><p>{policyStatusLabels(view.policyStatus).join(' ')}</p>
                        {view.policyStatus.job&&<p>Recorded cleanup job: {String(catalogObject(view.policyStatus.job).state)}.</p>}
                        {policySnapshot?.last_error!==null&&policySnapshot?.last_error!==undefined&&<p class="notice">{String(policySnapshot.last_error)}</p>}
                        {view.canResumePolicy&&selectedIndex.tombstone!==true&&view.policyStatus.job&&(['failed','canceled'].includes(String(catalogObject(view.policyStatus.job).state))||catalogObject(view.policyStatus.job).state==='succeeded'&&policyApplication?.physical_cleanup_pending===true)&&<button disabled={disabled||!!view.pending?.length} onClick={()=>void run(()=>request('indexing.policy.resume',{generation:view.generation}))}>Review resuming recorded cleanup</button>}
                        {!!policyDenies.length&&<form onSubmit={event=>{event.preventDefault();void run(()=>request('indexing.source.draft',{generation:view.generation,change:{operation:'unblock_path',rule_id:unblockRule}}));}}>
                            <label>Exclusion to unblock<select required value={unblockRule} onChange={event=>setUnblockRule(event.currentTarget.value)}><option value="">Choose a recorded exclusion</option>{policyDenies.filter(rule=>rule.reason==='trim').map(rule=><option key={String(rule.rule_id)} value={String(rule.rule_id)}>{String(rule.kind)}: {String(rule.path)||'(entire source root)'} · {String(rule.source_root_id)}</option>)}</select></label>
                            <button disabled={disabled||!unblockRule||selectedIndex.tombstone===true||!canReviewPolicyUnblock(view.policyStatus)}>Prepare unblock review</button>
                        </form>}
                        <details><summary>Complete native policy and cleanup response</summary><pre>{JSON.stringify(view.policyStatus,null,2)}</pre></details>
                    </>}
                    <p>Preparing a review changes nothing. A completed native policy receipt establishes the exclusion; removal and physical cleanup are reported separately. Cleanup states retain their observation time; later background cleanup is not inferred. Previously delivered results, exports and upgrade backups cannot be recalled. Physical cleanup is not secure erasure.</p>
                </article>}
            </article>:<p>Source changes and index deletion are unavailable on this connection.</p>)}
            {(!selectedIndex||indexTask==='memberships')&&<button disabled={disabled} onClick={()=>void page('memberships',false,selectedIndex?String(selectedIndex.id):undefined,project?String(project.id):undefined)}>Load Index set memberships</button>}
            {project&&<form onSubmit={event=>{event.preventDefault();void run(async()=>{await request('indexing.set.save',{selection:{id:String(project.id),name,index_ids:setIndexes},generation:view.generation});await request('indexing.scope.read',{});await request('indexing.page',{kind:'projects',next:false});await request('indexing.set.read',{id:String(project.id)});});}}>
                <label>Index set name<input required maxLength={256} value={name} onInput={event=>setName(event.currentTarget.value)} /></label>
                <p>Newly added indexes include all their sources. Existing folder selections stay unchanged.</p>
                {!view.setEditor||view.setEditor.id!==project.id||!view.setEditor.complete?<div><p>Complete index set membership is needed before editing.</p><button type="button" disabled={disabled} onClick={()=>void run(()=>request('indexing.set.read',{id:String(project.id)}))}>Retry loading index set</button></div>:<fieldset disabled={disabled} class="indexing-scope-list"><legend>Include indexes</legend>{scope?.indexes.filter(item=>item.tombstone!==true).map(item=><label key={String(item.id)}><input type="checkbox" checked={setIndexes.includes(String(item.id))} onChange={event=>setSetIndexes(items=>toggle(items,String(item.id),event.currentTarget.checked))}/>{String(item.name??item.id)}</label>)}</fieldset>}
                {scope?.indexContinuation&&<button type="button" disabled={disabled} onClick={()=>void run(()=>request('indexing.scope.read',{kind:'indexes',next:true}))}>More indexes</button>}
                <button disabled={disabled||!scope||!view.setEditor||view.setEditor.id!==project.id||!view.setEditor.complete}>Save index set</button>
                <button type="button" class="danger" disabled={disabled} onClick={()=>void preview({operation:'delete_project',id:project.id,expected_definition_revision:project.definition_revision,expected_scope_revision:project.scope_revision,expected_defaults_revision:project.defaults_revision})}>Delete index set</button>
                <p class="muted">Deleting a set keeps its indexes and original files.</p>
            </form>}

        </article>}
        {view.sources&&detail&&(!selectedIndex||indexTask==='sources')&&<article class="card"><h3>Sources</h3>{catalogItems(view.sources).map(row=><p key={String(row.value.id)}><code>{String(row.value.id)}</code> {String(row.value.canonical_path)} · {row.value.enabled?'Enabled':'Disabled'}</p>)}{next(view.sources)&&<button disabled={disabled} onClick={()=>void page('sources',true,selectedIndex?String(selectedIndex.id):index)}>Next Sources page</button>}</article>}
        {view.memberships&&detail&&(!selectedIndex||indexTask==='memberships')&&<details class="card"><summary>Complete index set membership details</summary><pre>{JSON.stringify(view.memberships,null,2)}</pre>{next(view.memberships)&&<button disabled={disabled} onClick={()=>void page('memberships',true,selectedIndex?String(selectedIndex.id):undefined,project?String(project.id):undefined)}>More memberships</button>}</details>}

        </>}
        <details class="card indexing-recovery" open={!!view.pending?.length||!!view.retirementReview}><summary>Operation status and recovery{view.pending?.length?` · ${view.pending.length} unconfirmed requests`:""}</summary>
            <p>Look up work that was already submitted. Status reads do not start or repeat an operation.</p>
            <IndexSplitPanel view={view} disabled={disabled} run={run} upgrade={props.state.runtimeUpgrade} recoveryOnly/>
        <article class="card"><h2>Source-change outcome</h2><p>Read the recorded native operation explicitly. Job history and metadata completion are separate; resolving metadata never rewrites a failed job as successful.</p>
            {view.sourceObservation?.status==='catalog_pending_source_change'&&view.sourceObservation.observation===null&&<p>No current source-change observation was returned for this index. This does not establish readiness or settle another request.</p>}
            <form onSubmit={event=>{event.preventDefault();void run(()=>request('indexing.source.read',{begin:beginOperation,generation:view.generation}));}}><label>Native begin operation ID<input required pattern="[0-9a-f]{32}" value={beginOperation} onInput={event=>setBeginOperation(event.currentTarget.value)} /></label><button disabled={disabled||!view.namespace}>Read native source-change observation</button></form>
            {sourceStatus&&<><p>Index <code>{String(catalogObject(catalogObject(catalogObject(sourceStatus.begin).request).operation).id)}</code> · begin <code>{sourceBegin}</code>.</p><p>Original configuration job: {String(sourceJob?.state??'unavailable')}. Metadata: {sourceStatus.completion?'resolved':'completion pending'}.</p><pre>{JSON.stringify(view.sourceObservation,null,2)}</pre>
                {sourceJob?.state==='failed'&&!sourceStatus.completion?<><p>Explicit resolution asks the native owner to prove and record the desired source configuration. This form accepts no filesystem or configuration proof.</p><button disabled={disabled||!!sourceBegin&&!!view.pending?.includes(sourceBegin)} onClick={()=>void run(()=>request('indexing.source.resolve',{generation:view.generation}))}>Resolve failed source metadata natively</button></>:!sourceStatus.completion&&<p>Desired-only resolution is unavailable for this phase. Finish or explicitly cancel accepted native work in Management.</p>}
            </>}
        </article>
        {!!view.pending?.length&&<article class="card"><h2>Unconfirmed requests</h2><p>{view.pending.length} saved request(s). Their outcomes are unconfirmed. Receipt lookup never repeats the apply; a missing receipt keeps the request unresolved.</p><button disabled={disabled} onClick={()=>void run(()=>request('indexing.reconcile',{}))}>Check native receipts</button>{view.pendingDetails&&<details><summary>Saved exact requests and native observations</summary><pre>{JSON.stringify(view.pendingDetails,null,2)}</pre></details>}</article>}
        {view.pendingDetails?.filter(row=>'catalog_index_setup_apply' in catalogObject(row.intent)).map(row=><article class="card" key={String(row.id)}><h3>Recover saved index setup</h3><p>Review the original exact native draft and latest receipt. Resume is available only after a fresh matching native receipt proves a known reserved or prepared stage. It reuses the same immutable request, key and digest.</p><pre>{JSON.stringify(row,null,2)}</pre>
            <button disabled={disabled||row.can_resume_setup!==true} onClick={()=>void run(()=>request('indexing.setup.resume',{id:String(row.id),generation:view.generation}))}>Resume saved native setup</button>
            {row.can_resume_setup!==true&&<p>Check native receipts first. Unknown or missing receipts never authorize Resume. If the index was retired, this request remains retained until matching native terminal proof can settle it; a new key is not recovery.</p>}
        </article>)}
        {view.canReviewRetirement&&view.pendingDetails?.filter(row=>{const intent=catalogObject(row.intent);return 'catalog_index_setup_apply' in intent||'catalog_source_change_resolve' in intent;}).map(row=><article class="card" key={`retirement-${String(row.id)}`}><h3>Check retirement recovery</h3><p>If this index was explicitly retired, read its full native retirement proof before stopping local tracking. This does not complete setup, resolve the failed job, or remove files. A missing proof keeps the original request tracked.</p><button disabled={disabled} onClick={()=>void run(()=>request('indexing.retirement.read',{id:String(row.id),generation:view.generation}))}>Read native retirement evidence</button></article>)}
        {view.retirementReview&&<article class="card"><h2>Review native retirement evidence</h2><pre>{JSON.stringify(view.retirementReview,null,2)}</pre>{view.retirementReview.response.evidence===null?<p>No matching native retirement evidence was returned. The original request remains tracked.</p>:<><p>Stop tracking only this superseded local request. Its original setup or source resolution is not reported as successful. No native work or file cleanup is issued.</p><button disabled={disabled} onClick={()=>void run(()=>request('indexing.retirement.stopTracking',{generation:view.generation}))}>Stop local tracking after verified retirement</button></>}</article>}
        </details>
        {view.review&&<article class="card"><h2>{view.review.request.operation.operation==='retire_index'?'Delete this index?':'Review changes'}</h2>{view.review.request.operation.operation==='retire_index'?<p><strong>{String(selectedIndex?.name??visibleRows.find(row=>row.value.id===view.review?.request.operation.id)?.value.name??view.review.request.operation.id)}</strong>: This stops maintenance, removes the index from search and index sets, and schedules cleanup of its owned derived data. Original source files are preserved. You can create a new index over the same files without splitting; use a new index storage folder and collection name.</p>:<p>Review the changes before applying.</p>}<details><summary>Complete native review</summary><pre>{JSON.stringify(view.review.response,null,2)}</pre></details>
            {policyImpact&&<article><h3>Permanent-exclusion impact</h3>
                <p>Reviewed source root: <code>{String(policyImpact.canonical_root)}</code>. Path: <code>{String(catalogObject(policyImpact.target).path)||'(entire source root)'}</code> · {catalogObject(policyImpact.target).kind==='exact'?'Exact file':'Directory and descendants'}.</p>
                <dl>{[['affected_artifacts','Artifacts'],['affected_vectors','Vectors'],['affected_facts','Facts'],['affected_relationships','Relationships']].map(([key,label])=><><dt>{label}</dt><dd>{policyImpact[key]===null?'Unknown':String(policyImpact[key])}</dd></>)}</dl>
                <p>{String(policyImpact.counts_basis)}</p>
                <p>Affected source roots: {(policyImpact.affected_source_root_ids as string[]).join(', ')||'None'}. Affected Index sets: {(policyImpact.affected_project_ids as string[]).join(', ')||'None'}.</p>
                <p>Other indexes with configured-root overlap: {(policyImpact.other_covering_index_ids as string[]).join(', ')||'None returned'}. {String(policyImpact.overlap_basis)}</p>
                <p>These observations do not certify effective eligibility in another index. The change applies only to the reviewed index. Counts remain unknown when native review has not scanned retained evidence.</p>
                <p>When present, imported analysis and the embedding cache are retired for the entire index. Imported analysis is not automatically regenerated. The native outcome lists the components actually retired.</p>
            </article>}
            {catalogReviewPreview(view.review).authority_fence_required===true&&<p>AIRK will confirm that the search-access change has taken effect before completing this request. If it cannot confirm that, the result stays pending.</p>}
            <button disabled={disabled} onClick={()=>void run(async()=>{const result=await request<CatalogView>('indexing.apply',{generation:view.generation});if(result.outcome?.status==='catalog_applied')await request('indexing.page',{kind:tab==='projects'?'projects':'indexes',next:false});})}>{view.review.request.operation.operation==='retire_index'?'Delete index and stop maintenance':view.review.request.operation.operation==='create_grant'?'Persist reviewed grant':'Apply reviewed changes'}</button></article>}
        {view.submittedReview&&<details class="card"><summary>Last submitted native review</summary><pre>{JSON.stringify(view.submittedReview,null,2)}</pre></details>}
        {view.setupReview&&<article class="card"><h2>Review native index setup</h2><p>Review the exact native locations, source aliases, collection and storage action below. Applying can create or claim index storage. Source content remains read-only. Choose the new index in Search scope when you want to search it.</p><pre>{JSON.stringify(view.setupReview.response,null,2)}</pre>
            <button disabled={disabled} onClick={()=>void run(()=>request('indexing.setup.apply',{generation:view.generation}))}>Apply reviewed index setup</button></article>}
        {view.submittedSetupReview&&<details class="card"><summary>Last submitted native setup review</summary><pre>{JSON.stringify(view.submittedSetupReview,null,2)}</pre></details>}
        {view.outcome&&<article class="card"><h2>Operation result</h2>{retirement&&view.outcome.status==='catalog_applied'&&<p role="status"><strong>{retiredName}</strong> was deleted from search and maintenance. Original source files are preserved. Owned index-data cleanup runs separately; its completion has not been observed here. Review Storage in Management for cleanup status.</p>}{view.outcome.status==='index_set_saved'&&<p role="status">Index set saved.</p>}{view.outcome.status==='search_scope_selected'&&<p role="status">Search scope updated.</p>}{view.outcome.status==='source_change_busy'&&<p>{String(catalogObject(view.outcome.busy).pending_jobs)} queued and {String(catalogObject(view.outcome.busy).running_jobs)} running accepted jobs. Finish or explicitly cancel them in Management before preparing another source change. No jobs were canceled or reordered by this request.</p>}
            {Array.isArray(view.outcome.receipts)&&view.outcome.receipts.some(value=>catalogObject(value).status==='catalog_receipt_unprepared')&&<p>Native reconciliation proved that the exact catalog request was not prepared. Its local uncertain intent is retired; this is not a completed mutation receipt. Prepare and review any desired change explicitly. No Apply or new key was submitted automatically.</p>}
            {view.submittedReview?.request.operation.operation==='begin_index_source_change'&&view.outcome.status==='catalog_applied'&&<p>The native source-change begin was accepted. Read its source-change observation for the recorded configuration job and actual metadata completion.</p>}
            {view.submittedReview?.request.operation.operation==='change_index_policy'&&view.outcome.status==='catalog_applied'&&<p>The reviewed policy change was accepted. Read permanent exclusions and cleanup status for the recorded removal publication and physical cleanup outcome.</p>}
            <details><summary>Complete operation result</summary><pre>{JSON.stringify(view.outcome,null,2)}</pre></details></article>}
        <details class="card"><summary>Complete catalog details and exports</summary><h2>Complete catalog details</h2><p>Includes the complete retained native page, detail, window association, Sources, memberships, preview and outcome. Each page replaces the previously displayed page; exports describe only retained pages.</p>
            <button disabled={disabled} onClick={()=>void run(()=>request('indexing.export',{format:'copy'}))}>Copy complete JSON</button><button disabled={disabled} onClick={()=>void run(()=>request('indexing.export',{format:'json'}))}>Export JSON…</button><button disabled={disabled} onClick={()=>void run(()=>request('indexing.export',{format:'copy_markdown'}))}>Copy complete Markdown</button><button disabled={disabled} onClick={()=>void run(()=>request('indexing.export',{format:'markdown'}))}>Export Markdown…</button>
            <details><summary>Complete response</summary><pre>{catalogExport(view as CatalogView)}</pre></details>
        </details>
    </section>;
}
