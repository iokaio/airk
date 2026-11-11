// SPDX-License-Identifier: Apache-2.0
import { useEffect, useState } from 'preact/hooks';
import { CatalogObject, CatalogScopeAtom, CatalogView, catalogExport, catalogItems, catalogMembershipDraft, catalogObject, validateCatalogScope } from '../indexingCatalog';
import type { ScreenProps } from './workspaces';
import { request } from './api';

/** Native metadata is rendered as escaped Preact text; complete replies stay available in details/export. */
export function IndexingScreen(props: ScreenProps) {
    const view=props.state.indexing??{generation:0,busy:false};
    const [error,setError]=useState(''),[name,setName]=useState(''),[filter,setFilter]=useState('');
    const [index,setIndex]=useState(''),[memberRevision,setMemberRevision]=useState('');
    const [scopeAtoms,setScopeAtoms]=useState<CatalogScopeAtom[]>([{source_root_id:'',kind:'subtree',path:''}]);
    const [defaults,setDefaults]=useState('');
    const [windowProjects,setWindowProjects]=useState(''),[windowIndexes,setWindowIndexes]=useState('');
    const savedContext=catalogItems(view.context,'detail')[0]?.value;
    const contextToken=view.context?JSON.stringify([catalogObject(view.context.detail).namespace,catalogObject(view.context.detail).revision,savedContext?.id,savedContext?.revision]):'';
    useEffect(()=>{setWindowProjects(Array.isArray(savedContext?.project_ids)?savedContext.project_ids.join(', '):'');setWindowIndexes(Array.isArray(savedContext?.index_ids)?savedContext.index_ids.join(', '):'');},[contextToken]);
    const run=async(work:()=>Promise<unknown>)=>{setError('');try{await work();}catch(failure){setError(failure instanceof Error?failure.message:String(failure));}};
    const preview=(operation:CatalogObject)=>run(()=>request('indexing.preview',{operation,generation:view.generation}));
    const disabled=view.busy||!props.state.trusted;
    const rows=catalogItems(view.page), detail=catalogItems(view.detail,'detail')[0];
    const project=detail?.kind==='project'?detail.value:undefined, selectedIndex=detail?.kind==='index'?detail.value:undefined;
    const pageKind=rows[0]?.kind;
    const expectedKind=props.route.tab==='projects'?'project':'index';
    const visibleRows=pageKind===expectedKind||rows.length===0?rows:[];
    const next=(response?:CatalogObject)=>!!response&&!!catalogObject(response.page).continuation;
    const page=(kind:'projects'|'indexes'|'sources'|'memberships',more=false,idx?:string,prj?:string)=>run(()=>request('indexing.page',{kind,next:more,...(idx?{index:idx}:{}),...(prj?{project:prj}:{})}));
    const editAtom=(at:number,change:Partial<CatalogScopeAtom>)=>setScopeAtoms(rows=>rows.map((row,i)=>i===at?{...row,...change}:row));
    const editMembership=(member:CatalogObject)=>{const draft=catalogMembershipDraft(member);setIndex(draft.index);setMemberRevision(draft.revision);setScopeAtoms(draft.scope);};
    return <section>
        {props.route.tab==='window'?<article class="card"><h2>This window</h2>
            <p>Saved window associations are preferences. Saving or deleting an association does not grant access, switch an evidence session or change a coding agent's configured scope.</p>
            {view.windowDescriptor?<><p>{view.windowDescriptor.name} · {view.windowDescriptor.kind}</p>
                <button disabled={disabled} onClick={()=>void run(()=>request('indexing.context.read',{}))}>Read saved association</button>
                {view.context&&<><p>{savedContext?`Saved association ${String(savedContext.id)}, revision ${String(savedContext.revision)}`:'No saved association for this window.'}</p>
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
            <p>Access grants and new-index setup remain unavailable in this screen. Existing single-index connections continue to use their configured source scope.</p>
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
        </article>
        {detail&&<article class="card"><h2>{detail.kind==='project'?'Project':'Index'}: {String(detail.value.name)}</h2><p><code>{String(detail.value.id)}</code></p>
            <form onSubmit={event=>{event.preventDefault();void preview({operation:detail.kind==='project'?'rename_project':'rename_index',id:detail.value.id,expected_definition_revision:detail.value.definition_revision,name});}}><label>New name<input required maxLength={256} value={name} onInput={event=>setName(event.currentTarget.value)} /></label><button disabled={disabled}>Preview rename</button></form>
            {selectedIndex&&<button disabled={disabled} onClick={()=>void page('sources',false,String(selectedIndex.id))}>Read Sources</button>}
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
        {!!view.pending?.length&&<article class="card"><h2>Unconfirmed requests</h2><p>{view.pending.length} saved request(s). Their outcomes are unconfirmed. Receipt lookup never repeats the apply; a missing receipt keeps the request unresolved.</p><button disabled={disabled} onClick={()=>void run(()=>request('indexing.reconcile',{}))}>Check native receipts</button></article>}
        {view.review&&<article class="card"><h2>Review native changes</h2><p>Review the complete native preview, including before/after state, file-data effects and authority fences.</p><pre>{JSON.stringify(view.review.response,null,2)}</pre>
            {catalogObject(view.review.response.preview).authority_fence_required===true&&<p>Native access changes require an authority fence. This build currently refuses them with <code>catalog_authority_fence_unavailable</code>.</p>}
            <button disabled={disabled} onClick={()=>void run(()=>request('indexing.apply',{generation:view.generation}))}>Apply reviewed changes</button></article>}
        {view.outcome&&<article class="card"><h2>Native outcome</h2><pre>{JSON.stringify(view.outcome,null,2)}</pre></article>}
        <article class="card"><h2>Complete catalog details</h2><p>Includes the complete retained native page, detail, window association, Sources, memberships, preview and outcome. Each page replaces the previously displayed page; exports describe only retained pages.</p>
            <button disabled={disabled} onClick={()=>void run(()=>request('indexing.export',{format:'copy'}))}>Copy complete JSON</button><button disabled={disabled} onClick={()=>void run(()=>request('indexing.export',{format:'json'}))}>Export JSON…</button>
            <details><summary>Complete response</summary><pre>{catalogExport(view as CatalogView)}</pre></details>
        </article>
    </section>;
}
