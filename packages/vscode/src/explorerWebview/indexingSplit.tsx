// SPDX-License-Identifier: Apache-2.0
import { useEffect, useState } from 'preact/hooks';
import { CatalogObject, CatalogScopeAtom, CatalogView, catalogItems, catalogObject } from '../indexingCatalog';
import { splitTerminal, validateSplitSelection } from '../indexingSplit';
import { request } from './api';

interface ProjectChoice { project_id:string; retain_original:boolean; destination:'omit'|'preserve_reviewed_scope'|'selected'; scope:string }
/** Review and explicit native management only; opening this form starts no job. */
export function IndexSplitPanel({view,index,disabled,run}:{view:CatalogView;index?:CatalogObject;disabled:boolean;run:(work:()=>Promise<unknown>)=>Promise<void>}) {
    const [name,setName]=useState(''),[dataRoot,setDataRoot]=useState(''),[collection,setCollection]=useState('');
    const [atoms,setAtoms]=useState<CatalogScopeAtom[]>([{source_root_id:'',kind:'subtree',path:''}]);
    const [projects,setProjects]=useState<ProjectChoice[]>([]),[growth,setGrowth]=useState(''),[temporary,setTemporary]=useState('0'),[rewrite,setRewrite]=useState('0'),[cleanup,setCleanup]=useState('');
    const [semantic,setSemantic]=useState(false),[watch,setWatch]=useState(false),[operation,setOperation]=useState('');
    useEffect(()=>{setAtoms([{source_root_id:'',kind:'subtree',path:''}]);},[index?.id]);
    const outcome=view.outcome?.snapshot?catalogObject(view.outcome.snapshot):undefined;
    useEffect(()=>{const receipt=outcome?.receipt?catalogObject(outcome.receipt):undefined;if(!operation&&typeof receipt?.operation_id==='string')setOperation(receipt.operation_id);},[outcome?.receipt,operation]);
    const status=view.splitStatus,snapshot=status?.snapshot?catalogObject(status.snapshot):undefined,receipt=snapshot?catalogObject(snapshot.receipt):undefined,state=receipt?catalogObject(receipt.state):undefined;
    const finalization=snapshot?.finalization?catalogObject(snapshot.finalization):undefined;
    const terminal=snapshot?splitTerminal(snapshot):false,postFence=receipt&&['original_fenced','remainder_published','membership_applied','cleanup_pending','complete'].includes(String(receipt.stage));
    const baseline=view.splitSourceState?.state?catalogObject(view.splitSourceState.state):undefined;
    const sourceState=baseline&&catalogObject(baseline.index).id===index?.id&&baseline.catalog_revision===view.revision?baseline:undefined;
    const sourceRoots=sourceState?catalogObject(sourceState.policy).roots:undefined;
    const sources=Array.isArray(sourceRoots)?sourceRoots.map(catalogObject):[];
    useEffect(()=>{
        if(!sourceState)return;
        setProjects((sourceState.affected_projects as unknown[]).map(catalogObject).map(effect=>({project_id:String(effect.project_id),retain_original:true,destination:'omit' as const,scope:'[]'})));
    },[view.splitSourceState]);
    const update=(at:number,change:Partial<CatalogScopeAtom>)=>setAtoms(prior=>prior.map((value,i)=>i===at?{...value,...change}:value));
    const updateProject=(at:number,change:Partial<ProjectChoice>)=>setProjects(prior=>prior.map((value,i)=>i===at?{...value,...change}:value));
    return <article class="card" id="split-review"><h2>Split an index</h2>
        <p>Build one independent destination and leave the original as the remainder. Exact files stay exact; subtrees include future descendants. Existing permanent exclusions remain authoritative in both. Preparing a review reads native metadata only.</p>
        {!view.canReviewSplit?<p>Split is unavailable until this connection explicitly reports native policy contract 2. Method names and version labels do not establish capability.</p>:index&&index.tombstone===false?<form onSubmit={event=>{event.preventDefault();void run(async()=>{
            const selection=validateSplitSelection({selectors:atoms,destination_name:name,destination_data_root:dataRoot,destination_collection:collection,
                projects:projects.map(project=>({project_id:project.project_id,retain_original:project.retain_original,membership:project.destination==='selected'?{destination:'selected',scope:JSON.parse(project.scope)}:{destination:project.destination}})),
                capacity:{destination_growth:Number(growth),destination_temporary:Number(temporary),original_rewrite:Number(rewrite),cleanup_allowance:Number(cleanup)},require_semantic_ready:semantic,destination_watch_enabled:watch});
            await request('indexing.split.draft',{selection,generation:view.generation});
        });}}>
            <p>Original index: <code>{String(index.id)}</code>. The reviewed native response includes every affected Project, exact source mappings, inherited denies, storage action and capacity requirements. No membership choice issues an evidence grant or changes saved defaults.</p>
            <button type="button" disabled={disabled} onClick={()=>void run(()=>request('indexing.split.sourceRead',{generation:view.generation}))}>Read complete native split baseline</button>
            <p>The canonical native baseline supplies all roots, policy and affected Projects. Paged library rows do not establish a complete census. Refresh it after a catalog revision changes.</p>
            {sourceState&&<details><summary>Complete native source baseline and capabilities</summary><pre>{JSON.stringify(view.splitSourceState,null,2)}</pre></details>}
            <datalist id="split-source-options">{sources.map(row=><option key={String(row.id)} value={String(row.id)}>{String(row.canonical_path)}</option>)}</datalist>
            {atoms.map((atom,at)=><fieldset key={at} disabled={disabled}><legend>Transferred source scope {at+1}</legend>
                <label>Source ID<input required list="split-source-options" value={atom.source_root_id} onInput={event=>update(at,{source_root_id:event.currentTarget.value})}/></label>
                <label>Canonical relative path<input value={atom.path} onInput={event=>update(at,{path:event.currentTarget.value})}/></label>
                <label>Selection<select value={atom.kind} onChange={event=>update(at,{kind:event.currentTarget.value==='exact'?'exact':'subtree'})}><option value="subtree">Subtree</option><option value="exact">Exact file</option></select></label>
                <button type="button" onClick={()=>setAtoms(prior=>prior.filter((_,i)=>i!==at))}>Remove scope</button>
            </fieldset>)}
            <button type="button" disabled={disabled||atoms.length>=128} onClick={()=>setAtoms(prior=>[...prior,{source_root_id:'',kind:'subtree',path:''}])}>Add transferred scope</button>
            <label>Destination name<input required value={name} onInput={event=>setName(event.currentTarget.value)}/></label>
            <label>Destination data root (absolute path)<input required value={dataRoot} onInput={event=>setDataRoot(event.currentTarget.value)}/></label>
            <label>Destination collection<input required value={collection} onInput={event=>setCollection(event.currentTarget.value)}/></label>
            <h3>Reviewed Project effects</h3><p>Native review supplies the complete affected-Project census. Unchosen Projects keep only the original remainder. Add deliberate destination memberships here; preserve reviewed scope or provide explicit scopes using original source IDs.</p>
            {projects.map((project,at)=><fieldset key={at} disabled={disabled}><legend>Project choice {at+1}</legend>
                <label>Project ID<input required value={project.project_id} onInput={event=>updateProject(at,{project_id:event.currentTarget.value})}/></label>
                <label><input type="checkbox" checked={project.retain_original} onChange={event=>updateProject(at,{retain_original:event.currentTarget.checked})}/>Retain original remainder membership</label>
                <label>Destination membership<select value={project.destination} onChange={event=>updateProject(at,{destination:event.currentTarget.value as ProjectChoice['destination']})}><option value="omit">Omit destination</option><option value="preserve_reviewed_scope">Preserve reviewed scope</option><option value="selected">Explicit source scopes</option></select></label>
                {project.destination==='selected'&&<label>Exact source scopes (JSON array)<textarea required value={project.scope} onInput={event=>updateProject(at,{scope:event.currentTarget.value})}/></label>}
                <button type="button" onClick={()=>setProjects(prior=>prior.filter((_,i)=>i!==at))}>Remove Project choice</button>
            </fieldset>)}
            <button type="button" disabled={disabled||projects.length>=32} onClick={()=>setProjects(prior=>[...prior,{project_id:'',retain_original:true,destination:'omit',scope:'[]'}])}>Add explicit Project choice</button>
            <h3>Peak growth ceilings</h3><p>Enter byte ceilings explicitly. These are admission limits, not measured file sizes or already reserved space. Native review binds both quota policies; admission reserves destination, temporary, original rewrite and cleanup capacity before build.</p>
            <label>Destination growth bytes<input required type="number" min="1" step="1" value={growth} onInput={event=>setGrowth(event.currentTarget.value)}/></label>
            <label>Destination temporary bytes<input required type="number" min="0" step="1" value={temporary} onInput={event=>setTemporary(event.currentTarget.value)}/></label>
            <label>Original rewrite bytes<input required type="number" min="0" step="1" value={rewrite} onInput={event=>setRewrite(event.currentTarget.value)}/></label>
            <label>Cleanup allowance bytes<input required type="number" min="1" step="1" value={cleanup} onInput={event=>setCleanup(event.currentTarget.value)}/></label>
            <label><input type="checkbox" checked={semantic} onChange={event=>setSemantic(event.currentTarget.checked)}/>Require semantic readiness before transfer</label>
            <label><input type="checkbox" checked={watch} onChange={event=>setWatch(event.currentTarget.checked)}/>Enable destination watching after reviewed activation</label>
            <button disabled={disabled||!sourceState}>Prepare complete native split review</button>
        </form>:<p>Inspect a live index in the native library before choosing split inputs.</p>}
        {view.splitReview&&<section><h3>Review the complete native split</h3><p>Read the selected and remainder policies, inherited exclusions, complete Project changes, capacity ceilings and control dependencies. Ancestor/global ignore controls and declared configurations may be read outside positive document selection; they do not become indexed siblings.</p><pre>{JSON.stringify(view.splitReview.response,null,2)}</pre><button disabled={disabled} onClick={()=>void run(()=>request('indexing.split.apply',{generation:view.generation}))}>Apply reviewed split</button></section>}
        {view.submittedSplitReview&&<details><summary>Exact submitted native split review</summary><pre>{JSON.stringify(view.submittedSplitReview,null,2)}</pre></details>}
        <h3>Read current split state</h3><p>Receipt lookup and status reads never repeat a mutation. A missing or expired receipt leaves its outcome unknown. Current state may be newer than an accepted historical recovery acknowledgment.</p>
        <form onSubmit={event=>{event.preventDefault();void run(()=>request('indexing.split.read',{operation,generation:view.generation}));}}><label>Native operation ID<input required pattern="[0-9a-f]{32}" value={operation} onInput={event=>setOperation(event.currentTarget.value)}/></label><button disabled={disabled||!view.canReviewSplit}>Read native split status</button></form>
        {snapshot&&<p>Destination retained evidence: {catalogObject(catalogObject(snapshot).request).extraction_policy==null?'legacy Agent Lean default':`${String(catalogObject(catalogObject(catalogObject(snapshot).request).extraction_policy).retained_profile)} with unchanged legacy fallback`}. Original retention is preserved. OCR remains an explicit operation.</p>}
        {status&&(snapshot?<><p>Current stage: <strong>{String(receipt?.stage)}</strong>. State: {String(state?.state)}. Receipt revision: {String(receipt?.revision)}.</p><p>{postFence?'The original transfer fence is active. Cancellation cannot restore transferred evidence; resume the recorded restrictive completion.':'The original remains admitted under its existing policy until the exact transfer fence. Before that fence, cancellation cleans only the owned destination.'}</p><p>{receipt?.physical_cleanup_authorization?'Physical cleanup authorization recorded.':'Physical cleanup not yet authorized.'} Funding releases: {Object.keys(catalogObject(receipt?.funding_release_receipts)).length}; completion requires all owned funding and cleanup to settle.</p><pre>{JSON.stringify(status,null,2)}</pre>
            {receipt?.cleanup_attention&&<p>Destination cleanup requires explicit recovery: <code>{String(receipt.cleanup_attention)}</code>.</p>}{finalization&&<p>Current terminal publication: {String(finalization.progress)}; envelope revision {String(finalization.revision)}. {finalization.attention?`Explicit recovery required: ${String(finalization.attention)}.`:'No current finalization attention latch.'} Funding acknowledgments: {Array.isArray(finalization.funding_acknowledged)?finalization.funding_acknowledged.length:0}. Historical recovery acknowledgment does not clear a later attention latch.</p>}
            <button disabled={disabled||terminal} onClick={()=>void run(()=>request('indexing.split.recover',{action:'resume',generation:view.generation}))}>{finalization?'Resume terminal publication and funding acknowledgments':'Resume recorded native phase'}</button>
            <button disabled={disabled||terminal||!!finalization} onClick={()=>void run(()=>request('indexing.split.recover',{action:'cancel',generation:view.generation}))}>{postFence?'Request native cancellation determination':'Cancel before original fence'}</button>
        </>:<p>No current native snapshot was returned. This does not prove completion, cancellation or no effects.</p>)}
        {view.reviewedSplitStatus&&<details><summary>Historical status used for the exact recovery request</summary><pre>{JSON.stringify(view.reviewedSplitStatus,null,2)}</pre></details>}
        {!!view.pendingDetails?.some(row=>{const intent=catalogObject(row.intent);return 'catalog_split_apply'in intent||'catalog_split_recover'in intent;})&&<details><summary>Tracked exact split requests, immutable recovery acknowledgments and later observations</summary><pre>{JSON.stringify(view.pendingDetails.filter(row=>{const intent=catalogObject(row.intent);return 'catalog_split_apply'in intent||'catalog_split_recover'in intent;}),null,2)}</pre><button disabled={disabled} onClick={()=>void run(()=>request('indexing.reconcile',{}))}>Read exact native receipts</button></details>}
    </article>;
}
