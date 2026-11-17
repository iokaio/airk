# SPDX-License-Identifier: Apache-2.0
from pathlib import Path

p=Path('packages/vscode/src/explorerWebview/indexing.tsx')
s=Path('artifacts/recovered-indexing-pre-edit-latest.tsx').read_text(encoding='utf-8')
s=s.replace('CatalogScopeAtom, ','').replace('catalogMembershipDraft, ','').replace('catalogWindowAssociation, ','').replace(', validateCatalogScope','')
a=s.index("    const [index,setIndex]");b=s.index("    const [setupName",a)
s=s[:a]+"    const [index,setIndex]=useState('');\n    const [setIndexes,setSetIndexes]=useState<string[]>([]);\n    const [scopeIndexes,setScopeIndexes]=useState<string[]>([]),[scopeProjects,setScopeProjects]=useState<string[]>([]);\n"+s[b:]
s=s.replace("    const [useIndex,setUseIndex]=useState(''),[useProjects,setUseProjects]=useState('');\n",'')
s=s.replace("    const [adviceProjects,setAdviceProjects]=useState(''),[adviceIndexes,setAdviceIndexes]=useState(''),[situation,setSituation]=useState('keep');","    const [situation,setSituation]=useState('keep');")
a=s.index('    const savedContext=');b=s.index('    const run=',a)
s=s[:a]+'''    const scope=view.searchScope;
    const scopeSelection=JSON.stringify([scope?.selectedIndexIds,scope?.selectedProjectIds]);
    useEffect(()=>{setScopeIndexes(scope?.selectedIndexIds??[]);setScopeProjects(scope?.selectedProjectIds??[]);},[scopeSelection]);
    const toggle=(items:string[],id:string,checked:boolean)=>checked?[...items.filter(item=>item!==id),id]:items.filter(item=>item!==id);
'''+s[b:]
s=s.replace("    const grantRow=catalogItems(view.grantDetail,'detail')[0],selectedGrant=grantRow?catalogObject(grantRow.value.grant):undefined;", "    const setSignature=JSON.stringify([view.setEditor?.id,view.setEditor?.name,view.setEditor?.indexIds]);\n    useEffect(()=>{if(view.setEditor?.id===project?.id){setName(view.setEditor?.name??'');setSetIndexes(view.setEditor?.indexIds??[]);}},[setSignature,project?.id]);")
s=s.replace("            if(!view.context&&!view.hostContext&&(view.canReadWindowContext||view.windowDescriptor)){automaticRead.current=readOwner;void run(()=>request('indexing.context.read',{}));}","            if(!scope){automaticRead.current=readOwner;void run(()=>request('indexing.scope.read',{}));}")
s=s.replace("            if(!pageLoaded&&!detail)void page(expectedPage);","            if(tab==='projects'&&!scope)void run(async()=>{await request('indexing.scope.read',{});if(!pageLoaded&&!detail)await request('indexing.page',{kind:'projects'});});\n            else if(!pageLoaded&&!detail)void page(expectedPage);")
s=s.replace('view.canReadWindowContext,view.windowDescriptor,view.context,view.hostContext','scope')
a=s.index('    const editAtom=');b=s.index('    return <section',a)
s=s[:a]+"    useEffect(()=>{if(project)void run(()=>request('indexing.set.read',{id:String(project.id)}));},[project?.id]);\n"+s[b:]
s=s.replace('Choose which Projects and indexes this window uses. Manage access and saved preferences here.','Choose the indexes and index sets to search in this VS Code window.').replace('Group existing indexes into Projects and choose their default search scope.','Save reusable sets of indexes without copying index data.').replace('Manage indexes on this machine. Open an index to change its sources, split it, or review which Projects use it.','Create and manage local indexes. Change sources, split an index, or delete an index you no longer need.')
a=s.index("        {props.route.tab==='window'?<>");b=s.index('        <details class="card" id="recommendations">',a)
s=s[:a]+'''        {props.route.tab==='window'?<><article class="card"><h2>Search scope</h2>
            <p>Check the indexes and index sets you want to search, then apply your selection. Shared indexes are searched once.</p>
            <button disabled={disabled} onClick={()=>void run(()=>request('indexing.scope.read',{}))}>Refresh choices</button>
            {!scope&&!view.busy&&<p>Search choices have not loaded. Use Refresh choices to retry.</p>}
            {scope&&<form onSubmit={event=>{event.preventDefault();void run(()=>request('indexing.scope.apply',{generation:view.generation,selection:{index_ids:scopeIndexes,project_ids:scopeProjects}}));}}>
                <fieldset disabled={disabled} class="indexing-scope-list"><legend>Indexes</legend>
                    {scope.indexes.filter(item=>item.tombstone!==true).map(item=><label key={String(item.id)}><input type="checkbox" checked={scopeIndexes.includes(String(item.id))} onChange={event=>setScopeIndexes(items=>toggle(items,String(item.id),event.currentTarget.checked))}/><span><strong>{String(item.name??item.id)}</strong>{typeof catalogObject(item.backing??{}).root==='string'&&<small>{String(catalogObject(item.backing).root)}</small>}</span></label>)}
                    {!scope.indexes.some(item=>item.tombstone!==true)&&<p>No indexes on this page. Create one in Index library.</p>}
                    {scope.indexContinuation&&<button type="button" disabled={disabled} onClick={()=>void run(()=>request('indexing.scope.read',{kind:'indexes',next:true}))}>More indexes</button>}
                </fieldset>
                <fieldset disabled={disabled} class="indexing-scope-list"><legend>Index sets</legend>
                    {scope.projects.map(item=><label key={String(item.id)}><input type="checkbox" checked={scopeProjects.includes(String(item.id))} onChange={event=>setScopeProjects(items=>toggle(items,String(item.id),event.currentTarget.checked))}/>{String(item.name??item.id)}</label>)}
                    {!scope.projects.length&&<p>No index sets on this page. Create one in Index sets.</p>}
                    {scope.projectContinuation&&<button type="button" disabled={disabled} onClick={()=>void run(()=>request('indexing.scope.read',{kind:'projects',next:true}))}>More index sets</button>}
                </fieldset>
                <p>{scopeIndexes.length} indexes and {scopeProjects.length} index sets selected. Uncheck everything to stop searching indexes in this window.</p>
                <p role="status">{scope.active?'Selection active in this window.':'Apply your selection to use it in this window.'}</p>
                <button disabled={disabled}>Apply selection</button>
                <p class="muted">Existing manual coding-agent configurations keep their own selection.</p>
            </form>}
        </article>
'''+s[b:]
s=s.replace("{situation,...(adviceProjects.trim()||adviceIndexes.trim()?{selection:{project_ids:adviceProjects.split(',').map(v=>v.trim()).filter(Boolean),index_ids:adviceIndexes.split(',').map(v=>v.trim()).filter(Boolean)}}:{})}",'{situation}')
s=s.replace('<label>Query Project IDs (comma separated)<input value={adviceProjects} onInput={event=>setAdviceProjects(event.currentTarget.value)} /></label>','').replace('<label>Query index IDs (comma separated)<input value={adviceIndexes} onInput={event=>setAdviceIndexes(event.currentTarget.value)} /></label>','')
s=s.replace('Leave both blank to inspect the active connection. Otherwise provide both explicit subsets. Selecting all or widening is a deliberate choice within the active grant.','Recommendations use the current search selection.')
# These substitutions target labels/prose only, never identifier substrings.
import re
s=re.sub(r'\bProjects\b','Index sets',s);s=re.sub(r'\bProject\b','Index set',s)
s=s.replace('A library entry or Index set membership does not grant source access.','')
s=s.replace('Each native page contains up to 16 entries within 48 KiB. The filter applies only to this displayed page.','Showing one page. Use Next page to see more. The filter applies only to this displayed page.')
s=s.replace("visibleRows.filter(row=>JSON.stringify(row.value)","visibleRows.filter(row=>row.kind!=='project'||!scope?.managedProjectIds?.includes(String(row.value.id))).filter(row=>JSON.stringify(row.value)")
s=s.replace('<p><code>{String(row.value.id)}</code></p>','<details><summary>Identity</summary><code>{String(row.value.id)}</code></details>')
s=s.replace("Open Index set","Open index set")
s=s.replace('>Split index</button>}</div>', '''>Split index</button>}{row.kind==='index'&&row.value.tombstone===false&&<button class="danger" disabled={disabled||!view.canReviewSource} onClick={()=>void run(async()=>{const current=await request<CatalogView>('indexing.detail',{kind:'index',id:String(row.value.id)});await request('indexing.source.draft',{generation:current.generation,change:{operation:'retire_index'}});})}>Delete index</button>}</div>''',1)
a=s.index("            {props.route.tab==='projects'&&<details>");b=s.index("            {props.route.tab==='library'",a)
s=s[:a]+'''            {props.route.tab==='projects'&&<details><summary>Create an index set</summary><form onSubmit={event=>{event.preventDefault();void run(()=>request('indexing.set.save',{selection:{name,index_ids:setIndexes},generation:view.generation}));}}>
                <label>Index set name<input required maxLength={256} value={name} onInput={event=>setName(event.currentTarget.value)} /></label>
                <fieldset disabled={disabled} class="indexing-scope-list"><legend>Include indexes</legend>{scope?.indexes.filter(item=>item.tombstone!==true).map(item=><label key={String(item.id)}><input type="checkbox" checked={setIndexes.includes(String(item.id))} onChange={event=>setSetIndexes(items=>toggle(items,String(item.id),event.currentTarget.checked))}/>{String(item.name??item.id)}</label>)}</fieldset>
                {scope?.indexContinuation&&<button type="button" disabled={disabled} onClick={()=>void run(()=>request('indexing.scope.read',{kind:'indexes',next:true}))}>More indexes</button>}
                <button disabled={disabled||!view.revision||!scope}>Create index set</button>
            </form></details>}
'''+s[b:]
s=s.replace('Prepare native setup review','Review new index').replace('Preparing a review creates no index and grants no access.','Use a new index storage folder and collection name, including when replacing a deleted index. Source folders may overlap another index.')
s=s.replace('Index storage location<input','New index storage folder<input').replace('<label>Collection<input','<label>New collection name<input')
s=s.replace('<p><code>{String(detail.value.id)}</code></p>','<details><summary>Identity</summary><code>{String(detail.value.id)}</code></details>')
s=s.replace('            {selectedIndex&&<nav class="actions indexing-tasks"', '''            {selectedIndex&&<div class="indexing-delete"><p>Deleting an index stops maintenance and removes its searchable data. Original files stay in place. You can create a different index over the same folders.</p><button class="danger" disabled={disabled||selectedIndex.tombstone===true||!view.canReviewSource} onClick={()=>void run(()=>request('indexing.source.draft',{generation:view.generation,change:{operation:'retire_index'}}))}>Delete index</button></div>}
            {selectedIndex&&<nav class="actions indexing-tasks"''')
s=s.replace("{(!selectedIndex||indexTask==='settings')&&<details>","{selectedIndex&&indexTask==='settings'&&<details>",1)
s=s.replace('<p>Retirement waits for native evidence and background work to stop, including embedding providers. Review the complete native receipt; source files are not deleted.</p>\n                <button disabled={disabled||selectedIndex.tombstone===true} onClick={()=>void run(()=>request(\'indexing.source.draft\',{generation:view.generation,change:{operation:\'retire_index\'}}))}>Prepare native index-retirement review</button>','')
a=s.index('            {project&&<>');b=s.index('\n        </article>}',a)
s=s[:a]+'''            {project&&<form onSubmit={event=>{event.preventDefault();void run(()=>request('indexing.set.save',{selection:{id:String(project.id),name,index_ids:setIndexes},generation:view.generation}));}}>
                <label>Index set name<input required maxLength={256} value={name} onInput={event=>setName(event.currentTarget.value)} /></label>
                <p>Newly added indexes include all their sources. Existing folder selections stay unchanged.</p>
                {view.setEditor?.id!==project.id||!view.setEditor.complete?<p>Loading complete index set membership before editing…</p>:<fieldset disabled={disabled} class="indexing-scope-list"><legend>Include indexes</legend>{scope?.indexes.filter(item=>item.tombstone!==true).map(item=><label key={String(item.id)}><input type="checkbox" checked={setIndexes.includes(String(item.id))} onChange={event=>setSetIndexes(items=>toggle(items,String(item.id),event.currentTarget.checked))}/>{String(item.name??item.id)}</label>)}</fieldset>}
                {scope?.indexContinuation&&<button type="button" disabled={disabled} onClick={()=>void run(()=>request('indexing.scope.read',{kind:'indexes',next:true}))}>More indexes</button>}
                <button disabled={disabled||!scope||view.setEditor?.id!==project.id||!view.setEditor.complete}>Save index set</button>
                <button type="button" class="danger" disabled={disabled} onClick={()=>void preview({operation:'delete_project',id:project.id,expected_definition_revision:project.definition_revision,expected_scope_revision:project.scope_revision,expected_defaults_revision:project.defaults_revision})}>Delete index set</button>
                <p class="muted">Deleting a set keeps its indexes and original files.</p>
            </form>}
'''+s[b:]
a=s.index('        {view.memberships&&detail');b=s.index('\n        </>}',a)
s=s[:a]+'''        {view.memberships&&detail&&(!selectedIndex||indexTask==='memberships')&&<details class="card"><summary>Complete index set membership details</summary><pre>{JSON.stringify(view.memberships,null,2)}</pre>{next(view.memberships)&&<button disabled={disabled} onClick={()=>void page('memberships',true,selectedIndex?String(selectedIndex.id):undefined,project?String(project.id):undefined)}>More memberships</button>}</details>}
'''+s[b:]
s=s.replace('{view.review&&<article class="card"><h2>Review native changes</h2><p>Review the complete native preview, including before/after state, file-data effects and authority fences.</p><pre>{JSON.stringify(view.review.response,null,2)}</pre>', '''{view.review&&<article class="card"><h2>{view.review.request.operation.operation==='retire_index'?'Delete this index?':'Review changes'}</h2>{view.review.request.operation.operation==='retire_index'?<p><strong>{String(selectedIndex?.name??visibleRows.find(row=>row.value.id===view.review?.request.operation.id)?.value.name??view.review.request.operation.id)}</strong>: This stops maintenance, removes the index from search and index sets, and schedules cleanup of its owned derived data. Original source files are preserved. You can create a new index over the same files without splitting; use a new index storage folder and collection name.</p>:<p>Review the changes before applying.</p>}<details><summary>Complete native review</summary><pre>{JSON.stringify(view.review.response,null,2)}</pre></details>''')
s=s.replace("view.review.request.operation.operation==='create_grant'?'Persist reviewed grant':'Apply reviewed changes'","view.review.request.operation.operation==='retire_index'?'Delete index and stop maintenance':view.review.request.operation.operation==='create_grant'?'Persist reviewed grant':'Apply reviewed changes'")
s=s.replace('Setup does not choose an evidence grant or session.','Choose the new index in Search scope when you want to search it.').replace('<h2>Native outcome</h2>','<h2>Operation result</h2>')
p.write_text(s,encoding='utf-8',newline='\n')
