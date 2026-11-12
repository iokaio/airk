// SPDX-License-Identifier: Apache-2.0
import { Notice, noticeKey } from './notices';
import { useEffect, useRef, useState } from 'preact/hooks';
import { catalogObject } from '../indexingCatalog';
import type { ScreenProps } from './workspaces';
import { request } from './api';
import { AdminButton } from './adminLinks';
import { LibraryView } from './libraryView';

/** The window search scope is not served by the AIRK Service yet (it moves to the session's W4 scope). */
export const SCOPE_UNAVAILABLE='The window search scope is not available yet. Search uses the indexes your coding agents and Workbench already select.';

/**
 * Indexing (U97): the window search scope and recommendations, and a read-only
 * index library. Native metadata is rendered as escaped Preact text; every
 * change to indexes, sources and index sets is made in AIRK Admin.
 */
export function IndexingScreen(props: ScreenProps) {
    const view=props.state.indexing??{generation:0,busy:false};
    const automaticRead=useRef('');
    const workingNow=useRef(false),[working,setWorking]=useState(false);
    const [error,setError]=useState(''),[errorDetail,setErrorDetail]=useState('');
    const [scopeIndexes,setScopeIndexes]=useState<string[]>([]),[scopeProjects,setScopeProjects]=useState<string[]>([]);
    const actionLabels:Record<string,string>={keep_layout:'Keep this layout',review_reuse:'Review reuse',use_query_scope:'Choose a query scope',review_split:'Review an independent split',split_pending:'Split unavailable on this connection',removal_pending:'Review permanent exclusions in AIRK Admin',manual_selection:'Choose manually while observations are unknown'};
    const [situation,setSituation]=useState('keep');
    const scope=view.searchScope;
    const scopeSelection=JSON.stringify([scope?.selectedIndexIds,scope?.selectedProjectIds]);
    useEffect(()=>{setScopeIndexes(scope?.selectedIndexIds??[]);setScopeProjects(scope?.selectedProjectIds??[]);},[scopeSelection]);
    const scopeDirty=JSON.stringify([scopeIndexes.slice().sort(),scopeProjects.slice().sort()])!==JSON.stringify([(scope?.selectedIndexIds??[]).slice().sort(),(scope?.selectedProjectIds??[]).slice().sort()]);
    const toggle=(items:string[],id:string,checked:boolean)=>checked?[...items.filter(item=>item!==id),id]:items.filter(item=>item!==id);
    // Keep the complete action chain exclusive, including the fresh read after Apply.
    // Search scope failures say the scope is not available yet; the native text stays in Details.
    const run=async(work:()=>Promise<unknown>,scopeWork=false)=>{if(workingNow.current)return;workingNow.current=true;setWorking(true);setError('');setErrorDetail('');try{await work();}catch(failure){const text=failure instanceof Error?failure.message:String(failure);if(scopeWork){setError(SCOPE_UNAVAILABLE);setErrorDetail(text);}else setError(text);}finally{workingNow.current=false;setWorking(false);}};
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
    const shownError=error||(tab==='window'&&view.error?SCOPE_UNAVAILABLE:'');
    const shownDetail=errorDetail||(tab==='window'&&!error?view.error??'':'');
    return <section class="indexing-screen">
        <div class="section-head"><p class="indexing-intro">{tab==='window'?'Choose the indexes and index sets to search in this VS Code window.':tab==='projects'?'Index sets on this computer. Create and change them in AIRK Admin.':'Indexes on this computer. Create, change and delete them in AIRK Admin.'}</p>
            {tab==='window'&&<AdminButton page="index-library" disabled={!props.state.trusted}/>}</div>
        {tab==='window'&&shownError&&<Notice noticeKey={noticeKey("indexing-error", shownError, shownDetail)} class="notice" role="alert"><p>{shownError}</p>{shownDetail&&<details><summary>Details</summary><code>{shownDetail}</code></details>}</Notice>}
        {!props.state.trusted&&<Notice global noticeKey="workspace-restricted" class="notice"><p>Trust this workspace to read indexes.</p></Notice>}
        {props.state.nativeExecutionEnabled===false&&<Notice global noticeKey="native-execution-disabled" class="notice"><p>Enable native execution in AIRK settings to read indexes.</p></Notice>}
        {tab==='window'?<>{view.busy&&<p role="status">Loading search choices…</p>}
        {connectionPending&&props.state.trusted&&<p role="status">{props.state.connection==='connecting'?'Connecting to the AIRK Service before loading search choices…':'The AIRK Service is unavailable. Search choices load when it is ready.'}</p>}
        <article class="card"><h2>Search scope</h2>
            <p>Check the indexes and index sets you want to search, then apply your selection. Shared indexes are searched once.</p>
            <button disabled={disabled} onClick={()=>void run(()=>request('indexing.scope.read',{}),true)}>Refresh choices</button>
            {!scope&&!view.busy&&<p>Search choices have not loaded. Use Refresh choices to retry.</p>}
            {scope&&<form onSubmit={event=>{event.preventDefault();void run(async()=>{await request('indexing.scope.apply',{generation:view.generation,selection:{index_ids:scopeIndexes,project_ids:scopeProjects}});await request('indexing.scope.read',{});},true);}}>
                <fieldset disabled={disabled} class="indexing-scope-list"><legend>Indexes</legend>
                    {scope.indexes.filter(item=>item.tombstone!==true).map(item=><label key={String(item.id)}><input type="checkbox" checked={scopeIndexes.includes(String(item.id))} onChange={event=>setScopeIndexes(items=>toggle(items,String(item.id),event.currentTarget.checked))}/><span><strong>{String(item.name??item.id)}</strong>{typeof catalogObject(item.backing??{}).root==='string'&&<small>{String(catalogObject(item.backing).root)}</small>}</span></label>)}
                    {!scope.indexes.some(item=>item.tombstone!==true)&&<p>No indexes on this page. Create one in AIRK Admin.</p>}
                    {scope.indexContinuation&&<button type="button" disabled={disabled} onClick={()=>void run(()=>request('indexing.scope.read',{kind:'indexes',next:true}),true)}>More indexes</button>}
                </fieldset>
                <fieldset disabled={disabled} class="indexing-scope-list"><legend>Index sets</legend>
                    {scope.projects.filter(item=>!scope.managedProjectIds?.includes(String(item.id))).map(item=><label key={String(item.id)}><input type="checkbox" checked={scopeProjects.includes(String(item.id))} onChange={event=>setScopeProjects(items=>toggle(items,String(item.id),event.currentTarget.checked))}/>{String(item.name??item.id)}</label>)}
                    {!scope.projects.some(item=>!scope.managedProjectIds?.includes(String(item.id)))&&<p>No index sets on this page. Create one in AIRK Admin.</p>}
                    {scope.projectContinuation&&<button type="button" disabled={disabled} onClick={()=>void run(()=>request('indexing.scope.read',{kind:'projects',next:true}),true)}>More index sets</button>}
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
                {Array.isArray(view.recommendations.cards)&&view.recommendations.cards.map((item,at)=>{const card=catalogObject(item);return <article key={at}><h3>{actionLabels[String(card.action)]??String(card.action)}</h3><p>{String(card.reason)}</p><p>{String(card.expected_benefit)}</p><p>{String(card.tradeoff)}</p>{card.action==='review_split'&&Array.isArray(adviceSelection?.index_ids)&&adviceSelection.index_ids.filter(id=>/^idx_[0-9a-f]{32}$/.test(String(id))).map(id=><AdminButton key={String(id)} page="index-library" index={String(id)} label={`Review a split of ${String(id)} in AIRK Admin`} disabled={disabled} choose={false}/>)}</article>;})}
                <button disabled={disabled} onClick={()=>void run(()=>request('indexing.recommendations.use',{}))}>Use this query scope</button>
                <button disabled={disabled} onClick={()=>void run(()=>request('indexing.recommendations.copy',{}))}>Copy agent scope instructions</button>
                <p>Use this query scope prefills a temporary Workbench form. Review it before running. It does not change an agent connection or replace saved session choices automatically.</p>
                <p>Reusable choices are saved as index sets, and splits are reviewed, in AIRK Admin.</p>
                <button disabled={disabled} onClick={()=>props.go({workspace:'indexing',tab:'projects'})}>View index sets</button>
                <details><summary>Complete Recommendations response</summary><pre>{JSON.stringify(view.recommendations,null,2)}</pre></details>
            </>:<p>Recommendations unavailable on this connection. Review the complete native response: <code>{JSON.stringify(view.recommendations)}</code></p>)}
            <button disabled={disabled} onClick={()=>void run(()=>request('indexing.recommendations.dismiss',{}))}>Keep this layout / Dismiss</button>
        </details>
        </>:<LibraryView key={tab} tab={tab==='projects'?'projects':'library'} state={props.state}/>}
    </section>;
}
