// SPDX-License-Identifier: Apache-2.0
import { Notice, noticeKey } from './notices';
import { useEffect, useRef, useState } from 'preact/hooks';
import { catalogObject } from '../indexingCatalog';
import type { ScreenProps } from './workspaces';
import { request } from './api';
import { AdminButton } from './adminLinks';
import { LibraryView } from './libraryView';

/** Shown when the AIRK Service does not advertise `window-scope/1` (CAPABILITY_DISABLED). */
export const SCOPE_UNAVAILABLE='The window search scope is not available from this AIRK Service. Search uses the selected project.';
export const SCOPE_OUT_OF_DATE='Search scope out of date — reselect. Refresh the choices, then apply your selection again.';
/** Nothing selected releases the window scope (scope_release). */
export const CLEAR_SCOPE={generation:0,selection:{index_ids:[],project_ids:[]}};

/** The native window selection bound (windowScope.ts SCOPE_MAX_SELECTION, CatalogPageRequest MAX_SELECTION). */
export const SCOPE_MAX_SELECTION=32;

/** (U104) The scope is a union: every member of the checked index sets plus every checked index.
 * Apply is enabled for a changed selection, or for the automatic scope (applying it makes it the
 * user's choice), within the selection bound. With complete memberships the page shows which checked
 * indexes are outside the checked sets (`extra`, read through their own sets) and which are in no
 * index set (`newSets`: Apply first creates an index set named after each). Unknown memberships
 * leave the rest of the validation to the service. */
export function scopeApplyState(input:{indexes:readonly string[];projects:readonly string[];appliedIndexes:readonly string[];appliedProjects:readonly string[];automatic?:boolean;
    indexSets?:Readonly<Record<string,readonly string[]>>;indexSetsComplete?:boolean}):{changed:boolean;valid:boolean;enabled:boolean;extra:string[];newSets:string[];limit?:string}{
    const key=(items:readonly string[])=>JSON.stringify([...items].sort());
    const changed=key(input.indexes)!==key(input.appliedIndexes)||key(input.projects)!==key(input.appliedProjects);
    const known=input.indexSets&&input.indexSetsComplete===true?input.indexSets:undefined;
    let extra:string[]=[],newSets:string[]=[],limit:string|undefined;
    if(known){
        const members=Object.keys(known).filter(id=>known[id].some(set=>input.projects.includes(set)));
        extra=input.indexes.filter(id=>!members.includes(id));
        newSets=extra.filter(id=>!known[id]?.length);
        const sets=new Set(input.projects);for(const id of extra)for(const set of known[id]??[])sets.add(set);
        const setCount=sets.size+newSets.length,indexCount=members.length+extra.length;
        if(setCount>SCOPE_MAX_SELECTION)limit=`A search scope holds at most ${SCOPE_MAX_SELECTION} index sets; this selection needs ${setCount}. Check fewer index sets or indexes.`;
        else if(indexCount>SCOPE_MAX_SELECTION)limit=`A search scope holds at most ${SCOPE_MAX_SELECTION} indexes; this selection has ${indexCount}. Check fewer index sets or indexes.`;
    }else if(input.indexes.length>SCOPE_MAX_SELECTION||input.projects.length>SCOPE_MAX_SELECTION){
        limit=`Choose at most ${SCOPE_MAX_SELECTION} indexes and ${SCOPE_MAX_SELECTION} index sets.`;
    }
    const valid=!limit;
    return {changed,valid,enabled:valid&&(changed||input.automatic===true),extra,newSets,...(limit?{limit}:{})};
}

/**
 * Indexing (U97): the window search scope and recommendations, and a read-only
 * index library. Native metadata is rendered as escaped Preact text. Indexes and
 * sources are changed in AIRK Admin; (U104) the search scope creates an index
 * set for an index in none on Apply and saves the checked indexes as a new
 * named index set (`airkctl library set-create --index`).
 */
export function IndexingScreen(props: ScreenProps) {
    const view=props.state.indexing??{generation:0,busy:false};
    const automaticRead=useRef('');
    const workingNow=useRef(false),[working,setWorking]=useState(false);
    const [error,setError]=useState(''),[errorDetail,setErrorDetail]=useState('');
    const [scopeIndexes,setScopeIndexes]=useState<string[]>([]),[scopeProjects,setScopeProjects]=useState<string[]>([]);
    const [setName,setSetName]=useState('');
    const actionLabels:Record<string,string>={keep_layout:'Keep this layout',review_reuse:'Review reuse',use_query_scope:'Choose a query scope',review_split:'Review an independent split',split_pending:'Split unavailable on this connection',removal_pending:'Review permanent exclusions in AIRK Admin',manual_selection:'Choose manually while observations are unknown'};
    const [situation,setSituation]=useState('keep');
    const scope=view.searchScope;
    const scopeSelection=JSON.stringify([scope?.selectedIndexIds,scope?.selectedProjectIds]);
    useEffect(()=>{setScopeIndexes(scope?.selectedIndexIds??[]);setScopeProjects(scope?.selectedProjectIds??[]);},[scopeSelection]);
    const apply=scopeApplyState({indexes:scopeIndexes,projects:scopeProjects,appliedIndexes:scope?.selectedIndexIds??[],appliedProjects:scope?.selectedProjectIds??[],automatic:scope?.automatic===true,indexSets:scope?.indexSets,indexSetsComplete:scope?.indexSetsComplete});
    const scopeDirty=apply.changed;
    const indexName=(id:string)=>{const row=scope?.indexes.find(item=>String(item.id)===id);return String(row?.name??id);};
    const route=props.state.searchScope?.route;
    const toggle=(items:string[],id:string,checked:boolean)=>checked?[...items.filter(item=>item!==id),id]:items.filter(item=>item!==id);
    // Keep the complete action chain exclusive, including the fresh read after Apply.
    // A scope failure shows its message and keeps its code in Details; only an absent
    // window-scope/1 capability says the scope is not available.
    const run=async(work:()=>Promise<unknown>,scopeWork=false)=>{if(workingNow.current)return;workingNow.current=true;setWorking(true);setError('');setErrorDetail('');try{await work();}catch(failure){const text=failure instanceof Error?failure.message:String(failure),code=(failure as {error?:{code?:unknown}})?.error?.code;if(scopeWork&&code==='CAPABILITY_DISABLED'){setError(SCOPE_UNAVAILABLE);setErrorDetail(text);}else{setError(text);if(typeof code==='string')setErrorDetail(code);}}finally{workingNow.current=false;setWorking(false);}};
    const connectionPending=['connecting','disconnected','incompatible','untrusted'].includes(props.state.connection);
    const disabled=working||view.busy||!props.state.trusted||props.state.nativeExecutionEnabled===false||connectionPending;
    const tab=props.route.tab;
    const adviceSelection=view.recommendations?.schema==='airk.recommendations/1'?catalogObject(view.recommendations.selected):undefined;
    const readOwner=JSON.stringify([tab,props.state.selectedProject,props.state.selectionEpoch,props.state.trusted,props.state.nativeExecutionEnabled]);
    useEffect(()=>{
        if(connectionPending){automaticRead.current='';return;}
        if(tab!=='window'||disabled||workingNow.current||automaticRead.current===readOwner)return;
        if(!scope){automaticRead.current=readOwner;void run(()=>request('indexing.scope.read',{}),true);}
    },[readOwner,disabled,connectionPending,scope]);
    const shownError=error||(tab==='window'&&(scope?.outOfDate||props.state.searchScope?.outOfDate)?SCOPE_OUT_OF_DATE:'');
    const shownDetail=errorDetail;
    return <section class="indexing-screen">
        <div class="section-head"><p class="indexing-intro">{tab==='window'?'Choose the indexes and index sets to search in this VS Code window.':tab==='projects'?'Index sets on this computer. Create and change them in AIRK Admin.':'Indexes on this computer. Create, change and delete them in AIRK Admin.'}</p>
            {tab==='window'&&<span class="actions"><button type="button" class="ghost" onClick={()=>props.go({workspace:'manage',tab:'agents'})}>Connect an agent</button><AdminButton page="index-library" disabled={!props.state.trusted}/></span>}</div>
        {tab==='window'&&shownError&&<Notice noticeKey={noticeKey("indexing-error", shownError, shownDetail)} class="notice" role="alert"><p>{shownError}</p>{shownDetail&&<details><summary>Details</summary><code>{shownDetail}</code></details>}</Notice>}
        {!props.state.trusted&&<Notice global condition noticeKey="workspace-restricted" class="notice"><p>Trust this workspace to read indexes.</p></Notice>}
        {props.state.nativeExecutionEnabled===false&&<Notice global condition noticeKey="native-execution-disabled" class="notice"><p>Enable native execution in AIRK settings to read indexes.</p></Notice>}
        {tab==='window'?<>{view.busy&&<p role="status">Loading search choices…</p>}
        {connectionPending&&props.state.trusted&&<p role="status">{props.state.connection==='connecting'?'Connecting to the AIRK Service before loading search choices…':'The AIRK Service is unavailable. Search choices load when it is ready.'}</p>}
        <article class="card"><h2>Search scope</h2>
            <p>Check the indexes and index sets you want to search, then apply your selection. Shared indexes are searched once.</p>
            {route?.state==='scope'&&route.automatic&&<p role="status">This window reads its folder's index automatically: <strong>{route.indexName??route.indexId}</strong>. Apply a selection to choose another scope.</p>}
            {route?.state==='not_readable'&&<p class="callout" role="status">{route.indexName??route.indexId??'This window\'s index'} is in no index set, so it is not readable yet. Check it below and apply: Apply creates an index set named after it.</p>}
            <button disabled={disabled} onClick={()=>void run(()=>request('indexing.scope.read',{}),true)}>Refresh choices</button>
            {!scope&&!view.busy&&<p>Search choices have not loaded.{disabled?'':' Use Refresh choices to retry.'}</p>}
            {scope&&<form onSubmit={event=>{event.preventDefault();void run(async()=>{await request('indexing.scope.apply',{generation:view.generation,selection:{index_ids:scopeIndexes,project_ids:scopeProjects}});await request('indexing.scope.read',{});},true);}}>
                <fieldset disabled={disabled} class="indexing-scope-list"><legend>Indexes</legend>
                    {scope.indexes.filter(item=>item.tombstone!==true).map(item=><label key={String(item.id)}><input type="checkbox" checked={scopeIndexes.includes(String(item.id))} onChange={event=>setScopeIndexes(items=>toggle(items,String(item.id),event.currentTarget.checked))}/><span><strong>{String(item.name??item.id)}</strong>{typeof catalogObject(item.backing??{}).root==='string'&&<small>{String(catalogObject(item.backing).root)}</small>}{scope.indexSetsComplete&&!scope.indexSets?.[String(item.id)]?.length&&<small>In no index set: Apply creates an index set named after it</small>}</span></label>)}
                    {!scope.indexes.some(item=>item.tombstone!==true)&&<p>No indexes on this page. Create one in AIRK Admin.</p>}
                    {scope.indexContinuation&&<button type="button" disabled={disabled} onClick={()=>void run(()=>request('indexing.scope.read',{kind:'indexes',next:true}),true)}>More indexes</button>}
                </fieldset>
                <fieldset disabled={disabled} class="indexing-scope-list"><legend>Index sets</legend>
                    {scope.projects.map(item=><label key={String(item.id)}><input type="checkbox" checked={scopeProjects.includes(String(item.id))} onChange={event=>setScopeProjects(items=>toggle(items,String(item.id),event.currentTarget.checked))}/>{String(item.name??item.id)}</label>)}
                    {!scope.projects.length&&<p>No index sets on this page. Create one in AIRK Admin.</p>}
                    {scope.projectContinuation&&<button type="button" disabled={disabled} onClick={()=>void run(()=>request('indexing.scope.read',{kind:'projects',next:true}),true)}>More index sets</button>}
                    <AdminButton page="index-library" label="Create or change index sets in AIRK Admin" ghost disabled={!props.state.trusted} choose={false}/>
                </fieldset>
                <p>{scopeIndexes.length} {scopeIndexes.length===1?'index':'indexes'} and {scopeProjects.length} {scopeProjects.length===1?'index set':'index sets'} selected. The search scope is everything in the checked index sets plus every checked index; each index is read through the index sets that hold it. Uncheck everything to stop searching indexes in this window.</p>
                {apply.newSets.map(id=><p key={id} class="callout" role="status">{indexName(id)} is in no index set. Apply creates an index set named “{indexName(id)}” for it, then applies your selection.</p>)}
                {apply.limit&&<p class="callout" role="alert">{apply.limit}</p>}
                <p role="status">{scopeDirty?'Changes not applied.':scope.automatic?'This folder\'s index is read automatically. Apply to make it this window\'s chosen scope.':scope.active?'Selection active in this window.':'Apply your selection to use it in this window.'}</p>
                <button disabled={disabled||!apply.enabled}>Apply selection</button>
                {scope.active&&<button type="button" class="ghost" disabled={disabled} onClick={()=>void run(async()=>{await request('indexing.scope.apply',CLEAR_SCOPE);await request('indexing.scope.read',{});},true)}>Clear search scope</button>}
                <div class="actions indexing-save-set">
                    <label>New index set name <input type="text" value={setName} maxLength={128} disabled={disabled} onInput={event=>setSetName(event.currentTarget.value)}/></label>
                    <button type="button" disabled={disabled||!setName.trim()||!scopeIndexes.length&&!scopeProjects.length} onClick={()=>{const name=setName.trim();if(!name){setError('Enter a name for the new index set.');return;}void run(async()=>{await request('indexing.scope.saveSet',{name,selection:{index_ids:scopeIndexes,project_ids:scopeProjects}});setSetName('');await request('indexing.scope.read',{});},true);}}>Save as index set</button>
                </div>
                <p class="muted">Save as index set creates a new index set holding the checked indexes and the indexes of the checked sets.</p>
                <p class="muted">Coding agents read the scope they were connected with. <button type="button" class="ghost" onClick={()=>props.go({workspace:'manage',tab:'agents'})}>Connect an agent</button></p>
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
                {Array.isArray(view.recommendations.cards)&&view.recommendations.cards.map((item,at)=>{const card=catalogObject(item);return <article key={at}><h3>{actionLabels[String(card.action)]??String(card.action)}</h3><p>{String(card.reason)}</p><p>{String(card.expected_benefit)}</p><p>{String(card.tradeoff)}</p>{card.action==='review_split'&&Array.isArray(adviceSelection?.index_ids)&&adviceSelection.index_ids.filter(id=>/^idx_[0-9a-f]{32}$/.test(String(id))).map(id=><AdminButton key={String(id)} page="index-library" index={String(id)} label={`Review a split of ${String(id)} in AIRK Admin`} disabled={disabled} choose={false}/>)}</article>;})}
                <button disabled={disabled} onClick={()=>void run(()=>request('indexing.recommendations.use',{}))}>Use this query scope</button>
                <button disabled={disabled} onClick={()=>void run(()=>request('indexing.recommendations.copy',{}))}>Copy agent scope instructions</button>
                <p>Use this query scope makes it this window's search scope and prefills a Workbench search form. Review it before running. It does not change an agent connection or an index set.</p>
                <p>Reusable choices are saved as index sets, and splits are reviewed, in AIRK Admin.</p>
                <button disabled={disabled} onClick={()=>props.go({workspace:'indexing',tab:'projects'})}>View index sets</button>
                <details><summary>Complete Recommendations response</summary><pre>{JSON.stringify(view.recommendations,null,2)}</pre></details>
            </>:<p>Recommendations unavailable on this connection. Review the complete native response: <code>{JSON.stringify(view.recommendations)}</code></p>)}
            {view.recommendations&&<button disabled={disabled} onClick={()=>void run(()=>request('indexing.recommendations.dismiss',{}))}>Keep this layout / Dismiss</button>}
        </details>
        </>:<LibraryView key={tab} tab={tab==='projects'?'projects':'library'} state={props.state}/>}
    </section>;
}
