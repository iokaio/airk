// SPDX-License-Identifier: Apache-2.0
import { useEffect, useState } from 'preact/hooks';
import { CatalogObject, CatalogScopeAtom, CatalogView, catalogExport, catalogItems, catalogMembershipDraft, catalogObject, catalogReviewPreview, catalogWindowAssociation, sourceChangeObservation, validateCatalogScope } from '../indexingCatalog';
import type { ScreenProps } from './workspaces';
import { request } from './api';
import { IndexSplitPanel } from './indexingSplit';
import { canReviewPolicyUnblock, policyStatusLabels, validatePolicyImpact } from '../indexingCatalog';

/** Native metadata is rendered as escaped Preact text; complete replies stay available in details/export. */
export function IndexingScreen(props: ScreenProps) {
    const view=props.state.indexing??{generation:0,busy:false};
    const [error,setError]=useState(''),[name,setName]=useState(''),[filter,setFilter]=useState('');
    const [index,setIndex]=useState(''),[memberRevision,setMemberRevision]=useState('');
    const [scopeAtoms,setScopeAtoms]=useState<CatalogScopeAtom[]>([{source_root_id:'',kind:'subtree',path:''}]);
    const [defaults,setDefaults]=useState('');
    const [windowProjects,setWindowProjects]=useState(''),[windowIndexes,setWindowIndexes]=useState('');
    const [windowReadBusy,setWindowReadBusy]=useState(false);
    const [grantProjects,setGrantProjects]=useState(''),[grantIndexes,setGrantIndexes]=useState('');
    const [setupName,setSetupName]=useState(''),[setupPrimary,setSetupPrimary]=useState(''),[setupData,setSetupData]=useState(''),[setupCollection,setSetupCollection]=useState(''),[setupWatch,setSetupWatch]=useState(false);
    const [setupRoots,setSetupRoots]=useState<{alias:string;path:string}[]>([]);
    const [useIndex,setUseIndex]=useState(''),[useProjects,setUseProjects]=useState('');
    const actionLabels:Record<string,string>={keep_layout:'Keep this layout',review_reuse:'Review reuse',use_query_scope:'Choose a query scope',review_split:'Review an independent split',split_pending:'Split unavailable on this connection',removal_pending:'Review permanent exclusions in Sources',manual_selection:'Choose manually while observations are unknown'};
    const [adviceProjects,setAdviceProjects]=useState(''),[adviceIndexes,setAdviceIndexes]=useState(''),[situation,setSituation]=useState('keep');
    const [sourceAlias,setSourceAlias]=useState(''),[sourcePath,setSourcePath]=useState(''),[removeAlias,setRemoveAlias]=useState(''),[beginOperation,setBeginOperation]=useState('');
    const [policyRoot,setPolicyRoot]=useState(''),[policyKind,setPolicyKind]=useState<'exact'|'subtree'>('exact'),[policyPath,setPolicyPath]=useState(''),[unblockRule,setUnblockRule]=useState('');
    const savedContext=catalogWindowAssociation(view);
    const contextToken=view.hostContext?JSON.stringify([catalogObject(view.hostContext.descriptor).namespace,catalogObject(view.hostContext.descriptor).catalog_revision,savedContext?.id,savedContext?.revision]):view.context?JSON.stringify([catalogObject(view.context.detail).namespace,catalogObject(view.context.detail).revision,savedContext?.id,savedContext?.revision]):'';
    useEffect(()=>{setWindowProjects(Array.isArray(savedContext?.project_ids)?savedContext.project_ids.join(', '):'');setWindowIndexes(Array.isArray(savedContext?.index_ids)?savedContext.index_ids.join(', '):'');},[contextToken]);
    const run=async(work:()=>Promise<unknown>)=>{setError('');try{await work();}catch(failure){setError(failure instanceof Error?failure.message:String(failure));}};
    const preview=(operation:CatalogObject)=>run(()=>request('indexing.preview',{operation,generation:view.generation}));
    const disabled=view.busy||!props.state.trusted||props.state.nativeExecutionEnabled===false;
    const rows=catalogItems(view.page), detail=catalogItems(view.detail,'detail')[0];
    const project=detail?.kind==='project'?detail.value:undefined, selectedIndex=detail?.kind==='index'?detail.value:undefined;
    useEffect(()=>{setPolicyRoot('');setPolicyPath('');setUnblockRule('');},[selectedIndex?.id]);
    const policySnapshot=view.policyStatus?.snapshot?catalogObject(view.policyStatus.snapshot):undefined;
    const policy=policySnapshot?catalogObject(policySnapshot.policy):undefined;
    const policyApplication=policySnapshot?.application?catalogObject(policySnapshot.application):undefined;
    const policyDenies=Array.isArray(policy?.denies)?policy.denies.map(catalogObject):[];
    const policyImpact=view.review?.request.operation.operation==='change_index_policy'?validatePolicyImpact(catalogReviewPreview(view.review).source_policy_impact):undefined;
    const adviceSelection=view.recommendations?.schema==='airk.recommendations/1'?catalogObject(view.recommendations.selected):undefined;
    const canSaveAdviceDefault=project&&Array.isArray(adviceSelection?.project_ids)&&adviceSelection.project_ids.length===1&&adviceSelection.project_ids[0]===project.id;
    const grantRow=catalogItems(view.grantDetail,'detail')[0],selectedGrant=grantRow?catalogObject(grantRow.value.grant):undefined;
    const sourceStatus=view.sourceObservation?sourceChangeObservation(view.sourceObservation):undefined,sourceJob=sourceStatus?.job?catalogObject(sourceStatus.job):undefined;
    const sourceBegin=sourceStatus?String(catalogObject(catalogObject(sourceStatus.begin).metadata).operation_id):undefined;
    useEffect(()=>{if(sourceBegin)setBeginOperation(sourceBegin);},[sourceBegin]);
    useEffect(()=>{if(view.submittedReview?.request.operation.operation==='begin_index_source_change'&&view.outcome?.status==='catalog_applied'){const receipt=catalogObject(view.outcome.receipt);if(typeof receipt.operation_id==='string')setBeginOperation(receipt.operation_id);}},[view.outcome]);
    const pageKind=rows[0]?.kind;
    const expectedKind=props.route.tab==='projects'?'project':'index';
    const visibleRows=pageKind===expectedKind||rows.length===0?rows:[];
    const next=(response?:CatalogObject)=>!!response&&!!catalogObject(response.page).continuation;
    const page=(kind:'projects'|'indexes'|'sources'|'memberships'|'grants',more=false,idx?:string,prj?:string)=>run(()=>request('indexing.page',{kind,next:more,...(idx?{index:idx}:{}),...(prj?{project:prj}:{})}));
    const editAtom=(at:number,change:Partial<CatalogScopeAtom>)=>setScopeAtoms(rows=>rows.map((row,i)=>i===at?{...row,...change}:row));
    const editMembership=(member:CatalogObject)=>{const draft=catalogMembershipDraft(member);setIndex(draft.index);setMemberRevision(draft.revision);setScopeAtoms(draft.scope);};
    return <section>
        <article class="card" id="recommendations"><h2>Recommendations</h2>
            <p>Choose what to reuse and query using authorized saved metadata. Readiness and costs without dated observations remain unknown. This read starts no source scan, evidence query or model.</p>
            <form onSubmit={event=>{event.preventDefault();void run(()=>request('indexing.recommendations',{arguments:{situation,...(adviceProjects.trim()||adviceIndexes.trim()?{selection:{project_ids:adviceProjects.split(',').map(v=>v.trim()).filter(Boolean),index_ids:adviceIndexes.split(',').map(v=>v.trim()).filter(Boolean)}}:{})}}));}}>
                <label>Situation<select value={situation} onChange={event=>setSituation(event.currentTarget.value)}>
                    <option value="keep">Keep this layout</option><option value="reuse">Reuse existing evidence</option><option value="focused_task">One application or package</option><option value="shared_dependencies">Shared code or specifications</option><option value="different_lifecycles">Different lifecycle or policy needs</option><option value="overlap">Possible overlapping indexes</option><option value="irrelevant_path">Path irrelevant to this task</option><option value="never_index_path">Path should never be indexed</option>
                </select></label>
                <label>Query Project IDs (comma separated)<input value={adviceProjects} onInput={event=>setAdviceProjects(event.currentTarget.value)} /></label>
                <label>Query index IDs (comma separated)<input value={adviceIndexes} onInput={event=>setAdviceIndexes(event.currentTarget.value)} /></label>
                <p>Leave both blank to inspect the active connection. Otherwise provide both explicit subsets. Selecting all or widening is a deliberate choice within the active grant.</p>
                <button disabled={disabled}>Read recommendations</button>
            </form>
            {view.recommendations&&(view.recommendations.schema==='airk.recommendations/1'?<>
                <p>Available to this connection: {Array.isArray(view.recommendations.available)?view.recommendations.available.length:0} unique indexes. Selected for this query: {Array.isArray(catalogObject(view.recommendations.selected).index_ids)?(catalogObject(view.recommendations.selected).index_ids as unknown[]).length:0}.</p>
                <p>Omitted indexes: {JSON.stringify(view.recommendations.omitted_index_ids)}. Observation: {String(catalogObject(view.recommendations.observation).source)} at {new Date(Number(view.recommendations.observed_unix_ms)).toLocaleString()}. Connection scope only; machine library coverage is incomplete.</p>
                <details><summary>Available indexes, Project names, selected source and prefix boundaries</summary><pre>{JSON.stringify({available:view.recommendations.available,projects:view.recommendations.projects,source_boundaries:view.recommendations.source_boundaries,selected_scope:view.recommendations.selected_scope},null,2)}</pre></details>
                {Array.isArray(view.recommendations.cards)&&view.recommendations.cards.map((item,at)=>{const card=catalogObject(item);return <article key={at}><h3>{actionLabels[String(card.action)]??String(card.action)}</h3><p>{String(card.reason)}</p><p>{String(card.expected_benefit)}</p><p>{String(card.tradeoff)}</p>{card.action==='review_split'&&view.canReviewSplit&&Array.isArray(adviceSelection?.index_ids)&&adviceSelection.index_ids.map(id=><button key={String(id)} disabled={disabled} onClick={()=>void run(()=>request('indexing.split.open',{index:String(id),generation:view.generation}))}>Open split review for {String(id)}</button>)}</article>;})}
                <button disabled={disabled} onClick={()=>void run(()=>request('indexing.recommendations.use',{}))}>Use this query scope</button>
                <button disabled={disabled} onClick={()=>void run(()=>request('indexing.recommendations.copy',{}))}>Copy agent scope instructions</button>
                <p>Use this query scope prefills a temporary Workbench form. Review it before running. It does not change an agent connection or replace saved session choices automatically.</p>
                {canSaveAdviceDefault&&<button disabled={disabled} onClick={()=>void preview({operation:'set_project_defaults',project_id:project!.id,expected_defaults_revision:project!.defaults_revision,index_ids:adviceSelection!.index_ids})}>Review saving as Project default</button>}
                <p>To save a single-Project scope, inspect that Project and review its native defaults change. For a combined scope, use each Project's defaults form below; a query index may belong to only one selected Project. Reuse opens existing membership review. Split recommendations remain unavailable until P10. Recommendations do not prefill permanent-removal jobs; open index Sources and prepare an explicit native review below.</p>
                <button disabled={disabled} onClick={()=>props.go({workspace:'indexing',tab:'projects'})}>Review reuse or Project defaults</button>
                <details><summary>Complete Recommendations response</summary><pre>{JSON.stringify(view.recommendations,null,2)}</pre></details>
            </>:<p>Recommendations unavailable on this connection. Review the complete native response: <code>{JSON.stringify(view.recommendations)}</code></p>)}
            <button disabled={disabled} onClick={()=>void run(()=>request('indexing.recommendations.dismiss',{}))}>Keep this layout / Dismiss</button>
        </article>
        {props.route.tab==='window'?<article class="card"><h2>This window</h2>
            <p>Saved window associations are preferences. Saving or deleting an association does not grant access, switch an evidence session or change a coding agent's configured scope.</p>
            {view.windowDescriptor||view.canReadWindowContext?<>{view.windowDescriptor&&<p>{view.windowDescriptor.name} · {view.windowDescriptor.kind}</p>}
                <button disabled={disabled||windowReadBusy} onClick={()=>{setWindowReadBusy(true);void run(()=>request('indexing.context.read',{})).finally(()=>setWindowReadBusy(false));}}>{windowReadBusy?'Reading saved association…':'Read saved association'}</button>
                {(view.context||view.hostContext)&&<><p>{savedContext?`Saved association ${String(savedContext.id)}, revision ${String(savedContext.revision)}`:'No saved association for this window.'}</p>
                    <form onSubmit={event=>{event.preventDefault();void run(()=>request('indexing.context.preview',{generation:view.generation,selection:{
                        project_ids:windowProjects.split(',').map(value=>value.trim()).filter(Boolean),index_ids:windowIndexes.split(',').map(value=>value.trim()).filter(Boolean),
                    }}));}}>
                        <label>Project IDs (comma separated; blank selects zero Projects)<input value={windowProjects} onInput={event=>setWindowProjects(event.currentTarget.value)} /></label>
                        <label>Default index IDs (comma separated; blank selects no index defaults)<input value={windowIndexes} onInput={event=>setWindowIndexes(event.currentTarget.value)} /></label>
                        <p class="muted">Up to 32 unique Projects and 32 unique indexes. Default indexes must belong to the selected Projects; zero Projects requires zero index defaults. The native preview remains authoritative.</p>
                        <button disabled={disabled}>Preview saved association</button>
                    </form>
                    {savedContext&&<button disabled={disabled} onClick={()=>void run(()=>request('indexing.context.delete',{generation:view.generation}))}>Preview delete association</button>}
                </>}
            </>:<p>Window association controls are unavailable because the trusted host has not supplied this window's canonical identity. No association is inferred from open folders.</p>}
            {view.windowDescriptor?.kind==='empty'&&<p class="muted">This empty window uses a transient private identity. A new empty window or restarted window does not promise to restore this association.</p>}
            {view.canReviewGrant?<form onSubmit={event=>{event.preventDefault();void run(()=>request('indexing.grant.draft',{generation:view.generation,selection:{
                project_ids:grantProjects.split(',').map(value=>value.trim()).filter(Boolean),index_ids:grantIndexes.trim()?grantIndexes.split(',').map(value=>value.trim()).filter(Boolean):null,
            }}));}}><h3>Review a new access grant</h3><p>Choose Projects explicitly. The native service freezes the actual memberships, sources and policy for review. Preparing a draft issues no access and changes no evidence session.</p>
                <label>Project IDs (1–32, comma separated)<input required value={grantProjects} onInput={event=>setGrantProjects(event.currentTarget.value)} /></label>
                <label>Optional index subset (1–32 IDs; blank asks native to include the selected Projects' memberships)<input value={grantIndexes} onInput={event=>setGrantIndexes(event.currentTarget.value)} /></label>
                {!view.hostContext&&<p class="muted">Read the current window association before preparing a grant. Saved preferences do not select this grant automatically.</p>}
                <button disabled={disabled||!view.hostContext}>Prepare native grant review</button>
            </form>:<p>Native access-grant review is unavailable on this connection.</p>}
            <h3>Saved access grants</h3><p>Inspect a native grant before selecting an index and Projects. Listing, inspecting or revoking a grant does not select another grant.</p>
            <button disabled={disabled} onClick={()=>void page('grants')}>Read grants</button>
            {catalogItems(view.grants).map(row=>{const grant=catalogObject(row.value.grant),provenance=catalogObject(row.value.provenance);return <article key={String(grant.id)}><p><code>{String(grant.id)}</code> · revision {String(grant.revision)} · {grant.revoked?'Revoked':'Unrevoked'} · {String(provenance.kind)}</p><button disabled={disabled} onClick={()=>void run(()=>request('indexing.detail',{kind:'grant',id:String(grant.id)}))}>Inspect grant</button></article>;})}
            {next(view.grants)&&<button disabled={disabled} onClick={()=>void page('grants',true)}>Next grants page</button>}
            {selectedGrant&&grantRow&&<article class="card"><h3>Grant {String(selectedGrant.id)}</h3><pre>{JSON.stringify(grantRow.value,null,2)}</pre>
                <button disabled={disabled||selectedGrant.revoked===true} onClick={()=>void run(()=>request('indexing.grant.revoke',{generation:view.generation}))}>Preview revoke grant</button>
                {view.canUseGrant&&catalogObject(grantRow.value.provenance).kind==='user_reviewed_project_grant'&&selectedGrant.revoked===false?<form onSubmit={event=>{event.preventDefault();void run(()=>request('indexing.grant.use',{generation:view.generation,choice:{grant_id:selectedGrant.id,expected_grant_revision:selectedGrant.revision,...(useIndex.trim()?{index_ids:useIndex.split(',').map(value=>value.trim()).filter(Boolean)}:{}),project_ids:useProjects.split(',').map(value=>value.trim()).filter(Boolean)}}));}}>
                    <label>Optional index subset from this grant (1–32 IDs; blank includes all permitted members)<input value={useIndex} onInput={event=>setUseIndex(event.currentTarget.value)} /></label>
                    <label>Project IDs from this grant (1–32, comma separated)<input required value={useProjects} onInput={event=>setUseProjects(event.currentTarget.value)} /></label>
                    <p>Use Grant explicitly selects these Projects and the optional index subset for this window's evidence session and automatic MCP definition. Shared indexes run once. Saved manual agent configurations keep their own scope. Read the current window association first; native metadata must share one catalog revision.</p>
                    <button disabled={disabled||!view.windowDescriptor}>Use Grant for this window</button>
                </form>:<p>This grant cannot be selected here. Imported compatibility grants remain attached to their existing legacy connections.</p>}
            </article>}
            <p>Persisting a reviewed grant does not activate a connection or change existing agent configuration. Existing single-index connections continue to use their configured source scope.</p>
            <button onClick={()=>props.go({workspace:'indexing',tab:'library'})}>Open Index library</button>
        </article>:<>
        <article class="card"><h2>{props.route.tab==='projects'?'Projects':'Index library'}</h2>
            <p>Metadata on this AIRK host. A library entry or Project membership does not grant access to source evidence.</p>
            <button disabled={disabled} onClick={()=>void page(props.route.tab==='projects'?'projects':'indexes')}>Refresh {props.route.tab==='projects'?'Projects':'indexes'}</button>
            <label>Filter displayed page<input value={filter} onInput={event=>setFilter(event.currentTarget.value)} /></label>
            <p class="muted">Each native page contains up to 16 entries within 48 KiB. The filter applies only to this displayed page.</p>
            {visibleRows.filter(row=>JSON.stringify(row.value).toLocaleLowerCase().includes(filter.toLocaleLowerCase())).map(row=><article class="card" key={String(row.value.id)}><strong>{String(row.value.name??row.value.id)}</strong><p><code>{String(row.value.id)}</code></p>
                {row.kind==='index'&&<p>{String(catalogObject(row.value.backing).root??'Source location is in complete details')}</p>}
                <button disabled={disabled} onClick={()=>void run(()=>request('indexing.detail',{kind:row.kind==='project'?'project':'index',id:String(row.value.id)}))}>Inspect</button></article>)}
            {view.page&&pageKind===expectedKind&&next(view.page)&&<button disabled={disabled} onClick={()=>void page(props.route.tab==='projects'?'projects':'indexes',true)}>Next page</button>}
            {view.page&&rows.length===0&&<p>No entries on this native page.</p>}
            {!view.page&&<p>Refresh to read the native catalog. An inactive or unsupported catalog reports its native failure; it is not an empty library.</p>}
            {props.route.tab==='projects'&&<form onSubmit={event=>{event.preventDefault();void run(()=>request('indexing.create',{name,generation:view.generation}));}}><label>New Project name<input required maxLength={256} value={name} onInput={event=>setName(event.currentTarget.value)} /></label><button disabled={disabled||!view.revision}>Preview create Project</button></form>}
            {props.route.tab==='library'&&(view.canReviewSetup?<form onSubmit={event=>{event.preventDefault();void run(()=>request('indexing.setup.draft',{generation:view.generation,selection:{name:setupName,primary_root:setupPrimary,data_root:setupData,collection:setupCollection,additional_roots:setupRoots,setup_watch:setupWatch}}));}}>
                <h3>Set up a new index</h3><p>Choose source and index-storage locations explicitly. Native review resolves their actual locations and storage ownership. Preparing a review creates no index and grants no access.</p>
                <label>Index name<input required value={setupName} onInput={event=>setSetupName(event.currentTarget.value)} /></label>
                <label>Primary source location<input required value={setupPrimary} onInput={event=>setSetupPrimary(event.currentTarget.value)} /></label>
                <label>Index storage location<input required value={setupData} onInput={event=>setSetupData(event.currentTarget.value)} /></label>
                <label>Collection<input required value={setupCollection} onInput={event=>setSetupCollection(event.currentTarget.value)} /></label>
                {setupRoots.map((root,at)=><fieldset key={at} disabled={disabled}><legend>Additional source {at+1}</legend>
                    <label>Alias<input required value={root.alias} onInput={event=>{const alias=event.currentTarget.value;setSetupRoots(rows=>rows.map((row,i)=>i===at?{...row,alias}:row));}} /></label>
                    <label>Source location<input required value={root.path} onInput={event=>{const path=event.currentTarget.value;setSetupRoots(rows=>rows.map((row,i)=>i===at?{...row,path}:row));}} /></label>
                    <button type="button" onClick={()=>setSetupRoots(rows=>rows.filter((_,i)=>i!==at))}>Remove additional source {at+1}</button>
                </fieldset>)}
                <button type="button" disabled={disabled||setupRoots.length>=16} onClick={()=>setSetupRoots(rows=>[...rows,{alias:'',path:''}])}>Add source location</button>
                <label><input type="checkbox" checked={setupWatch} onChange={event=>setSetupWatch(event.currentTarget.checked)} />Enable native watching after setup</label>
                {!view.revision&&<p>Refresh the index library before preparing setup.</p>}
                <button disabled={disabled||!view.revision}>Prepare native setup review</button>
            </form>:<p>Native index setup is unavailable on this connection.</p>)}
        </article>
        {detail&&<article class="card"><h2>{detail.kind==='project'?'Project':'Index'}: {String(detail.value.name)}</h2><p><code>{String(detail.value.id)}</code></p>
            <form onSubmit={event=>{event.preventDefault();void preview({operation:detail.kind==='project'?'rename_project':'rename_index',id:detail.value.id,expected_definition_revision:detail.value.definition_revision,name});}}><label>New name<input required maxLength={256} value={name} onInput={event=>setName(event.currentTarget.value)} /></label><button disabled={disabled}>Preview rename</button></form>
            {selectedIndex&&<button disabled={disabled} onClick={()=>void page('sources',false,String(selectedIndex.id))}>Read Sources</button>}
            {selectedIndex&&(view.canReviewSource?<article><h3>Manage index sources</h3><p>Native review freezes actual source identities, configuration and policy. Preparing a draft makes no changes. Applying a root change begins a native configuration job; a begin receipt does not mean the root update has completed.</p>
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
                <p>Retirement waits for native evidence and background work to stop, including embedding providers. Review the complete native receipt; source files are not deleted.</p>
                <button disabled={disabled||selectedIndex.tombstone===true} onClick={()=>void run(()=>request('indexing.source.draft',{generation:view.generation,change:{operation:'retire_index'}}))}>Prepare native index-retirement review</button>
                {view.canReadPolicy&&<article><h3>Permanent path exclusions</h3>
                    <p>Block an exact file or a directory and its future descendants in this index. Every Project using this index is affected. Other indexes covering the same source remain separate. Source files are preserved.</p>
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
            </article>:<p>Native source mutation and retirement review are unavailable on this connection.</p>)}
            <button disabled={disabled} onClick={()=>void page('memberships',false,selectedIndex?String(selectedIndex.id):undefined,project?String(project.id):undefined)}>Read memberships</button>
            {project&&<>
                <p>Default indexes: {Array.isArray(project.default_indexes)?project.default_indexes.join(', '):'Unknown'}</p>
                <form onSubmit={event=>{event.preventDefault();void preview({operation:'set_project_defaults',project_id:project.id,expected_defaults_revision:project.defaults_revision,index_ids:defaults.split(',').map(value=>value.trim()).filter(Boolean)});}}><label>Default index IDs (comma separated; blank clears defaults)<input value={defaults} onInput={event=>setDefaults(event.currentTarget.value)} /></label><button disabled={disabled}>Preview defaults</button></form>
                <form onSubmit={event=>{event.preventDefault();void run(async()=>{const prior=memberRevision===''?null:Number(memberRevision);const atoms=validateCatalogScope(scopeAtoms);await request('indexing.preview',{generation:view.generation,operation:{operation:'set_membership',expected_scope_revision:project.scope_revision,expected_membership_revision:prior,membership:{project_id:project.id,index_id:index,revision:prior===null?1:prior+1,scope:atoms}}});});}}>
                    <h3>Assign an existing index</h3><p>Inspect the index library and its Sources first. Review the exact source scope before applying. Empty scopes never mean unrestricted access.</p>
                    <label>Index ID<input required value={index} onInput={event=>setIndex(event.currentTarget.value)} placeholder="idx_…" /></label>
                    <button type="button" disabled={disabled||!index} onClick={()=>void page('sources',false,index)}>Read Sources for this index</button>
                    <datalist id="membership-source-options">{catalogItems(view.sources).filter(row=>row.value.index_id===index).map(row=><option key={String(row.value.id)} value={String(row.value.id)}>{String(row.value.canonical_path)}</option>)}</datalist>
                    <p>{scopeAtoms.length} of 128 source scopes. The complete native request must fit 48 KiB. Removing every row means no source authority.</p>
                    {scopeAtoms.map((atom,at)=><fieldset key={at} disabled={disabled}><legend>Source scope {at+1}</legend>
                        <label>Source ID<input required list="membership-source-options" value={atom.source_root_id} onInput={event=>editAtom(at,{source_root_id:event.currentTarget.value})} placeholder="src_…" /></label>
                        <label>Canonical relative path<input value={atom.path} onInput={event=>editAtom(at,{path:event.currentTarget.value})} /></label>
                        <label>Scope<select value={atom.kind} onChange={event=>editAtom(at,{kind:event.currentTarget.value==='exact'?'exact':'subtree'})}><option value="subtree">Subtree (blank path means this source root)</option><option value="exact">Exact path</option></select></label>
                        <button type="button" onClick={()=>setScopeAtoms(rows=>rows.filter((_,i)=>i!==at))}>Remove scope {at+1}</button>
                    </fieldset>)}
                    <button type="button" disabled={disabled||scopeAtoms.length>=128} onClick={()=>setScopeAtoms(rows=>[...rows,{source_root_id:'',kind:'subtree',path:''}])}>Add source scope</button>
                    <label>Current membership revision (blank for new membership)<input inputMode="numeric" value={memberRevision} onInput={event=>setMemberRevision(event.currentTarget.value)} /></label>
                    <button disabled={disabled}>Preview membership</button>
                </form>
                <button disabled={disabled} onClick={()=>void preview({operation:'delete_project',id:project.id,expected_definition_revision:project.definition_revision,expected_scope_revision:project.scope_revision,expected_defaults_revision:project.defaults_revision})}>Preview delete Project</button>
                <p class="muted">Deleting a Project removes its metadata and memberships. The native preview describes affected defaults and grants. Source files remain read-only.</p>
            </>}
        </article>}
        {view.sources&&detail&&<article class="card"><h3>Sources</h3>{catalogItems(view.sources).map(row=><p key={String(row.value.id)}><code>{String(row.value.id)}</code> {String(row.value.canonical_path)} · {row.value.enabled?'Enabled':'Disabled'}</p>)}{next(view.sources)&&<button disabled={disabled} onClick={()=>void page('sources',true,selectedIndex?String(selectedIndex.id):index)}>Next Sources page</button>}</article>}
        {view.memberships&&detail&&<article class="card"><h3>Memberships</h3>{catalogItems(view.memberships).map(row=><article key={`${row.value.project_id}/${row.value.index_id}`}><p><code>{String(row.value.project_id)} / {String(row.value.index_id)}</code> · revision {String(row.value.revision)}</p><pre>{JSON.stringify(row.value.scope,null,2)}</pre>
            {project&&<><button disabled={disabled} onClick={()=>editMembership(row.value)}>Edit all membership scopes</button><button disabled={disabled} onClick={()=>void preview({operation:'remove_membership',project_id:project.id,index_id:row.value.index_id,expected_scope_revision:project.scope_revision,expected_membership_revision:row.value.revision})}>Preview remove membership</button></>}</article>)}{next(view.memberships)&&<button disabled={disabled} onClick={()=>void page('memberships',true,selectedIndex?String(selectedIndex.id):undefined,project?String(project.id):undefined)}>Next membership page</button>}</article>}
        </>}
        {(error||view.error)&&<p class="notice" role="alert">{error||view.error}</p>}
        {view.busy&&<p role="status">Waiting for native catalog response…</p>}
        <IndexSplitPanel view={view} index={selectedIndex} disabled={disabled} run={run}/>
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
        {view.review&&<article class="card"><h2>Review native changes</h2><p>Review the complete native preview, including before/after state, file-data effects and authority fences.</p><pre>{JSON.stringify(view.review.response,null,2)}</pre>
            {policyImpact&&<article><h3>Permanent-exclusion impact</h3>
                <p>Reviewed source root: <code>{String(policyImpact.canonical_root)}</code>. Path: <code>{String(catalogObject(policyImpact.target).path)||'(entire source root)'}</code> · {catalogObject(policyImpact.target).kind==='exact'?'Exact file':'Directory and descendants'}.</p>
                <dl>{[['affected_artifacts','Artifacts'],['affected_vectors','Vectors'],['affected_facts','Facts'],['affected_relationships','Relationships']].map(([key,label])=><><dt>{label}</dt><dd>{policyImpact[key]===null?'Unknown':String(policyImpact[key])}</dd></>)}</dl>
                <p>{String(policyImpact.counts_basis)}</p>
                <p>Affected source roots: {(policyImpact.affected_source_root_ids as string[]).join(', ')||'None'}. Affected Projects: {(policyImpact.affected_project_ids as string[]).join(', ')||'None'}.</p>
                <p>Other indexes with configured-root overlap: {(policyImpact.other_covering_index_ids as string[]).join(', ')||'None returned'}. {String(policyImpact.overlap_basis)}</p>
                <p>These observations do not certify effective eligibility in another index. The change applies only to the reviewed index. Counts remain unknown when native review has not scanned retained evidence.</p>
                <p>When present, imported analysis and the embedding cache are retired for the entire index. Imported analysis is not automatically regenerated. The native outcome lists the components actually retired.</p>
            </article>}
            {catalogReviewPreview(view.review).authority_fence_required===true&&<p>Native access changes require a complete delivery-fence receipt. Incomplete preparation remains an unconfirmed outcome; metadata alone does not establish completion.</p>}
            <button disabled={disabled} onClick={()=>void run(()=>request('indexing.apply',{generation:view.generation}))}>{view.review.request.operation.operation==='create_grant'?'Persist reviewed grant':'Apply reviewed changes'}</button></article>}
        {view.submittedReview&&<details class="card"><summary>Last submitted native review</summary><pre>{JSON.stringify(view.submittedReview,null,2)}</pre></details>}
        {view.setupReview&&<article class="card"><h2>Review native index setup</h2><p>Review the exact native locations, source aliases, collection and storage action below. Applying can create or claim index storage. Source content remains read-only. Setup does not choose an evidence grant or session.</p><pre>{JSON.stringify(view.setupReview.response,null,2)}</pre>
            <button disabled={disabled} onClick={()=>void run(()=>request('indexing.setup.apply',{generation:view.generation}))}>Apply reviewed index setup</button></article>}
        {view.submittedSetupReview&&<details class="card"><summary>Last submitted native setup review</summary><pre>{JSON.stringify(view.submittedSetupReview,null,2)}</pre></details>}
        {view.outcome&&<article class="card"><h2>Native outcome</h2>{view.outcome.status==='source_change_busy'&&<p>{String(catalogObject(view.outcome.busy).pending_jobs)} queued and {String(catalogObject(view.outcome.busy).running_jobs)} running accepted jobs. Finish or explicitly cancel them in Management before preparing another source change. No jobs were canceled or reordered by this request.</p>}
            {Array.isArray(view.outcome.receipts)&&view.outcome.receipts.some(value=>catalogObject(value).status==='catalog_receipt_unprepared')&&<p>Native reconciliation proved that the exact catalog request was not prepared. Its local uncertain intent is retired; this is not a completed mutation receipt. Prepare and review any desired change explicitly. No Apply or new key was submitted automatically.</p>}
            {view.submittedReview?.request.operation.operation==='begin_index_source_change'&&view.outcome.status==='catalog_applied'&&<p>The native source-change begin was accepted. Read its source-change observation for the recorded configuration job and actual metadata completion.</p>}
            {view.submittedReview?.request.operation.operation==='change_index_policy'&&view.outcome.status==='catalog_applied'&&<p>The reviewed policy change was accepted. Read permanent exclusions and cleanup status for the recorded removal publication and physical cleanup outcome.</p>}
            <pre>{JSON.stringify(view.outcome,null,2)}</pre></article>}
        <article class="card"><h2>Complete catalog details</h2><p>Includes the complete retained native page, detail, window association, Sources, memberships, preview and outcome. Each page replaces the previously displayed page; exports describe only retained pages.</p>
            <button disabled={disabled} onClick={()=>void run(()=>request('indexing.export',{format:'copy'}))}>Copy complete JSON</button><button disabled={disabled} onClick={()=>void run(()=>request('indexing.export',{format:'json'}))}>Export JSON…</button><button disabled={disabled} onClick={()=>void run(()=>request('indexing.export',{format:'copy_markdown'}))}>Copy complete Markdown</button><button disabled={disabled} onClick={()=>void run(()=>request('indexing.export',{format:'markdown'}))}>Export Markdown…</button>
            <details><summary>Complete response</summary><pre>{catalogExport(view as CatalogView)}</pre></details>
        </article>
    </section>;
}
